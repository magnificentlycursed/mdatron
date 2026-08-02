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

/// An ATX heading's `(level, text)` — the number of leading `#` (1–6, followed
/// by a space or end of line) and the trimmed heading text (any closing `#`
/// sequence removed), or `None` if `line` is not an ATX heading. `#foo` (no
/// space) is a paragraph, not a heading. Leading indentation is tolerated.
pub(crate) fn atx_heading(line: &str) -> Option<(usize, &str)> {
    let t = line.trim_start();
    let level = t.chars().take_while(|&c| c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &t[level..];
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    // Trim a closing `#` sequence (`## Foo ##` -> `Foo`); slugify would drop the
    // `#`s anyway, but trimming keeps the surrounding hyphen from leaking.
    Some((level, rest.trim().trim_end_matches('#').trim_end()))
}

/// The text of an ATX heading, or `None` if `line` is not one. Level-agnostic
/// wrapper over [`atx_heading`].
pub(crate) fn atx_heading_text(line: &str) -> Option<&str> {
    atx_heading(line).map(|(_, text)| text)
}

/// The leading `**bold**` name of a `- ` (or `*`/`+`) list item, or `None`.
/// Used by the marker family (#147) and the section-structural family (#157) to
/// pull an id out of a bullet's bold lead.
pub(crate) fn list_item_bold_name(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let rest = t
        .strip_prefix("- ")
        .or_else(|| t.strip_prefix("* "))
        .or_else(|| t.strip_prefix("+ "))?
        .trim_start();
    let after_open = rest.strip_prefix("**")?;
    let end = after_open.find("**")?;
    Some(&after_open[..end])
}

/// The heading-delimited span of `content` named by `heading_spec` (e.g.
/// `"## Decomposition (phase 1c)"` — matched by level AND text): the byte slice
/// from that heading's line through just before the next heading of the same or
/// higher level (heading line inclusive), or the rest of the document if none
/// follows. `None` if the heading is not found. Fence-aware (a `#` inside a code
/// fence is not a heading). Used by the pin family to hash one section (#146),
/// and the raw-span twin of the marker family's section gating.
pub(crate) fn section_span<'a>(content: &'a str, heading_spec: &str) -> Option<&'a str> {
    let (want_level, want_text) = atx_heading(heading_spec)?;
    let mut start: Option<usize> = None;
    for (offset, line) in non_fenced_lines(content) {
        if let Some((level, text)) = atx_heading(line) {
            match start {
                None => {
                    if level == want_level && text == want_text {
                        start = Some(offset); // include the heading line itself
                    }
                }
                Some(s) => {
                    if level <= want_level {
                        return Some(&content[s..offset]); // next same/higher heading ends it
                    }
                }
            }
        }
    }
    start.map(|s| &content[s..])
}

/// The byte ranges of `line` covered by inline code spans (backtick-delimited),
/// including the delimiters. A code span opens with a run of N backticks and
/// closes at the next run of **exactly** N backticks (CommonMark); an opener
/// with no matching closer is literal text, not a span. Shared by the
/// body-token scanners (link, marker, code-catalog) so a token shown inside
/// `` `code` `` is an EXAMPLE, not a live reference (#154). Line-scoped: a code
/// span spanning multiple lines is not tracked (the scanners are line-based).
pub(crate) fn inline_code_ranges(line: &str) -> Vec<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        // Opening backtick run of length n.
        let open = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        let n = i - open;
        // Find a closing run of exactly n backticks.
        let mut j = i;
        let mut closed = None;
        while j < bytes.len() {
            if bytes[j] != b'`' {
                j += 1;
                continue;
            }
            let run = j;
            while j < bytes.len() && bytes[j] == b'`' {
                j += 1;
            }
            if j - run == n {
                closed = Some(j); // end (exclusive) of the whole span
                break;
            }
        }
        match closed {
            Some(end) => {
                ranges.push((open, end));
                i = end;
            }
            // No closer: the run is literal; resume just past it.
            None => i = open + n,
        }
    }
    ranges
}

/// Whether byte offset `pos` falls inside any of `ranges` (an inline code span).
pub(crate) fn in_code_span(ranges: &[(usize, usize)], pos: usize) -> bool {
    ranges.iter().any(|&(s, e)| pos >= s && pos < e)
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
    fn list_item_bold_name_extracts_leading_bold() {
        assert_eq!(
            list_item_bold_name("- **Slice 1 — self gov.** detail"),
            Some("Slice 1 — self gov.")
        );
        assert_eq!(list_item_bold_name("- plain item"), None);
        assert_eq!(list_item_bold_name("not a list item"), None);
        assert_eq!(
            list_item_bold_name("  * **Indented.** x"),
            Some("Indented.")
        );
    }

    #[test]
    fn atx_heading_reports_level_and_text() {
        assert_eq!(
            atx_heading("## Decomposition (phase 1c)"),
            Some((2, "Decomposition (phase 1c)"))
        );
        assert_eq!(atx_heading("# Title"), Some((1, "Title")));
        assert_eq!(atx_heading("- **not a heading**"), None);
        assert_eq!(atx_heading("#nospace"), None);
    }

    #[test]
    fn section_span_is_heading_inclusive_until_same_or_higher() {
        let doc = "# Top\n\n## A\n\napple\n\n### A.1\n\nsub\n\n## B\n\nbee\n";
        let a = section_span(doc, "## A").unwrap();
        assert!(a.starts_with("## A"), "span includes the heading line");
        assert!(a.contains("apple") && a.contains("### A.1") && a.contains("sub"));
        assert!(
            !a.contains("## B") && !a.contains("bee"),
            "next H2 ends the span"
        );
        // A heading that does not exist has no span.
        assert!(section_span(doc, "## Nope").is_none());
        // The last section runs to end of document.
        let b = section_span(doc, "## B").unwrap();
        assert!(b.starts_with("## B") && b.contains("bee"));
    }

    #[test]
    fn inline_code_ranges_covers_backtick_spans() {
        // `[x](y)` sits inside a code span; the plain link does not.
        let line = "see `[x](y)` and [real](z.md) here";
        let ranges = inline_code_ranges(line);
        let code_at = line.find("[x]").unwrap();
        let real_at = line.find("[real]").unwrap();
        assert!(
            in_code_span(&ranges, code_at),
            "the link in backticks is masked"
        );
        assert!(
            !in_code_span(&ranges, real_at),
            "the plain link is not masked"
        );
        // A lone backtick with no closer is literal (no span).
        assert!(inline_code_ranges("a ` lone backtick").is_empty());
        // A double-backtick span closes only on a matching double run, so an
        // inner single backtick stays inside it.
        let dbl = "``a ` b``c";
        let r = inline_code_ranges(dbl);
        assert!(
            in_code_span(&r, 4),
            "inner single backtick is inside the span"
        );
        assert!(
            !in_code_span(&r, 9),
            "the `c` after the closing run is outside"
        );
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
