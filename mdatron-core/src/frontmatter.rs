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
    // The first line must be exactly `---` followed by a newline.
    if !content.starts_with("---\n") {
        return Ok(None);
    }

    // Walk lines from byte 4 (after the opening `---\n`) looking for a closing `---` line.
    // A closing line is: line content exactly equal to `---`. The line may or may not have a
    // trailing newline (last-line-of-file case). This handles the empty-frontmatter case
    // (`---\n---\n`) uniformly with the typical case.
    let bytes = content.as_bytes();
    let mut pos = 4usize;
    let mut yaml_end: Option<usize> = None;
    let mut after_close: Option<usize> = None;

    while pos <= bytes.len() {
        let line_end = bytes[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|rel| pos + rel)
            .unwrap_or(bytes.len());

        if &content[pos..line_end] == "---" {
            yaml_end = Some(pos);
            after_close = Some(if line_end < bytes.len() {
                line_end + 1
            } else {
                line_end
            });
            break;
        }

        if line_end == bytes.len() {
            break;
        }
        pos = line_end + 1;
    }

    let yaml_end = match yaml_end {
        Some(e) => e,
        None => return Ok(None),
    };
    let body_start = after_close.unwrap_or(bytes.len());

    let yaml_str = &content[4..yaml_end];
    let body = content.get(body_start..).unwrap_or("");

    let value: Value = if yaml_str.trim().is_empty() {
        Value::Mapping(Default::default())
    } else {
        serde_yaml_ng::from_str(yaml_str)?
    };

    Ok(Some((value, body)))
}

/// The frontmatter YAML substring (between the `---` markers), or `None` when
/// there is no well-formed block. The substring's first line is file line 2
/// (the opening `---` is line 1). Used by [`resolve_pointer_location`].
fn yaml_block(content: &str) -> Option<&str> {
    if !content.starts_with("---\n") {
        return None;
    }
    let bytes = content.as_bytes();
    let mut pos = 4usize;
    while pos <= bytes.len() {
        let line_end = bytes[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|rel| pos + rel)
            .unwrap_or(bytes.len());
        if &content[pos..line_end] == "---" {
            return Some(&content[4..pos]);
        }
        if line_end == bytes.len() {
            break;
        }
        pos = line_end + 1;
    }
    None
}

/// Resolve a schema-validation JSON Pointer (`instance_path`, e.g.
/// `/action_vocabulary/20`) to its 1-based `(line, column)` in the original
/// file, by re-parsing the frontmatter with the position-tracking `saphyr`
/// parser. Runs only on the schema-violation error path — never the happy path.
/// Returns `None` when there is no frontmatter block or the pointer does not
/// resolve to a node; the caller then falls back to the block's start line.
///
/// The value: mdatron's diagnostics are agent-first (`DESIGN.md` § Agents are
/// the first consumer). A bare pointer forces the fixing agent to re-parse and
/// count array indices to locate the edit; a `file:line` is directly
/// actionable, matching the `location`-carrying diagnostics of the Thermite
/// reference.
pub fn resolve_pointer_location(content: &str, pointer: &str) -> Option<(u32, u32)> {
    use saphyr::{LoadableYamlNode, MarkedYaml};

    let yaml = yaml_block(content)?;
    let docs = MarkedYaml::load_from_str(yaml).ok()?;
    let node = walk_pointer(docs.first()?, pointer)?;
    let marker = node.span.start;
    // saphyr lines are 1-based within `yaml`, whose first line is file line 2
    // (the opening `---` is line 1): file_line = marker.line() + 1. Columns are
    // 0-based, rendered 1-based.
    let line = u32::try_from(marker.line()).ok()?.checked_add(1)?;
    let column = u32::try_from(marker.col()).ok()?.saturating_add(1);
    Some((line, column))
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
