//! YAML frontmatter parsing for markdown documents.
//!
//! Frontmatter is the YAML block delimited by `---` markers at the very top of the file.
//! [`parse`] returns the parsed frontmatter value and the body slice; `Ok(None)` if no
//! frontmatter is present; `Err` if the frontmatter is present but malformed YAML.
//!
//! Edge cases the contract covers (asserted by Red Gate tests below):
//! - File with no frontmatter at all → `Ok(None)`
//! - Empty frontmatter (`---\n---\n`) → returns the empty mapping plus the body
//! - Trailing newline immediately after closing `---` → stripped from body's leading position
//! - Malformed YAML inside the markers → `Err`
//! - File where closing `---` is missing → `Ok(None)` (no frontmatter detected)
//! - Dashes inside body without leading newline → not matched as closing
//!
//! Implemented; tests below assert the contracts.

use crate::Error;
use serde_yaml_ng::Value;

/// Parse YAML frontmatter from markdown content.
///
/// Returns:
/// - `Ok(Some((frontmatter, body)))` when frontmatter is present and parses cleanly
/// - `Ok(None)` when no frontmatter is present (or no closing marker found)
/// - `Err` when the frontmatter is present but malformed YAML
pub fn parse(content: &str) -> Result<Option<(Value, &str)>, Error> {
    let content = strip_bom(content);
    let Some((yaml_start, yaml_end, body_start)) = fence_bounds(content) else {
        return Ok(None);
    };

    let yaml_str = &content[yaml_start..yaml_end];
    let body = content.get(body_start..).unwrap_or("");

    let value: Value = if yaml_str.trim().is_empty() {
        Value::Mapping(Default::default())
    } else {
        serde_yaml_ng::from_str(yaml_str)?
    };

    Ok(Some((value, body)))
}

/// Strip a single leading UTF-8 BOM (U+FEFF). Windows editors prepend one; a
/// BOM'd governed file must not read as no-frontmatter, because parse ABSENCE
/// is indistinguishable from not-governed and the file would silently pass the
/// walk (#78, consumer raise).
fn strip_bom(content: &str) -> &str {
    content.strip_prefix('\u{FEFF}').unwrap_or(content)
}

/// A fence line is `---` alone on its line, tolerating one trailing CR so CRLF
/// files (Windows-editor provenance) close their frontmatter like LF files do.
fn is_fence_line(line: &str) -> bool {
    line.strip_suffix('\r').unwrap_or(line) == "---"
}

/// Locate the frontmatter fences in (BOM-stripped) content. Returns
/// `(yaml_start, yaml_end, body_start)` byte offsets, or `None` when there is
/// no well-formed opening+closing fence pair.
///
/// The opening line must be exactly `---` (LF or CRLF). The closing fence is
/// the FIRST subsequent [`is_fence_line`] — deliberately including one inside a
/// block scalar: a fence scanner cannot know it is inside a scalar without a
/// full YAML parse, and vsdd-cli's loader shares this truncation semantics; the
/// #78 conformance corpus pins it so the two tools cannot silently diverge.
/// The closing line may lack a trailing newline (last-line-of-file case); the
/// empty block (`---\n---\n`) resolves uniformly with the typical case.
fn fence_bounds(content: &str) -> Option<(usize, usize, usize)> {
    let yaml_start = if content.starts_with("---\n") {
        4
    } else if content.starts_with("---\r\n") {
        5
    } else {
        return None;
    };

    let bytes = content.as_bytes();
    let mut pos = yaml_start;
    while pos <= bytes.len() {
        let line_end = bytes[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|rel| pos + rel)
            .unwrap_or(bytes.len());

        if is_fence_line(&content[pos..line_end]) {
            let body_start = if line_end < bytes.len() {
                line_end + 1
            } else {
                line_end
            };
            return Some((yaml_start, pos, body_start));
        }

        if line_end == bytes.len() {
            break;
        }
        pos = line_end + 1;
    }
    None
}

/// The frontmatter YAML substring (between the `---` markers), or `None` when
/// there is no well-formed block. The substring's first line is file line 2
/// (the opening `---` is line 1; a leading BOM sits on that same line, so line
/// numbering is unaffected). Used by [`resolve_pointer_location`].
fn yaml_block(content: &str) -> Option<&str> {
    let content = strip_bom(content);
    let (yaml_start, yaml_end, _) = fence_bounds(content)?;
    Some(&content[yaml_start..yaml_end])
}

/// Resolve every schema-violation location for one file in a SINGLE marked
/// parse. Each `(instance_path, unexpected_key)` yields the 1-based
/// `(line, column)` of the offending node, or `None` (the caller then falls back
/// to the block start). `unexpected_key` is `""` unless the violation is an
/// `additionalProperties`, in which case it is the offending key. Runs only on
/// the error path — never the happy path.
///
/// - The frontmatter is parsed once for all violations, not once per violation
///   (#70).
/// - The parse runs inside `catch_unwind`, so a panic in the pre-1.0 `saphyr`
///   parser degrades to `None` rather than aborting the whole run (#72).
/// - For an `additionalProperties` violation — whose pointer is the parent
///   object, not the offending key — the caller-supplied `unexpected_key` is
///   located, so the line points at the key itself rather than the mapping
///   start (#71).
///
/// Value: mdatron's diagnostics are agent-first (`DESIGN.md` § Agents are the
/// first consumer). A `file:line` is directly actionable where a bare pointer
/// forces the fixing agent to re-parse and count array indices — matching the
/// `location`-carrying diagnostics of the Thermite reference.
pub fn resolve_e0050_locations(content: &str, items: &[(&str, &str)]) -> Vec<Option<(u32, u32)>> {
    use saphyr::{LoadableYamlNode, MarkedYaml};

    let root: Option<MarkedYaml> = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let yaml = yaml_block(content)?;
        MarkedYaml::load_from_str(yaml).ok()?.into_iter().next()
    }))
    .ok()
    .flatten();

    let Some(root) = root else {
        return vec![None; items.len()];
    };
    items
        .iter()
        .map(|(pointer, unexpected_key)| resolve_one(&root, pointer, unexpected_key))
        .collect()
}

/// Single-pointer convenience wrapper over [`resolve_e0050_locations`].
pub fn resolve_pointer_location(content: &str, pointer: &str) -> Option<(u32, u32)> {
    resolve_e0050_locations(content, &[(pointer, "")])
        .into_iter()
        .next()
        .flatten()
}

fn resolve_one(
    root: &saphyr::MarkedYaml,
    pointer: &str,
    unexpected_key: &str,
) -> Option<(u32, u32)> {
    let node = walk_pointer(root, pointer)?;
    // additionalProperties: the pointer addresses the parent object; the caller
    // supplies the offending key (from the finding's quoted `unexpected` region,
    // no longer parsed out of a message string) so we can point at the key's own
    // line rather than the parent object's.
    if !unexpected_key.is_empty() {
        if let Some(loc) = key_location(node, unexpected_key) {
            return Some(loc);
        }
    }
    marker_to_location(node.span.start)
}

/// The 1-based file `(line, column)` of a marked node's start. saphyr lines are
/// 1-based within the block, whose first line is file line 2 (the opening `---`
/// is line 1), so `file_line = marker.line() + 1`; columns are 0-based, rendered
/// 1-based.
fn marker_to_location(marker: saphyr::Marker) -> Option<(u32, u32)> {
    let line = u32::try_from(marker.line()).ok()?.checked_add(1)?;
    let column = u32::try_from(marker.col()).ok()?.saturating_add(1);
    Some((line, column))
}

/// Locate a child key by name within a mapping node, returning the KEY's
/// position (not the value's) — used to point at an unexpected property.
fn key_location(node: &saphyr::MarkedYaml, key: &str) -> Option<(u32, u32)> {
    use saphyr::{Scalar, YamlData};
    let YamlData::Mapping(map) = &node.data else {
        return None;
    };
    map.iter().find_map(|(k, _)| match &k.data {
        YamlData::Value(Scalar::String(s)) if s.as_ref() == key => marker_to_location(k.span.start),
        _ => None,
    })
}

/// Walk a JSON Pointer (RFC 6901) over a marked YAML tree to the addressed node.
fn walk_pointer<'a, 'i>(
    root: &'a saphyr::MarkedYaml<'i>,
    pointer: &str,
) -> Option<&'a saphyr::MarkedYaml<'i>> {
    use saphyr::{Scalar, YamlData};

    if pointer.is_empty() {
        return Some(root);
    }
    let mut node = root;
    // '/'-separated tokens; unescape `~1` -> `/` then `~0` -> `~`.
    for raw in pointer.split('/').skip(1) {
        let token = raw.replace("~1", "/").replace("~0", "~");
        node = match &node.data {
            YamlData::Mapping(map) => map.iter().find_map(|(k, v)| match &k.data {
                YamlData::Value(Scalar::String(s)) if s.as_ref() == token.as_str() => Some(v),
                _ => None,
            })?,
            YamlData::Sequence(seq) => seq.get(token.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(node)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_frontmatter_and_returns_body() {
        let content = "---\nschema_class: design-doc\nversion: 0.1.0\n---\n\n# Title\nbody\n";
        let result = parse(content).expect("typical frontmatter should parse cleanly");
        let (fm, body) = result.expect("typical case returns Some, not None");
        assert_eq!(fm["schema_class"].as_str(), Some("design-doc"));
        assert_eq!(fm["version"].as_str(), Some("0.1.0"));
        assert_eq!(body, "\n# Title\nbody\n");
    }

    #[test]
    fn returns_none_for_no_frontmatter() {
        let content = "# Title\nbody\n";
        let result = parse(content).expect("no-frontmatter must not error");
        assert!(result.is_none(), "no frontmatter present; expected None");
    }

    #[test]
    fn returns_none_when_closing_marker_absent() {
        let content = "---\nschema_class: design-doc\n# Title body without closing\n";
        let result = parse(content).expect("missing-close must not error");
        assert!(
            result.is_none(),
            "no closing --- marker; should treat as no-frontmatter"
        );
    }

    #[test]
    fn handles_empty_frontmatter_as_empty_mapping() {
        let content = "---\n---\n# Title\n";
        let result = parse(content).expect("empty frontmatter is well-formed");
        let (fm, body) = result.expect("empty frontmatter still returns Some");
        assert!(
            fm.is_mapping(),
            "empty frontmatter should deserialize to a mapping value"
        );
        assert!(
            fm.as_mapping().expect("mapping").is_empty(),
            "empty frontmatter mapping should have no entries"
        );
        assert_eq!(body, "# Title\n");
    }

    #[test]
    fn dashes_inside_body_without_leading_newline_not_a_closer() {
        let content =
            "---\nfoo: bar\n---\n\n## Section\n---\nNot a closer because not at line start of body.\n";
        let (_fm, body) = parse(content)
            .expect("well-formed frontmatter parses")
            .expect("frontmatter present");
        // The body should still contain the `---` inside it (not be cut off at it).
        assert!(
            body.contains("---\nNot a closer"),
            "body should include the body-internal --- since it is not a real closing marker; got: {body:?}"
        );
    }

    #[test]
    fn malformed_yaml_returns_err() {
        let content = "---\n: invalid: : :\n---\n";
        let result = parse(content);
        assert!(
            result.is_err(),
            "malformed YAML between markers should return Err"
        );
    }

    // Pins the parser's duplicate-key contract across dependency changes
    // (#69: serde_yaml -> serde_yaml_ng). A duplicate mapping key is rejected,
    // not silently last-wins — the safe behavior a validation tool wants, and
    // an uncovered edge before this test.
    #[test]
    fn duplicate_mapping_key_returns_err() {
        let content = "---\ntitle: a\ntitle: b\n---\n";
        let result = parse(content);
        assert!(
            result.is_err(),
            "a duplicate mapping key must be rejected, not silently resolved"
        );
    }

    // #65: JSON-pointer -> source-line resolution. Fixture file lines:
    //   1: ---
    //   2: title: hello
    //   3: tags:
    //   4:   - a
    //   5:   - b
    //   6: count: 5
    //   7: ---
    const FIXTURE: &str = "---\ntitle: hello\ntags:\n  - a\n  - b\ncount: 5\n---\nbody\n";

    // ── #78 fence-edge conformance corpus (shared with vsdd-cli) ────────────
    //
    // The consumer raise: a BOM'd or CRLF'd governed artifact read as
    // no-frontmatter and PASSED the walk silently, while vsdd's lenient
    // consumer-side loader validated it — one file, validated by one tool,
    // invisible to the other. Parse FAILURE is loud (E0001); parse ABSENCE is
    // indistinguishable from not-governed, so absence must not be manufactured
    // by encoding trivia. Cases contributed by vsdd-cli; both projects test
    // against this corpus.

    // RED GATE (#78): BOM-prefixed opening fence. Pre-fix: Ok(None) — silent skip.
    #[test]
    fn bom_prefixed_opening_fence_is_frontmatter() {
        let content = "\u{FEFF}---\nschema_class: design-doc\n---\nbody\n";
        let (fm, body) = parse(content)
            .expect("BOM'd frontmatter should parse")
            .expect("BOM must not read as no-frontmatter (the silent-skip hole)");
        assert_eq!(fm["schema_class"].as_str(), Some("design-doc"));
        assert_eq!(body, "body\n");
    }

    // RED GATE (#78): CRLF fences (Windows-editor provenance). Pre-fix: Ok(None).
    #[test]
    fn crlf_fences_are_frontmatter() {
        let content = "---\r\nschema_class: design-doc\r\nversion: 0.1.0\r\n---\r\nbody\r\n";
        let (fm, body) = parse(content)
            .expect("CRLF frontmatter should parse")
            .expect("CRLF fences must not read as no-frontmatter");
        assert_eq!(fm["schema_class"].as_str(), Some("design-doc"));
        assert_eq!(fm["version"].as_str(), Some("0.1.0"));
        assert_eq!(body, "body\r\n");
    }

    // RED GATE (#78): both trivia at once.
    #[test]
    fn bom_and_crlf_combined_is_frontmatter() {
        let content = "\u{FEFF}---\r\nschema_class: design-doc\r\n---\r\nbody\n";
        let (fm, _body) = parse(content)
            .expect("BOM+CRLF frontmatter should parse")
            .expect("BOM+CRLF must not read as no-frontmatter");
        assert_eq!(fm["schema_class"].as_str(), Some("design-doc"));
    }

    // Corpus pin: a bare `---` line inside a frontmatter block scalar truncates
    // the frontmatter THERE. Both mdatron and vsdd's loader share this
    // semantics; the corpus pins it as deliberate, matching behavior (a fence
    // scanner cannot know it is inside a scalar without a full YAML parse, and
    // diverging silently between the two tools would reopen the split-brain
    // hole this corpus exists to close).
    #[test]
    fn bare_fence_inside_block_scalar_truncates_there() {
        let content = "---\nnote: |\n  before\n---\n  after\n---\nbody\n";
        let (fm, body) = parse(content)
            .expect("truncated-at-first-fence content parses")
            .expect("frontmatter present");
        // The frontmatter is everything before the FIRST bare fence line
        // (`|` literal scalars keep their trailing newline).
        assert_eq!(fm["note"].as_str(), Some("before\n"));
        assert!(
            body.starts_with("  after"),
            "body begins at the first bare fence; got: {body:?}"
        );
    }

    // Corpus pin: empty frontmatter block, CRLF variant.
    #[test]
    fn empty_frontmatter_block_crlf() {
        let content = "---\r\n---\r\nbody\n";
        let (fm, body) = parse(content)
            .expect("empty CRLF frontmatter is well-formed")
            .expect("empty CRLF frontmatter returns Some");
        assert!(fm.as_mapping().expect("mapping").is_empty());
        assert_eq!(body, "body\n");
    }

    // Corpus pin: unterminated fence stays no-frontmatter (BOM'd variant too —
    // tolerance must not manufacture frontmatter where none closes).
    #[test]
    fn unterminated_fence_is_no_frontmatter_even_with_bom() {
        for content in ["---\nkey: value\n", "\u{FEFF}---\r\nkey: value\r\n"] {
            let result = parse(content).expect("unterminated must not error");
            assert!(result.is_none(), "no closing fence => None for {content:?}");
        }
    }

    // The resolver walks the same fences: E0050 locations must survive BOM+CRLF.
    #[test]
    fn resolve_pointer_location_tolerates_bom_and_crlf() {
        let content = "\u{FEFF}---\r\na: 1\r\nb: 2\r\n---\r\nbody\n";
        let loc = resolve_pointer_location(content, "/b");
        // The pointer addresses b's VALUE node (`2`, col 4) — identical to the
        // LF case, proving BOM+CRLF shift neither line nor column.
        assert_eq!(
            loc,
            Some((3, 4)),
            "b's value sits on file line 3 col 4 despite BOM+CRLF"
        );
    }

    #[test]
    fn resolve_pointer_top_level_key() {
        // /title -> value "hello" on file line 2.
        assert_eq!(
            resolve_pointer_location(FIXTURE, "/title").map(|(l, _)| l),
            Some(2)
        );
    }

    #[test]
    fn resolve_pointer_nested_array_element() {
        // /tags/1 -> "b" on file line 5 — the high-value nested/array case an
        // agent would otherwise have to count to by hand.
        assert_eq!(
            resolve_pointer_location(FIXTURE, "/tags/1").map(|(l, _)| l),
            Some(5)
        );
    }

    #[test]
    fn resolve_pointer_later_key() {
        // /count -> value on file line 6.
        assert_eq!(
            resolve_pointer_location(FIXTURE, "/count").map(|(l, _)| l),
            Some(6)
        );
    }

    #[test]
    fn resolve_pointer_unresolvable_is_none() {
        // A pointer that names no node resolves to None (caller falls back).
        assert_eq!(resolve_pointer_location(FIXTURE, "/nope"), None);
        assert_eq!(resolve_pointer_location(FIXTURE, "/tags/9"), None);
        // No frontmatter block at all.
        assert_eq!(
            resolve_pointer_location("no frontmatter here\n", "/x"),
            None
        );
    }
}
