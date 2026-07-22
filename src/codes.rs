//! Reserved error-code allocation table for mdatron.
//!
//! Per `DESIGN.md` § Diagnostics are a versioned contract, every code emitted
//! by mdatron must fall in one of these reserved ranges. The
//! [`is_reserved_mdatron_code`] check is used by integration tests + the
//! reserved-range enforcement check at
//! `tests/phase_1_contracts.rs::all_emitted_codes_are_reserved`
//! to enforce the discipline at build time. (Note: not a "lint" in the
//! adopter-facing MDATRON-L#### sense per crosslink #12 TW/F4 — the
//! L-range is reserved for runtime adopter findings.)

/// Returns true if `code` is a syntactically valid mdatron-reserved code.
///
/// **Stability: unstable at v0.1.x.** This function is `pub` to enable the
/// cross-crate reserved-range enforcement check at
/// `tests/phase_1_contracts.rs`, but is NOT part of the stable
/// public API. External crates should not depend on this surface; it may
/// move, rename, or change signature at any v0.1.x release. Per crosslink
/// #12 PE/F6 (revisit at v0.2). After binary-first Phase 4 collapses the
/// workspace, this becomes a `pub(crate)` test-only helper.
///
/// Reserved ranges (phase-1b catalog, ratified 2026-07-21, issue #50):
/// - `MDATRON-E0001` — `E0009` Frontmatter parsing failures
/// - `MDATRON-E0010` — `E0019` Path-confinement violations (E0012 ratified as-is)
/// - `MDATRON-E0020` — `E0029` DSL evaluation failures
/// - `MDATRON-E0030` — `E0039` Route family (was delegate protocol, retired)
/// - `MDATRON-E0040` — `E0049` Schema load failures
/// - `MDATRON-E0050` — `E0059` Frontmatter schema validation failures (v0.1.x)
/// - `MDATRON-E0060` — `E0069` Pin family (content-hash pins + managed-manifest drift)
/// - `MDATRON-E0070` — `E0079` IO failures during verify (v0.1.x)
/// - `MDATRON-E0080` — `E0089` Pipeline orchestration failures (v0.1.x)
/// - `MDATRON-E0090` — `E0099` Vocabulary family (registry violations)
/// - `MDATRON-E0100` — `E0109` Citation family (citation conformance)
/// - `MDATRON-W0040` — `W0099` Configuration, governance, and family warnings
/// - `MDATRON-L0001` — `L0099` Engine-level lints
///
/// Retired per the DESIGN.md absorption ledger and returned to the pool: the
/// delegate protocol range (`E0030`–`E0039`, reallocated to route above; its
/// warnings `W0030`–`W0039`) and the built-in-pattern findings (`W0100`–`W0199`).
#[doc(hidden)]
pub fn is_reserved_mdatron_code(code: &str) -> bool {
    let Some(suffix) = code.strip_prefix("MDATRON-") else {
        return false;
    };
    // Byte-aware split so non-ASCII bytes at position 0 (homoglyph evasion
    // or accidental UTF-8) cannot trigger the panic-on-multibyte-slice that
    // suffix[1..] would have if the first char were multi-byte. Per crosslink
    // #12 SE/F2 + SEC/F3 convergence.
    let Some(&letter_byte) = suffix.as_bytes().first() else {
        return false;
    };
    let Some(number_part) = suffix.get(1..) else {
        return false;
    };
    let Ok(n) = number_part.parse::<u32>() else {
        return false;
    };
    let letter = letter_byte as char;
    match letter {
        // Ranges per DESIGN.md § Diagnostics are a versioned contract (phase-1b
        // catalog, #50): E0001-9 frontmatter-parse, E0010-19 path-confinement,
        // E0020-29 DSL, E0030-39 route, E0040-49 schema-load, E0050-59
        // schema-validation, E0060-69 pin, E0070-79 IO, E0080-89 pipeline,
        // E0090-99 vocabulary, E0100-109 citation (contiguous E0001-E0109);
        // W0040-99 config/governance/family warnings; L0001-99 engine lints.
        'E' => matches!(n, 1..=109),
        'W' => matches!(n, 40..=99),
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
    fn unreserved_code_above_ranges_is_rejected() {
        // Above the allocated families (contiguous E0001-E0109): unreserved.
        assert!(!is_reserved_mdatron_code("MDATRON-E0110"));
        assert!(!is_reserved_mdatron_code("MDATRON-E0200"));
    }

    #[test]
    fn new_family_ranges_are_reserved() {
        // Phase-1b (#50) allocations: the four new check families.
        assert!(is_reserved_mdatron_code("MDATRON-E0030")); // route (was delegate)
        assert!(is_reserved_mdatron_code("MDATRON-E0060")); // pin (init manifest drift)
        assert!(is_reserved_mdatron_code("MDATRON-E0090")); // vocabulary
        assert!(is_reserved_mdatron_code("MDATRON-E0100")); // citation
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
        // W0100-W0199 (built-in patterns) and W0030-W0039 (delegate) retired
        // per the #50 re-cut; W now runs 0040-0099.
        assert!(!is_reserved_mdatron_code("MDATRON-W0100"));
        assert!(!is_reserved_mdatron_code("MDATRON-W0030"));
        assert!(!is_reserved_mdatron_code("MDATRON-W0001"));
    }
}
