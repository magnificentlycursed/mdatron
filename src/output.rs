//! Output-format output object for `mdatron verify --json`.
//!
//! Implements the Phase 0 output contract behavioral contracts BC-1 through BC-3 + BC-8 per
//! `vsdd-cli/docs/refactor/phase-0-output-format/DESIGN.md` (cross-repo design).
//!
//! Phase 2b: this module turns the output_format Red Gate green for output object-shape
//! contracts. Exit-code semantics (BC-4) + stream contract (BC-5) live at the binary
//! boundary (src/main.rs).
//!
//! Output version is [`OUTPUT_VERSION`], versioned per SemVer: an additive,
//! backward-compatible change (a new optional field, a new enum value) bumps the
//! MINOR; a breaking change (a removed, renamed, or reshaped field, a field's
//! type changing, a new REQUIRED field under a closed object, or a change to an
//! emitted code's meaning) bumps the MAJOR. The published schema at
//! `schema/mdatron-output.schema.json` is the machine-readable pin; contract-
//! stability tripwires keep the two in step.

use serde::{Deserialize, Serialize};

use crate::diagnostic::{Finding, Severity};

/// Output-version contract value. Semver per SO disposition 2026-06-02 (Raise-to-SO #1).
/// 1.1.0 (#90, released in 0.3.0): additive `families` field.
/// 2.0.0 (#120, 0.4.0): MAJOR — since the last released envelope (1.1.0) the shape
/// broke twice: `families` reshaped string → `{state, reason}` object (#107) and
/// `quoted[]` gained REQUIRED `origin`/`trusted` under a closed object (#114),
/// both rejected by a 1.1.0-schema validator; `pipeline_error` is additive (#112).
/// Development passed through 1.2.0–1.4.0 (never released); those minor bumps
/// under-signalled the breaking reshape, so 0.4.0 corrects the released contract
/// to a single honest major bump.
/// 2.1.0 (#124, roast SHO1; released in 0.5.0): MINOR — additive
/// `pipeline_error.kind` value `bound_exceeded` for the input-resource-bound
/// enforcement (a new closed-enum member is additive under the SemVer rule).
/// 3.0.0 (#145, 0.6.0): MAJOR — the `families` object gains a sixth REQUIRED
/// member `link` (body-link/anchor resolution) under a closed object, which is a
/// breaking change; the same bump makes `families` forward-extensible (additional
/// members of the `FamilyActivity` shape are allowed) so future families are
/// additive/minor. This is the last families-driven major.
/// Must move in lockstep with the published schema
/// (`schema/mdatron-output.schema.json`); the `envelope_version_matches_published_schema`
/// tripwire enforces it.
pub const OUTPUT_VERSION: &str = "3.0.0";

/// The published output-envelope JSON Schema, embedded so `mdatron schema` can
/// print it to stdout for a binary-only (`cargo install`) consumer that has no
/// repo checkout (#127, roast SHO10 / DataEng-1). Kept in lockstep with
/// [`OUTPUT_VERSION`] by the contract-stability tripwires.
pub const OUTPUT_SCHEMA: &str = include_str!("../schema/mdatron-output.schema.json");

/// Whether a check family was invoked in a verify run — active means its data
/// was supplied and the family ran (NOT that it produced findings; a clean
/// active family is distinguishable from an inactive one). Per the ratified
/// #90 envelope shape (vsdd-cli consumer ask + DESIGN § Validation is
/// data-driven "the inactivity is reported").
/// Per-family activity as a tri-state plus a reason (#107, falsifiable audit
/// signal — vsdd-cli v0.3.0 review item 4). A consumer can tell "ran this pass"
/// (`active`) from "configured but did no work" (`inert`) from "not configured"
/// (`inactive`), and the `reason` documents precisely why — closing the
/// ambiguity where `active`/`inactive` conflated configured-vs-ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum FamilyActivity {
    /// Data supplied and the check ran this pass.
    Active { reason: String },
    /// Data supplied but the check did no work this pass (e.g. its scope
    /// matched no walked file) — configured, but not exercised.
    Inert { reason: String },
    /// No data supplied — the family is not part of this project's config.
    Inactive { reason: String },
}

impl FamilyActivity {
    pub fn active(reason: impl Into<String>) -> Self {
        Self::Active {
            reason: reason.into(),
        }
    }
    pub fn inert(reason: impl Into<String>) -> Self {
        Self::Inert {
            reason: reason.into(),
        }
    }
    pub fn inactive(reason: impl Into<String>) -> Self {
        Self::Inactive {
            reason: reason.into(),
        }
    }
    /// True when the check ran this pass (`active`) — the primary audit signal.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }
}

/// Per-verify activity of the five check families (`DESIGN.md` § Five check
/// families), emitted under the envelope's `families` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Families {
    pub schema: FamilyActivity,
    pub route: FamilyActivity,
    pub pin: FamilyActivity,
    pub vocabulary: FamilyActivity,
    pub citation: FamilyActivity,
    /// The link family (#145): body-link/anchor resolution. One of the two
    /// reference families the 3.0.0 envelope ships with (the `families` object
    /// became forward-extensible in the same bump — additional members of the
    /// `FamilyActivity` shape are allowed).
    pub link: FamilyActivity,
    /// The marker-line reference family (#147, vsdd GH#20 P3): a declared-pattern
    /// body line whose captured name must resolve to an element in a named doc.
    /// The second reference family present in 3.0.0; families added *after*
    /// publish are additive/minor thanks to the forward-extensible `families`.
    pub marker: FamilyActivity,
    /// The adopter code-catalog family (#148, vsdd GH#20 P4): every adopter code
    /// token cited in the corpus resolves to a declared entry. An additive
    /// member of the forward-extensible `families` (a MINOR, folded into the
    /// unpublished 3.0.0 alongside the other reference families).
    pub code_catalog: FamilyActivity,
    /// The section-structural family (#157, vsdd GH#20 P5): count/disjointness
    /// assertions over markdown body sections. Additive member of the
    /// forward-extensible `families` (folded into the unpublished 3.0.0).
    pub section: FamilyActivity,
}

impl Families {
    /// All families inactive — the state when the pipeline did not run (a
    /// pipeline-error envelope reports no family as invoked).
    pub fn all_inactive() -> Self {
        let reason = || FamilyActivity::inactive("the pipeline did not run");
        Self {
            schema: reason(),
            route: reason(),
            pin: reason(),
            vocabulary: reason(),
            citation: reason(),
            link: reason(),
            marker: reason(),
            code_catalog: reason(),
            section: reason(),
        }
    }
}

/// Pipeline status — emitted as the `pipeline_status` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PipelineStatus {
    Ok,
    Failed,
}

/// The structured reason a pipeline failed, emitted as the optional
/// `pipeline_error` field when `pipeline_status` is `failed` (#112, vsdd items
/// 5 + 6). It carries the failure INTO the envelope — the stderr note is
/// suppressed by `--quiet`, so a `--json --quiet` consumer previously got
/// `findings: []` with no cause. `kind` disambiguates the many senses that the
/// single `MDATRON-E0080` code conflated (config vs io vs schema-load vs …), so
/// a consumer can branch on the failure class without parsing prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineError {
    /// The namespace code (`MDATRON-E0080`); stable across kinds.
    pub code: String,
    /// The failure class (e.g. `config`, `io`, `schema_load`, `glob`).
    pub kind: String,
    /// The rendered failure reason.
    pub message: String,
}

/// Per-severity finding counts emitted under the output object's `summary` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub error_count: u32,
    pub warning_count: u32,
    pub lint_count: u32,
    pub files_checked: u32,
}

impl Summary {
    /// Compute summary counts from a slice of findings + the number of files checked.
    ///
    /// Pure function — Phase 1b purity-boundary candidate; Phase 5 property-test target.
    pub fn from_findings(findings: &[Finding], files_checked: u32) -> Self {
        let mut s = Self {
            error_count: 0,
            warning_count: 0,
            lint_count: 0,
            files_checked,
        };
        for f in findings {
            match f.severity {
                Severity::Error => s.error_count += 1,
                Severity::Warning => s.warning_count += 1,
                Severity::Lint => s.lint_count += 1,
            }
        }
        s
    }
}

/// Top-level output output object emitted on stdout by `mdatron verify --json`.
///
/// Field order per BC-2:
/// 1. `mdatron_output_version` (semver)
/// 2. `mdatron_version` (mdatron's own crate version)
/// 3. `pipeline_status` ("ok" / "failed")
/// 4. `summary` (per-severity counts + files_checked)
/// 5. `families` (per-family activity; #90)
/// 6. `findings` (array of Finding objects)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Output {
    pub mdatron_output_version: String,
    pub mdatron_version: String,
    pub pipeline_status: PipelineStatus,
    /// Present only on a failed pipeline (#112). Omitted entirely on success, so
    /// a clean run's envelope is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline_error: Option<PipelineError>,
    pub summary: Summary,
    pub families: Families,
    pub findings: Vec<Finding>,
}

impl Output {
    /// Construct an output object from findings + files_checked + pipeline status.
    ///
    /// `mdatron_version` is taken from `CARGO_PKG_VERSION` at the call site so the
    /// output object reflects the running binary's crate version. The output version is the
    /// compile-time constant [`OUTPUT_VERSION`].
    ///
    /// Pure function — Phase 1b purity-boundary candidate.
    pub fn build(
        findings: Vec<Finding>,
        files_checked: u32,
        pipeline_status: PipelineStatus,
        pipeline_error: Option<PipelineError>,
        families: Families,
        mdatron_version: &str,
    ) -> Self {
        let summary = Summary::from_findings(&findings, files_checked);
        Self {
            mdatron_output_version: OUTPUT_VERSION.to_string(),
            mdatron_version: mdatron_version.to_string(),
            pipeline_status,
            pipeline_error,
            summary,
            families,
            findings,
        }
    }

    /// Derive the BC-4 exit code from the output object's pipeline status + error count.
    ///
    /// Pure function. Returns:
    /// - 0 when pipeline ran + no errors (warnings/lints may exist)
    /// - 1 when pipeline ran + at least one error-severity finding
    /// - 2 when pipeline did not run to completion (PipelineStatus::Failed)
    pub fn derive_exit_code(&self) -> u8 {
        match self.pipeline_status {
            PipelineStatus::Failed => 2,
            PipelineStatus::Ok if self.summary.error_count > 0 => 1,
            PipelineStatus::Ok => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Location;
    use std::path::PathBuf;

    fn err_finding(code: &str) -> Finding {
        Finding {
            code: code.into(),
            severity: Severity::Error,
            summary: "x".into(),
            message: "y".into(),
            help: None,
            location: Location {
                file: PathBuf::from("a.md"),
                line: 1,
                column: 0,
            },
            explain_ref: None,
            quoted: Vec::new(),
        }
    }

    fn warn_finding(code: &str) -> Finding {
        let mut f = err_finding(code);
        f.severity = Severity::Warning;
        f
    }

    fn lint_finding(code: &str) -> Finding {
        let mut f = err_finding(code);
        f.severity = Severity::Lint;
        f
    }

    #[test]
    fn summary_counts_by_severity() {
        let findings = vec![
            err_finding("MDATRON-E0001"),
            err_finding("MDATRON-E0002"),
            warn_finding("MDATRON-W0050"),
            lint_finding("MDATRON-L0050"),
        ];
        let s = Summary::from_findings(&findings, 7);
        assert_eq!(s.error_count, 2);
        assert_eq!(s.warning_count, 1);
        assert_eq!(s.lint_count, 1);
        assert_eq!(s.files_checked, 7);
    }

    #[test]
    fn output_build_sets_required_fields() {
        let env = Output::build(
            vec![],
            0,
            PipelineStatus::Ok,
            None,
            Families::all_inactive(),
            "0.1.0",
        );
        assert_eq!(env.mdatron_output_version, OUTPUT_VERSION);
        assert_eq!(env.mdatron_version, "0.1.0");
        assert_eq!(env.pipeline_status, PipelineStatus::Ok);
    }

    #[test]
    fn exit_code_zero_when_clean() {
        let env = Output::build(
            vec![],
            5,
            PipelineStatus::Ok,
            None,
            Families::all_inactive(),
            "0.1.0",
        );
        assert_eq!(env.derive_exit_code(), 0);
    }

    #[test]
    fn exit_code_one_when_error_present() {
        let env = Output::build(
            vec![err_finding("MDATRON-E0001")],
            5,
            PipelineStatus::Ok,
            None,
            Families::all_inactive(),
            "0.1.0",
        );
        assert_eq!(env.derive_exit_code(), 1);
    }

    #[test]
    fn exit_code_zero_when_warnings_only_no_errors() {
        // BC-4: warnings alone do not fail the pipeline.
        let env = Output::build(
            vec![warn_finding("MDATRON-W0050")],
            5,
            PipelineStatus::Ok,
            None,
            Families::all_inactive(),
            "0.1.0",
        );
        assert_eq!(env.derive_exit_code(), 0);
    }

    #[test]
    fn pipeline_error_is_carried_and_omitted_on_success() {
        // Failed pipeline: the field is present and round-trips (#112).
        let failed = Output::build(
            vec![],
            0,
            PipelineStatus::Failed,
            Some(PipelineError {
                code: "MDATRON-E0080".into(),
                kind: "config".into(),
                message: "no jurisdiction declared".into(),
            }),
            Families::all_inactive(),
            "0.3.0",
        );
        let json = serde_json::to_value(&failed).unwrap();
        assert_eq!(json["pipeline_error"]["kind"], "config");
        assert_eq!(
            json["pipeline_error"]["message"],
            "no jurisdiction declared"
        );
        let back: Output = serde_json::from_value(json).unwrap();
        assert_eq!(back, failed, "pipeline_error round-trips");

        // Clean pipeline: the field is omitted entirely, so a success envelope is
        // byte-unchanged from before the field existed.
        let ok = Output::build(
            vec![],
            3,
            PipelineStatus::Ok,
            None,
            Families::all_inactive(),
            "0.3.0",
        );
        let json = serde_json::to_value(&ok).unwrap();
        assert!(
            json.get("pipeline_error").is_none(),
            "pipeline_error must be absent on success; got {json}"
        );
    }

    #[test]
    fn exit_code_two_when_pipeline_failed() {
        let env = Output::build(
            vec![],
            0,
            PipelineStatus::Failed,
            None,
            Families::all_inactive(),
            "0.1.0",
        );
        assert_eq!(env.derive_exit_code(), 2);
    }

    /// The published envelope schema (`schema/mdatron-output.schema.json`),
    /// embedded so the tripwires run without filesystem access.
    const PUBLISHED_SCHEMA: &str = include_str!("../schema/mdatron-output.schema.json");

    fn representative_envelope() -> Output {
        let mut errf = err_finding("MDATRON-E0050");
        errf.help = Some("fix it".into());
        errf.explain_ref = Some("MDATRON-E0050".into());
        errf.quoted = vec![crate::diagnostic::QuotedRegion {
            label: "found".into(),
            content: "\"bogus\"".into(),
        }];
        Output::build(
            vec![
                errf,
                warn_finding("MDATRON-W0041"),
                lint_finding("MDATRON-L0001"),
            ],
            3,
            PipelineStatus::Ok,
            None,
            Families {
                schema: FamilyActivity::active("schemas supplied"),
                route: FamilyActivity::active("routes supplied"),
                pin: FamilyActivity::inactive("no pins.yaml"),
                vocabulary: FamilyActivity::inactive("no vocabulary.yaml"),
                citation: FamilyActivity::inactive("no citations route"),
                link: FamilyActivity::inactive("no links route"),
                marker: FamilyActivity::inactive("no marker_rules route"),
                code_catalog: FamilyActivity::inactive("no code-catalogs.yaml"),
                section: FamilyActivity::inactive("no route supplies section_rules"),
            },
            "0.3.0",
        )
    }

    fn compile_published() -> jsonschema::Validator {
        let schema_json: serde_json::Value =
            serde_json::from_str(PUBLISHED_SCHEMA).expect("published schema is valid JSON");
        jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(&schema_json)
            .expect("published schema compiles")
    }

    // CONTRACT-STABILITY TRIPWIRE (#90): every emitted envelope validates against
    // the published schema. additionalProperties:false throughout means a shape
    // change (added/removed/renamed field) that is not mirrored in the schema
    // FAILS here — the "seeded envelope-shape change fails CI" criterion.
    #[test]
    fn envelope_validates_against_published_schema() {
        let compiled = compile_published();
        for env in [
            representative_envelope(),
            Output::build(
                vec![],
                0,
                PipelineStatus::Failed,
                None,
                Families::all_inactive(),
                "0.3.0",
            ),
            // #112: a failed pipeline carrying a structured pipeline_error must
            // validate against the published schema (the new optional field).
            Output::build(
                vec![],
                0,
                PipelineStatus::Failed,
                Some(PipelineError {
                    code: "MDATRON-E0080".into(),
                    kind: "config".into(),
                    message: "no jurisdiction declared".into(),
                }),
                Families::all_inactive(),
                "0.3.0",
            ),
            Output::build(
                vec![],
                5,
                PipelineStatus::Ok,
                None,
                Families::all_inactive(),
                "0.3.0",
            ),
        ] {
            let json = serde_json::to_value(&env).expect("envelope serializes");
            let errs: Vec<String> = compiled
                .iter_errors(&json)
                .map(|e| format!("{e} at {}", e.instance_path()))
                .collect();
            assert!(
                errs.is_empty(),
                "envelope failed the published schema:\n{}",
                errs.join("\n")
            );
        }
    }

    // TRIPWIRE (#90): the schema's declared const version equals OUTPUT_VERSION.
    // A struct-shape change bumps OUTPUT_VERSION, which forces the schema const
    // to move (else validation above breaks) — shape and version stay locked.
    #[test]
    fn published_schema_version_matches_output_version() {
        let schema: serde_json::Value = serde_json::from_str(PUBLISHED_SCHEMA).unwrap();
        let declared = schema["properties"]["mdatron_output_version"]["const"]
            .as_str()
            .expect("schema pins the output version as a const");
        assert_eq!(
            declared, OUTPUT_VERSION,
            "published schema version must equal OUTPUT_VERSION"
        );
        assert!(
            schema["$id"]
                .as_str()
                .unwrap_or("")
                .ends_with(OUTPUT_VERSION),
            "schema $id should carry the version"
        );
    }

    // TRIPWIRE (#90): the three output forms agree on the finding set (DESIGN
    // § Diagnostics are a versioned contract: "the three output forms agree on
    // fixture findings"). Every finding's code appears in the JSON envelope, its
    // TTY rendering, and its compact rendering.
    #[test]
    fn three_output_forms_agree_on_findings() {
        let env = representative_envelope();
        let json_codes: Vec<&str> = env.findings.iter().map(|f| f.code.as_str()).collect();
        assert_eq!(
            json_codes,
            ["MDATRON-E0050", "MDATRON-W0041", "MDATRON-L0001"]
        );
        for f in &env.findings {
            assert!(
                f.format_tty().contains(&f.code),
                "TTY form drops {}",
                f.code
            );
            assert!(
                f.format_compact().contains(&f.code),
                "compact form drops {}",
                f.code
            );
        }
    }

    #[test]
    fn output_version_is_semver_triple() {
        let parts: Vec<&str> = OUTPUT_VERSION.split('.').collect();
        assert_eq!(parts.len(), 3);
        for p in parts {
            assert!(
                p.parse::<u32>().is_ok(),
                "output version part not numeric: {p}"
            );
        }
    }
}
