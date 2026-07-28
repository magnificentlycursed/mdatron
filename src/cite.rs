//! Citation family (#86; `DESIGN.md` § Five check families): file-and-line
//! citations in governed artifacts are verified against the snapshot of the
//! working tree — the working tree is authoritative, uncommitted edits count,
//! and no git subprocess is invoked.
//!
//! Data-less: the family activates per route via the optional
//! `citations: true` flag (amendment recorded on #86 — blanket activation over
//! all routed files would flood historical corpora whose citations were true
//! when written). The engine recognizes `path.ext:line` and
//! `path.ext:start-end` tokens in prose; a citation naming absent content is
//! rejected (`MDATRON-E0100`), one past the target's end is rejected
//! (`MDATRON-E0101`), and citation paths are held to the confinement contract
//! (escapes under `E0010`/`E0011`, symlinked targets refused `E0012`).

use std::path::Path;

use crate::confine::{confine_lexically, open_confined, LexicalViolation, OpenViolation};
use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};

/// Scan one opted-in file's prose for `file:line` citations and verify each
/// against the tree. `content` is the whole file; `body_offset` is where prose
/// begins.
pub fn check_file(
    project_root: &Path,
    path: &Path,
    content: &str,
    body_offset: usize,
    findings: &mut Vec<Finding>,
) {
    // path-with-extension : line [ - line ]. Conservative: the path segment
    // must carry an extension, so bare version numbers and prose ratios don't
    // read as citations.
    // Three shapes so confinement sees escapes intact: parent-prefixed,
    // absolute, and word-start relative (interior `..` rides the relative
    // branch's class and is still confinement-refused).
    let detector = regex_lite::Regex::new(
        r"((?:\.\./)+[A-Za-z0-9_/.-]*\.[A-Za-z][A-Za-z0-9]{0,7}|/[A-Za-z0-9_][A-Za-z0-9_/.-]*\.[A-Za-z][A-Za-z0-9]{0,7}|\b[A-Za-z0-9_][A-Za-z0-9_/.-]*\.[A-Za-z][A-Za-z0-9]{0,7}):([0-9]{1,6})(?:-([0-9]{1,6}))?",
    )
    .expect("engine citation detector compiles");

    let body = &content[body_offset..];
    for caps in detector.captures_iter(body) {
        let whole = caps.get(0).expect("match exists");
        // URL guard: a token whose match begins right after `/` or `:` is a
        // URL tail (http://x.md:1), not a citation.
        let prefix = &body[..whole.start()];
        if prefix.ends_with('/') || prefix.ends_with(':') {
            continue;
        }
        let cited_path = caps.get(1).expect("path group").as_str();
        let start_line: u64 = caps
            .get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let end_line: Option<u64> = caps.get(3).and_then(|m| m.as_str().parse().ok());

        let token = whole.as_str().to_string();
        let at = body_offset + whole.start();

        // Confinement first: a citation escaping the governed tree is refused
        // whether or not its target exists (the falsification clause).
        let confined = match confine_lexically(Path::new(cited_path)) {
            Ok(c) => c,
            Err(v) => {
                let (code, summary) = match v {
                    LexicalViolation::Absolute => ("MDATRON-E0010", "absolute-path-refused"),
                    LexicalViolation::ParentSegment => ("MDATRON-E0011", "parent-segment-refused"),
                };
                findings.push(cite_finding(
                    path,
                    content,
                    at,
                    code,
                    summary,
                    "a citation's path escapes the governed tree",
                    &token,
                ));
                continue;
            }
        };

        match open_confined(project_root, &confined) {
            Ok(mut handle) => {
                use std::io::Read;
                let mut target = String::new();
                if handle.read_to_string(&mut target).is_err() {
                    // Non-UTF8 or unreadable target: verified for existence
                    // only; line ranges are not checkable against bytes we
                    // cannot line-split, and a citation into such a file is
                    // not rejected on that basis.
                    continue;
                }
                let line_count = target.lines().count() as u64;
                let last = end_line.unwrap_or(start_line);
                if start_line == 0 || last > line_count || start_line > last {
                    findings.push(cite_finding(
                        path,
                        content,
                        at,
                        "MDATRON-E0101",
                        "citation-line-out-of-range",
                        &format!(
                            "the cited target has {line_count} line(s); the citation \
                             names content past its end (lines are 1-based)"
                        ),
                        &token,
                    ));
                }
            }
            Err(OpenViolation::Symlink { .. }) => {
                findings.push(cite_finding(
                    path,
                    content,
                    at,
                    "MDATRON-E0012",
                    "symlinked-component-refused",
                    "a citation's target resolves through a symbolic link; \
                     no-follow resolution refuses it",
                    &token,
                ));
            }
            Err(OpenViolation::Io(_)) => {
                findings.push(cite_finding(
                    path,
                    content,
                    at,
                    "MDATRON-E0100",
                    "dead-citation",
                    "the citation names content that does not exist in the \
                     working-tree snapshot (uncommitted content counts; no git \
                     history is consulted)",
                    &token,
                ));
            }
        }
    }
}

fn cite_finding(
    path: &Path,
    content: &str,
    offset: usize,
    code: &str,
    summary: &str,
    message: &str,
    token: &str,
) -> Finding {
    let line = 1 + content[..offset.min(content.len())].matches('\n').count() as u32;
    Finding {
        code: code.into(),
        severity: Severity::Error,
        summary: summary.into(),
        message: message.into(),
        help: None,
        location: Location {
            file: path.to_path_buf(),
            line,
            column: 0,
        },
        explain_ref: Some(code.to_string()),
        quoted: vec![QuotedRegion {
            label: "citation".into(),
            content: token.into(),
        }],
    }
}
