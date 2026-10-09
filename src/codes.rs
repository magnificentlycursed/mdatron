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
/// `pub` only so the integration-test crate (`tests/phase_1_contracts.rs`,
/// the reserved-range enforcement check) can call it; the lib is not a public
/// API (`lib.rs`), so this carries no stability promise.
///
/// Reserved ranges (phase-1b catalog, ratified 2026-07-21, issue #50):
/// - `MDATRON-E0001` — `E0009` Frontmatter parsing failures
/// - `MDATRON-E0010` — `E0019` Path-confinement violations (E0012 ratified as-is)
/// - `MDATRON-E0020` — `E0029` DSL evaluation failures; `E0021` rule
///   field-reference validation (undeclared `$self` field under a closed
///   schema, hard-gated at load, #156); `E0023` rule-evaluation-failed — a
///   FINDING (exit 1) for a rule that could not be evaluated on one file,
///   split out of the `E0080` pipeline failure in 0.8.0 (#228)
/// - `MDATRON-E0030` — `E0039` Route family (was delegate protocol, retired);
///   route-bound schema `E0033`/`E0034` (#208), `name_equals_dir` `E0035` (#209),
///   `max_bytes` `E0036` (#216), `requires_sibling` `E0037` (#221)
/// - `MDATRON-E0040` — `E0049` Schema load failures
/// - `MDATRON-E0050` — `E0059` Frontmatter schema validation failures (v0.1.x)
/// - `MDATRON-E0060` — `E0069` Pin family (content-hash pins + managed-manifest
///   drift; section-scoped pins E0063, #146; generated regions E0064 stale,
///   E0065 source missing, E0066 malformed marker, #252)
/// - `MDATRON-E0070` — `E0079` IO failures during verify (v0.1.x)
/// - `MDATRON-E0080` — `E0089` Pipeline orchestration failures (v0.1.x); `E0081`
///   reference-target-not-captured — a FINDING (exit 1) for a pin/citation/link/
///   marker target the run never captured, split out of `E0080` in 0.7.0 (#204)
/// - `MDATRON-E0090` — `E0099` Vocabulary family (registry violations)
/// - `MDATRON-E0100` — `E0109` Citation family (citation conformance)
/// - `MDATRON-E0110` — `E0119` Reference families: body links/anchors (link, #145);
///   marker-line references (marker, `E0112`, #147); adopter code-catalog
///   integrity (code_catalog, `E0113`, #148); the offline external-link
///   checks (link, #215: `E0115` undeclared URL, `E0116` undeclared fragment,
///   `E0117` malformed URL, `E0118` policy violation)
/// - `MDATRON-E0120` — `E0129` Section-structural family (count/disjointness over
///   body sections: `E0120` section-count-violation, `E0121` section-ids-not-disjoint, #157;
///   `E0123` section-element-mismatch, #213; `E0124` section-order-violation, #214)
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
        // E0090-99 vocabulary, E0100-109 citation, E0110-119 reference
        // (link/marker/code, #145/#147/#148), E0120-129 section-structural (#157)
        // (contiguous E0001-E0129); W0040-99 config/governance/family warnings;
        // L0001-99 engine lints.
        'E' => matches!(n, 1..=129),
        'W' => matches!(n, 40..=99),
        'L' => matches!(n, 1..=99),
        _ => false,
    }
}

/// The PRODUCTION region of a Rust source file, for the code-discipline
/// controls (the namespace-separation check, the every-code-resolves-in-explain
/// tripwire, the methodology denylist): the text before the file's INLINE
/// unit-test module — a line opening with `#[cfg(test)]` whose item, after any
/// further attributes, `//` comments and a `pub`/`pub(...)` qualifier, is
/// `mod <name> {`. Everything else is production: a `#[cfg(test)]` on a lone
/// item (a test-only helper fn, an inline block) — cutting at the first
/// `#[cfg(test)]` of any kind blinded the controls to everything after such an
/// item, none of verify.rs's emitted codes were being scanned (L3 cold review,
/// MAJOR-1); a `mod tests;` FILE module, which removes nothing from this file
/// (round 2, MINOR-A); a marker that is not the first token on its line, as in
/// a comment or a string. A file with no inline test module is scanned whole.
/// `Err` when a file carries two: the region is then undefined and the caller
/// must not guess.
///
/// The heuristic's boundary: only the bare `#[cfg(test)]` spelling is a
/// module marker; a combined `#[cfg(all(test, …))]` module is scanned as
/// production — the loud direction (a false positive, never a blind spot).
///
/// `pub` for the integration-test crates, like [`is_reserved_mdatron_code`].
pub fn production_region(source: &str) -> Result<&str, String> {
    const MARKER: &str = "#[cfg(test)]";
    let mut cut: Option<usize> = None;
    let mut line_start = 0;
    for line in source.split_inclusive('\n') {
        let at = line_start;
        line_start += line.len();
        let indent = line.len() - line.trim_start().len();
        if !line[indent..].starts_with(MARKER) {
            continue;
        }
        let marker_at = at + indent;
        let mut rest = source[marker_at + MARKER.len()..].trim_start();
        loop {
            if rest.starts_with("#[") {
                rest = skip_attribute(rest).trim_start();
            } else if rest.starts_with("//") {
                rest = rest.split_once('\n').map_or("", |(_, r)| r).trim_start();
            } else {
                break;
            }
        }
        if let Some(after_pub) = rest.strip_prefix("pub") {
            rest = if after_pub.starts_with('(') {
                after_pub.split_once(')').map_or("", |(_, r)| r)
            } else {
                after_pub
            }
            .trim_start();
        }
        let Some(tail) = rest.strip_prefix("mod") else {
            continue;
        };
        if !tail.starts_with(char::is_whitespace) {
            continue;
        }
        let after_name = tail
            .trim_start()
            .trim_start_matches(|c: char| c.is_alphanumeric() || c == '_')
            .trim_start();
        if !after_name.starts_with('{') {
            continue; // `mod tests;` declares a file module: nothing here is test code
        }
        if let Some(first) = cut {
            return Err(format!(
                "two inline test modules (byte offsets {first} and {marker_at}); the production region is undefined"
            ));
        }
        cut = Some(marker_at);
    }
    Ok(cut.map_or(source, |c| &source[..c]))
}

/// The text after one `#[…]` attribute that opens `s`, brackets balanced (so a
/// `]` inside the attribute, as in `#[doc = "x[y]"]`, does not end it early).
fn skip_attribute(s: &str) -> &str {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return &s[i + 1..];
                }
            }
            _ => {}
        }
    }
    ""
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
        // Above the allocated families (contiguous E0001-E0129, section added #157):
        // unreserved.
        assert!(!is_reserved_mdatron_code("MDATRON-E0130"));
        assert!(!is_reserved_mdatron_code("MDATRON-E0200"));
    }

    #[test]
    fn new_family_ranges_are_reserved() {
        // Phase-1b (#50) allocations: the four new check families.
        assert!(is_reserved_mdatron_code("MDATRON-E0030")); // route (was delegate)
        assert!(is_reserved_mdatron_code("MDATRON-E0060")); // pin (init manifest drift)
        assert!(is_reserved_mdatron_code("MDATRON-E0090")); // vocabulary
        assert!(is_reserved_mdatron_code("MDATRON-E0100")); // citation
        assert!(is_reserved_mdatron_code("MDATRON-E0110")); // link (#145)
    }

    // MAJOR-1 regression: an early `#[cfg(test)]` on a lone item must not end
    // the production region; only the test MODULE does.
    #[test]
    fn production_region_cuts_at_the_test_module_not_the_first_cfg_test() {
        let cfg = "#[cfg(test)]";
        let src = format!(
            "fn a() {{}}\n{cfg}\nfn helper() {{}}\npub const LEAK: &str = \"after the helper\";\n\
             {cfg} {{ inline_block(); }}\nfn b() {{}}\n{cfg}\n#[allow(dead_code)]\nmod tests {{\n    \
             const IN_TESTS: &str = \"not production\";\n}}\n"
        );
        let prod = production_region(&src).unwrap();
        assert!(prod.contains("after the helper"), "{prod}");
        assert!(prod.contains("fn b()"));
        assert!(!prod.contains("not production"));
        assert!(prod.ends_with("fn b() {}\n"), "{prod:?}");

        // No test module: the whole file is production.
        let src = format!("fn a() {{}}\n{cfg}\nfn helper() {{}}\nconst X: &str = \"x\";\n");
        assert_eq!(production_region(&src).unwrap(), src);

        // A test-only `modern_thing` fn is not a module.
        let src = format!("{cfg}\nfn modern() {{}}\nconst X: &str = \"x\";\n");
        assert_eq!(production_region(&src).unwrap(), src);

        // Two inline test modules: undefined, refused.
        let src = format!("{cfg}\nmod tests {{}}\nfn c() {{}}\n{cfg}\nmod more_tests {{}}\n");
        assert!(production_region(&src).is_err());

        // Round-2 MINOR-A: a FILE module (`mod tests;`) removes nothing from
        // this file — the whole file stays production; and a marker that is
        // not the first token on its line (a comment, a string) is no marker.
        let src = format!("{cfg}\nmod tests;\nconst X: &str = \"x\";\n");
        assert_eq!(production_region(&src).unwrap(), src);
        let src = format!(
            "// TODO: move these into a {cfg} mod tests later\nconst X: &str = \"x\";\n\
             const S: &str = \"{cfg} mod tests {{\";\nfn f() {{}}\n"
        );
        assert_eq!(production_region(&src).unwrap(), src);

        // Round-2 NIT-D: a `pub(crate)` module, `//` comments and an attribute
        // with `]` inside between the cfg and the item are still the module.
        let src = format!(
            "fn a() {{}}\n{cfg}\n// why this module exists\n#[doc = \"x[y]\"]\n\
             pub(crate) mod test_support {{ const T: &str = \"t\"; }}\n"
        );
        assert_eq!(production_region(&src).unwrap(), "fn a() {}\n");
        let src = format!("{cfg} pub mod tests {{}}\n");
        assert_eq!(production_region(&src).unwrap(), "");

        // This very file: the region ends before its own test module.
        let me = std::fs::read_to_string(file!()).unwrap();
        let prod = production_region(&me).unwrap();
        assert!(prod.contains("pub fn production_region"));
        assert!(!prod.contains("fn production_region_cuts_at_the_test_module"));
    }

    #[test]
    fn other_prefixes_are_not_mdatron_codes() {
        // An adopter namespace: the engine knows it only as code-catalog data,
        // never as a reserved range. Naming the code here is legal — the
        // namespace-separation control (tests/output_format.rs::
        // mdatron_source_never_emits_vsdd_code_prefix) scans PRODUCTION
        // regions for emitted literals, not test modules (#185).
        assert!(!is_reserved_mdatron_code("VSDD-E0001"));
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
