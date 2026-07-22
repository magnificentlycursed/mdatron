//! Phase 2a Red Gate for crosslink #12 (codes + DSL Field + defined() fixes).
//!
//! Tests fail by default; Phase 2b implementation turns each green. Each
//! grouping below corresponds to one of the three changes documented in
//! `vsdd-cli/docs/refactor/phase-1-codes-and-dsl/DESIGN.md`.

use mdatron::codes::is_reserved_mdatron_code;
use mdatron::dsl::{evaluate, EvalContext, EvalError, Expr, Value, VarRef};
use mdatron::verify::{verify, VerifyConfig};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

// ── Reserved-code drift fix ────────────────────────────────────────────────────

#[test]
fn schema_violation_emits_e0050() {
    // Spawn the verify pipeline against a fixture project that has a real
    // schema-violation; assert the resulting Finding's code. NOT a probe-
    // constant comparison — that's what Phase 3 QE caught as circular.
    let proj = TempFixture::new("schema-violation");
    proj.write_schema(
        "blog.json",
        r#"{"type":"object","required":["schema_class","slug"],
            "properties":{"schema_class":{"const":"blog"},
                          "slug":{"type":"string","pattern":"^[a-z0-9-]+$"}},
            "additionalProperties":false}"#,
    );
    proj.write_md("post.md", "---\nschema_class: blog\nslug: Bad Slug!\n---\n");

    let cfg = VerifyConfig::new(proj.root());
    let findings = verify(&cfg).expect("verify runs");
    let codes: Vec<&str> = findings.iter().map(|f| f.code.as_str()).collect();
    assert!(
        codes.contains(&"MDATRON-E0050"),
        "schema-violation must emit MDATRON-E0050 per DESIGN.md § Diagnostics are a versioned contract; \
         got codes: {codes:?}"
    );
}

#[test]
fn frontmatter_parse_failure_emits_e0001() {
    // Same shape: spawn the pipeline against a fixture with malformed YAML;
    // assert the Finding's code.
    let proj = TempFixture::new("parse-failure");
    proj.write_schema(
        "blog.json",
        r#"{"type":"object","required":["schema_class"],
            "properties":{"schema_class":{"const":"blog"}}}"#,
    );
    // Malformed frontmatter: missing closing quote.
    proj.write_md("broken.md", "---\nschema_class: \"blog\n---\n");

    let cfg = VerifyConfig::new(proj.root());
    let findings = verify(&cfg).expect("verify runs (emits finding, not error)");
    let codes: Vec<&str> = findings.iter().map(|f| f.code.as_str()).collect();
    assert!(
        codes.contains(&"MDATRON-E0001"),
        "frontmatter-parse-failed must emit MDATRON-E0001 per DESIGN.md § Five check families (schema conformance); \
         got codes: {codes:?}"
    );
}

#[test]
fn all_emitted_codes_are_reserved() {
    // Static lint: every "MDATRON-" code literal in the workspace must resolve
    // to a reserved range per DESIGN.md § Diagnostics are a versioned contract.
    //
    // Walk: workspace members from Cargo.toml + the workspace root itself
    // (catches future siblings per Phase 3 Red-Team + SE finding that the
    // hardcoded crate list was a supply-chain hole).
    //
    // Extensions: .rs / .yaml / .yml / .json / .toml — covers pattern files,
    // schemas, config, and any non-`.rs` carrier per Phase 3 Red-Team finding.
    // Excludes .md: design docs legitimately describe ranges + reserved-for-
    // future-use labels (e.g. range-header forms like the literal-digit codes
    // in the DESIGN.md reserved-range table) that are reserved-as-future but not yet
    // class-assigned; treating those as violations would block adding new
    // spec rows.

    // Single-crate layout (#81): the manifest dir IS the repo root. The
    // workspace_members() helper returns empty on a non-workspace manifest and
    // the loop below scans the root itself, so the sweep still covers src/.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let members = workspace_members(&workspace_root);
    let mut scan_roots: Vec<PathBuf> = members.iter().map(|m| workspace_root.join(m)).collect();
    scan_roots.push(workspace_root.clone());

    let lint_extensions = ["rs", "yaml", "yml", "json", "toml"];
    let mut violations: Vec<(PathBuf, usize, String)> = Vec::new();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    for root in scan_roots {
        for entry in walk_files(&root, &lint_extensions) {
            if !seen.insert(entry.clone()) {
                continue;
            }
            if entry.ends_with("codes.rs") {
                continue;
            }
            let content = fs::read_to_string(&entry).unwrap_or_default();
            for (line_no, code) in extract_mdatron_code_literals_with_lines(&content) {
                if !is_reserved_mdatron_code(&code) {
                    violations.push((entry.clone(), line_no, code));
                }
            }
        }
    }
    // Format violations as `path:line: code` per finding (one per line) to
    // match the rustc-shape diagnostic style used elsewhere in mdatron. Per
    // crosslink #12 DR/F6 (contributor-friendly error message).
    if !violations.is_empty() {
        let formatted = violations
            .iter()
            .map(|(path, line, code)| format!("    {}:{}: {}", path.display(), line, code))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "every MDATRON-* literal in the workspace must resolve to a reserved \
             range per DESIGN.md; violations:\n{formatted}"
        );
    }
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

    let expr = Expr::Field(Box::new(Expr::Var(VarRef::SelfVar)), "missing_key".into());
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

    let expr = Expr::Field(Box::new(Expr::Var(VarRef::SelfVar)), "anything".into());
    let result = evaluate(&expr, &ctx).unwrap();
    assert_eq!(result, Value::Null);
}

#[test]
fn field_on_nested_missing_returns_null() {
    // Per Phase 3 QE + SE convergence: nested-missing was untested.
    // Object{a: Object{x: 1}}; access $self.a.missing — the inner Object
    // exists but the key 'missing' doesn't. Pre-fix raised FieldNotFound;
    // post-fix returns Null.
    let inner = Value::Object(BTreeMap::from([("x".to_string(), Value::Int(1))]));
    let self_v = Value::Object(BTreeMap::from([("a".to_string(), inner)]));
    let file_v = Value::Null;
    let project_v = Value::Null;
    let ctx = EvalContext::new(&self_v, &file_v, &project_v);

    let expr = Expr::Field(
        Box::new(Expr::Field(
            Box::new(Expr::Var(VarRef::SelfVar)),
            "a".into(),
        )),
        "missing".into(),
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

    let expr = Expr::Field(Box::new(Expr::Var(VarRef::SelfVar)), "anything".into());
    let result = evaluate(&expr, &ctx);
    assert!(
        matches!(result, Err(EvalError::TypeMismatch { .. })),
        "Field on a non-Object, non-Null value must error; got {result:?}"
    );
}

// ── Quantifier-over-Null collections (crosslink #12 SA/F4) ─────────────────────

#[test]
fn every_over_missing_optional_field_does_not_panic() {
    // Field-on-missing-key returns Null; every() over Null collections
    // must evaluate as vacuously-true (empty-collection semantic), not
    // as a TypeMismatch. Per crosslink #12 SA/F4.
    let self_v = Value::Object(BTreeMap::from([(
        "present_key".to_string(),
        Value::Str("x".into()),
    )]));
    let file_v = Value::Null;
    let project_v = Value::Null;
    let ctx = EvalContext::new(&self_v, &file_v, &project_v);

    // every(d in $self.optional, d == "anything") where optional is absent.
    let expr = Expr::Every(
        "d".into(),
        Box::new(Expr::Field(
            Box::new(Expr::Var(VarRef::SelfVar)),
            "optional".into(),
        )),
        Box::new(Expr::Eq(
            Box::new(Expr::Var(VarRef::Binding("d".into()))),
            Box::new(Expr::Lit(Value::Str("anything".into()))),
        )),
    );
    let result = evaluate(&expr, &ctx).expect("every over Null must not error");
    assert_eq!(
        result,
        Value::Bool(true),
        "every over Null collection must be vacuously true"
    );
}

#[test]
fn some_over_missing_optional_field_does_not_panic() {
    // some() over Null collections must evaluate as vacuously-false.
    let self_v = Value::Object(BTreeMap::new());
    let file_v = Value::Null;
    let project_v = Value::Null;
    let ctx = EvalContext::new(&self_v, &file_v, &project_v);

    let expr = Expr::Some_(
        "d".into(),
        Box::new(Expr::Field(
            Box::new(Expr::Var(VarRef::SelfVar)),
            "optional".into(),
        )),
        Box::new(Expr::Eq(
            Box::new(Expr::Var(VarRef::Binding("d".into()))),
            Box::new(Expr::Lit(Value::Str("anything".into()))),
        )),
    );
    let result = evaluate(&expr, &ctx).expect("some over Null must not error");
    assert_eq!(
        result,
        Value::Bool(false),
        "some over Null collection must be vacuously false"
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

struct TempFixture {
    root: PathBuf,
}

impl TempFixture {
    fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("mdatron-phase-1-{label}-{nanos}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".mdatron/schemas")).unwrap();
        fs::create_dir_all(root.join(".mdatron/patterns")).unwrap();
        Self { root }
    }

    fn root(&self) -> &std::path::Path {
        &self.root
    }

    fn write_schema(&self, name: &str, content: &str) {
        fs::write(self.root.join(".mdatron/schemas").join(name), content).unwrap();
    }

    fn write_md(&self, rel: &str, content: &str) {
        fs::write(self.root.join(rel), content).unwrap();
    }
}

impl Drop for TempFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn walk_files(root: &std::path::Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // Skip well-known build / vcs directories.
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if matches!(name, "target" | ".git" | "node_modules") {
                    continue;
                }
            }
            if path.is_dir() {
                stack.push(path);
            } else if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if extensions.contains(&ext) {
                    out.push(path);
                }
            }
        }
    }
    out
}

/// Read workspace member names from the workspace Cargo.toml. Returns the
/// member strings as listed (e.g., "mdatron-core", "mdatron-cli"). Falls back
/// to the empty Vec if Cargo.toml isn't found or parseable; the caller's loop
/// also scans the workspace root, so absent-members is non-fatal.
fn workspace_members(workspace_root: &std::path::Path) -> Vec<String> {
    let cargo_toml = workspace_root.join("Cargo.toml");
    let Ok(content) = fs::read_to_string(&cargo_toml) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut in_workspace = false;
    let mut in_members = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[workspace]" {
            in_workspace = true;
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_workspace = false;
            in_members = false;
            continue;
        }
        if !in_workspace {
            continue;
        }
        if trimmed.starts_with("members") {
            // members = ["a", "b"] or members = [\n "a",\n "b",\n]
            in_members = true;
            for piece in trimmed.split(['[', ']', '"', ',']) {
                let p = piece.trim();
                if !p.is_empty() && !p.starts_with("members") && !p.starts_with('=') {
                    out.push(p.to_string());
                }
            }
            if trimmed.contains(']') {
                in_members = false;
            }
        } else if in_members {
            for piece in trimmed.split(['"', ',', '[', ']']) {
                let p = piece.trim();
                if !p.is_empty() {
                    out.push(p.to_string());
                }
            }
            if trimmed.contains(']') {
                in_members = false;
            }
        }
    }
    out
}

/// Extract well-formed MDATRON-* code literals along with the 1-based line
/// number where each appears. Per crosslink #12 DR/F6: contributor seeing
/// a lint failure needs `path:line: code` not just `path: code` for grep-
/// to-fix loops.
fn extract_mdatron_code_literals_with_lines(content: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let prefix_len = "MDATRON-".len();
        let bytes = line.as_bytes();
        let mut i = 0;
        while i + prefix_len + 5 <= bytes.len() {
            if &bytes[i..i + prefix_len] == b"MDATRON-" {
                let after = &bytes[i + prefix_len..];
                let letter = after[0];
                let digits = &after[1..5];
                if matches!(letter, b'E' | b'W' | b'L') && digits.iter().all(u8::is_ascii_digit) {
                    let code_end = i + prefix_len + 5;
                    out.push((
                        line_no,
                        String::from_utf8_lossy(&bytes[i..code_end]).into_owned(),
                    ));
                    i = code_end;
                    continue;
                }
            }
            i += 1;
        }
    }
    out
}
