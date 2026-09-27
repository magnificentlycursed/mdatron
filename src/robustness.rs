//! Never-panic properties over the FIRST-PARTY body-text scanners (#193; the
//! lane-C C3 follow-up to the parser-robustness harness in
//! `tests/parser_robustness.rs`, GH #52 major 1 / #184).
//!
//! The harness covers every INPUT parser at the public API. What it named as
//! not covered was mdatron's own markdown-body surface: the marker, citation,
//! link (percent-decode, heading slugs, HTML anchors), section and vocabulary
//! scanners plus the `markup` primitives they share — pure functions over
//! content and configuration, hand-rolled, and the home of the #48-era
//! byte-slice class (a `&s[i..]` at a non-char boundary next to a multibyte
//! character). Since #185 those modules are crate-private, so the properties
//! live here, in-crate and test-only (`#[cfg(test)] mod robustness;`), and
//! CI's parser-robustness job runs them beside the harness
//! (`cargo test --lib robustness::`, bounded and deep; `PROPTEST_CASES` /
//! `PROPTEST_RNG_SEED` apply as there).
//!
//! Two layers, as in the harness: deterministic revert-detector seeds pinning
//! the known hostile classes, and bounded property exploration around them.
//! Only panic-freedom is pinned — a finding, no finding, or a structured
//! error are all acceptable outcomes; a panic (or an abort) is the defect.
//!
//! Reference: the lychee link-family audit (knowledge page
//! `lychee-link-family-audit`) — a link checker's percent-decode, fragment and
//! path-resolution edges are where hand-rolled scanners fail first.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use proptest::prelude::*;

use crate::markup::{self, ElementClass};
use crate::route::MarkerRule;
use crate::snapshot::Snapshot;

// ── Scratch roots ───────────────────────────────────────────────────────────

/// A fresh scratch project root (unique per call; removed on drop).
struct ScratchRoot(PathBuf);

static SCRATCH_N: AtomicU64 = AtomicU64::new(0);

impl ScratchRoot {
    fn new() -> Self {
        let n = SCRATCH_N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "mdatron-robustness-{}-{n}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn write(&self, rel: &str, content: &str) {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
}

impl Drop for ScratchRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ── Strategies ──────────────────────────────────────────────────────────────

/// Markdown-shaped hostile fragments: every syntax character the scanners key
/// on, adjacent to multibyte text; percent-encoding edge cases; unterminated
/// fences and code spans; control, BOM, bidi and separator characters; both
/// line endings; bracket floods; citation and marker shapes.
const FRAGMENTS: &[&str] = &[
    "# ",
    "## Heading — with dash € and é\n",
    "#—\n",
    "###### deep\n",
    "#\n",
    "# `code` in heading\n",
    "```",
    "```\n",
    "~~~",
    "~~~~\n",
    "```rust\n",
    "``` ` ``",
    "`",
    "``",
    "`é`",
    "` ` `",
    "[text](target.md)",
    "[t](%)",
    "[t](%2)",
    "[t](%zz)",
    "[t](%C3)",
    "[t](%E2%82)",
    "[t](%C3%A9.md#fr%C3%A4g)",
    "[t](%00.md)",
    "[t](#)",
    "[t](#%)",
    "[t](#é—)",
    "[é](x.md)",
    "[t](./a/../../x.md)",
    "[t](/abs.md)",
    "[t](x.md#frag#two)",
    "[t](http://x/y.md:1)",
    "[t]()",
    "[t](",
    "[t](x.md \"title\")",
    "![img](i.png)",
    "[t]: x.md",
    "[t][ref]",
    "[[[",
    "]]]",
    "(((",
    ")))",
    "<a name=\"an[chor\">",
    "<a id='é'>",
    "<!-- unterminated",
    "<!-- -->",
    "- **Bold**: item\n",
    "- **é**\n",
    "* **Name** — text\n",
    "-  **spaced**\n",
    "- **\n",
    "docs/x.md:12-3",
    "../x.md:999999",
    "x.md:1-",
    "é.md:1",
    "x.md:0",
    "a/b.md:12-13 ",
    "\r\n",
    "\r",
    "\n",
    "\u{0}",
    "\u{FEFF}",
    "\u{2028}",
    "\u{2029}",
    "\u{202E}",
    "\u{200B}",
    "\u{301}",
    "é",
    "—",
    "€",
    "👩‍💻",
    "ﬁ",
    "İ",
    "ß",
    "REQ-1",
    "ABC-12",
    "SEC-F3",
    "M2",
    "L2",
    "   ",
    "\t",
    " \t \n\n\n",
];

fn fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        8 => prop::sample::select(FRAGMENTS).prop_map(str::to_string),
        1 => any::<char>().prop_map(String::from),
        1 => "\\PC{0,8}",
    ]
}

/// A hostile markdown body: a concatenation of fragments.
fn hostile_body() -> impl Strategy<Value = String> {
    prop::collection::vec(fragment(), 0..48).prop_map(|v| v.concat())
}

/// A hostile heading specification for the section scanners: an ATX heading
/// or an arbitrary token.
fn heading_spec() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("## Members".to_string()),
        Just("# ".to_string()),
        Just("#".to_string()),
        Just(String::new()),
        Just("## Heading — with dash € and é".to_string()),
        Just("### `code`".to_string()),
        prop::collection::vec(fragment(), 0..6).prop_map(|v| v.concat()),
    ]
}

/// A section rule as an adopter would write it under `section_rules:` — the
/// count and disjoint shapes, with hostile section names and patterns. Invalid
/// rules are an `Err` at compile, never a panic.
fn section_rule_yaml() -> impl Strategy<Value = String> {
    let element = prop::sample::select(vec![
        "heading",
        "h1",
        "h2",
        "h3",
        "h6",
        "list-item-bold-name",
        "h3-heading",
        "bullet-lead",
        "bogus",
    ]);
    let pattern = prop::sample::select(vec![
        "^REQ-[0-9]+$",
        "(",
        "^$",
        ".*",
        "^[A-Z]+-\\d+$",
        "é",
        "[",
    ]);
    let count = prop::sample::select(vec![">= 1", "== 0", "<= 3", "> -1", "!= 2", "1", ">=", ""]);
    prop_oneof![
        (heading_spec(), element.clone(), pattern.clone(), count).prop_map(|(s, e, p, c)| {
            format!(
                "section: {s:?}\nelement: {e}\nmatch: {p:?}\ncount: {c:?}\n"
            )
        }),
        (heading_spec(), element.clone(), pattern.clone(), heading_spec(), element, pattern).prop_map(
            |(s1, e1, p1, s2, e2, p2)| {
                format!(
                    "disjoint:\n  - section: {s1:?}\n    element: {e1}\n    id_pattern: {p1:?}\n  - section: {s2:?}\n    element: {e2}\n    id_pattern: {p2:?}\n"
                )
            }
        ),
    ]
}

/// A `body_offset` that is a char boundary of `content` (the pipeline hands
/// the scanners the offset just past the frontmatter fence; the property
/// exercises every boundary, including 0 and the end).
fn char_boundary(content: &str, pick: usize) -> usize {
    let mut boundaries: Vec<usize> = content.char_indices().map(|(i, _)| i).collect();
    boundaries.push(content.len());
    boundaries[pick % boundaries.len()]
}

const ALL_ELEMENTS: [ElementClass; 8] = [
    ElementClass::Heading,
    ElementClass::H1,
    ElementClass::H2,
    ElementClass::H3,
    ElementClass::H4,
    ElementClass::H5,
    ElementClass::H6,
    ElementClass::ListItemBoldName,
];

// ── Drivers (shared by seeds and properties) ─────────────────────────────────

/// Every `markup` primitive over a body and each of its lines.
fn drive_markup_primitives(body: &str) {
    let _ = markup::heading_slugs(body);
    let _ = markup::body_links(body);
    let _ = markup::fenced_ranges(body);
    let _ = markup::non_fenced_lines(body);
    let _ = markup::body_inline_code_ranges(body);
    for line in body.lines() {
        let _ = markup::inline_code_ranges(line);
        let _ = markup::atx_heading(line);
        let _ = markup::list_item_bold_name(line);
        let _ = markup::fence_marker(line);
        let _ = markup::slugify(line);
        for class in ALL_ELEMENTS {
            let _ = class.name_in(line);
        }
    }
}

fn drive_section_spans(body: &str, spec: &str) {
    let _ = markup::section_span(body, spec);
    let _ = markup::section_spans(body, spec);
}

/// Target extraction for the citation and link families — the percent-decode
/// and confinement path — at a given body offset.
fn drive_target_extraction(body: &str, body_offset: usize, root_relative: bool) {
    let _ = crate::cite::cited_targets(body, body_offset);
    let _ = crate::link::link_targets(
        Path::new("docs/sub/file.md"),
        body,
        body_offset,
        root_relative,
    );
    let _ = crate::link::link_targets(Path::new("top.md"), body, body_offset, root_relative);
}

/// A section rule compiled from adopter YAML and, when it compiles, run over
/// the body.
fn drive_section_rule(rule_yaml: &str, body: &str, body_offset: usize) {
    let raw: Result<crate::section::RawRule, _> = serde_yaml_ng::from_str(rule_yaml);
    let Ok(raw) = raw else { return };
    let Ok(rule) = crate::section::compile_rule(raw) else {
        return;
    };
    let mut findings = Vec::new();
    crate::section::check_file(
        &[&rule],
        Path::new("docs/x.md"),
        body,
        body_offset,
        &mut findings,
    );
}

/// The vocabulary scan with this repository's own register (the richest
/// real-world register to hand: terms, reserved spellings, anti-patterns,
/// label schemes, numeric claims).
fn drive_vocabulary_scan(body: &str, body_offset: usize, coinage: bool) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let loaded = crate::vocab::load(root)
        .expect("the repository's own vocabulary.yaml loads")
        .expect("the repository declares a vocabulary register");
    let mut findings = Vec::new();
    crate::vocab::check_file(
        &loaded,
        Path::new("docs/x.md"),
        body,
        body_offset,
        None,
        coinage,
        &mut findings,
    );
}

/// The snapshot-backed scanners — citation, link and marker `check_file` —
/// over a scratch root whose governed file AND target document are both
/// hostile. Targets are pre-captured the way the pipeline's coverage pass does
/// it, so the target-side scans (heading slugs, section spans, member lookup)
/// run over hostile content too.
fn drive_snapshot_backed_scanners(body: &str, target: &str, root_relative: bool) {
    let root = ScratchRoot::new();
    root.write("docs/target.md", target);
    root.write("docs/sub/file.md", body);
    let mut snapshot = Snapshot::new(
        crate::verify::MAX_FILE_BYTES,
        crate::verify::MAX_AGGREGATE_BYTES,
    );
    let rel_file = crate::confine::confine_lexically(Path::new("docs/sub/file.md")).unwrap();
    let _ = snapshot.capture(&root.0, &rel_file);
    let mut targets = crate::cite::cited_targets(body, 0);
    targets.extend(crate::link::link_targets(
        Path::new("docs/sub/file.md"),
        body,
        0,
        root_relative,
    ));
    if let Ok(t) = crate::confine::confine_lexically(Path::new("docs/target.md")) {
        targets.push(t);
    }
    for t in &targets {
        let _ = snapshot.capture(&root.0, t);
    }
    snapshot.seal();

    let path = root.0.join("docs/sub/file.md");
    let mut findings = Vec::new();
    let mut memo = crate::memo::RefMemo::default();
    crate::cite::check_file(&snapshot, &path, body, 0, &mut findings);
    crate::link::check_file(
        &snapshot,
        &root.0,
        &path,
        body,
        0,
        root_relative,
        &mut memo,
        &mut findings,
    );
    let rules = [
        MarkerRule {
            pattern: regex_lite::Regex::new(r"^- \*\*([^*]+)\*\*").unwrap(),
            element: ElementClass::H3,
            target_doc: "docs/target.md".to_string(),
            target_section: Some("## Members".to_string()),
        },
        MarkerRule {
            pattern: regex_lite::Regex::new(r"^\* \*\*([^*]+)\*\*").unwrap(),
            element: ElementClass::ListItemBoldName,
            target_doc: "docs/target.md".to_string(),
            target_section: None,
        },
        MarkerRule {
            pattern: regex_lite::Regex::new(r"^#+ (.+)$").unwrap(),
            element: ElementClass::Heading,
            target_doc: "docs/missing.md".to_string(),
            target_section: Some("# ".to_string()),
        },
    ];
    let refs: Vec<&MarkerRule> = rules.iter().collect();
    crate::marker::check_file(&snapshot, &path, body, 0, &refs, &mut memo, &mut findings);
}

// ── Revert-detector seeds ────────────────────────────────────────────────────
//
// Deterministic classes: multibyte text adjacent to every syntax character the
// scanners slice around (the #48-era byte-slice class), the percent-decode
// edges, unterminated fences and code spans, and control/bidi/separator
// characters. Each runs every driver, so a regression in any scanner is
// caught without depending on the RNG.

const SEEDS: &[&str] = &[
    "[é](x.md) `€` #— - **é**: x docs/é.md:1 <a name=\"é\">\n## é — €\n",
    "[t](%) [t](%2) [t](%zz) [t](%C3) [t](%E2%82) [t](%C3%A9.md#fr%C3%A4g) [t](%00.md) [t](#%)\n",
    "```\nno end\n``` ` ``\n~~~\n` unbalanced `` code\n",
    "\u{FEFF}# \u{0}Heading\u{2028}\r\n- **\u{202E}name**\r[t](\u{200B}x.md)\r\n",
    "[[[]]] ((())) [t]( [t]() ![i](%) [t][r] [r]: %\n",
    "# \n#\n######\n####### seven\n#\u{301}\n- **\n* ** **\n-  **x**\n",
    "x.md:0 x.md:1- ../../../x.md:99999999 /abs.md:1-2 http://h/x.md:1 é.md:1-1\n",
];

#[test]
fn seed_hostile_classes_never_panic_the_body_scanners() {
    for body in SEEDS {
        drive_markup_primitives(body);
        for spec in ["## Members", "# ", "", "## é — €", "### `code`"] {
            drive_section_spans(body, spec);
        }
        for offset in [0, body.len()] {
            drive_target_extraction(body, offset, false);
            drive_target_extraction(body, offset, true);
            drive_vocabulary_scan(body, offset, true);
        }
        drive_section_rule(
            "section: \"## é\"\nelement: h3\nmatch: \"^REQ-\\\\d+$\"\ncount: \">= 1\"\n",
            body,
            0,
        );
        drive_snapshot_backed_scanners(body, body, false);
        drive_snapshot_backed_scanners(body, body, true);
    }
}

// ── Properties ───────────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_markup_primitives_never_panic(body in hostile_body()) {
        drive_markup_primitives(&body);
    }

    #[test]
    fn prop_markup_primitives_never_panic_on_arbitrary_text(body in any::<String>()) {
        drive_markup_primitives(&body);
    }

    #[test]
    fn prop_section_spans_never_panic(body in hostile_body(), spec in heading_spec()) {
        drive_section_spans(&body, &spec);
    }

    #[test]
    fn prop_target_extraction_never_panics(
        body in hostile_body(),
        pick in any::<usize>(),
        root_relative in any::<bool>(),
    ) {
        drive_target_extraction(&body, char_boundary(&body, pick), root_relative);
    }

    #[test]
    fn prop_section_rules_never_panic(
        body in hostile_body(),
        rule in section_rule_yaml(),
        pick in any::<usize>(),
    ) {
        drive_section_rule(&rule, &body, char_boundary(&body, pick));
    }

    #[test]
    fn prop_vocabulary_scan_never_panics(
        body in hostile_body(),
        pick in any::<usize>(),
        coinage in any::<bool>(),
    ) {
        drive_vocabulary_scan(&body, char_boundary(&body, pick), coinage);
    }
}

proptest! {
    // Filesystem-backed: a scratch root per case, so a lower case count than
    // the pure drivers (this explicit `cases` also means the deep run's
    // PROPTEST_CASES does not multiply the IO cost).
    #![proptest_config(ProptestConfig {
        cases: 32,
        ..ProptestConfig::default()
    })]
    #[test]
    fn prop_snapshot_backed_scanners_never_panic(
        body in hostile_body(),
        target in hostile_body(),
        root_relative in any::<bool>(),
    ) {
        drive_snapshot_backed_scanners(&body, &target, root_relative);
    }
}
