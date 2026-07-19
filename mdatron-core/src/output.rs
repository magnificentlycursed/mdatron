//! Output-format output object for `mdatron verify --json`.
//!
//! Implements the Phase 0 output contract behavioral contracts BC-1 through BC-3 + BC-8 per
//! `vsdd-cli/docs/refactor/phase-0-output-format/DESIGN.md` (cross-repo design).
//!
//! Phase 2b: this module turns the output_format Red Gate green for output object-shape
//! contracts. Exit-code semantics (BC-4) + stream contract (BC-5) live at the binary
//! boundary (mdatron-cli/src/main.rs).
//!
//! Output version stays at 1.0.0 for v0.1.0; subsequent additive optional-field changes
//! bump minor; required-field or shape changes bump major.

use serde::{Deserialize, Serialize};

use crate::diagnostic::{Finding, Severity};

/// Output-version contract value. Semver per SO disposition 2026-06-02 (Raise-to-SO #1).
pub const OUTPUT_VERSION: &str = "1.0.0";

/// Pipeline status — emitted as the `pipeline_status` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PipelineStatus {
    Ok,
    Failed,
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
/// 5. `findings` (array of Finding objects)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Output {
    pub mdatron_output_version: String,
    pub mdatron_version: String,
    pub pipeline_status: PipelineStatus,
    pub summary: Summary,
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
        mdatron_version: &str,
    ) -> Self {
        let summary = Summary::from_findings(&findings, files_checked);
        Self {
            mdatron_output_version: OUTPUT_VERSION.to_string(),
            mdatron_version: mdatron_version.to_string(),
            pipeline_status,
            summary,
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
        let env = Output::build(vec![], 0, PipelineStatus::Ok, "0.1.0");
        assert_eq!(env.mdatron_output_version, OUTPUT_VERSION);
        assert_eq!(env.mdatron_version, "0.1.0");
        assert_eq!(env.pipeline_status, PipelineStatus::Ok);
    }

    #[test]
    fn exit_code_zero_when_clean() {
        let env = Output::build(vec![], 5, PipelineStatus::Ok, "0.1.0");
        assert_eq!(env.derive_exit_code(), 0);
    }

    #[test]
    fn exit_code_one_when_error_present() {
        let env = Output::build(
            vec![err_finding("MDATRON-E0001")],
            5,
            PipelineStatus::Ok,
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
            "0.1.0",
        );
        assert_eq!(env.derive_exit_code(), 0);
    }

    #[test]
    fn exit_code_two_when_pipeline_failed() {
        let env = Output::build(vec![], 0, PipelineStatus::Failed, "0.1.0");
        assert_eq!(env.derive_exit_code(), 2);
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
