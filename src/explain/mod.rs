//! Embedded `mdatron explain CODE` catalog.
//!
//! v0.1.x catalog: MDATRON-E0001, E0002, E0010, E0011, E0012, E0050, E0060, E0070,
//! E0080, W0040; route family E0030, E0031, E0032, W0041; pin family E0061, E0062, E0063, W0042, L0001; vocabulary family E0090-E0094; citation family E0100, E0101; link family E0110, E0111; marker family E0112; code-catalog family E0113; section-structural family E0120, E0121; DSL rule field-reference validation E0021. The catalog grows by one entry per newly-emitted code per the
//! Phase 0 DESIGN open question #2 SO disposition (2026-06-02); the
//! path-confinement trio (E0010/E0011/E0012) landed with the confinement
//! rework (the path-confinement defect issue in this tracker).
//!
//! Pages are author-Markdown with four required structural elements per the
//! Phase 1a behavioral spec:
//!   - `**Severity:**` line
//!   - `**Introduced in:**` line
//!   - `## What this means` section
//!   - `## How to fix` section
//!
//! Two surfaces:
//!   - [`lookup`] returns the raw markdown for `mdatron explain <code>` TTY
//!     output (default operator-facing mode).
//!   - [`lookup_structured`] returns a parsed [`ExplainPage`] for
//!     `mdatron explain --json <code>` agent-loop consumers + downstream
//!     tooling.

use serde::{Deserialize, Serialize};

const E0001: &str = include_str!("MDATRON-E0001.md");
const E0002: &str = include_str!("MDATRON-E0002.md");
const E0003: &str = include_str!("MDATRON-E0003.md");
const E0010: &str = include_str!("MDATRON-E0010.md");
const E0011: &str = include_str!("MDATRON-E0011.md");
const E0012: &str = include_str!("MDATRON-E0012.md");
const E0050: &str = include_str!("MDATRON-E0050.md");
const E0060: &str = include_str!("MDATRON-E0060.md");
const E0070: &str = include_str!("MDATRON-E0070.md");
const E0080: &str = include_str!("MDATRON-E0080.md");
const W0040: &str = include_str!("MDATRON-W0040.md");
const E0030: &str = include_str!("MDATRON-E0030.md");
const E0031: &str = include_str!("MDATRON-E0031.md");
const E0032: &str = include_str!("MDATRON-E0032.md");
const W0041: &str = include_str!("MDATRON-W0041.md");
const E0061: &str = include_str!("MDATRON-E0061.md");
const E0062: &str = include_str!("MDATRON-E0062.md");
const E0063: &str = include_str!("MDATRON-E0063.md");
const W0042: &str = include_str!("MDATRON-W0042.md");
const W0043: &str = include_str!("MDATRON-W0043.md");
const W0044: &str = include_str!("MDATRON-W0044.md");
const W0045: &str = include_str!("MDATRON-W0045.md");
const W0046: &str = include_str!("MDATRON-W0046.md");
const W0047: &str = include_str!("MDATRON-W0047.md");
const W0048: &str = include_str!("MDATRON-W0048.md");
const W0049: &str = include_str!("MDATRON-W0049.md");
const L0001: &str = include_str!("MDATRON-L0001.md");
const E0090: &str = include_str!("MDATRON-E0090.md");
const E0091: &str = include_str!("MDATRON-E0091.md");
const E0092: &str = include_str!("MDATRON-E0092.md");
const E0093: &str = include_str!("MDATRON-E0093.md");
const E0094: &str = include_str!("MDATRON-E0094.md");
const E0100: &str = include_str!("MDATRON-E0100.md");
const E0101: &str = include_str!("MDATRON-E0101.md");
const E0110: &str = include_str!("MDATRON-E0110.md");
const E0111: &str = include_str!("MDATRON-E0111.md");
const E0112: &str = include_str!("MDATRON-E0112.md");
const E0113: &str = include_str!("MDATRON-E0113.md");
const E0021: &str = include_str!("MDATRON-E0021.md");
const E0022: &str = include_str!("MDATRON-E0022.md");
const W0050: &str = include_str!("MDATRON-W0050.md");
const E0120: &str = include_str!("MDATRON-E0120.md");
const E0121: &str = include_str!("MDATRON-E0121.md");

/// Structured shape of an explain page. Surfaces the required fields named
/// in the Phase 1a behavioral spec. Used by `mdatron explain --json <code>`.
///
/// Per crosslink #13 DE/F1 (explain-page format schema): this struct is the
/// machine-readable contract; the markdown form is the operator-readable
/// rendering of the same content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplainPage {
    pub code: String,
    pub severity: String,
    pub status: String,
    pub introduced_in: String,
    pub what_this_means: String,
    pub how_to_fix: String,
    /// Raw markdown body. Operators who want the full page can read this.
    pub markdown: String,
}

/// Look up the embedded explain page markdown for a code. Returns `None` if
/// the code is not in the v0.1.0 baseline catalog.
pub fn lookup(code: &str) -> Option<&'static str> {
    match code {
        "MDATRON-E0001" => Some(E0001),
        "MDATRON-E0002" => Some(E0002),
        "MDATRON-E0003" => Some(E0003),
        "MDATRON-E0010" => Some(E0010),
        "MDATRON-E0011" => Some(E0011),
        "MDATRON-E0012" => Some(E0012),
        "MDATRON-E0050" => Some(E0050),
        "MDATRON-E0060" => Some(E0060),
        "MDATRON-E0070" => Some(E0070),
        "MDATRON-E0080" => Some(E0080),
        "MDATRON-W0040" => Some(W0040),
        "MDATRON-E0030" => Some(E0030),
        "MDATRON-E0031" => Some(E0031),
        "MDATRON-E0032" => Some(E0032),
        "MDATRON-W0041" => Some(W0041),
        "MDATRON-E0061" => Some(E0061),
        "MDATRON-E0062" => Some(E0062),
        "MDATRON-E0063" => Some(E0063),
        "MDATRON-W0042" => Some(W0042),
        "MDATRON-W0043" => Some(W0043),
        "MDATRON-W0044" => Some(W0044),
        "MDATRON-W0045" => Some(W0045),
        "MDATRON-W0046" => Some(W0046),
        "MDATRON-W0047" => Some(W0047),
        "MDATRON-W0048" => Some(W0048),
        "MDATRON-W0049" => Some(W0049),
        "MDATRON-L0001" => Some(L0001),
        "MDATRON-E0090" => Some(E0090),
        "MDATRON-E0091" => Some(E0091),
        "MDATRON-E0092" => Some(E0092),
        "MDATRON-E0093" => Some(E0093),
        "MDATRON-E0094" => Some(E0094),
        "MDATRON-E0021" => Some(E0021),
        "MDATRON-E0022" => Some(E0022),
        "MDATRON-W0050" => Some(W0050),
        "MDATRON-E0100" => Some(E0100),
        "MDATRON-E0101" => Some(E0101),
        "MDATRON-E0110" => Some(E0110),
        "MDATRON-E0111" => Some(E0111),
        "MDATRON-E0112" => Some(E0112),
        "MDATRON-E0113" => Some(E0113),
        "MDATRON-E0120" => Some(E0120),
        "MDATRON-E0121" => Some(E0121),
        _ => None,
    }
}

/// Render the compact one-liner form of an explain page. Suitable for
/// PostToolUse hook context budgets + high-density agent-loop scenarios.
/// Per crosslink #13 AIE/F2.
///
/// Form: `<code> <severity>: <summary> — <first-sentence-of-how-to-fix>`.
/// Returns `None` when the code is not in the catalog.
pub fn lookup_compact(code: &str) -> Option<String> {
    let page = lookup_structured(code)?;
    // Summary derived from the H1 title — the part after the em-dash.
    let summary = page
        .markdown
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("# "))
        .and_then(|l| l.split_once('\u{2014}').or_else(|| l.split_once("-")))
        .map(|(_, after)| after.trim().to_string())
        .unwrap_or_else(|| page.code.clone());
    // First sentence of "How to fix" (up to first . or newline).
    let first_sentence = page
        .how_to_fix
        .split(['.', '\n'])
        .next()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    Some(format!(
        "{} {}: {} — {}",
        page.code, page.severity, summary, first_sentence
    ))
}

/// Look up + parse the explain page into the structured [`ExplainPage`] form.
/// Returns `None` if the code is not in the catalog OR if the page is missing
/// any required structural element (the unit test catches authoring drift).
pub fn lookup_structured(code: &str) -> Option<ExplainPage> {
    let markdown = lookup(code)?;
    let severity = extract_field(markdown, "Severity")?;
    let status = extract_field(markdown, "Status")?;
    let introduced_in = extract_field(markdown, "Introduced in")?;
    let what_this_means = extract_section(markdown, "What this means")?;
    let how_to_fix = extract_section(markdown, "How to fix")?;
    Some(ExplainPage {
        code: code.to_string(),
        severity,
        status,
        introduced_in,
        what_this_means,
        how_to_fix,
        markdown: markdown.to_string(),
    })
}

/// Returns true when the code is in mdatron's `MDATRON-` namespace
/// (regardless of whether it is in the explain catalog yet).
pub fn is_mdatron_namespace(code: &str) -> bool {
    code.starts_with("MDATRON-")
}

/// The published code catalog as `(code, summary)` pairs, sorted by code (#117,
/// vsdd W4 — powers `mdatron explain --list`). Sourced from the golden
/// `schema/code-catalog.json` so the list can never drift from the contract the
/// tripwires enforce.
pub fn catalog() -> Vec<(String, String)> {
    const CODE_CATALOG_JSON: &str = include_str!("../../schema/code-catalog.json");
    let map: std::collections::BTreeMap<String, String> =
        serde_json::from_str(CODE_CATALOG_JSON).unwrap_or_default();
    map.into_iter().collect()
}

/// Migration-note pairs surfaced when an operator searches for a code
/// whose semantic meaning has SHIFTED across emission sites during the
/// bootstrap period. Each entry pairs `(code, prior-meaning-context)`
/// — the operator who recalls the prior meaning sees a note pointing at
/// the current semantic.
///
/// Per crosslink #12 UX/F1: an operator who saw "MDATRON-E0001:
/// frontmatter-schema-violation" in pre-Phase-1 bootstrap output, then
/// later sees "MDATRON-E0001: frontmatter-parse-failed", needs a
/// discoverable bridge — searching old logs surfaces correct-but-stale
/// guidance.
///
/// Post-v0.1.0, code semantics become semver-stable per Rust's
/// E0000-series convention; this table grows by one entry per future
/// rename event with the previous meaning preserved as the durable
/// migration record.
pub const MIGRATION_NOTES: &[(&str, &str)] = &[(
    "MDATRON-E0001",
    "Pre-Phase-1 bootstrap snapshots emitted this code for \
         frontmatter-schema-violation; from Phase 1 onward, E0001 is \
         exclusively frontmatter-parse-failed and schema-violation \
         moved to MDATRON-E0050. If you saw E0001 in pre-Phase-1 \
         output and the message body said 'schema-violation', see \
         `mdatron explain MDATRON-E0050`.",
)];

/// Look up the migration note for a code, if one exists.
pub fn migration_note(code: &str) -> Option<&'static str> {
    MIGRATION_NOTES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, note)| *note)
}

/// Extract the value of a `**<field>:** <value>` line. Returns the trimmed
/// value or `None` if the field marker isn't present.
fn extract_field(markdown: &str, field: &str) -> Option<String> {
    let marker = format!("**{field}:**");
    markdown.lines().find_map(|line| {
        line.find(&marker)
            .map(|i| line[i + marker.len()..].trim().to_string())
    })
}

/// Extract the prose body under a `## <heading>` H2 section. Returns the
/// trimmed body up to the next H2 / H1 / end-of-file.
fn extract_section(markdown: &str, heading: &str) -> Option<String> {
    let marker = format!("## {heading}");
    let start = markdown.find(&marker)?;
    let after_heading = start + marker.len();
    let body_start = markdown[after_heading..].find('\n')? + after_heading + 1;
    let rest = &markdown[body_start..];
    let end = rest
        .find("\n## ")
        .or_else(|| rest.find("\n# "))
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASELINE: &[&str] = &[
        "MDATRON-E0001",
        "MDATRON-E0002",
        "MDATRON-E0003",
        "MDATRON-E0010",
        "MDATRON-E0011",
        "MDATRON-E0012",
        "MDATRON-E0050",
        "MDATRON-E0060",
        "MDATRON-E0070",
        "MDATRON-E0080",
        "MDATRON-W0040",
        "MDATRON-E0030",
        "MDATRON-E0031",
        "MDATRON-E0032",
        "MDATRON-W0041",
        "MDATRON-E0061",
        "MDATRON-E0062",
        "MDATRON-E0063",
        "MDATRON-W0042",
        "MDATRON-W0043",
        "MDATRON-W0044",
        "MDATRON-W0045",
        "MDATRON-W0046",
        "MDATRON-W0047",
        "MDATRON-W0048",
        "MDATRON-W0049",
        "MDATRON-W0050",
        "MDATRON-L0001",
        "MDATRON-E0090",
        "MDATRON-E0091",
        "MDATRON-E0092",
        "MDATRON-E0093",
        "MDATRON-E0094",
        "MDATRON-E0021",
        "MDATRON-E0022",
        "MDATRON-E0100",
        "MDATRON-E0101",
        "MDATRON-E0110",
        "MDATRON-E0111",
        "MDATRON-E0112",
        "MDATRON-E0113",
        "MDATRON-E0120",
        "MDATRON-E0121",
    ];

    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    /// Collect every `MDATRON-<L><NNNN>` literal in `content`.
    fn mdatron_codes_in(content: &str) -> Vec<String> {
        let bytes = content.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while let Some(rel) = content[i..].find("MDATRON-") {
            let start = i + rel;
            let tail = start + "MDATRON-".len();
            if tail + 5 <= bytes.len() {
                let letter = bytes[tail];
                let digits = &bytes[tail + 1..tail + 5];
                if matches!(letter, b'E' | b'L' | b'W') && digits.iter().all(u8::is_ascii_digit) {
                    out.push(content[start..tail + 5].to_string());
                }
            }
            i = tail;
        }
        out
    }

    fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                rs_files(&p, out);
            } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
                out.push(p);
            }
        }
    }

    // CONTRACT-STABILITY TRIPWIRE (#90): every MDATRON-* code literal in
    // PRODUCTION source (the region before the file's `#[cfg(test)]` module)
    // resolves in the explain catalog. Emitting or referencing a code without
    // a page fails here — the "every emitted code resolves in explain" criterion
    // (DESIGN § Diagnostics are a versioned contract).
    #[test]
    fn every_production_code_resolves_in_explain() {
        let mut files = Vec::new();
        rs_files(&repo_root().join("src"), &mut files);
        let mut missing: Vec<(String, String)> = Vec::new();
        for f in files {
            // codes.rs is the reserved-range registry: it names range
            // representatives (E0020, E0040, ...) that are reserved-but-not-yet-
            // assigned and so have no page — excluded like the reserved-codes
            // lint does for the same reason.
            if f.ends_with("codes.rs") {
                continue;
            }
            let content = std::fs::read_to_string(&f).unwrap_or_default();
            // Production region only: test fixtures use non-emitted codes.
            let prod = content.split("#[cfg(test)]").next().unwrap_or(&content);
            for code in mdatron_codes_in(prod) {
                if lookup(&code).is_none() {
                    missing.push((f.display().to_string(), code));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "production codes with no explain page (add the page + BASELINE entry): {missing:?}"
        );
    }

    // TRIPWIRE (#90): the committed code-semantics catalog
    // (`schema/code-catalog.json`, code -> one-line summary from each page's
    // H1) is current. Changing a code's MEANING (its page summary) without
    // regenerating the catalog FAILS — the "seeded code-meaning change fails
    // CI" criterion; the regen is the intentional acknowledgment.
    #[test]
    fn code_semantics_catalog_is_current() {
        let dir = repo_root().join("src").join("explain");
        let mut built: std::collections::BTreeMap<String, String> = Default::default();
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if !(name.starts_with("MDATRON-") && name.ends_with(".md")) {
                continue;
            }
            let first = std::fs::read_to_string(&p).unwrap();
            let h1 = first.lines().next().unwrap_or("");
            let (code, summary) = h1
                .strip_prefix("# ")
                .and_then(|s| s.split_once(" \u{2014} "))
                .unwrap_or_else(|| {
                    panic!("page {name} H1 must be `# CODE \u{2014} summary`; got {h1:?}")
                });
            built.insert(code.trim().to_string(), summary.trim().to_string());
        }
        let golden: std::collections::BTreeMap<String, String> = serde_json::from_str(
            &std::fs::read_to_string(repo_root().join("schema/code-catalog.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            built, golden,
            "code-semantics drift: regenerate schema/code-catalog.json (a code's meaning changed)"
        );
    }

    // Consistency: the BASELINE list, the explain-page files, and the golden
    // catalog are the same code set — adding a code requires touching all three.
    #[test]
    fn baseline_pages_and_catalog_are_the_same_code_set() {
        use std::collections::BTreeSet;
        let baseline: BTreeSet<&str> = BASELINE.iter().copied().collect();
        let mut pages: BTreeSet<String> = Default::default();
        for e in std::fs::read_dir(repo_root().join("src/explain"))
            .unwrap()
            .flatten()
        {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some(code) = name.strip_prefix("").and_then(|n| n.strip_suffix(".md")) {
                if code.starts_with("MDATRON-") {
                    pages.insert(code.to_string());
                }
            }
        }
        let pages_ref: BTreeSet<&str> = pages.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            baseline, pages_ref,
            "BASELINE and explain-page files must match exactly"
        );
        let golden: std::collections::BTreeMap<String, String> = serde_json::from_str(
            &std::fs::read_to_string(repo_root().join("schema/code-catalog.json")).unwrap(),
        )
        .unwrap();
        let golden_codes: BTreeSet<&str> = golden.keys().map(|s| s.as_str()).collect();
        assert_eq!(
            baseline, golden_codes,
            "BASELINE and the golden catalog must match"
        );
    }

    #[test]
    fn every_baseline_code_has_a_catalog_page() {
        for code in BASELINE {
            let page = lookup(code).unwrap_or_else(|| panic!("missing catalog page for {code}"));
            assert!(
                page.contains("## What this means"),
                "{code} page missing required '## What this means' heading"
            );
            assert!(
                page.contains("## How to fix"),
                "{code} page missing required '## How to fix' heading"
            );
            assert!(
                page.contains("**Severity:**"),
                "{code} page missing required '**Severity:**' frontline"
            );
            assert!(
                page.contains("**Introduced in:**"),
                "{code} page missing required '**Introduced in:**' frontline"
            );
        }
    }

    #[test]
    fn every_baseline_code_parses_into_structured_explain_page() {
        // Per crosslink #13 DE/F1: parser-level catalog integrity.
        for code in BASELINE {
            let parsed =
                lookup_structured(code).unwrap_or_else(|| panic!("{code} failed to parse"));
            assert_eq!(parsed.code, *code);
            assert!(!parsed.severity.is_empty(), "{code} severity empty");
            assert!(!parsed.status.is_empty(), "{code} status empty");
            assert!(
                !parsed.introduced_in.is_empty(),
                "{code} introduced_in empty"
            );
            assert!(
                parsed.what_this_means.len() >= 30,
                "{code} 'what this means' section under minimum prose length"
            );
            assert!(
                parsed.how_to_fix.len() >= 30,
                "{code} 'how to fix' section under minimum prose length"
            );
        }
    }

    #[test]
    fn unknown_code_returns_none() {
        // E9999 is constructed at runtime to keep the literal out of source
        // (the reserved-codes lint at tests/phase_1_contracts.rs
        // walks .rs files looking for non-reserved MDATRON-Ennnn literals).
        let unreserved = format!("{}-{}", "MDATRON", "E9999");
        assert!(lookup(&unreserved).is_none());
        assert!(lookup_structured(&unreserved).is_none());
        assert!(lookup("").is_none());
    }

    #[test]
    fn case_sensitive_lookup() {
        // Codes are case-sensitive — adopters paste codes verbatim from
        // diagnostic output. Lowercase lookups fail.
        assert!(lookup("mdatron-e0001").is_none());
    }

    #[test]
    fn is_mdatron_namespace_distinguishes_prefix() {
        assert!(is_mdatron_namespace("MDATRON-E0001"));
        // Constructed to avoid a literal "VSDD-" in source for the lint.
        let other_ns = format!("{}{}-E0001", "VS", "DD");
        assert!(!is_mdatron_namespace(&other_ns));
    }
}
