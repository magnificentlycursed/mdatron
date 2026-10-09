//! Never-panic properties over the FIRST-PARTY body-text scanners (#193; the
//! lane-C C3 follow-up to the parser-robustness harness in
//! `tests/parser_robustness.rs`, GH #52 major 1 / #184).
//!
//! The harness covers every INPUT parser at the public API. What it named as
//! not covered was mdatron's own markdown-body surface: the marker, citation,
//! link (percent-decode, heading slugs, HTML anchors), section, vocabulary and
//! code-catalog scanners plus the `markup` primitives they share — pure
//! functions over content and configuration, hand-rolled, and the home of the
//! #48-era byte-slice class (a `&s[i..]` at a non-char boundary next to a
//! multibyte character). Since #185 those modules are crate-private, so the
//! properties live here, in-crate and test-only (`#[cfg(test)] mod
//! robustness;`), and CI's parser-robustness job runs them beside the harness
//! (`cargo test --lib robustness::`; `PROPTEST_CASES` / `PROPTEST_RNG_SEED`
//! apply to the pure properties; the file-backed property is hard-capped
//! through an explicit `TestRunner` because a `proptest!` config pin is
//! re-contextualized from the env and would not hold).
//!
//! Four layers: deterministic revert-detector seeds pinning the known hostile
//! classes; REACH seeds feeding hand-written inputs to the drivers and
//! asserting each scanner branch produces its finding; STRATEGY-LEVEL reach
//! tests running the properties' own strategies at a fixed seed and counting
//! those findings (so a strategy that quietly stops reaching a branch fails —
//! round 2 found the disjoint comparison reached 0 times while every
//! hand-written reach seed was green); and bounded property exploration. Only panic-freedom is pinned by the
//! properties — a finding, no finding, or a structured error are all
//! acceptable; a panic (or an abort) is the defect.
//!
//! Configuration is SELF-CONTAINED: a fixture vocabulary register and code
//! catalog are written to a scratch root and loaded through the real loaders.
//! A shipped unit test must never read the repository's `.mdatron/` tree — the
//! packaged crate has none, and its `cargo test` would fail (the
//! docs/limits.md lesson, GH #48 lane E; L4 cold review MAJOR-1).
//!
//! COVERED: the `markup` primitives per body and per line; `section_span(s)`
//! with hostile specs; `cite::cited_targets` and `link::link_targets` at generated
//! char-boundary body offsets; section rules built as YAML VALUES (so hostile
//! section names survive to `compile_rule`) and drawn TOGETHER with the body
//! from one section prefix, so the count and disjoint comparisons run
//! (asserted by the strategy-level reach test); the vocabulary scan with
//! reserved spellings, anti-patterns, label schemes and numeric claims (with
//! generated frontmatter), each reached from the body strategy (asserted);
//! the code-catalog scan against a comprehensive fixture catalog; and the
//! snapshot-backed cite/link/marker `check_file`s at a generated body offset
//! over a scratch root whose target documents (both the document-relative and
//! the root-relative resolution of the fragments' links) are hostile too and
//! carry the marker rules' section and members half the time (cross-file
//! anchors, missing members and resolved members all asserted).
//! NOT COVERED: pulldown-cmark's own parse (hardened upstream); the
//! snapshot/confinement IO paths (symlinks, unreadable files, size bounds —
//! the confine tests own them); semantic/differential properties; and
//! coverage-guided mutation (no libFuzzer on stable).
//!
//! Reference: the lychee link-family audit (knowledge page
//! `lychee-link-family-audit`) — a link checker's percent-decode, fragment and
//! path-resolution edges are where hand-rolled scanners fail first.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;

use proptest::prelude::*;
use proptest::test_runner::{RngAlgorithm, TestRng, TestRunner};
use serde_yaml_ng::Value as Yaml;

use crate::codecat::CodeCatalog;
use crate::diagnostic::Finding;
use crate::markup::{self, ElementClass};
use crate::route::MarkerRule;
use crate::snapshot::Snapshot;
use crate::vocab::LoadedVocab;

// ── Scratch roots ───────────────────────────────────────────────────────────

/// A fresh scratch project root (unique per call; removed on drop — Drop runs
/// through proptest's unwind, so a failing case leaves nothing behind).
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

// ── Fixture configuration ───────────────────────────────────────────────────

/// A register exercising every vocabulary-family lane: registered terms
/// (multibyte, single-letter, uppercase-next-to-a-label-scheme, multi-word),
/// a draft that conflicts with a registered spelling, reserved spellings,
/// anti-patterns with edge-case regexes, numeric claims, permissive label
/// schemes and a coinage scope. Every term and reserved spelling also appears
/// in [`FRAGMENTS`], so the bodies reach the reserved-word and anti-pattern
/// arms (L4 cold review MAJOR-3c).
const FUZZ_REGISTER: &str = r#"mdatron_format_version: 1
terms:
  - term: "conformance engine"
    status: registered
    sense: "the product"
  - term: "café engine"
    status: registered
    sense: "a multibyte term"
  - term: "walked file"
    status: registered
    sense: "a file in the jurisdiction"
  - term: "e"
    status: registered
    sense: "a one-letter term"
  - term: "REQ"
    status: registered
    sense: "an uppercase term next to a label scheme"
  - term: "Layer 1"
    status: reserved
    sense: "retired pedagogy"
  - term: "bullet lead"
    status: reserved
    sense: "retired alias"
  - term: "—"
    status: reserved
    sense: "a punctuation-only reserved spelling"
  - term: "widget"
    status: draft
    sense: "a draft term"
  - term: "widget"
    status: registered
    sense: "the same term registered: a draft conflict"
coinage_globs:
  - "docs/**/*.md"
label_schemes:
  allow:
    - "^REQ-[0-9]+$"
    - "^[A-Z]{2,4}-F?[0-9]+$"
    - "^$"
anti_patterns:
  - pattern: "\\bvery\\b"
    guidance: "delete it"
  - pattern: "(é+)"
    guidance: "a multibyte anti-pattern"
  - pattern: "^$"
    guidance: "matches the empty string"
numeric_claims:
  - field: latency_ms
  - field: "é"
"#;

/// A comprehensive catalog (so an undeclared token under its prefix is an
/// orphan) plus a non-comprehensive one; both prefixes appear in
/// [`FRAGMENTS`], declared and undeclared.
const FUZZ_CATALOGS: &str = r#"mdatron_format_version: 1
catalogs:
  - namespace: "ADOPTER-"
    comprehensive: true
    codes: ["E0001", "W0100"]
  - namespace: "OTHER-"
    comprehensive: false
    codes: ["X1"]
"#;

/// The fixture register, loaded ONCE through the real loader from a scratch
/// root (a per-case load was the dominant deep-run cost, MINOR-4).
static REGISTER: LazyLock<LoadedVocab> = LazyLock::new(|| {
    let root = ScratchRoot::new();
    root.write(".mdatron/vocabulary.yaml", FUZZ_REGISTER);
    crate::vocab::load(&root.0)
        .expect("the fixture register loads")
        .expect("the fixture register is present")
});

/// The fixture catalogs, loaded once likewise.
static CATALOGS: LazyLock<Vec<CodeCatalog>> = LazyLock::new(|| {
    let root = ScratchRoot::new();
    root.write(".mdatron/code-catalogs.yaml", FUZZ_CATALOGS);
    crate::codecat::load(&root.0)
        .expect("the fixture catalogs load")
        .expect("the fixture catalogs are present")
        .catalogs
});

// ── Strategies ──────────────────────────────────────────────────────────────

/// Markdown-shaped hostile fragments: every syntax character the scanners key
/// on, adjacent to multibyte text; percent-encoding edge cases; unterminated
/// fences and code spans; control, BOM, bidi and separator characters; both
/// line endings; bracket floods; citation and marker shapes; the sections,
/// members and links the section, marker and link rules of this module look
/// for (so the target-side branches are reached, not just the not-found arms);
/// and the fixture register's terms, reserved spellings, anti-patterns and the
/// fixture catalogs' code tokens.
const FRAGMENTS: &[&str] = &[
    // headings, fences, code spans
    "# ",
    "## Heading — with dash € and é\n",
    "#—\n",
    "###### deep\n",
    "#\n",
    "# `code` in heading\n",
    "## Members\n",
    "## Others\n",
    "### Bold\n",
    "### REQ-1\n",
    "#### ABC-12\n",
    "### é\n",
    "# Title\n",
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
    // links: same-document anchors, document-relative and root-relative targets, percent edges
    "[text](target.md)",
    "[t](../target.md)",
    "[t](../target.md#members)",
    "[t](/docs/target.md#é)",
    "[t](../target.md#heading--with-dash--and-)",
    "[t](target.md#bold)",
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
    "<a name=\"members\">",
    "<!-- unterminated",
    "<!-- -->",
    // list items with bold names (marker shapes) — line-initial, since a marker
    // rule's pattern is anchored at the line start (round 2, MINOR-E)
    "\n- **Bold**: item\n",
    "\n- **Nope**: item\n",
    "\n- **é**\n",
    "\n* **Name** — text\n",
    "\n-  **spaced**\n",
    "\n- **\n",
    "\n- **REQ-2**: item\n",
    "- **Bold**",
    // citations
    "docs/x.md:12-3",
    "../x.md:999999",
    "x.md:1-",
    "é.md:1",
    "x.md:0",
    "a/b.md:12-13 ",
    "docs/target.md:1",
    "docs/target.md:999",
    "\u{FEFF}docs/x.md:12-3",
    // vocabulary: terms, reserved spellings, anti-patterns, label clusters
    "conformance engine",
    "café engine",
    "walked file",
    "walked files",
    "Layer 1",
    "bullet lead",
    "widget",
    "very",
    "REQ",
    "REQ-1",
    "ZZZZZ-9",
    "SEC-F3",
    "M2",
    "L2",
    "ABC-12",
    // numeric claims: the field name (or its spaced form) with a number in the
    // words before it is what the comparison keys on (round 2, MINOR-B)
    "12 latency_ms\n",
    "twelve latency ms",
    "we measured 12 latency_ms today",
    "3 latency ms",
    "latency_ms",
    "latency is 12 ms",
    "12",
    "e",
    // code-catalog tokens: declared, undeclared, mistyped
    "ADOPTER-E0001",
    "ADOPTER-W0100",
    "ADOPTER-X9",
    "ADOPTER-",
    "`ADOPTER-E0002`",
    "OTHER-X1",
    "OTHER-Y2",
    // control, BOM, bidi, separators, line endings, multibyte
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
    "🟠",
    "ﬁ",
    "İ",
    "ß",
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

/// Section prefixes the section and marker rules resolve against: the named
/// sections with id-bearing elements of every class the rules use; four of
/// the five carry an id in BOTH sections, so a disjoint rule over them finds
/// an overlap (the E0121 arm) and not only the clean case.
const SECTION_PREFIXES: &[&str] = &[
    "## Members\n### REQ-1\n- **REQ-2**: item\n#### ABC-12\n### Bold\n## Others\n### REQ-1\n- **REQ-2**\n",
    "## Others\n### REQ-1\n### REQ-3\n### Bold\n## Members\n### REQ-3\n",
    "## Members\n### é\n- **é**\n### Bold\n## Others\n### é\n",
    "# Title\n## Members\n### Bold\n\n## Others\n### REQ-1\n",
    "## Members\n### REQ-1\n### Bold\n## Others\n### REQ-1\n",
];

/// A hostile body that, half the time, opens with a section prefix — so the
/// section rules' count and disjoint arms, and the marker rules' member
/// lookups, run over real sections rather than only their not-found arms
/// (MAJOR-3a/3b).
fn sectioned_body() -> impl Strategy<Value = String> {
    (
        any::<bool>(),
        prop::sample::select(SECTION_PREFIXES),
        hostile_body(),
    )
        .prop_map(|(with, prefix, body)| {
            if with {
                format!("{prefix}{body}")
            } else {
                body
            }
        })
}

/// Heading specifications the section scanners resolve: the sections the
/// bodies carry, plus hostile shapes.
const HEADING_SPECS: &[&str] = &[
    "## Members",
    "## Others",
    "### Bold",
    "# Title",
    "# ",
    "#",
    "",
    "## Heading — with dash € and é",
    "### `code`",
    "### é",
];

fn heading_spec() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => prop::sample::select(HEADING_SPECS).prop_map(str::to_string),
        1 => prop::collection::vec(fragment(), 0..4).prop_map(|v| v.concat()),
    ]
}

fn yaml_map(pairs: Vec<(&str, Yaml)>) -> Yaml {
    let mut m = serde_yaml_ng::Mapping::new();
    for (k, v) in pairs {
        m.insert(Yaml::String(k.to_string()), v);
    }
    Yaml::Mapping(m)
}

fn ystr(s: impl Into<String>) -> Yaml {
    Yaml::String(s.into())
}

fn element_pick() -> impl Strategy<Value = &'static str> {
    // Valid classes dominate; the aliases and one bogus class keep the
    // compile-error arm honest without starving the scanners.
    prop::sample::select(
        &[
            "heading",
            "h1",
            "h2",
            "h3",
            "h3",
            "h3",
            "h4",
            "h6",
            "list-item-bold-name",
            "list-item-bold-name",
            "h3-heading",
            "bullet-lead",
            "bogus",
        ][..],
    )
}

/// The classes an every or order rule (and a whole-document count rule) draws
/// from: the line-based classes added for #213/#214/#217 beside the headings
/// and list items the prefixes carry.
fn shape_element_pick() -> impl Strategy<Value = &'static str> {
    prop::sample::select(
        &[
            "h3",
            "h3",
            "heading",
            "list-item",
            "list-item",
            "list-item-bold-name",
            "blockquote",
            "line",
            "line",
            "bogus",
        ][..],
    )
}

/// Disjoint operands lean on the classes whose ids the prefixes carry in both
/// sections (`h3`, `heading`), so the overlap arm is reached, not only the
/// clean and not-found arms.
fn disjoint_element_pick() -> impl Strategy<Value = &'static str> {
    prop::sample::select(
        &[
            "h3",
            "h3",
            "h3",
            "heading",
            "heading",
            "list-item-bold-name",
            "h2",
            "h3-heading",
            "bogus",
        ][..],
    )
}

/// A count rule's `match` is tested against the whole element LINE.
fn match_pick() -> impl Strategy<Value = &'static str> {
    prop::sample::select(
        &[
            "REQ-[0-9]+",
            "REQ-[0-9]+",
            "^#{3} REQ-[0-9]+$",
            "[A-Z]+-[0-9]+",
            ".*",
            "^$",
            "é",
            "REQ",
            "Bold",
            "(",
        ][..],
    )
}

/// A disjoint operand's `id_pattern` is tested against the element's NAME, with
/// the id in capture group 1.
fn id_pick() -> impl Strategy<Value = &'static str> {
    prop::sample::select(
        &[
            "^(REQ-[0-9]+)$",
            "^(REQ-[0-9]+)$",
            "^(REQ-[0-9]+)$",
            "^([A-Z]+-[0-9]+)$",
            "(.*)",
            "(.*)",
            "(é+)",
            "^$",
            "REQ-[0-9]+",
            "(",
        ][..],
    )
}

fn count_pick() -> impl Strategy<Value = &'static str> {
    prop::sample::select(
        &[
            ">= 1", ">= 1", ">= 1", "== 0", "<= 3", "> 0", "!= 2", "== 1", "1", ">=",
        ][..],
    )
}

/// The rule's section: one of the SECTIONS (`##` headings) the chosen prefix
/// carries three times in five — those are the spans with id-bearing
/// elements inside, so the count and disjoint comparisons run — any of its
/// headings one time in five (a sub-heading names an empty span: the
/// nothing-to-count arm), and a hostile spec one time in five.
fn section_pick(headings: Vec<String>) -> impl Strategy<Value = String> {
    let sections: Vec<String> = headings
        .iter()
        .filter(|h| markup::atx_heading(h).is_some_and(|(level, _)| level == 2))
        .cloned()
        .collect();
    prop_oneof![
        3 => prop::sample::select(sections),
        1 => prop::sample::select(headings),
        1 => heading_spec(),
    ]
}

fn headings_of(prefix: &str) -> Vec<String> {
    prefix
        .lines()
        .filter(|l| markup::atx_heading(l).is_some())
        .map(str::to_string)
        .collect()
}

fn yaml_rule_count(section: String, element: &str, matcher: Option<&str>, count: &str) -> Yaml {
    let mut pairs = vec![
        ("section", ystr(section)),
        ("element", ystr(element)),
        ("count", ystr(count)),
    ];
    if let Some(m) = matcher {
        pairs.insert(2, ("match", ystr(m)));
    }
    yaml_map(pairs)
}

fn yaml_rule_disjoint(a: (String, &str, &str), b: (String, &str, &str)) -> Yaml {
    let operand = |(s, e, p): (String, &str, &str)| {
        yaml_map(vec![
            ("section", ystr(s)),
            ("element", ystr(e)),
            ("id_pattern", ystr(p)),
        ])
    };
    yaml_map(vec![(
        "disjoint",
        Yaml::Sequence(vec![operand(a), operand(b)]),
    )])
}

/// A body and a section rule drawn TOGETHER (L4 round 2, MAJOR-A): the same
/// section prefix seeds the body (three cases in four) and supplies the
/// rule's section names, so the rule resolves a section that exists and the
/// count and disjoint comparisons actually run — a rule drawn independently
/// of the body found its section 3 times in 1024 and compared two id sets
/// never. Rules are YAML VALUES, not text, so BOM/bidi/combining characters
/// in a section name reach `compile_rule` and the scanners rather than dying
/// as a YAML escape error (MINOR-2). Valid shapes dominate; the harness's
/// loader property owns the config-error paths.
fn rule_case() -> impl Strategy<Value = (String, Yaml)> {
    (0..SECTION_PREFIXES.len()).prop_flat_map(|i| {
        let prefix = SECTION_PREFIXES[i];
        let headings = headings_of(prefix);
        let body = (prop::bool::weighted(0.75), hostile_body()).prop_map(move |(with, h)| {
            if with {
                format!("{prefix}{h}")
            } else {
                h
            }
        });
        let count_rule = (
            section_pick(headings.clone()),
            element_pick(),
            match_pick(),
            count_pick(),
            prop::bool::weighted(0.1),
        )
            .prop_map(|(s, e, m, c, drop_match)| {
                yaml_rule_count(s, e, (!drop_match).then_some(m), c)
            });
        let disjoint_rule = (
            section_pick(headings.clone()),
            disjoint_element_pick(),
            id_pick(),
            section_pick(headings),
            disjoint_element_pick(),
            id_pick(),
        )
            .prop_map(|(s1, e1, p1, s2, e2, p2)| yaml_rule_disjoint((s1, e1, p1), (s2, e2, p2)));
        // A COHERENT pair (round 3, MINOR-3): both sections the prefixes
        // share, one id-bearing class, one id pattern — an overlap otherwise
        // needed six independent draws to line up. Three of the five prefixes
        // carry the same REQ id in both sections.
        let coherent_rule = Just(yaml_rule_disjoint(
            ("## Members".to_string(), "h3", "^(REQ-[0-9]+)$"),
            ("## Others".to_string(), "h3", "^(REQ-[0-9]+)$"),
        ));
        // The every, order and whole-document shapes (#213/#214/#217): the
        // section is the prefix's own three times in four and absent (the
        // whole document) otherwise.
        let optional_section = || {
            prop_oneof![
                3 => section_pick(headings_of(prefix)).prop_map(Some),
                1 => Just(None),
            ]
        };
        let with_section = |section: Option<String>, mut pairs: Vec<(&'static str, Yaml)>| {
            if let Some(s) = section {
                pairs.insert(0, ("section", ystr(s)));
            }
            yaml_map(pairs)
        };
        let every_rule =
            (optional_section(), shape_element_pick(), match_pick()).prop_map(move |(s, e, m)| {
                with_section(s, vec![("element", ystr(e)), ("every", ystr(m))])
            });
        let order_item = || {
            (shape_element_pick(), match_pick())
                .prop_map(|(e, m)| yaml_map(vec![("element", ystr(e)), ("match", ystr(m))]))
        };
        let order_rule = (
            optional_section(),
            prop::collection::vec(order_item(), 1..5),
        )
            .prop_map(move |(s, items)| with_section(s, vec![("order", Yaml::Sequence(items))]));
        // A COHERENT order rule, for the reason the coherent disjoint pair
        // exists: two of the five prefixes put `### Bold` after `### REQ-1` in
        // `## Members`, which this order forbids.
        let coherent_order = Just(yaml_map(vec![
            ("section", ystr("## Members")),
            (
                "order",
                Yaml::Sequence(vec![
                    yaml_map(vec![("element", ystr("h3")), ("match", ystr("Bold"))]),
                    yaml_map(vec![("element", ystr("h3")), ("match", ystr("REQ"))]),
                ]),
            ),
        ]));
        // The budget shape (#253): a small bound, so the over-budget path runs;
        // a span budget needs a section, so the whole-document form is always
        // per paragraph.
        let budget_rule = (optional_section(), 1..64u64, prop::bool::weighted(0.3)).prop_map(
            move |(s, max, per_paragraph)| {
                let per_paragraph = per_paragraph || s.is_none();
                let mut pairs = vec![("max_bytes", Yaml::Number(max.into()))];
                if per_paragraph {
                    pairs.push(("per", ystr("paragraph")));
                }
                with_section(s, pairs)
            },
        );
        let whole_document_count =
            (shape_element_pick(), match_pick(), count_pick()).prop_map(|(e, m, c)| {
                yaml_map(vec![
                    ("element", ystr(e)),
                    ("match", ystr(m)),
                    ("count", ystr(c)),
                ])
            });
        (
            body,
            prop_oneof![
                4 => count_rule,
                3 => disjoint_rule,
                2 => coherent_rule,
                3 => every_rule,
                2 => order_rule,
                3 => coherent_order,
                1 => whole_document_count,
                2 => budget_rule,
            ],
        )
    })
}

/// ONE input strategy per offset-bearing property, shared by the property and
/// its strategy-level reach test — so the reach test measures the same
/// input distribution CI runs (a fixed seed here, CI's seed there) (round 3, MAJOR-1: the reach tests had measured offset 0
/// while the properties drew a uniform offset that cut the prefix away).
fn section_input() -> impl Strategy<Value = ((String, Yaml), Offset)> {
    (rule_case(), offset_pick())
}

fn vocabulary_input() -> impl Strategy<Value = (String, Offset, Option<Yaml>, bool)> {
    (
        sectioned_body(),
        offset_pick(),
        frontmatter_value(),
        any::<bool>(),
    )
}

fn snapshot_input() -> impl Strategy<Value = (String, String, bool, Offset)> {
    (
        sectioned_body(),
        sectioned_body(),
        any::<bool>(),
        offset_pick(),
    )
}

/// Frontmatter for the numeric-claims comparison: the claimed field in every
/// shape, another field, none.
fn frontmatter_value() -> impl Strategy<Value = Option<Yaml>> {
    prop::sample::select(vec![
        None,
        Some("latency_ms: 12"),
        Some("latency_ms: 13"),
        Some("latency_ms: \"12 ms\""),
        Some("latency_ms: [1, 2]"),
        Some("latency_ms: 18446744073709551615"),
        Some("latency_ms: -9223372036854775808"),
        Some("latency_ms: -3"),
        Some("latency_ms: 1.5"),
        Some("latency_ms: null"),
        Some("\"é\": 1"),
        Some("other: 12"),
        Some("{}"),
    ])
    .prop_map(|yaml| yaml.map(|y| crate::yaml::from_str::<Yaml>(y).unwrap()))
}

/// A body offset draw (L4 round 3, MAJOR-1/2). Three times in four it is the
/// fence position (0 here, where the generated bodies carry no frontmatter),
/// so a body's leading section prefix survives and the section and marker
/// arms run; one time in four it is a proportional [`prop::sample::Index`]
/// into the body's char boundaries. Both halves SHRINK: `prop_oneof` prefers
/// the first arm and `Index` shrinks toward 0 — a raw `usize` taken modulo the
/// boundary count remapped to an unrelated offset every time a fragment was
/// removed, so failing bodies never shrank.
type Offset = Option<prop::sample::Index>;

fn offset_pick() -> impl Strategy<Value = Offset> {
    prop_oneof![
        3 => Just(None),
        1 => any::<prop::sample::Index>().prop_map(Some),
    ]
}

/// The char-boundary offset an [`Offset`] names in `content`.
fn offset_in(content: &str, offset: &Offset) -> usize {
    match offset {
        None => 0,
        Some(ix) => {
            let mut boundaries: Vec<usize> = content.char_indices().map(|(i, _)| i).collect();
            boundaries.push(content.len());
            boundaries[ix.index(boundaries.len())]
        }
    }
}

/// A `body_offset` that is a char boundary of `content`, by position — used by
/// the deterministic seeds to hit the middle of a body.
fn char_boundary(content: &str, pick: usize) -> usize {
    let mut boundaries: Vec<usize> = content.char_indices().map(|(i, _)| i).collect();
    boundaries.push(content.len());
    boundaries[pick % boundaries.len()]
}

const ALL_ELEMENTS: [ElementClass; 11] = [
    ElementClass::Heading,
    ElementClass::H1,
    ElementClass::H2,
    ElementClass::H3,
    ElementClass::H4,
    ElementClass::H5,
    ElementClass::H6,
    ElementClass::ListItemBoldName,
    ElementClass::ListItem,
    ElementClass::Blockquote,
    ElementClass::Line,
];

// ── Drivers (shared by seeds, reach checks and properties) ───────────────────

/// Every `markup` primitive over a body and each of its lines.
fn drive_markup_primitives(body: &str) {
    let _ = markup::heading_slugs(body);
    let _ = markup::body_links(body);
    let _ = markup::body_imports(body);
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
    let _ = crate::link::import_targets(Path::new("docs/sub/file.md"), body, body_offset);
}

/// A section rule from a YAML value, compiled and — when it compiles — run
/// over the body. Returns the findings (None when the rule did not compile).
fn drive_section_rule(rule: &Yaml, body: &str, body_offset: usize) -> Option<Vec<Finding>> {
    let raw: crate::section::RawRule = serde_yaml_ng::from_value(rule.clone()).ok()?;
    let rule = crate::section::compile_rule(raw).ok()?;
    let mut findings = Vec::new();
    crate::section::check_file(
        &[&rule],
        Path::new("docs/x.md"),
        body,
        body_offset,
        &mut findings,
    );
    Some(findings)
}

/// The vocabulary scan with the fixture register: the register's own findings
/// (the draft conflict) and the body scan with optional frontmatter for the
/// numeric claims.
fn drive_vocabulary_scan(
    body: &str,
    body_offset: usize,
    frontmatter: Option<&Yaml>,
    coinage: bool,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    crate::vocab::registry_findings(
        &REGISTER,
        Path::new(".mdatron/vocabulary.yaml"),
        &mut findings,
    );
    crate::vocab::check_file(
        &REGISTER,
        Path::new("docs/x.md"),
        body,
        body_offset,
        frontmatter,
        coinage,
        &mut findings,
    );
    findings
}

/// The code-catalog scan with the fixture catalogs.
fn drive_code_catalog_scan(body: &str, body_offset: usize) -> Vec<Finding> {
    let mut findings = Vec::new();
    crate::codecat::check_file(
        &CATALOGS,
        Path::new("docs/x.md"),
        body,
        body_offset,
        &mut findings,
    );
    findings
}

fn marker_rules() -> [MarkerRule; 3] {
    [
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
    ]
}

/// The snapshot-backed scanners — citation, link and marker `check_file` — at
/// `body_offset` over a scratch root whose governed file AND target documents
/// are hostile. The target content is written at both resolutions the
/// fragments' links reach from `docs/sub/file.md` (`../target.md` →
/// `docs/target.md`; `target.md` → `docs/sub/target.md`), and every target the
/// extraction names is pre-captured the way the pipeline's coverage pass does
/// it, so the target-side scans (heading slugs, section spans, member lookup)
/// run over hostile content. Returns the findings.
fn drive_snapshot_backed_scanners(
    body: &str,
    target: &str,
    root_relative: bool,
    body_offset: usize,
) -> Vec<Finding> {
    let root = ScratchRoot::new();
    root.write("docs/target.md", target);
    root.write("docs/sub/target.md", target);
    root.write("docs/sub/file.md", body);
    let mut snapshot = Snapshot::new(
        crate::verify::MAX_FILE_BYTES,
        crate::verify::MAX_AGGREGATE_BYTES,
    );
    let rel_file = crate::confine::confine_lexically(Path::new("docs/sub/file.md")).unwrap();
    let _ = snapshot.capture(&root.0, &rel_file);
    let mut targets = crate::cite::cited_targets(body, body_offset);
    targets.extend(crate::link::link_targets(
        Path::new("docs/sub/file.md"),
        body,
        body_offset,
        root_relative,
    ));
    targets.extend(crate::link::import_targets(
        Path::new("docs/sub/file.md"),
        body,
        body_offset,
    ));
    for doc in ["docs/target.md", "docs/sub/target.md", "docs/missing.md"] {
        if let Ok(t) = crate::confine::confine_lexically(Path::new(doc)) {
            targets.push(t);
        }
    }
    for t in &targets {
        let _ = snapshot.capture(&root.0, t);
    }
    snapshot.seal();

    let path = root.0.join("docs/sub/file.md");
    let mut findings = Vec::new();
    let mut memo = crate::memo::RefMemo::default();
    // A small register and policy so the external-link branches (#215) run
    // against the hostile bodies too, not only the well-formedness check.
    let register = crate::links::Register::new(vec![
        crate::links::Entry {
            url: "https://docs.acme.dev/".into(),
            prefix: true,
            fragments: None,
        },
        crate::links::Entry {
            url: "https://acme.dev/ref".into(),
            prefix: false,
            fragments: Some(vec!["a".into()]),
        },
    ]);
    let policy = crate::links::Policy::compile(
        crate::yaml::from_str(
            "schemes: [https]\nhosts: [acme.dev, '*.acme.dev']\nforbid_query: ['^utm_']",
        )
        .unwrap(),
    )
    .unwrap();
    crate::cite::check_file(&snapshot, &path, body, body_offset, &mut findings);
    crate::link::check_file(
        &snapshot,
        &root.0,
        &path,
        body,
        body_offset,
        &crate::links::Context {
            root_relative,
            register: Some(&register),
            policy: Some(&policy),
            ..Default::default()
        },
        &mut memo,
        &mut findings,
    );
    crate::link::check_imports(
        &snapshot,
        &root.0,
        &path,
        body,
        body_offset,
        &mut memo,
        &mut findings,
    );
    let rules = marker_rules();
    let refs: Vec<&MarkerRule> = rules.iter().collect();
    crate::marker::check_file(
        &snapshot,
        &path,
        body,
        body_offset,
        &refs,
        &mut memo,
        &mut findings,
    );
    findings
}

fn codes(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.code.as_str()).collect()
}

// ── Revert-detector seeds ────────────────────────────────────────────────────
//
// Deterministic classes: multibyte text adjacent to every syntax character the
// scanners slice around (the #48-era byte-slice class), the percent-decode
// edges, unterminated fences and code spans, control/bidi/separator characters
// — each with and without a trailing newline, since a last unterminated
// segment is its own slice path — plus the minimal inputs proptest shrank to
// when defects were seeded in review (L4 cold review MINOR-3: the seeds must
// pin what the RNG found). Each runs every driver, so a regression in any
// scanner is caught without depending on the RNG.

const SEEDS: &[&str] = &[
    "[é](x.md) `€` #— - **é**: x docs/é.md:1 <a name=\"é\">\n## é — €\n",
    "[é](x.md) `€` #— - **é**: x docs/é.md:1 <a name=\"é\">\n## é — €",
    "[t](%) [t](%2) [t](%zz) [t](%C3) [t](%E2%82) [t](%C3%A9.md#fr%C3%A4g) [t](%00.md) [t](#%)\n",
    "[t](%2)",
    "```\nno end\n``` ` ``\n~~~\n` unbalanced `` code\n",
    "```\nno end",
    "\u{FEFF}# \u{0}Heading\u{2028}\r\n- **\u{202E}name**\r[t](\u{200B}x.md)\r\n",
    "[[[]]] ((())) [t]( [t]() ![i](%) [t][r] [r]: %\n",
    "# \n#\n######\n####### seven\n#\u{301}\n- **\n* ** **\n-  **x**\n",
    "x.md:0 x.md:1- ../../../x.md:99999999 /abs.md:1-2 http://h/x.md:1 é.md:1-1\n",
    // shrunk minimal inputs from seeded defects (review round 1)
    "é",
    "\u{FEFF}docs/x.md:12-3",
    "- **é**\n",
    "- **é**",
    "🟠",
    "`🟠",
    "very café engine Layer 1 bullet lead — ADOPTER-X9 OTHER-Y2 ZZZZZ-9",
    // a reserved spelling or term ENDING the body (the find_word end bound;
    // round 2, MINOR-D)
    "Layer 1",
    "x bullet lead",
    "—",
    "### REQ-1\nZZZZZ-90¡A豈0𑌵## Others\nLayer 1",
    "12 latency_ms",
];

const SEED_TARGET: &str = "## Members\n### Bold\n### é\n\n## Others\n### REQ-1\n";

#[test]
fn seed_hostile_classes_never_panic_the_body_scanners() {
    let count_rule = yaml_map(vec![
        ("section", ystr("## Members")),
        ("element", ystr("h3")),
        ("match", ystr("REQ-[0-9]+")),
        ("count", ystr(">= 1")),
    ]);
    let disjoint_rule = yaml_map(vec![(
        "disjoint",
        Yaml::Sequence(vec![
            yaml_map(vec![
                ("section", ystr("## Members")),
                ("element", ystr("h3")),
                ("id_pattern", ystr("^(REQ-[0-9]+)$")),
            ]),
            yaml_map(vec![
                ("section", ystr("## Others")),
                ("element", ystr("heading")),
                ("id_pattern", ystr("(.*)")),
            ]),
        ]),
    )]);
    // The line-based shapes (#213/#214/#217): every hostile line is an element
    // of `line`, so these slice and locate around every seed's content.
    let every_rule = yaml_map(vec![
        ("element", ystr("line")),
        ("every", ystr("^[a-z]")),
        ("match_on", ystr("name")),
    ]);
    let order_rule = yaml_map(vec![(
        "order",
        Yaml::Sequence(vec![
            yaml_map(vec![("element", ystr("blockquote")), ("match", ystr("."))]),
            yaml_map(vec![("element", ystr("list-item")), ("match", ystr("."))]),
            yaml_map(vec![("element", ystr("line")), ("match", ystr("."))]),
        ]),
    )]);
    let fm: Yaml = crate::yaml::from_str("latency_ms: 12").unwrap();
    for body in SEEDS {
        for with_prefix in [false, true] {
            let body = if with_prefix {
                format!("{}{body}", SECTION_PREFIXES[0])
            } else {
                body.to_string()
            };
            drive_markup_primitives(&body);
            for spec in HEADING_SPECS {
                drive_section_spans(&body, spec);
            }
            let middle = char_boundary(&body, body.chars().count() / 2);
            for offset in [0, middle, body.len()] {
                drive_target_extraction(&body, offset, false);
                drive_target_extraction(&body, offset, true);
                drive_vocabulary_scan(&body, offset, Some(&fm), true);
                drive_vocabulary_scan(&body, offset, None, false);
                drive_code_catalog_scan(&body, offset);
                drive_section_rule(&count_rule, &body, offset);
                drive_section_rule(&disjoint_rule, &body, offset);
                drive_section_rule(&every_rule, &body, offset);
                drive_section_rule(&order_rule, &body, offset);
            }
            for target in [body.as_str(), SEED_TARGET] {
                drive_snapshot_backed_scanners(&body, target, false, 0);
                drive_snapshot_backed_scanners(&body, target, true, middle);
            }
        }
    }
}

// ── Reach seeds ──────────────────────────────────────────────────────────────
//
// The properties claim to cover the scanner BRANCHES, not just their
// not-found arms. These deterministic checks assert that the drivers, fed the
// shapes the strategies emit, produce the finding each branch owns — so a
// strategy that quietly stops reaching a branch (a renamed section, a target
// that no longer resolves, a register without the term) is caught here rather
// than discovered in a review (L4 cold review MAJOR-3).

#[test]
fn reach_section_rules_count_and_disjoint_arms() {
    // `match` is tested against the whole element line (so it is unanchored
    // here); `id_pattern` against the element name with the id in group 1.
    let count = yaml_map(vec![
        ("section", ystr("## Members")),
        ("element", ystr("h3")),
        ("match", ystr("REQ-[0-9]+")),
        ("count", ystr(">= 1")),
    ]);
    // Section present, no matching ids: the count violation.
    let f = drive_section_rule(&count, "## Members\n### X\n", 0).unwrap();
    assert!(codes(&f).contains(&"MDATRON-E0120"), "{f:?}");
    // Section present with a matching id: clean.
    let f = drive_section_rule(&count, SECTION_PREFIXES[0], 0).unwrap();
    assert!(f.is_empty(), "{f:?}");
    // Section absent.
    let f = drive_section_rule(&count, "# Title\n", 0).unwrap();
    assert!(codes(&f).contains(&"MDATRON-E0122"), "{f:?}");
    // Disjoint: the same id in both sections (SECTION_PREFIXES[4]).
    let disjoint = yaml_map(vec![(
        "disjoint",
        Yaml::Sequence(vec![
            yaml_map(vec![
                ("section", ystr("## Members")),
                ("element", ystr("h3")),
                ("id_pattern", ystr("^(REQ-[0-9]+)$")),
            ]),
            yaml_map(vec![
                ("section", ystr("## Others")),
                ("element", ystr("h3")),
                ("id_pattern", ystr("^(REQ-[0-9]+)$")),
            ]),
        ]),
    )]);
    let f = drive_section_rule(&disjoint, SECTION_PREFIXES[4], 0).unwrap();
    assert!(codes(&f).contains(&"MDATRON-E0121"), "{f:?}");
    // Every (#213): the one h3 in `## Members` that is not a REQ id.
    let every = yaml_map(vec![
        ("section", ystr("## Members")),
        ("element", ystr("h3")),
        ("every", ystr("REQ-[0-9]+")),
    ]);
    let f = drive_section_rule(&every, SECTION_PREFIXES[4], 0).unwrap();
    assert_eq!(codes(&f), vec!["MDATRON-E0123"], "{f:?}");
    // Order (#214): `### Bold` after `### REQ-1`, which the order forbids.
    let order = yaml_map(vec![
        ("section", ystr("## Members")),
        (
            "order",
            Yaml::Sequence(vec![
                yaml_map(vec![("element", ystr("h3")), ("match", ystr("Bold"))]),
                yaml_map(vec![("element", ystr("h3")), ("match", ystr("REQ"))]),
            ]),
        ),
    ]);
    let f = drive_section_rule(&order, SECTION_PREFIXES[4], 0).unwrap();
    assert_eq!(codes(&f), vec!["MDATRON-E0124"], "{f:?}");
    // Whole document (#217): no section, so an empty body is a count of 0.
    let not_empty = yaml_map(vec![
        ("element", ystr("line")),
        ("match", ystr(".")),
        ("count", ystr(">= 1")),
    ]);
    let f = drive_section_rule(&not_empty, "\n \n", 0).unwrap();
    assert_eq!(codes(&f), vec!["MDATRON-E0120"], "{f:?}");
    // A hostile section name survives to the compiler as a VALUE (MINOR-2).
    let hostile = yaml_map(vec![
        ("section", ystr("## \u{FEFF}\u{202E}Members\u{301}")),
        ("element", ystr("h3")),
        ("match", ystr("REQ-[0-9]+")),
        ("count", ystr(">= 1")),
    ]);
    assert!(drive_section_rule(&hostile, "## Members\n", 0).is_some());
}

#[test]
fn reach_vocabulary_reserved_anti_pattern_label_and_numeric_arms() {
    let fm: Yaml = crate::yaml::from_str("latency_ms: 13").unwrap();
    let f = drive_vocabulary_scan(
        "Layer 1 is very good; ZZZZZ-9 names it; latency is 12 ms.\n",
        0,
        Some(&fm),
        true,
    );
    let c = codes(&f);
    assert!(c.contains(&"MDATRON-E0092"), "reserved spelling: {c:?}");
    assert!(c.contains(&"MDATRON-E0093"), "anti-pattern: {c:?}");
    assert!(c.contains(&"MDATRON-E0091"), "invented label scheme: {c:?}");
    // The register's own draft conflict is reported by registry_findings.
    assert!(
        c.iter().any(|code| code.starts_with("MDATRON-W")),
        "draft conflict warning: {c:?}"
    );
    // Numeric claims: a prose number before the field name, disagreeing with
    // the frontmatter value, is the drift finding (MINOR-B).
    let f = drive_vocabulary_scan("we measured 12 latency_ms today\n", 0, Some(&fm), false);
    assert!(
        codes(&f).contains(&"MDATRON-E0094"),
        "numeric-claim drift: {f:?}"
    );
    let clean = drive_vocabulary_scan("conformance engine\n", 0, Some(&fm), false);
    assert!(
        clean.is_empty() || !codes(&clean).contains(&"MDATRON-E0092"),
        "{clean:?}"
    );
}

// ── Strategy-level reach ─────────────────────────────────────────────────────
//
// The reach seeds above feed hand-written inputs to the drivers. These run the
// PROPERTIES' OWN STRATEGIES at a fixed seed and count the findings, so a
// strategy that quietly stops reaching a branch fails here — the gap round 2
// found (MAJOR-A: the disjoint comparison ran 0 times in 1024 cases while
// every hand-written reach seed was green). Fixed seed, fixed case count:
// deterministic, independent of PROPTEST_RNG_SEED.

fn count_over_strategy<S: Strategy>(strategy: S, cases: u32, tally: impl Fn(S::Value)) {
    let config = ProptestConfig {
        cases,
        max_shrink_iters: 0,
        failure_persistence: None,
        ..ProptestConfig::default()
    };
    let rng = TestRng::from_seed(RngAlgorithm::ChaCha, &[7u8; 32]);
    let mut runner = TestRunner::new_with_rng(config, rng);
    runner
        .run(&strategy, |v| {
            tally(v);
            Ok(())
        })
        .unwrap_or_else(|e| panic!("strategy run failed: {e}"));
}

#[test]
fn reach_strategy_section_rules_hit_the_count_and_disjoint_arms() {
    use std::cell::Cell;
    let (compiled, e0120, e0121, e0122) = (Cell::new(0), Cell::new(0), Cell::new(0), Cell::new(0));
    let (e0123, e0124) = (Cell::new(0), Cell::new(0));
    let (e0125, e0126) = (Cell::new(0), Cell::new(0));
    count_over_strategy(section_input(), 768, |((body, rule), off)| {
        if let Some(f) = drive_section_rule(&rule, &body, offset_in(&body, &off)) {
            compiled.set(compiled.get() + 1);
            // Tallied per CASE, not per finding: one every rule over one body
            // can emit a dozen E0123, which would meet a floor on its own
            // (cold review round 2).
            for c in codes(&f)
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
            {
                match c {
                    "MDATRON-E0120" => e0120.set(e0120.get() + 1),
                    "MDATRON-E0121" => e0121.set(e0121.get() + 1),
                    "MDATRON-E0122" => e0122.set(e0122.get() + 1),
                    "MDATRON-E0123" => e0123.set(e0123.get() + 1),
                    "MDATRON-E0124" => e0124.set(e0124.get() + 1),
                    "MDATRON-E0125" => e0125.set(e0125.get() + 1),
                    "MDATRON-E0126" => e0126.set(e0126.get() + 1),
                    _ => {}
                }
            }
        }
    });
    let (compiled, e0120, e0121, e0122) = (compiled.get(), e0120.get(), e0121.get(), e0122.get());
    let (e0123, e0124) = (e0123.get(), e0124.get());
    let (e0125, e0126) = (e0125.get(), e0126.get());
    eprintln!("section rules over 768 cases: compiled {compiled}, E0120 {e0120}, E0121 {e0121}, E0122 {e0122}");
    eprintln!(
        "section rules over 768 cases: E0123 {e0123}, E0124 {e0124}, E0125 {e0125}, E0126 {e0126}"
    );
    assert!(
        compiled >= 384,
        "most generated rules must compile (got {compiled}/768)"
    );
    assert!(
        e0120 >= 15,
        "the count arm must fail sometimes (E0120 {e0120})"
    );
    assert!(
        e0121 >= 15,
        "the disjoint comparison must find overlaps sometimes (E0121 {e0121})"
    );
    assert!(
        e0122 >= 10,
        "the section-not-found arm must run too (E0122 {e0122})"
    );
    assert!(
        e0123 >= 15,
        "the every arm must find mismatched elements sometimes (E0123 {e0123})"
    );
    assert!(
        e0124 >= 8,
        "the order arm must find out-of-order elements sometimes (E0124 {e0124})"
    );
    // #253: the budget arm's small bounds put both findings in reach.
    assert!(
        e0125 >= 8,
        "the budget arm must find over-budget sections sometimes (E0125 {e0125})"
    );
    assert!(
        e0126 >= 8,
        "the budget arm must find over-budget paragraphs sometimes (E0126 {e0126})"
    );
}

#[test]
fn reach_strategy_vocabulary_hits_reserved_anti_pattern_and_numeric_arms() {
    use std::cell::Cell;
    let (e0092, e0093, e0094) = (Cell::new(0), Cell::new(0), Cell::new(0));
    count_over_strategy(vocabulary_input(), 512, |(body, off, fm, coinage)| {
        for c in codes(&drive_vocabulary_scan(
            &body,
            offset_in(&body, &off),
            fm.as_ref(),
            coinage,
        )) {
            match c {
                "MDATRON-E0092" => e0092.set(e0092.get() + 1),
                "MDATRON-E0093" => e0093.set(e0093.get() + 1),
                "MDATRON-E0094" => e0094.set(e0094.get() + 1),
                _ => {}
            }
        }
    });
    let (e0092, e0093, e0094) = (e0092.get(), e0093.get(), e0094.get());
    eprintln!("vocabulary over 512 cases: E0092 {e0092}, E0093 {e0093}, E0094 {e0094}");
    assert!(e0092 >= 20, "reserved spellings (E0092 {e0092})");
    assert!(e0093 >= 20, "anti-patterns (E0093 {e0093})");
    assert!(e0094 >= 5, "numeric-claim drift (E0094 {e0094})");
}

#[test]
fn reach_strategy_snapshot_backed_scanners_resolve_targets_and_members() {
    use std::cell::Cell;
    // Resolved members are asserted on a hand-built input in
    // `reach_snapshot_backed_target_side_arms` ("Bold IS a member"): a
    // strategy-level proxy (marker lines vs E0112 counts) could pass with no
    // lookup at all (round 3, MINOR-4), so none is claimed here.
    let (e0111, e0112, e0114) = (Cell::new(0), Cell::new(0), Cell::new(0));
    count_over_strategy(
        snapshot_input(),
        48,
        |(body, target, root_relative, off)| {
            let f = drive_snapshot_backed_scanners(
                &body,
                &target,
                root_relative,
                offset_in(&body, &off),
            );
            let c = codes(&f);
            e0111.set(e0111.get() + c.iter().filter(|c| **c == "MDATRON-E0111").count());
            e0112.set(e0112.get() + c.iter().filter(|c| **c == "MDATRON-E0112").count());
            e0114.set(e0114.get() + c.iter().filter(|c| **c == "MDATRON-E0114").count());
        },
    );
    let (e0111, e0112, e0114) = (e0111.get(), e0112.get(), e0114.get());
    eprintln!("snapshot-backed over 48 cases: E0111 {e0111}, E0112 {e0112}, E0114 {e0114}");
    assert!(
        e0111 >= 1,
        "a cross-file dead anchor on a captured target (E0111 {e0111})"
    );
    assert!(
        e0112 >= 1,
        "a marker name that is not a member (E0112 {e0112})"
    );
    assert!(e0114 >= 1, "a target without the section (E0114 {e0114})");
}

#[test]
fn reach_code_catalog_orphan_arm() {
    let f = drive_code_catalog_scan("see ADOPTER-X9 and ADOPTER-E0001 and OTHER-Y2\n", 0);
    let c = codes(&f);
    assert!(
        c.contains(&"MDATRON-E0113"),
        "orphan under the comprehensive catalog: {c:?}"
    );
    assert_eq!(
        c.iter().filter(|c| **c == "MDATRON-E0113").count(),
        1,
        "declared and non-comprehensive tokens are not orphans: {c:?}"
    );
}

#[test]
fn reach_snapshot_backed_target_side_arms() {
    // Cross-file link to a captured target with a dead anchor; a live anchor;
    // a dead target; a marker whose name is not among the target's members;
    // one that is; a citation past the target's line count.
    let body = "[a](../target.md#nope) [b](../target.md#members) [c](../gone.md)\n\
                - **Nope**: x\n- **Bold**: y\ndocs/target.md:999 docs/target.md:1\n";
    let f = drive_snapshot_backed_scanners(body, SEED_TARGET, false, 0);
    let c = codes(&f);
    assert!(
        c.contains(&"MDATRON-E0111"),
        "dead anchor on a captured target: {c:?}"
    );
    assert!(c.contains(&"MDATRON-E0110"), "dead target: {c:?}");
    assert!(
        c.contains(&"MDATRON-E0112"),
        "marker name not a member: {c:?}"
    );
    assert_eq!(
        c.iter().filter(|c| **c == "MDATRON-E0112").count(),
        1,
        "Bold IS a member: {c:?}"
    );
    assert!(
        c.contains(&"MDATRON-E0101"),
        "citation line out of range: {c:?}"
    );
    assert!(
        !c.contains(&"MDATRON-E0114"),
        "the target carries ## Members: {c:?}"
    );
    // The section-not-found arm, when the target lacks the section.
    let f = drive_snapshot_backed_scanners("- **Bold**: y\n", "# Title\n", false, 0);
    assert!(codes(&f).contains(&"MDATRON-E0114"), "{f:?}");
    // At a non-zero body offset the same findings appear (line math holds).
    let fm = "---\nx: 1\n---\n";
    let content = format!("{fm}{body}");
    let f = drive_snapshot_backed_scanners(&content, SEED_TARGET, false, fm.len());
    let c = codes(&f);
    assert!(
        c.contains(&"MDATRON-E0111") && c.contains(&"MDATRON-E0112"),
        "{c:?}"
    );
}

// ── Properties ───────────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_markup_primitives_never_panic(body in sectioned_body()) {
        drive_markup_primitives(&body);
    }

    #[test]
    fn prop_markup_primitives_never_panic_on_arbitrary_text(body in any::<String>()) {
        drive_markup_primitives(&body);
    }

    #[test]
    fn prop_section_spans_never_panic(body in sectioned_body(), spec in heading_spec()) {
        drive_section_spans(&body, &spec);
    }

    #[test]
    fn prop_target_extraction_never_panics(
        body in sectioned_body(),
        off in offset_pick(),
        root_relative in any::<bool>(),
    ) {
        drive_target_extraction(&body, offset_in(&body, &off), root_relative);
    }

    #[test]
    fn prop_section_rules_never_panic(((body, rule), off) in section_input()) {
        drive_section_rule(&rule, &body, offset_in(&body, &off));
    }

    #[test]
    fn prop_vocabulary_scan_never_panics((body, off, frontmatter, coinage) in vocabulary_input()) {
        drive_vocabulary_scan(&body, offset_in(&body, &off), frontmatter.as_ref(), coinage);
    }

    #[test]
    fn prop_code_catalog_scan_never_panics(body in sectioned_body(), off in offset_pick()) {
        drive_code_catalog_scan(&body, offset_in(&body, &off));
    }
}

/// Filesystem-backed: a scratch root per case, so a HARD case cap — through an
/// explicit `TestRunner`, because a `#![proptest_config(cases)]` pin inside
/// `proptest!` is re-contextualized from `PROPTEST_CASES` and does not hold
/// under CI's env (MAJOR-2). `PROPTEST_RNG_SEED` still applies through
/// `ProptestConfig::default()`.
#[test]
fn prop_snapshot_backed_scanners_never_panic() {
    let config = ProptestConfig {
        cases: 32,
        // The default shrink budget derives from `cases` (×4 = 128 iterations),
        // which left seeded failures half-shrunk (round 2, MINOR-C); the pure
        // properties get 4096 from PROPTEST_CASES=1024, so match that.
        max_shrink_iters: 4096,
        source_file: Some(file!()),
        ..ProptestConfig::default()
    };
    let mut runner = TestRunner::new(config);
    runner
        .run(&snapshot_input(), |(body, target, root_relative, off)| {
            drive_snapshot_backed_scanners(&body, &target, root_relative, offset_in(&body, &off));
            Ok(())
        })
        .unwrap_or_else(|e| panic!("snapshot-backed scanner property failed: {e}"));
}
