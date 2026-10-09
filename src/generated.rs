//! Generated regions (#252; GH #79 item 7): a marker pair in a governed
//! markdown file delimits a region whose bytes must equal a named committed
//! file, so a table or a block a project's own generator writes cannot drift
//! from its source unnoticed.
//!
//! ```text
//! <!-- mdatron:generated from="docs/generated/table.md" -->
//! …exactly the bytes of docs/generated/table.md…
//! <!-- /mdatron:generated -->
//! ```
//!
//! Opt-in per route (`generated: true`), like the link and citation families.
//! `from` is root-relative and confined like every adopter path. The region is
//! the bytes after the opening marker's line and before the closing marker's
//! line. A marker starts at column 0: an indented one (an indented code
//! block, a list item) and one inside fenced code are text, not markers. The
//! engine checks equality only — it never generates (the operator ruling of
//! 2026-10-08: no built-in renderer).
//!
//! Codes (the pin family's range: content equality against a named file):
//! `MDATRON-E0064` the region differs from its source; `E0065` the source is
//! missing; `E0066` a malformed marker (unclosed, nested, stray close, or no
//! `from`). An escaping `from` is `E0010`/`E0011`, a symlinked one `E0012`, an
//! oversized one `W0048`.

use std::path::Path;

use crate::confine::{confine_lexically, ConfinedPath, LexicalViolation};
use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};
use crate::markup::non_fenced_lines;
use crate::snapshot::{Captured, Snapshot};

/// What makes a line an opening marker (well-formed or not); the space keeps
/// a future `<!-- mdatron:generated-by … -->` from reading as one.
const OPEN_MARK: &str = "<!-- mdatron:generated ";
const OPEN_PREFIX: &str = "<!-- mdatron:generated from=\"";
const OPEN_SUFFIX: &str = "\" -->";
const CLOSE: &str = "<!-- /mdatron:generated -->";

/// One marker-pair outcome in a body, in document order.
#[derive(Debug, PartialEq)]
pub(crate) enum Marker<'a> {
    /// A well-formed region: `from`, the opening marker's offset, and the
    /// region's byte range in the body.
    Region {
        from: &'a str,
        at: usize,
        range: std::ops::Range<usize>,
    },
    /// A malformed marker at `at`, with why.
    Malformed { at: usize, why: &'static str },
}

/// Scan `body` for generated-region markers, outside fenced code.
pub(crate) fn markers(body: &str) -> Vec<Marker<'_>> {
    let mut out = Vec::new();
    // (from, opening offset, region start)
    let mut open: Option<(&str, usize, usize)> = None;
    for (offset, line) in non_fenced_lines(body) {
        // A marker starts at column 0 (#252 review): an indented line may be
        // an indented code block showing the syntax, and an indented region
        // could never equal an unindented source anyway. Trailing
        // whitespace is tolerated.
        let trimmed = line.trim_end();
        if trimmed.starts_with(OPEN_MARK) {
            if let Some((_, at, _)) = open {
                out.push(Marker::Malformed {
                    at,
                    why: "a generated region opens inside another one",
                });
            }
            let from = trimmed
                .strip_prefix(OPEN_PREFIX)
                .and_then(|r| r.strip_suffix(OPEN_SUFFIX))
                .filter(|f| !f.is_empty() && !f.contains('"'));
            match from {
                Some(from) => {
                    // The region starts after this line's line ending.
                    let start = body[offset..]
                        .find('\n')
                        .map_or(body.len(), |nl| offset + nl + 1);
                    open = Some((from, offset, start));
                }
                None => {
                    out.push(Marker::Malformed {
                        at: offset,
                        why: "an opening marker must read exactly \
                              <!-- mdatron:generated from=\"<path>\" --> with a non-empty path",
                    });
                    open = None;
                }
            }
        } else if trimmed == CLOSE {
            match open.take() {
                Some((from, at, start)) => out.push(Marker::Region {
                    from,
                    at,
                    range: start..offset.max(start),
                }),
                None => out.push(Marker::Malformed {
                    at: offset,
                    why: "a closing marker with no opening marker before it",
                }),
            }
        }
    }
    if let Some((_, at, _)) = open {
        out.push(Marker::Malformed {
            at,
            why: "a generated region is never closed",
        });
    }
    out
}

/// The sources the regions in `content` name, confined, for capture before
/// the seam — the same extraction [`check_file`] runs, so the two agree.
pub(crate) fn targets(content: &str, body_offset: usize) -> Vec<ConfinedPath> {
    markers(&content[body_offset..])
        .into_iter()
        .filter_map(|m| match m {
            Marker::Region { from, .. } => confine_lexically(Path::new(from)).ok(),
            Marker::Malformed { .. } => None,
        })
        .collect()
}

/// Check every generated region in one governed file against its source.
pub(crate) fn check_file(
    snapshot: &Snapshot,
    path: &Path,
    content: &str,
    body_offset: usize,
    findings: &mut Vec<Finding>,
) {
    let body = &content[body_offset..];
    let line_of = |at: usize| 1 + content[..body_offset + at].matches('\n').count() as u32;
    let finding = |at: usize, code: &str, summary: &str, message: String, quoted| Finding {
        code: code.into(),
        severity: if code == "MDATRON-W0048" {
            Severity::Warning
        } else {
            Severity::Error
        },
        summary: summary.into(),
        message,
        help: None,
        location: Location {
            file: path.to_path_buf(),
            line: line_of(at),
            column: 0,
        },
        explain_ref: Some(code.into()),
        quoted,
    };
    let source = |from: &str| {
        vec![QuotedRegion {
            platform_variant: false,
            label: "from".into(),
            content: from.into(),
        }]
    };
    for marker in markers(body) {
        let (from, at, range) = match marker {
            Marker::Region { from, at, range } => (from, at, range),
            Marker::Malformed { at, why } => {
                findings.push(finding(
                    at,
                    "MDATRON-E0066",
                    "generated-region-malformed",
                    format!("this generated-region marker is malformed: {why}"),
                    Vec::new(),
                ));
                continue;
            }
        };
        let confined = match confine_lexically(Path::new(from)) {
            Ok(c) => c,
            Err(v) => {
                let (code, summary) = match v {
                    LexicalViolation::Absolute => ("MDATRON-E0010", "absolute-path-refused"),
                    LexicalViolation::ParentSegment => ("MDATRON-E0011", "parent-segment-refused"),
                };
                findings.push(finding(
                    at,
                    code,
                    summary,
                    "a generated region's `from` must be a root-relative path inside \
                     the governed tree"
                        .into(),
                    source(from),
                ));
                continue;
            }
        };
        let region = &body.as_bytes()[range];
        match snapshot.get(confined.as_path()) {
            Some(Captured::Content(c)) => {
                if c.bytes() != region {
                    findings.push(finding(
                        at,
                        "MDATRON-E0064",
                        "generated-region-stale",
                        format!(
                            "this generated region ({} bytes) does not equal its source ({} \
                             bytes); regenerate it from the source",
                            region.len(),
                            c.bytes().len()
                        ),
                        source(from),
                    ));
                }
            }
            Some(Captured::TooLarge { .. }) => findings.push(finding(
                at,
                "MDATRON-W0048",
                "reference-target-unverified",
                "this generated region's source is over the input size budget, so the \
                 region was not compared"
                    .into(),
                source(from),
            )),
            Some(Captured::SymlinkRefused { tag, .. }) => {
                let reparse = crate::confine::describe_reparse(tag.as_ref().copied());
                findings.push(finding(
                    at,
                    "MDATRON-E0012",
                    "symlinked-component-refused",
                    format!(
                        "a generated region's source resolves through {}; no-follow \
                         resolution refuses it",
                        reparse.what
                    ),
                    source(from),
                ));
            }
            Some(Captured::OpenedUnreadable { .. } | Captured::OpenIo { .. }) => {
                findings.push(finding(
                    at,
                    "MDATRON-E0065",
                    "generated-region-source-missing",
                    "this generated region's source is missing or not a readable regular \
                     file in the working tree"
                        .into(),
                    source(from),
                ))
            }
            None => findings.push(finding(
                at,
                "MDATRON-E0081",
                "reference-target-not-captured",
                "this generated region's source was never captured into the run snapshot \
                 (an engine defect); the region is unverified"
                    .into(),
                source(from),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_find_regions_and_malformed_pairs() {
        let body = "intro\n\
                    <!-- mdatron:generated from=\"gen/t.md\" -->\n\
                    | a |\n\
                    <!-- /mdatron:generated -->\n\
                    ```\n\
                    <!-- mdatron:generated from=\"x.md\" -->\n\
                    ```\n\
                    <!-- /mdatron:generated -->\n\
                    <!-- mdatron:generated from=\"\" -->\n\
                    \u{20}\u{20}\u{20}\u{20}<!-- mdatron:generated from=\"indented.md\" -->\n\
                    <!-- mdatron:generated-by tool -->\n\
                    <!-- mdatron:generated from=\"a.md\" -->\n\
                    <!-- mdatron:generated from=\"b.md\" -->\n";
        let m = markers(body);
        match &m[0] {
            Marker::Region { from, range, .. } => {
                assert_eq!(*from, "gen/t.md");
                assert_eq!(&body[range.clone()], "| a |\n");
            }
            other => panic!("{other:?}"),
        }
        let malformed: Vec<&str> = m
            .iter()
            .filter_map(|x| match x {
                Marker::Malformed { why, .. } => Some(*why),
                _ => None,
            })
            .collect();
        // The fenced example is not a marker; the stray close, the empty
        // `from`, the nested open and the unclosed one are each malformed.
        assert_eq!(malformed.len(), 4, "{m:?}");
        assert!(malformed[0].contains("closing marker"));
        assert!(malformed[1].contains("exactly"));
        assert!(malformed[2].contains("inside another"));
        assert!(malformed[3].contains("never closed"));
    }

    #[test]
    fn an_empty_region_is_the_empty_slice() {
        let body = "<!-- mdatron:generated from=\"e.md\" -->\n<!-- /mdatron:generated -->\n";
        match &markers(body)[0] {
            Marker::Region { range, .. } => assert!(range.is_empty()),
            other => panic!("{other:?}"),
        }
    }
}
