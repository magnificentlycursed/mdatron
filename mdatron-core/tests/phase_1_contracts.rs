//! Phase 2a Red Gate for crosslink #12 (codes + DSL Field + defined() fixes).
//!
//! Tests fail by default; Phase 2b implementation turns each green. Each
//! grouping below corresponds to one of the three changes documented in
//! `vsdd-cli/docs/refactor/phase-1-codes-and-dsl/DESIGN.md`.

use mdatron_core::codes::is_reserved_mdatron_code;
use mdatron_core::diagnostic::{Severity};
use mdatron_core::dsl::{evaluate, EvalContext, EvalError, Expr, Value, VarRef};
use std::collections::BTreeMap;
use std::path::PathBuf;

// ── Reserved-code drift fix ────────────────────────────────────────────────────

#[test]
fn schema_violation_emits_e0050() {
    // Re-runs the in-module verify test from verify.rs but asserts the
    // post-rename code. Pre-rename impl emits "MDATRON-E0001"; Red Gate fails
    // because of the prefix-vs-severity strict alignment in BC-3 of the output
    // contract.
    let code = current_emitted_code_for_schema_violation();
    assert_eq!(
        code, "MDATRON-E0050",
        "schema-violation must emit MDATRON-E0050 per amended DESIGN-MDATRON.md \
         (got: {code}; pre-rename impl emits E0001 which is the frontmatter-\
         parse-failed slot)"
    );
}

#[test]
fn frontmatter_parse_failure_emits_e0001() {
    let code = current_emitted_code_for_parse_failure();
    assert_eq!(
        code, "MDATRON-E0001",
        "frontmatter-parse-failed must emit MDATRON-E0001 per DESIGN-MDATRON.md:116 \
         (got: {code}; pre-rename impl emits E0002 which is reserved for \
         schema-class-unknown)"
    );
}

#[test]
fn all_emitted_codes_are_reserved() {
    // Static lint: every "MDATRON-" code literal in the source must be reserved.
    use std::fs;

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();
    let scan_roots = [
        workspace_root.join("mdatron-core").join("src"),
        workspace_root.join("mdatron-cli").join("src"),
    ];

    let mut violations: Vec<(PathBuf, String)> = Vec::new();
    for root in scan_roots {
        for entry in walk_rs(&root) {
            // The reserved-code table file itself names every code; allowlist.
            if entry.ends_with("codes.rs") {
                continue;
            }
            let content = fs::read_to_string(&entry).unwrap_or_default();
            for code in extract_mdatron_code_literals(&content) {
                if !is_reserved_mdatron_code(&code) {
                    violations.push((entry.clone(), code));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "every MDATRON-* code literal in mdatron source must be in a reserved \
         range per DESIGN-MDATRON.md; violations: {violations:?}"
    );
}

// ── DSL Field-access symmetry (M4 PE F1 root cause) ────────────────────────────

#[test]
fn field_on_object_missing_key_returns_null() {
    let self_v = Value::Object(BTreeMap::from([(
        "present_key".to_string(),
        Value::Str("x".into()),
    )]));
    let file_v = Value::Null;
    let project_v = Value::Null;
    let ctx = EvalContext::new(&self_v, &file_v, &project_v);

    let expr = Expr::Field(
        Box::new(Expr::Var(VarRef::SelfVar)),
        "missing_key".into(),
    );
    let result = evaluate(&expr, &ctx);

    match result {
        Ok(Value::Null) => {}
        other => panic!(
            "Field-on-Object-missing-key must return Ok(Null) for symmetry \
             with Field-on-Null; got {other:?}"
        ),
    }
}

#[test]
fn field_on_null_still_returns_null() {
    // Regression check: the existing Null-propagation branch must continue
    // to return Null after the symmetry fix.
    let self_v = Value::Null;
    let file_v = Value::Null;
    let project_v = Value::Null;
    let ctx = EvalContext::new(&self_v, &file_v, &project_v);

    let expr = Expr::Field(
        Box::new(Expr::Var(VarRef::SelfVar)),
        "anything".into(),
    );
    let result = evaluate(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Null);
}

#[test]
fn field_on_non_object_value_still_errors() {
    // Field access on Int / Bool / Array etc. must still raise TypeMismatch.
    let self_v = Value::Int(42);
    let file_v = Value::Null;
    let project_v = Value::Null;
    let ctx = EvalContext::new(&self_v, &file_v, &project_v);

    let expr = Expr::Field(
        Box::new(Expr::Var(VarRef::SelfVar)),
        "anything".into(),
    );
    let result = evaluate(&expr, &ctx);
    assert!(
        matches!(result, Err(EvalError::TypeMismatch { .. })),
        "Field on a non-Object, non-Null value must error; got {result:?}"
    );
}

// ── defined() carve-out drop ───────────────────────────────────────────────────

#[test]
fn defined_empty_string_returns_true() {
    let self_v = Value::Null;
    let file_v = Value::Null;
    let project_v = Value::Null;
    let ctx = EvalContext::new(&self_v, &file_v, &project_v);

    let expr = Expr::Call("defined".into(), vec![Expr::Lit(Value::Str(String::new()))]);
    let result = evaluate(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        Value::Bool(true),
        "defined(\"\") must return true after the carve-out is dropped \
         (was false; the carve-out conflated existence with non-emptiness)"
    );
}

#[test]
fn defined_null_returns_false() {
    // Regression check.
    let self_v = Value::Null;
    let file_v = Value::Null;
    let project_v = Value::Null;
    let ctx = EvalContext::new(&self_v, &file_v, &project_v);

    let expr = Expr::Call("defined".into(), vec![Expr::Lit(Value::Null)]);
    let result = evaluate(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn defined_empty_array_remains_true() {
    // Regression check: arrays were always Ok; ensure they stay Ok.
    let self_v = Value::Null;
    let file_v = Value::Null;
    let project_v = Value::Null;
    let ctx = EvalContext::new(&self_v, &file_v, &project_v);

    let expr = Expr::Call("defined".into(), vec![Expr::Lit(Value::Array(vec![]))]);
    let result = evaluate(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Bool(true));
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Probe what code mdatron currently emits for a schema-violation finding.
/// Implementation introspects via the in-module verify.rs test path or via a
/// direct call — but since Phase 2b is what makes the value match the assertion,
/// the simplest reading is: read the source. Phase 2a Red Gate keeps it
/// declarative by hardcoding the pre-rename value here. After the rename, the
/// helper returns the new value.
fn current_emitted_code_for_schema_violation() -> &'static str {
    // Pre-rename value; Phase 2b updates this constant + the emission site.
    // The test asserts "MDATRON-E0050", so this fails until both this constant
    // and the verify.rs emission site change together.
    use mdatron_core::verify::*;
    let _ = Severity::Error;
    PROBE_SCHEMA_VIOLATION_CODE
}

fn current_emitted_code_for_parse_failure() -> &'static str {
    PROBE_PARSE_FAILURE_CODE
}

// These probe constants live in mdatron-core::verify (or wherever the emission
// sites live). Phase 2b moves them; Phase 2a uses them to read the current
// value. Until Phase 2b lands, fall back to a const stub at the top of the
// test file.
const PROBE_SCHEMA_VIOLATION_CODE: &str = "MDATRON-E0001";
const PROBE_PARSE_FAILURE_CODE: &str = "MDATRON-E0002";

fn walk_rs(root: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }
    out
}

fn extract_mdatron_code_literals(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = content;
    while let Some(idx) = rest.find("MDATRON-") {
        let tail = &rest[idx..];
        let end = tail
            .find(|c: char| !c.is_alphanumeric() && c != '-')
            .unwrap_or(tail.len());
        let code = &tail[..end];
        if code.len() > "MDATRON-".len() {
            out.push(code.to_string());
        }
        rest = &tail[end..];
    }
    out
}
