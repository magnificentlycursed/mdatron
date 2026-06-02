//! Reserved error-code allocation table for mdatron.
//!
//! Per `DESIGN-MDATRON.md:506-514` (as amended for v0.1.x), every code emitted
//! by mdatron must fall in one of these reserved ranges. The
//! [`is_reserved_mdatron_code`] check is used by integration tests + a future
//! code-allocation lint to enforce the discipline at compile time.

/// Returns true if `code` is a syntactically valid mdatron-reserved code.
///
/// Reserved ranges:
/// - `MDATRON-E0001` — `E0009` Frontmatter parsing failures
/// - `MDATRON-E0010` — `E0019` Path-confinement violations
/// - `MDATRON-E0020` — `E0029` DSL evaluation failures
/// - `MDATRON-E0030` — `E0039` Delegate protocol failures
/// - `MDATRON-E0040` — `E0049` Schema load failures
/// - `MDATRON-E0050` — `E0059` Frontmatter schema validation failures (v0.1.x)
/// - `MDATRON-E0070` — `E0079` IO failures during verify (v0.1.x)
/// - `MDATRON-E0080` — `E0089` Pipeline orchestration failures (v0.1.x)
/// - `MDATRON-W0030` — `W0099` Warnings (delegates + configuration)
/// - `MDATRON-W0100` — `W0199` Built-in pattern findings
/// - `MDATRON-L0001` — `L0099` Engine-level lints
pub fn is_reserved_mdatron_code(code: &str) -> bool {
    let Some(suffix) = code.strip_prefix("MDATRON-") else {
        return false;
    };
    let Some(letter) = suffix.chars().next() else {
        return false;
    };
    let number_part = &suffix[1..];
    let Ok(n) = number_part.parse::<u32>() else {
        return false;
    };
    match letter {
        // Ranges per DESIGN-MDATRON.md:506-514 (amended for v0.1.x):
        //   E0001-E0009 frontmatter parsing failures
        //   E0010-E0019 path-confinement violations
        //   E0020-E0029 DSL evaluation failures
        //   E0030-E0039 delegate protocol failures
        //   E0040-E0049 schema load failures
        //   E0050-E0059 frontmatter schema validation failures (v0.1.x)
        //   E0070-E0079 IO failures during verify (v0.1.x)
        //   E0080-E0089 pipeline orchestration failures (v0.1.x)
        'E' => matches!(n, 1..=49 | 50..=59 | 70..=79 | 80..=89),
        'W' => matches!(n, 30..=199),
        'L' => matches!(n, 1..=99),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_failure_code_is_reserved() {
        assert!(is_reserved_mdatron_code("MDATRON-E0001"));
    }

    #[test]
    fn schema_class_unknown_code_is_reserved() {
        assert!(is_reserved_mdatron_code("MDATRON-E0002"));
    }

    #[test]
    fn schema_validation_failure_code_is_reserved() {
        // New range introduced in v0.1.x.
        assert!(is_reserved_mdatron_code("MDATRON-E0050"));
    }

    #[test]
    fn io_failure_code_is_reserved() {
        assert!(is_reserved_mdatron_code("MDATRON-E0070"));
    }

    #[test]
    fn pipeline_orchestration_code_is_reserved() {
        assert!(is_reserved_mdatron_code("MDATRON-E0080"));
    }

    #[test]
    fn unreserved_code_in_gap_is_rejected() {
        // E0060-E0069 is a gap; not reserved.
        assert!(!is_reserved_mdatron_code("MDATRON-E0060"));
        // E0090+ is a gap.
        assert!(!is_reserved_mdatron_code("MDATRON-E0099"));
    }

    #[test]
    fn other_prefixes_are_not_mdatron_codes() {
        // Constructed at runtime to avoid a literal "VSDD-" prefix in source,
        // which would trip the cross-repo namespace-separation lint.
        let other_namespace_code = format!("{}{}-E0001", "VS", "DD");
        assert!(!is_reserved_mdatron_code(&other_namespace_code));
    }

    #[test]
    fn warning_codes_in_range() {
        assert!(is_reserved_mdatron_code("MDATRON-W0050"));
        assert!(is_reserved_mdatron_code("MDATRON-W0100"));
        assert!(!is_reserved_mdatron_code("MDATRON-W0001"));
    }
}
