//! mdatron CLI binary.
//!
//! `mdatron verify` runs the full pipeline from `mdatron_core::verify`: loads schemas
//! from `<root>/.mdatron/schemas/`, patterns from `<root>/.mdatron/patterns/`, walks
//! the project per `--files` globs, and applies Layer 1 (JSON Schema) + Layer 2 (DSL)
//! against every matched markdown file.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mdatron_core::diagnostic::Finding;
use mdatron_core::verify::{verify, VerifyConfig, VerifyError};

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

        /// Emit a JSON wire envelope on stdout (Phase 0 wire-format contract).
        /// Phase 2a Red Gate stub: flag accepted but envelope emission not yet implemented.
        #[arg(long = "json")]
        json: bool,

        /// Suppress stderr human-readable diagnostics (machine-only consumers).
        /// Phase 2a Red Gate stub: flag accepted but behavior not yet implemented.
        #[arg(long = "quiet", short = 'q')]
        quiet: bool,
    },

    /// Show extended documentation for an error code (rustc --explain pattern).
    Explain {
        /// The error code, e.g. MDATRON-E0001 or VSDD-E0017.
        code: String,
    },
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
        Command::Explain { code } => cmd_explain(&code),
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
    use mdatron_core::wire::{Envelope, PipelineStatus};

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

    let envelope = Envelope::build(
        findings,
        files_checked,
        pipeline_status,
        env!("CARGO_PKG_VERSION"),
    );

    // BC-5 stream contract: --json puts the envelope on stdout; otherwise diagnostics
    // are rustc-shaped on stderr.
    if json {
        match serde_json::to_string(&envelope) {
            Ok(line) => println!("{line}"),
            Err(e) => {
                if !quiet {
                    eprintln!("error[MDATRON-E0080]: envelope serialization failed\n   = note: {e}");
                }
                return ExitCode::from(2);
            }
        }
    }

    if !quiet {
        if let Some(e) = &pipeline_err {
            print_pipeline_error(e);
        } else {
            for f in &envelope.findings {
                print_finding(f);
            }
            if envelope.summary.error_count == 0 && envelope.summary.warning_count == 0 {
                if !json {
                    println!("mdatron verify: clean");
                }
            } else {
                eprintln!(
                    "mdatron verify: {} error(s), {} warning(s) across {} finding(s)",
                    envelope.summary.error_count,
                    envelope.summary.warning_count,
                    envelope.findings.len()
                );
            }
        }
    }

    ExitCode::from(envelope.derive_exit_code())
}

fn print_finding(f: &Finding) {
    eprintln!(
        "{label}[{code}]: {summary}\n  --> {file}:{line}\n   = note: {message}",
        label = f.severity.label(),
        code = f.code,
        summary = f.summary,
        file = f.location.file.display(),
        line = f.location.line,
        message = f.message,
    );
    if let Some(help) = &f.help {
        eprintln!("   = help: {help}");
    }
}

fn print_pipeline_error(e: &VerifyError) {
    eprintln!("error[MDATRON-E0080]: verify pipeline failed\n   = note: {e}");
}

fn cmd_explain(code: &str) -> ExitCode {
    eprintln!("mdatron explain {code}: extended docs not yet implemented at v0.1.0");
    ExitCode::from(2)
}
