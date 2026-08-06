//! Link family (#145; `DESIGN.md` § check families): inline body links in
//! governed markdown are resolved against the snapshot of the working tree
//! (#103 — the run's capture-time view; uncommitted content counts, no git
//! history is consulted) — a link to a file that is not there, or a fragment
//! that matches no heading, is a dead link.
//!
//! Data-less, per-route opt-in via the optional `links: true` flag (mirrors the
//! citation family's `citations: true`): blanket activation over every routed
//! file would flood historical corpora whose links have since rotted, so a
//! route declares the check on its own scope.
//!
//! **Resolution is DOCUMENT-relative** (CommonMark / GitHub semantics): a link
//! target resolves against the *containing file's directory*, not the project
//! root — `[api](api.md)` in `docs/guide.md` is `docs/api.md`, and
//! `[readme](../README.md)` is the repository README. This diverges from the
//! citation family, which treats `path:line` tokens as project-root-relative
//! and refuses every `..` outright (BOUNDARY-PREAMBLE § 7). The divergence is
//! deliberate: `..` is *ordinary, meaningful navigation* in a markdown link,
//! and a checker that flagged every `../README.md` would be unusable in exactly
//! the `docs/**` corpora this family targets. Confinement is therefore decided
//! on the *resolved* path — `..` is collapsed against the containing directory
//! (lexically, no filesystem walk), and only a target that escapes ABOVE the
//! root is refused (`MDATRON-E0011`); an absolute target is refused
//! (`MDATRON-E0010`) and a symlinked component refused (`MDATRON-E0012`), so the
//! family-wide confinement contract still holds on every target that is opened.
//!
//! **Optional root-relative mode** (`link_root: true` on a route, GH #37): a
//! leading-slash link `/docs/x.md` resolves from the project ROOT instead of
//! being refused `E0010` — the opt-in analog of lychee's `--root-dir`
//! ([[lychee-link-family-audit]] item 5), for static-site corpora that author
//! links that way. Confinement is unchanged: a `..` climbing above the root is
//! still `E0011`, a symlinked component `E0012`. Document-relative remains the
//! default (lychee's default too); root-relative is a per-consumer convention.
//! Destinations are **percent-decoded** before resolution (`my%20doc.md` →
//! `my doc.md`), all-or-nothing so a malformed escape never corrupts a path.
//!
//! Findings: a link whose in-tree target does not exist is `MDATRON-E0110`
//! (dead-link-target); an existing markdown target (or the same document) whose
//! `#fragment` matches no heading is `MDATRON-E0111` (dead-anchor). Anchors are
//! matched with the GitHub heading-slug algorithm.
//!
//! Link discovery is a single **CommonMark parse** (`markup::body_links`, #155):
//! inline `[t](d)`, **reference-style** `[t][ref]`, and **image** `![alt](src)`
//! links are all resolved, and a destination inside an inline code span or a
//! fenced/indented code block is not a link event, so a syntax example is
//! excluded structurally (retiring the hand-rolled scanner and its #154 defect).
//! Heading anchors cover ATX **and setext** headings, GitHub duplicate-heading
//! `-N` disambiguation, and explicit HTML anchors (`markup::heading_slugs`).
//!
//! External links (any URL scheme, or protocol-relative `//host`) are out of
//! scope by design — the engine does not reach the network.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use crate::confine::{confine_lexically, ConfinedPath};
use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};
use crate::markup::{body_links, heading_slugs, slugify};
use crate::snapshot::{Captured, Snapshot};

/// The confinement-accepted link targets of one file's body — the paths target
/// discovery captures into the snapshot before the seam (#103). Runs the SAME
/// CommonMark extraction and document-relative resolution the check runs, so
/// the check-time target set cannot diverge from the captured set. External
/// links, same-document fragments, and confinement violations resolve to no
/// target and are not returned.
pub(crate) fn link_targets(
    rel: &Path,
    content: &str,
    body_offset: usize,
    root_relative: bool,
) -> Vec<ConfinedPath> {
    let base_dir = rel.parent().unwrap_or_else(|| Path::new(""));
    let body = &content[body_offset..];
    let mut out = Vec::new();
    for link in body_links(body) {
        if is_external(&link.dest) {
            continue;
        }
        let (path_part, _anchor) = split_fragment(&link.dest);
        if path_part.is_empty() {
            continue;
        }
        let path_part = percent_decode(path_part);
        if let Ok(confined) = resolve_target(base_dir, path_part.as_ref(), root_relative) {
            // Skip a bare-root `/` (empty resolved path): the check skips it too
            // (GH #37), so capturing it would be a discovery/check mismatch.
            if !confined.as_path().as_os_str().is_empty() {
                out.push(confined);
            }
        }
    }
    out
}

/// Scan one opted-in file's body for links + images and resolve each against
/// the captured snapshot (#103) — the same bytes every other check saw, never
/// a post-seam filesystem read. `content` is the whole file; `body_offset` is
/// where the prose body begins (frontmatter is not link-scanned). Signature
/// mirrors [`crate::cite::check_file`]. Link discovery is a single CommonMark
/// parse ([`body_links`]) covering inline, reference-style, and image links, so
/// a destination inside a code span or fence is excluded structurally.
#[allow(clippy::too_many_arguments)]
pub fn check_file(
    snapshot: &Snapshot,
    project_root: &Path,
    path: &Path,
    content: &str,
    body_offset: usize,
    root_relative: bool,
    findings: &mut Vec<Finding>,
) {
    // The containing file's directory, root-relative — the base every
    // document-relative target resolves against. `path` is the engine-supplied
    // absolute path; strip the root to get the governed-tree-relative form
    // (a no-op if already relative). `parent()` of `docs/x.md` is `docs`.
    let rel = path.strip_prefix(project_root).unwrap_or(path);
    let base_dir = rel.parent().unwrap_or_else(|| Path::new(""));

    let body = &content[body_offset..];

    // This file's own heading slugs, for same-document `#fragment` links.
    let own_slugs = heading_slugs(body);

    // Cache of a target file's heading slugs (None = target exists but is not
    // anchor-checkable — non-markdown, or unreadable — so its fragment is not
    // resolved), keyed by resolved root-relative path. Avoids re-reading a
    // target linked from several places.
    let mut target_slugs: HashMap<PathBuf, Option<HashSet<String>>> = HashMap::new();

    // One CommonMark parse yields every inline / reference-style / image link's
    // destination with its byte offset. Destinations inside a code span or a
    // fenced/indented code block are not link events, so a syntax example is
    // never resolved — the masking is structural (#155), not a line heuristic.
    for link in body_links(body) {
        let at = body_offset + link.offset;
        resolve_link(
            snapshot,
            path,
            content,
            at,
            base_dir,
            &link.dest,
            &own_slugs,
            root_relative,
            &mut target_slugs,
            findings,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_link(
    snapshot: &Snapshot,
    path: &Path,
    content: &str,
    at: usize,
    base_dir: &Path,
    dest: &str,
    own_slugs: &HashSet<String>,
    root_relative: bool,
    target_slugs: &mut HashMap<PathBuf, Option<HashSet<String>>>,
    findings: &mut Vec<Finding>,
) {
    // External links (any URL scheme, or protocol-relative `//host`) are not
    // the engine's to resolve — it never reaches the network.
    if is_external(dest) {
        return;
    }

    let (path_part, anchor) = split_fragment(dest);

    // Same-document link: only a `#fragment`, no path. Resolve against this
    // file's own headings. A bare `#` (top of page) is always valid.
    if path_part.is_empty() {
        if let Some(frag) = anchor {
            // GitHub percent-decodes the fragment before slug matching, so
            // `#caf%C3%A9` resolves the "Café" heading (GH #37, the fragment
            // twin of the path percent-decode).
            let frag = percent_decode(frag);
            if !frag.is_empty() && !own_slugs.contains(&slugify(&frag)) {
                findings.push(link_finding(
                    path,
                    content,
                    at,
                    "MDATRON-E0111",
                    "dead-anchor",
                    "this same-document link names a `#fragment` that matches no \
                     heading in this file (fragments resolve via the GitHub \
                     heading-slug algorithm)",
                    dest,
                ));
            }
        }
        return;
    }

    // Document-relative confinement decided on the RESOLVED path. Percent-decode
    // first (GH #37): `my%20doc.md` resolves to `my doc.md` rather than being a
    // literal E0110 false positive. The finding still quotes the ORIGINAL `dest`.
    let confined = match resolve_target(base_dir, percent_decode(path_part).as_ref(), root_relative)
    {
        Ok(c) => c,
        Err(TargetViolation::Absolute) => {
            findings.push(link_finding(
                path,
                content,
                at,
                "MDATRON-E0010",
                "absolute-path-refused",
                "a link target is an absolute path; the governed tree admits \
                 only relative, in-tree targets",
                dest,
            ));
            return;
        }
        Err(TargetViolation::Escapes) => {
            findings.push(link_finding(
                path,
                content,
                at,
                "MDATRON-E0011",
                "parent-segment-refused",
                "a link target resolves outside the governed tree (its `../` \
                 segments climb above the project root)",
                dest,
            ));
            return;
        }
    };

    // A bare root-relative `/` (GH #37) resolves to the empty path — the
    // project root DIRECTORY, not a file. It is not a document-link target, so
    // it is out of the family's scope rather than a dead link (reporting E0110
    // "does not exist" would be wrong: the root exists).
    if confined.as_path().as_os_str().is_empty() {
        return;
    }

    // The target's capture-time state (#103): `link_targets` is the same
    // extraction discovery ran, so a confined target is always captured; a
    // miss is an engine defect and reports as one (the None arm), never a
    // filesystem fallback.
    match snapshot.get(confined.as_path()) {
        Some(Captured::Content(c)) => {
            // Target exists. If the link carries a fragment and the target is a
            // markdown file, the fragment must match one of its headings.
            let Some(frag) = anchor else { return };
            // GitHub percent-decodes the fragment before slug matching (GH #37).
            let frag = percent_decode(frag);
            if frag.is_empty() || !is_markdown(confined.as_path()) {
                return;
            }
            let key = confined.as_path().to_path_buf();
            if !target_slugs.contains_key(&key) {
                // Non-UTF8 markdown target: existence is verified, the
                // fragment is not resolved (not flagged).
                let slugs = c.text().map(|t| heading_slugs(markdown_body(t)));
                target_slugs.insert(key.clone(), slugs);
            }
            if let Some(Some(slugs)) = target_slugs.get(&key) {
                if !slugs.contains(&slugify(&frag)) {
                    findings.push(link_finding(
                        path,
                        content,
                        at,
                        "MDATRON-E0111",
                        "dead-anchor",
                        "this link's `#fragment` matches no heading in the target \
                         file (fragments resolve via the GitHub heading-slug \
                         algorithm)",
                        dest,
                    ));
                }
            }
        }
        // Opened but unreadable (non-UTF8, FIFO, directory): the target
        // exists; its fragment (if any) is not resolved — the pre-#103
        // posture, unchanged and quiet by necessity.
        Some(Captured::OpenedUnreadable { .. }) => {}
        // Over the size budget: the target exists. W0048 fires ONLY when a
        // check was actually skipped — a fragment-bearing link to a markdown
        // target (the anchor check needed the bytes). A fragment-less link,
        // or a non-markdown target, is FULLY verified by existence alone, and
        // warning there would fail `--deny-warnings` gates on verified links
        // (#103 phase-3 R3-3). Warning severity keeps the run alive.
        Some(Captured::TooLarge { .. }) => {
            let anchor_check_skipped =
                anchor.is_some_and(|frag| !frag.is_empty()) && is_markdown(confined.as_path());
            if anchor_check_skipped {
                let mut f = link_finding(
                    path,
                    content,
                    at,
                    "MDATRON-W0048",
                    "reference-target-unverified",
                    "this link's target exceeds the input size budget, so its \
                     fragment was NOT resolved — existence only; raise attention \
                     on the link or shrink the target",
                    dest,
                );
                f.severity = Severity::Warning;
                findings.push(f);
            }
        }
        Some(Captured::SymlinkRefused { .. }) => {
            findings.push(link_finding(
                path,
                content,
                at,
                "MDATRON-E0012",
                "symlinked-component-refused",
                "a link's target resolves through a symbolic link; no-follow \
                 resolution refuses it",
                dest,
            ));
        }
        Some(Captured::OpenIo { .. }) => {
            findings.push(link_finding(
                path,
                content,
                at,
                "MDATRON-E0110",
                "dead-link-target",
                "this link points at a relative path that does not exist in the \
                 working tree (uncommitted content counts; no git history is \
                 consulted)",
                dest,
            ));
        }
        // Never captured: an ENGINE defect in target discovery, not a dead
        // link — misreporting would send the adopter to fix a healthy file
        // (#103 phase-3 I-5/A-3).
        None => {
            findings.push(link_finding(
                path,
                content,
                at,
                "MDATRON-E0080",
                "pipeline-orchestration-failure",
                "this link's target was never captured into the run snapshot — \
                 an engine defect in target discovery, not a defect in this \
                 document; please report it upstream",
                dest,
            ));
        }
    }
}

/// A document-relative confinement violation, decided on path text alone.
#[derive(Debug)]
enum TargetViolation {
    /// An absolute (root/prefix) target. `MDATRON-E0010` territory.
    Absolute,
    /// The `../` segments climb above the project root. `MDATRON-E0011`.
    Escapes,
}

/// Resolve `path_part` relative to `base_dir` (the containing file's directory,
/// itself an in-tree relative path), collapsing `.`/`..` lexically, and return
/// the root-relative target as a [`crate::confine::ConfinedPath`]. Touches no
/// filesystem, so a missing target is judged on the same basis as a present
/// one. `..` that climbs above the root is [`TargetViolation::Escapes`]; an
/// absolute `path_part` is [`TargetViolation::Absolute`].
fn resolve_target(
    base_dir: &Path,
    path_part: &str,
    root_relative: bool,
) -> Result<crate::confine::ConfinedPath, TargetViolation> {
    // Seed the stack with the containing directory's components (all Normal by
    // construction — it is a walked governed path).
    let mut stack: Vec<std::ffi::OsString> = base_dir
        .components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(n.to_os_string()),
            _ => None,
        })
        .collect();

    for component in Path::new(path_part).components() {
        match component {
            // A drive/UNC prefix is always absolute — root-relative mode does
            // not admit it.
            Component::Prefix(_) => return Err(TargetViolation::Absolute),
            // A leading `/`: document-relative refuses it (E0010); root-relative
            // (GH #37) resolves it from the project root — discard the base
            // directory and start the stack empty, still fully confined.
            Component::RootDir => {
                if root_relative {
                    stack.clear();
                } else {
                    return Err(TargetViolation::Absolute);
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                // Pop within the tree; popping past the root is an escape.
                if stack.pop().is_none() {
                    return Err(TargetViolation::Escapes);
                }
            }
            Component::Normal(n) => stack.push(n.to_os_string()),
        }
    }

    // The collapsed path has no `.`/`..` left, so lexical confinement accepts
    // it (its sole remaining job here is to mint the ConfinedPath the confined
    // open requires).
    let mut resolved = PathBuf::new();
    for c in &stack {
        resolved.push(c);
    }
    confine_lexically(&resolved).map_err(|_| TargetViolation::Escapes)
}

/// Percent-decode a link path component (GH #37): `my%20doc.md` → `my doc.md`.
///
/// **All-or-nothing per destination** — if any `%` sequence is malformed (not
/// followed by two hex digits) or the decoded bytes are not valid UTF-8, the
/// original literal is returned unchanged. A link is therefore never resolved
/// to a corrupted path, so decoding can only *remove* a false positive, never
/// introduce a false resolution. Applied AFTER fragment splitting (RFC 3986:
/// the fragment delimiter is a literal `#`; an encoded `%23` is path content).
fn percent_decode(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains('%') {
        return std::borrow::Cow::Borrowed(s);
    }
    let hex = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    };
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            match (
                (i + 3 <= bytes.len()).then(|| hex(bytes[i + 1])).flatten(),
                (i + 3 <= bytes.len()).then(|| hex(bytes[i + 2])).flatten(),
            ) {
                (Some(hi), Some(lo)) => {
                    out.push(hi * 16 + lo);
                    i += 3;
                    continue;
                }
                // Malformed `%` sequence: leave the whole destination literal.
                _ => return std::borrow::Cow::Borrowed(s),
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    match String::from_utf8(out) {
        Ok(decoded) => std::borrow::Cow::Owned(decoded),
        // Decoded bytes are not UTF-8: keep the literal rather than corrupt it.
        Err(_) => std::borrow::Cow::Borrowed(s),
    }
}

/// Split a destination into its path and optional `#fragment`. The first `#`
/// delimits the fragment (URL fragments always do); everything before is the
/// path, everything after is the fragment (possibly empty).
fn split_fragment(dest: &str) -> (&str, Option<&str>) {
    match dest.find('#') {
        Some(i) => (&dest[..i], Some(&dest[i + 1..])),
        None => (dest, None),
    }
}

/// True when `dest` carries a URL scheme (`http:`, `mailto:`, …) or is
/// protocol-relative (`//host`) — an external reference the engine does not
/// resolve. A scheme is `[A-Za-z][A-Za-z0-9+.-]*:`.
fn is_external(dest: &str) -> bool {
    if dest.starts_with("//") {
        return true;
    }
    let bytes = dest.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    let mut i = 1;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b':' {
            return true;
        }
        if !(b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-')) {
            return false;
        }
        i += 1;
    }
    false
}

/// True when `path` has a markdown extension (`.md` / `.markdown`,
/// case-insensitive) — only markdown targets carry resolvable heading anchors.
fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
        .unwrap_or(false)
}

/// The markdown body of a file, with any leading frontmatter stripped so a
/// YAML comment (`# ...`) in frontmatter is not mistaken for a heading.
fn markdown_body(content: &str) -> &str {
    match crate::frontmatter::parse(content) {
        Ok(Some((_, body))) => body,
        _ => content,
    }
}

fn link_finding(
    path: &Path,
    content: &str,
    offset: usize,
    code: &str,
    summary: &str,
    message: &str,
    dest: &str,
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
            label: "link".into(),
            content: dest.into(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_external_classifies_schemes() {
        assert!(is_external("https://example.com/a.md"));
        assert!(is_external("mailto:x@y.z"));
        assert!(is_external("//cdn.example.com/x"));
        assert!(!is_external("docs/a.md"));
        assert!(!is_external("../b.md"));
        assert!(!is_external("#fragment"));
        assert!(!is_external("a.md#frag"));
    }

    #[test]
    fn split_fragment_splits_on_first_hash() {
        assert_eq!(split_fragment("a.md#sec"), ("a.md", Some("sec")));
        assert_eq!(split_fragment("#sec"), ("", Some("sec")));
        assert_eq!(split_fragment("a.md"), ("a.md", None));
        assert_eq!(split_fragment("a.md#"), ("a.md", Some("")));
    }

    #[test]
    fn percent_decode_decodes_valid_and_preserves_invalid() {
        assert_eq!(percent_decode("my%20doc.md"), "my doc.md");
        assert_eq!(percent_decode("a%2Fb.md"), "a/b.md"); // encoded slash
        assert_eq!(percent_decode("plain.md"), "plain.md"); // no `%` -> borrowed
                                                            // Malformed sequences stay literal (no corruption, no false resolution).
        assert_eq!(percent_decode("bad%2.md"), "bad%2.md"); // one hex digit
        assert_eq!(percent_decode("bad%zz.md"), "bad%zz.md"); // non-hex
        assert_eq!(percent_decode("trailing%"), "trailing%"); // truncated
                                                              // Non-UTF8 decode stays literal.
        assert_eq!(percent_decode("%ff%fe"), "%ff%fe");
        // A real UTF-8 multibyte sequence decodes.
        assert_eq!(percent_decode("caf%C3%A9.md"), "café.md");
    }

    #[test]
    fn resolve_target_is_document_relative() {
        // `docs` + `api.md` -> `docs/api.md`
        assert_eq!(
            resolve_target(Path::new("docs"), "api.md", false)
                .unwrap()
                .as_path(),
            Path::new("docs/api.md")
        );
        // `docs` + `../README.md` -> `README.md` (in-tree)
        assert_eq!(
            resolve_target(Path::new("docs"), "../README.md", false)
                .unwrap()
                .as_path(),
            Path::new("README.md")
        );
        // climbing above the root escapes
        assert!(matches!(
            resolve_target(Path::new("docs"), "../../outside.md", false),
            Err(TargetViolation::Escapes)
        ));
        // absolute is refused distinctly
        assert!(matches!(
            resolve_target(Path::new("docs"), "/etc/passwd", false),
            Err(TargetViolation::Absolute)
        ));
    }

    // GH #37 (lychee-link-family-audit item 5): root-relative mode resolves a
    // leading-slash link from the project root while preserving confinement —
    // the opt-in analog of lychee's `--root-dir`. mdatron adds the confinement
    // layer the mature tools lack.
    #[test]
    fn resolve_target_root_relative_mode() {
        // `/docs/x.md` from a file in `notes/` -> `docs/x.md` (base ignored).
        assert_eq!(
            resolve_target(Path::new("notes"), "/docs/x.md", true)
                .unwrap()
                .as_path(),
            Path::new("docs/x.md")
        );
        // A `..` immediately climbing above the root is still an escape.
        assert!(matches!(
            resolve_target(Path::new("docs"), "/../secret.md", true),
            Err(TargetViolation::Escapes)
        ));
        // A `..` that stays in-tree collapses normally.
        assert_eq!(
            resolve_target(Path::new("docs"), "/a/../b.md", true)
                .unwrap()
                .as_path(),
            Path::new("b.md")
        );
        // Without the flag the SAME leading-slash path is refused (default).
        assert!(matches!(
            resolve_target(Path::new("docs"), "/docs/x.md", false),
            Err(TargetViolation::Absolute)
        ));
        // A document-relative path is unaffected by the flag.
        assert_eq!(
            resolve_target(Path::new("notes"), "peer.md", true)
                .unwrap()
                .as_path(),
            Path::new("notes/peer.md")
        );
    }
}
