//! Diagnostic types: [`Finding`], [`Severity`], [`Location`].
//!
//! Phase 2a Red Gate: method bodies are stubbed with `todo!()`. Tests in this module assert
//! the behavioral contracts; they fail-by-default (panic on `todo!()`). Phase 2b implements
//! the bodies to turn the Red Gate green.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Severity tier for a finding.
///
/// Maps to rustc-style diagnostic levels: `error` blocks pre-commit / CI; `warning` surfaces
/// but allows; `lint` is informational only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Lint,
}

impl Severity {
    /// The string used in TTY-style diagnostic output (rustc convention):
    /// `Error` → `"error"`, `Warning` → `"warning"`, `Lint` → `"info"`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Lint => "info",
        }
    }
}

/// A source-span location: file + line + column.
///
/// `line` and `column` are 1-based; column may be 0 if the validator could not pinpoint
/// the column (e.g. whole-frontmatter findings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub file: PathBuf,
    pub line: u32,
    pub column: u32,
}

impl Location {
    /// Construct a whole-file location: the given file at line 1, column 0.
    pub fn whole_file(file: impl Into<PathBuf>) -> Self {
        Self {
            file: file.into(),
            line: 1,
            column: 0,
        }
    }

    /// Render the file path for TTY output with control characters escaped
    /// to `\xNN` form. Defends against ANSI escape sequences injected via
    /// attacker-crafted filenames (Unix paths may contain newlines + control
    /// chars). Per crosslink #13 SEC/F2 + SE/F6 convergence.
    pub fn safe_display(&self) -> String {
        let mut out = String::new();
        for ch in self.file.display().to_string().chars() {
            if ch.is_control() {
                use std::fmt::Write;
                let _ = write!(out, "\\x{:02X}", ch as u32);
            } else {
                out.push(ch);
            }
        }
        out
    }
}

/// A diagnostic finding emitted by the validator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub code: String,
    pub severity: Severity,
    pub summary: String,
    pub message: String,
    pub help: Option<String>,
    pub location: Location,
    pub explain_ref: Option<String>,
}

impl Finding {
    /// Render the finding in rustc-style TTY format. Single source of truth
    /// for TTY rendering across the engine + CLI per
    /// `vsdd-cli/docs/refactor/phase-2-mdatron-json/phase-1a-behavioral-spec.md`.
    ///
    /// Output structure (matches rustc / clippy convention):
    /// - Line 1: `<severity_label>[<code>]: <summary>`
    /// - Line 2: `  --> <file>:<line>` (column appended as `:<column>` when nonzero)
    /// - Line 3: `   = note: <message>`
    /// - Optional `   = help: <help>` line when `help` is `Some`
    /// - Optional `   = explain: mdatron explain <explain_ref>` line when `explain_ref` is `Some`
    pub fn format_tty(&self) -> String {
        use std::fmt::Write;
        let mut output = format!(
            "{}[{}]: {}\n  --> {}:{}",
            self.severity.label(),
            self.code,
            self.summary,
            self.location.safe_display(),
            self.location.line,
        );
        if self.location.column > 0 {
            let _ = write!(output, ":{}", self.location.column);
        }
        // Per crosslink #13 SE/F4: skip the `= note:` line when the message
        // is just the summary (no additional info beyond the headline).
        if self.message != self.summary {
            // Per crosslink #13 SE/F6: indent continuation lines so multi-line
            // messages don't break the rustc-shape layout. First line takes
            // `\n   = note: `; later lines align under the body text.
            let mut lines = self.message.lines();
            if let Some(first) = lines.next() {
                let _ = write!(output, "\n   = note: {first}");
                for cont in lines {
                    let _ = write!(output, "\n           {cont}");
                }
            }
        }
        if let Some(help) = &self.help {
            let mut lines = help.lines();
            if let Some(first) = lines.next() {
                let _ = write!(output, "\n   = help: {first}");
                for cont in lines {
                    let _ = write!(output, "\n           {cont}");
                }
            }
        }
        if let Some(explain) = &self.explain_ref {
            let _ = write!(output, "\n   = explain: mdatron explain {explain}");
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_error_label_is_error() {
        assert_eq!(Severity::Error.label(), "error");
    }

    #[test]
    fn severity_warning_label_is_warning() {
        assert_eq!(Severity::Warning.label(), "warning");
    }

    #[test]
    fn severity_lint_label_is_info() {
        // Lint maps to "info" per rustc convention (info-level diagnostics).
        assert_eq!(Severity::Lint.label(), "info");
    }

    #[test]
    fn location_whole_file_uses_given_path_at_line_one() {
        let loc = Location::whole_file("docs/example.md");
        assert_eq!(loc.file, PathBuf::from("docs/example.md"));
        assert_eq!(loc.line, 1);
        assert_eq!(loc.column, 0);
    }

    #[test]
    fn format_tty_minimal_finding() {
        let finding = Finding {
            code: "MDATRON-E0001".into(),
            severity: Severity::Error,
            summary: "frontmatter-parse-failed".into(),
            message: "could not parse frontmatter".into(),
            help: None,
            location: Location {
                file: "docs/x.md".into(),
                line: 1,
                column: 0,
            },
            explain_ref: None,
        };
        let output = finding.format_tty();
        assert!(
            output.contains("error[MDATRON-E0001]"),
            "missing severity[code]: prefix; got: {output}"
        );
        assert!(
            output.contains("could not parse frontmatter"),
            "missing message body; got: {output}"
        );
        assert!(
            output.contains("--> docs/x.md:1"),
            "missing rustc-style location arrow; got: {output}"
        );
        assert!(
            !output.contains(":0\n") && !output.ends_with(":0"),
            "column 0 should NOT be appended; got: {output}"
        );
    }

    #[test]
    fn format_tty_with_column_appends_column() {
        let finding = Finding {
            code: "MDATRON-W0050".into(),
            severity: Severity::Warning,
            summary: "header-count-mismatch".into(),
            message: "header declares (3) but table has 4 rows".into(),
            help: None,
            location: Location {
                file: "docs/y.md".into(),
                line: 41,
                column: 30,
            },
            explain_ref: None,
        };
        let output = finding.format_tty();
        assert!(
            output.contains("warning[MDATRON-W0050]"),
            "missing warning severity[code]: prefix; got: {output}"
        );
        assert!(
            output.contains("--> docs/y.md:41:30"),
            "missing column-appended location; got: {output}"
        );
    }

    #[test]
    fn format_tty_with_help_and_explain_includes_both_lines() {
        let finding = Finding {
            code: "MDATRON-W0050".into(),
            severity: Severity::Warning,
            summary: "header-count-mismatch".into(),
            message: "header declares (3) but table has 4 rows".into(),
            help: Some("change the header count or remove an extra row".into()),
            location: Location {
                file: "docs/y.md".into(),
                line: 41,
                column: 30,
            },
            explain_ref: Some("MDATRON-W0050".into()),
        };
        let output = finding.format_tty();
        assert!(
            output.contains("= help: change the header count or remove an extra row"),
            "missing help line in rustc convention; got: {output}"
        );
        assert!(
            output.contains("= explain: mdatron explain MDATRON-W0050"),
            "missing explain ref line in rustc convention; got: {output}"
        );
    }

    #[test]
    fn lint_severity_label_distinguishes_from_warning() {
        // Lint must NOT collide with Warning's label string.
        assert_ne!(Severity::Lint.label(), Severity::Warning.label());
    }
}
