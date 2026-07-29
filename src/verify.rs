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

use std::collections::{BTreeMap, BTreeSet};
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
    /// Globs whose matching files the vocabulary family scans (#97). Empty
    /// falls back to every walked file (prior behavior).
    pub vocabulary_globs: Vec<String>,
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
            vocabulary_globs: Vec::new(),
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
        cfg.vocabulary_globs = pc.vocabulary_globs;
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

/// A completed verification run: the findings plus which check families were
/// invoked (#90). The envelope's `families` field is built from `families`.
pub struct VerifyReport {
    pub findings: Vec<Finding>,
    pub families: crate::output::Families,
}

/// A completed incremental run (#102): the report plus the observable
/// visited-file scope. `visited` is `None` when the change forced a whole-tree
/// run (a `.mdatron/` input change), meaning every walked file was verified.
pub struct IncrementalReport {
    pub report: VerifyReport,
    pub visited: Option<BTreeSet<PathBuf>>,
}

/// The internal pipeline result: findings, per-family activity, and the visited
/// scope (`None` for a whole-tree run).
type RunResult = Result<
    (
        Vec<Finding>,
        crate::output::Families,
        Option<BTreeSet<PathBuf>>,
    ),
    VerifyError,
>;

/// Run the verification pipeline. Returns findings sorted by file path then code.
/// Thin wrapper over [`verify_report`] for callers that only need findings.
pub fn verify(config: &VerifyConfig) -> Result<Vec<Finding>, VerifyError> {
    verify_report(config).map(|r| r.findings)
}

/// Run the verification pipeline and report both findings and per-family
/// activity (`DESIGN.md` § Five check families; #90). A family is **active**
/// when its data was supplied and it ran this pass — independent of whether it
/// produced findings.
pub fn verify_report(config: &VerifyConfig) -> Result<VerifyReport, VerifyError> {
    let (findings, families, _visited) = run(config, None, None)?;
    Ok(VerifyReport { findings, families })
}

/// Incremental verification (#102, sub-lane C of #92): verify only the changed
/// file and its transitive dependents (#100), against the full cross-file
/// context, and report the same findings a whole-tree run would produce for
/// those files. A change under `.mdatron/` forces a whole-tree run (the input
/// data changed). `visited` is the observable scope.
pub fn verify_incremental(
    config: &VerifyConfig,
    changed: &Path,
) -> Result<IncrementalReport, VerifyError> {
    let (findings, families, visited) = run(config, Some(changed), None)?;
    Ok(IncrementalReport {
        report: VerifyReport { findings, families },
        visited,
    })
}

/// The verification pipeline. `changed == None` runs whole-tree; `Some(path)`
/// runs incrementally — verify the changed file plus its dependents, then keep
/// only the findings located in that scope, so the result equals the whole-tree
/// result filtered to the scope. A `.mdatron/` change forces whole-tree.
/// Returns `(findings, families, visited)`; `visited` is `None` for whole-tree.
fn run(
    config: &VerifyConfig,
    changed: Option<&Path>,
    on_capture_complete: Option<&dyn Fn()>,
) -> RunResult {
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

    // Vocabulary scope (#97): the naming register applies only to these globs
    // when supplied; empty falls back to every walked file (prior behavior).
    // Kept separate from file_globs so a historical archive stays walked +
    // routed without its retired handle scheme tripping the register.
    let vocab_globs: Vec<glob::Pattern> = config
        .vocabulary_globs
        .iter()
        .map(|g| {
            glob::Pattern::new(g)
                .map_err(|e| VerifyError::Config(format!("vocabulary_globs '{g}': {e}")))
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

    // Pin family (#84): mirror of the route load — absent file = inactive;
    // load-time findings include confinement drops and the standing
    // weakening annotations (L0001/W0042).
    let pin_data = match crate::pin::load(&project_root) {
        Ok(p) => p,
        Err(e) => return Err(VerifyError::Config(e.to_string())),
    };

    // Vocabulary family (#85): registry-driven prose scan; absent = inactive.
    let vocab = match crate::vocab::load(&project_root) {
        Ok(v) => v,
        Err(e) => return Err(VerifyError::Config(e.to_string())),
    };

    // Capture per-family activity before the Options are consumed (#90).
    // Active = data supplied and the family ran this pass. Citation is
    // data-less, activating only via a route opting in with `citations: true`.
    use crate::output::{Families, FamilyActivity};
    let families = Families {
        schema: FamilyActivity::from_supplied(!schemas.is_empty()),
        route: FamilyActivity::from_supplied(routes.is_some()),
        pin: FamilyActivity::from_supplied(pin_data.is_some()),
        vocabulary: FamilyActivity::from_supplied(vocab.is_some()),
        citation: FamilyActivity::from_supplied(
            routes
                .as_ref()
                .map(|r| r.routes.iter().any(|x| x.citations))
                .unwrap_or(false),
        ),
    };

    let mut findings: Vec<Finding> = Vec::new();
    // Keep the pins for after the scope filter: a pin finding locates at
    // pins.yaml but is ABOUT the pinned file, so incremental includes it by the
    // pinned file's scope membership, not by the finding's own location (#102).
    let pins: Vec<crate::pin::Pin> = if let Some(mut loaded) = pin_data {
        findings.append(&mut loaded.findings);
        loaded.pins
    } else {
        Vec::new()
    };
    let routes = match routes {
        Some(mut loaded) => {
            findings.append(&mut loaded.findings);
            Some(loaded.routes)
        }
        None => None,
    };
    // Collect the governed files once (absolute + root-relative paths).
    let mut governed: Vec<(PathBuf, PathBuf)> = Vec::new();
    for glob_pattern in &config.file_globs {
        let absolute = project_root.join(glob_pattern);
        let paths = glob::glob(&absolute.to_string_lossy())
            .map_err(|e| VerifyError::Glob(format!("'{glob_pattern}': {e}")))?;
        for entry in paths {
            let path = entry.map_err(|e| VerifyError::Glob(format!("'{glob_pattern}': {e}")))?;
            let rel = path
                .strip_prefix(&project_root)
                .unwrap_or(&path)
                .to_path_buf();
            governed.push((path, rel));
        }
    }

    // Confined snapshot of the governed-file BODY (#103, security): each
    // governed file's body is read ONCE through a confined no-follow handle, so
    // the per-file schema and rule checks run against those bytes — a mutation
    // after capture cannot change what they see, and a symlinked governed-file
    // component is refused (E0012) rather than followed, closing the raw-read
    // gap where verify_file used std::fs::read_to_string. PARTIAL (security
    // review A, tracked on #103): the cross-file index is built earlier and the
    // pin/citation families still read independently, so this is not yet the
    // single-immutable-snapshot contract for ALL inputs — only the body read.
    let mut snapshot: BTreeMap<PathBuf, String> = BTreeMap::new();
    for (abs, rel) in &governed {
        let confined = match crate::confine::confine_lexically(rel) {
            Ok(c) => c,
            // A governed file whose path escapes the root (a `file_globs` with an
            // absolute or `..` component) is a confinement violation, reported —
            // never a silent skip (#103 security review D).
            Err(v) => {
                let (code, summary) = match v {
                    crate::confine::LexicalViolation::Absolute => {
                        ("MDATRON-E0010", "governed-path-absolute")
                    }
                    crate::confine::LexicalViolation::ParentSegment => {
                        ("MDATRON-E0011", "governed-path-parent-traversal")
                    }
                };
                findings.push(Finding {
                    code: code.into(),
                    severity: Severity::Error,
                    summary: summary.into(),
                    message: "a governed file resolves outside the project root; \
                              confinement rejects it — narrow the file_globs to the \
                              governed tree"
                        .into(),
                    help: None,
                    location: Location {
                        file: abs.clone(),
                        line: 1,
                        column: 0,
                    },
                    explain_ref: Some(code.into()),
                    quoted: Vec::new(),
                });
                continue;
            }
        };
        match crate::confine::open_confined(&project_root, &confined) {
            Ok(mut handle) => {
                use std::io::Read;
                let mut content = String::new();
                if let Err(e) = handle.read_to_string(&mut content) {
                    return Err(VerifyError::Io {
                        path: abs.to_string_lossy().into_owned(),
                        error: e.to_string(),
                    });
                }
                snapshot.insert(rel.clone(), content);
            }
            Err(crate::confine::OpenViolation::Symlink { component }) => {
                findings.push(Finding {
                    code: "MDATRON-E0012".into(),
                    severity: Severity::Error,
                    summary: "symlinked-component-refused".into(),
                    message: "a governed file resolves through a symbolic link; \
                              no-follow resolution refuses it (the handle that \
                              passed confinement is the handle that is read)"
                        .into(),
                    help: Some(
                        "replace the symlink with the real file inside the \
                         governed tree"
                            .into(),
                    ),
                    location: Location {
                        file: abs.clone(),
                        line: 1,
                        column: 0,
                    },
                    explain_ref: Some("MDATRON-E0012".into()),
                    quoted: vec![QuotedRegion {
                        label: "component".into(),
                        content: component.to_string_lossy().into_owned(),
                    }],
                });
            }
            Err(crate::confine::OpenViolation::Io(e)) => {
                return Err(VerifyError::Io {
                    path: abs.to_string_lossy().into_owned(),
                    error: e.to_string(),
                });
            }
        }
    }

    // Capture-complete seam (#103): the snapshot is sealed. A test injects a
    // mutation here to prove the run reports against the snapshot bytes rather
    // than racing a real filesystem window.
    if let Some(cb) = on_capture_complete {
        cb();
    }

    // Incremental scope (#102, cold-review F3/F4): resolve the changed path to a
    // governed file by canonicalizing both sides — robust to symlinked roots
    // (/tmp -> /private/tmp) and absolute/relative forms. Anything that does not
    // resolve to a walked governed file (a `.mdatron/` change, a typo, a non-.md
    // or deleted path) forces a fail-safe whole-tree run rather than silently
    // verifying nothing.
    let governed_rels: BTreeSet<PathBuf> = governed.iter().map(|(_, rel)| rel.clone()).collect();
    let incremental_changed =
        changed.and_then(|c| resolve_changed_rel(&project_root, c, &governed_rels));

    // In incremental mode, resolve the changed file's dependents (#100) over the
    // full file/route/pattern/index context, and scope the checks to them.
    let scope: Option<BTreeSet<PathBuf>> = if let Some(changed_rel) = &incremental_changed {
        let govfiles: Vec<crate::dep::GovernedFile> = governed
            .iter()
            .map(|(_abs, rel)| crate::dep::GovernedFile {
                path: rel.clone(),
                schema_class: snapshot.get(rel).and_then(|c| read_schema_class(c)),
            })
            .collect();
        let graph = crate::dep::DepGraph::build(
            &govfiles,
            routes.as_deref().unwrap_or(&[]),
            &patterns,
            &registry,
        );
        let mut s = graph.dependents(changed_rel);
        s.insert(changed_rel.clone());
        Some(s)
    } else {
        None
    };

    // #98: track whether a scoped register matched any walked file. Whole-tree
    // only — W0043 is `.mdatron/`-located (never in an incremental scope) and
    // the incremental walk sees only part of the tree.
    let vocab_scoped = vocab.is_some() && !vocab_globs.is_empty();
    let mut vocab_scoped_hits = 0usize;
    for (path, rel) in &governed {
        // Incremental: skip files outside the scope.
        if let Some(scope) = &scope {
            if !scope.contains(rel) {
                continue;
            }
        }
        let mut cite_enabled = false;
        if let Some(routes) = &routes {
            crate::route::check_file(routes, rel, path, &mut findings);
            cite_enabled = crate::route::citations_enabled(routes, rel);
        }
        // Vocabulary scope (#97): empty globs = every walked file.
        let vocab_enabled =
            vocab_globs.is_empty() || vocab_globs.iter().any(|p| p.matches_path(rel));
        if vocab_scoped && vocab_enabled {
            vocab_scoped_hits += 1;
        }
        // Read from the immutable snapshot, never the filesystem (#103). A file
        // refused at capture (symlinked) has no snapshot entry and its E0012 was
        // already recorded.
        let Some(content) = snapshot.get(rel) else {
            continue;
        };
        verify_file(
            path,
            content,
            &project_root,
            &require,
            cite_enabled,
            vocab.as_ref().filter(|_| vocab_enabled),
            &schemas,
            &patterns,
            &registry,
            &mut findings,
        )?;
    }

    // #95: registry-level vocabulary findings (a registered-and-draft term
    // resolves to draft with a W0044 warning) — file-independent, once per run.
    if let Some(v) = vocab.as_ref() {
        let vocab_path = project_root.join(".mdatron").join(crate::vocab::VOCAB_NAME);
        crate::vocab::registry_findings(v, &vocab_path, &mut findings);
    }

    // #98: a scoped register that matched nothing is loud (whole-tree only —
    // see the vocab_scoped comment above).
    if scope.is_none() && vocab_scoped && vocab_scoped_hits == 0 {
        let config_path = project_root
            .join(".mdatron")
            .join(crate::config::CONFIG_NAME);
        findings.push(Finding {
            code: "MDATRON-W0043".into(),
            severity: Severity::Warning,
            summary: "vocabulary-scope-matches-nothing".into(),
            message: "a `vocabulary.yaml` registry is present but the \
                      `vocabulary_globs` in .mdatron/config.yaml match no walked \
                      file, so the whole vocabulary family is inert — a mistyped \
                      glob would pass silently as if there were nothing to flag"
                .into(),
            help: Some(
                "correct the `vocabulary_globs` to cover the files the register \
                 should scan, or remove the list to scan every walked file"
                    .into(),
            ),
            location: Location {
                file: config_path,
                line: 1,
                column: 0,
            },
            explain_ref: Some("MDATRON-W0043".into()),
            quoted: Vec::new(),
        });
    }

    // Incremental soundness (#102): keep only findings located within the
    // scope. verify_file findings are already scope-local (it ran on scope
    // files only); this filters the location-based whole-run findings (route
    // load, W0043/W0044) to scope.
    if let Some(scope) = &scope {
        findings.retain(|f| {
            let rel = f
                .location
                .file
                .strip_prefix(&project_root)
                .unwrap_or(&f.location.file);
            scope.contains(rel)
        });
    }

    // Pin findings (#102 B1): a pin's staleness is a finding ABOUT the pinned
    // file (E0061/E0062), but it locates at pins.yaml — so the location filter
    // above would wrongly drop it. Include it by the PINNED FILE's scope
    // membership instead (whole-tree includes every pin). Added after the
    // filter so a stale pin on a changed governed file is caught incrementally,
    // not silently missed.
    for pin in &pins {
        let relevant = scope
            .as_ref()
            .is_none_or(|s| s.contains(Path::new(&pin.file)));
        if relevant {
            crate::pin::check(&project_root, std::slice::from_ref(pin), &mut findings);
        }
    }

    findings.sort_by(|a, b| {
        a.location
            .file
            .cmp(&b.location.file)
            .then_with(|| a.code.cmp(&b.code))
    });
    Ok((findings, families, scope))
}

/// Resolve a caller-supplied changed path to its root-relative form IF it names
/// a walked governed file — canonicalizing both sides so a symlinked root or an
/// absolute/relative form still matches the graph's node identity. Returns
/// `None` otherwise, so the caller runs whole-tree: the fail-safe direction
/// (never silently verify nothing) and the fix for the `.mdatron/`-change bypass
/// (#102 cold-review F3/F4).
fn resolve_changed_rel(
    project_root: &Path,
    changed: &Path,
    governed_rels: &BTreeSet<PathBuf>,
) -> Option<PathBuf> {
    let candidate = if changed.is_absolute() {
        changed.to_path_buf()
    } else {
        project_root.join(changed)
    };
    let canon = candidate.canonicalize().ok()?;
    let rel = canon.strip_prefix(project_root).ok()?.to_path_buf();
    governed_rels.contains(&rel).then_some(rel)
}

/// A governed file's `schema_class` from its snapshot content, for the
/// dependency pre-pass (#102/#103) — a light frontmatter parse, independent of
/// the full per-file check. `None` when there is no frontmatter or class.
fn read_schema_class(content: &str) -> Option<String> {
    let (fm, _body) = crate::frontmatter::parse(content).ok().flatten()?;
    let internal = crate::dsl::index::yaml_to_value(&fm);
    internal
        .as_object()
        .and_then(|o| o.get("schema_class"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
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

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn verify_file(
    path: &Path,
    content: &str,
    project_root: &Path,
    require_frontmatter: &[glob::Pattern],
    cite_enabled: bool,
    vocab: Option<&crate::vocab::LoadedVocab>,
    schemas: &BTreeMap<String, Schema>,
    patterns: &[PatternFile],
    registry: &IndexRegistry,
    findings: &mut Vec<Finding>,
) -> Result<(), VerifyError> {
    // Content comes from the immutable snapshot (#103): every governed file is
    // read once through a confined no-follow handle in `run`, never from the
    // filesystem here — so a mutation after capture cannot change what this
    // check sees, and a symlinked component was already refused at capture.
    let content = content.to_string();

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

    let (frontmatter_value, body_len) = match fm_opt {
        Some((fm, body)) => (fm, body.len()),
        None => {
            // Opt-in loudness (#80 D2): inside a require_frontmatter glob,
            // "no frontmatter" must not be indistinguishable from "passed" —
            // the parse-ABSENCE half of #78's loud-failure/silent-absence
            // asymmetry. Matching is on the root-relative path.
            // Vocabulary scans prose-only files too (whole content as body).
            if let Some(v) = vocab {
                crate::vocab::check_file(v, path, &content, 0, None, findings);
            }
            if cite_enabled {
                crate::cite::check_file(project_root, path, &content, 0, findings);
            }
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

    // Vocabulary prose scan over the body, with frontmatter context for the
    // numeric-claims comparison (#80 D3).
    if let Some(v) = vocab {
        let body_offset = content.len() - body_len;
        crate::vocab::check_file(
            v,
            path,
            &content,
            body_offset,
            Some(&frontmatter_value),
            findings,
        );
    }
    if cite_enabled {
        let body_offset = content.len() - body_len;
        crate::cite::check_file(project_root, path, &content, body_offset, findings);
    }

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

pub(crate) fn context_matches(
    context: &ContextSelector,
    schema_class: Option<&str>,
    path: &Path,
) -> bool {
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

    // ── pin family (#84): sha256 governance over governed files ────────────

    fn pinned_project(label: &str, content: &str) -> TempProject {
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
        proj.write("governed.md", content);
        let sha = crate::init::sha256_hex(content.as_bytes());
        proj.write(
            ".mdatron/pins.yaml",
            &format!(
                "pins:\n- governing: GOVERNING.md\n  file: governed.md\n  sha256: \"{sha}\"\n"
            ),
        );
        proj
    }

    // RED GATE (#84, drawn from the live incident: codes.rs changed under
    // DESIGN's OPEN block with no signal): a governed-file change with a stale
    // pin FAILS (E0061) — and passes again after re-pin. Pre-impl, pins.yaml
    // is inert and the drift is silent.
    #[test]
    fn stale_pin_fails_and_passes_after_repin() {
        let proj = pinned_project("pin-stale", "original content\n");
        // Fresh pin: clean.
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        assert!(verify(&cfg).unwrap().is_empty());
        // The governed file changes; the pin is now stale: FAIL.
        proj.write("governed.md", "changed content\n");
        let findings = verify(&cfg).unwrap();
        let f = findings
            .iter()
            .find(|f| f.code == "MDATRON-E0061")
            .unwrap_or_else(|| panic!("expected E0061 pin-stale; got {findings:?}"));
        assert_eq!(f.severity, Severity::Error);
        assert!(f.quoted.iter().any(|q| q.content.contains("governed.md")));
        // Re-pin (the single-command recompute, as a library call): clean again.
        let updated = crate::pin::update(&proj.0, false).unwrap();
        assert_eq!(updated.len(), 1, "one pin recomputed");
        assert!(verify(&cfg).unwrap().is_empty(), "re-pin restores clean");
    }

    // RED GATE (#84): a pin whose target is gone is E0062, not silence.
    #[test]
    fn missing_pin_target_is_e0062() {
        let proj = pinned_project("pin-missing", "content\n");
        std::fs::remove_file(proj.0.join("governed.md")).unwrap();
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f.code == "MDATRON-E0062" && f.severity == Severity::Error),
            "expected E0062; got {findings:?}"
        );
    }

    // #84 confinement: escaping pin targets rejected under the confinement
    // codes, existent and non-existent alike (falsification clause).
    #[test]
    fn escaping_pin_entries_are_rejected() {
        for (pins, code) in [
            (
                "pins:\n- governing: GOVERNING.md\n  file: \"../escape.md\"\n  sha256: \"00\"\n",
                "MDATRON-E0011",
            ),
            (
                "pins:\n- governing: GOVERNING.md\n  file: \"/etc/passwd\"\n  sha256: \"00\"\n",
                "MDATRON-E0010",
            ),
        ] {
            let proj = pinned_project("pin-escape", "content\n");
            proj.write(".mdatron/pins.yaml", pins);
            let cfg = VerifyConfig::from_project(&proj.0).unwrap();
            let findings = verify(&cfg).unwrap();
            assert!(
                findings.iter().any(|f| f.code == code),
                "expected {code}; got {findings:?}"
            );
        }
    }

    // RED GATE (#84, DESIGN §Validation is data-driven): a standing tombstoned
    // weakening (unpinned entry with justification) emits its informational
    // lint on every whole-tree run (L0001); an UNJUSTIFIED weakening is
    // flagged louder (W0042).
    #[test]
    fn unpinned_annotations_stay_loud() {
        let proj = pinned_project("pin-unpinned", "content\n");
        proj.write(
            ".mdatron/pins.yaml",
            "pins: []\nunpinned:\n- file: governed.md\n  governing: GOVERNING.md\n  reason: \"governance moved to route naming\"\n  owner: operator\n- file: other.md\n  governing: GOVERNING.md\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f.code == "MDATRON-L0001" && f.severity == Severity::Lint),
            "justified tombstone emits the standing lint; got {findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|f| f.code == "MDATRON-W0042" && f.severity == Severity::Warning),
            "unjustified weakening is flagged; got {findings:?}"
        );
    }

    // #84 inactivity: no pins.yaml -> family inactive.
    #[test]
    fn absent_pins_file_keeps_family_inactive() {
        let proj = routed_project("pin-inactive");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().all(|f| !f.code.starts_with("MDATRON-E006")),
            "no pin findings without pin data; got {findings:?}"
        );
    }

    // ── vocabulary family (#85): registry-driven prose scan ────────────────

    fn vocab_project(label: &str, vocab: &str, body: &str) -> TempProject {
        let proj = TempProject::new(label);
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write(".mdatron/vocabulary.yaml", vocab);
        proj.write(
            "docs/doc.md",
            &format!(
                "---\nschema_class: phase-primer\nphase: phase-1a\nrelevant_domains: [se, qe, pe]\n---\n{body}"
            ),
        );
        proj
    }

    fn codes_of(findings: &[Finding], code: &str) -> usize {
        findings.iter().filter(|f| f.code == code).count()
    }

    // RED GATE (#85): a bold-introduced term absent from the registry is
    // flagged (E0090); registered AND draft terms are exempt (draft-status
    // exemption per contract). Pre-impl, vocabulary.yaml is inert.
    #[test]
    fn unregistered_coinage_flagged_registered_and_draft_exempt() {
        let proj = vocab_project(
            "vocab-coinage",
            "terms:\n- term: fix-lane\n  status: registered\n  sense: \"the gated change lane\"\n- term: half-idea\n  status: draft\n  sense: \"tbd\"\n",
            "The **fix-lane** holds. The **half-idea** floats. The **brand-new-coinage** lands.\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-E0090"),
            1,
            "only the unregistered coinage flags; got {findings:?}"
        );
        let f = findings.iter().find(|f| f.code == "MDATRON-E0090").unwrap();
        assert!(f.quoted.iter().any(|q| q.content == "brand-new-coinage"));
        assert!(
            !f.message.contains("brand-new-coinage"),
            "term rides quoted"
        );
    }

    // RED GATE (#85): a letter+number cluster outside the allowlist is an
    // invented label scheme (E0091); allowed schemes pass. Line is precise.
    #[test]
    fn invented_label_scheme_outside_allowlist_flagged() {
        let proj = vocab_project(
            "vocab-cluster",
            "label_schemes:\n  allow:\n  - \"^MDATRON-[ELW][0-9]{4}$\"\n",
            "Fine: MDATRON-E0050.\nInvented: SEC-F3 recurs.\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(codes_of(&findings, "MDATRON-E0091"), 1, "got {findings:?}");
        let f = findings.iter().find(|f| f.code == "MDATRON-E0091").unwrap();
        assert!(f.quoted.iter().any(|q| q.content == "SEC-F3"));
        assert_eq!(f.location.line, 7, "body line 2 = file line 7 (5 fm lines)");
    }

    // #97: vocabulary_globs scopes the register. A cluster in an in-scope file
    // fires E0091; the same shape in a file that is walked but OUTSIDE
    // vocabulary_globs stays silent — the mechanism that keeps a historical
    // archive's retired handles frozen while the register governs live prose.
    #[test]
    fn vocabulary_globs_scopes_the_register() {
        let proj = TempProject::new("vocab-scope");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"live/**/*.md\"\n  - \"archive/**/*.md\"\nvocabulary_globs:\n  - \"live/**/*.md\"\n",
        );
        proj.write(
            ".mdatron/vocabulary.yaml",
            "label_schemes:\n  allow:\n  - \"^MDATRON-[ELW][0-9]{4}$\"\n",
        );
        proj.write("live/doc.md", "Coined here: ZQ7 recurs.\n");
        proj.write("archive/old.md", "Frozen handle: SEC-F3 stays.\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-E0091"),
            1,
            "only the in-scope file's cluster fires; got {findings:?}"
        );
        let f = findings.iter().find(|f| f.code == "MDATRON-E0091").unwrap();
        assert!(f.quoted.iter().any(|q| q.content == "ZQ7"));
        assert!(
            f.location.file.ends_with("live/doc.md"),
            "the finding is on the in-scope file, not the archive"
        );
    }

    // #98 red gate: vocabulary.yaml present (register active) but
    // vocabulary_globs matches no walked file -> the whole family is silently
    // inert. W0043 makes that loud, so a mistyped glob is not indistinguishable
    // from "nothing to flag" (the F1 footgun routed from #97).
    #[test]
    fn vocabulary_globs_matching_nothing_is_loud() {
        let proj = TempProject::new("vocab-zero-match");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"live/**/*.md\"\nvocabulary_globs:\n  - \"does-not-exist/**/*.md\"\n",
        );
        proj.write(
            ".mdatron/vocabulary.yaml",
            "label_schemes:\n  allow:\n  - \"^MDATRON-[ELW][0-9]{4}$\"\n",
        );
        proj.write("live/doc.md", "Coined: ZQ7 here.\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-W0043"),
            1,
            "a zero-match active register must be loud; got {findings:?}"
        );
        // The register really was inert — the coined cluster went unscanned.
        assert_eq!(
            codes_of(&findings, "MDATRON-E0091"),
            0,
            "the register was scoped away from the only walked file"
        );
    }

    // #97 (phase-3 F2): the scope gate covers the frontmatter branch too, not
    // just prose-only files. Both files carry frontmatter (the review-log's own
    // shape, and the only path where E0094 can fire), so this exercises the
    // second `vocab::check_file` call site — the out-of-scope cluster must still
    // stay silent. Guards a refactor that split the gate and left the
    // frontmatter branch scanning unconditionally.
    #[test]
    fn vocabulary_globs_scopes_frontmatter_files() {
        let proj = TempProject::new("vocab-scope-fm");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"live/**/*.md\"\n  - \"archive/**/*.md\"\nvocabulary_globs:\n  - \"live/**/*.md\"\n",
        );
        proj.write(
            ".mdatron/vocabulary.yaml",
            "label_schemes:\n  allow:\n  - \"^MDATRON-[ELW][0-9]{4}$\"\n",
        );
        let fm = "---\nschema_class: phase-primer\nphase: phase-1a\nrelevant_domains: [se, qe, pe]\n---\n";
        proj.write("live/doc.md", &format!("{fm}Coined: ZQ7 here.\n"));
        proj.write("archive/old.md", &format!("{fm}Frozen: SEC-F3 stays.\n"));
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-E0091"),
            1,
            "only the in-scope frontmattered file's cluster fires; got {findings:?}"
        );
        let f = findings.iter().find(|f| f.code == "MDATRON-E0091").unwrap();
        assert!(
            f.location.file.ends_with("live/doc.md"),
            "the finding is on the in-scope file, not the archive"
        );
    }

    // #97: an empty (absent) vocabulary_globs falls back to every walked file —
    // the register still bites without the scoping field, so configs predating
    // it are unaffected.
    #[test]
    fn empty_vocabulary_globs_scans_every_walked_file() {
        let proj = TempProject::new("vocab-scope-empty");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"live/**/*.md\"\n",
        );
        proj.write(
            ".mdatron/vocabulary.yaml",
            "label_schemes:\n  allow:\n  - \"^MDATRON-[ELW][0-9]{4}$\"\n",
        );
        proj.write("live/doc.md", "Coined here: ZQ7 recurs.\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-E0091"),
            1,
            "no vocabulary_globs means the register still scans; got {findings:?}"
        );
    }

    // #95: a term declared both registered and draft resolves to draft (the
    // permissive status) and names a W0044 warning — conflicting adopter data
    // yields a diagnostic, not a silent first-wins pick (DESIGN acceptance).
    #[test]
    fn registered_and_draft_term_resolves_to_draft_with_warning() {
        let proj = vocab_project(
            "vocab-status-conflict",
            "terms:\n- term: fixture\n  status: registered\n  sense: \"a\"\n- term: fixture\n  status: draft\n  sense: \"b\"\n",
            "The **fixture** holds. The **coinage** floats.\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-W0044"),
            1,
            "the registered-and-draft conflict is warned once; got {findings:?}"
        );
        // Resolved to draft (in the registry, so exempt) — only the genuinely
        // unregistered coinage flags E0090.
        assert_eq!(
            codes_of(&findings, "MDATRON-E0090"),
            1,
            "only the unregistered 'coinage' flags; got {findings:?}"
        );
        let w = findings.iter().find(|f| f.code == "MDATRON-W0044").unwrap();
        assert!(
            w.quoted.iter().any(|q| q.content == "fixture"),
            "the conflicting term rides a quoted region"
        );
        assert!(!w.message.contains("fixture"), "the term is not inline");
    }

    // RED GATE (#85): a reserved-status term appearing in prose is flagged
    // (E0092) — reserved means held, not usable.
    #[test]
    fn reserved_word_use_is_flagged() {
        let proj = vocab_project(
            "vocab-reserved",
            "terms:\n- term: mcp-serve\n  status: reserved\n  sense: \"reserved subcommand spelling\"\n",
            "Someday mcp-serve will exist.\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(codes_of(&findings, "MDATRON-E0092"), 1, "got {findings:?}");
    }

    // RED GATE (#85): a listed register anti-pattern match is flagged (E0093)
    // with the matched prose quoted, never inline.
    #[test]
    fn register_anti_pattern_flagged() {
        let proj = vocab_project(
            "vocab-anti",
            "anti_patterns:\n- pattern: \"very unique\"\n  register: hedged-absolute\n",
            "This is a very unique approach.\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(codes_of(&findings, "MDATRON-E0093"), 1, "got {findings:?}");
        let f = findings.iter().find(|f| f.code == "MDATRON-E0093").unwrap();
        assert!(f.message.contains("hedged-absolute"));
        assert!(f.quoted.iter().any(|q| q.content.contains("very unique")));
    }

    // RED GATE (#85 / #80 D3, the vsdd drift class): a prose numeral
    // restating a configured field's count and drifting from it is flagged
    // (E0094); an agreeing numeral is clean. Configured references only.
    #[test]
    fn numeric_claim_drift_flagged_agreement_clean() {
        let proj = vocab_project(
            "vocab-numeric",
            "numeric_claims:\n- field: relevant_domains\n",
            "This review spans 4 relevant domains in total.\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(codes_of(&findings, "MDATRON-E0094"), 1, "got {findings:?}");

        let proj2 = vocab_project(
            "vocab-numeric-ok",
            "numeric_claims:\n- field: relevant_domains\n",
            "This review spans 3 relevant domains in total.\n",
        );
        let cfg2 = VerifyConfig::from_project(&proj2.0).unwrap();
        let findings2 = verify(&cfg2).unwrap();
        assert_eq!(
            codes_of(&findings2, "MDATRON-E0094"),
            0,
            "agreement is clean; got {findings2:?}"
        );
    }

    // #85: word-numbers count too (the vsdd 'seven core domains' case shape).
    #[test]
    fn numeric_claim_word_number_drift_flagged() {
        let proj = vocab_project(
            "vocab-word-num",
            "numeric_claims:\n- field: relevant_domains\n",
            "Covers seven relevant domains.\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(codes_of(&findings, "MDATRON-E0094"), 1, "got {findings:?}");
    }

    // #94: vsdd-cli's three real E0094 numeric-claims drift cases, intaken
    // verbatim as regression fixtures (vsdd-cli rounds #684/#705/#710). Cases
    // 1-2 are count drift E0094 catches; case 3 is the unperformable-basis class
    // it does NOT catch (the digit agrees but its verification path dangles) —
    // pinned as the documented boundary of the check.
    #[test]
    fn intake_vsdd_numeric_claims_drift_cases() {
        fn e0094(label: &str, frontmatter: &str, body: &str, fields: &[&str]) -> usize {
            let proj = TempProject::new(label);
            proj.write(".mdatron/schemas/.keep.json", "{}");
            proj.write(
                ".mdatron/config.yaml",
                "file_globs:\n  - \"docs/**/*.md\"\n",
            );
            let claims: String = fields.iter().map(|f| format!("- field: {f}\n")).collect();
            proj.write(
                ".mdatron/vocabulary.yaml",
                &format!("numeric_claims:\n{claims}"),
            );
            proj.write("docs/d.md", &format!("---\n{frontmatter}---\n{body}"));
            let cfg = VerifyConfig::from_project(&proj.0).unwrap();
            codes_of(&verify(&cfg).unwrap(), "MDATRON-E0094")
        }

        // CASE 1 — plain count drift (installed-artifact-manifest.md:83): prose
        // "27 members / 17 domain prompts" vs disk 28 / 18. Both drift.
        assert_eq!(
            e0094(
                "e0094-case1",
                "members: 28\ndomain_prompts: 18\n",
                "There are 27 members and 17 domain prompts on disk.\n",
                &["members", "domain_prompts"],
            ),
            2,
            "case 1: plain count drift caught on both fields"
        );

        // CASE 2 — decomposed drift (economics-data.md:35): prose "18 total: 15
        // role + 2 meta" vs disk total 18 / role 16 / meta 2. The role component
        // drifted (15 vs 16) while the arithmetic stayed internally consistent
        // (15+2=17); E0094 checks each component against its basis, catching it.
        assert_eq!(
            e0094(
                "e0094-case2",
                "total: 18\nrole: 16\nmeta: 2\n",
                "The 18 total breaks down as 15 role and 2 meta.\n",
                &["total", "role", "meta"],
            ),
            1,
            "case 2: the drifted role component is caught; total and meta agree"
        );

        // CASE 3 — unperformable basis (the high-value case): prose "six core
        // role domains" where the configured field's count IS six. The digit
        // agrees, so E0094 stays clean — it cannot see that the six-member
        // subset is enumerated nowhere. This pins the BOUNDARY: E0094 mechanizes
        // "the number is wrong," not "the number's verification path doesn't
        // resolve" — the harder, higher-value half, not yet mechanized.
        assert_eq!(
            e0094(
                "e0094-case3",
                "role_domains: 6\n",
                "There are six core role domains.\n",
                &["role_domains"],
            ),
            0,
            "case 3: the digit agrees, so E0094 is clean — the unperformable-basis class is beyond it"
        );
    }

    // #85 inactivity: no vocabulary.yaml -> family inactive.
    #[test]
    fn absent_vocabulary_file_keeps_family_inactive() {
        let proj = routed_project("vocab-inactive");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().all(|f| !f.code.starts_with("MDATRON-E009")),
            "no vocab findings without vocab data; got {findings:?}"
        );
    }

    // ── citation family (#86): file:line verification vs the snapshot ──────

    fn cite_project(label: &str, body: &str) -> TempProject {
        let proj = TempProject::new(label);
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("GOVERNING.md", "# gov\n");
        proj.write(
            ".mdatron/routes.yaml",
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n  citations: true\n",
        );
        proj.write("src-file.rs", "line1\nline2\nline3\nline4\nline5\n");
        proj.write("docs/2026-07-27-doc.md", &format!("---\nschema_class: phase-primer\nphase: phase-1a\nrelevant_domains: [se]\n---\n{body}"));
        proj
    }

    // RED GATE (#86): a citation naming absent content is rejected (E0100)
    // against the working-tree snapshot. Pre-impl, citations are inert.
    #[test]
    fn dead_citation_rejected() {
        let proj = cite_project("cite-dead", "Per no-such-file.rs:12 this holds.\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let f = findings
            .iter()
            .find(|f| f.code == "MDATRON-E0100")
            .unwrap_or_else(|| panic!("expected E0100; got {findings:?}"));
        assert!(f
            .quoted
            .iter()
            .any(|q| q.content.contains("no-such-file.rs:12")));
        assert_eq!(
            f.location.line, 6,
            "citation sits on body line 1 = file line 6"
        );
    }

    // RED GATE (#86): a citation past the target's end is E0101; one in range
    // is clean — including a RANGE form.
    #[test]
    fn citation_line_out_of_range() {
        let proj = cite_project(
            "cite-range",
            "Good: src-file.rs:5 and src-file.rs:2-4.\nBad: src-file.rs:99 and src-file.rs:2-9.\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.code == "MDATRON-E0101")
                .count(),
            2,
            "exactly the two out-of-range citations flag; got {findings:?}"
        );
        assert!(findings.iter().all(|f| f.code != "MDATRON-E0100"));
    }

    // #86: uncommitted content COUNTS (working tree authoritative, no git):
    // a citation to a file that was never committed anywhere is live.
    #[test]
    fn uncommitted_target_counts_as_live() {
        let proj = cite_project("cite-uncommitted", "See fresh.md:1.\n");
        proj.write("fresh.md", "only line\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().all(|f| !f.code.starts_with("MDATRON-E010")),
            "uncommitted-but-present target is live; got {findings:?}"
        );
    }

    // #86 confinement: escaping citations rejected under the confinement codes.
    #[test]
    fn escaping_citations_rejected() {
        let proj = cite_project(
            "cite-escape",
            "Bad: ../outside.rs:3 and /etc/passwd.txt:1.\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().any(|f| f.code == "MDATRON-E0011"),
            "parent-segment citation refused; got {findings:?}"
        );
        assert!(
            findings.iter().any(|f| f.code == "MDATRON-E0010"),
            "absolute citation refused; got {findings:?}"
        );
    }

    // #86 opt-in: a route WITHOUT citations: true gets no citation findings —
    // historical corpora stay archival (the amendment recorded on #86).
    #[test]
    fn citations_are_per_route_opt_in() {
        let proj = cite_project("cite-optout", "Per no-such-file.rs:12.\n");
        proj.write(
            ".mdatron/routes.yaml",
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().all(|f| !f.code.starts_with("MDATRON-E010")),
            "no opt-in, no citation findings; got {findings:?}"
        );
    }

    // #88 (#47 cold-run finding): an ARRAY selection fans out to one index
    // entry per element — the natural registry shape (array of objects) is
    // indexable. Pre-fix, indexed_by hit the whole array and errored.
    #[test]
    fn keys_array_selection_fans_out_per_element() {
        let proj = TempProject::new("fanout");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write(
            "registry/domains.md",
            "---\nschema_class: domain-registry\nentries:\n- id: se\n  pairs_with: qe\n- id: qe\n  pairs_with: se\n---\n",
        );
        proj.write(
            ".mdatron/patterns/p.yaml",
            r#"mdatron_dsl_version: 1
pattern:
  id: fanout
  keys:
    - name: domains
      source: "registry/domains.md"
      select: "$.frontmatter.entries"
      indexed_by: "$.id"
  rules:
    - id: r
      context: work-entry
      assert: 'every(d in $self.relevant_domains, defined(key("domains", $d)))'
      code: T-E0001
      message: "unresolvable domain"
"#,
        );
        proj.write(
            "docs/good.md",
            "---\nschema_class: work-entry\nrelevant_domains: [se, qe]\n---\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        assert!(verify(&cfg).unwrap().is_empty(), "both ids resolve");
        proj.write(
            "docs/good.md",
            "---\nschema_class: work-entry\nrelevant_domains: [se, zz]\n---\n",
        );
        let findings = verify(&cfg).unwrap();
        assert_eq!(findings.len(), 1, "zz misses; got {findings:?}");
        assert_eq!(findings[0].code, "T-E0001");
    }

    // #102 soundness oracle (cold-review F2): the reference scope is HAND-
    // AUTHORED, independent of the engine's own resolution (DESIGN forbids a
    // self-referential oracle). Asserts both that the resolver's visited set
    // equals the hand-authored scope (catching a too-few/too-many resolver) and
    // that incremental findings equal the whole-tree result filtered to it.
    fn assert_incremental_sound(root: &Path, changed: &str, expected_scope: &[&str]) {
        let cfg = VerifyConfig::from_project(root).unwrap();
        let whole = verify_report(&cfg).unwrap().findings;
        let inc = verify_incremental(&cfg, Path::new(changed)).unwrap();
        let visited = inc
            .visited
            .expect("an in-tree change scopes, not whole-tree");
        let expected_visited: BTreeSet<PathBuf> =
            expected_scope.iter().map(PathBuf::from).collect();
        assert_eq!(
            visited, expected_visited,
            "resolver scope for {changed} must equal the hand-authored dependent set"
        );
        let canon = root.canonicalize().unwrap();
        let loc = |f: &Finding| -> (String, PathBuf, u32) {
            let rel = f
                .location
                .file
                .strip_prefix(&canon)
                .unwrap_or(&f.location.file)
                .to_path_buf();
            (f.code.clone(), rel, f.location.line)
        };
        let expected: Vec<_> = whole
            .iter()
            .filter(|f| expected_visited.contains(&loc(f).1))
            .map(loc)
            .collect();
        let got: Vec<_> = inc.report.findings.iter().map(loc).collect();
        assert_eq!(
            got, expected,
            "incremental({changed}) must equal whole-tree filtered to the hand-authored scope"
        );
    }

    #[test]
    fn incremental_equals_whole_tree_filtered_to_scope() {
        let proj = TempProject::new("incremental-sound");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(".mdatron/config.yaml", "file_globs:\n  - \"**/*.md\"\n");
        proj.write(
            ".mdatron/patterns/p.yaml",
            r#"mdatron_dsl_version: 1
pattern:
  id: cross
  keys:
    - name: domains
      source: "registry/domains.md"
      select: "$.frontmatter.entries"
      indexed_by: "$.id"
    - name: others
      source: "registry/others.md"
      select: "$.frontmatter.entries"
      indexed_by: "$.id"
  rules:
    - id: dr
      context: work-entry
      assert: 'every(d in $self.relevant_domains, defined(key("domains", $d)))'
      code: T-E0001
      message: "unresolvable domain"
    - id: orr
      context: other-entry
      assert: 'every(d in $self.relevant, defined(key("others", $d)))'
      code: T-E0002
      message: "unresolvable other"
"#,
        );
        proj.write(
            "registry/domains.md",
            "---\nschema_class: domain-registry\nentries:\n- id: se\n- id: qe\n---\n",
        );
        proj.write(
            "registry/others.md",
            "---\nschema_class: other-registry\nentries:\n- id: x\n---\n",
        );
        // Each fires (a missing domain/other); the other-entry is coupled to a
        // different registry, so it stays out of the work-entry cluster's scope.
        proj.write(
            "docs/a.md",
            "---\nschema_class: work-entry\nrelevant_domains: [se, zz]\n---\n",
        );
        proj.write(
            "docs/b.md",
            "---\nschema_class: work-entry\nrelevant_domains: [qe, ww]\n---\n",
        );
        proj.write(
            "docs/other.md",
            "---\nschema_class: other-entry\nrelevant: [x, yy]\n---\n",
        );

        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        assert_eq!(
            verify_report(&cfg).unwrap().findings.len(),
            3,
            "three findings whole-tree"
        );

        // Soundness across several changed files.
        // Hand-authored dependent sets (independent of the resolver): the
        // work-entries couple through registry/domains.md; the other-entry
        // couples through registry/others.md only.
        assert_incremental_sound(
            &proj.0,
            "docs/a.md",
            &["docs/a.md", "docs/b.md", "registry/domains.md"],
        );
        assert_incremental_sound(
            &proj.0,
            "docs/other.md",
            &["docs/other.md", "registry/others.md"],
        );
        assert_incremental_sound(
            &proj.0,
            "registry/domains.md",
            &["docs/a.md", "docs/b.md", "registry/domains.md"],
        );

        // The work-entry cluster's scope excludes the unrelated other-entry.
        let inc_a = verify_incremental(&cfg, Path::new("docs/a.md")).unwrap();
        let visited = inc_a.visited.unwrap();
        assert!(visited.contains(Path::new("docs/a.md")));
        assert!(
            visited.contains(Path::new("docs/b.md")),
            "cluster peer via the registry"
        );
        assert!(
            !visited.contains(Path::new("docs/other.md")),
            "unrelated file is not in scope"
        );
        assert_eq!(
            inc_a.report.findings.len(),
            2,
            "only the cluster's findings"
        );

        // A .mdatron/ change forces a whole-tree run.
        let inc_m = verify_incremental(&cfg, Path::new(".mdatron/config.yaml")).unwrap();
        assert!(
            inc_m.visited.is_none(),
            "a .mdatron/ change forces whole-tree"
        );
        assert_eq!(
            inc_m.report.findings.len(),
            3,
            "whole-tree fallback sees all"
        );

        // A non-governed / mistyped path fails safe to whole-tree (F3/F4) — it
        // never silently verifies nothing.
        let inc_typo = verify_incremental(&cfg, Path::new("docs/nope.md")).unwrap();
        assert!(
            inc_typo.visited.is_none(),
            "a non-governed path fails safe to whole-tree"
        );
        assert_eq!(inc_typo.report.findings.len(), 3);
    }

    // #102 B1: a stale pin on a governed file is caught INCREMENTALLY when that
    // file is in scope — even though the finding locates at pins.yaml. An
    // unrelated changed file's scope excludes it.
    #[test]
    fn incremental_includes_stale_pin_for_in_scope_file() {
        let proj = TempProject::new("incremental-pin");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("docs/x.md", "---\nschema_class: doc\n---\n# x\n");
        proj.write("docs/other.md", "---\nschema_class: doc\n---\n# other\n");
        // A pin over docs/x.md with a deliberately wrong sha -> stale (E0061).
        proj.write(
            ".mdatron/pins.yaml",
            "pins:\n- governing: docs/other.md\n  file: docs/x.md\n  sha256: \
             0000000000000000000000000000000000000000000000000000000000000000\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let e0061 = |fs: &[Finding]| fs.iter().filter(|f| f.code == "MDATRON-E0061").count();
        assert_eq!(
            e0061(&verify_report(&cfg).unwrap().findings),
            1,
            "whole-tree sees the stale pin"
        );
        let inc_x = verify_incremental(&cfg, Path::new("docs/x.md")).unwrap();
        assert_eq!(
            e0061(&inc_x.report.findings),
            1,
            "stale pin caught incrementally for the pinned file"
        );
        let inc_o = verify_incremental(&cfg, Path::new("docs/other.md")).unwrap();
        assert_eq!(
            e0061(&inc_o.report.findings),
            0,
            "stale pin not surfaced for an unrelated scope"
        );
    }

    // #103: the governed-file BODY snapshot is immutable — a mutation injected
    // at the capture-complete seam does not change what the per-file schema/rule
    // checks verify (they report against the snapshot bytes), deterministically
    // via the seam. Scoped to the body read: the pin/citation families read
    // separately today (security review A/B — full-input unification is remaining
    // #103 work), so this fixture uses no pins and no citations.
    #[test]
    fn snapshot_body_is_immutable_against_mutation_at_capture_seam() {
        let proj = TempProject::new("snapshot-immutable");
        proj.write(
            ".mdatron/schemas/blog.json",
            r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","required":["schema_class","title"],"properties":{"schema_class":{"const":"blog"},"title":{"type":"string"}},"additionalProperties":false}"#,
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        // Valid at capture time (has the required title).
        proj.write("docs/d.md", "---\nschema_class: blog\ntitle: ok\n---\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();

        // At the seam (snapshot sealed), mutate the file to a schema-violating
        // form. The run must NOT see it — it verifies the snapshot bytes.
        let doc = proj.0.join("docs/d.md");
        let mutate = || {
            std::fs::write(&doc, "---\nschema_class: blog\n---\n").unwrap();
        };
        let (findings, _fam, _vis) = run(&cfg, None, Some(&mutate)).unwrap();
        assert!(
            findings.is_empty(),
            "the run reports against snapshot bytes, not the post-capture mutation; got {findings:?}"
        );

        // A subsequent run captures a fresh snapshot and DOES see the mutation.
        let after = verify_report(&cfg).unwrap().findings;
        assert_eq!(
            codes_of(&after, "MDATRON-E0050"),
            1,
            "a later run sees the now-invalid file"
        );
    }

    // #103 security: a symlinked governed file is REFUSED at capture (E0012),
    // not followed — closing the raw-read gap where verify_file read the
    // symlink's target.
    #[cfg(unix)]
    #[test]
    fn symlinked_governed_file_is_refused_not_followed() {
        let proj = TempProject::new("snapshot-symlink");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("secret.md", "---\nschema_class: x\n---\n");
        proj.write("docs/real.md", "---\nschema_class: x\n---\n");
        // docs/link.md is a symlink to ../secret.md — a symlinked governed file.
        let link = proj.0.join("docs/link.md");
        std::os::unix::fs::symlink("../secret.md", &link).unwrap();
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-E0012"),
            1,
            "the symlinked governed file is refused; got {findings:?}"
        );
    }

    // #89 (#47 cold-run finding): chained let bindings evaluate in declaration
    // order end-to-end — `missing` (alphabetically first) references
    // `required` (declared first). Pre-fix: 'undefined binding: required'.
    #[test]
    fn chained_let_bindings_evaluate_in_declaration_order() {
        let proj = TempProject::new("let-chain");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write(
            ".mdatron/patterns/p.yaml",
            r#"mdatron_dsl_version: 1
pattern:
  id: chain
  rules:
    - id: r
      context: work-entry
      let:
        required: '["a", "b"]'
        missing: 'difference($required, $self.covers)'
      assert: 'count($missing) == 0'
      code: T-E0002
      message: "missing entries"
"#,
        );
        proj.write(
            "docs/d.md",
            "---\nschema_class: work-entry\ncovers: [a]\n---\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            findings.len(),
            1,
            "chained lets evaluate and the rule fires; got {findings:?}"
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

    // RED GATE (#93, vsdd rank-3): the exactly-one rule shape — count over a
    // predicate-filtered array — works end to end. Two lanes fire; one is clean.
    #[test]
    fn exactly_one_via_count_filter_runs_end_to_end() {
        let proj = TempProject::new("filter-e2e");
        proj.write(
            ".mdatron/patterns/lanes.yaml",
            r#"mdatron_dsl_version: 1
pattern:
  id: lane-count
  rules:
    - id: exactly-one-lane
      context: registry
      assert: 'count(filter(m in $self.members, $m.kind == "lane")) == 1'
      code: TEST-E0009
      message: "the registry must declare exactly one lane member"
"#,
        );
        // Two lanes -> fires.
        proj.write(
            "bad.md",
            "---\nschema_class: registry\nmembers:\n- kind: lane\n- kind: domain\n- kind: lane\n---\n",
        );
        let cfg = VerifyConfig::new(&proj.0);
        let findings = verify(&cfg).unwrap();
        assert_eq!(findings.len(), 1, "two lanes must fire; got {findings:?}");
        assert_eq!(findings[0].code, "TEST-E0009");

        // Exactly one lane -> clean.
        proj.write(
            "bad.md",
            "---\nschema_class: registry\nmembers:\n- kind: lane\n- kind: domain\n---\n",
        );
        assert!(verify(&cfg).unwrap().is_empty(), "one lane is clean");
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
