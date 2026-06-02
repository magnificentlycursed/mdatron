//! mdatron CLI binary.
//!
//! `mdatron verify` runs the full pipeline from `mdatron_core::verify`: loads schemas
//! from `<root>/.mdatron/schemas/`, patterns from `<root>/.mdatron/patterns/`, walks
//! the project per `--files` globs, and applies Layer 1 (JSON Schema) + Layer 2 (DSL)
//! against every matched markdown file.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mdatron_core::diagnostic::{Finding, Severity};
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
        } => cmd_verify(project_root, schemas, patterns, files),
        Command::Explain { code } => cmd_explain(&code),
    }
}

fn cmd_verify(
    project_root: Option<PathBuf>,
    schemas: Option<PathBuf>,
    patterns: Option<PathBuf>,
    files: Vec<String>,
) -> ExitCode {
    let root = match project_root.map(Ok).unwrap_or_else(std::env::current_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error[MDATRON-E0070]: cannot resolve project root: {e}");
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

    let findings = match verify(&config) {
        Ok(f) => f,
        Err(e) => {
            print_pipeline_error(&e);
            return ExitCode::from(2);
        }
    };

    let mut errors = 0usize;
    let mut warnings = 0usize;
    for f in &findings {
        print_finding(f);
        match f.severity {
            Severity::Error => errors += 1,
            Severity::Warning => warnings += 1,
            Severity::Lint => {}
        }
    }

    if errors == 0 && warnings == 0 {
        println!("mdatron verify: clean");
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "mdatron verify: {errors} error(s), {warnings} warning(s) across {} finding(s)",
            findings.len()
        );
        if errors > 0 {
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        }
    }
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
