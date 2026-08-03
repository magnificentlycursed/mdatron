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

use crate::confine::{confine_lexically, ConfinedPath, LexicalViolation};
use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};
use crate::snapshot::{Captured, Snapshot};

/// One detected citation token in a file's prose.
struct Citation<'a> {
    token: String,
    cited_path: &'a str,
    start_line: u64,
    end_line: Option<u64>,
    /// Byte offset of the token in the WHOLE file content.
    at: usize,
}

/// Extract every citation token from `content[body_offset..]`. This is the ONE
/// extraction both target discovery (#103, pre-seam capture) and the check run,
/// so the set of targets the check consults is byte-identical to the set that
/// was captured.
fn citations(content: &str, body_offset: usize) -> Vec<Citation<'_>> {
    // path-with-extension : line [ - line ]. Conservative: the path segment
    // must carry an extension, so bare version numbers and prose ratios don't
    // read as citations.
    // Three shapes so confinement sees escapes intact: parent-prefixed,
    // absolute, and word-start relative (interior `..` rides the relative
    // branch's class and is still confinement-refused).
    // Compiled once: this extraction runs twice per opted-in file (discovery +
    // check), so a per-call compile would tax the hot path (phase-3 I-6).
    static DETECTOR: std::sync::LazyLock<regex_lite::Regex> = std::sync::LazyLock::new(|| {
        regex_lite::Regex::new(
            r"((?:\.\./)+[A-Za-z0-9_/.-]*\.[A-Za-z][A-Za-z0-9]{0,7}|/[A-Za-z0-9_][A-Za-z0-9_/.-]*\.[A-Za-z][A-Za-z0-9]{0,7}|\b[A-Za-z0-9_][A-Za-z0-9_/.-]*\.[A-Za-z][A-Za-z0-9]{0,7}):([0-9]{1,6})(?:-([0-9]{1,6}))?",
        )
        .expect("engine citation detector compiles")
    });
    let detector = &*DETECTOR;

    let body = &content[body_offset..];
    let mut out = Vec::new();
    for caps in detector.captures_iter(body) {
        let whole = caps.get(0).expect("match exists");
        // URL guard: a token whose match begins right after `/` or `:` is a
        // URL tail (http://x.md:1), not a citation.
        let prefix = &body[..whole.start()];
        if prefix.ends_with('/') || prefix.ends_with(':') {
            continue;
        }
        out.push(Citation {
            token: whole.as_str().to_string(),
            cited_path: caps.get(1).expect("path group").as_str(),
            start_line: caps
                .get(2)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0),
            end_line: caps.get(3).and_then(|m| m.as_str().parse().ok()),
            at: body_offset + whole.start(),
        });
    }
    out
}

/// The confinement-accepted citation targets of one file's prose — the paths
/// target discovery captures into the snapshot before the seam (#103).
/// Lexically-escaping citations never reach the filesystem (the check reports
/// them on path text alone), so they are not returned.
pub(crate) fn cited_targets(content: &str, body_offset: usize) -> Vec<ConfinedPath> {
    citations(content, body_offset)
        .into_iter()
        .filter_map(|c| confine_lexically(Path::new(c.cited_path)).ok())
        .collect()
}

/// Scan one opted-in file's prose for `file:line` citations and verify each
/// against the captured snapshot (#103): the same bytes every other check saw,
/// never a post-seam filesystem read. `content` is the whole file;
/// `body_offset` is where prose begins.
pub fn check_file(
    snapshot: &Snapshot,
    path: &Path,
    content: &str,
    body_offset: usize,
    findings: &mut Vec<Finding>,
) {
    for citation in citations(content, body_offset) {
        let Citation {
            token,
            cited_path,
            start_line,
            end_line,
            at,
        } = citation;

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

        // The target's capture-time state. `citations` is the same extraction
        // discovery ran, so a confined target is always captured; a miss is an
        // engine defect and reports as one (the None arm), never a filesystem
        // fallback.
        match snapshot.get(confined.as_path()) {
            Some(Captured::Content(c)) => {
                let Some(target) = c.text() else {
                    // Non-UTF8 target: verified for existence only; line
                    // ranges are not checkable against bytes we cannot
                    // line-split, and a citation into such a file is not
                    // rejected on that basis.
                    continue;
                };
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
            // Opened but unreadable (non-UTF8, FIFO, directory): the target
            // EXISTS; existence is verified and the line range is not
            // checkable against bytes we cannot line-split — the pre-#103
            // posture, unchanged and quiet by necessity.
            Some(Captured::OpenedUnreadable { .. }) => {}
            // Over the size budget: the target exists and COULD be verified —
            // the check was skipped by budget, not necessity, so the
            // degradation is loud (W0048): a fabricated or out-of-range
            // citation into an oversized file must not silently satisfy a
            // gate (#103 phase-3 R2S-2). Warning severity keeps the run
            // alive; `--deny-warnings` gates can escalate it.
            Some(Captured::TooLarge { .. }) => {
                let mut f = cite_finding(
                    path,
                    content,
                    at,
                    "MDATRON-W0048",
                    "reference-target-unverified",
                    "this citation's target exceeds the input size budget, so \
                     its line range was NOT verified — existence only; raise \
                     attention on the reference or shrink the target",
                    &token,
                );
                f.severity = Severity::Warning;
                findings.push(f);
            }
            Some(Captured::SymlinkRefused { .. }) => {
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
            // Accepted residue (#103 phase-3 R2I-6): OpenIo conflates
            // absent with open-refused (EACCES), so a permission-denied
            // target reports as dead — matching the pre-#103 live-read
            // behavior; splitting the state is future work.
            Some(Captured::OpenIo { .. }) => {
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
            // Never captured: an ENGINE defect (discovery failed to mirror
            // this extraction), not a defect in this document — misreporting
            // it as a dead citation would send the adopter to fix a healthy
            // file (#103 phase-3 I-5/A-3).
            None => {
                findings.push(cite_finding(
                    path,
                    content,
                    at,
                    "MDATRON-E0080",
                    "pipeline-orchestration-failure",
                    "this citation's target was never captured into the run \
                     snapshot — an engine defect in target discovery, not a \
                     defect in this document; please report it upstream",
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
