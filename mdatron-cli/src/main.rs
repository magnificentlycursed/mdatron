//! mdatron CLI binary.
//!
//! `mdatron verify` runs the full pipeline from `mdatron_core::verify`: loads schemas
//! from `<root>/.mdatron/schemas/`, patterns from `<root>/.mdatron/patterns/`, walks
//! the project per `--files` globs, and applies Layer 1 (JSON Schema) + Layer 2 (DSL)
//! against every matched markdown file.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mdatron_core::diagnostic::{Finding, Location, Severity};
use mdatron_core::verify::{verify, VerifyConfig, VerifyError};

mod explain;

#[derive(Parser, Debug)]
#[command(name = "mdatron", about, version, long_about = None)]
#[command(after_help = "Descended from Schematron (ISO/IEC 19757-3). \
                       Not related to the TRON blockchain.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Validate markdown documents against configured schemas and patterns.
    Verify {
        /// Project root. Defaults to the current directory.
        #[arg(long = "project-root", value_name = "DIR")]
        project_root: Option<PathBuf>,

        /// Schemas directory. Defaults to `<project-root>/.mdatron/schemas`.
        #[arg(long = "schemas", value_name = "DIR")]
        schemas: Option<PathBuf>,

        /// Patterns directory. Defaults to `<project-root>/.mdatron/patterns`.
        #[arg(long = "patterns", value_name = "DIR")]
        patterns: Option<PathBuf>,

        /// File globs (relative to project root) to validate. Defaults to `**/*.md`.
        #[arg(long = "files", value_name = "GLOB", num_args = 1..)]
        files: Vec<String>,

        /// Emit a JSON output object on stdout (per the Phase 0 output-format contract).
        #[arg(long = "json")]
        json: bool,

        /// Suppress stderr human-readable diagnostics (machine-only consumers).
        #[arg(long = "quiet", short = 'q')]
        quiet: bool,
    },

    /// Show extended documentation for an error code (rustc --explain pattern).
    Explain {
        /// The error code, e.g. MDATRON-E0001 or VSDD-E0017.
        /// Must match `^[A-Z][A-Z0-9]*-[ELW][0-9]{4}$` — operator-pasted from
        /// diagnostic output. Rejects ANSI escapes and shell-meta injection
        /// (crosslink #13 SEC/F1 + RT/F2 convergence).
        #[arg(value_parser = parse_explain_code)]
        code: String,

        /// Emit the explain page as a structured JSON object on stdout
        /// (per crosslink #13 AIE/F7). Without this flag, the markdown body
        /// is printed verbatim.
        #[arg(long = "json")]
        json: bool,

        /// Emit a one-line compact form: `<code> <severity>: <summary> —
        /// <first-sentence-of-fix>`. Suitable for agent-loop hot paths +
        /// PostToolUse hook context budgets (per crosslink #13 AIE/F2).
        #[arg(long = "compact", conflicts_with = "json")]
        compact: bool,
    },
}

fn parse_explain_code(s: &str) -> Result<String, String> {
    let bytes = s.as_bytes();
    let prefix_len = bytes.iter().position(|b| *b == b'-').ok_or_else(|| {
        format!("code must have form '<NAMESPACE>-<L><NNNN>' (e.g. MDATRON-E0001); got: {s}")
    })?;
    if prefix_len == 0 {
        return Err(format!("code namespace is empty; got: {s}"));
    }
    let prefix = &bytes[..prefix_len];
    if !prefix.iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) {
        return Err(format!(
            "code namespace must be [A-Z][A-Z0-9]*; got: {s}"
        ));
    }
    let suffix = &bytes[prefix_len + 1..];
    if suffix.len() != 5 {
        return Err(format!(
            "code body must be one letter + four digits (e.g. E0001); got: {s}"
        ));
    }
    let letter = suffix[0];
    let digits = &suffix[1..];
    if !matches!(letter, b'E' | b'L' | b'W') {
        return Err(format!(
            "code letter must be one of E (error), L (lint), W (warning); got: {s}"
        ));
    }
    if !digits.iter().all(u8::is_ascii_digit) {
        return Err(format!("code body digits must be ASCII 0-9; got: {s}"));
    }
    Ok(s.to_string())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Verify {
            project_root,
            schemas,
            patterns,
            files,
            json,
            quiet,
        } => cmd_verify(project_root, schemas, patterns, files, json, quiet),
        Command::Explain {
            code,
            json,
            compact,
        } => cmd_explain(&code, json, compact),
    }
}

fn cmd_verify(
    project_root: Option<PathBuf>,
    schemas: Option<PathBuf>,
    patterns: Option<PathBuf>,
    files: Vec<String>,
    json: bool,
    quiet: bool,
) -> ExitCode {
    use mdatron_core::output::{Output, PipelineStatus};

    let root = match project_root.map(Ok).unwrap_or_else(std::env::current_dir) {
        Ok(r) => r,
        Err(e) => {
            if !quiet {
                eprintln!("error[MDATRON-E0070]: cannot resolve project root: {e}");
            }
            return ExitCode::from(2);
        }
    };

    let mut config = VerifyConfig::new(&root);
    if let Some(s) = schemas {
        config.schemas_dir = s;
    }
    if let Some(p) = patterns {
        config.patterns_dir = p;
    }
    if !files.is_empty() {
        config.file_globs = files;
    }

    let (findings, pipeline_status, pipeline_err) = match verify(&config) {
        Ok(f) => (f, PipelineStatus::Ok, None),
        Err(e) => (Vec::new(), PipelineStatus::Failed, Some(e)),
    };

    // BC-2: files_checked count. v0.1.0 stub: 0 when pipeline failed; otherwise the
    // number of unique files referenced in findings (approximation pending a
    // verify()-level file-count return value in v0.1.x).
    let files_checked: u32 = if matches!(pipeline_status, PipelineStatus::Failed) {
        0
    } else {
        let mut seen: std::collections::BTreeSet<&std::path::Path> = std::collections::BTreeSet::new();
        for f in &findings {
            seen.insert(&f.location.file);
        }
        u32::try_from(seen.len()).unwrap_or(u32::MAX)
    };

    let output = Output::build(
        findings,
        files_checked,
        pipeline_status,
        env!("CARGO_PKG_VERSION"),
    );

    // BC-5 stream contract: --json puts the output on stdout; otherwise diagnostics
    // are rustc-shaped on stderr.
    if json {
        match serde_json::to_string(&output) {
            Ok(line) => println!("{line}"),
            Err(e) => {
                if !quiet {
                    eprintln!("error[MDATRON-E0080]: output serialization failed\n   = note: {e}");
                }
                return ExitCode::from(2);
            }
        }
    }

    if !quiet {
        if let Some(e) = &pipeline_err {
            print_pipeline_error(e);
        } else {
            for f in &output.findings {
                print_finding(f);
            }
            if output.summary.error_count == 0 && output.summary.warning_count == 0 {
                if !json {
                    // Summary line on stderr (consistent with the count summary
                    // below + with rustc convention). Per crosslink #13 QE/F2
                    // surfacing the inconsistency during README test tightening.
                    eprintln!("mdatron verify: clean");
                }
            } else {
                eprintln!(
                    "mdatron verify: {} error(s), {} warning(s) across {} finding(s)",
                    output.summary.error_count,
                    output.summary.warning_count,
                    output.findings.len()
                );
            }
        }
    }

    ExitCode::from(output.derive_exit_code())
}

fn print_finding(f: &Finding) {
    // Delegate to Finding::format_tty so the engine + CLI render TTY
    // diagnostics through one code path. Per Phase 1a behavioral spec
    // (vsdd-cli/docs/refactor/phase-2-mdatron-json/phase-1a-behavioral-spec.md).
    eprintln!("{}", f.format_tty());
}

fn print_pipeline_error(e: &VerifyError) {
    // Construct a Finding for the pipeline error so the same format_tty
    // path that renders per-file diagnostics renders this one too. Single
    // source of truth for TTY rendering. Per crosslink #13 SE/F5.
    let finding = Finding {
        code: "MDATRON-E0080".into(),
        severity: Severity::Error,
        summary: "verify pipeline failed".into(),
        message: e.to_string(),
        help: None,
        location: Location {
            file: std::path::PathBuf::new(),
            line: 0,
            column: 0,
        },
        explain_ref: None,
    };
    eprintln!("{}", finding.format_tty());
}

fn cmd_explain(code: &str, json: bool, compact: bool) -> ExitCode {
    if compact {
        if let Some(line) = explain::lookup_compact(code) {
            println!("{line}");
            return ExitCode::from(0);
        }
    } else if json {
        if let Some(structured) = explain::lookup_structured(code) {
            match serde_json::to_string(&structured) {
                Ok(line) => {
                    println!("{line}");
                    return ExitCode::from(0);
                }
                Err(e) => {
                    eprintln!("error[MDATRON-E0080]: output serialization failed\n   = note: {e}");
                    return ExitCode::from(2);
                }
            }
        }
    } else if let Some(page) = explain::lookup(code) {
        // Normalize trailing whitespace + write exactly one trailing newline.
        // Per crosslink #13 SE/F1.
        println!("{}", page.trim_end());
        // Per crosslink #12 UX/F1: if this code has a migration note (its
        // semantic shifted across emission sites), surface it AFTER the
        // page so operators recalling the prior meaning see the bridge.
        if let Some(note) = explain::migration_note(code) {
            println!();
            println!("## Migration note");
            println!();
            println!("{note}");
        }
        return ExitCode::from(0);
    }
    if explain::is_mdatron_namespace(code) {
        eprintln!(
            "error[MDATRON-E0080]: no explain page found for {code}\n   \
             = note: the explain catalog grows by one entry per emitted code; \
             {code} is not in the v0.1.0 baseline catalog\n   \
             = help: see DESIGN.md \u{00A7} Diagnostics are a versioned contract for \
             the structural meaning of unimplemented codes"
        );
        return ExitCode::from(2);
    }
    // Non-MDATRON namespace (e.g., VSDD-Exxxx): mdatron's catalog covers
    // its own namespace only per phase-0-output-format/DESIGN.md
    // namespace-separation contract.
    eprintln!(
        "error[MDATRON-E0080]: {code} is outside the mdatron namespace\n   \
         = note: mdatron explain covers MDATRON-Exxxx codes only; \
         see `vsdd explain {code}` for the VSDD namespace"
    );
    ExitCode::from(2)
}
