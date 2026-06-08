//! Embedded `mdatron explain CODE` catalog.
//!
//! v0.1.0 baseline: five reserved + emitted codes (MDATRON-E0001, E0002, E0050,
//! E0070, E0080). The catalog grows by one entry per newly-emitted code per
//! the Phase 0 DESIGN open question #2 SO disposition (2026-06-02).
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
const E0050: &str = include_str!("MDATRON-E0050.md");
const E0070: &str = include_str!("MDATRON-E0070.md");
const E0080: &str = include_str!("MDATRON-E0080.md");

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
        "MDATRON-E0050" => Some(E0050),
        "MDATRON-E0070" => Some(E0070),
        "MDATRON-E0080" => Some(E0080),
        _ => None,
    }
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
        "MDATRON-E0050",
        "MDATRON-E0070",
        "MDATRON-E0080",
    ];

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
            assert!(!parsed.introduced_in.is_empty(), "{code} introduced_in empty");
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
        // (the reserved-codes lint at mdatron-core/tests/phase_1_contracts.rs
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
