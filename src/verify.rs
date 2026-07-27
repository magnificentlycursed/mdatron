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
//! - Source-span: `MDATRON-E0050` resolves the violation's precise source line
//!   from its schema pointer (#65); other findings still land at line 1 column 0
//!   pending their own precise-location work

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};
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
    /// Globs whose matching files must carry frontmatter (`MDATRON-W0040`
    /// on absence). Empty disables the check (#80 D2).
    pub require_frontmatter: Vec<String>,
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
            require_frontmatter: Vec::new(),
        }
    }

    /// Build a config honoring the project's committed `.mdatron/config.yaml`:
    /// its `file_globs` are the consumer-authored jurisdiction (#77), so files
    /// outside them — third-party markdown a chassis or unrelated tool deploys
    /// into the tree — are not walked at all.
    ///
    /// An ABSENT config is a refusal (#80 D1): jurisdiction is always explicit,
    /// never guessed — a governed tree that lost its config must not silently
    /// walk everything (overreach) or nothing (the silent-skip hole at tree
    /// scale). `mdatron init` seeds the config; an explicit `--files`
    /// invocation (which declares jurisdiction on the command line via
    /// [`Self::new`]) is the ad-hoc escape hatch. A present-but-unparsable
    /// config errs the same way (loud, no silent default).
    pub fn from_project(project_root: impl Into<PathBuf>) -> Result<Self, crate::Error> {
        let mut cfg = Self::new(project_root);
        let Some(pc) = crate::config::load(&cfg.project_root)? else {
            return Err(crate::Error::Config(format!(
                "no jurisdiction declared: '{}' is missing — run `mdatron init` \
                 to seed it, or pass explicit --files globs for an ad-hoc run",
                cfg.project_root
                    .join(".mdatron")
                    .join(crate::config::CONFIG_NAME)
                    .display()
            )));
        };
        if !pc.file_globs.is_empty() {
            cfg.file_globs = pc.file_globs;
        }
        cfg.require_frontmatter = pc.require_frontmatter;
        Ok(cfg)
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

    // The wrapped string is a rendered `crate::Error`, which already carries
    // its own "config error:" category label — no second prefix here.
    #[error("{0}")]
    Config(String),

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

    // Opt-in frontmatter requirement (#80 D2): compile the globs once; a
    // malformed pattern is a config error, not a silent no-op.
    let require: Vec<glob::Pattern> = config
        .require_frontmatter
        .iter()
        .map(|g| {
            glob::Pattern::new(g)
                .map_err(|e| VerifyError::Config(format!("require_frontmatter '{g}': {e}")))
        })
        .collect::<Result<_, _>>()?;

    // Route family (#83): load + compile the allowlist; absent file = family
    // inactive. Per-entry defects arrive as findings (confinement escapes drop
    // the entry fail-closed; an absent governing doc reports once, entry stays
    // matchable so it doesn't cascade into spurious unrouted errors).
    let routes = match crate::route::load(&project_root) {
        Ok(r) => r,
        Err(e) => return Err(VerifyError::Config(e.to_string())),
    };

    let mut findings: Vec<Finding> = Vec::new();
    let routes = match routes {
        Some(mut loaded) => {
            findings.append(&mut loaded.findings);
            Some(loaded.routes)
        }
        None => None,
    };
    for glob_pattern in &config.file_globs {
        let absolute = project_root.join(glob_pattern);
        let paths = glob::glob(&absolute.to_string_lossy())
            .map_err(|e| VerifyError::Glob(format!("'{glob_pattern}': {e}")))?;
        for entry in paths {
            let path = entry.map_err(|e| VerifyError::Glob(format!("'{glob_pattern}': {e}")))?;
            if let Some(routes) = &routes {
                let rel = path.strip_prefix(&project_root).unwrap_or(&path);
                crate::route::check_file(routes, rel, &path, &mut findings);
            }
            verify_file(
                &path,
                &project_root,
                &require,
                &schemas,
                &patterns,
                &registry,
                &mut findings,
            )?;
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
    project_root: &Path,
    require_frontmatter: &[glob::Pattern],
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
                quoted: Vec::new(),
            });
            return Ok(());
        }
    };

    let frontmatter_value = match fm_opt {
        Some((fm, _body)) => fm,
        None => {
            // Opt-in loudness (#80 D2): inside a require_frontmatter glob,
            // "no frontmatter" must not be indistinguishable from "passed" —
            // the parse-ABSENCE half of #78's loud-failure/silent-absence
            // asymmetry. Matching is on the root-relative path.
            let rel = path.strip_prefix(project_root).unwrap_or(path);
            if require_frontmatter.iter().any(|p| p.matches_path(rel)) {
                findings.push(Finding {
                    code: "MDATRON-W0040".into(),
                    severity: Severity::Warning,
                    summary: "governed-file-has-no-frontmatter".into(),
                    message: "this file matches a `require_frontmatter` glob in \
                              .mdatron/config.yaml but carries no frontmatter block, \
                              so no schema or rule can govern it"
                        .into(),
                    help: Some(
                        "add the frontmatter block (starting with `---` on line 1), \
                         or narrow the require_frontmatter globs if this file is \
                         genuinely prose-only"
                            .into(),
                    ),
                    location: Location {
                        file: path.to_path_buf(),
                        line: 1,
                        column: 0,
                    },
                    explain_ref: Some("MDATRON-W0040".into()),
                    quoted: Vec::new(),
                });
            }
            return Ok(());
        }
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
            let violations = schema.validate(&frontmatter_value);
            // Resolve every violation's source line in a single marked parse so
            // the diagnostics are directly actionable for the fixing agent
            // (#65/#70/#71); each falls back to the block start when unresolved.
            // For additionalProperties, hand the resolver the offending key from
            // the finding's quoted `unexpected` region (first key) so it can
            // pinpoint the key's own line; `""` for every other violation shape.
            let items: Vec<(&str, &str)> = violations
                .iter()
                .map(|ve| {
                    let key = ve
                        .quoted
                        .iter()
                        .find(|q| q.label == "unexpected")
                        .and_then(|q| q.content.lines().next())
                        .unwrap_or("");
                    (ve.instance_path.as_str(), key)
                })
                .collect();
            let locations = crate::frontmatter::resolve_e0050_locations(&content, &items);
            for (ve, loc) in violations.iter().zip(locations) {
                let (line, column) = loc.unwrap_or((1, 0));
                findings.push(Finding {
                    code: "MDATRON-E0050".into(),
                    severity: Severity::Error,
                    summary: "frontmatter-schema-violation".into(),
                    // The message is engine-authored (schema-side data only); the
                    // failing document value / unexpected keys ride in `quoted`
                    // for prefix-marked rendering, never inline (DESIGN §Output).
                    message: ve.message.clone(),
                    help: None,
                    location: Location {
                        file: path.to_path_buf(),
                        line,
                        column,
                    },
                    explain_ref: Some("MDATRON-E0050".into()),
                    quoted: ve.quoted.clone(),
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
        let (message, quoted) =
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
            quoted,
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

fn interpolate_message(
    template: &str,
    ctx: &EvalContext,
) -> Result<(String, Vec<QuotedRegion>), String> {
    let mut out = String::new();
    let mut quoted = Vec::new();
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
                // The interpolated value is adopter document content. Keep it out
                // of the engine message — an inline value is a forgeable marking
                // (DESIGN §Output). The message carries a `{binding}` placeholder;
                // the value renders in a prefix-marked block beneath it.
                out.push('{');
                out.push_str(expr_str);
                out.push('}');
                quoted.push(QuotedRegion {
                    label: expr_str.to_string(),
                    content: format_value(&value),
                });
                i = expr_end + 2;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    Ok((out, quoted))
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

    // RED GATE (#77, consumer raise 3): config.yaml `file_globs` are the
    // consumer-authored jurisdiction. A file outside them — e.g. third-party
    // markdown a chassis deploys into the tree — is not walked at all, even
    // with broken frontmatter; it is not mdatron's to refuse. Pre-fix, verify
    // ignored the committed config entirely (VerifyConfig::new hardcoded
    // `**/*.md`) and E0001-refused the vendor file.
    #[test]
    fn project_config_file_globs_scope_the_walk() {
        let proj = TempProject::new("config-scope");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        // In-jurisdiction and violating: caught.
        proj.write(
            "docs/bad.md",
            "---\nschema_class: phase-primer\nphase: invalid\nrelevant_domains: [se]\n---\n",
        );
        // Out-of-jurisdiction third-party file with non-YAML frontmatter: untouched.
        proj.write("vendor/tool.md", "---\nnot: valid: yaml: [\n---\n");

        let cfg = VerifyConfig::from_project(&proj.0).expect("config loads");
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            findings.len(),
            1,
            "only the in-jurisdiction file is validated; got {findings:?}"
        );
        assert!(findings[0].location.file.ends_with("docs/bad.md"));

        // The engine default (config ignored) still walks everything — the
        // pre-#77 behavior this red gate exists to demonstrate.
        let all = verify(&VerifyConfig::new(&proj.0)).unwrap();
        assert!(
            all.len() >= 2,
            "default globs walk the vendor file too; got {all:?}"
        );
    }

    // #77: an explicit caller override (`--files`) takes precedence over the
    // project config — the CLI passes its own globs by replacing file_globs.
    // RED GATE (#82 / #80 D1): an absent config is a REFUSAL, not a silent
    // fallback to walk-all — jurisdiction is always explicit, never guessed.
    // Pre-fix, from_project defaulted to `**/*.md`.
    #[test]
    fn absent_config_is_refused_not_defaulted() {
        let proj = TempProject::new("config-absent");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        let err = VerifyConfig::from_project(&proj.0)
            .expect_err("absent config must refuse: no jurisdiction declared");
        assert!(
            err.to_string().contains("no jurisdiction declared"),
            "refusal names the problem; got: {err}"
        );
    }

    // RED GATE (#82 / #80 D2): a file matching a `require_frontmatter` glob
    // that parses to no-frontmatter fires MDATRON-W0040 — "governed file
    // skipped" stops looking identical to "file passed". Pre-fix the config
    // key was tolerated-unknown and the file passed silently.
    #[test]
    fn require_frontmatter_glob_fires_w0040() {
        let proj = TempProject::new("w0040");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\nrequire_frontmatter:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("docs/naked.md", "# prose only, no frontmatter\n");
        let cfg = VerifyConfig::from_project(&proj.0).expect("config loads");
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            findings.len(),
            1,
            "exactly the naked governed file warns; got {findings:?}"
        );
        assert_eq!(findings[0].code, "MDATRON-W0040");
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].location.file.ends_with("docs/naked.md"));
    }

    // ── route family (#83): the allowlist over the governed tree ───────────

    fn routed_project(label: &str) -> TempProject {
        let proj = TempProject::new(label);
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("GOVERNING.md", "# governing doc\n");
        proj.write(
            "docs/2026-07-27-good-name.md",
            "---\nschema_class: phase-primer\nphase: phase-1a\nrelevant_domains: [se]\n---\n",
        );
        proj
    }

    // RED GATE (#83): with route data supplied, a walked file matching no
    // route BLOCKS (E0030). Pre-impl, routes.yaml is inert and the file passes.
    #[test]
    fn unrouted_file_blocks_when_routes_supplied() {
        let proj = routed_project("route-unrouted");
        proj.write(
            ".mdatron/routes.yaml",
            "routes:\n- files: \"docs/registry/**/*.md\"\n  governed_by: GOVERNING.md\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f.code == "MDATRON-E0030" && f.severity == Severity::Error),
            "unrouted docs/2026-07-27-good-name.md must block; got {findings:?}"
        );
    }

    // RED GATE (#83): a route citing an absent governing document BLOCKS (E0031),
    // with the adopter path quoted out-of-line per the marking discipline.
    #[test]
    fn absent_governing_document_blocks() {
        let proj = routed_project("route-absent-gov");
        proj.write(
            ".mdatron/routes.yaml",
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: NO-SUCH-DOC.md\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let f = findings
            .iter()
            .find(|f| f.code == "MDATRON-E0031")
            .unwrap_or_else(|| panic!("expected E0031; got {findings:?}"));
        assert_eq!(f.severity, Severity::Error);
        assert!(
            !f.message.contains("NO-SUCH-DOC"),
            "adopter path must not ride inline: {}",
            f.message
        );
        assert!(f.quoted.iter().any(|q| q.content.contains("NO-SUCH-DOC")));
    }

    // RED GATE (#83): an underivable name is FLAGGED (W0041, warning).
    #[test]
    fn underivable_name_is_flagged() {
        let proj = routed_project("route-naming");
        proj.write(
            "docs/Bad Name With Spaces.md",
            "---\nschema_class: phase-primer\nphase: phase-1a\nrelevant_domains: [se]\n---\n",
        );
        proj.write(
            ".mdatron/routes.yaml",
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n  naming: \"^[0-9]{4}-[0-9]{2}-[0-9]{2}-[a-z0-9-]+\\\\.md$\"\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let w: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "MDATRON-W0041")
            .collect();
        assert_eq!(w.len(), 1, "exactly the bad name flags; got {findings:?}");
        assert_eq!(w[0].severity, Severity::Warning);
        assert!(w[0].location.file.ends_with("docs/Bad Name With Spaces.md"));
    }

    // RED GATE (#83): two routes claiming one file is an ERROR (E0032) per the
    // agnosticism-audit conflict contract (DESIGN: asserted by value+severity).
    #[test]
    fn two_routes_claiming_one_file_is_an_error() {
        let proj = routed_project("route-conflict");
        proj.write(
            ".mdatron/routes.yaml",
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n- files: \"docs/2026-*.md\"\n  governed_by: GOVERNING.md\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f.code == "MDATRON-E0032" && f.severity == Severity::Error),
            "route conflict must error; got {findings:?}"
        );
    }

    // #83 confinement: a route escaping the governed tree is rejected under the
    // path-confinement codes — parent-segment (E0011) and absolute (E0010) in
    // both the files glob and governed_by, existent and non-existent targets
    // alike (the falsification clause).
    #[test]
    fn escaping_route_entries_are_rejected_under_confinement_codes() {
        for (routes, code) in [
            (
                "routes:\n- files: \"../outside/**/*.md\"\n  governed_by: GOVERNING.md\n",
                "MDATRON-E0011",
            ),
            (
                "routes:\n- files: \"docs/**/*.md\"\n  governed_by: \"../escape.md\"\n",
                "MDATRON-E0011",
            ),
            (
                "routes:\n- files: \"docs/**/*.md\"\n  governed_by: \"/etc/passwd\"\n",
                "MDATRON-E0010",
            ),
        ] {
            let proj = routed_project("route-escape");
            proj.write(".mdatron/routes.yaml", routes);
            let cfg = VerifyConfig::from_project(&proj.0).unwrap();
            let findings = verify(&cfg).unwrap();
            assert!(
                findings.iter().any(|f| f.code == code),
                "expected {code} for {routes:?}; got {findings:?}"
            );
        }
    }

    // #83 inactivity: no routes.yaml -> family inactive, no route findings.
    #[test]
    fn absent_routes_file_keeps_family_inactive() {
        let proj = routed_project("route-inactive");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().all(|f| !f.code.starts_with("MDATRON-E003")),
            "no route findings without route data; got {findings:?}"
        );
    }

    // D2 boundary: a file with frontmatter satisfies the requirement — W0040
    // is about ABSENCE, not validity (a malformed block is E0001's job).
    #[test]
    fn require_frontmatter_is_satisfied_by_present_frontmatter() {
        let proj = TempProject::new("w0040-ok");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\nrequire_frontmatter:\n  - \"docs/**/*.md\"\n",
        );
        proj.write(
            "docs/typed.md",
            "---\nschema_class: phase-primer\nphase: phase-1a\nrelevant_domains: [se]\n---\nbody\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).expect("config loads");
        let findings = verify(&cfg).unwrap();
        assert!(findings.is_empty(), "typed file is clean; got {findings:?}");
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
        // Engine-authored message names the constraint (schema-side), and the
        // failing document value is carried out-of-line in a quoted region — not
        // inline in the message (DESIGN §Output marking discipline).
        assert!(
            findings[0].message.contains("allowed options"),
            "message should describe the constraint: {}",
            findings[0].message
        );
        assert!(
            !findings[0].message.contains("invalid-phase"),
            "adopter value must not leak into the engine message: {}",
            findings[0].message
        );
        assert!(
            findings[0]
                .quoted
                .iter()
                .any(|q| q.content.contains("invalid-phase")),
            "failing value should ride in a quoted region; got {:?}",
            findings[0].quoted
        );
    }

    #[test]
    fn schema_violation_reports_source_line_not_block_start() {
        // #65: the E0050 diagnostic must point at the violation's SOURCE LINE so
        // the fixing agent edits directly, rather than the frontmatter block's
        // start. Here the bad `phase` is on file line 6, not line 1. (Red gate:
        // against the prior hardcoded `line: 1` this fails, expecting 6.)
        let proj = TempProject::new("e0050-line");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            "primer.md",
            "---\nschema_class: phase-primer\nrelevant_domains:\n  - se\n  - pe\nphase: invalid-phase\n---\n",
        );

        let cfg = VerifyConfig::new(&proj.0);
        let findings = verify(&cfg).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "MDATRON-E0050");
        assert_eq!(
            findings[0].location.line, 6,
            "E0050 must point at the violation's source line (6), not the block start (1); got {:?}",
            findings[0].location
        );
    }

    #[test]
    fn additional_property_reports_offending_key_line() {
        // #71: an additionalProperties violation's pointer is the parent object,
        // but the offending key is named in the message. E0050 should point at
        // the unexpected key's line (file line 3), not the mapping start (2) or
        // the block start (1).
        let proj = TempProject::new("e0050-addl-prop");
        proj.write(
            ".mdatron/schemas/strict-primer.json",
            r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["schema_class"],
  "properties": { "schema_class": { "const": "strict-primer" } },
  "additionalProperties": false
}"#,
        );
        proj.write(
            "primer.md",
            "---\nschema_class: strict-primer\nunexpected_key: oops\n---\n",
        );

        let cfg = VerifyConfig::new(&proj.0);
        let findings = verify(&cfg).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "MDATRON-E0050");
        assert_eq!(
            findings[0].location.line, 3,
            "E0050 must point at the unexpected key's line (3); got {:?}",
            findings[0].location
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
        // The interpolated value is marked out-of-line: the message keeps a
        // `{binding}` placeholder and the value rides in a quoted region.
        assert!(
            findings[0].message.contains("{$self.phase}")
                && !findings[0].message.contains("phase-2a"),
            "interpolated value must not be inline; got: {}",
            findings[0].message
        );
        assert!(
            findings[0]
                .quoted
                .iter()
                .any(|q| q.content.contains("phase-2a")),
            "interpolated value should ride in a quoted region; got {:?}",
            findings[0].quoted
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
      code: MDATRON-W0050
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
        assert_eq!(findings[0].code, "MDATRON-W0050");
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
        let (message, quoted) = interpolate_message("got phase: {{$self.phase}}", &ctx).unwrap();
        // The value is out-of-line: the message keeps a `{binding}` placeholder.
        assert_eq!(message, "got phase: {$self.phase}");
        assert_eq!(quoted.len(), 1);
        assert_eq!(quoted[0].label, "$self.phase");
        assert_eq!(quoted[0].content, "phase-2a");
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
        let (message, quoted) = interpolate_message("domains: {{$self.domains}}", &ctx).unwrap();
        assert_eq!(message, "domains: {$self.domains}");
        assert_eq!(quoted.len(), 1);
        assert_eq!(quoted[0].content, "se, qe");
    }

    #[test]
    fn interpolate_message_preserves_literal_text() {
        let self_v = Value::Null;
        let file_v = Value::Null;
        let project_v = Value::Null;
        let ctx = EvalContext::new(&self_v, &file_v, &project_v);
        let (message, quoted) = interpolate_message("no interpolation markers here", &ctx).unwrap();
        assert_eq!(message, "no interpolation markers here");
        assert!(quoted.is_empty());
    }
}
