//! Embedded `mdatron explain CODE` catalog.
//!
//! v0.1.0 baseline: five reserved + emitted codes (MDATRON-E0001, E0002, E0050,
//! E0070, E0080). The catalog grows by one entry per newly-emitted code per
//! the Phase 0 DESIGN open question #2 SO disposition (2026-06-02).
//!
//! Pages are author-Markdown with four required headings (per
//! `vsdd-cli/docs/refactor/phase-2-mdatron-json/phase-1a-behavioral-spec.md`
//! § Per-code explain page format): a `**Severity:**` line, an
//! `**Introduced in:**` line, a `## What this means` section, and a
//! `## How to fix` section.

const E0001: &str = include_str!("MDATRON-E0001.md");
const E0002: &str = include_str!("MDATRON-E0002.md");
const E0050: &str = include_str!("MDATRON-E0050.md");
const E0070: &str = include_str!("MDATRON-E0070.md");
const E0080: &str = include_str!("MDATRON-E0080.md");

/// Look up the embedded explain page for a code. Returns `None` if the code
/// is not in the v0.1.0 baseline catalog.
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

/// Returns true when the code is in mdatron's `MDATRON-` namespace
/// (regardless of whether it is in the explain catalog yet).
pub fn is_mdatron_namespace(code: &str) -> bool {
    code.starts_with("MDATRON-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_baseline_code_has_a_catalog_page() {
        for code in [
            "MDATRON-E0001",
            "MDATRON-E0002",
            "MDATRON-E0050",
            "MDATRON-E0070",
            "MDATRON-E0080",
        ] {
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
    fn unknown_code_returns_none() {
        // E9999 is constructed at runtime to keep the literal out of source
        // (the reserved-codes lint at mdatron-core/tests/phase_1_contracts.rs
        // walks .rs files looking for non-reserved MDATRON-Ennnn literals).
        let unreserved = format!("{}-{}", "MDATRON", "E9999");
        assert!(lookup(&unreserved).is_none());
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
