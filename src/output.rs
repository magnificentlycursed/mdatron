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
/// Also folded into the still-unpublished 3.0.0 (the pre-cut envelope batch,
/// operator-ruled — one contract snapshot for the first consumer):
/// - `envelope_schema` (REQUIRED, #176): the published schema's `$id`, so a
///   consumer can pin/fetch the exact contract (the SARIF `$schema` posture).
/// - `inputs` (REQUIRED, #176): governance-input lineage — a map of the config
///   inputs this run consumed to `sha256:<hex>` digests of the bytes it read.
/// - per-finding `fingerprint` (REQUIRED on findings, #177): a `v1:`-prefixed
///   line-churn-stable identity for cross-run trending.
/// - `timings` (OPTIONAL, #175): flag-gated run-phase wall-clock; omitted by
///   default so the default envelope stays deterministic.
///
/// Must move in lockstep with the published schema
/// (`schema/mdatron-output.schema.json`); the `envelope_version_matches_published_schema`
/// tripwire enforces it.
pub const OUTPUT_VERSION: &str = "3.0.0";

/// The published envelope schema's `$id` (#176) — emitted verbatim as the
/// envelope's `envelope_schema` field so a consumer can pin the exact contract
/// an envelope was produced under (the sarif-envelope-audit `$schema`-pin gap).
/// An identifier, not a fetch target (DEF6); kept in lockstep with
/// [`OUTPUT_VERSION`] and the schema's own `$id` by the version tripwire.
pub const ENVELOPE_SCHEMA_ID: &str =
    "https://github.com/magnificentlycursed/mdatron/schema/mdatron-output/3.0.0";

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

/// Run-phase wall-clock timings in milliseconds (#175), emitted under the
/// envelope's OPTIONAL `timings` field only when `verify --timings` is passed.
/// Flat fixed keys: `load` covers config/schema/pattern/route/pin/vocabulary/
/// catalog loading; `capture` the governed walk + snapshot build through the
/// capture-complete seam; `check` the per-file loop and cross-file checks;
/// `total` the whole `run()`. Timings are the envelope's sole non-deterministic
/// zone — flag-gating keeps the DEFAULT envelope byte-identical across runs on
/// an unchanged tree (the DEF4/determinism guardrail).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timings {
    pub total_ms: u64,
    pub load_ms: u64,
    pub capture_ms: u64,
    pub check_ms: u64,
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
/// Field order per BC-2 (extended by the pre-cut envelope batch, #175–#177):
/// 1. `mdatron_output_version` (semver)
/// 2. `envelope_schema` (the published schema's `$id`; #176)
/// 3. `mdatron_version` (mdatron's own crate version)
/// 4. `pipeline_status` ("ok" / "failed")
/// 5. `summary` (per-severity counts + files_checked)
/// 6. `families` (per-family activity; #90)
/// 7. `inputs` (governance-input lineage digests; #176)
/// 8. `timings` (OPTIONAL, flag-gated; #175)
/// 9. `findings` (array of Finding objects, each carrying a `fingerprint`; #177)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Output {
    pub mdatron_output_version: String,
    /// The published envelope schema's `$id` (#176) — the exact contract this
    /// envelope was produced under, pinnable by a consumer.
    #[serde(default)]
    pub envelope_schema: String,
    pub mdatron_version: String,
    pub pipeline_status: PipelineStatus,
    /// Present only on a failed pipeline (#112). Omitted entirely on success, so
    /// a clean run's envelope is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline_error: Option<PipelineError>,
    pub summary: Summary,
    pub families: Families,
    /// Governance-input lineage (#176): each input the run consumed —
    /// `config.yaml`, `routes.yaml`, `vocabulary.yaml`, `pins.yaml`,
    /// `code-catalogs.yaml` (present only when found and read), plus one
    /// aggregate digest each for the `schemas` and `patterns` directories —
    /// mapped to a `sha256:<lowercase-hex>` digest of the same bytes the run
    /// read. Deterministic (sorted map, forward-slashed names inside the
    /// aggregates); always empty on a failed pipeline — some inputs may have
    /// been read before the failure, but partial lineage is deliberately not
    /// attested (cold-review R3).
    #[serde(default)]
    pub inputs: std::collections::BTreeMap<String, String>,
    /// Run-phase timings (#175) — present only under `verify --timings`, so
    /// the default envelope stays deterministic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timings: Option<Timings>,
    /// Serialized with a per-finding `fingerprint` attached (#177) — computed
    /// at envelope-build time over the whole list (the occurrence ordinal
    /// needs sibling context), never stored on the Finding itself.
    #[serde(serialize_with = "serialize_findings_with_fingerprints")]
    pub findings: Vec<Finding>,
}

/// The `v1` per-finding fingerprints for a run's findings, positionally aligned
/// (#177): `sha256` over an INJECTIVE, netstring-style encoding of — in order —
/// the finding's `code`, its FORWARD-SLASHED project-root-relative file path,
/// its `summary` (each as `{byte_len}:{bytes}`), the IDENTITY-BEARING
/// quoted-region COUNT (as `{n};`), each such region's label then content
/// (each `{byte_len}:{bytes}`), and finally the 0-based occurrence ordinal
/// among findings with an otherwise-identical input in the same run; truncated
/// to 16 bytes (32 lowercase hex chars) and prefixed `v1:` (a future algorithm
/// change mints `v2`). Every field is length-prefixed and the region list is
/// count-prefixed, so no adopter-controlled byte (a YAML `"\0"` escape in a
/// rule id or document value) can shift a field or region boundary — two
/// distinct inputs always encode to distinct byte strings; the count covers
/// the FILTERED list, so the layout stays injective over it. Identity-bearing
/// means adopter-content regions (the default); a region marked
/// `platform_variant` — engine prose quoting platform/environment-variant text
/// such as an `io::Error` (strerror on unix, FormatMessage on Windows) — is
/// EXCLUDED (cold-review R7), or the same defect would fingerprint differently
/// per platform and split the cross-run trend identity, the same class the
/// forward-slashed path rule closes. Line/column are likewise EXCLUDED by
/// design — the fingerprint survives line churn, which is its purpose
/// (cross-run identity for a consumer trending envelopes across regenerated
/// documents). The ordinal disambiguates byte-identical siblings (two
/// identical dead links in one file get distinct prints); removing the first
/// transfers its identity to the survivor — the standard SARIF-style tradeoff.
pub fn fingerprints(findings: &[Finding]) -> Vec<String> {
    use std::fmt::Write;
    let mut seen: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let field = |out: &mut String, s: &str| {
        let _ = write!(out, "{}:", s.len());
        out.push_str(s);
    };
    findings
        .iter()
        .map(|f| {
            let mut identity = String::new();
            field(&mut identity, &f.code);
            field(
                &mut identity,
                &crate::diagnostic::to_forward_slash(&f.location.file),
            );
            field(&mut identity, &f.summary);
            // R7: only identity-bearing regions participate — the count is of
            // the filtered list, keeping the netstring layout injective over it.
            let in_identity: Vec<_> = f.quoted.iter().filter(|q| !q.platform_variant).collect();
            let _ = write!(identity, "{};", in_identity.len());
            for q in in_identity {
                field(&mut identity, &q.label);
                field(&mut identity, &q.content);
            }
            let n = seen.entry(identity.clone()).or_insert(0);
            let ordinal = *n;
            *n += 1;
            let _ = write!(identity, "{ordinal}");
            let hex = crate::init::sha256_hex(identity.as_bytes());
            format!("v1:{}", &hex[..32])
        })
        .collect()
}

/// Serialize the findings array with each finding's `fingerprint` attached
/// (#177). A field-order-preserving MIRROR of [`Finding`]'s serialized shape
/// plus the trailing `fingerprint`. Drift guards, per leg (cold-review R5
/// scoped the honest claim): an added/renamed field fails the
/// envelope-validates tripwire (schema `additionalProperties: false` +
/// `required` on findings); dropping the `quoted` skip-when-empty attr fails
/// the schema's `minItems: 1` on `quoted` plus the no-quoted-key assertion in
/// `envelope_carries_the_precut_fields_and_validates`.
fn serialize_findings_with_fingerprints<S: serde::Serializer>(
    findings: &[Finding],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use crate::diagnostic::{Location, QuotedRegion};
    use serde::ser::SerializeSeq;

    #[derive(Serialize)]
    struct Row<'a> {
        code: &'a str,
        severity: Severity,
        summary: &'a str,
        message: &'a str,
        help: &'a Option<String>,
        location: &'a Location,
        explain_ref: &'a Option<String>,
        #[serde(skip_serializing_if = "<[QuotedRegion]>::is_empty")]
        quoted: &'a [QuotedRegion],
        fingerprint: &'a str,
    }

    let prints = fingerprints(findings);
    let mut seq = serializer.serialize_seq(Some(findings.len()))?;
    for (f, fp) in findings.iter().zip(&prints) {
        seq.serialize_element(&Row {
            code: &f.code,
            severity: f.severity,
            summary: &f.summary,
            message: &f.message,
            help: &f.help,
            location: &f.location,
            explain_ref: &f.explain_ref,
            quoted: &f.quoted,
            fingerprint: fp,
        })?;
    }
    seq.end()
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
            envelope_schema: ENVELOPE_SCHEMA_ID.to_string(),
            mdatron_version: mdatron_version.to_string(),
            pipeline_status,
            pipeline_error,
            summary,
            families,
            inputs: std::collections::BTreeMap::new(),
            timings: None,
            findings,
        }
    }

    /// Attach the run's governance-input lineage (#176). Chainable; `build`
    /// starts with an empty map (a failed pipeline loaded nothing).
    pub fn with_inputs(mut self, inputs: std::collections::BTreeMap<String, String>) -> Self {
        self.inputs = inputs;
        self
    }

    /// Attach run-phase timings (#175). Chainable; pass `None` (the default)
    /// to keep the deterministic envelope — only `verify --timings` sets it.
    pub fn with_timings(mut self, timings: Option<Timings>) -> Self {
        self.timings = timings;
        self
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
            platform_variant: false,
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
    // Extended by #176: ENVELOPE_SCHEMA_ID is the third leg of the lockstep —
    // it must equal the schema's own `$id`, equal the schema's declared
    // `envelope_schema` const, and carry OUTPUT_VERSION as its version segment.
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
        assert_eq!(
            schema["$id"].as_str().unwrap_or(""),
            ENVELOPE_SCHEMA_ID,
            "the emitted envelope_schema const must equal the schema's own $id"
        );
        assert_eq!(
            schema["properties"]["envelope_schema"]["const"]
                .as_str()
                .unwrap_or(""),
            ENVELOPE_SCHEMA_ID,
            "the schema pins envelope_schema to its own $id"
        );
        assert!(
            ENVELOPE_SCHEMA_ID.ends_with(OUTPUT_VERSION),
            "ENVELOPE_SCHEMA_ID must carry OUTPUT_VERSION as its version segment"
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

    // ── the pre-cut envelope batch (#175–#177) ──────────────────────────────

    fn f_at(code: &str, file: &str, line: u32) -> Finding {
        let mut f = err_finding(code);
        f.location.file = PathBuf::from(file);
        f.location.line = line;
        f
    }

    // RED GATE (#177): the fingerprint is line-churn-STABLE — the same finding
    // at a different line keeps its identity (line/column excluded by design).
    #[test]
    fn fingerprint_survives_line_churn() {
        let a = fingerprints(&[f_at("MDATRON-E0110", "docs/a.md", 10)]);
        let b = fingerprints(&[f_at("MDATRON-E0110", "docs/a.md", 99)]);
        assert_eq!(a, b, "line churn must not change the fingerprint");
        assert!(
            a[0].starts_with("v1:") && a[0].len() == 3 + 32,
            "v1-prefixed 32-hex form: {:?}",
            a[0]
        );
        assert!(
            a[0][3..]
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "lowercase hex: {:?}",
            a[0]
        );
    }

    // RED GATE (#177): two byte-identical findings in one run get DISTINCT
    // fingerprints via the occurrence ordinal, and the ordinal is positional —
    // removing the first transfers its identity to the survivor (the SARIF-
    // style tradeoff, documented).
    #[test]
    fn fingerprint_ordinal_distinguishes_identical_siblings() {
        let one = f_at("MDATRON-E0110", "docs/a.md", 3);
        let two = f_at("MDATRON-E0110", "docs/a.md", 7);
        let prints = fingerprints(&[one.clone(), two]);
        assert_ne!(
            prints[0], prints[1],
            "identical siblings are disambiguated by ordinal"
        );
        let survivor = fingerprints(&[one]);
        assert_eq!(
            prints[0], survivor[0],
            "removing the first transfers identity to the survivor"
        );
    }

    // #177: the identity is sensitive to what it claims to cover — quoted
    // content (and code/path/summary) change it.
    #[test]
    fn fingerprint_changes_with_quoted_content_and_path() {
        let mut a = f_at("MDATRON-E0110", "docs/a.md", 1);
        a.quoted = vec![crate::diagnostic::QuotedRegion {
            platform_variant: false,
            label: "link".into(),
            content: "gone.md".into(),
        }];
        let mut b = a.clone();
        b.quoted[0].content = "other.md".into();
        assert_ne!(
            fingerprints(&[a.clone()])[0],
            fingerprints(&[b])[0],
            "quoted content participates in the identity"
        );
        let mut c = a.clone();
        c.location.file = PathBuf::from("docs/b.md");
        assert_ne!(
            fingerprints(&[a])[0],
            fingerprints(&[c])[0],
            "the file path participates in the identity"
        );
    }

    // RED GATE (#177 cold-review R1, MAJOR): the identity encoding is
    // INJECTIVE — an adopter-controlled NUL (a YAML `"\0"` escape in a rule id
    // or a document value) must not shift a field boundary. Under the old
    // NUL-terminated encoding BOTH constructions below collided end-to-end.
    #[test]
    fn fingerprint_encoding_resists_nul_field_shifting() {
        // Collision 1: summary "s" + quoted ("l","SECRET") vs a different
        // finding whose summary smuggles the whole tail ("s\0l\0SECRET") with
        // no quoted regions.
        let mut a = f_at("T-E0001", "docs/a.md", 1);
        a.summary = "s".into();
        a.quoted = vec![crate::diagnostic::QuotedRegion {
            platform_variant: false,
            label: "l".into(),
            content: "SECRET".into(),
        }];
        let mut b = f_at("T-E0001", "docs/a.md", 1);
        b.summary = "s\0l\0SECRET".into();
        assert_ne!(
            fingerprints(&[a])[0],
            fingerprints(&[b])[0],
            "a NUL-smuggling summary must not collide with a quoted region"
        );
    }

    // RED GATE (#177 cold-review R1, MAJOR): quoted-region SPLICING — one
    // region whose content smuggles a NUL-framed second region must not
    // collide with the honest two-region finding (the region count and the
    // per-field length prefixes make the list encoding injective).
    #[test]
    fn fingerprint_encoding_resists_region_splicing() {
        let region = |label: &str, content: &str| crate::diagnostic::QuotedRegion {
            platform_variant: false,
            label: label.into(),
            content: content.into(),
        };
        let mut spliced = f_at("T-E0001", "docs/a.md", 1);
        spliced.quoted = vec![region("found", "x\0found\0y")];
        let mut honest = f_at("T-E0001", "docs/a.md", 1);
        honest.quoted = vec![region("found", "x"), region("found", "y")];
        assert_ne!(
            fingerprints(&[spliced])[0],
            fingerprints(&[honest])[0],
            "one spliced region must not collide with two honest regions"
        );
    }

    // RED GATE (#177 cold-review R7): a region marked platform_variant —
    // engine prose quoting platform-variant text (an io::Error: strerror on
    // unix, FormatMessage on Windows) — is EXCLUDED from the identity, so the
    // SAME dead link fingerprints identically across platforms; an
    // adopter-content region's bytes still participate.
    #[test]
    fn fingerprint_excludes_platform_variant_regions_only() {
        let base = || {
            let mut f = f_at("MDATRON-E0110", "docs/a.md", 1);
            f.quoted = vec![
                crate::diagnostic::QuotedRegion {
                    platform_variant: false,
                    label: "link".into(),
                    content: "gone.md".into(),
                },
                crate::diagnostic::QuotedRegion {
                    platform_variant: true,
                    label: "os error".into(),
                    content: "No such file or directory (os error 2)".into(),
                },
            ];
            f
        };
        // The desired platform invariance: only the os-error prose differs.
        let unix = base();
        let mut windows = base();
        windows.quoted[1].content =
            "The system cannot find the file specified. (os error 2)".into();
        assert_eq!(
            fingerprints(&[unix.clone()])[0],
            fingerprints(&[windows])[0],
            "platform-variant engine prose must not split the identity"
        );
        // Adopter content still participates.
        let mut other_link = base();
        other_link.quoted[0].content = "other.md".into();
        assert_ne!(
            fingerprints(&[unix])[0],
            fingerprints(&[other_link])[0],
            "adopter-content regions stay identity-bearing"
        );
    }

    // #177: the path is fingerprinted FORWARD-SLASHED, so the identity cannot
    // split across platforms on the separator.
    #[cfg(windows)]
    #[test]
    fn fingerprint_path_is_forward_slashed() {
        let back = f_at("MDATRON-E0110", "docs\\a.md", 1);
        let fwd = f_at("MDATRON-E0110", "docs/a.md", 1);
        assert_eq!(fingerprints(&[back])[0], fingerprints(&[fwd])[0]);
    }

    // RED GATE (#175/#176/#177 envelope shape): the serialized envelope carries
    // envelope_schema (the schema's $id), the inputs map, a fingerprint on
    // every finding — and NO timings key by default (the determinism
    // guardrail); with_timings emits the four flat keys and both shapes
    // validate against the published schema.
    #[test]
    fn envelope_carries_the_precut_fields_and_validates() {
        let inputs = std::collections::BTreeMap::from([(
            "config.yaml".to_string(),
            format!("sha256:{}", "0".repeat(64)),
        )]);
        let default_env = representative_envelope().with_inputs(inputs.clone());
        let json = serde_json::to_value(&default_env).unwrap();
        assert_eq!(json["envelope_schema"], ENVELOPE_SCHEMA_ID);
        assert_eq!(
            json["inputs"]["config.yaml"],
            format!("sha256:{}", "0".repeat(64))
        );
        assert!(
            json.get("timings").is_none(),
            "no --timings, no timings key (byte-compat guardrail); got {json}"
        );
        for f in json["findings"].as_array().unwrap() {
            let fp = f["fingerprint"]
                .as_str()
                .expect("every finding carries a fingerprint");
            assert!(fp.starts_with("v1:"), "algorithm-versioned: {fp}");
        }
        // Cold-review R5: a finding WITHOUT quoted regions serializes with NO
        // `quoted` key at all — never an empty array. Together with the
        // schema's `minItems: 1`, this pins the Row mirror's skip-when-empty
        // attr (dropping it emits `quoted: []`, which both legs now catch).
        assert!(
            json["findings"][0].get("quoted").is_some(),
            "the quoted-bearing finding keeps its regions"
        );
        assert!(
            json["findings"][1].get("quoted").is_none(),
            "a region-less finding carries no quoted key; got {}",
            json["findings"][1]
        );

        let timed_env = representative_envelope()
            .with_inputs(inputs)
            .with_timings(Some(Timings {
                total_ms: 12,
                load_ms: 3,
                capture_ms: 4,
                check_ms: 5,
            }));
        let timed = serde_json::to_value(&timed_env).unwrap();
        for key in ["total_ms", "load_ms", "capture_ms", "check_ms"] {
            assert!(
                timed["timings"][key].is_u64(),
                "timings carries flat u64 {key}; got {timed}"
            );
        }

        let compiled = compile_published();
        for env_json in [json, timed] {
            let errs: Vec<String> = compiled
                .iter_errors(&env_json)
                .map(|e| format!("{e} at {}", e.instance_path()))
                .collect();
            assert!(
                errs.is_empty(),
                "pre-cut envelope failed the published schema:\n{}",
                errs.join("\n")
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
