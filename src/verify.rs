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
    evaluate, parse_expression, parse_pattern_file, ContextSelector, EvalContext, EvalError, Expr,
    IndexError, IndexRegistry, PatternFile, Rule, Value, VarRef,
};
use crate::frontmatter;
use crate::schema::{FieldPathStatus, Schema};

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
                // Project-root-RELATIVE path (roast A1, #140): the config always
                // lives at this relative location, and an absolute path baked into
                // this free-form message leaks the host layout into
                // pipeline_error.message — the DEF4 leak `relativize_paths` cannot
                // reach in prose (this is the most common pipeline failure).
                "no jurisdiction declared: '.mdatron/{}' is missing — run `mdatron \
                 init` to seed it, or pass explicit --files globs for an ad-hoc run",
                crate::config::CONFIG_NAME
            )));
        };
        // A present config that declares NO file_globs must refuse, not silently
        // fall back to the whole-tree `**/*.md` default (#125, roast SHO7): that
        // fallback is the overreach #80-D1 forbids, and — because unknown fields
        // are tolerated — a key typo (`file_glob:`) deserializes to empty and
        // would silently broaden jurisdiction to the entire tree. The `**/*.md`
        // default is reserved for the explicit `--files`/`new` ad-hoc path.
        if pc.file_globs.is_empty() {
            return Err(crate::Error::Config(format!(
                // Project-root-relative path (roast A1, #140) — see above.
                "'.mdatron/{}' declares no `file_globs`: jurisdiction must be \
                 explicit (#80-D1). Add a `file_globs` list (check for a typo'd \
                 key), or pass explicit `--files` globs for an ad-hoc run.",
                crate::config::CONFIG_NAME
            )));
        }
        cfg.file_globs = pc.file_globs;
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

    // A declared hook-time resource bound was exceeded (#124, roast SHO1). Loud
    // by design (DESIGN § Verification is fast where invoked: "exceeding a bound
    // is itself a diagnostic — no silent degradation").
    #[error("resource bound exceeded ({bound}): {detail}")]
    BoundExceeded { bound: String, detail: String },
}

/// Declared hook-time resource limits (#124). Shipped as constants (the "declared
/// limits" of DESIGN § Verification is fast where invoked). Generous for real
/// governed markdown; a hostile oversized or deeply-nested input trips a loud
/// `BoundExceeded` (pipeline_error kind `bound_exceeded`) instead of exhausting
/// CPU/memory silently.
pub const MAX_FILE_BYTES: usize = crate::limits::SHIPPED.per_file_bytes;
pub const MAX_AGGREGATE_BYTES: usize = crate::limits::SHIPPED.aggregate_bytes;
pub const MAX_STRUCTURAL_NESTING: usize = crate::limits::SHIPPED.structural_nesting;

/// Maximum flow-collection nesting depth (`[`/`{`) in a governed file (#124,
/// roast SHO1 depth-bomb). O(n) pre-scan: a compact deeply-nested collection is
/// the cheapest way to drive quadratic YAML-parse blowup, and 256 is far beyond
/// any legitimate frontmatter.
pub fn max_flow_nesting(s: &str) -> usize {
    let mut depth = 0usize;
    let mut max = 0usize;
    for b in s.bytes() {
        match b {
            b'[' | b'{' => {
                depth += 1;
                if depth > max {
                    max = depth;
                }
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    max
}

impl VerifyError {
    /// A stable failure-class discriminator for the envelope's
    /// `pipeline_error.kind` (#112). Disambiguates the senses the single
    /// `MDATRON-E0080` code otherwise conflates, letting a machine consumer
    /// branch on the failure class without parsing the message prose. Exhaustive
    /// so a new variant forces a kind here.
    pub fn kind(&self) -> &'static str {
        match self {
            VerifyError::Io { .. } => "io",
            VerifyError::SchemaLoad { .. } => "schema_load",
            VerifyError::PatternLoad { .. } => "pattern_load",
            VerifyError::IndexBuild(_) => "index_build",
            VerifyError::ExprParse { .. } => "expr_parse",
            VerifyError::Eval { .. } => "eval",
            VerifyError::Glob(_) => "glob",
            VerifyError::Config(_) => "config",
            VerifyError::Frontmatter { .. } => "frontmatter",
            VerifyError::BoundExceeded { .. } => "bound_exceeded",
        }
    }

    /// Rewrite any absolute filesystem path this error carries to be project-
    /// root-relative (DEF4 completion, #134, roast B1). `pipeline_error.message`
    /// is built from this error's `Display`, so an absolute path in a `{path}`
    /// variant would leak the host layout into the JSON envelope on a failed
    /// pipeline — the same leak the findings-relativization pass closed for
    /// `findings[].location.file`. Applied once at the `pipeline_error`
    /// construction site against the canonicalized root. A path not under `root`
    /// (a pre-canonicalization error, or one already relative) is left as-is.
    pub fn relativize_paths(self, root: &Path) -> Self {
        fn rel(p: String, root: &Path) -> String {
            match Path::new(&p).strip_prefix(root) {
                Ok(r) if r.as_os_str().is_empty() => ".".into(),
                Ok(r) => r.to_string_lossy().into_owned(),
                Err(_) => p,
            }
        }
        match self {
            VerifyError::Io { path, error } => VerifyError::Io {
                path: rel(path, root),
                error,
            },
            VerifyError::SchemaLoad { path, error } => VerifyError::SchemaLoad {
                path: rel(path, root),
                error,
            },
            VerifyError::PatternLoad { path, error } => VerifyError::PatternLoad {
                path: rel(path, root),
                error,
            },
            VerifyError::Frontmatter { path, error } => VerifyError::Frontmatter {
                path: rel(path, root),
                error,
            },
            other => other,
        }
    }
}

/// A completed verification run: the findings plus which check families were
/// invoked (#90). The envelope's `families` field is built from `families`.
pub struct VerifyReport {
    pub findings: Vec<Finding>,
    pub families: crate::output::Families,
    /// The number of files this run actually validated (ran the per-file schema
    /// and rule checks on) — the true audit signal, not a count of files that
    /// happened to produce findings (#105). A clean run over N files reports N;
    /// an empty jurisdiction reports 0.
    pub files_checked: u32,
}

/// A completed incremental run (#102): the report plus the observable
/// visited-file scope. `visited` is `None` when the change forced a whole-tree
/// run (a `.mdatron/` input change), meaning every walked file was verified.
pub struct IncrementalReport {
    pub report: VerifyReport,
    pub visited: Option<BTreeSet<PathBuf>>,
}

/// The internal pipeline result: findings, per-family activity, the visited
/// scope (`None` for a whole-tree run), and the count of files validated (#105).
type RunResult = Result<
    (
        Vec<Finding>,
        crate::output::Families,
        Option<BTreeSet<PathBuf>>,
        u32,
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
    let (findings, families, _visited, files_checked) = run(config, None, None)?;
    Ok(VerifyReport {
        findings,
        families,
        files_checked,
    })
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
    let (findings, families, visited, files_checked) = run(config, Some(changed), None)?;
    Ok(IncrementalReport {
        report: VerifyReport {
            findings,
            families,
            files_checked,
        },
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
    // #110 (vsdd item 2): past the both-missing guard, a still-absent schemas dir
    // means Layer 1 cannot run at all — a false-clean the run would otherwise
    // report silently. Captured here (the load below fails safe to empty) and
    // surfaced as W0047 on a whole-tree pass. A present-but-empty dir is a
    // deliberate opt-out, not drift, so only true absence is flagged.
    let schemas_dir_missing = !config.schemas_dir.is_dir();
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

    // Concurrent VERIFY-invocation bound (#92 D / DESIGN § hook-time cost;
    // the standalone `pin` command is outside the count — docs/limits.md
    // scopes this honestly): hold one of the declared per-user, per-root
    // slots for the run's duration, or report the bound — never pile
    // unbounded concurrent hook-driven verify runs onto one machine. The
    // guard's lock dies with the process (flock / exclusive share-mode), so a
    // crashed run cannot wedge the count. Slot-infrastructure IO failure is a
    // loud error, not a silent unbounded run (no-silent-degradation
    // doctrine).
    let _invocation_slot = match crate::limits::acquire_invocation_slot(
        &project_root,
        crate::limits::SHIPPED.concurrent_invocations,
    ) {
        Ok(crate::limits::SlotOutcome::Acquired(slot)) => slot,
        Ok(crate::limits::SlotOutcome::Busy {
            repaired_permissive_dir,
        }) => {
            // A pool that was just repaired from a permissive mode may be held
            // by processes that grabbed its slots THROUGH that window (the
            // repair cannot revoke their locks) — say so rather than
            // misattribute it to N genuine concurrent runs (#103 phase-3 R3-1).
            let detail = if repaired_permissive_dir {
                format!(
                    "the invocation-slot directory for this project root was found \
                     world-accessible and repaired to 0700; its {} slots may be held \
                     by processes that opened them through that window — point TMPDIR \
                     at a private directory if this run is not genuinely concurrent",
                    crate::limits::SHIPPED.concurrent_invocations
                )
            } else {
                format!(
                    "all {} concurrent verify invocation slots for this project root are busy",
                    crate::limits::SHIPPED.concurrent_invocations
                )
            };
            return Err(VerifyError::BoundExceeded {
                bound: "concurrent-invocation-count".into(),
                detail,
            });
        }
        Err(e) => {
            return Err(VerifyError::Io {
                path: "invocation slot directory".into(),
                error: e.to_string(),
            })
        }
    };

    // Union of all patterns' `keys:` declarations. The registry itself is built
    // AFTER the governed-body capture, from the same snapshot (#103): index
    // sources and governed bodies are one read-once input set, so a file that
    // is both is seen as one byte-state by every check.
    let mut all_keys = Vec::new();
    for pf in &patterns {
        all_keys.extend(pf.pattern.keys.clone());
    }

    // Opt-in frontmatter requirement (#80 D2): compile the globs once; a
    // malformed OR tree-escaping pattern is a loud config error, not a silent
    // no-op (the confinement gap, SA F4 — see `confine_and_compile_globs`).
    let require = confine_and_compile_globs(&config.require_frontmatter, "require_frontmatter")?;

    // Vocabulary scope (#97): the naming register applies only to these globs
    // when supplied; empty falls back to every walked file (prior behavior).
    // Kept separate from file_globs so a historical archive stays walked +
    // routed without its retired handle scheme tripping the register.
    let vocab_globs = confine_and_compile_globs(&config.vocabulary_globs, "vocabulary_globs")?;

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
    let catalogs = match crate::codecat::load(&project_root) {
        Ok(c) => c,
        Err(e) => return Err(VerifyError::Config(e.to_string())),
    };
    // Capture data-presence per family BEFORE the Options are consumed (#90);
    // the tri-state families object (#107) is built after the walk, since
    // vocabulary's `inert` state needs the scope-hit count.
    use crate::output::{Families, FamilyActivity};
    let schemas_supplied = !schemas.is_empty();
    let route_supplied = routes.is_some();
    let pin_supplied = pin_data.is_some();
    let vocab_supplied = vocab.is_some();
    let code_catalog_supplied = catalogs.is_some();
    let section_supplied = routes
        .as_ref()
        .map(|r| r.routes.iter().any(|x| !x.section_rules.is_empty()))
        .unwrap_or(false);
    let citation_supplied = routes
        .as_ref()
        .map(|r| r.routes.iter().any(|x| x.citations))
        .unwrap_or(false);
    let link_supplied = routes
        .as_ref()
        .map(|r| r.routes.iter().any(|x| x.links))
        .unwrap_or(false);
    let marker_supplied = routes
        .as_ref()
        .map(|r| r.routes.iter().any(|x| !x.marker_rules.is_empty()))
        .unwrap_or(false);

    let mut findings: Vec<Finding> = Vec::new();

    // Rule field-reference validation (#156): a project-level pass, run once
    // before any document is walked (Cedar's validate-before-deploy posture).
    // Each rule's `$self.<field>` references are checked against the frontmatter
    // schema its context binds; a path naming an undeclared property under a
    // closed object hard-gates as E0021. Conservative by construction — see
    // `validate_rule_field_refs`.
    validate_rule_field_refs(&config.patterns_dir, &patterns, &schemas, &mut findings);

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
    let catalogs = match catalogs {
        Some(mut loaded) => {
            findings.append(&mut loaded.findings);
            Some(loaded.catalogs)
        }
        None => None,
    };
    // Collect the governed files once (absolute + root-relative paths). Two
    // `file_globs` can overlap on the same file; it is deduped by root-relative
    // path (#109) so the walk verifies each file exactly once — a doubled walk
    // would double every per-file finding and over-count `files_checked` (#105),
    // corrupting the checked-N-vs-checked-nothing audit signal.
    //
    // Each `file_globs` entry that matches nothing is a shrunk jurisdiction
    // (#108, vsdd item 10): recorded here so a whole-tree run can flag it (W0046)
    // rather than walking fewer files than the adopter declared, silently. The
    // dead-glob test is per disk match, not per newly-inserted file — a glob that
    // only re-matches an already-collected file is redundant, not dead.
    let mut governed: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut dead_globs: Vec<String> = Vec::new();
    for glob_pattern in &config.file_globs {
        let absolute = project_root.join(glob_pattern);
        let paths = glob::glob(&absolute.to_string_lossy())
            .map_err(|e| VerifyError::Glob(format!("'{glob_pattern}': {e}")))?;
        let mut matched = false;
        for entry in paths {
            // A per-entry read error's `Display` embeds the ABSOLUTE path it
            // failed on (roast A2, #140) — relativize it against the root so the
            // Glob pipeline_error.message does not leak the host layout.
            let path = entry.map_err(|e| {
                let rel = e.path().strip_prefix(&project_root).unwrap_or(e.path());
                VerifyError::Glob(format!(
                    "'{glob_pattern}': cannot read '{}': {}",
                    rel.display(),
                    e.error()
                ))
            })?;
            let rel = path
                .strip_prefix(&project_root)
                .unwrap_or(&path)
                .to_path_buf();
            matched = true;
            if seen.insert(rel.clone()) {
                governed.push((path, rel));
            }
        }
        if !matched {
            dead_globs.push(glob_pattern.clone());
        }
    }

    // The ONE immutable snapshot (#103, security): every input the checks
    // consult — governed bodies here, then index sources, then discovered
    // cross-file targets — is captured through confined no-follow handles into
    // this structure before the capture-complete seam; after the seam the run
    // touches no filesystem. Capture is idempotent per path (read-once across
    // roles) and bounded per-file + in aggregate for every input class (#124).
    let mut snapshot = crate::snapshot::Snapshot::new(MAX_FILE_BYTES, MAX_AGGREGATE_BYTES);
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
                    // DEF4 completion (#134, roast A1): anchor at the config that
                    // declared the escaping glob, NOT at the escaping path itself —
                    // that path is OUTSIDE the root (that is the violation), so it
                    // cannot be relativized and would leak an absolute host path
                    // into the envelope. The config anchor is under the root, so the
                    // run-tail relativization pass makes it `.mdatron/config.yaml`.
                    // The offending path rides as prefix-marked adopter content.
                    location: Location {
                        file: project_root.join(".mdatron").join("config.yaml"),
                        line: 1,
                        column: 0,
                    },
                    explain_ref: Some(code.into()),
                    quoted: vec![QuotedRegion {
                        label: "escaping-path".into(),
                        content: rel.to_string_lossy().into_owned(),
                    }],
                });
                continue;
            }
        };
        match snapshot.capture(&project_root, &confined)? {
            crate::snapshot::Captured::Content(content) => {
                // The nesting bound applies to bodies (their frontmatter is
                // YAML-parsed per file); a non-UTF8 body is reported at the
                // walk (E0003), never a whole-run abort (#103 security C).
                if let Some(text) = content.text() {
                    let nesting = max_flow_nesting(text);
                    if nesting > MAX_STRUCTURAL_NESTING {
                        return Err(VerifyError::BoundExceeded {
                            bound: "structural-nesting-depth".into(),
                            detail: format!(
                                "'{}' nests flow collections {nesting} deep (limit {MAX_STRUCTURAL_NESTING})",
                                crate::diagnostic::escape_path_text(&rel.to_string_lossy())
                            ),
                        });
                    }
                }
            }
            crate::snapshot::Captured::SymlinkRefused { component } => {
                let component = component.to_string_lossy().into_owned();
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
                        content: component,
                    }],
                });
            }
            // A governed body over the per-file cap is the config-scoped
            // posture: a loud whole-run bound error (the jurisdiction declared
            // this file; raising limits is the bounds catalog's business).
            crate::snapshot::Captured::TooLarge { limit, dimension } => {
                return Err(crate::snapshot::Snapshot::too_large_error(
                    rel, *limit, *dimension,
                ));
            }
            // Unreadable OR open-refused: reported per-file at the walk
            // (E0003). Open refusal (permission denied, raced deletion) is
            // squarely "unreadable" — aborting the whole run here was the
            // denial-of-verification lever (#103 phase-3 S-2): one chmod'd
            // file must not blind every other check.
            crate::snapshot::Captured::OpenedUnreadable { .. }
            | crate::snapshot::Captured::OpenIo { .. } => {}
        }
    }

    // Index sources (#103): resolve each declaration's sources (a bounded,
    // no-follow, pre-seam enumeration), capture their content into the same
    // snapshot, and build the registry from those captured bytes — the index
    // sees exactly the bytes every other check sees.
    let mut decl_sources: Vec<(crate::dsl::KeyDecl, Vec<crate::confine::ConfinedPath>)> =
        Vec::new();
    for decl in all_keys {
        let sources = crate::dsl::resolve_source(&project_root, decl.source.trim())
            .map_err(VerifyError::from)?;
        for rel in &sources {
            // Config-scoped posture: an oversized index source is the
            // declared-bounds abort, same as a governed body.
            if let crate::snapshot::Captured::TooLarge { limit, dimension } =
                snapshot.capture(&project_root, rel)?
            {
                return Err(crate::snapshot::Snapshot::too_large_error(
                    rel.as_path(),
                    *limit,
                    *dimension,
                ));
            }
        }
        decl_sources.push((decl, sources));
    }
    let (registry, degraded_sources) = IndexRegistry::build_from_parts(&decl_sources, &snapshot)?;
    // #162: a missing/unreadable/non-UTF8 index source degraded to an empty
    // contribution rather than aborting the run — render each as a loud
    // availability warning (W0049) at the patterns-family config home, so the
    // inert source is observable. Rules referencing the now-empty key surface
    // their own findings; this is the family's availability signal, not the
    // conformance gate. Whole-tree only — an incremental scope filter would
    // otherwise drop a patterns-located finding (mirrors W0043/W0046/W0047).
    for d in &degraded_sources {
        findings.push(index_source_degraded_finding(&project_root, d));
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
                schema_class: snapshot.text(rel).and_then(read_schema_class),
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

    // Cross-file targets (#103): discover and capture, BEFORE the seam, every
    // path the post-seam checks will consult — each captured AS discovered, so
    // the transient working set is one path, not an accumulated list (phase-3
    // S-4). Pin targets are declared in pins.yaml; cite/link targets are
    // extracted from in-scope body text with the same extraction the checks
    // run (a pure function over snapshot bytes, so the check-time set cannot
    // diverge); marker targets come from route config. A lexically-escaping
    // path never reaches the filesystem — the check reports it on path text
    // alone, so no capture is required. Bound posture splits by who controls
    // the path (phase-3 A-1): config-scoped targets (pins, marker target_doc)
    // escalate TooLarge to the whole-run bound error like bodies and index
    // sources; prose-scoped targets (citations, links) record the state and
    // the checks treat it as present-but-unverifiable — a prose line must not
    // be able to abort the run.
    for pin in &pins {
        let relevant = scope
            .as_ref()
            .is_none_or(|s| s.contains(Path::new(&pin.file)));
        if !relevant {
            continue;
        }
        if let Ok(confined) = crate::confine::confine_lexically(Path::new(&pin.file)) {
            if let crate::snapshot::Captured::TooLarge { limit, dimension } =
                snapshot.capture(&project_root, &confined)?
            {
                return Err(crate::snapshot::Snapshot::too_large_error(
                    confined.as_path(),
                    *limit,
                    *dimension,
                ));
            }
        }
    }
    if let Some(routes) = &routes {
        // ALL config-scoped targets capture BEFORE any prose-scoped one
        // (phase-3 R3-1): prose captures consume the shared aggregate budget
        // with the degrade posture, so a config-scoped capture ordered after
        // them could inherit a prose-caused aggregate breach and escalate it
        // into a whole-run abort — one authored citation line flipping a
        // surviving run into a denial of verification. Config-first ordering
        // means a config-scoped aggregate abort reflects config-declared
        // inputs alone, and prose consumption can only ever degrade prose
        // checks.
        for (_path, rel) in &governed {
            if let Some(scope) = &scope {
                if !scope.contains(rel) {
                    continue;
                }
            }
            // Same gating as the prose pass below (phase-3 I-1): an
            // unreadable or frontmatter-parse-failed file runs NO cross-file
            // checks, marker included, so its rules' targets are not consulted.
            let has_cross_file_checks = snapshot.text(rel).and_then(body_offset_of).is_some();
            if !has_cross_file_checks {
                continue;
            }
            for rule in crate::route::marker_rules_for(routes, rel) {
                if let Ok(confined) = crate::confine::confine_lexically(Path::new(&rule.target_doc))
                {
                    if let crate::snapshot::Captured::TooLarge { limit, dimension } =
                        snapshot.capture(&project_root, &confined)?
                    {
                        return Err(crate::snapshot::Snapshot::too_large_error(
                            confined.as_path(),
                            *limit,
                            *dimension,
                        ));
                    }
                }
            }
        }
        for (_path, rel) in &governed {
            if let Some(scope) = &scope {
                if !scope.contains(rel) {
                    continue;
                }
            }
            // Extract this ONE file's prose targets (the borrow of the body
            // text ends before capture needs the snapshot mutably); the
            // transient list is bounded by a single file's content, never the
            // corpus (phase-3 S-4).
            let prose_targets: Vec<crate::confine::ConfinedPath> = {
                let Some(content) = snapshot.text(rel) else {
                    continue;
                };
                // Discovery mirrors verify_file's gating exactly (phase-3
                // I-1): a file whose frontmatter fails to parse gets E0001 and
                // NO cite/link/marker checks, so none of its targets are
                // consulted — capturing them could only widen the abort
                // surface.
                let Some(body_offset) = body_offset_of(content) else {
                    continue;
                };
                let mut ts = Vec::new();
                if crate::route::citations_enabled(routes, rel) {
                    ts.extend(crate::cite::cited_targets(content, body_offset));
                }
                if crate::route::links_enabled(routes, rel) {
                    ts.extend(crate::link::link_targets(
                        rel,
                        content,
                        body_offset,
                        crate::route::link_root_enabled(routes, rel),
                    ));
                }
                ts
            };
            for target in &prose_targets {
                // Degrading capture (phase-3 R2S-1): a prose-named target that
                // would breach the AGGREGATE budget records the unverifiable
                // state instead of erroring — otherwise one authored citation
                // line could tip a large corpus over the cap and deny the
                // whole run.
                snapshot.capture_degrading(&project_root, target)?;
            }
        }
    }

    // Capture-complete seam (#103): the snapshot is sealed — every input this
    // run will consult is captured, and a later capture is an engine error,
    // not a silent filesystem reopen. A test injects a mutation here to prove
    // the checks report against snapshot bytes, not a live filesystem window.
    snapshot.seal();
    if let Some(cb) = on_capture_complete {
        cb();
    }

    // #98: track whether a scoped register matched any walked file. Whole-tree
    // only — W0043 is `.mdatron/`-located (never in an incremental scope) and
    // the incremental walk sees only part of the tree.
    let vocab_scoped = vocab.is_some() && !vocab_globs.is_empty();
    let mut vocab_scoped_hits = 0usize;
    let mut files_checked: u32 = 0;
    // #110: does any walked file declare a schema_class that routed to neither a
    // schema nor a rule context? With the schemas dir missing entirely, that is
    // an unserved Layer-1 request (W0047) even where W0045's has-infra gate stays
    // its hand.
    let mut any_unrouted_schema_class = false;
    for (path, rel) in &governed {
        // Incremental: skip files outside the scope.
        if let Some(scope) = &scope {
            if !scope.contains(rel) {
                continue;
            }
        }
        let mut cite_enabled = false;
        let mut link_enabled = false;
        let mut link_root = false;
        let mut marker_rules: Vec<&crate::route::MarkerRule> = Vec::new();
        let mut section_rules: Vec<&crate::section::Rule> = Vec::new();
        if let Some(routes) = &routes {
            crate::route::check_file(routes, rel, path, &mut findings);
            cite_enabled = crate::route::citations_enabled(routes, rel);
            link_enabled = crate::route::links_enabled(routes, rel);
            link_root = crate::route::link_root_enabled(routes, rel);
            marker_rules = crate::route::marker_rules_for(routes, rel);
            section_rules = crate::route::section_rules_for(routes, rel);
        }
        // Vocabulary scope (#97): empty globs = every walked file.
        let vocab_enabled =
            vocab_globs.is_empty() || vocab_globs.iter().any(|p| p.matches_path(rel));
        if vocab_scoped && vocab_enabled {
            vocab_scoped_hits += 1;
        }
        // Read from the immutable snapshot, never the filesystem (#103). A
        // symlinked file was refused at capture (its E0012 is recorded); a
        // captured-but-unreadable body (non-UTF8, or a read failure past the
        // open) is a PER-FILE finding (E0003), never a whole-run abort — one
        // hostile file must not deny verification of the rest of the tree
        // (#103 security C). Neither counts as validated in files_checked.
        let content = match snapshot.get(rel) {
            Some(crate::snapshot::Captured::Content(c)) => match c.text() {
                Some(text) => text,
                None => {
                    findings.push(unreadable_body_finding(
                        path,
                        "the file's bytes are not valid UTF-8",
                    ));
                    continue;
                }
            },
            Some(crate::snapshot::Captured::OpenedUnreadable { error })
            | Some(crate::snapshot::Captured::OpenIo { error }) => {
                findings.push(unreadable_body_finding(path, error));
                continue;
            }
            // Defensive: a too-large body aborts at capture; if one ever
            // reaches the walk, report it rather than skip it silently.
            Some(crate::snapshot::Captured::TooLarge { .. }) => {
                findings.push(unreadable_body_finding(
                    path,
                    "the file exceeds the per-file input size limit",
                ));
                continue;
            }
            // Symlinked: refused at capture, its E0012 already recorded.
            Some(crate::snapshot::Captured::SymlinkRefused { .. }) | None => continue,
        };
        any_unrouted_schema_class |= verify_file(
            path,
            content,
            &snapshot,
            &project_root,
            &require,
            cite_enabled,
            link_enabled,
            link_root,
            &marker_rules,
            catalogs.as_deref().unwrap_or(&[]),
            &section_rules,
            vocab.as_ref().filter(|_| vocab_enabled),
            &schemas,
            &patterns,
            &registry,
            schemas_dir_missing,
            &mut findings,
        )?;
        // A validated file (#105): the audit signal counts files the per-file
        // checks actually ran on, not files that produced findings.
        files_checked += 1;
    }

    // #95: registry-level vocabulary findings (a registered-and-draft term
    // resolves to draft with a W0044 warning) — file-independent, once per run.
    if let Some(v) = vocab.as_ref() {
        let vocab_path = project_root.join(".mdatron").join(crate::vocab::VOCAB_NAME);
        crate::vocab::registry_findings(v, &vocab_path, &mut findings);
    }

    // #108 (vsdd item 10): a `file_globs` entry that matches no file shrinks the
    // checked corpus silently — verify walks fewer files than declared and still
    // exits 0, indistinguishable from a clean jurisdiction. W0046 makes each dead
    // glob loud, naming the pattern so a typo or a stale path is caught. Whole-
    // tree only: an incremental walk sees part of the tree, so "matched nothing"
    // there is expected, not a jurisdiction defect (mirrors W0043's gate; the
    // scope filter above would drop a config-located finding anyway).
    if scope.is_none() {
        let config_path = project_root
            .join(".mdatron")
            .join(crate::config::CONFIG_NAME);
        for pattern in &dead_globs {
            findings.push(Finding {
                code: "MDATRON-W0046".into(),
                severity: Severity::Warning,
                summary: "jurisdiction-glob-matches-nothing".into(),
                message: "a `file_globs` entry in .mdatron/config.yaml matches no \
                          file, so it contributes nothing to the checked corpus — a \
                          typo or a stale path narrows the jurisdiction silently, and \
                          the run exits as if that slice were clean"
                    .into(),
                help: Some(
                    "correct the glob to cover the files it should govern, or remove \
                     it if the jurisdiction genuinely no longer includes them"
                        .into(),
                ),
                location: Location {
                    file: config_path.clone(),
                    line: 1,
                    column: 0,
                },
                explain_ref: Some("MDATRON-W0046".into()),
                quoted: vec![QuotedRegion {
                    label: "glob".into(),
                    content: pattern.clone(),
                }],
            });
        }

        // #110 (vsdd item 2): the schemas dir is absent AND a walked file declares
        // a schema_class that routed to neither a schema nor a rule context — an
        // unserved Layer-1 request. Without the schemas dir the family validated
        // nothing, yet the run would exit clean; W0047 makes that false-clean
        // loud. Gated on an actual unrouted class so a Schematron-only project
        // (whose schema_class selects a rule context) is not nagged for the
        // schemas dir it never needed.
        if schemas_dir_missing && any_unrouted_schema_class {
            findings.push(Finding {
                code: "MDATRON-W0047".into(),
                severity: Severity::Warning,
                summary: "schema-dir-missing".into(),
                message: "the `.mdatron/schemas/` directory is absent, so the schema \
                          family (Layer 1) validated nothing — a file declares a \
                          `schema_class` that no schema and no rule context serves, so \
                          it passes unchecked and the run exits as if its frontmatter \
                          were conformant"
                    .into(),
                help: Some(
                    "run `mdatron init` to restore the skeleton and add \
                     `.mdatron/schemas/<class>.json`, or add a pattern rule with \
                     `context: <class>`, or correct the schema_class"
                        .into(),
                ),
                location: Location {
                    file: project_root.join(".mdatron").join("schemas"),
                    line: 1,
                    column: 0,
                },
                explain_ref: Some("MDATRON-W0047".into()),
                quoted: Vec::new(),
            });
        }
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
            crate::pin::check(
                &project_root,
                std::slice::from_ref(pin),
                &snapshot,
                &mut findings,
            );
        }
    }

    // Tri-state families (#107): each carries the reason that makes the audit
    // signal falsifiable. Vocabulary distinguishes not-configured (no registry)
    // from inert (registry present but scoped to nothing) — the ambiguity
    // vsdd-cli's review flagged (vocabulary_globs alone does not activate it).
    let families = Families {
        schema: if schemas_supplied {
            FamilyActivity::active("schemas supplied; Layer 1 ran")
        } else {
            FamilyActivity::inactive("no schemas in .mdatron/schemas/")
        },
        route: if route_supplied {
            FamilyActivity::active(".mdatron/routes.yaml supplied")
        } else {
            FamilyActivity::inactive("no .mdatron/routes.yaml")
        },
        pin: if pin_supplied {
            FamilyActivity::active(".mdatron/pins.yaml supplied")
        } else {
            FamilyActivity::inactive("no .mdatron/pins.yaml")
        },
        vocabulary: if !vocab_supplied {
            FamilyActivity::inactive(
                "no .mdatron/vocabulary.yaml (vocabulary_globs alone does not activate the family)",
            )
        } else if vocab_scoped && vocab_scoped_hits == 0 {
            FamilyActivity::inert(
                "vocabulary.yaml present but vocabulary_globs matched no walked file",
            )
        } else {
            FamilyActivity::active("vocabulary.yaml supplied; scanned the corpus")
        },
        citation: if citation_supplied {
            FamilyActivity::active("a route opts in with citations: true")
        } else {
            FamilyActivity::inactive("no route opts in with citations: true")
        },
        link: if link_supplied {
            FamilyActivity::active("a route opts in with links: true")
        } else {
            FamilyActivity::inactive("no route opts in with links: true")
        },
        marker: if marker_supplied {
            FamilyActivity::active("a route supplies marker_rules")
        } else {
            FamilyActivity::inactive("no route supplies marker_rules")
        },
        code_catalog: if code_catalog_supplied {
            FamilyActivity::active(".mdatron/code-catalogs.yaml supplied")
        } else {
            FamilyActivity::inactive("no .mdatron/code-catalogs.yaml")
        },
        section: if section_supplied {
            FamilyActivity::active("a route supplies section_rules")
        } else {
            FamilyActivity::inactive("no route supplies section_rules")
        },
    };

    // DEF4 (#131 SO ruling): every diagnostic path is project-root-relative, so
    // the JSON envelope neither leaks the host filesystem layout nor differs
    // across machines (a machine consumer can diff two runs' findings). Per-file
    // findings already carry a relativized path; this single pass also relativizes
    // the project-level findings (schemas dir, config, vocab, routes/pins) whose
    // sources build absolute paths via `project_root.join(...)`. It is a no-op on
    // an already-relative path (strip_prefix fails → kept), and runs before the
    // sort so ordering is by relative path too (equally reproducible).
    for f in &mut findings {
        if let Ok(rel) = f.location.file.strip_prefix(&project_root) {
            f.location.file = rel.to_path_buf();
        }
    }
    findings.sort_by(|a, b| {
        a.location
            .file
            .cmp(&b.location.file)
            .then_with(|| a.code.cmp(&b.code))
    });
    Ok((findings, families, scope, files_checked))
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

/// Where the prose body begins — the same offset the per-file checks compute
/// from the frontmatter parse — or `None` for a file whose frontmatter fails
/// to parse. Target discovery (#103) uses this so its extraction mirrors
/// `verify_file` exactly: a parse-failed file gets E0001 and none of the
/// cross-file checks, so discovery must not extract targets from it either
/// (phase-3 I-1 — capturing them could only widen the abort surface). No
/// frontmatter at all means the body is the whole file.
fn body_offset_of(content: &str) -> Option<usize> {
    match crate::frontmatter::parse(content) {
        Ok(Some((_fm, body))) => Some(content.len() - body.len()),
        Ok(None) => Some(0),
        Err(_) => None,
    }
}

/// A governed body that was captured but cannot be verified as text — non-UTF8
/// bytes, or a read failure after the confined open (#103 security C). A
/// localized per-file finding: the run continues over the rest of the tree.
fn unreadable_body_finding(path: &Path, cause: &str) -> Finding {
    Finding {
        code: "MDATRON-E0003".into(),
        severity: Severity::Error,
        summary: "governed-file-unreadable".into(),
        message: format!(
            "this governed file's content cannot be verified ({cause}); \
             nothing validated it"
        ),
        help: Some(
            "re-encode the file as UTF-8 (or repair its readability); a file \
             that cannot be read cannot be governed"
                .into(),
        ),
        location: Location {
            file: path.to_path_buf(),
            line: 1,
            column: 0,
        },
        explain_ref: Some("MDATRON-E0003".into()),
        quoted: Vec::new(),
    }
}

/// A `keys:` source that could not be read (#162): a loud availability warning
/// located at the patterns-family config home (`.mdatron/patterns/`, mirroring
/// how W0043/W0046/W0047 locate at the declaring config surface). The index is
/// inert for that source, so a rule referencing the now-empty key surfaces its
/// own finding — this warning is the availability signal, not the conformance
/// gate. The source path arrives pre-escaped from the DSL layer.
fn index_source_degraded_finding(project_root: &Path, d: &crate::dsl::DegradedSource) -> Finding {
    Finding {
        code: "MDATRON-W0049".into(),
        severity: Severity::Warning,
        summary: "index-source-unreadable".into(),
        // Message is FULLY engine-authored: the adopter-controlled index name,
        // source path, and OS reason ride ONLY in the quoted regions below,
        // which are escaped at render time. Interpolating any of them inline
        // here would inject control bytes into the TTY/compact agent-facing
        // views (the marking discipline, DESIGN § Agents are the first
        // consumer; #125/SHO5, #162 phase-3 F-1).
        message: "a `keys:` source could not be read, so it contributed \
                  nothing to its index — the index is inert for that source \
                  (the index name, source, and reason are quoted below). A \
                  rule that requires the key will surface its own finding; \
                  this warning makes the missing coverage observable rather \
                  than silent."
            .into(),
        help: Some(
            "restore or re-encode the source as a readable UTF-8 file, or \
             remove the `keys:` declaration if the index is no longer needed"
                .into(),
        ),
        location: Location {
            file: project_root.join(".mdatron").join("patterns"),
            line: 1,
            column: 0,
        },
        explain_ref: Some("MDATRON-W0049".into()),
        quoted: vec![
            QuotedRegion {
                label: "index".into(),
                content: d.key_name.clone(),
            },
            QuotedRegion {
                label: "source".into(),
                content: d.source_display.clone(),
            },
            QuotedRegion {
                label: "reason".into(),
                content: d.reason.clone(),
            },
        ],
    }
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

/// Confine + compile a list of adopter **scope globs** (`require_frontmatter`,
/// `vocabulary_globs`) against the governed tree. A glob whose text escapes the
/// tree — an absolute path, or one climbing above the root with `..` — is a loud
/// config error, closing the confinement gap the Solution Architect review found
/// (SA F4): such a glob compiles fine but, matched root-relative, silently
/// matches nothing — a no-op where the engine promises loud refusal. `field`
/// names the config key for the message. Scope globs are pure `matches_path`
/// predicates over the already-confined walk, so this only makes an escaping
/// glob *legible*; it never widens the file set (Security's scope-∩-confinement).
fn confine_and_compile_globs(
    globs: &[String],
    field: &str,
) -> Result<Vec<glob::Pattern>, VerifyError> {
    globs
        .iter()
        .map(|g| {
            crate::confine::confine_lexically(Path::new(g)).map_err(|v| {
                let why = match v {
                    crate::confine::LexicalViolation::Absolute => "is an absolute path",
                    crate::confine::LexicalViolation::ParentSegment => {
                        "climbs above the project root with `..`"
                    }
                };
                VerifyError::Config(format!(
                    "{field} glob '{g}' {why}; a scope glob must stay within the governed tree"
                ))
            })?;
            glob::Pattern::new(g)
                .map_err(|e| VerifyError::Config(format!("{field} glob '{g}': {e}")))
        })
        .collect()
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

// ── Rule field-reference validation (#156) ──────────────────────────────────────

/// Validate every rule's `$self.<field>` references against the frontmatter
/// schema its context binds (#156, adopting Cedar's validate-before-deploy
/// posture; [[cedar-opa-dsl-audit]]). A path that names an UNDECLARED property
/// under a CLOSED object (`additionalProperties: false`) is a typo the run would
/// otherwise absorb silently — the field reads as absent, so the assertion
/// mis-fires or passes vacuously against every governed document. Surfaced as
/// `MDATRON-E0021` (ERROR), hard-gating the run before any document is checked.
///
/// **Conservative by construction** — this is a hard gate, so a false positive
/// breaks an adopter build. Three guards keep it sound:
/// - only rules whose context resolves to a *loaded* schema_class are examined
///   (a path-glob context, or an unknown class, is left unchecked);
/// - only `$self`-rooted field chains are walked — bindings, `$file`,
///   `$project`, and quantifier variables never enter;
/// - only `FieldPathStatus::UndeclaredClosed` is flagged — every undecidable
///   shape (open object, array, `$ref`, combinator, missing `properties`) passes.
fn validate_rule_field_refs(
    patterns_dir: &Path,
    patterns: &[PatternFile],
    schemas: &BTreeMap<String, Schema>,
    findings: &mut Vec<Finding>,
) {
    for pf in patterns {
        for rule in &pf.pattern.rules {
            let Some(schema_class) = context_schema_class(&rule.context) else {
                continue;
            };
            let Some(schema) = schemas.get(schema_class) else {
                continue;
            };
            // Every expression the rule evaluates: each let-binding value in
            // order, then the assertion itself.
            let sources = rule
                .let_bindings
                .iter()
                .map(|(_, v)| v.as_str())
                .chain(std::iter::once(rule.assert.as_str()));
            let mut paths: Vec<Vec<String>> = Vec::new();
            for src in sources {
                // A parse failure here is not a field typo; the same parse runs
                // at eval time and surfaces as `ExprParse` against a matching
                // document. Skip it rather than double-reporting.
                if let Ok(expr) = parse_expression(src) {
                    collect_self_paths(&expr, &mut paths);
                }
            }
            paths.sort();
            paths.dedup();
            for path in paths {
                if schema.field_path_status(&path) == FieldPathStatus::UndeclaredClosed {
                    findings.push(field_ref_finding(
                        patterns_dir,
                        &pf.pattern.id,
                        rule,
                        schema_class,
                        &path,
                    ));
                }
            }
        }
    }
}

/// The schema_class a rule's context statically binds `$self` to: a bare
/// non-glob string, or the `schema_class` of a combined selector. A path-glob
/// context binds `$self` to whatever schema the matched files route to (not
/// knowable at load), so it yields `None` and the rule is left unchecked.
fn context_schema_class(ctx: &ContextSelector) -> Option<&str> {
    match ctx {
        ContextSelector::Bare(s) => {
            if s.contains('*') || s.contains('?') || s.contains('/') {
                None
            } else {
                Some(s.as_str())
            }
        }
        ContextSelector::Combined { schema_class, .. } => schema_class.as_deref(),
    }
}

/// The `$self`-rooted field path of `e`, if `e` is a `Field` chain bottoming out
/// at `$self` (`$self.a.b` → `["a", "b"]`; bare `$self` → `[]`). `None` for
/// anything else — a chain rooted at a binding, `$file`, `$project`, or a
/// non-field expression.
fn self_path(e: &Expr) -> Option<Vec<String>> {
    match e {
        Expr::Var(VarRef::SelfVar) => Some(Vec::new()),
        Expr::Field(inner, name) => {
            let mut p = self_path(inner)?;
            p.push(name.clone());
            Some(p)
        }
        _ => None,
    }
}

/// Collect every `$self`-rooted field path reachable in `e`, recursing through
/// all operand-bearing variants. A `$self` chain is captured whole (never
/// descended past); everything else recurses so paths nested in operators,
/// calls, and quantifier collections/predicates are found. A quantifier BINDING
/// (`m` in `every(m in $self.xs, …)`) is a `VarRef::Binding`, so `$m.k` yields
/// no self-path — only the collection's `$self.xs` is captured.
fn collect_self_paths(e: &Expr, out: &mut Vec<Vec<String>>) {
    if let Some(p) = self_path(e) {
        if !p.is_empty() {
            out.push(p);
        }
        return;
    }
    match e {
        Expr::Field(inner, _) => collect_self_paths(inner, out),
        Expr::Eq(a, b)
        | Expr::Ne(a, b)
        | Expr::And(a, b)
        | Expr::Or(a, b)
        | Expr::In(a, b)
        | Expr::NotIn(a, b) => {
            collect_self_paths(a, out);
            collect_self_paths(b, out);
        }
        Expr::Not(a) => collect_self_paths(a, out),
        Expr::Call(_, args) => {
            for a in args {
                collect_self_paths(a, out);
            }
        }
        Expr::Every(_, coll, pred) | Expr::Some_(_, coll, pred) | Expr::Filter(_, coll, pred) => {
            collect_self_paths(coll, out);
            collect_self_paths(pred, out);
        }
        Expr::Lit(_) | Expr::Var(_) => {}
    }
}

/// Build the `MDATRON-E0021` finding for an undeclared `$self` field reference.
/// Anchored at the patterns directory (rules carry no per-file source span); the
/// pattern id, rule id, and offending `$self.<path>` name the exact site.
fn field_ref_finding(
    patterns_dir: &Path,
    pattern_id: &str,
    rule: &Rule,
    schema_class: &str,
    path: &[String],
) -> Finding {
    let dotted = format!("$self.{}", path.join("."));
    Finding {
        code: "MDATRON-E0021".into(),
        severity: Severity::Error,
        summary: "undeclared-field-reference".into(),
        message: format!(
            "rule references `{dotted}`, but `{last}` is not a declared property \
             of the closed schema `{schema_class}` — the reference would read as \
             absent at evaluation. Correct the field name or declare it in the schema",
            last = path.last().map(String::as_str).unwrap_or_default(),
        ),
        help: None,
        location: Location {
            file: patterns_dir.to_path_buf(),
            line: 0,
            column: 0,
        },
        explain_ref: Some("MDATRON-E0021".to_string()),
        quoted: vec![
            QuotedRegion {
                label: "pattern".into(),
                content: pattern_id.into(),
            },
            QuotedRegion {
                label: "rule".into(),
                content: rule.id.clone(),
            },
            QuotedRegion {
                label: "reference".into(),
                content: dotted,
            },
        ],
    }
}

// ── Per-file processing ────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn verify_file(
    path: &Path,
    content: &str,
    snapshot: &crate::snapshot::Snapshot,
    project_root: &Path,
    require_frontmatter: &[glob::Pattern],
    cite_enabled: bool,
    link_enabled: bool,
    link_root: bool,
    marker_rules: &[&crate::route::MarkerRule],
    code_catalogs: &[crate::codecat::CodeCatalog],
    section_rules: &[&crate::section::Rule],
    vocab: Option<&crate::vocab::LoadedVocab>,
    schemas: &BTreeMap<String, Schema>,
    patterns: &[PatternFile],
    registry: &IndexRegistry,
    schemas_dir_missing: bool,
    findings: &mut Vec<Finding>,
) -> Result<bool, VerifyError> {
    // Content comes from the immutable snapshot (#103): every governed file is
    // read once through a confined no-follow handle in `run`, never from the
    // filesystem here — so a mutation after capture cannot change what this
    // check sees, and a symlinked component was already refused at capture. The
    // snapshot already owns the bytes (#124): verify_file borrows them, so no
    // per-file owned copy is made (that doubled peak memory, roast SHO1).

    let fm_opt = match frontmatter::parse(content) {
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
            // Unparseable frontmatter has no readable schema_class to route.
            return Ok(false);
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
                crate::vocab::check_file(v, path, content, 0, None, findings);
            }
            if cite_enabled {
                crate::cite::check_file(snapshot, path, content, 0, findings);
            }
            if link_enabled {
                crate::link::check_file(
                    snapshot,
                    project_root,
                    path,
                    content,
                    0,
                    link_root,
                    findings,
                );
            }
            crate::marker::check_file(snapshot, path, content, 0, marker_rules, findings);
            crate::codecat::check_file(code_catalogs, path, content, 0, findings);
            crate::section::check_file(section_rules, path, content, 0, findings);
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
            // No frontmatter -> no schema_class declared -> nothing to route.
            return Ok(false);
        }
    };

    // Vocabulary prose scan over the body, with frontmatter context for the
    // numeric-claims comparison (#80 D3).
    if let Some(v) = vocab {
        let body_offset = content.len() - body_len;
        crate::vocab::check_file(
            v,
            path,
            content,
            body_offset,
            Some(&frontmatter_value),
            findings,
        );
    }
    if cite_enabled {
        let body_offset = content.len() - body_len;
        crate::cite::check_file(snapshot, path, content, body_offset, findings);
    }
    if link_enabled {
        let body_offset = content.len() - body_len;
        crate::link::check_file(
            snapshot,
            project_root,
            path,
            content,
            body_offset,
            link_root,
            findings,
        );
    }
    {
        let body_offset = content.len() - body_len;
        crate::marker::check_file(snapshot, path, content, body_offset, marker_rules, findings);
        crate::codecat::check_file(code_catalogs, path, content, body_offset, findings);
        crate::section::check_file(section_rules, path, content, body_offset, findings);
    }

    let frontmatter_internal = crate::dsl::index::yaml_to_value(&frontmatter_value);
    let schema_class_opt = frontmatter_internal
        .as_object()
        .and_then(|o| o.get("schema_class"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // ── Layer 1: structural validation ─────────────────────────────────────
    let mut schema_matched = false;
    if let Some(schema_class) = &schema_class_opt {
        if let Some(schema) = schemas.get(schema_class) {
            schema_matched = true;
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
            let locations = crate::frontmatter::resolve_e0050_locations(content, &items);
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
    // DEF4 completion (#134, roast B2): the DSL `$file.path` is project-root-
    // relative, matching the relativized `location.file` — an absolute path here
    // would leak the host layout into any rule message that interpolates it (a
    // `quoted[]` region) and diverge from the finding's own relative location.
    let rel_path = path.strip_prefix(project_root).unwrap_or(path);
    let file_value = Value::Object(BTreeMap::from([(
        "path".to_string(),
        // Forward-slash separators (#136) so a rule interpolating `$file.path`
        // yields the same envelope on Unix and Windows.
        Value::Str(crate::diagnostic::to_forward_slash(rel_path)),
    )]));
    let project_value = Value::Null;

    let rule_ctx = RuleContext {
        self_value: &frontmatter_internal,
        file_value: &file_value,
        project_value: &project_value,
        registry,
        path,
    };
    let mut any_context_matched = false;
    for pf in patterns {
        for rule in &pf.pattern.rules {
            if !context_matches(&rule.context, schema_class_opt.as_deref(), rel_path) {
                continue;
            }
            any_context_matched = true;
            verify_rule(pf, rule, &rule_ctx, findings)?;
        }
    }

    // #106 (audit signal, #104 item 3): a file that DECLARES a schema_class
    // which matches no schema AND no rule context was silently unvalidated —
    // flag it, so a typo'd or unregistered class on an unchecked file is loud,
    // not a false-clean. A class validated by a schema OR any matching rule is
    // routed and stays silent.
    //
    // #111 (design Q, answered by vsdd-cli mdatron#1): the declaration itself is
    // the adopter data — a file asking for a type nothing serves is the same
    // silent-false-clean class whether or not any *other* schema exists. So W0045
    // no longer gates on has-validation-infra; it fires whenever the schemas dir
    // is PRESENT (empty or not) and the declared class is unrouted. Fresh-init-
    // clean is preserved because a file with no schema_class is not unrouted. A
    // MISSING schemas dir is deliberately left to the project-level W0047 (#110)
    // instead (see `run`) so the two never double-report the same file.
    //
    // The raw (dir-independent) signal is returned so `run` can raise W0047 when
    // the schemas dir is missing entirely.
    let unrouted_schema_class =
        schema_class_opt.is_some() && !schema_matched && !any_context_matched;
    if let Some(schema_class) = &schema_class_opt {
        if !schemas_dir_missing && unrouted_schema_class {
            findings.push(Finding {
                code: "MDATRON-W0045".into(),
                severity: Severity::Warning,
                summary: "schema-class-unrouted".into(),
                message: "this file declares a schema_class that matches no \
                          schema and no rule context, so nothing validated it"
                    .into(),
                help: Some(
                    "add a .mdatron/schemas/<class>.json, or a pattern rule with \
                     `context: <class>`, or correct the schema_class"
                        .into(),
                ),
                location: Location {
                    file: path.to_path_buf(),
                    line: 1,
                    column: 0,
                },
                explain_ref: Some("MDATRON-W0045".into()),
                quoted: vec![QuotedRegion {
                    label: "schema_class".into(),
                    content: schema_class.clone(),
                }],
            });
        }
    }
    Ok(unrouted_schema_class)
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

/// Match a glob pattern against a **project-root-relative** path — the form both
/// callers now pass (SA F1, the scoping-consistency pass). Uses `matches_path`,
/// the same matcher and same root-relative basis as the route and vocabulary
/// scopes, so a `context: "docs/**/*.md"` resolves identically everywhere.
///
/// (Previously this matched `to_string_lossy()` — a **string** match — and the
/// checking pass passed the *absolute* path, so a path-glob context was compared
/// against `/…/root/docs/x.md` and silently never fired; the dependency-graph
/// pass meanwhile passed the relative path, so a rule could register a dependency
/// edge yet never actually run.)
fn glob_matches(pattern: &str, path: &Path) -> bool {
    match glob::Pattern::new(pattern) {
        Ok(p) => p.matches_path(path),
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
                // (DESIGN §Output). The message carries a `[see: <label>]`
                // cross-reference (#116, vsdd ruling on item 8 part 1); the value
                // renders in the labeled, prefix-marked quoted block beneath it.
                // The label drops the `$self.` prefix so it reads as a field name,
                // and numbers a collision/repeat so each pointer resolves to one
                // block (the fallback vsdd reserved for shared labels).
                let base_label = expr_str.strip_prefix("$self.").unwrap_or(expr_str);
                let mut label = base_label.to_string();
                let mut n = 2;
                while quoted.iter().any(|q: &QuotedRegion| q.label == label) {
                    label = format!("{base_label} [{n}]");
                    n += 1;
                }
                out.push_str("[see: ");
                out.push_str(&label);
                out.push(']');
                quoted.push(QuotedRegion {
                    label,
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
        // The governed files sit at the root, so the jurisdiction glob must reach
        // them — a `docs/**/*.md` here would be a dead glob (W0046, #108), since
        // the pin family reads pins.yaml directly and never needed the walk.
        proj.write(".mdatron/config.yaml", "file_globs:\n  - \"**/*.md\"\n");
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

    // A section pin's sha256 is computed over the heading-delimited span, so the
    // fixture pins the section's own hash.
    fn section_pinned_project(label: &str, content: &str, section: &str) -> TempProject {
        let proj = TempProject::new(label);
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(".mdatron/config.yaml", "file_globs:\n  - \"**/*.md\"\n");
        proj.write("GOVERNING.md", "# governing doc\n");
        proj.write("governed.md", content);
        let span = crate::markup::section_span(content, section).expect("test section exists");
        let sha = crate::init::sha256_hex(span.as_bytes());
        proj.write(
            ".mdatron/pins.yaml",
            &format!(
                "pins:\n- governing: GOVERNING.md\n  file: governed.md\n  section: {section:?}\n  sha256: \"{sha}\"\n"
            ),
        );
        proj
    }

    // RED GATE (#146, vsdd#20 P2): a section pin tracks only its heading-delimited
    // span — an edit INSIDE the section is stale (E0061), an edit OUTSIDE it is
    // clean (a whole-file pin would trip), and re-pin restores clean.
    #[test]
    fn section_pin_detects_in_section_change_only() {
        let content = "# Governed\n\n## Alpha\n\nalpha content\n\n## Beta\n\nbeta content\n";
        let proj = section_pinned_project("pin-section", content, "## Alpha");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        assert!(
            verify(&cfg)
                .unwrap()
                .iter()
                .all(|f| !f.code.starts_with("MDATRON-E006")),
            "fresh section pin is clean"
        );
        // Edit OUTSIDE the pinned section (Beta): a section pin stays clean.
        proj.write(
            "governed.md",
            "# Governed\n\n## Alpha\n\nalpha content\n\n## Beta\n\nbeta CHANGED\n",
        );
        assert!(
            verify(&cfg)
                .unwrap()
                .iter()
                .all(|f| f.code != "MDATRON-E0061"),
            "an out-of-section edit does not trip a section pin"
        );
        // Edit INSIDE the pinned section (Alpha): stale.
        proj.write(
            "governed.md",
            "# Governed\n\n## Alpha\n\nalpha CHANGED\n\n## Beta\n\nbeta CHANGED\n",
        );
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().any(|f| f.code == "MDATRON-E0061"),
            "an in-section edit trips the section pin; got {findings:?}"
        );
        // Re-pin (the single-command recompute) restores clean over the span.
        crate::pin::update(&proj.0, false).unwrap();
        assert!(
            verify(&cfg)
                .unwrap()
                .iter()
                .all(|f| f.code != "MDATRON-E0061"),
            "re-pin restores clean"
        );
    }

    // RED GATE (#146): a section pin whose named heading is gone (renamed / mistyped)
    // is E0063 — the pinned section cannot be located, loud rather than silent.
    #[test]
    fn section_pin_missing_heading_is_e0063() {
        let content = "# Governed\n\n## Alpha\n\nalpha\n";
        let proj = section_pinned_project("pin-section-missing", content, "## Alpha");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        assert!(verify(&cfg)
            .unwrap()
            .iter()
            .all(|f| f.code != "MDATRON-E0063"));
        // Rename the pinned heading: its section can no longer be located.
        proj.write("governed.md", "# Governed\n\n## Renamed\n\nalpha\n");
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().any(|f| f.code == "MDATRON-E0063"),
            "a renamed/missing pinned heading is E0063; got {findings:?}"
        );
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

    // RED GATE (#158, vsdd GH#28 Gap 2): a label cluster / register anti-pattern
    // QUOTED in an inline code span is a citation, not a use — the vocab prose
    // checks (E0091/E0093) skip inline code, like the link check. The bare token
    // on the same line still fires, proving the mask is targeted.
    #[test]
    fn vocab_prose_checks_skip_inline_code_spans() {
        let proj = vocab_project(
            "vocab-inline-e0091",
            "label_schemes:\n  allow:\n  - \"^MDATRON-[ELW][0-9]{4}$\"\n",
            "Cited in code `SEC-F3` is fine; but a bare SEC-F4 is invented.\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let e0091: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "MDATRON-E0091")
            .collect();
        assert_eq!(
            e0091.len(),
            1,
            "only the bare cluster fires; got {findings:?}"
        );
        assert!(e0091[0].quoted.iter().any(|q| q.content == "SEC-F4"));

        let proj2 = vocab_project(
            "vocab-inline-e0093",
            "anti_patterns:\n- pattern: \"very unique\"\n  register: hedged-absolute\n",
            "Quoting `very unique` verbatim is a citation; saying very unique is not.\n",
        );
        let cfg2 = VerifyConfig::from_project(&proj2.0).unwrap();
        let f2 = verify(&cfg2).unwrap();
        assert_eq!(
            codes_of(&f2, "MDATRON-E0093"),
            1,
            "only the bare anti-pattern fires; got {f2:?}"
        );
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

    // #108 (vsdd item 10): a `file_globs` entry that matches no file silently
    // shrinks the checked corpus — verify exits 0 having walked fewer files than
    // the adopter declared, indistinguishable from "the jurisdiction is clean".
    // W0046 makes a dead jurisdiction glob loud, naming the pattern so a typo or
    // a stale path is caught rather than passing as coverage.
    #[test]
    fn file_glob_matching_nothing_is_loud() {
        let proj = TempProject::new("file-glob-zero-match");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"live/**/*.md\"\n  - \"typo-dir/**/*.md\"\n",
        );
        proj.write("live/doc.md", "body\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-W0046"),
            1,
            "exactly the dead glob must be loud; got {findings:?}"
        );
        let dead = findings
            .iter()
            .find(|f| f.code == "MDATRON-W0046")
            .expect("W0046 present");
        assert_eq!(
            dead.quoted.first().map(|q| q.content.as_str()),
            Some("typo-dir/**/*.md"),
            "the finding must carry the dead pattern as quoted adopter text"
        );
    }

    // #108: the mirror-image guard — a config whose every `file_globs` entry
    // matches at least one file is healthy and stays silent. Guards against
    // W0046 firing on a live jurisdiction (the false positive that would train
    // adopters to ignore it).
    #[test]
    fn file_globs_all_matching_stays_quiet() {
        let proj = TempProject::new("file-glob-all-match");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"live/**/*.md\"\n  - \"archive/**/*.md\"\n",
        );
        proj.write("live/doc.md", "body\n");
        proj.write("archive/old.md", "body\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-W0046"),
            0,
            "a fully-live jurisdiction must not warn; got {findings:?}"
        );
    }

    // #109 (surfaced by the #108 cold SE review): two overlapping `file_globs`
    // both match sub/x.md. Before the walk deduped `governed`, the file was
    // verified once per matching glob — a doubled `W0040` and a `files_checked`
    // that over-counts, corrupting the very audit signal (#105) meant to tell
    // checked-N apart from checked-nothing.
    #[test]
    fn overlapping_file_globs_check_each_file_once() {
        let proj = TempProject::new("overlap-globs");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"**/*.md\"\n  - \"sub/**/*.md\"\n\
             require_frontmatter:\n  - \"sub/**/*.md\"\n",
        );
        proj.write("sub/x.md", "no frontmatter\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let report = verify_report(&cfg).unwrap();
        assert_eq!(
            report.files_checked, 1,
            "sub/x.md is one file though two globs match it; got {}",
            report.files_checked
        );
        assert_eq!(
            codes_of(&report.findings, "MDATRON-W0040"),
            1,
            "one naked file yields one W0040, not one per matching glob; got {:?}",
            report.findings
        );
    }

    // #110 (vsdd item 2): an absent `.mdatron/schemas/` dir leaves Layer 1 unable
    // to run, yet verify exits 0 "clean" — a false-clean only `families.schema`
    // hints at. (Both dirs absent already hard-errors; this is the single-missing
    // case that slipped through.) W0047 makes the missing skeleton dir loud. An
    // *empty* dir is a legitimate opt-out and stays quiet — only a *missing* dir,
    // which `mdatron init` would have created, is drift.
    #[test]
    fn missing_schemas_dir_is_loud() {
        let proj = TempProject::new("missing-schemas");
        proj.write(".mdatron/config.yaml", "file_globs:\n  - \"**/*.md\"\n");
        // A file declaring a schema_class that nothing serves: no schema (dir
        // absent) and no rule context (patterns empty). This is the unserved
        // Layer-1 request that the missing schemas dir turns into a false-clean.
        proj.write("doc.md", "---\nschema_class: ghost\n---\nbody\n");
        // patterns dir present (empty) so the both-missing pipeline error does not
        // fire; schemas dir deliberately absent -> Layer 1 cannot run.
        std::fs::create_dir_all(proj.0.join(".mdatron/patterns")).unwrap();
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-W0047"),
            1,
            "an absent schemas dir with an unserved schema_class must be loud; got {findings:?}"
        );
        // W0045 and W0047 never double-report the same file: W0045 is the
        // present-dir per-file signal (#111), W0047 the missing-dir project-level
        // one (#110). Here the schemas dir is MISSING, so W0045 stays its hand and
        // W0047 is the sole signal.
        assert_eq!(
            codes_of(&findings, "MDATRON-W0045"),
            0,
            "schemas dir missing -> W0045 defers to W0047 as the sole signal; got {findings:?}"
        );
    }

    // #110: a Schematron-only project (a schema_class that selects a rule context,
    // no schemas dir) is legitimate and must NOT trip W0047 — the schema_class is
    // doing Layer-2 work, so the absent schemas dir is not a false-clean.
    #[test]
    fn missing_schemas_dir_with_layer_two_coverage_stays_quiet() {
        let proj = TempProject::new("missing-schemas-l2");
        // A pattern whose rule context is `note`: it serves schema_class `note`
        // via Layer 2, so the file is validated despite the absent schemas dir.
        proj.write(
            ".mdatron/patterns/notes.yaml",
            r#"mdatron_dsl_version: 1
pattern:
  id: notes
  rules:
    - id: note-ok
      context: note
      assert: $self.schema_class == "note"
      code: TEST-E0001
      message: "note check"
"#,
        );
        proj.write("doc.md", "---\nschema_class: note\n---\nbody\n");
        let cfg = VerifyConfig::new(&proj.0);
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-W0047"),
            0,
            "a schema_class served by a rule context is not a false-clean; got {findings:?}"
        );
    }

    // #110: the negative guard — a present schemas dir (even one seeded with a
    // real schema) must not warn. Guards against W0047 firing on a healthy
    // skeleton.
    // #125 (roast SHO7): a present config that declares no file_globs refuses
    // rather than silently defaulting to whole-tree `**/*.md` (the #80-D1
    // overreach; also what a typo'd `file_glob:` key would trigger).
    #[test]
    fn present_config_with_no_file_globs_refuses() {
        let proj = TempProject::new("no-globs");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        // config present, but no file_globs list at all.
        proj.write(
            ".mdatron/config.yaml",
            "require_frontmatter:\n  - \"**/*.md\"\n",
        );
        let err = VerifyConfig::from_project(&proj.0).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("no `file_globs`") || msg.contains("jurisdiction must be explicit"),
            "a globless config must refuse, not default to whole-tree; got: {msg}"
        );
        // roast A1 (#140): the refusal names the config RELATIVELY (this message
        // becomes pipeline_error.message), so it must not leak the absolute root.
        assert!(
            msg.contains("config.yaml") && !msg.contains(&*proj.0.to_string_lossy()),
            "the refusal carries a relative config path, not an absolute one; got: {msg}"
        );
    }

    // roast A1 (#140): the most common pipeline failure — a missing config — names
    // the config RELATIVELY in its message (which becomes pipeline_error.message),
    // so it does not leak the host filesystem layout into the JSON envelope.
    #[test]
    fn missing_config_error_path_is_relative() {
        let proj = TempProject::new("no-config-at-all");
        let err = VerifyConfig::from_project(&proj.0).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("no jurisdiction declared"),
            "the missing-config refusal fires; got: {msg}"
        );
        assert!(
            msg.contains("config.yaml") && !msg.contains(&*proj.0.to_string_lossy()),
            "no absolute host path in the missing-config message; got: {msg}"
        );
    }

    #[test]
    fn present_schemas_dir_stays_quiet() {
        let proj = TempProject::new("present-schemas");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(".mdatron/config.yaml", "file_globs:\n  - \"**/*.md\"\n");
        proj.write("doc.md", "body\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-W0047"),
            0,
            "a present schemas dir must not warn; got {findings:?}"
        );
    }

    // DEF4 (#131 SO ruling): every diagnostic path in a verify run is
    // project-root-relative — no absolute host path leaks into the envelope, and
    // two machines' runs are diffable. W0047 (a project-level finding) previously
    // emitted an absolute `project_root.join(...)` path.
    #[test]
    fn all_diagnostic_paths_are_project_root_relative() {
        let proj = TempProject::new("rel-paths");
        proj.write(".mdatron/config.yaml", "file_globs:\n  - \"**/*.md\"\n");
        // schemas dir absent -> W0047 (a project-level finding); patterns present.
        std::fs::create_dir_all(proj.0.join(".mdatron/patterns")).unwrap();
        proj.write("doc.md", "---\nschema_class: ghost\n---\nbody\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(!findings.is_empty(), "the scenario yields findings");
        for f in &findings {
            assert!(
                f.location.file.is_relative(),
                "diagnostic path must be project-root-relative; got absolute {:?} for {}",
                f.location.file,
                f.code
            );
        }
        let w0047 = findings
            .iter()
            .find(|f| f.code == "MDATRON-W0047")
            .expect("the absent schemas dir warns");
        assert_eq!(
            w0047.location.file,
            std::path::Path::new(".mdatron/schemas"),
            "the project-level finding carries the relative skeleton path"
        );
    }

    // DEF4 completion (#134, roast A1): an absolute `file_globs` entry escapes the
    // root; the E0010 finding anchors at the config (relative) with the escaping
    // path as quoted content — it must NOT leak an absolute path in location.file
    // (the escaping path is outside the root and cannot be relativized).
    #[cfg(unix)]
    #[test]
    fn absolute_glob_escape_finding_is_root_relative() {
        let outside = std::env::temp_dir().join(format!(
            "mdatron-escape-{}.md",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&outside, "x\n").unwrap();
        let proj = TempProject::new("abs-glob");
        proj.write(
            ".mdatron/config.yaml",
            &format!("file_globs:\n  - \"{}\"\n", outside.display()),
        );
        std::fs::create_dir_all(proj.0.join(".mdatron/patterns")).unwrap();
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let esc = findings
            .iter()
            .find(|f| f.code == "MDATRON-E0010")
            .expect("an absolute glob escapes the root -> E0010");
        assert_eq!(
            esc.location.file,
            std::path::Path::new(".mdatron/config.yaml"),
            "the escape finding anchors at the config, not the absolute path"
        );
        assert!(esc.location.file.is_relative());
        let _ = std::fs::remove_file(&outside);
    }

    // DEF4 completion (#134, roast B1): a pipeline error's absolute path is
    // relativized, so `pipeline_error.message` does not leak the host layout.
    #[test]
    fn verify_error_relativize_paths_strips_the_root() {
        let root = std::path::Path::new("/tmp/proj");
        let e = VerifyError::SchemaLoad {
            path: "/tmp/proj/.mdatron/schemas/x.json".into(),
            error: "bad json".into(),
        };
        assert_eq!(
            e.relativize_paths(root).to_string(),
            "schema load error at '.mdatron/schemas/x.json': bad json"
        );
        // a path not under root (a pre-canonicalization error) is left intact.
        let e2 = VerifyError::Io {
            path: "/elsewhere/x".into(),
            error: "boom".into(),
        };
        assert!(e2
            .relativize_paths(root)
            .to_string()
            .contains("/elsewhere/x"));
        // the root itself relativizes to "." rather than an empty ''.
        let e3 = VerifyError::Io {
            path: "/tmp/proj".into(),
            error: "boom".into(),
        };
        assert_eq!(
            e3.relativize_paths(root).to_string(),
            "io error at '.': boom"
        );
    }

    // DEF4 completion (#134, roast B2): the DSL `$file.path` interpolated into a
    // rule message rides as a quoted region with the project-root-relative path,
    // not the absolute host path — matching the finding's relativized location.
    #[test]
    fn file_path_in_a_rule_message_is_project_root_relative() {
        let proj = TempProject::new("file-path-rel");
        proj.write(
            ".mdatron/patterns/p.yaml",
            r#"mdatron_dsl_version: 1
pattern:
  id: p
  rules:
    - id: always-fails
      context: doc
      assert: $self.schema_class == "never"
      code: TEST-E0002
      message: "checked {{$file.path}}"
"#,
        );
        proj.write("sub/doc.md", "---\nschema_class: doc\n---\n");
        let cfg = VerifyConfig::new(&proj.0);
        let findings = verify(&cfg).unwrap();
        let f = findings
            .iter()
            .find(|f| f.code == "TEST-E0002")
            .expect("the rule fires");
        let quoted: Vec<&str> = f.quoted.iter().map(|q| q.content.as_str()).collect();
        // Separator-agnostic: `$file.path` renders with the OS separator (a
        // backslash on Windows), so compare as a Path, not a raw string. (The
        // envelope carrying OS-native separators, rather than always `/`, is a
        // cross-platform reproducibility gap tracked separately as a DEF4
        // follow-up.)
        assert!(
            quoted
                .iter()
                .any(|c| std::path::Path::new(c) == std::path::Path::new("sub/doc.md")),
            "$file.path is project-root-relative; got {quoted:?}"
        );
        let abs = proj.0.to_string_lossy().into_owned();
        assert!(
            !quoted.iter().any(|c| c.contains(&abs)),
            "no absolute host path in quoted content; got {quoted:?}"
        );
    }

    // #124 (roast SHO1): the flow-nesting pre-scan counts bracket depth.
    #[test]
    fn max_flow_nesting_counts_bracket_depth() {
        assert_eq!(max_flow_nesting("[[[]]]"), 3);
        assert_eq!(max_flow_nesting("{a: [1, {b: 2}]}"), 3);
        assert_eq!(max_flow_nesting("plain: value"), 0);
    }

    // #124 (roast SHO1): a deeply-nested frontmatter (the O(n^2) depth-bomb) trips
    // a loud resource bound instead of driving quadratic parse CPU. The bound is a
    // pipeline failure (kind `bound_exceeded`), never a silent hang.
    #[test]
    fn deeply_nested_frontmatter_trips_the_bound() {
        let proj = TempProject::new("depth-bound");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(".mdatron/config.yaml", "file_globs:\n  - \"**/*.md\"\n");
        let bomb = format!(
            "---\nx: {}{}\n---\nbody\n",
            "[".repeat(300),
            "]".repeat(300)
        );
        proj.write("doc.md", &bomb);
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        match verify(&cfg) {
            Err(VerifyError::BoundExceeded { bound, .. }) => {
                assert_eq!(bound, "structural-nesting-depth")
            }
            other => panic!("expected BoundExceeded(structural-nesting-depth); got {other:?}"),
        }
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

    // #105 (audit signal): files_checked is the count of files VALIDATED, not
    // files that produced findings — a clean run over N files reports N, so
    // "checked N, all clean" is distinguishable from "checked nothing".
    #[test]
    fn files_checked_counts_validated_files_not_findings() {
        let proj = TempProject::new("files-checked");
        // Route class `x` to a permissive schema so the corpus is genuinely
        // clean (not W0045-unrouted).
        proj.write(
            ".mdatron/schemas/x.json",
            r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object"}"#,
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("docs/a.md", "---\nschema_class: x\n---\n");
        proj.write("docs/b.md", "---\nschema_class: x\n---\n");
        proj.write("docs/c.md", "---\nschema_class: x\n---\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let report = verify_report(&cfg).unwrap();
        assert!(report.findings.is_empty(), "the corpus is clean");
        assert_eq!(
            report.files_checked, 3,
            "a clean run over 3 files reports 3, not 0"
        );
    }

    // An empty jurisdiction validates 0 files — distinguishable from a clean
    // corpus (which reports N), closing the through-line audit-signal gap.
    #[test]
    fn files_checked_is_zero_for_empty_jurisdiction() {
        let proj = TempProject::new("files-checked-empty");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"nothing/**/*.md\"\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        assert_eq!(verify_report(&cfg).unwrap().files_checked, 0);
    }

    // Incremental mode counts only the files it validated (the scope).
    #[test]
    fn files_checked_in_incremental_is_the_scope_size() {
        let proj = TempProject::new("files-checked-inc");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("docs/a.md", "---\nschema_class: x\n---\n");
        proj.write("docs/b.md", "---\nschema_class: x\n---\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        // docs/a.md has no dependents -> scope {a}; one file validated.
        let inc = verify_incremental(&cfg, Path::new("docs/a.md")).unwrap();
        assert_eq!(
            inc.report.files_checked, 1,
            "only the changed file validated"
        );
    }

    // #106 (audit signal): a schema_class that routes to no schema AND no rule
    // context is flagged (W0045), not silently unvalidated.
    #[test]
    fn unrouted_schema_class_is_flagged() {
        let proj = TempProject::new("unrouted-class");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("docs/a.md", "---\nschema_class: made-up\n---\n# a\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-W0045"),
            1,
            "a made-up class is flagged; got {findings:?}"
        );
        let f = findings.iter().find(|f| f.code == "MDATRON-W0045").unwrap();
        assert!(
            f.quoted.iter().any(|q| q.content == "made-up"),
            "the class rides quoted"
        );

        // A class WITH a schema is routed -> silent.
        proj.write(
            ".mdatron/schemas/made-up.json",
            r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object"}"#,
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        assert_eq!(
            codes_of(&verify(&cfg).unwrap(), "MDATRON-W0045"),
            0,
            "a class with a schema is routed, not flagged"
        );
    }

    // #111 (design Q, answered by vsdd-cli mdatron#1): a declared schema_class that
    // nothing serves is unrouted even with NO validation infrastructure at all —
    // the present-but-empty schemas dir `mdatron init` leaves, plus a schema_class
    // whose `.json` was never added. The old has-infra gate suppressed W0045 here
    // and the present dir suppressed W0047, so it exited silently clean. Now W0045
    // fires whenever the schemas dir is PRESENT (empty or not); a MISSING dir stays
    // W0047's project-level job (missing_schemas_dir_is_loud), and a file with no
    // schema_class stays clean (fresh-init-clean preserved).
    #[test]
    fn declared_class_with_present_empty_schemas_dir_is_flagged() {
        let proj = TempProject::new("empty-schemas-declared");
        // schemas + patterns dirs present but EMPTY: no validation infrastructure.
        std::fs::create_dir_all(proj.0.join(".mdatron/schemas")).unwrap();
        std::fs::create_dir_all(proj.0.join(".mdatron/patterns")).unwrap();
        proj.write(".mdatron/config.yaml", "file_globs:\n  - \"**/*.md\"\n");
        proj.write("doc.md", "---\nschema_class: anything\n---\nbody\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-W0045"),
            1,
            "an unserved schema_class under an empty (present) schemas dir is flagged; got {findings:?}"
        );
        // The dir is present, so the MISSING-dir signal (W0047) must stay quiet.
        assert_eq!(
            codes_of(&findings, "MDATRON-W0047"),
            0,
            "schemas dir present (empty) -> W0047 (missing-dir) does not fire; got {findings:?}"
        );

        // Fresh-init-clean preserved: an init'd project with NO declared
        // schema_class stays clean (present-empty schemas + patterns, prose only).
        let fresh = TempProject::new("empty-schemas-fresh");
        std::fs::create_dir_all(fresh.0.join(".mdatron/schemas")).unwrap();
        std::fs::create_dir_all(fresh.0.join(".mdatron/patterns")).unwrap();
        fresh.write(".mdatron/config.yaml", "file_globs:\n  - \"**/*.md\"\n");
        fresh.write("prose.md", "---\ntitle: just prose\n---\nbody\n");
        let cfg = VerifyConfig::from_project(&fresh.0).unwrap();
        assert_eq!(
            codes_of(&verify(&cfg).unwrap(), "MDATRON-W0045"),
            0,
            "a project with no declared schema_class stays clean; got findings"
        );
    }

    // A class validated by a RULE context (no schema) is routed -> silent (no
    // false positive on legitimate pattern-only classes).
    #[test]
    fn schema_class_routed_by_rule_context_is_not_flagged() {
        let proj = TempProject::new("class-by-rule");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write(
            ".mdatron/patterns/p.yaml",
            "mdatron_dsl_version: 1\npattern:\n  id: p\n  rules:\n    - id: r\n      context: post\n      assert: '$self.title != \"\"'\n      code: ADOPTER-E0001\n      message: m\n",
        );
        proj.write("docs/a.md", "---\nschema_class: post\ntitle: ok\n---\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        assert_eq!(
            codes_of(&verify(&cfg).unwrap(), "MDATRON-W0045"),
            0,
            "a class routed by a rule context is not flagged"
        );
    }

    // A file with NO schema_class is not a W0045 candidate (it claims no type).
    #[test]
    fn no_schema_class_is_not_unrouted() {
        let proj = TempProject::new("no-class");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("docs/a.md", "---\ntitle: prose\n---\n# a\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        assert_eq!(codes_of(&verify(&cfg).unwrap(), "MDATRON-W0045"), 0);
    }

    // RED GATE (SA F1, scoping-consistency pass): a PATH-GLOB `context:` fires on
    // a matching file. It now resolves against the project-root-relative path via
    // `matches_path`; before the fix it matched the ABSOLUTE path via a string
    // compare and silently never fired — a rule that could register a dependency
    // edge yet never actually run.
    #[test]
    fn path_glob_context_fires_on_matching_file() {
        let proj = TempProject::new("ctx-pathglob");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write(
            ".mdatron/patterns/p.yaml",
            "mdatron_dsl_version: 1\npattern:\n  id: p\n  rules:\n    - id: r\n      context: \"docs/**/*.md\"\n      assert: '$self.title != \"\"'\n      code: ADOPTER-E0001\n      message: m\n",
        );
        // title is empty → the assertion is FALSE → the rule fires, but ONLY if
        // the path-glob context matched docs/a.md (root-relative) in the first place.
        proj.write("docs/a.md", "---\ntitle: \"\"\n---\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        assert!(
            verify(&cfg)
                .unwrap()
                .iter()
                .any(|f| f.code == "ADOPTER-E0001"),
            "a path-glob context resolves root-relative and fires on docs/a.md"
        );
    }

    // RED GATE (SA F4, scoping-consistency pass): a scope glob that escapes the
    // governed tree is a LOUD config error, not a silent no-op. `../secret/*.md`
    // used to compile and match nothing (silently disabling the scope); now it is
    // refused at load — the confinement contract applied to config scope globs.
    #[test]
    fn escaping_scope_glob_is_a_loud_config_error() {
        let proj = TempProject::new("scope-escape");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\nvocabulary_globs:\n  - \"../secret/*.md\"\n",
        );
        proj.write("docs/a.md", "prose\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let err = format!("{}", verify(&cfg).unwrap_err());
        assert!(
            err.contains("vocabulary_globs") && err.contains(".."),
            "an escaping vocabulary_globs glob is refused loudly; got {err}"
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

    // ── link family (#145): body-link / anchor resolution ──────────────────
    //
    // Links resolve DOCUMENT-relative (CommonMark/GitHub semantics): a target
    // is relative to the containing file's directory, not the project root. So
    // a one-level `../` that stays inside the tree is legitimate, and only an
    // escape ABOVE the root is refused — the divergence from the citation
    // family's categorical `..`-refusal is deliberate (a link checker that
    // flagged every `../README.md` would be unusable in `docs/**`).

    fn link_project(label: &str, body: &str) -> TempProject {
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
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n  links: true\n",
        );
        // A sibling target carrying one heading -> slug "real-heading".
        proj.write("docs/target.md", "# Real Heading\n\nbody\n");
        proj.write(
            "docs/2026-07-27-doc.md",
            &format!("---\nschema_class: phase-primer\nphase: phase-1a\nrelevant_domains: [se]\n---\n{body}"),
        );
        proj
    }

    fn link_families(proj: &TempProject) -> crate::output::Families {
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let (_f, fam, _v, _n) = run(&cfg, None, None).unwrap();
        fam
    }

    // GH #37: with `link_root: true` on the route, a leading-slash link
    // resolves from the project root (existing target clean; missing flags),
    // and confinement still holds. Without the flag it stays E0010.
    #[test]
    fn root_relative_link_mode_resolves_from_project_root() {
        let proj = TempProject::new("link-root");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(".mdatron/config.yaml", "file_globs:\n  - \"**/*.md\"\n");
        proj.write("GOVERNING.md", "# gov\n");
        proj.write(
            ".mdatron/routes.yaml",
            "routes:\n- files: \"notes/**/*.md\"\n  governed_by: GOVERNING.md\n  links: true\n  link_root: true\n",
        );
        proj.write("docs/real.md", "# real\n");
        // A note deep in the tree links root-relative to /docs/real.md (exists)
        // and /docs/gone.md (missing).
        proj.write(
            "notes/deep/n.md",
            "---\nschema_class: phase-primer\nphase: phase-1a\nrelevant_domains: [se]\n---\nSee [ok](/docs/real.md) and [bad](/docs/gone.md).\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let dead: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.code == "MDATRON-E0110")
            .collect();
        assert_eq!(
            dead.len(),
            1,
            "only the missing root-relative target; got {findings:?}"
        );
        assert!(dead[0]
            .quoted
            .iter()
            .any(|q| q.content.contains("/docs/gone.md")));
        // No E0010: the leading slash is resolved, not refused.
        assert_eq!(codes_of(&findings, "MDATRON-E0010"), 0, "{findings:?}");
    }

    // GH #37: WITHOUT link_root, a leading-slash link is refused E0010 (default
    // document-relative posture, matching lychee's default).
    #[test]
    fn leading_slash_link_without_root_mode_is_e0010() {
        let proj = link_project("link-slash-default", "See [x](/docs/target.md).\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(codes_of(&findings, "MDATRON-E0010"), 1, "{findings:?}");
    }

    // GH #37 phase-3 F-1: `link_root: true` WITHOUT `links: true` is inert —
    // the link family does no work at all, so a leading-slash link produces no
    // link-family finding (the flag only affects HOW the link check resolves,
    // never whether it runs).
    #[test]
    fn link_root_without_links_is_inert() {
        let proj = TempProject::new("link-root-inert");
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
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n  link_root: true\n",
        );
        proj.write(
            "docs/d.md",
            "---\nschema_class: phase-primer\nphase: phase-1a\nrelevant_domains: [se]\n---\nSee [x](/docs/gone.md) and [y](also-gone.md).\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        for code in ["MDATRON-E0110", "MDATRON-E0111", "MDATRON-E0010"] {
            assert_eq!(
                codes_of(&findings, code),
                0,
                "{code} should not fire; got {findings:?}"
            );
        }
    }

    // GH #37 phase-3 F-2: a percent-encoded `#fragment` resolves the heading
    // (GitHub decodes fragments) — the fragment twin of the path decode.
    #[test]
    fn percent_encoded_fragment_resolves_heading() {
        let proj = link_project("link-frag-pct", "See [x](target.md#caf%C3%A9).\n");
        // target.md's heading "Café" -> slug "café".
        proj.write("docs/target.md", "# Café\n\nbody\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-E0111"),
            0,
            "the decoded fragment matches the heading; got {findings:?}"
        );
    }

    // GH #37 phase-3 F-3: a bare-root `/` link under root mode is out of scope
    // (the project root is a directory, not a document) — not a dead-link E0110.
    #[test]
    fn bare_root_link_under_root_mode_is_not_dead_link() {
        let proj = TempProject::new("link-bare-root");
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
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n  links: true\n  link_root: true\n",
        );
        proj.write(
            "docs/d.md",
            "---\nschema_class: phase-primer\nphase: phase-1a\nrelevant_domains: [se]\n---\nSee [root](/).\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(codes_of(&findings, "MDATRON-E0110"), 0, "{findings:?}");
        assert_eq!(
            codes_of(&findings, "MDATRON-E0080"),
            0,
            "no engine-defect miss either"
        );
    }

    // GH #37: a percent-encoded link to a real file with a space in its name
    // resolves (was a latent E0110 false positive on the literal `%20`), while a
    // percent-encoded link to a genuinely-missing file still flags.
    #[test]
    fn percent_encoded_link_resolves_against_decoded_path() {
        let proj = link_project(
            "link-pct",
            "See [spaced](my%20doc.md) and [gone](ab%20sent.md).\n",
        );
        proj.write("docs/my doc.md", "# spaced target\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let dead: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.code == "MDATRON-E0110")
            .collect();
        assert_eq!(
            dead.len(),
            1,
            "only the truly-missing target flags; got {findings:?}"
        );
        // The finding quotes the ORIGINAL destination, not the decoded form.
        assert!(
            dead[0]
                .quoted
                .iter()
                .any(|q| q.content.contains("ab%20sent.md")),
            "the original encoded dest is quoted: {:?}",
            dead[0].quoted
        );
    }

    // RED GATE (#145): a body link to a missing in-tree file is E0110.
    #[test]
    fn dead_link_target_rejected() {
        let proj = link_project("link-dead", "See [the missing doc](no-such.md).\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let f = findings
            .iter()
            .find(|f| f.code == "MDATRON-E0110")
            .unwrap_or_else(|| panic!("expected E0110; got {findings:?}"));
        assert!(f.quoted.iter().any(|q| q.content.contains("no-such.md")));
    }

    // RED GATE (#145): a link to an existing file whose fragment matches no
    // heading is E0111; the same file with a real heading fragment is clean.
    #[test]
    fn dead_anchor_rejected() {
        let proj = link_project(
            "link-anchor",
            "Good: [x](target.md#real-heading). Bad: [y](target.md#ghost).\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.code == "MDATRON-E0111")
                .count(),
            1,
            "exactly the ghost anchor flags; got {findings:?}"
        );
        assert!(findings.iter().all(|f| f.code != "MDATRON-E0110"));
    }

    // RED GATE (#145): a same-document anchor resolves against this file's own
    // headings; a missing one is E0111 and quotes the fragment.
    #[test]
    fn same_doc_anchor_resolves() {
        let proj = link_project(
            "link-samedoc",
            "## Local Section\n\nJump [here](#local-section) and [nowhere](#gone).\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let anchors: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "MDATRON-E0111")
            .collect();
        assert_eq!(
            anchors.len(),
            1,
            "only the #gone anchor flags; got {findings:?}"
        );
        assert!(anchors[0]
            .quoted
            .iter()
            .any(|q| q.content.contains("#gone")));
    }

    // RED GATE (#145): a valid relative link to an existing file is clean,
    // including a document-relative one-level parent link that stays in-tree.
    #[test]
    fn valid_links_are_clean() {
        let proj = link_project(
            "link-clean",
            "See [sibling](target.md) and [root gov](../GOVERNING.md).\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().all(|f| !f.code.starts_with("MDATRON-E011")),
            "in-tree links (incl. one-level-up parent) are clean; got {findings:?}"
        );
    }

    // RED GATE (#145): external links are deferred — an http URL and a mailto
    // are never resolved or flagged.
    #[test]
    fn external_links_are_deferred() {
        let proj = link_project(
            "link-external",
            "See [site](https://example.com/a.md) and [mail](mailto:x@y.z).\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().all(|f| !f.code.starts_with("MDATRON-E011")),
            "external links are deferred; got {findings:?}"
        );
    }

    // RED GATE (#145): a link target escaping the tree is refused under the
    // confinement codes; document-relative resolution means the escape must
    // climb ABOVE the root (a one-level parent that stays in-tree is clean).
    #[test]
    fn escaping_link_target_refused() {
        let proj = link_project(
            "link-escape",
            "Bad: [up](../../outside.md) and [abs](/etc/passwd.txt).\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().any(|f| f.code == "MDATRON-E0011"),
            "parent-escape link refused; got {findings:?}"
        );
        assert!(
            findings.iter().any(|f| f.code == "MDATRON-E0010"),
            "absolute link refused; got {findings:?}"
        );
    }

    // RED GATE (#145): links inside a fenced code block are examples, not live
    // links — they must not be resolved (else a docs page showing link syntax
    // false-positives).
    #[test]
    fn links_in_code_fences_are_ignored() {
        let proj = link_project(
            "link-fence",
            "```\n[example](no-such-in-fence.md)\n```\n\nreal [ok](target.md)\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().all(|f| f.code != "MDATRON-E0110"),
            "a link inside a code fence is not resolved; got {findings:?}"
        );
    }

    // RED GATE (#154): a link inside an INLINE code span is an example, not a
    // live link — the line-based scanner must mask inline `code`, not just
    // fenced blocks (the lychee-review defect).
    #[test]
    fn links_in_inline_code_are_ignored() {
        let proj = link_project(
            "link-inline-code",
            "Use the `[example](no-such.md)` form; real [ok](target.md).\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().all(|f| f.code != "MDATRON-E0110"),
            "a link inside an inline code span is not resolved; got {findings:?}"
        );
    }

    // RED GATE (#145): opt-in — a route WITHOUT links: true gets no link findings.
    #[test]
    fn links_are_per_route_opt_in() {
        let proj = link_project("link-optout", "See [missing](no-such.md).\n");
        proj.write(
            ".mdatron/routes.yaml",
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().all(|f| !f.code.starts_with("MDATRON-E011")),
            "no opt-in, no link findings; got {findings:?}"
        );
    }

    // RED GATE (#145): the link family reports active exactly when a route opts
    // in with links: true, inactive otherwise (falsifiable audit signal).
    #[test]
    fn link_family_activity_tracks_opt_in() {
        let opted_in = link_project("link-active", "prose\n");
        assert!(link_families(&opted_in).link.is_active());

        let opted_out = link_project("link-inactive", "prose\n");
        opted_out.write(
            ".mdatron/routes.yaml",
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n",
        );
        assert!(!link_families(&opted_out).link.is_active());
    }

    // ── link family gap closures (#155, the pulldown-cmark migration) ───────
    //
    // Reference-style links, image links, setext headings, HTML anchors, and
    // GitHub duplicate-heading `-N` disambiguation — all deferred in the #145
    // first cut, all closed by the CommonMark parse.

    // RED GATE (#155 gap 1): GitHub gives a repeated heading the suffixes
    // `-1`/`-2`. A link to the disambiguated `#dup-1` resolves; only a truly
    // absent anchor (`#dup-2`, no third heading) flags.
    #[test]
    fn duplicate_heading_disambiguation_resolves() {
        let proj = link_project(
            "link-dup-heading",
            "## Dup\n\ntext\n\n## Dup\n\nSecond [x](#dup-1) and absent [y](#dup-2).\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let anchors: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "MDATRON-E0111")
            .collect();
        assert_eq!(
            anchors.len(),
            1,
            "#dup-1 resolves to the 2nd heading; only #dup-2 flags; got {findings:?}"
        );
        assert!(anchors[0]
            .quoted
            .iter()
            .any(|q| q.content.contains("#dup-2")));
    }

    // RED GATE (#155 gap 2): reference-style `[t][ref]` links are now resolved.
    // A definition pointing at a missing file is E0110; one pointing at a real
    // file is clean (was a silent false negative).
    #[test]
    fn reference_style_link_resolves() {
        let proj = link_project(
            "link-refstyle",
            "Broken [a][bad] and good [b][ok].\n\n[bad]: no-such.md\n[ok]: target.md\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let dead: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "MDATRON-E0110")
            .collect();
        assert_eq!(
            dead.len(),
            1,
            "only the missing reference target flags; got {findings:?}"
        );
        assert!(dead[0]
            .quoted
            .iter()
            .any(|q| q.content.contains("no-such.md")));
    }

    // RED GATE (#155 gap 2): image links `![alt](src)` are now resolved for
    // existence — a broken image is E0110 (was a silent false negative).
    #[test]
    fn image_link_to_missing_is_e0110() {
        let proj = link_project(
            "link-image",
            "![diagram](no-image.png) then a present target ![ok](target.md).\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let dead: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "MDATRON-E0110")
            .collect();
        assert_eq!(dead.len(), 1, "the missing image flags; got {findings:?}");
        assert!(dead[0]
            .quoted
            .iter()
            .any(|q| q.content.contains("no-image.png")));
    }

    // RED GATE (#155 gap 3): a setext heading (underlined) is a heading, so its
    // slug resolves a same-document anchor.
    #[test]
    fn setext_heading_anchor_resolves() {
        let proj = link_project(
            "link-setext",
            "Setext Title\n============\n\nJump [here](#setext-title) and [gone](#nope).\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let anchors: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "MDATRON-E0111")
            .collect();
        assert_eq!(
            anchors.len(),
            1,
            "the setext slug resolves; only #nope flags; got {findings:?}"
        );
    }

    // RED GATE (#155 gap 3): an explicit HTML anchor `<a name=...>` is a valid
    // `#fragment` target.
    #[test]
    fn html_anchor_resolves() {
        let proj = link_project(
            "link-html-anchor",
            "<a name=\"manual-spot\"></a>\n\nJump [here](#manual-spot) not [x](#absent).\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let anchors: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "MDATRON-E0111")
            .collect();
        assert_eq!(
            anchors.len(),
            1,
            "the HTML anchor resolves #manual-spot; only #absent flags; got {findings:?}"
        );
    }

    // ── marker-line reference family (#147, vsdd GH#20 P3 / GH#22) ──────────
    //
    // A body line matching a declared pattern names a reference whose captured
    // `<name>` must resolve to an element in a rule-config-named target doc.
    // Fixtures mirror vsdd's live shape: a build-plan's `Provenance:` lines
    // resolving to a contract's `- **bold name**` list items (name-equality,
    // trailing `.` tolerated), optionally scoped to a target section.

    // The resolution target doc (rule-config-named, project-root-relative, and
    // deliberately OUTSIDE the walked `docs/**` set — a pure resolution target).
    const MARKER_TARGET: &str = r#"# Contract

## Decomposition (phase 1c)

- **Slice 1 — Live self-governance: the tracker join.** first slice detail
- **Slice 2 — First guardrail.** second slice detail

## Other Section

- **Elsewhere item.** unrelated
"#;

    // A route opting its files into a list-item-bold-name marker rule.
    const MARKER_ROUTE_OPTIN: &str = r#"routes:
- files: "docs/**/*.md"
  governed_by: GOVERNING.md
  marker_rules:
    - pattern: "^Provenance: (.+)$"
      element: list-item-bold-name
      target_doc: refs/contract.md
"#;

    fn marker_project(label: &str, plan_body: &str, routes_yaml: &str) -> TempProject {
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
        proj.write(".mdatron/routes.yaml", routes_yaml);
        // Resolution target, outside the walked set (so it is never E0030-unrouted).
        proj.write("refs/contract.md", MARKER_TARGET);
        // The governed marker file (vsdd's build-plan analog).
        proj.write("docs/build-plan.md", plan_body);
        proj
    }

    fn marker_families(proj: &TempProject) -> crate::output::Families {
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let (_f, fam, _v, _n) = run(&cfg, None, None).unwrap();
        fam
    }

    // RED GATE (#147): a Provenance line naming a member that is not in the
    // target doc is E0112, and quotes the captured name.
    #[test]
    fn dead_marker_reference_rejected() {
        let proj = marker_project(
            "marker-dead",
            "Provenance: No Such Slice\n",
            MARKER_ROUTE_OPTIN,
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let f = findings
            .iter()
            .find(|f| f.code == "MDATRON-E0112")
            .unwrap_or_else(|| panic!("expected E0112; got {findings:?}"));
        assert!(f.quoted.iter().any(|q| q.content.contains("No Such Slice")));
    }

    // RED GATE (#147): a Provenance line whose name matches a `- **bold**` list
    // item resolves clean — the target carries a trailing `.` the marker omits
    // (tolerated), and the name is an em-dashed descriptive phrase, not an id.
    #[test]
    fn resolved_marker_reference_is_clean() {
        let proj = marker_project(
            "marker-ok",
            "Provenance: Slice 1 — Live self-governance: the tracker join\n",
            MARKER_ROUTE_OPTIN,
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().all(|f| f.code != "MDATRON-E0112"),
            "a resolved marker reference is clean; got {findings:?}"
        );
    }

    // RED GATE (#147): target_section scopes resolution to that heading's span —
    // a name present in the doc but OUTSIDE the section does not resolve; a name
    // INSIDE it does.
    #[test]
    fn marker_target_section_scopes_resolution() {
        let routes = r###"routes:
- files: "docs/**/*.md"
  governed_by: GOVERNING.md
  marker_rules:
    - pattern: "^Provenance: (.+)$"
      element: list-item-bold-name
      target_doc: refs/contract.md
      target_section: "## Decomposition (phase 1c)"
"###;
        // 'Elsewhere item' lives under '## Other Section', outside the scope.
        let out = marker_project("marker-section-out", "Provenance: Elsewhere item\n", routes);
        let out_f = verify(&VerifyConfig::from_project(&out.0).unwrap()).unwrap();
        assert!(
            out_f.iter().any(|f| f.code == "MDATRON-E0112"),
            "a name outside the scoped section does not resolve; got {out_f:?}"
        );
        // 'Slice 2' lives inside the scoped section.
        let inside = marker_project(
            "marker-section-in",
            "Provenance: Slice 2 — First guardrail\n",
            routes,
        );
        let in_f = verify(&VerifyConfig::from_project(&inside.0).unwrap()).unwrap();
        assert!(
            in_f.iter().all(|f| f.code != "MDATRON-E0112"),
            "an in-section name resolves; got {in_f:?}"
        );
    }

    // RED GATE (#147): the `heading` element class resolves the name against the
    // target doc's headings (name-equality).
    #[test]
    fn marker_heading_element_class() {
        let routes = r#"routes:
- files: "docs/**/*.md"
  governed_by: GOVERNING.md
  marker_rules:
    - pattern: "^See: (.+)$"
      element: heading
      target_doc: refs/contract.md
"#;
        let good = marker_project("marker-head-ok", "See: Other Section\n", routes);
        assert!(verify(&VerifyConfig::from_project(&good.0).unwrap())
            .unwrap()
            .iter()
            .all(|f| f.code != "MDATRON-E0112"));
        let bad = marker_project("marker-head-bad", "See: Nonexistent Heading\n", routes);
        assert!(verify(&VerifyConfig::from_project(&bad.0).unwrap())
            .unwrap()
            .iter()
            .any(|f| f.code == "MDATRON-E0112"));
    }

    // RED GATE (#147): the target_doc obeys the confinement contract (it is a
    // rule-config path, held categorically like a citation path): a parent-escape
    // is E0011, an absolute path is E0010.
    #[test]
    fn marker_target_doc_confinement() {
        let parent = r#"routes:
- files: "docs/**/*.md"
  governed_by: GOVERNING.md
  marker_rules:
    - pattern: "^Provenance: (.+)$"
      element: list-item-bold-name
      target_doc: ../../outside.md
"#;
        let p = marker_project("marker-escape", "Provenance: Anything\n", parent);
        let pf = verify(&VerifyConfig::from_project(&p.0).unwrap()).unwrap();
        assert!(
            pf.iter().any(|f| f.code == "MDATRON-E0011"),
            "parent-escape target_doc refused; got {pf:?}"
        );
        let abs = r#"routes:
- files: "docs/**/*.md"
  governed_by: GOVERNING.md
  marker_rules:
    - pattern: "^Provenance: (.+)$"
      element: list-item-bold-name
      target_doc: /etc/passwd.txt
"#;
        let a = marker_project("marker-abs", "Provenance: Anything\n", abs);
        let af = verify(&VerifyConfig::from_project(&a.0).unwrap()).unwrap();
        assert!(
            af.iter().any(|f| f.code == "MDATRON-E0010"),
            "absolute target_doc refused; got {af:?}"
        );
    }

    // RED GATE (#147): a marker line inside a fenced code block is an example,
    // not a live reference — not resolved.
    #[test]
    fn marker_lines_in_code_fences_are_ignored() {
        let plan = "```\nProvenance: No Such Slice\n```\n\nProvenance: Slice 2 — First guardrail\n";
        let proj = marker_project("marker-fence", plan, MARKER_ROUTE_OPTIN);
        let findings = verify(&VerifyConfig::from_project(&proj.0).unwrap()).unwrap();
        assert!(
            findings.iter().all(|f| f.code != "MDATRON-E0112"),
            "a marker line inside a code fence is not resolved; got {findings:?}"
        );
    }

    // RED GATE (#147): opt-in — a route WITHOUT marker_rules gets no marker findings.
    #[test]
    fn markers_are_per_route_opt_in() {
        let proj = marker_project(
            "marker-optout",
            "Provenance: No Such Slice\n",
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n",
        );
        let findings = verify(&VerifyConfig::from_project(&proj.0).unwrap()).unwrap();
        assert!(
            findings.iter().all(|f| f.code != "MDATRON-E0112"),
            "no marker_rules, no marker findings; got {findings:?}"
        );
    }

    // RED GATE (#147): the marker family reports active exactly when a route
    // supplies marker_rules (falsifiable audit signal).
    #[test]
    fn marker_family_activity_tracks_opt_in() {
        let opted_in = marker_project("marker-active", "prose\n", MARKER_ROUTE_OPTIN);
        assert!(marker_families(&opted_in).marker.is_active());
        let opted_out = marker_project(
            "marker-inactive",
            "prose\n",
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n",
        );
        assert!(!marker_families(&opted_out).marker.is_active());
    }

    // ── adopter code-catalog family (#148, vsdd GH#20 P4) ───────────────────
    // Spine tests (token grammar / severity pending vsdd-cli#27): a cited
    // adopter code resolves against a declared comprehensive catalog, or blocks
    // as E0113. The core scan/resolve logic is unit-tested in codecat.rs; these
    // exercise the load + per-file wiring + family activity end to end.

    // vsdd's live shape (vsdd-cli#27): classes E/W, four digits, the declared
    // legal set. W0070 is deliberately OMITTED so a citation of it orphans —
    // the real `.claude/commands/vsdd-domain-red-team.md` case.
    const CODE_CATALOG_COMPREHENSIVE: &str = r#"mdatron_format_version: 1
catalogs:
  - namespace: "VSDD-"
    comprehensive: true
    codes: ["E0010", "E0016", "E0018", "E0050", "W0010", "W0030", "W0080", "W0180"]
"#;

    fn code_catalog_project(label: &str, body: &str, catalogs_yaml: &str) -> TempProject {
        let proj = TempProject::new(label);
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write(".mdatron/code-catalogs.yaml", catalogs_yaml);
        proj.write("docs/doc.md", body);
        proj
    }

    fn code_catalog_families(proj: &TempProject) -> crate::output::Families {
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let (_f, fam, _v, _n) = run(&cfg, None, None).unwrap();
        fam
    }

    #[test]
    fn orphaned_adopter_code_is_e0113() {
        // The real red-team.md case: prose fires `VSDD-W0070`, not in the catalog.
        let proj = code_catalog_project(
            "codecat-orphan",
            "namespaced-wrong bypass (fires `VSDD-W0070`).\n",
            CODE_CATALOG_COMPREHENSIVE,
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        let f = findings
            .iter()
            .find(|f| f.code == "MDATRON-E0113")
            .unwrap_or_else(|| panic!("expected E0113; got {findings:?}"));
        // Built at runtime so the source carries no literal VSDD-W code (the
        // cross-repo namespace-separation lint, tests/output_format.rs).
        assert!(f
            .quoted
            .iter()
            .any(|q| q.content == format!("VSDD-{}", "W0070")));
    }

    #[test]
    fn declared_adopter_code_is_clean() {
        let proj = code_catalog_project(
            "codecat-ok",
            "Cites VSDD-E0010 and VSDD-W0180, both declared.\n",
            CODE_CATALOG_COMPREHENSIVE,
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).unwrap();
        assert!(
            findings.iter().all(|f| f.code != "MDATRON-E0113"),
            "declared codes resolve clean; got {findings:?}"
        );
    }

    // #154 note: unlike links, a code token inside inline code is NOT masked —
    // adopters routinely format a real citation as `VSDD-W0070` (vsdd's live
    // orphan is backticked). The orphaned_adopter_code_is_e0113 fixture (which
    // backticks its token) is the regression guard for that.

    #[test]
    fn code_catalog_family_activity_tracks_data() {
        let with = code_catalog_project("codecat-active", "prose\n", CODE_CATALOG_COMPREHENSIVE);
        assert!(code_catalog_families(&with).code_catalog.is_active());
        // No code-catalogs.yaml -> inactive.
        let without = TempProject::new("codecat-inactive");
        without.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        without.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        without.write("docs/doc.md", "prose\n");
        assert!(!code_catalog_families(&without).code_catalog.is_active());
    }

    // RED GATE (DEF5, #131): code-catalogs.yaml is BORN VERSIONED in 0.6.0 — a
    // file omitting `mdatron_format_version` is a loud config error end-to-end
    // (the required-on-new-files leg; the two-pass probe surfaces it legibly
    // before the strict parse).
    #[test]
    fn code_catalog_without_format_version_is_a_loud_error() {
        let proj = code_catalog_project(
            "codecat-noversion",
            "prose\n",
            "catalogs:\n  - namespace: \"VSDD-\"\n    codes: [\"E0001\"]\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let err = format!("{}", verify(&cfg).unwrap_err());
        assert!(
            err.contains("mdatron_format_version") && err.contains("must declare"),
            "a versioned file must declare its format version; got {err}"
        );
    }

    // ── section-structural family (#157, vsdd GH#20 P5 / GH#29) ─────────────
    // Rules pinned to vsdd's live build-plan shape: count of open-phase H3s in
    // `## Requirements` (>= 1, an empty section is the retire trigger) and slice
    // ids disjoint between `## Requirements` (H3 headings) and `## Completed
    // phases` (bullet leads). Core logic is unit-tested in section.rs.

    // Route-attached (#34): the section rules live on the route that claims
    // `plan/**/*.md`, scoped by its `files` glob — not a standalone corpus-wide
    // file. This is the vsdd GH#34 fix in situ.
    const SECTION_ROUTES: &str = r###"routes:
- files: "plan/**/*.md"
  governed_by: GOVERNING.md
  section_rules:
    - section: "## Requirements"
      element: h3
      match: '^### Phase \d+: .*\((parallel|sequential)\)$'
      count: ">= 1"
    - disjoint:
        - section: "## Requirements"
          id_from: h3-heading
          id_pattern: 'Slice (\d+)'
        - section: "## Completed phases"
          id_from: bullet-lead
          id_pattern: 'Slice (\d+)'
"###;

    fn section_project(label: &str, plan: &str) -> TempProject {
        let proj = TempProject::new(label);
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"plan/**/*.md\"\n",
        );
        proj.write("GOVERNING.md", "# gov\n");
        proj.write(".mdatron/routes.yaml", SECTION_ROUTES);
        proj.write("plan/build-plan.md", plan);
        proj
    }

    // The live clean case: 5 open phases (>= 1), open ids {2,4,5,6,7} disjoint
    // from completed {1,3}. Phase 2's BODY mentions `Slice 3` — a full-span scan
    // would false-overlap; heading-scoped extraction must not. This test is the
    // trap regression guard.
    const CLEAN_PLAN: &str = "# Build plan\n\n## Requirements\n\n### Phase 2: Slice 2 (sequential)\nProvenance: Slice 3 — Install\n### Phase 3: Slice 4 (sequential)\n### Phase 4: Slice 5 (sequential)\n### Phase 5: Slice 6 (sequential)\n### Phase 6: Slice 7 (sequential)\n\n## Completed phases\n\n- **Slice 1 tracker join (complete):** done\n- **Slice 3 static half (complete):** done\n- **The engine bullet:** no id\n";

    #[test]
    fn section_rules_clean_on_vsdd_build_plan_shape() {
        let proj = section_project("section-clean", CLEAN_PLAN);
        let findings = verify(&VerifyConfig::from_project(&proj.0).unwrap()).unwrap();
        assert!(
            findings
                .iter()
                .all(|f| f.code != "MDATRON-E0120" && f.code != "MDATRON-E0121"),
            "5 open phases (>= 1) + disjoint ids (no false Slice 3 overlap) is clean; got {findings:?}"
        );
        assert!(section_families(&proj).section.is_active());
    }

    fn section_families(proj: &TempProject) -> crate::output::Families {
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let (_f, fam, _v, _n) = run(&cfg, None, None).unwrap();
        fam
    }

    #[test]
    fn empty_requirements_section_fires_e0120() {
        let plan = "# Build plan\n\n## Requirements\n\nall done, nothing open.\n\n## Completed phases\n\n- **Slice 1 done (complete):** x\n";
        let proj = section_project("section-empty", plan);
        let findings = verify(&VerifyConfig::from_project(&proj.0).unwrap()).unwrap();
        assert!(
            findings.iter().any(|f| f.code == "MDATRON-E0120"),
            "0 open-phase H3s violates count >= 1; got {findings:?}"
        );
    }

    #[test]
    fn slice_open_and_complete_fires_e0121() {
        let plan = "# Build plan\n\n## Requirements\n\n### Phase 2: Slice 2 (sequential)\n\n## Completed phases\n\n- **Slice 2 also done (complete):** oops\n";
        let proj = section_project("section-overlap", plan);
        let findings = verify(&VerifyConfig::from_project(&proj.0).unwrap()).unwrap();
        assert!(
            findings.iter().any(|f| f.code == "MDATRON-E0121"),
            "Slice 2 open AND complete is not disjoint; got {findings:?}"
        );
    }

    // RED GATE (#34): a section rule is scoped by ITS ROUTE'S `files` glob — it
    // must NOT fire on a walked file a *different* route claims. This is the vsdd
    // GH#34 repro: an unrelated `## Requirements` elsewhere in the corpus fired
    // E0120 on 80 files under the old standalone (corpus-wide) form; route-
    // attachment scopes the rule so it cannot.
    #[test]
    fn section_rules_do_not_fire_outside_their_route_scope() {
        let proj = TempProject::new("section-scope");
        proj.write(
            ".mdatron/schemas/phase-primer.json",
            minimal_phase_primer_schema(),
        );
        proj.write(".mdatron/config.yaml", "file_globs:\n  - \"**/*.md\"\n");
        proj.write("GOVERNING.md", "# gov\n");
        // Route A claims plan/** and carries the section rules; route B claims
        // notes/** with NONE — so a `## Requirements` under notes/ is out of scope.
        proj.write(
            ".mdatron/routes.yaml",
            &format!("{SECTION_ROUTES}- files: \"notes/**/*.md\"\n  governed_by: GOVERNING.md\n"),
        );
        // An unrelated file with a `## Requirements` heading and zero open phases:
        // corpus-wide it would fire E0120; scoped to plan/** it must stay silent.
        proj.write(
            "notes/other.md",
            "# Notes\n\n## Requirements\n\nnothing open here.\n",
        );
        proj.write("plan/build-plan.md", CLEAN_PLAN);
        let findings = verify(&VerifyConfig::from_project(&proj.0).unwrap()).unwrap();
        assert!(
            findings
                .iter()
                .all(|f| f.code != "MDATRON-E0120" && f.code != "MDATRON-E0121"),
            "the section rule is scoped to plan/**; notes/other.md must not misfire; got {findings:?}"
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
        // The registries are index data sources (selected by $.frontmatter.
        // entries), not typed docs — no schema_class, so no W0045.
        proj.write(
            "registry/domains.md",
            "---\nentries:\n- id: se\n- id: qe\n---\n",
        );
        proj.write("registry/others.md", "---\nentries:\n- id: x\n---\n");
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
        let (findings, _fam, _vis, _n) = run(&cfg, None, Some(&mutate)).unwrap();
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

    // ── #103 unification red gate ────────────────────────────────────────────
    //
    // Every input class is served from the ONE immutable snapshot sealed at the
    // capture-complete seam (DESIGN.md § Verification is fast: "reads its inputs
    // once ... all checks run against snapshot bytes"). Each fixture mutates or
    // deletes an input AT THE SEAM and asserts the run reports capture-time
    // state; a follow-up fresh run proves the mutation landed (no vacuous pass).

    // A pin certifies the bytes the run validated, not whatever is on disk when
    // pin::check happens to execute.
    #[test]
    fn pin_verdict_reflects_capture_time_bytes() {
        use sha2::{Digest, Sha256};
        let proj = TempProject::new("pin-seam");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        let content = "---\nschema_class: doc\n---\n# x\n";
        proj.write("docs/x.md", content);
        proj.write("docs/gov.md", "# governing\n");
        let sha = format!("{:x}", Sha256::digest(content.as_bytes()));
        proj.write(
            ".mdatron/pins.yaml",
            &format!("pins:\n- governing: docs/gov.md\n  file: docs/x.md\n  sha256: {sha}\n"),
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();

        let doc = proj.0.join("docs/x.md");
        let mutate = || {
            std::fs::write(&doc, "---\nschema_class: doc\n---\n# mutated\n").unwrap();
        };
        let (findings, _fam, _vis, _n) = run(&cfg, None, Some(&mutate)).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-E0061"),
            0,
            "the pin verdict must certify the capture-time bytes (which match the \
             recorded sha), not the post-seam mutation; got {findings:?}"
        );
        let after = verify_report(&cfg).unwrap().findings;
        assert_eq!(
            codes_of(&after, "MDATRON-E0061"),
            1,
            "a later run captures fresh and sees the stale pin"
        );
    }

    // A citation's line-range check runs against the capture-time target.
    #[test]
    fn cite_range_checked_against_capture_time_target() {
        let proj = cite_project("cite-seam-range", "Per src-file.rs:5 this holds.\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let target = proj.0.join("src-file.rs");
        let mutate = || {
            std::fs::write(&target, "line1\nline2\n").unwrap();
        };
        let (findings, _fam, _vis, _n) = run(&cfg, None, Some(&mutate)).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-E0101"),
            0,
            "the range check must use capture-time bytes (5 lines); got {findings:?}"
        );
        let after = verify_report(&cfg).unwrap().findings;
        assert_eq!(
            codes_of(&after, "MDATRON-E0101"),
            1,
            "a later run captures fresh and sees the truncated target"
        );
    }

    // Read-once, observably: a citation target DELETED after capture still
    // resolves from the snapshot — the run never re-touches the filesystem.
    #[test]
    fn cite_target_deleted_at_seam_still_resolves() {
        let proj = cite_project("cite-seam-delete", "Per src-file.rs:2 this holds.\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let target = proj.0.join("src-file.rs");
        let mutate = || {
            std::fs::remove_file(&target).unwrap();
        };
        let (findings, _fam, _vis, _n) = run(&cfg, None, Some(&mutate)).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-E0100"),
            0,
            "a target captured before the seam is served from the snapshot; got {findings:?}"
        );
        let after = verify_report(&cfg).unwrap().findings;
        assert_eq!(
            codes_of(&after, "MDATRON-E0100"),
            1,
            "a later run captures fresh and reports the dead citation"
        );
    }

    // A link's anchor resolution runs against the capture-time target headings.
    #[test]
    fn link_anchor_resolved_against_capture_time_target() {
        let proj = TempProject::new("link-seam");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("GOVERNING.md", "# gov\n");
        proj.write(
            ".mdatron/routes.yaml",
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n  links: true\n",
        );
        proj.write("docs/a.md", "See [t](t2.md#section-one).\n");
        proj.write("docs/t2.md", "# Section One\n\nbody\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();

        let target = proj.0.join("docs/t2.md");
        let mutate = || {
            std::fs::write(&target, "# Renamed\n\nbody\n").unwrap();
        };
        let (findings, _fam, _vis, _n) = run(&cfg, None, Some(&mutate)).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-E0111"),
            0,
            "anchor resolution must use capture-time headings; got {findings:?}"
        );
        let after = verify_report(&cfg).unwrap().findings;
        assert_eq!(
            codes_of(&after, "MDATRON-E0111"),
            1,
            "a later run captures fresh and sees the dead anchor"
        );
    }

    // A marker rule's member set comes from the capture-time target_doc.
    #[test]
    fn marker_members_resolved_against_capture_time_target() {
        let proj = marker_project(
            "marker-seam",
            "Provenance: Slice 1 — Live self-governance: the tracker join\n",
            MARKER_ROUTE_OPTIN,
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let target = proj.0.join("refs/contract.md");
        let mutate = || {
            std::fs::write(
                &target,
                "# Contract\n\n## Decomposition (phase 1c)\n\n- **Other.** x\n",
            )
            .unwrap();
        };
        let (findings, _fam, _vis, _n) = run(&cfg, None, Some(&mutate)).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-E0112"),
            0,
            "marker resolution must use the capture-time member set; got {findings:?}"
        );
        let after = verify_report(&cfg).unwrap().findings;
        assert_eq!(
            codes_of(&after, "MDATRON-E0112"),
            1,
            "a later run captures fresh and sees the dead reference"
        );
    }

    // The per-file input bound covers EVERY captured input, not only governed
    // bodies: an oversized index source is refused loudly (memory-DoS lever —
    // today it is read unbounded).
    #[test]
    fn oversized_index_source_trips_the_per_file_bound() {
        let proj = TempProject::new("index-bound");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        let mut big = String::from("k: v\n");
        while big.len() <= MAX_FILE_BYTES {
            big.push_str("# padding comment line to grow the file past the cap\n");
        }
        proj.write("registry/big.yaml", &big);
        proj.write(
            ".mdatron/patterns/p.yaml",
            "mdatron_dsl_version: 1\npattern:\n  id: bound\n  keys:\n    - name: k\n      source: registry/big.yaml\n      select: $\n      indexed_by: $key\n  rules:\n    - id: r\n      context: no-such-class\n      assert: \"true\"\n      code: T-E0001\n      message: never\n",
        );
        proj.write("docs/d.md", "# plain\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let err = verify(&cfg).unwrap_err();
        match err {
            VerifyError::BoundExceeded { ref bound, .. } => {
                assert_eq!(bound, "max-input-size-per-file", "got {err:?}")
            }
            other => {
                panic!("an oversized index source must trip the per-file bound; got {other:?}")
            }
        }
    }

    // An oversized PROSE-scoped target (a citation) must NOT abort the run —
    // a prose line is not a lever over the whole tree (#103 phase-3 A-1) —
    // and the skipped range check must be LOUD (W0048, phase-3 R2S-2), so an
    // out-of-range citation into a big file cannot silently satisfy a gate.
    #[test]
    fn oversized_cite_target_degrades_loudly_instead_of_aborting() {
        let proj = cite_project("cite-bound", "Per src-file.rs:1 this holds.\n");
        let mut big = String::new();
        while big.len() <= MAX_FILE_BYTES {
            big.push_str("padding line to grow the cited target past the per-file cap\n");
        }
        proj.write("src-file.rs", &big);
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).expect("a prose-named oversized target must not abort");
        assert_eq!(
            codes_of(&findings, "MDATRON-E0100") + codes_of(&findings, "MDATRON-E0101"),
            0,
            "existence verified, no dead/range finding fabricated; got {findings:?}"
        );
        assert_eq!(
            codes_of(&findings, "MDATRON-W0048"),
            1,
            "the skipped range check is loud; got {findings:?}"
        );
    }

    // A CONFIG-scoped oversized target (a pinned file) keeps the loud
    // declared-bounds abort — the jurisdiction declared it (#103 phase-3 A-1).
    #[test]
    fn oversized_pin_target_trips_the_per_file_bound() {
        let proj = TempProject::new("pin-bound");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("docs/gov.md", "# governing\n");
        let mut big = String::new();
        while big.len() <= MAX_FILE_BYTES {
            big.push_str("padding line to grow the pinned file past the per-file cap\n");
        }
        proj.write("pinned.txt", &big);
        proj.write(
            ".mdatron/pins.yaml",
            "pins:\n- governing: docs/gov.md\n  file: pinned.txt\n  sha256: \
             0000000000000000000000000000000000000000000000000000000000000000\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let err = verify(&cfg).unwrap_err();
        match err {
            VerifyError::BoundExceeded { ref bound, .. } => {
                assert_eq!(bound, "max-input-size-per-file", "got {err:?}")
            }
            other => panic!("an oversized pinned file must trip the per-file bound; got {other:?}"),
        }
    }

    // Phase-3 S-2: a governed body whose OPEN is refused (permission denied)
    // is a per-file E0003, never a whole-run abort — one chmod'd file must not
    // blind every other check.
    #[cfg(unix)]
    #[test]
    fn open_refused_governed_body_is_a_finding_not_an_abort() {
        use std::os::unix::fs::PermissionsExt;
        let proj = TempProject::new("perm-denied");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("docs/good.md", "# fine\n");
        proj.write("docs/bad.md", "# unreachable\n");
        std::fs::set_permissions(
            proj.0.join("docs/bad.md"),
            std::fs::Permissions::from_mode(0o000),
        )
        .unwrap();
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let (findings, _fam, _vis, files_checked) =
            run(&cfg, None, None).expect("one unreadable file must not abort the run");
        std::fs::set_permissions(
            proj.0.join("docs/bad.md"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-E0003"),
            1,
            "open refusal is a localized finding; got {findings:?}"
        );
        assert_eq!(files_checked, 1, "the good file is still verified");
    }

    // Phase-3 S-1 (the reproduced hang): a FIFO governed body and a FIFO cite
    // target are both refused deterministically — never a blocking open.
    #[cfg(unix)]
    #[test]
    fn fifo_inputs_are_refused_not_hung() {
        use std::os::unix::ffi::OsStrExt;
        let proj = cite_project("fifo", "Per pipe-target.rs:1 this holds.\n");
        let fifo_target = proj.0.join("pipe-target.rs");
        let c_target = std::ffi::CString::new(fifo_target.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_target.as_ptr(), 0o644) }, 0);
        let fifo_body = proj.0.join("docs/pipe.md");
        let c_body = std::ffi::CString::new(fifo_body.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_body.as_ptr(), 0o644) }, 0);
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _ = tx.send(run(&cfg, None, None).map(|(f, _, _, _)| f));
        });
        let findings = match rx.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(result) => result.expect("FIFO inputs must not abort the run"),
            Err(_) => panic!("a FIFO input hung the run (the S-1 denial of verification)"),
        };
        let _ = worker.join();
        // The FIFO governed body is per-file unverifiable (E0003); the FIFO
        // cite target is existence-verified-unverifiable (no dead-citation).
        assert_eq!(codes_of(&findings, "MDATRON-E0003"), 1, "got {findings:?}");
        assert_eq!(codes_of(&findings, "MDATRON-E0100"), 0, "got {findings:?}");
    }

    // Shared fixture for the discovery-gating pair: a route whose marker rule
    // names an OVERSIZED target_doc (config-scoped: capturing it escalates to
    // the whole-run bound abort), reachable only through docs/plan.md.
    fn marker_big_target_project(label: &str, plan_frontmatter: &str) -> TempProject {
        let proj = TempProject::new(label);
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("GOVERNING.md", "# gov\n");
        proj.write(
            ".mdatron/routes.yaml",
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n  marker_rules:\n    - pattern: \"^Provenance: (.+)$\"\n      element: list-item-bold-name\n      target_doc: refs/big.md\n",
        );
        let mut big = String::new();
        while big.len() <= MAX_FILE_BYTES {
            big.push_str("padding line to grow the marker target past the per-file cap\n");
        }
        proj.write("refs/big.md", &big);
        proj.write(
            "docs/plan.md",
            &format!("{plan_frontmatter}Provenance: Slice 1\n"),
        );
        proj
    }

    // Phase-3 I-1 (re-gated per round-2 R2I-2, red against a reverted
    // `body_offset_of` Err arm): a file whose frontmatter fails to parse gets
    // E0001 and NO cross-file checks — so discovery must not capture its
    // targets either. The target here is CONFIG-scoped (a marker target_doc),
    // so an ungated discovery would escalate its oversize to a whole-run
    // abort — this test is red without the gating, unlike a prose target
    // (whose degrade posture would mask the regression).
    #[test]
    fn parse_failed_file_targets_are_not_captured() {
        let proj =
            marker_big_target_project("parse-err-discovery", "---\n: not: [valid yaml\n---\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings =
            verify(&cfg).expect("a target reachable only from a parse-failed file must not abort");
        assert_eq!(
            codes_of(&findings, "MDATRON-E0001"),
            1,
            "the parse failure itself reports; got {findings:?}"
        );
    }

    // The complement pinning the CONFIG-scoped escalation itself (phase-3
    // R2I-6): the same oversized marker target_doc, reached through a VALID
    // file, is the loud declared-bounds abort.
    #[test]
    fn oversized_marker_target_trips_the_per_file_bound() {
        let proj = marker_big_target_project("marker-bound", "");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let err = verify(&cfg).unwrap_err();
        match err {
            VerifyError::BoundExceeded { ref bound, .. } => {
                assert_eq!(bound, "max-input-size-per-file", "got {err:?}")
            }
            other => {
                panic!("an oversized marker target_doc must trip the per-file bound; got {other:?}")
            }
        }
    }

    // Phase-3 R2I-6: the link family's prose-scoped oversize posture — the
    // run survives, the skipped anchor check is LOUD (W0048), and no dead
    // link/anchor is fabricated.
    #[test]
    fn oversized_link_target_degrades_loudly() {
        let proj = TempProject::new("link-bound");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("GOVERNING.md", "# gov\n");
        proj.write(
            ".mdatron/routes.yaml",
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n  links: true\n",
        );
        // One fragment-bearing link (anchor check genuinely skipped -> W0048)
        // and one fragment-less link (fully verified by existence — NO W0048,
        // phase-3 R3-3).
        proj.write(
            "docs/a.md",
            "See [t](../big.md#section-one) and [u](../big.md).\n",
        );
        let mut big = String::new();
        while big.len() <= MAX_FILE_BYTES {
            big.push_str("padding line to grow the link target past the per-file cap\n");
        }
        proj.write("big.md", &big);
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).expect("a prose-named oversized link target must not abort");
        assert_eq!(codes_of(&findings, "MDATRON-E0110"), 0, "{findings:?}");
        assert_eq!(codes_of(&findings, "MDATRON-E0111"), 0, "{findings:?}");
        assert_eq!(
            codes_of(&findings, "MDATRON-W0048"),
            1,
            "exactly the skipped anchor check warns — the fragment-less link \
             is fully verified and stays quiet; got {findings:?}"
        );
    }

    // Phase-3 R3-1 (the reproduced ordering lever): prose captures must never
    // cause a config-scoped capture to inherit an aggregate breach — config-
    // scoped targets capture FIRST. Nine sub-cap citation targets tip the
    // aggregate; the marker target_doc must already be captured by then, so
    // the run SURVIVES, the marker check resolves, and only the prose
    // degradations warn. Red with prose-before-config ordering (the marker
    // capture then aborts the whole run).
    #[test]
    fn prose_aggregate_consumption_cannot_abort_config_captures() {
        let proj = TempProject::new("capture-order");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("GOVERNING.md", "# gov\n");
        proj.write(
            ".mdatron/routes.yaml",
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n  citations: true\n  marker_rules:\n    - pattern: \"^Provenance: (.+)$\"\n      element: list-item-bold-name\n      target_doc: refs/contract.md\n",
        );
        // A 4 MB marker target (config-scoped) carrying the referenced member.
        let mut contract = String::from("- **Slice 1.** the slice\n");
        while contract.len() < 4_000_000 {
            contract.push_str("padding prose line for the contract body\n");
        }
        proj.write("refs/contract.md", &contract);
        // Nine sub-cap citation targets whose combined size tips the aggregate.
        let big = "x".repeat(7_999_999);
        let mut plan = String::from("Provenance: Slice 1\n");
        for i in 0..9 {
            proj.write(&format!("big{i}.rs"), &big);
            plan.push_str(&format!("Per big{i}.rs:1 this holds.\n"));
        }
        proj.write("docs/plan.md", &plan);
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg)
            .expect("prose aggregate consumption must not abort a config-scoped capture");
        assert_eq!(
            codes_of(&findings, "MDATRON-E0112"),
            0,
            "the marker check resolves from its (config-first) capture; got {findings:?}"
        );
        assert!(
            codes_of(&findings, "MDATRON-W0048") >= 1,
            "the degraded prose references are loud; got {findings:?}"
        );
    }

    // ── #92 sub-lane D: declared bounds catalog ──────────────────────────────

    // DESIGN:148 "concurrent-invocation-count exceedance (N+1 overlapping
    // runs)": with every declared slot held, a run reports the bound; freeing
    // one slot lets it proceed.
    #[test]
    fn concurrent_invocation_count_is_bounded() {
        let proj = TempProject::new("slots");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("docs/d.md", "# plain\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();

        let limit = crate::limits::SHIPPED.concurrent_invocations;
        let mut held = Vec::new();
        for _ in 0..limit {
            match crate::limits::acquire_invocation_slot(&proj.0, limit).unwrap() {
                crate::limits::SlotOutcome::Acquired(slot) => held.push(slot),
                crate::limits::SlotOutcome::Busy { .. } => panic!("slots free under the limit"),
            }
        }
        let err = verify(&cfg).unwrap_err();
        match err {
            VerifyError::BoundExceeded { ref bound, .. } => {
                assert_eq!(bound, "concurrent-invocation-count", "got {err:?}")
            }
            other => panic!("the N+1st invocation must report the bound; got {other:?}"),
        }
        held.clear();
        assert!(
            verify(&cfg).is_ok(),
            "with slots free again the run proceeds"
        );
    }

    // Phase-3 R4-1: when the pool is busy AND the slot directory was just
    // repaired from a permissive mode, the diagnostic must name the possible
    // foreign hold (the honesty guarantee R3-1 added) — not the generic
    // "N concurrent runs" message. Reproduces the real window: foreign
    // holders take the slots while the dir is world-accessible, then the
    // victim's run repairs it but the locks survive (chmod revokes none).
    #[cfg(unix)]
    #[test]
    fn busy_after_repair_names_the_foreign_hold() {
        use std::os::unix::fs::PermissionsExt;
        let proj = TempProject::new("repaired-busy");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("docs/d.md", "# plain\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();

        // Hold every slot (this creates + repairs the dir to 0700).
        let limit = crate::limits::SHIPPED.concurrent_invocations;
        let mut held = Vec::new();
        for _ in 0..limit {
            match crate::limits::acquire_invocation_slot(&proj.0, limit).unwrap() {
                crate::limits::SlotOutcome::Acquired(slot) => held.push(slot),
                crate::limits::SlotOutcome::Busy { .. } => panic!("slots free under the limit"),
            }
        }
        // Re-permission the (still-held) dir to simulate the window a foreign
        // process grabbed the slots through; the held flocks survive the mode
        // change.
        let (dir, _) = crate::limits::slot_dir(&proj.0).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        let err = verify(&cfg).unwrap_err();
        match err {
            VerifyError::BoundExceeded { ref detail, .. } => assert!(
                detail.contains("world-accessible and repaired"),
                "the busy-after-repair diagnostic must name the foreign-hold \
                 possibility; got {detail:?}"
            ),
            other => panic!("expected the bound; got {other:?}"),
        }
        held.clear();
    }

    // DESIGN:148 "seeded alias-bomb": the YAML repetition guard refuses the
    // expansion as a localized E0001 parse finding — bounded time and memory,
    // never a whole-run abort, never an OOM. (The guard rides the parser; this
    // fixture pins that the engine's posture over it is the per-file one.)
    #[test]
    fn frontmatter_alias_bomb_is_a_bounded_per_file_finding() {
        let proj = TempProject::new("alias-bomb");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("docs/good.md", "# fine\n");
        let mut bomb = String::from("---\na0: &a0 [\"AAAAAAAA\",\"AAAAAAAA\",\"AAAAAAAA\",\"AAAAAAAA\",\"AAAAAAAA\",\"AAAAAAAA\",\"AAAAAAAA\",\"AAAAAAAA\"]\n");
        for i in 1..10 {
            let refs = (0..8)
                .map(|_| format!("*a{}", i - 1))
                .collect::<Vec<_>>()
                .join(",");
            bomb.push_str(&format!("a{i}: &a{i} [{refs}]\n"));
        }
        bomb.push_str("---\nbody\n");
        proj.write("docs/bomb.md", &bomb);
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let (findings, _fam, _vis, files_checked) =
            run(&cfg, None, None).expect("an alias bomb must not abort the run");
        assert_eq!(
            codes_of(&findings, "MDATRON-E0001"),
            1,
            "the refused expansion is a localized parse finding; got {findings:?}"
        );
        // Both files count as checked: the bomb ENTERED verify_file and was
        // validated-as-malformed (E0001), unlike an E0003 unreadable body
        // which never enters. The point pinned here is the sibling verifying
        // and the run surviving.
        assert_eq!(files_checked, 2, "both files ran the per-file checks");
    }

    // DESIGN:148 "seeded depth-bomb" on an INDEX source (the #103-routed
    // observation): the parser's recursion guard refuses it as a clean index
    // build error — bounded, no stack overflow.
    #[test]
    fn index_source_depth_bomb_is_a_bounded_index_error() {
        let proj = TempProject::new("index-depth-bomb");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        let deep = format!("k: {}{}\n", "[".repeat(3000), "]".repeat(3000));
        proj.write("registry/deep.yaml", &deep);
        proj.write(
            ".mdatron/patterns/p.yaml",
            "mdatron_dsl_version: 1\npattern:\n  id: depth\n  keys:\n    - name: k\n      source: registry/deep.yaml\n      select: $\n      indexed_by: $key\n  rules:\n    - id: r\n      context: no-such-class\n      assert: \"true\"\n      code: T-E0001\n      message: never\n",
        );
        proj.write("docs/d.md", "# plain\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let err = verify(&cfg).unwrap_err();
        assert!(
            matches!(err, VerifyError::IndexBuild(IndexError::Parse { .. })),
            "the depth bomb is a clean parse refusal; got {err:?}"
        );
    }

    // DESIGN:147 "a symlink-cycle fixture terminates": a directory symlink
    // cycle under the MAIN file_globs walk terminates promptly, every
    // cycle-path capture refused loudly (E0012) — no unbounded enumeration.
    #[cfg(unix)]
    #[test]
    fn main_walk_symlink_cycle_terminates_with_refusals() {
        let proj = TempProject::new("walk-cycle");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("docs/real.md", "# fine\n");
        std::os::unix::fs::symlink(proj.0.join("docs"), proj.0.join("docs/loop")).unwrap();
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _ = tx.send(verify(&cfg));
        });
        let findings = match rx.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(result) => result.expect("the cycle walk must complete, not abort"),
            Err(_) => panic!("the main walk did not terminate on a symlink cycle"),
        };
        let _ = worker.join();
        assert!(
            codes_of(&findings, "MDATRON-E0012") >= 1,
            "cycle paths are refused loudly; got {findings:?}"
        );
    }

    // ── #92 sub-lane E: concurrency safety ───────────────────────────────────

    // DESIGN:148 "overlapping invocations with a mutation between their starts
    // each report against their own snapshots": run 1 seals its snapshot and
    // parks at the seam; the tree mutates; run 2 starts and completes against
    // the NEW state; run 1 resumes and reports its OWN capture-time state.
    #[test]
    fn overlapping_runs_each_report_their_own_snapshot() {
        let proj = TempProject::new("overlap-own");
        proj.write(
            ".mdatron/schemas/blog.json",
            r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","required":["schema_class","title"],"properties":{"schema_class":{"const":"blog"},"title":{"type":"string"}},"additionalProperties":false}"#,
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        // Valid at run 1's capture.
        proj.write("docs/d.md", "---\nschema_class: blog\ntitle: ok\n---\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let doc = proj.0.join("docs/d.md");

        let (sealed_tx, sealed_rx) = std::sync::mpsc::channel::<()>();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel::<()>();
        let cfg1 = VerifyConfig::from_project(&proj.0).unwrap();
        let run1 = std::thread::spawn(move || {
            let park = move || {
                let _ = sealed_tx.send(());
                let _ = resume_rx.recv();
            };
            run(&cfg1, None, Some(&park)).map(|(f, _, _, _)| f)
        });
        sealed_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("run 1 reaches its seam");
        // Mutate between the starts: run 2 must see this, run 1 must not.
        std::fs::write(&doc, "---\nschema_class: blog\n---\n").unwrap();
        let findings2 = verify(&cfg).expect("run 2 completes during run 1's window");
        assert_eq!(
            codes_of(&findings2, "MDATRON-E0050"),
            1,
            "run 2 captured the mutated (now-invalid) state; got {findings2:?}"
        );
        resume_tx.send(()).unwrap();
        let findings1 = run1.join().unwrap().expect("run 1 completes");
        assert!(
            findings1.is_empty(),
            "run 1 reports its own (pre-mutation) snapshot; got {findings1:?}"
        );
    }

    // DESIGN:148 "overlapping invocations producing results that differ from
    // serial runs" is a falsification: on a STATIC tree, an overlapped run
    // pair equals the serial results exactly.
    #[test]
    fn overlapping_runs_on_a_static_tree_equal_serial() {
        let proj = TempProject::new("overlap-serial");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("docs/a.md", "---\nschema_class: doc\n---\n# a\n");
        proj.write("docs/b.md", "# plain\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let serial = verify(&cfg).unwrap();

        let (sealed_tx, sealed_rx) = std::sync::mpsc::channel::<()>();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel::<()>();
        let cfg1 = VerifyConfig::from_project(&proj.0).unwrap();
        let run1 = std::thread::spawn(move || {
            let park = move || {
                let _ = sealed_tx.send(());
                let _ = resume_rx.recv();
            };
            run(&cfg1, None, Some(&park)).map(|(f, _, _, _)| f)
        });
        sealed_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("run 1 reaches its seam");
        let overlapped = verify(&cfg).expect("run 2 completes inside run 1's window");
        resume_tx.send(()).unwrap();
        let findings1 = run1.join().unwrap().expect("run 1 completes");

        let key = |f: &Finding| (f.location.file.clone(), f.code.clone(), f.location.line);
        let mut s: Vec<_> = serial.iter().map(key).collect();
        let mut o: Vec<_> = overlapped.iter().map(key).collect();
        let mut f1: Vec<_> = findings1.iter().map(key).collect();
        s.sort();
        o.sort();
        f1.sort();
        assert_eq!(s, o, "the overlapped run equals serial");
        assert_eq!(s, f1, "the parked run equals serial");
    }

    // Phase-3 R2I-6: the remaining snapshot-miss arms (link, marker, pin) all
    // report the E0080 engine defect, never a family finding — probed like the
    // cite arm, directly against an empty snapshot.
    #[test]
    fn snapshot_miss_probes_for_link_marker_pin() {
        let empty = crate::snapshot::Snapshot::new(64, 4096);

        let mut findings = Vec::new();
        crate::link::check_file(
            &empty,
            Path::new("/no-root"),
            Path::new("docs/a.md"),
            "See [t](t2.md#a).\n",
            0,
            false,
            &mut findings,
        );
        assert_eq!(codes_of(&findings, "MDATRON-E0080"), 1, "{findings:?}");
        assert_eq!(codes_of(&findings, "MDATRON-E0110"), 0, "{findings:?}");

        let rule = crate::route::MarkerRule {
            pattern: regex_lite::Regex::new("^Provenance: (.+)$").unwrap(),
            element: crate::route::ElementClass::ListItemBoldName,
            target_doc: "refs/contract.md".into(),
            target_section: None,
        };
        let mut findings = Vec::new();
        crate::marker::check_file(
            &empty,
            Path::new("docs/plan.md"),
            "Provenance: Slice 1\n",
            0,
            &[&rule],
            &mut findings,
        );
        assert_eq!(codes_of(&findings, "MDATRON-E0080"), 1, "{findings:?}");
        assert_eq!(codes_of(&findings, "MDATRON-E0112"), 0, "{findings:?}");

        let pin = crate::pin::Pin {
            governing: "docs/gov.md".into(),
            file: "docs/x.md".into(),
            section: None,
            sha256: "00".repeat(32),
        };
        let mut findings = Vec::new();
        crate::pin::check(Path::new("/no-root"), &[pin], &empty, &mut findings);
        assert_eq!(codes_of(&findings, "MDATRON-E0080"), 1, "{findings:?}");
        assert_eq!(codes_of(&findings, "MDATRON-E0062"), 0, "{findings:?}");
    }

    // Phase-3 I-2: pipeline-level pinning of the capture-state -> IndexError
    // taxonomy build_from_parts applies (the path production executes).
    #[cfg(unix)]
    #[test]
    fn symlinked_index_source_aborts_via_build_from_parts() {
        let proj = TempProject::new("index-symlink");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("real.yaml", "k: v\n");
        std::os::unix::fs::symlink(proj.0.join("real.yaml"), proj.0.join("alias.yaml")).unwrap();
        proj.write(
            ".mdatron/patterns/p.yaml",
            "mdatron_dsl_version: 1\npattern:\n  id: sym\n  keys:\n    - name: k\n      source: alias.yaml\n      select: $\n      indexed_by: $key\n  rules:\n    - id: r\n      context: no-such-class\n      assert: \"true\"\n      code: T-E0001\n      message: never\n",
        );
        proj.write("docs/d.md", "# plain\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let err = verify(&cfg).unwrap_err();
        match err {
            VerifyError::IndexBuild(IndexError::SymlinkRefused { .. }) => {}
            other => panic!("a symlinked index source is refused as such; got {other:?}"),
        }
    }

    // #162: a non-UTF8 index source no longer ABORTS the run — it degrades to
    // an empty contribution plus a loud W0049 (the index family is inert for
    // that source, not silent), and the run completes. Rules referencing the
    // now-empty key surface their own findings.
    #[test]
    fn non_utf8_index_source_degrades_to_w0049_not_abort() {
        let proj = TempProject::new("index-raw");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        std::fs::write(proj.0.join("data.yaml"), [0xFF, 0xFE, b'k']).unwrap();
        proj.write(
            ".mdatron/patterns/p.yaml",
            "mdatron_dsl_version: 1\npattern:\n  id: raw\n  keys:\n    - name: k\n      source: data.yaml\n      select: $\n      indexed_by: $key\n  rules:\n    - id: r\n      context: no-such-class\n      assert: \"true\"\n      code: T-E0001\n      message: never\n",
        );
        proj.write("docs/d.md", "# plain\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).expect("a non-UTF8 index source must not abort the run");
        let f = findings
            .iter()
            .find(|f| f.code == "MDATRON-W0049")
            .unwrap_or_else(|| panic!("expected W0049; got {findings:?}"));
        assert!(
            f.quoted.iter().any(|q| q.content == "k")
                && f.quoted.iter().any(|q| q.content.contains("data.yaml")),
            "the finding names the key and the source; got {:?}",
            f.quoted
        );
    }

    // #162 phase-3 F-1: an adopter-controlled index NAME carrying control
    // bytes must NOT reach the engine-authored message raw — it rides only in
    // the (render-escaped) quoted regions. Pins the marking discipline against
    // an ANSI/splitter injection into the TTY/compact agent-facing views.
    #[test]
    fn w0049_message_never_carries_raw_adopter_bytes() {
        let proj = TempProject::new("index-inject");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        std::fs::write(proj.0.join("data.yaml"), [0xFF, 0xFE]).unwrap();
        // The index name embeds an ESC + a newline.
        proj.write(
            ".mdatron/patterns/p.yaml",
            "mdatron_dsl_version: 1\npattern:\n  id: inj\n  keys:\n    - name: \"boom\\u001b[31m\\nx\"\n      source: data.yaml\n      select: $\n      indexed_by: $key\n  rules:\n    - id: r\n      context: no-such-class\n      assert: \"true\"\n      code: T-E0001\n      message: never\n",
        );
        proj.write("docs/d.md", "# plain\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).expect("must not abort");
        let f = findings
            .iter()
            .find(|f| f.code == "MDATRON-W0049")
            .expect("W0049 present");
        assert!(
            !f.message.contains('\u{1b}') && !f.message.contains('\n'),
            "no raw control byte may reach the engine-authored message: {:?}",
            f.message
        );
        // The name IS carried — in a quoted region (escaped at render).
        assert!(
            f.quoted.iter().any(|q| q.label == "index"),
            "the index name rides in a quoted region"
        );
    }

    // #162 phase-3 coverage: a DIRECTORY as an index source is a distinct
    // capture state (OpenedUnreadable, not OpenIo) — it degrades too.
    #[test]
    fn directory_index_source_degrades_to_w0049() {
        let proj = TempProject::new("index-dir");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        std::fs::create_dir_all(proj.0.join("adir")).unwrap();
        proj.write(
            ".mdatron/patterns/p.yaml",
            "mdatron_dsl_version: 1\npattern:\n  id: dir\n  keys:\n    - name: k\n      source: adir\n      select: $\n      indexed_by: $key\n  rules:\n    - id: r\n      context: no-such-class\n      assert: \"true\"\n      code: T-E0001\n      message: never\n",
        );
        proj.write("docs/d.md", "# plain\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).expect("a directory index source must not abort");
        assert_eq!(codes_of(&findings, "MDATRON-W0049"), 1, "got {findings:?}");
    }

    // #162: a MISSING literal index source likewise degrades (was a hard Io
    // abort) — the index is empty for that key and the run completes loud.
    #[test]
    fn missing_index_source_degrades_to_w0049_not_abort() {
        let proj = TempProject::new("index-missing");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write(
            ".mdatron/patterns/p.yaml",
            "mdatron_dsl_version: 1\npattern:\n  id: miss\n  keys:\n    - name: k\n      source: registry/absent.yaml\n      select: $\n      indexed_by: $key\n  rules:\n    - id: r\n      context: no-such-class\n      assert: \"true\"\n      code: T-E0001\n      message: never\n",
        );
        proj.write("docs/d.md", "# plain\n");
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).expect("a missing index source must not abort the run");
        assert_eq!(codes_of(&findings, "MDATRON-W0049"), 1, "got {findings:?}");
    }

    // #162: the degraded index behaves as ABSENT, so a rule that requires the
    // key (`defined(key(...))`) fires its OWN configured finding — the W0049
    // is the availability signal, the rule is the conformance gate.
    #[test]
    fn rule_over_degraded_index_surfaces_its_own_finding() {
        let proj = TempProject::new("index-degraded-rule");
        proj.write(
            ".mdatron/schemas/doc.json",
            r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","required":["schema_class","dep"],"properties":{"schema_class":{"const":"doc"},"dep":{"type":"string"}},"additionalProperties":true}"#,
        );
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        std::fs::write(proj.0.join("registry.yaml"), [0xFF, 0xFE]).unwrap();
        proj.write(
            ".mdatron/patterns/p.yaml",
            "mdatron_dsl_version: 1\npattern:\n  id: dep\n  keys:\n    - name: deps\n      source: registry.yaml\n      select: $\n      indexed_by: $key\n  rules:\n    - id: r\n      context: doc\n      assert: 'defined(key(\"deps\", $self.dep))'\n      code: T-E0001\n      message: \"dep must resolve\"\n",
        );
        proj.write(
            "docs/d.md",
            "---\nschema_class: doc\ndep: alpha\n---\n# d\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let findings = verify(&cfg).expect("must not abort");
        assert_eq!(
            codes_of(&findings, "MDATRON-W0049"),
            1,
            "the source degradation is loud; got {findings:?}"
        );
        assert_eq!(
            findings.iter().filter(|f| f.code == "T-E0001").count(),
            1,
            "the rule requiring the empty key fires its own finding; got {findings:?}"
        );
    }

    // Phase-3 I-5/A-3: a snapshot miss is reported as an ENGINE defect
    // (E0080), never misattributed as a dead citation on a healthy document —
    // exercised directly against an empty snapshot (the only way to construct
    // a miss without an actual discovery bug).
    #[test]
    fn snapshot_miss_reports_engine_defect_not_dead_citation() {
        let empty = crate::snapshot::Snapshot::new(64, 4096);
        let mut findings = Vec::new();
        crate::cite::check_file(
            &empty,
            Path::new("docs/a.md"),
            "Per real.rs:1 ok.\n",
            0,
            &mut findings,
        );
        assert_eq!(codes_of(&findings, "MDATRON-E0080"), 1, "{findings:?}");
        assert_eq!(codes_of(&findings, "MDATRON-E0100"), 0, "{findings:?}");
    }

    // Post-seam consumers read the snapshot ACROSS ROLES, end to end: one
    // file serving as governed body, index source, and citation target —
    // deleted at the seam — still verifies and still resolves for every
    // consumer. (Read-MULTIPLICITY itself — one read, not three — is pinned
    // at unit level by snapshot::tests::capture_is_idempotent_and_never_
    // rereads; a seam-time deletion cannot distinguish the pre-seam reads,
    // so this e2e fixture deliberately does not claim it — phase-3 R2I-3.)
    #[test]
    fn post_seam_consumers_read_snapshot_across_roles() {
        let proj = TempProject::new("cross-role");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("GOVERNING.md", "# gov\n");
        proj.write(
            ".mdatron/routes.yaml",
            "routes:\n- files: \"docs/**/*.md\"\n  governed_by: GOVERNING.md\n  citations: true\n",
        );
        // docs/shared.md: a governed body, an index source, AND a cite target.
        proj.write("docs/shared.md", "---\nid: shared\n---\nline three\n");
        proj.write("docs/citer.md", "Per docs/shared.md:4 this holds.\n");
        proj.write(
            ".mdatron/patterns/p.yaml",
            "mdatron_dsl_version: 1\npattern:\n  id: cross\n  keys:\n    - name: shared\n      source: docs/shared.md\n      select: $.frontmatter\n      indexed_by: $.id\n  rules:\n    - id: r\n      context: no-such-class\n      assert: \"true\"\n      code: T-E0001\n      message: never\n",
        );
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let shared = proj.0.join("docs/shared.md");
        let mutate = || {
            std::fs::remove_file(&shared).unwrap();
        };
        let (findings, _fam, _vis, files_checked) = run(&cfg, None, Some(&mutate)).unwrap();
        assert_eq!(
            codes_of(&findings, "MDATRON-E0100"),
            0,
            "the citation resolves against the captured bytes; got {findings:?}"
        );
        assert_eq!(files_checked, 2, "both files verified from the snapshot");
    }

    // Security review C (#103): a non-UTF8 governed body is a PER-FILE finding
    // (E0003), never a whole-run abort — an adversary dropping one bad file must
    // not deny verification of the rest of the tree.
    #[test]
    fn non_utf8_governed_body_is_a_finding_not_an_abort() {
        let proj = TempProject::new("non-utf8");
        proj.write(".mdatron/schemas/.keep.json", "{}");
        proj.write(
            ".mdatron/config.yaml",
            "file_globs:\n  - \"docs/**/*.md\"\n",
        );
        proj.write("docs/good.md", "# fine\n");
        std::fs::write(
            proj.0.join("docs/bad.md"),
            b"---\nschema_class: doc\n---\n\xFF\xFE not utf8",
        )
        .unwrap();
        let cfg = VerifyConfig::from_project(&proj.0).unwrap();
        let (findings, _fam, _vis, files_checked) =
            run(&cfg, None, None).expect("one bad file must not abort the run");
        assert_eq!(
            codes_of(&findings, "MDATRON-E0003"),
            1,
            "the non-UTF8 body is a localized finding; got {findings:?}"
        );
        assert_eq!(
            files_checked, 1,
            "the good file is still verified; the refused file does not count as validated"
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
        // `[see: <label>]` cross-reference (#116) and the value rides in a quoted
        // region — never inline.
        assert!(
            findings[0].message.contains("[see: phase]")
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
        // The value stays out-of-line (DESIGN §Output — never inline); the message
        // carries a `[see: <label>]` cross-reference to the labeled quoted block
        // (#116, vsdd ruling on item 8 part 1). The label drops the `$self.`
        // prefix so it reads as a field name pointing at the `=phase:` block.
        assert_eq!(message, "got phase: [see: phase]");
        assert_eq!(quoted.len(), 1);
        assert_eq!(quoted[0].label, "phase");
        assert_eq!(quoted[0].content, "phase-2a");
    }

    // #116: two references to the SAME value, and two distinct exprs that would
    // clean to the SAME label, both disambiguate by number — the fallback vsdd
    // reserved for collisions/repeats. Each `[see: …]` resolves to exactly one
    // block.
    #[test]
    fn interpolate_message_numbers_colliding_labels() {
        let self_v = Value::Object(BTreeMap::from([
            ("status".to_string(), Value::Str("draft".into())),
            (
                "nested".to_string(),
                Value::Object(BTreeMap::from([(
                    "status".to_string(),
                    Value::Str("final".into()),
                )])),
            ),
        ]));
        let file_v = Value::Null;
        let project_v = Value::Null;
        let ctx = EvalContext::new(&self_v, &file_v, &project_v);
        // First and third reference the same expr; the second is a different expr
        // whose base label ("status") collides.
        let (message, quoted) = interpolate_message(
            "a {{$self.status}} b {{$self.nested.status}} c {{$self.status}}",
            &ctx,
        )
        .unwrap();
        // Each pointer resolves to a uniquely-labeled block; no bare label repeats.
        let labels: Vec<&str> = quoted.iter().map(|q| q.label.as_str()).collect();
        let mut sorted = labels.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), labels.len(), "labels are unique: {labels:?}");
        // Every `[see: X]` in the message names a real block.
        for q in &quoted {
            assert!(
                message.contains(&format!("[see: {}]", q.label)),
                "pointer for {:?} present in {message:?}",
                q.label
            );
        }
        assert!(
            !message.contains('{'),
            "no legacy brace placeholder: {message}"
        );
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
        assert_eq!(message, "domains: [see: domains]");
        assert_eq!(quoted.len(), 1);
        assert_eq!(quoted[0].label, "domains");
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

    // ── Rule field-reference validation red gate (#156) ─────────────────────────
    //
    // E0021 is a HARD GATE (error, exit 1): a false positive breaks an adopter's
    // build. This gate is therefore weighted toward the negative cases — every
    // undecidable shape (open object, binding, quantifier var, path-glob context)
    // must pass — with true positives proving the typo IS caught in each of the
    // three sites a `$self` path can appear (assert, let-binding, quantifier
    // collection).

    /// A closed frontmatter schema (`additionalProperties: false`) declaring a
    /// fixed property set — the only shape under which E0021 fires.
    const CLOSED_DOC_SCHEMA: &str = r#"{
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "schema_class": { "type": "string" },
        "title":  { "type": "string" },
        "owner":  { "type": "string" },
        "items":  { "type": "array" }
      }
    }"#;

    fn e0021_count(findings: &[Finding]) -> usize {
        findings
            .iter()
            .filter(|f| f.code == "MDATRON-E0021")
            .count()
    }

    /// Write a one-rule pattern whose `context` is `doc` (bound to the closed
    /// schema), with the given `let:` block body and assertion, then run verify.
    fn run_field_ref_gate(
        label: &str,
        schema: &str,
        let_block: &str,
        assert: &str,
    ) -> Vec<Finding> {
        let proj = TempProject::new(label);
        proj.write(".mdatron/schemas/doc.json", schema);
        proj.write(
            ".mdatron/patterns/p.yaml",
            &format!(
                "mdatron_dsl_version: 1\n\
                 pattern:\n  \
                   id: p\n  \
                   rules:\n    \
                     - id: r\n      \
                       context: doc\n\
                 {let_block}      \
                       assert: '{assert}'\n      \
                       code: T-E0001\n      \
                       message: \"m\"\n"
            ),
        );
        let cfg = VerifyConfig::new(&proj.0);
        verify(&cfg).expect("verify runs")
    }

    #[test]
    fn rg_undeclared_field_in_assert_is_e0021() {
        // TRUE POSITIVE: a typo'd `$self.ownr` (for `owner`) under a closed
        // schema hard-gates.
        let findings = run_field_ref_gate(
            "e0021-assert-typo",
            CLOSED_DOC_SCHEMA,
            "",
            r#"$self.ownr == "x""#,
        );
        let e0021: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.code == "MDATRON-E0021")
            .collect();
        assert_eq!(e0021.len(), 1, "one undeclared reference; got {findings:?}");
        assert_eq!(e0021[0].severity, Severity::Error, "E0021 hard-gates");
        assert!(
            e0021[0]
                .quoted
                .iter()
                .any(|q| q.label == "reference" && q.content == "$self.ownr"),
            "names the offending path: {:?}",
            e0021[0].quoted
        );
    }

    #[test]
    fn rg_valid_field_reference_under_closed_schema_is_clean() {
        // NEGATIVE: a declared property is not flagged.
        let findings = run_field_ref_gate(
            "e0021-valid",
            CLOSED_DOC_SCHEMA,
            "",
            r#"$self.owner == "x""#,
        );
        assert_eq!(
            e0021_count(&findings),
            0,
            "declared field is clean: {findings:?}"
        );
    }

    #[test]
    fn rg_undeclared_field_under_open_schema_not_flagged() {
        // NEGATIVE (the load-bearing guard): an OPEN object is the JSON-Schema
        // default — an unknown field there is legal, never E0021.
        let open = r#"{ "type": "object", "properties": { "title": { "type": "string" } } }"#;
        let findings = run_field_ref_gate("e0021-open", open, "", r#"$self.anything_goes == "x""#);
        assert_eq!(
            e0021_count(&findings),
            0,
            "open object tolerates unknown fields: {findings:?}"
        );
    }

    #[test]
    fn rg_quantifier_and_binding_vars_not_flagged() {
        // NEGATIVE: `$m` is a quantifier binding (not `$self`); only the
        // collection `$self.items` (declared) is a self-path. No false positive
        // on `$m.ownr`.
        let findings = run_field_ref_gate(
            "e0021-binding",
            CLOSED_DOC_SCHEMA,
            "",
            r#"every(m in $self.items, $m.ownr == "x")"#,
        );
        assert_eq!(
            e0021_count(&findings),
            0,
            "quantifier binding path is not a self-path: {findings:?}"
        );
    }

    #[test]
    fn rg_undeclared_field_inside_quantifier_collection_is_e0021() {
        // TRUE POSITIVE: the walker descends into a quantifier's COLLECTION, so a
        // typo'd `$self.itmes` (for `items`) there is still caught.
        let findings = run_field_ref_gate(
            "e0021-quant-coll",
            CLOSED_DOC_SCHEMA,
            "",
            r#"every(m in $self.itmes, $m.x == 1)"#,
        );
        let e0021: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.code == "MDATRON-E0021")
            .collect();
        assert_eq!(e0021.len(), 1, "collection typo caught; got {findings:?}");
        assert!(
            e0021[0]
                .quoted
                .iter()
                .any(|q| q.label == "reference" && q.content == "$self.itmes"),
            "names the collection path: {:?}",
            e0021[0].quoted
        );
    }

    #[test]
    fn rg_undeclared_field_in_let_binding_is_e0021() {
        // TRUE POSITIVE: a let-binding VALUE is an expression too — a typo there
        // hard-gates.
        let findings = run_field_ref_gate(
            "e0021-let",
            CLOSED_DOC_SCHEMA,
            "      let:\n        o: $self.ownr\n",
            r#"$o == "x""#,
        );
        assert_eq!(
            e0021_count(&findings),
            1,
            "let-binding typo caught: {findings:?}"
        );
    }

    #[test]
    fn rg_path_glob_context_is_not_checked() {
        // NEGATIVE: a path-glob context binds `$self` to whatever schema the
        // matched files route to — not statically known — so the rule is left
        // unchecked (no schema to validate the reference against).
        let proj = TempProject::new("e0021-pathglob");
        proj.write(".mdatron/schemas/doc.json", CLOSED_DOC_SCHEMA);
        proj.write(
            ".mdatron/patterns/p.yaml",
            "mdatron_dsl_version: 1\n\
             pattern:\n  id: p\n  rules:\n    \
               - id: r\n      \
                 context: \"docs/*.md\"\n      \
                 assert: '$self.ownr == \"x\"'\n      \
                 code: T-E0001\n      \
                 message: \"m\"\n",
        );
        let cfg = VerifyConfig::new(&proj.0);
        let findings = verify(&cfg).expect("verify runs");
        assert_eq!(
            e0021_count(&findings),
            0,
            "path-glob context has no static schema: {findings:?}"
        );
    }
}
