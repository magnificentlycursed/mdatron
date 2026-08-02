//! Link family (#145; `DESIGN.md` § check families): inline body links in
//! governed markdown are resolved against the working tree — a link to a file
//! that is not there, or a fragment that matches no heading, is a dead link.
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
//! Findings: a link whose in-tree target does not exist is `MDATRON-E0110`
//! (dead-link-target); an existing markdown target (or the same document) whose
//! `#fragment` matches no heading is `MDATRON-E0111` (dead-anchor). Anchors are
//! matched with the GitHub heading-slug algorithm.
//!
//! Deferred (first cut): reference-style links (`[text][ref]`), autolinks,
//! image links (`![alt](src)`), setext headings, inline-code-span links,
//! percent-decoding, and duplicate-heading `-N` disambiguation. External links
//! (any URL scheme, or protocol-relative `//host`) are out of scope by design —
//! the engine does not reach the network.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::confine::{confine_lexically, open_confined, OpenViolation};
use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};

/// Scan one opted-in file's body for inline links and resolve each against the
/// working tree. `content` is the whole file; `body_offset` is where the prose
/// body begins (frontmatter is not link-scanned). Signature mirrors
/// [`crate::cite::check_file`].
pub fn check_file(
    project_root: &Path,
    path: &Path,
    content: &str,
    body_offset: usize,
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

    // Line-oriented scan so fenced code blocks (link EXAMPLES, not live links)
    // are skipped and each finding gets an accurate line. `split_inclusive`
    // keeps the newline so the running byte cursor stays exact.
    let mut fence: Option<(char, usize)> = None;
    let mut cursor = 0usize; // byte offset of the current line within `body`
    for raw_line in body.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\n', '\r']);
        let line_start = cursor;
        cursor += raw_line.len();

        // Fenced code toggles: a line whose leading run is >= 3 of ` or ~.
        if let Some(marker) = fence_marker(line) {
            match fence {
                None => fence = Some(marker),
                Some((fc, flen)) => {
                    if marker.0 == fc && marker.1 >= flen {
                        fence = None;
                    }
                }
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }

        for link in inline_links(line) {
            // `at` is the absolute byte offset of the link in `content`, used
            // for the 1-based line number (matches cite's convention).
            let at = body_offset + line_start + link.start;
            resolve_link(
                project_root,
                path,
                content,
                at,
                base_dir,
                link.dest,
                &own_slugs,
                &mut target_slugs,
                findings,
            );
        }
    }
}

/// One inline link found on a line: its destination text (fragment included)
/// and the byte offset at which the `[` sits within the line.
struct InlineLink<'a> {
    dest: &'a str,
    start: usize,
}

/// Extract inline `[text](dest)` links from a single line. Image links
/// (`![alt](src)`) are skipped. Reference-style and shortcut links do not match
/// (no `](` … `)` run) and are thereby deferred. Conservative: link text and
/// destination do not span the `]`/`)` that close them, so a destination
/// containing `)` truncates — rare for file paths, and never a false escape.
fn inline_links(line: &str) -> Vec<InlineLink<'_>> {
    let detector = regex_lite::Regex::new(r"\[[^\]\n]*\]\(([^)\n]*)\)")
        .expect("engine inline-link detector compiles");
    let mut out = Vec::new();
    for caps in detector.captures_iter(line) {
        let whole = caps.get(0).expect("match exists");
        // Image link: the `[` is immediately preceded by `!`.
        if line[..whole.start()].ends_with('!') {
            continue;
        }
        let inner = caps.get(1).expect("dest group").as_str().trim();
        // Angle-bracket destination `<url>` (may hold spaces); otherwise the
        // destination is the text up to the first whitespace (a trailing
        // `"title"` is dropped).
        let dest = if inner.len() >= 2 && inner.starts_with('<') && inner.ends_with('>') {
            inner[1..inner.len() - 1].trim()
        } else {
            inner.split_whitespace().next().unwrap_or("")
        };
        if dest.is_empty() {
            continue;
        }
        out.push(InlineLink {
            dest,
            start: whole.start(),
        });
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn resolve_link(
    project_root: &Path,
    path: &Path,
    content: &str,
    at: usize,
    base_dir: &Path,
    dest: &str,
    own_slugs: &HashSet<String>,
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
            if !frag.is_empty() && !own_slugs.contains(&slugify(frag)) {
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

    // Document-relative confinement decided on the RESOLVED path.
    let confined = match resolve_target(base_dir, path_part) {
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

    match open_confined(project_root, &confined) {
        Ok(mut handle) => {
            // Target exists. If the link carries a fragment and the target is a
            // markdown file, the fragment must match one of its headings.
            let Some(frag) = anchor else { return };
            if frag.is_empty() || !is_markdown(confined.as_path()) {
                return;
            }
            let key = confined.as_path().to_path_buf();
            if !target_slugs.contains_key(&key) {
                let mut buf = String::new();
                let slugs = if handle.read_to_string(&mut buf).is_ok() {
                    Some(heading_slugs(markdown_body(&buf)))
                } else {
                    // Non-UTF8 / unreadable markdown target: existence is
                    // verified, the fragment is not resolved (not flagged).
                    None
                };
                target_slugs.insert(key.clone(), slugs);
            }
            if let Some(Some(slugs)) = target_slugs.get(&key) {
                if !slugs.contains(&slugify(frag)) {
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
        Err(OpenViolation::Symlink { .. }) => {
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
        Err(OpenViolation::Io(_)) => {
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
            Component::Prefix(_) | Component::RootDir => return Err(TargetViolation::Absolute),
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

/// The set of GitHub heading-slugs of `body`, skipping fenced code blocks so a
/// `#`-comment in a shell example is not read as a heading. ATX headings only
/// (setext deferred).
fn heading_slugs(body: &str) -> HashSet<String> {
    let mut slugs = HashSet::new();
    let mut fence: Option<(char, usize)> = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(marker) = fence_marker(trimmed) {
            match fence {
                None => fence = Some(marker),
                Some((fc, flen)) => {
                    if marker.0 == fc && marker.1 >= flen {
                        fence = None;
                    }
                }
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        if let Some(text) = atx_heading_text(trimmed) {
            slugs.insert(slugify(text));
        }
    }
    slugs
}

/// If `line` opens or closes a fenced code block, return its `(fence char, run
/// length)`. A fence is a leading run of at least three backticks or tildes.
fn fence_marker(line: &str) -> Option<(char, usize)> {
    let first = line.chars().next()?;
    if first != '`' && first != '~' {
        return None;
    }
    let run = line.chars().take_while(|&c| c == first).count();
    (run >= 3).then_some((first, run))
}

/// The text of an ATX heading (`#`…`######` followed by a space or end of
/// line), with any closing `#` sequence trimmed, or `None` if `line` is not one.
/// `#foo` (no space) is a paragraph, not a heading.
fn atx_heading_text(line: &str) -> Option<&str> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &line[hashes..];
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    // Trim a closing `#` sequence (`## Foo ##` -> `Foo`); slugify would drop
    // the `#`s anyway, but trimming keeps the surrounding hyphen from leaking.
    Some(rest.trim().trim_end_matches('#').trim_end())
}

/// GitHub's heading-to-anchor slug: lowercase, drop every character that is not
/// alphanumeric / `_` / `-`, and turn each space into a hyphen. (Duplicate
/// headings' `-1`/`-2` disambiguation is deferred.)
fn slugify(heading: &str) -> String {
    let mut out = String::with_capacity(heading.len());
    for ch in heading.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else if ch == ' ' {
            out.push('-');
        }
    }
    out
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
    fn slugify_matches_github_shape() {
        assert_eq!(slugify("Five check families"), "five-check-families");
        assert_eq!(slugify("Section 1.2"), "section-12");
        assert_eq!(slugify("Foo & Bar"), "foo--bar");
        assert_eq!(slugify("snake_case-kept"), "snake_case-kept");
        assert_eq!(slugify("v0.6.0 release"), "v060-release");
    }

    #[test]
    fn atx_heading_requires_space() {
        assert_eq!(atx_heading_text("## Real Heading"), Some("Real Heading"));
        assert_eq!(atx_heading_text("### Foo ###"), Some("Foo"));
        assert_eq!(atx_heading_text("#hashtag"), None);
        assert_eq!(atx_heading_text("####### too deep"), None);
        assert_eq!(atx_heading_text("not a heading"), None);
    }

    #[test]
    fn fenced_headings_are_not_collected() {
        let body = "# Real\n\n```sh\n# not a heading\n```\n\n## Also Real\n";
        let slugs = heading_slugs(body);
        assert!(slugs.contains("real"));
        assert!(slugs.contains("also-real"));
        assert!(!slugs.contains("not-a-heading"));
    }

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
    fn inline_links_skip_images_and_capture_dest() {
        let links = inline_links("see [a](x.md) not ![img](y.png) and [b](z.md#f)");
        let dests: Vec<_> = links.iter().map(|l| l.dest).collect();
        assert_eq!(dests, vec!["x.md", "z.md#f"]);
    }

    #[test]
    fn resolve_target_is_document_relative() {
        // `docs` + `api.md` -> `docs/api.md`
        assert_eq!(
            resolve_target(Path::new("docs"), "api.md")
                .unwrap()
                .as_path(),
            Path::new("docs/api.md")
        );
        // `docs` + `../README.md` -> `README.md` (in-tree)
        assert_eq!(
            resolve_target(Path::new("docs"), "../README.md")
                .unwrap()
                .as_path(),
            Path::new("README.md")
        );
        // climbing above the root escapes
        assert!(matches!(
            resolve_target(Path::new("docs"), "../../outside.md"),
            Err(TargetViolation::Escapes)
        ));
        // absolute is refused distinctly
        assert!(matches!(
            resolve_target(Path::new("docs"), "/etc/passwd"),
            Err(TargetViolation::Absolute)
        ));
    }
}
