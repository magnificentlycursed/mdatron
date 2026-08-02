//! Shared markdown-scanning primitives for the body-reference check families.
//!
//! The reference-resolution families — citation (#86), link (#145), and
//! marker-line references (#147) — are instances of one pattern: scan a
//! governed file's prose body for tokens of a declared shape and resolve each
//! against an existing target. This module holds the mechanical primitives they
//! share, so the fenced-code discipline and the GitHub heading-slug algorithm
//! are defined once rather than copied per family:
//!
//! - [`non_fenced_lines`] — iterate the body's live (non-code-fence) lines with
//!   their byte offsets, so a link/marker *example* inside a ``` block is never
//!   resolved and each finding still lands on the right line.
//! - [`heading_slugs`] — the set of a body's heading anchors, for resolving a
//!   `#fragment` (link) or a `Provenance: <name>` reference (marker) against the
//!   headings a document actually declares.
//! - [`slugify`] — the GitHub heading-to-anchor slug algorithm.
//!
//! `pub(crate)`: engine-internal, shared across families, never a consumer
//! contract (mdatron is binary-first; the lib carries no API-stability promise).

use std::collections::HashSet;

/// Iterate the lines of `body` that are **not** inside a fenced code block,
/// yielding each line's byte offset within `body` and its content (trailing
/// newline trimmed). Fence-marker lines are themselves skipped. Shared by the
/// reference scans (link, marker) so a token inside a ``` example is never
/// resolved, and `split_inclusive` keeps the newline so the running byte cursor
/// (hence each finding's line number) stays exact.
pub(crate) fn non_fenced_lines(body: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut cursor = 0usize; // byte offset of the current line within `body`
    for raw_line in body.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\n', '\r']);
        let line_start = cursor;
        cursor += raw_line.len();
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
        out.push((line_start, line));
    }
    out
}

/// The set of GitHub heading-slugs of `body`, skipping fenced code blocks so a
/// `#`-comment in a shell example is not read as a heading. ATX headings only
/// (setext deferred).
pub(crate) fn heading_slugs(body: &str) -> HashSet<String> {
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
pub(crate) fn fence_marker(line: &str) -> Option<(char, usize)> {
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
pub(crate) fn atx_heading_text(line: &str) -> Option<&str> {
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
pub(crate) fn slugify(heading: &str) -> String {
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
    fn non_fenced_lines_skips_fences_and_tracks_offsets() {
        let body = "a\n```\nb\n```\nc\n";
        let lines = non_fenced_lines(body);
        let texts: Vec<_> = lines.iter().map(|(_, l)| *l).collect();
        assert_eq!(texts, vec!["a", "c"], "fenced `b` is skipped");
        // The offsets point at each line's start within `body`.
        assert_eq!(&body[lines[0].0..lines[0].0 + 1], "a");
        assert_eq!(&body[lines[1].0..lines[1].0 + 1], "c");
    }
}
