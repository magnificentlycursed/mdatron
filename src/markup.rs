//! Shared markdown-scanning primitives for the body-scanning check families.
//!
//! Several families scan a governed file's prose body for tokens of a declared
//! shape — citation (#86), link (#145), marker-line references (#147), pin
//! sections (#146), the code-catalog (#148), and the vocabulary register/coinage
//! checks. This module holds the mechanical primitives they share, so the
//! fenced-code discipline, the GitHub heading-slug algorithm, and the inline-code
//! masking are defined once rather than copied per family:
//!
//! - [`non_fenced_lines`] — the body's live (non-code-fence) lines with byte
//!   offsets, so a token inside a ``` block is an example, not a live reference.
//! - [`fence_marker`] — recognize a fenced-code toggle line (CommonMark 0–3
//!   indent rule).
//! - [`fenced_ranges`] — the byte ranges covered by fenced blocks, for a
//!   scanner that matches over the whole body at once (the vocabulary family).
//! - [`atx_heading`] — parse an ATX heading's level+text.
//! - [`ElementClass`] — the one by-name element vocabulary (`heading`, `h1`…`h6`,
//!   `list-item-bold-name`) the marker and section families resolve against.
//! - [`section_span`] — the byte span of one heading-delimited section (pin #146).
//! - [`heading_slugs`] / [`slugify`] — a body's heading anchors (ATX + setext,
//!   with GitHub `-N` disambiguation and explicit HTML anchors, via a CommonMark
//!   parse, #155), and the GitHub heading-to-anchor slug algorithm, for resolving
//!   `#fragment` / by-name refs.
//! - [`body_links`] — every link + image destination (inline, reference-style,
//!   image) via a CommonMark parse (#155); code-span/fence masking is structural
//!   (a destination inside `` `code` `` or a ``` fence is never a link event).
//! - [`inline_code_ranges`] / [`body_inline_code_ranges`] / [`in_code_span`] —
//!   mask inline `` `code` `` spans so a vocabulary term (#158) shown in backticks
//!   is not treated as a live use. (A code-catalog citation is the deliberate
//!   exception — a backticked code stays a real reference. The link family no
//!   longer needs this: `body_links` masks code structurally, #155.)
//!
//! `pub(crate)`: engine-internal, shared across families, never a consumer
//! contract (mdatron is binary-first; the lib carries no API-stability promise).

use std::collections::{HashMap, HashSet};

use serde::Deserialize;

/// The by-name element class a marker rule or a section rule resolves against
/// (#147 / #157). One enum, one value set, shared by marker `element`,
/// section-count `element`, and section-disjoint `element` (unified in 0.7.0
/// per the ontology review, #204 D2-1 — the same idea had three spellings):
///
/// - `heading` — an ATX heading of ANY level, named by its text.
/// - `h1` … `h6` — an ATX heading of exactly that level.
/// - `list-item-bold-name` — the leading `**bold**` name of a `- ` list item
///   (vsdd's live shape: `- **Slice 1 — …** …`, referenced by that name).
/// - `list-item` — any list item (`- `, `* `, `+ `, or an ordinal `1. `/`1) `),
///   named by its text after the marker (#213).
/// - `blockquote` — a `>` line, named by its text after the marker (#214).
/// - `line` — any non-blank line, named by the whole line (#217/#218).
///
/// All classes are LINE-based: an element is the line that opens it (a list
/// item's continuation lines are not part of the element), and a line inside
/// a fenced code block is never an element.
///
/// The retired spellings `h3-heading` and `bullet-lead` (the 0.6.0
/// section-disjoint `id_from` values) are accepted as aliases per
/// `DESIGN.md` § Input-field renames are aliased and ledgered. `frontmatter-key` stays reserved for a later
/// cut.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ElementClass {
    Heading,
    H1,
    H2,
    #[serde(alias = "h3-heading")]
    H3,
    H4,
    H5,
    H6,
    #[serde(alias = "bullet-lead")]
    ListItemBoldName,
    ListItem,
    Blockquote,
    Line,
}

impl ElementClass {
    /// The element name `line` carries for this class, or `None` when the line
    /// is not an element of the class: the heading text for the heading
    /// classes (any level for `heading`, the exact level for `h1`…`h6`), the
    /// bold lead for `list-item-bold-name`, the item text for `list-item`, the
    /// quoted text for `blockquote`, the whole line for `line` (never a blank
    /// one). Fence discipline is the caller's (feed it [`non_fenced_lines`]).
    pub(crate) fn name_in(self, line: &str) -> Option<&str> {
        match self {
            Self::Heading => atx_heading(line).map(|(_, t)| t),
            Self::ListItemBoldName => list_item_bold_name(line),
            Self::ListItem => list_item_text(line),
            Self::Blockquote => blockquote_text(line),
            Self::Line => (!line.trim().is_empty()).then_some(line),
            exact => atx_heading(line)
                .filter(|(l, _)| Some(*l) == exact.heading_level())
                .map(|(_, t)| t),
        }
    }

    /// The exact heading level of `h1`…`h6`; `None` for the classes that are
    /// not level-specific.
    fn heading_level(self) -> Option<usize> {
        match self {
            Self::H1 => Some(1),
            Self::H2 => Some(2),
            Self::H3 => Some(3),
            Self::H4 => Some(4),
            Self::H5 => Some(5),
            Self::H6 => Some(6),
            Self::Heading
            | Self::ListItemBoldName
            | Self::ListItem
            | Self::Blockquote
            | Self::Line => None,
        }
    }

    /// The adopter-facing spelling — the `element:` value — for messages.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Heading => "heading",
            Self::H1 => "h1",
            Self::H2 => "h2",
            Self::H3 => "h3",
            Self::H4 => "h4",
            Self::H5 => "h5",
            Self::H6 => "h6",
            Self::ListItemBoldName => "list-item-bold-name",
            Self::ListItem => "list-item",
            Self::Blockquote => "blockquote",
            Self::Line => "line",
        }
    }
}

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

/// The CommonMark parser every body-scanning pass uses, with GitHub's footnote
/// syntax (#243): without it a footnote definition whose body is one token —
/// `[^1]: [fn](https://…)`, `[^1]: notes.md` — parsed as a REFERENCE
/// definition, and each `[^1]` became a link to that text (a false `E0110`).
/// With it the footnote body is ordinary inline content, so a link inside it
/// is found like any other. ONE constructor, so the slug and link passes never
/// disagree about what a footnote is.
fn parser(body: &str) -> pulldown_cmark::Parser<'_> {
    pulldown_cmark::Parser::new_ext(body, pulldown_cmark::Options::ENABLE_FOOTNOTES)
}

/// The set of GitHub heading-anchor slugs of `body`, via a CommonMark parse
/// (#155). Covers ATX **and setext** headings (both are heading events), applies
/// GitHub's duplicate-heading `-N` disambiguation (the first "Foo" is `foo`, the
/// second `foo-1`, the third `foo-2`), and adds explicit HTML anchors
/// (`<a name>`/`id=`). A `#`-comment inside a fenced or indented code block is not
/// a heading, so it never enters the set — the parser models code structurally
/// rather than the old fence-line heuristic. Adding an anchor only ever REMOVES a
/// dead-anchor false positive, so the set is deliberately generous.
pub(crate) fn heading_slugs(body: &str) -> HashSet<String> {
    use pulldown_cmark::{Event, Tag, TagEnd};
    let mut slugs = HashSet::new();
    let mut counts: HashMap<String, u32> = HashMap::new();
    let mut in_heading = false;
    let mut text = String::new();
    // The heading text as GitHub renders it with its footnote markers (#243
    // review): a reference renders as its ordinal (`# Title[^1]` → `title1`).
    // The bare form and a label form are kept too — the set is generous.
    let mut rendered = String::new();
    let mut labelled = String::new();
    // GitHub numbers footnotes by first reference; each further reference to
    // the same label gets `fnref-<label>-<k>`.
    let mut ordinals: HashMap<String, usize> = HashMap::new();
    let mut ref_counts: HashMap<String, usize> = HashMap::new();
    for event in parser(body) {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                in_heading = true;
                text.clear();
                rendered.clear();
                labelled.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                let base = slugify(&text);
                if !base.is_empty() {
                    // GitHub gives the first occurrence the bare slug and appends
                    // `-1`, `-2`, … to each subsequent repeat of the same slug.
                    let n = counts.entry(base.clone()).or_insert(0);
                    let suffix = if *n == 0 {
                        String::new()
                    } else {
                        format!("-{n}")
                    };
                    *n += 1;
                    for variant in [&text, &rendered, &labelled] {
                        let slug = slugify(variant);
                        if !slug.is_empty() {
                            slugs.insert(format!("{slug}{suffix}"));
                        }
                    }
                }
            }
            // Heading text and inline `code` within it both contribute to the slug.
            Event::Text(t) | Event::Code(t) if in_heading => {
                text.push_str(&t);
                rendered.push_str(&t);
                labelled.push_str(&t);
            }
            // GitHub's footnote anchors (#243 review): the definition
            // (`fn-<label>`), each reference (`fnref-<label>`, `-2`, …) and the
            // footnotes section, with and without the `user-content-` prefix.
            Event::FootnoteReference(label) => {
                let next = ordinals.len() + 1;
                let ordinal = *ordinals.entry(label.to_string()).or_insert(next);
                let k = ref_counts.entry(label.to_string()).or_insert(0);
                *k += 1;
                let id = if *k == 1 {
                    format!("fnref-{label}")
                } else {
                    format!("fnref-{label}-{k}")
                };
                slugs.insert(format!("user-content-{id}"));
                slugs.insert(id);
                if in_heading {
                    rendered.push_str(&ordinal.to_string());
                    labelled.push_str(&label);
                }
            }
            Event::Start(Tag::FootnoteDefinition(label)) => {
                for id in [format!("fn-{label}"), "footnote-label".to_string()] {
                    slugs.insert(format!("user-content-{id}"));
                    slugs.insert(id);
                }
            }
            // Explicit HTML anchors are valid targets wherever they appear.
            Event::Html(h) | Event::InlineHtml(h) => insert_html_anchors(&h, &mut slugs),
            _ => {}
        }
    }
    slugs
}

/// Insert explicit HTML anchor targets from an HTML fragment into `slugs` (#155
/// gap 3): a `<a name="x">`, `<a id="x">`, or any element's `id="x"` is a valid
/// `#x` target on GitHub. Both the raw id and its slugified form are added, so a
/// non-slug-shaped id still resolves. The attribute scan is a linear-time regex
/// over the engine-authored pattern (not an adopter-supplied one), so it carries
/// no ReDoS surface (DESIGN § Project declarations (linear-time pattern engines)).
fn insert_html_anchors(html: &str, slugs: &mut HashSet<String>) {
    // HTML comments are not rendered and carry no anchors — strip them first so
    // an `id=`/`name=` inside `<!-- … -->` does not register a phantom target.
    let scanned = strip_html_comments(html);
    // A `name=`/`id=` attribute (boundary-anchored so `grid=` does not match)
    // with a single- or double-quoted value. Compiled once (GH #48 lane G):
    // this runs per HTML event per anchor-bearing file — a per-call compile
    // taxed the hot path (the cite DETECTOR's idiom).
    #[allow(
        clippy::expect_used,
        reason = "an engine literal; a compile failure is a build defect, pinned by the unit tests"
    )]
    static ANCHOR_ATTR: std::sync::LazyLock<regex_lite::Regex> = std::sync::LazyLock::new(|| {
        regex_lite::Regex::new(r#"(?:^|[\s"'])(?:name|id)\s*=\s*["']([^"']+)["']"#)
            .expect("engine html-anchor detector compiles")
    });
    for caps in ANCHOR_ATTR.captures_iter(&scanned) {
        let Some(id) = caps.get(1) else { continue };
        let id = id.as_str();
        slugs.insert(id.to_string());
        let slug = slugify(id);
        if !slug.is_empty() {
            slugs.insert(slug);
        }
    }
}

/// Remove `<!-- … -->` comment spans from an HTML fragment (linear scan). An
/// unterminated comment drops the remainder, matching how a renderer treats it.
fn strip_html_comments(html: &str) -> std::borrow::Cow<'_, str> {
    if !html.contains("<!--") {
        return std::borrow::Cow::Borrowed(html);
    }
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start + 4..].find("-->") {
            Some(end) => rest = &rest[start + 4 + end + 3..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    std::borrow::Cow::Owned(out)
}

/// A link or image destination found by [`body_links`], with the byte offset of
/// its opening delimiter within the body (for the finding location).
pub(crate) struct BodyLink {
    pub dest: String,
    pub offset: usize,
}

/// Every resolvable link and image destination in `body`, via a CommonMark parse
/// (#155). Covers inline `[t](d)`, reference `[t][r]` / collapsed / shortcut, and
/// image `![alt](src)` links uniformly — pulldown-cmark pairs reference
/// definitions and yields each destination once, with its byte offset. A
/// destination inside an inline code span or a fenced/indented code block never
/// surfaces as a link event, so code examples are excluded **structurally** —
/// this retires the hand-rolled fence + inline-code masking and its #154 defect.
/// A URL autolink (`<https://…>`) carries its scheme; an EMAIL autolink
/// (`<x@y.z>`) is yielded as the bare address — the HTML writer is what adds
/// `mailto:` — so it is given its scheme here, and the caller's external
/// filter sees every autolink as the absolute destination it renders as.
pub(crate) fn body_links(body: &str) -> Vec<BodyLink> {
    use pulldown_cmark::{Event, LinkType, Tag};
    let mut out = Vec::new();
    for (event, range) in parser(body).into_offset_iter() {
        let dest = match event {
            Event::Start(Tag::Link {
                link_type: LinkType::Email,
                dest_url,
                ..
            }) => format!("mailto:{dest_url}"),
            Event::Start(Tag::Link { dest_url, .. })
            | Event::Start(Tag::Image { dest_url, .. }) => dest_url.to_string(),
            _ => continue,
        };
        out.push(BodyLink {
            dest,
            offset: range.start,
        });
    }
    out
}

/// Every `@path` import in `body` (#226): Claude Code's CLAUDE.md / AGENTS.md
/// import syntax, as its documentation states it — the path runs from the `@`
/// to the first whitespace; an `@` is an import only at the start of a line or
/// after whitespace (so `a@b.md` and a quoted `"@x"` are not); and "import
/// parsing skips Markdown code spans and fenced code blocks". Returned with the
/// byte offset of the `@` within `body`, like [`body_links`]. Everything after
/// the `@` is the path, trailing punctuation included: that is what Claude Code
/// would try to open. A backslash before a space keeps the space in the path
/// ("To import a file whose path contains spaces, put a backslash before each
/// space"); the backslash itself is not part of the path.
pub(crate) fn body_imports(body: &str) -> Vec<BodyLink> {
    let mut out = Vec::new();
    for (line_start, line) in non_fenced_lines(body) {
        let code = inline_code_ranges(line);
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'@' {
                i += 1;
                continue;
            }
            let at_boundary = i == 0 || line[..i].ends_with([' ', '\t']);
            if !at_boundary || in_code_span(&code, i) {
                i += 1;
                continue;
            }
            let rest = &line[i + 1..];
            let (dest, consumed) = import_path(rest);
            if !dest.is_empty() {
                out.push(BodyLink {
                    dest,
                    offset: line_start + i,
                });
            }
            i += 1 + consumed;
        }
    }
    out
}

/// The import path at the start of `rest` (the text after an `@`) and the
/// bytes it occupies: up to the first whitespace, where a `\ ` is an escaped
/// space that stays in the path without its backslash.
fn import_path(rest: &str) -> (String, usize) {
    let mut dest = String::new();
    let mut chars = rest.char_indices().peekable();
    let mut consumed = rest.len();
    while let Some((j, c)) = chars.next() {
        if c == '\\' && matches!(chars.peek(), Some((_, ' '))) {
            chars.next();
            dest.push(' ');
            continue;
        }
        if c.is_whitespace() {
            consumed = j;
            break;
        }
        dest.push(c);
    }
    (dest, consumed)
}

/// If `line` opens or closes a fenced code block, return its `(fence char, run
/// length)`. A fence is a run of at least three backticks or tildes indented by
/// at most three spaces (CommonMark's 0–3 rule, GH #48 finding 5's sibling
/// minor): one to three leading spaces still open/close a fence, while four or
/// more — or a leading tab, which reaches the 4-space code indent — make the
/// line indented code, not a fence.
pub(crate) fn fence_marker(line: &str) -> Option<(char, usize)> {
    let mut rest = line;
    let mut indent = 0usize;
    while let Some(r) = rest.strip_prefix(' ') {
        indent += 1;
        if indent > 3 {
            return None;
        }
        rest = r;
    }
    let first = rest.chars().next()?;
    if first != '`' && first != '~' {
        return None;
    }
    let run = rest.chars().take_while(|&c| c == first).count();
    (run >= 3).then_some((first, run))
}

/// The byte ranges of `content` covered by fenced code blocks — each range runs
/// from the start of the opening fence-marker line through the end of the
/// closing marker line (or end of input when unclosed), so a match starting
/// anywhere inside the block, markers included, is inside a range. Fence
/// recognition is [`fence_marker`] (CommonMark 0–3 indent rule), the same logic
/// [`non_fenced_lines`] applies — this is the whole-body companion for a scanner
/// that regex-matches over the raw body at once (the vocabulary family's
/// `find_iter`, GH #48 finding 6) and so cannot use the line iterator without
/// losing legitimate multi-line matches.
pub(crate) fn fenced_ranges(content: &str) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut open_at = 0usize;
    let mut cursor = 0usize;
    for raw_line in content.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\n', '\r']);
        let line_start = cursor;
        cursor += raw_line.len();
        if let Some(marker) = fence_marker(line) {
            match fence {
                None => {
                    fence = Some(marker);
                    open_at = line_start;
                }
                Some((fc, flen)) => {
                    if marker.0 == fc && marker.1 >= flen {
                        fence = None;
                        out.push(open_at..cursor);
                    }
                }
            }
        }
    }
    if fence.is_some() {
        out.push(open_at..content.len());
    }
    out
}

/// An ATX heading's `(level, text)` — the number of leading `#` (1–6, followed
/// by a space or end of line) and the trimmed heading text (any closing `#`
/// sequence removed), or `None` if `line` is not an ATX heading. `#foo` (no
/// space) is a paragraph, not a heading. Leading indentation of up to three
/// spaces is tolerated (CommonMark's 0–3 rule, GH #48 finding 5); four or more
/// spaces — or any leading tab, which reaches the 4-space code indent — make
/// the line indented CODE, never a heading, so `    # install deps` inside a
/// pinned section cannot silently truncate the section's span.
pub(crate) fn atx_heading(line: &str) -> Option<(usize, &str)> {
    let mut indent = 0usize;
    for c in line.chars() {
        match c {
            '\t' => return None,
            ' ' => indent += 1,
            _ => break,
        }
        if indent >= 4 {
            return None;
        }
    }
    // Slice at the counted SPACE indent — never trim_start(), which swallows
    // any Unicode whitespace and would let a form-feed/NBSP-prefixed `#` line
    // parse as a heading (GH #48 lanes-B-E review F5: only 0-3 literal spaces
    // are heading indentation; anything else before `#` makes paragraph text).
    let t = &line[indent..];
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

/// The text of a list item after its marker — `-`, `*`, `+`, or an ordinal
/// (`1.` / `1)`, up to nine digits as CommonMark allows), followed by a space
/// or a tab — or `None` when the line does not open a list item (#213).
///
/// Recognised by the line's prefix alone, so three things differ from a
/// CommonMark parse and are stated in `docs/inputs.md`: any indentation of
/// spaces or tabs is accepted (a nested item is an item, and so is an
/// item-shaped line of indented code); a marker with nothing after it is not
/// an item; a thematic break written with spaces (`* * *`, `- - -`) is NOT an
/// item, though it opens with a marker.
pub(crate) fn list_item_text(line: &str) -> Option<&str> {
    // Spaces and tabs only: `trim_start` would also strip a no-break space,
    // which is text, not indentation.
    let t = line.trim_start_matches([' ', '\t']);
    if is_thematic_break(t) {
        return None;
    }
    let after_marker = match t.as_bytes().first()? {
        b'-' | b'*' | b'+' => &t[1..],
        _ => {
            let digits = t.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 || digits > 9 {
                return None;
            }
            t[digits..]
                .strip_prefix('.')
                .or_else(|| t[digits..].strip_prefix(')'))?
        }
    };
    after_marker
        .strip_prefix([' ', '\t'])
        .map(|rest| rest.trim())
}

/// A CommonMark thematic break: three or more of one of `-`, `*`, `_`, with
/// only spaces or tabs between and around them.
fn is_thematic_break(line: &str) -> bool {
    let mut marks = line.chars().filter(|c| !matches!(c, ' ' | '\t'));
    let Some(first) = marks.next() else {
        return false;
    };
    matches!(first, '-' | '*' | '_') && marks.clone().all(|c| c == first) && marks.count() >= 2
}

/// The text of a blockquote line after its `>` marker, or `None`. The marker
/// may be indented by up to three spaces (CommonMark's 0–3 rule); four or more,
/// or a tab, make the line indented code (#214).
pub(crate) fn blockquote_text(line: &str) -> Option<&str> {
    let indent = line.bytes().take_while(|b| *b == b' ').count();
    if indent > 3 {
        return None;
    }
    line[indent..].strip_prefix('>').map(str::trim)
}

/// The heading-delimited span of `content` named by `heading_spec` (e.g.
/// `"## Requirements"` — matched by level AND text): the byte slice
/// from that heading's line through just before the next heading of the same or
/// higher level (heading line inclusive), or the rest of the document if none
/// follows. `None` if the heading is not found. Fence-aware (a `#` inside a code
/// fence is not a heading). Used by the pin family to hash one section (#146),
/// and the raw-span twin of the marker family's section gating.
// In-tree production consumers migrated to the plural `section_spans` (GH #48
// lanes A/G closed the duplicate-heading evasion for section rules and pins);
// the singular first-occurrence resolver is kept — semantics deliberately
// untouched — and remains exercised by tests.
#[cfg_attr(not(test), allow(dead_code))]
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

/// EVERY heading-delimited span of `content` named by `heading_spec`, in
/// document order — the plural sibling of [`section_span`] with the SAME
/// matching semantics (exact heading-line equality: level AND text; fence-
/// aware). Used by the section-structural family (GH #48 round 2) so content
/// under a DUPLICATE same-level/same-text heading cannot evade a count or
/// disjointness gate — the first-occurrence [`section_span`] is deliberately
/// left unchanged (the pin family depends on its semantics; the pin
/// duplicate-binding question is routed separately). Empty when no heading
/// matches.
pub(crate) fn section_spans<'a>(content: &'a str, heading_spec: &str) -> Vec<&'a str> {
    let Some((want_level, want_text)) = atx_heading(heading_spec) else {
        return Vec::new();
    };
    let mut spans = Vec::new();
    let mut start: Option<usize> = None;
    for (offset, line) in non_fenced_lines(content) {
        if let Some((level, text)) = atx_heading(line) {
            if let Some(s) = start {
                if level <= want_level {
                    spans.push(&content[s..offset]); // next same/higher heading ends it
                    start = None;
                }
            }
            // An adjacent duplicate both ends the previous span and starts the
            // next one, so the two arms are sequential, not exclusive.
            if start.is_none() && level == want_level && text == want_text {
                start = Some(offset); // include the heading line itself
            }
        }
    }
    if let Some(s) = start {
        spans.push(&content[s..]);
    }
    spans
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

/// Inline code span ranges across a whole multi-line `body`, in `body` byte
/// coordinates — the body-wide companion to [`inline_code_ranges`], for a
/// scanner that matches over the whole body at once (the vocabulary family's
/// `find_iter`, #158) rather than line by line. A code span does not cross a
/// line, so each line's spans are computed and shifted by its start offset.
pub(crate) fn body_inline_code_ranges(body: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut cursor = 0usize;
    for raw_line in body.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\n', '\r']);
        for (s, e) in inline_code_ranges(line) {
            ranges.push((cursor + s, cursor + e));
        }
        cursor += raw_line.len();
    }
    ranges
}

/// GitHub's heading-to-anchor slug for one heading's text: lowercase, drop every
/// character that is not alphanumeric / `_` / `-`, and turn each space into a
/// hyphen. Duplicate-heading `-1`/`-2` disambiguation is applied by
/// [`heading_slugs`] across the whole document, not here.
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
        let text = |l| atx_heading(l).map(|(_, t)| t);
        assert_eq!(text("## Real Heading"), Some("Real Heading"));
        assert_eq!(text("### Foo ###"), Some("Foo"));
        assert_eq!(text("#hashtag"), None);
        assert_eq!(text("####### too deep"), None);
        assert_eq!(text("not a heading"), None);
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

    // GH #48 finding 5 (lane B): CommonMark's 0–3 indent rule. A 4+-space (or
    // tab) indented `# line` is CODE, not a heading — pre-fix the unbounded
    // trim_start read `    # install deps` as an H1, silently truncating any
    // section span (hence any section pin's hashed bytes) above it.
    #[test]
    fn atx_heading_honors_commonmark_indent_bound() {
        assert_eq!(
            atx_heading("   # x"),
            Some((1, "x")),
            "1–3 spaces still parse"
        );
        assert_eq!(atx_heading("  ## y"), Some((2, "y")));
        assert_eq!(atx_heading("    # x"), None, "4 spaces = indented code");
        assert_eq!(atx_heading("      # x"), None);
        assert_eq!(
            atx_heading("\t# x"),
            None,
            "a leading tab reaches the code indent"
        );
        assert_eq!(
            atx_heading(" \t# x"),
            None,
            "a tab anywhere in the indent is code"
        );
        // Lanes-B-E review F5: only literal SPACES are heading indentation —
        // exotic whitespace before `#` is paragraph text (the old trim_start
        // swallowed any Unicode whitespace after the space count).
        assert_eq!(atx_heading("\u{0c}# x"), None, "form feed is not indent");
        assert_eq!(atx_heading("\u{a0}# x"), None, "NBSP is not indent");
        assert_eq!(atx_heading(" \u{a0}# x"), None, "space then NBSP is text");
    }

    // GH #48 finding 5 (lane B, sibling minor): a fence indented 1–3 spaces is a
    // fence (CommonMark); 4+ spaces (or a tab) is indented code, not a fence.
    #[test]
    fn fence_marker_honors_commonmark_indent_bound() {
        assert_eq!(fence_marker("```"), Some(('`', 3)));
        assert_eq!(
            fence_marker("  ```sh"),
            Some(('`', 3)),
            "2-space indent toggles"
        );
        assert_eq!(fence_marker("   ~~~~"), Some(('~', 4)));
        assert_eq!(fence_marker("    ```"), None, "4-space indent is code");
        assert_eq!(fence_marker("\t```"), None, "tab indent is code");
        // A heading inside a 2-space-indented fence is not a heading: the fence
        // toggles, so the fenced line never reaches the heading scanner.
        let body = "# Real\n  ```\n# fenced\n  ```\nafter\n";
        let lines: Vec<&str> = non_fenced_lines(body).into_iter().map(|(_, l)| l).collect();
        assert_eq!(
            lines,
            vec!["# Real", "after"],
            "the fenced '# fenced' is masked"
        );
    }

    // GH #48 finding 5 (lane B): a section containing an indented `# comment`
    // (a shell snippet) runs to the REAL next heading — pre-fix the span
    // terminated at the snippet, so a section pin hashed only the bytes above
    // it and edits below never went stale.
    #[test]
    fn section_span_is_not_truncated_by_indented_code_comment() {
        let doc = "\
# Top\n\n## Contract\n\nintro\n\n    # install deps\n    make install\n\n\
below the snippet\n\n## Next\n\ntail\n";
        let span = section_span(doc, "## Contract").unwrap();
        assert!(
            span.contains("below the snippet"),
            "the span includes the lines below the indented snippet; got {span:?}"
        );
        assert!(
            span.contains("# install deps"),
            "the snippet itself is inside"
        );
        assert!(
            !span.contains("## Next"),
            "the real next heading still ends it"
        );
        // The plural resolver agrees (same scanner).
        assert_eq!(section_spans(doc, "## Contract"), vec![span]);
    }

    // GH #48 finding 6 (lane D): fenced-block byte ranges over a whole body,
    // marker lines included, unclosed fence running to end of input, 1–3-space
    // indented fences recognized.
    #[test]
    fn fenced_ranges_covers_blocks_including_markers() {
        let body = "before\n```yaml\nlayer: chassis\n```\nafter\n  ~~~\nx\n";
        let ranges = fenced_ranges(body);
        assert_eq!(
            ranges.len(),
            2,
            "one closed + one unclosed block: {ranges:?}"
        );
        let inside = body.find("layer").unwrap();
        let marker = body.find("```yaml").unwrap();
        let after = body.find("after").unwrap();
        assert!(ranges[0].contains(&inside), "fenced content is covered");
        assert!(
            ranges[0].contains(&marker),
            "the opening marker line is covered"
        );
        assert!(
            !ranges.iter().any(|r| r.contains(&after)),
            "prose between blocks is not"
        );
        let x = body.rfind('x').unwrap();
        assert!(
            ranges[1].contains(&x),
            "an unclosed fence runs to end of input"
        );
        assert!(fenced_ranges("no fences here\n").is_empty());
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

    // GH #48 round 2: the plural resolver returns EVERY matching span in
    // document order (same matching semantics as section_span), so content
    // under a duplicate heading cannot evade a section-structural gate.
    #[test]
    fn section_spans_returns_every_matching_span() {
        let doc = "## A\n\none\n\n## B\n\nbee\n\n## A\n\ntwo\n";
        let spans = section_spans(doc, "## A");
        assert_eq!(spans.len(), 2, "both `## A` spans resolve");
        assert!(spans[0].contains("one") && !spans[0].contains("bee"));
        assert!(spans[1].contains("two"), "the last span runs to end of doc");
        // The first span equals the singular resolver's (semantics agree).
        assert_eq!(spans[0], section_span(doc, "## A").unwrap());
        // Adjacent duplicates: the second heading both ends span 1 and opens span 2.
        let adj = "## A\n\nfirst\n\n## A\n\nsecond\n";
        let spans = section_spans(adj, "## A");
        assert_eq!(spans.len(), 2);
        assert!(spans[0].contains("first") && !spans[0].contains("second"));
        assert!(spans[1].contains("second"));
        // No match → empty; a fenced `## A` is not a heading.
        assert!(section_spans(doc, "## Nope").is_empty());
        assert!(section_spans("```\n## A\n```\n", "## A").is_empty());
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
    fn body_inline_code_ranges_are_body_coordinates_across_lines() {
        let body = "a `x` b\ncd `y` e\n";
        let r = body_inline_code_ranges(body);
        assert!(
            in_code_span(&r, body.find("`x`").unwrap() + 1),
            "inside `x` on line 1"
        );
        assert!(
            in_code_span(&r, body.find("`y`").unwrap() + 1),
            "inside `y` on line 2"
        );
        assert!(
            !in_code_span(&r, body.find('b').unwrap()),
            "plain `b` is outside"
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

    // ── #155: CommonMark-backed heading anchors + link extraction ───────────

    #[test]
    fn heading_slugs_disambiguates_duplicate_headings() {
        // GitHub: first "Foo" -> foo, second -> foo-1, third -> foo-2.
        let slugs = heading_slugs("# Foo\n\n## Foo\n\n### Foo\n");
        assert!(slugs.contains("foo"));
        assert!(slugs.contains("foo-1"));
        assert!(slugs.contains("foo-2"));
        assert!(!slugs.contains("foo-3"));
    }

    #[test]
    fn heading_slugs_covers_setext_and_html_anchor() {
        // A setext heading is a heading; an explicit <a name> is an anchor.
        let slugs = heading_slugs("Setext H1\n=========\n\n<a name=\"custom-spot\"></a>\n");
        assert!(slugs.contains("setext-h1"), "setext underline is a heading");
        assert!(slugs.contains("custom-spot"), "html anchor is a target");
    }

    #[test]
    fn heading_slugs_ignores_ids_inside_html_comments() {
        // A comment is not rendered, so an `id=` inside `<!-- … -->` is not an
        // anchor (else a dead `#ghost` link would silently resolve).
        let slugs = heading_slugs(
            "# Real\n\n<!-- id=\"ghost\" name=\"phantom\" -->\n\n<a id=\"live\"></a>\n",
        );
        assert!(slugs.contains("real"));
        assert!(slugs.contains("live"), "a real anchor still registers");
        assert!(!slugs.contains("ghost"), "commented id is not an anchor");
        assert!(
            !slugs.contains("phantom"),
            "commented name is not an anchor"
        );
    }

    #[test]
    fn heading_slugs_ignores_headings_inside_code_fences() {
        // A `#`-comment inside a fenced block is not a heading (structural).
        let slugs = heading_slugs("# Real\n\n```\n# Not A Heading\n```\n");
        assert!(slugs.contains("real"));
        assert!(!slugs.contains("not-a-heading"));
    }

    #[test]
    fn email_autolinks_carry_their_mailto_scheme() {
        let dests: Vec<String> = body_links("<team@acme.dev> and <https://acme.dev/x>\n")
            .into_iter()
            .map(|l| l.dest)
            .collect();
        assert_eq!(dests, vec!["mailto:team@acme.dev", "https://acme.dev/x"]);
    }

    #[test]
    fn body_links_covers_inline_reference_image_and_masks_code() {
        // Inline, reference-style, and image destinations are all found; a link
        // inside an inline code span or a fence is not a link event.
        let body = "\
[inline](a.md) and [ref][r] and ![img](c.png).\n\
Not `[code](nope.md)` and\n\
```\n[fenced](also-nope.md)\n```\n\n\
[r]: b.md\n";
        let dests: Vec<String> = body_links(body).into_iter().map(|l| l.dest).collect();
        assert!(dests.contains(&"a.md".to_string()), "inline");
        assert!(
            dests.contains(&"b.md".to_string()),
            "reference-style resolved"
        );
        assert!(dests.contains(&"c.png".to_string()), "image");
        assert!(
            !dests.iter().any(|d| d.contains("nope")),
            "code-span and fenced links are excluded structurally; got {dests:?}"
        );
    }

    // RED GATE (#243): a GFM footnote definition is not a reference
    // definition. Without the footnotes extension `[^1]: [fn](https://…)` was
    // read as one, the text `[fn](https://…)` became a "destination" and a
    // `[^1]` reference a link to it (a false E0110); the link INSIDE the
    // footnote body is the real destination, and a footnote's heading-like
    // body text never shifts the heading slugs.
    #[test]
    fn footnote_definitions_are_footnotes_not_reference_links() {
        // (footnote body, the destinations a renderer links)
        for (def, want) in [
            ("[fn](https://acme.dev/x)", vec!["https://acme.dev/x"]),
            ("notes.md", vec![]),
            ("see [rel](b.md)", vec!["b.md"]),
        ] {
            let body = format!("Claimed.[^1]\n\n[^1]: {def}\n\n# Real\n");
            let dests: Vec<String> = body_links(&body).into_iter().map(|l| l.dest).collect();
            assert_eq!(dests, want, "{def}");
            assert!(heading_slugs(&body).contains("real"), "{def}");
        }
    }

    // #243 review: a heading holding a footnote reference keeps the slug
    // GitHub renders (the ordinal) beside the bare one, and GitHub's footnote
    // anchors resolve — before, `#fn-1` and `#title1` were dead anchors.
    #[test]
    fn footnote_anchors_and_footnoted_headings_resolve() {
        let body = "# Title[^note]\n\nText.[^note] More.[^2]\n\n[^note]: a\n[^2]: b\n";
        let slugs = heading_slugs(body);
        for want in [
            "title",
            "title1",
            "titlenote",
            "fn-note",
            "user-content-fn-note",
            "fnref-note",
            "fnref-note-2",
            "user-content-fnref-2",
            "footnote-label",
        ] {
            assert!(slugs.contains(want), "{want}: {slugs:?}");
        }
        assert!(!heading_slugs("# Plain\n").contains("footnote-label"));
    }
}
