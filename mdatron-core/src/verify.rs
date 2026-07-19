//! Top-level validation pipeline.
//!
//! Walks a project tree, loads schemas and patterns from `.mdatron/`, applies both
//! Layer 1 (JSON Schema) and Layer 2 (DSL rules) against every markdown file with
//! frontmatter, and returns a `Vec<Finding>`.
//!
//! Surface:
//! - [`VerifyConfig`] — input: project root + schemas/patterns dirs + file globs
//! - [`verify`] — runs the pipeline; returns findings
//! - [`VerifyError`] — pipeline-internal errors (distinct from findings, which are
//!   validation outcomes)
//!
//! v0.1.x scope:
//! - Schema-class dispatch via the `schema_class:` frontmatter field
//! - Layer 1 emits `MDATRON-E0050: frontmatter-schema-violation` per validation error
//!   (E0001 is now exclusively reserved for `frontmatter-parse-failed`; see codes.rs)
//! - Layer 2 runs every pattern rule whose context matches the file's schema_class
//!   or path glob; emits the rule's `code` on assertion failure
//! - Message interpolation via `{{<expression>}}` markers
//! - Source-span: line 1 column 0 for v0.1.x (precise location lands later)

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::diagnostic::{Finding, Location, Severity};
use crate::dsl::{
    evaluate, parse_expression, parse_pattern_file, ContextSelector, EvalContext, EvalError,
    IndexError, IndexRegistry, PatternFile, Rule, Value,
};
use crate::frontmatter;
use crate::schema::Schema;

// ── Public surface ─────────────────────────────────────────────────────────────

/// Configuration for a verification run.
#[derive(Debug, Clone)]
pub struct VerifyConfig {
    pub project_root: PathBuf,
    pub schemas_dir: PathBuf,
    pub patterns_dir: PathBuf,
    /// Globs (relative to `project_root`) of files to validate.
    pub file_globs: Vec<String>,
}

impl VerifyConfig {
    /// Build a config with the conventional `.mdatron/` paths under `project_root`.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        let root: PathBuf = project_root.into();
        Self {
            schemas_dir: root.join(".mdatron").join("schemas"),
            patterns_dir: root.join(".mdatron").join("patterns"),
            project_root: root,
            file_globs: vec!["**/*.md".to_string()],
        }
    }
}

/// Errors arising during pipeline orchestration. Distinct from validation
/// outcomes (which are [`Finding`]s).
#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("io error at '{path}': {error}")]
    Io { path: String, error: String },

    #[error("schema load error at '{path}': {error}")]
    SchemaLoad { path: String, error: String },

    #[error("pattern load error at '{path}': {error}")]
    PatternLoad { path: String, error: String },

    #[error("index build error: {0}")]
    IndexBuild(#[from] IndexError),

    #[error(
        "expression parse error in pattern '{pattern_id}' rule '{rule_id}' ({field}): {error}"
    )]
    ExprParse {
        pattern_id: String,
        rule_id: String,
        field: String,
        error: String,
    },

    #[error("expression evaluation error in pattern '{pattern_id}' rule '{rule_id}': {error}")]
    Eval {
        pattern_id: String,
        rule_id: String,
        error: EvalError,
    },

    #[error("glob error: {0}")]
    Glob(String),

    #[error("frontmatter parse error at '{path}': {error}")]
    Frontmatter { path: String, error: String },
}

/// Run the verification pipeline. Returns findings sorted by file path then code.
pub fn verify(config: &VerifyConfig) -> Result<Vec<Finding>, VerifyError> {
    // BC-4 pipeline-fail detection: refuse to proceed when neither schemas nor patterns
    // directories exist. A project without either has nothing to validate against; this
    // is a configuration error, not a clean run with zero findings.
    if !config.schemas_dir.is_dir() && !config.patterns_dir.is_dir() {
        return Err(VerifyError::SchemaLoad {
            path: config.schemas_dir.to_string_lossy().into_owned(),
            error: "no schemas or patterns directory; run `mdatron init` first".into(),
        });
    }
    let schemas = load_schemas(&config.schemas_dir)?;
    let patterns = load_patterns(&config.patterns_dir)?;

    // Canonicalize the project root so globs joined against it produce absolute
    // patterns. This avoids cwd ambiguity when callers pass a relative root.
    let project_root = config
        .project_root
        .canonicalize()
        .map_err(|e| VerifyError::Io {
            path: config.project_root.to_string_lossy().into_owned(),
            error: e.to_string(),
        })?;

    // Build a single registry from the union of all patterns' keys: declarations.
    let mut all_keys = Vec::new();
    for pf in &patterns {
        all_keys.extend(pf.pattern.keys.clone());
    }
    let registry = IndexRegistry::build(&project_root, &all_keys)?;

    let mut findings: Vec<Finding> = Vec::new();
    for glob_pattern in &config.file_globs {
        let absolute = project_root.join(glob_pattern);
        let paths = glob::glob(&absolute.to_string_lossy())
            .map_err(|e| VerifyError::Glob(format!("'{glob_pattern}': {e}")))?;
        for entry in paths {
            let path = entry.map_err(|e| VerifyError::Glob(format!("'{glob_pattern}': {e}")))?;
            verify_file(&path, &schemas, &patterns, &registry, &mut findings)?;
        }
    }

    findings.sort_by(|a, b| {
        a.location
            .file
            .cmp(&b.location.file)
            .then_with(|| a.code.cmp(&b.code))
    });
    Ok(findings)
}

// ── Schema + pattern loading ───────────────────────────────────────────────────

fn load_schemas(dir: &Path) -> Result<BTreeMap<String, Schema>, VerifyError> {
    let mut out = BTreeMap::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir).map_err(|e| VerifyError::Io {
        path: dir.to_string_lossy().into_owned(),
        error: e.to_string(),
    })? {
        let entry = entry.map_err(|e| VerifyError::Io {
            path: dir.to_string_lossy().into_owned(),
            error: e.to_string(),
        })?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let schema_class = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| VerifyError::SchemaLoad {
                path: path.to_string_lossy().into_owned(),
                error: "could not derive schema_class from filename".into(),
            })?;
        let content = std::fs::read_to_string(&path).map_err(|e| VerifyError::SchemaLoad {
            path: path.to_string_lossy().into_owned(),
            error: e.to_string(),
        })?;
        let json: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| VerifyError::SchemaLoad {
                path: path.to_string_lossy().into_owned(),
                error: e.to_string(),
            })?;
        let schema = Schema::compile(&json).map_err(|e| VerifyError::SchemaLoad {
            path: path.to_string_lossy().into_owned(),
            error: e.to_string(),
        })?;
        out.insert(schema_class, schema);
    }
    Ok(out)
}

fn load_patterns(dir: &Path) -> Result<Vec<PatternFile>, VerifyError> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir).map_err(|e| VerifyError::Io {
        path: dir.to_string_lossy().into_owned(),
        error: e.to_string(),
    })? {
        let entry = entry.map_err(|e| VerifyError::Io {
            path: dir.to_string_lossy().into_owned(),
            error: e.to_string(),
        })?;
        let path = entry.path();
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext != "yaml" && ext != "yml" {
            continue;
        }
        let content = std::fs::read_to_string(&path).map_err(|e| VerifyError::PatternLoad {
            path: path.to_string_lossy().into_owned(),
            error: e.to_string(),
        })?;
        let pf = parse_pattern_file(&content).map_err(|e| VerifyError::PatternLoad {
            path: path.to_string_lossy().into_owned(),
            error: e.to_string(),
        })?;
        out.push(pf);
    }
    Ok(out)
}

// ── Per-file processing ────────────────────────────────────────────────────────

fn verify_file(
    path: &Path,
    schemas: &BTreeMap<String, Schema>,
    patterns: &[PatternFile],
    registry: &IndexRegistry,
    findings: &mut Vec<Finding>,
) -> Result<(), VerifyError> {
    let content = std::fs::read_to_string(path).map_err(|e| VerifyError::Io {
        path: path.to_string_lossy().into_owned(),
        error: e.to_string(),
    })?;

    let fm_opt = match frontmatter::parse(&content) {
        Ok(opt) => opt,
        Err(e) => {
            findings.push(Finding {
                code: "MDATRON-E0001".into(),
                severity: Severity::Error,
                summary: "frontmatter-parse-failed".into(),
                message: e.to_string(),
                help: None,
                location: Location {
                    file: path.to_path_buf(),
                    line: 1,
                    column: 0,
                },
                explain_ref: Some("MDATRON-E0001".into()),
            });
            return Ok(());
        }
    };

    let frontmatter_value = match fm_opt {
        Some((fm, _body)) => fm,
        None => return Ok(()),
    };

    let frontmatter_internal = crate::dsl::index::yaml_to_value(&frontmatter_value);
    let schema_class_opt = frontmatter_internal
        .as_object()
        .and_then(|o| o.get("schema_class"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // ── Layer 1: structural validation ─────────────────────────────────────
    if let Some(schema_class) = &schema_class_opt {
        if let Some(schema) = schemas.get(schema_class) {
            for ve in schema.validate(&frontmatter_value) {
                findings.push(Finding {
                    code: "MDATRON-E0050".into(),
                    severity: Severity::Error,
                    summary: "frontmatter-schema-violation".into(),
                    message: format!("{} ({})", ve.message, ve.instance_path),
                    help: None,
                    location: Location {
                        file: path.to_path_buf(),
                        line: 1,
                        column: 0,
                    },
                    explain_ref: Some("MDATRON-E0050".into()),
                });
            }
        }
    }

    // ── Layer 2: rule-based validation ─────────────────────────────────────
    let file_value = Value::Object(BTreeMap::from([(
        "path".to_string(),
        Value::Str(path.to_string_lossy().into_owned()),
    )]));
    let project_value = Value::Null;

    let rule_ctx = RuleContext {
        self_value: &frontmatter_internal,
        file_value: &file_value,
        project_value: &project_value,
        registry,
        path,
    };
    for pf in patterns {
        for rule in &pf.pattern.rules {
            if !context_matches(&rule.context, schema_class_opt.as_deref(), path) {
                continue;
            }
            verify_rule(pf, rule, &rule_ctx, findings)?;
        }
    }
    Ok(())
}

struct RuleContext<'a> {
    self_value: &'a Value,
    file_value: &'a Value,
    project_value: &'a Value,
    registry: &'a IndexRegistry,
    path: &'a Path,
}

fn verify_rule(
    pf: &PatternFile,
    rule: &Rule,
    rc: &RuleContext,
    findings: &mut Vec<Finding>,
) -> Result<(), VerifyError> {
    let mut ctx =
        EvalContext::new(rc.self_value, rc.file_value, rc.project_value).with_indices(rc.registry);

    // Evaluate let bindings in declared order (BTreeMap iterates by key — not strictly the
    // declared order, but stable; for v0.1.x this is acceptable).
    for (name, expr_str) in &rule.let_bindings {
        let expr = parse_expression(expr_str).map_err(|e| VerifyError::ExprParse {
            pattern_id: pf.pattern.id.clone(),
            rule_id: rule.id.clone(),
            field: format!("let.{name}"),
            error: e.message,
        })?;
        let value = evaluate(&expr, &ctx).map_err(|e| VerifyError::Eval {
            pattern_id: pf.pattern.id.clone(),
            rule_id: rule.id.clone(),
            error: e,
        })?;
        ctx.bindings.insert(name.clone(), value);
    }

    let assert_expr = parse_expression(&rule.assert).map_err(|e| VerifyError::ExprParse {
        pattern_id: pf.pattern.id.clone(),
        rule_id: rule.id.clone(),
        field: "assert".into(),
        error: e.message,
    })?;
    let result = evaluate(&assert_expr, &ctx).map_err(|e| VerifyError::Eval {
        pattern_id: pf.pattern.id.clone(),
        rule_id: rule.id.clone(),
        error: e,
    })?;

    let passed = matches!(result, Value::Bool(true));
    if !passed {
        let message =
            interpolate_message(&rule.message, &ctx).map_err(|e| VerifyError::ExprParse {
                pattern_id: pf.pattern.id.clone(),
                rule_id: rule.id.clone(),
                field: "message".into(),
                error: e,
            })?;
        findings.push(Finding {
            code: rule.code.clone(),
            severity: Severity::Error,
            summary: rule.id.clone(),
            message,
            help: None,
            location: Location {
                file: rc.path.to_path_buf(),
                line: 1,
                column: 0,
            },
            explain_ref: Some(rule.code.clone()),
        });
    }
    Ok(())
}

// ── Context-selector matching ──────────────────────────────────────────────────

fn context_matches(context: &ContextSelector, schema_class: Option<&str>, path: &Path) -> bool {
    match context {
        ContextSelector::Bare(s) => {
            if s.contains('*') || s.contains('?') || s.contains('/') {
                glob_matches(s, path)
            } else {
                schema_class.map(|sc| sc == s).unwrap_or(false)
            }
        }
        ContextSelector::Combined {
            schema_class: sc,
            path: p,
        } => {
            let schema_ok = sc
                .as_deref()
                .map(|expected| schema_class.map(|sc| sc == expected).unwrap_or(false))
                .unwrap_or(true);
            let path_ok = p
                .as_deref()
                .map(|pattern| glob_matches(pattern, path))
                .unwrap_or(true);
            schema_ok && path_ok
        }
    }
}

/// Match a glob pattern against a path. Used for context-selector path matching
/// (not for resolving glob sources, which use the glob crate's directory walker).
fn glob_matches(pattern: &str, path: &Path) -> bool {
    // Use globset-style matching via the glob crate's match helper, which only matches
    // a single path against a pattern.
    let path_str = path.to_string_lossy();
    match glob::Pattern::new(pattern) {
        Ok(p) => p.matches(&path_str),
        Err(_) => false,
    }
}

// ── Message interpolation ──────────────────────────────────────────────────────

fn interpolate_message(template: &str, ctx: &EvalContext) -> Result<String, String> {
    let mut out = String::new();
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(close_rel) = template[i + 2..].find("}}") {
                let expr_start = i + 2;
                let expr_end = i + 2 + close_rel;
                let expr_str = template[expr_start..expr_end].trim();
                let expr = parse_expression(expr_str)
                    .map_err(|e| format!("interpolation '{expr_str}': {}", e.message))?;
                let value = evaluate(&expr, ctx)
                    .map_err(|e| format!("interpolation '{expr_str}' eval: {e}"))?;
                out.push_str(&format_value(&value));
                i = expr_end + 2;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    Ok(out)
}

fn format_value(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(a) => {
            let parts: Vec<String> = a.iter().map(format_value).collect();
            parts.join(", ")
        }
        Value::Object(_) => format!("{v:?}"),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempProject(PathBuf);

    impl TempProject {
        fn new(label: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("mdatron-verify-{label}-{nanos}"));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, rel: &str, content: &str) {
            let full = self.0.join(rel);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&full, content).unwrap();
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn minimal_phase_primer_schema() -> &'static str {
        r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["schema_class", "phase", "relevant_domains"],
  "properties": {
    "schema_class": { "type": "string", "const": "phase-primer" },
    "phase": { "type": "string", "enum": ["phase-1a", "phase-2a", "phase-2b"] },
    "relevant_domains": { "type": "array", "items": { "type": "string" } }
  },
  "additionalProperties": true
}"#
    }

    #[test]
    fn clean_project_returns_zero_findings() {
        let proj = TempProject::new("clean");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            "primer.md",
            "---\nschema_class: phase-primer\nphase: phase-1a\nrelevant_domains: [se]\n---\n# body\n",
        );

        let cfg = VerifyConfig::new(&proj.0);
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.is_empty(),
            "expected no findings; got {findings:?}"
        );
    }

    #[test]
    fn schema_violation_emits_mdatron_e0001() {
        let proj = TempProject::new("schema-fail");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            "bad.md",
            "---\nschema_class: phase-primer\nphase: invalid-phase\nrelevant_domains: [se]\n---\n",
        );

        let cfg = VerifyConfig::new(&proj.0);
        let findings = verify(&cfg).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "MDATRON-E0050");
        assert!(
            findings[0].message.contains("invalid-phase") || findings[0].message.contains("enum")
        );
    }

    #[test]
    fn rule_violation_emits_rule_code_with_interpolated_message() {
        let proj = TempProject::new("rule-fail");
        proj.write(
            ".mdatron/patterns/check-phase.yaml",
            r#"mdatron_dsl_version: 1
pattern:
  id: phase-check
  rules:
    - id: phase-must-be-1a
      context: phase-primer
      assert: $self.phase == "phase-1a"
      code: TEST-E0001
      message: "phase must be phase-1a; got {{$self.phase}}"
"#,
        );
        proj.write(
            "primer.md",
            "---\nschema_class: phase-primer\nphase: phase-2a\nrelevant_domains: [se]\n---\n",
        );

        let cfg = VerifyConfig::new(&proj.0);
        let findings = verify(&cfg).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "TEST-E0001");
        assert!(
            findings[0].message.contains("phase-2a"),
            "interpolation should substitute $self.phase; got: {}",
            findings[0].message
        );
    }

    #[test]
    fn passing_rule_emits_no_finding() {
        let proj = TempProject::new("rule-pass");
        proj.write(
            ".mdatron/patterns/check-phase.yaml",
            r#"mdatron_dsl_version: 1
pattern:
  id: phase-check
  rules:
    - id: phase-must-be-1a
      context: phase-primer
      assert: $self.phase == "phase-1a"
      code: TEST-E0001
      message: "phase must be phase-1a"
"#,
        );
        proj.write(
            "primer.md",
            "---\nschema_class: phase-primer\nphase: phase-1a\nrelevant_domains: [se]\n---\n",
        );

        let cfg = VerifyConfig::new(&proj.0);
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.is_empty(),
            "no findings expected; got {findings:?}"
        );
    }

    #[test]
    fn cross_file_rule_with_key_lookup_runs_end_to_end() {
        let proj = TempProject::new("e2e-key");
        proj.write(
            ".mdatron/patterns/composition.yaml",
            r#"mdatron_dsl_version: 1
pattern:
  id: phase-composition
  keys:
    - name: composition-matrix
      source: .vsdd/registry/phase-domain-matrix.yaml
      select: $.matrix
      indexed_by: $key
  rules:
    - id: required-domains-present
      context: phase-primer
      let:
        expected: key("composition-matrix", $self.phase)
      assert: every(d in $expected.required, d in $self.relevant_domains)
      code: MDATRON-W0100
      message: "phase {{$self.phase}} missing required domain(s)"
"#,
        );
        proj.write(
            ".vsdd/registry/phase-domain-matrix.yaml",
            "matrix:\n  phase-2a:\n    required: [se, qe]\n  phase-1a:\n    required: [so]\n",
        );
        // primer that satisfies the rule
        proj.write(
            "good.md",
            "---\nschema_class: phase-primer\nphase: phase-2a\nrelevant_domains: [se, qe, sa]\n---\n",
        );
        // primer that fails (missing qe)
        proj.write(
            "bad.md",
            "---\nschema_class: phase-primer\nphase: phase-2a\nrelevant_domains: [se]\n---\n",
        );

        let cfg = VerifyConfig::new(&proj.0);
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            findings.len(),
            1,
            "expected exactly one finding (the bad primer); got {findings:?}"
        );
        assert_eq!(findings[0].code, "MDATRON-W0100");
        assert!(findings[0]
            .location
            .file
            .to_string_lossy()
            .contains("bad.md"));
    }

    #[test]
    fn file_without_frontmatter_is_skipped() {
        let proj = TempProject::new("no-fm");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            "plain.md",
            "# Just a plain markdown file\nNo frontmatter here.\n",
        );

        let cfg = VerifyConfig::new(&proj.0);
        let findings = verify(&cfg).unwrap();
        assert!(findings.is_empty());
    }

    #[test]
    fn file_without_schema_class_skips_layer_one_runs_layer_two() {
        let proj = TempProject::new("no-class");
        proj.write(
            ".mdatron/patterns/always-fire.yaml",
            r#"mdatron_dsl_version: 1
pattern:
  id: glob-rule
  rules:
    - id: every-md-file
      context: "**/*.md"
      assert: false
      code: TEST-W0001
      message: "fires on every markdown file"
"#,
        );
        proj.write("any.md", "---\nfoo: bar\n---\n");

        let cfg = VerifyConfig::new(&proj.0);
        let findings = verify(&cfg).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "TEST-W0001");
    }

    #[test]
    fn findings_are_sorted_by_file_then_code() {
        let proj = TempProject::new("sorted");
        proj.write(
            ".mdatron/patterns/multi.yaml",
            r#"mdatron_dsl_version: 1
pattern:
  id: multi-rule
  rules:
    - id: r1
      context: "**/*.md"
      assert: false
      code: ZZZ-E0001
      message: "first rule"
    - id: r2
      context: "**/*.md"
      assert: false
      code: AAA-E0001
      message: "second rule"
"#,
        );
        proj.write("doc.md", "---\nfoo: bar\n---\n");

        let cfg = VerifyConfig::new(&proj.0);
        let findings = verify(&cfg).unwrap();
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].code, "AAA-E0001");
        assert_eq!(findings[1].code, "ZZZ-E0001");
    }

    // ── interpolate_message ─────────────────────────────────────────────────

    #[test]
    fn interpolate_message_replaces_simple_field_access() {
        let self_v = Value::Object(BTreeMap::from([(
            "phase".to_string(),
            Value::Str("phase-2a".into()),
        )]));
        let file_v = Value::Null;
        let project_v = Value::Null;
        let ctx = EvalContext::new(&self_v, &file_v, &project_v);
        let result = interpolate_message("got phase: {{$self.phase}}", &ctx).unwrap();
        assert_eq!(result, "got phase: phase-2a");
    }

    #[test]
    fn interpolate_message_handles_array_values() {
        let self_v = Value::Object(BTreeMap::from([(
            "domains".to_string(),
            Value::Array(vec![Value::Str("se".into()), Value::Str("qe".into())]),
        )]));
        let file_v = Value::Null;
        let project_v = Value::Null;
        let ctx = EvalContext::new(&self_v, &file_v, &project_v);
        let result = interpolate_message("domains: {{$self.domains}}", &ctx).unwrap();
        assert_eq!(result, "domains: se, qe");
    }

    #[test]
    fn interpolate_message_preserves_literal_text() {
        let self_v = Value::Null;
        let file_v = Value::Null;
        let project_v = Value::Null;
        let ctx = EvalContext::new(&self_v, &file_v, &project_v);
        let result = interpolate_message("no interpolation markers here", &ctx).unwrap();
        assert_eq!(result, "no interpolation markers here");
    }
}
