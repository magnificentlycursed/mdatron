//! mdatron CLI binary.
//!
//! See DESIGN-MDATRON.md § CLI surface for the full v1.0 subcommand surface. This v0.1.0
//! skeleton ships `verify` (file-by-file frontmatter parsing) and `explain` (stub).
//! Other subcommands land in subsequent Phase A → B → C → F deliverables.
//!
//! Phase 2a Red Gate: `cmd_verify` body stubbed with `todo!()`. Integration tests against
//! the CLI will fail by panic; Phase 2b implements the body.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

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
        /// Files to validate. Required at v0.1.0; project-walk lands in a later iteration.
        #[arg(long = "files", value_name = "FILE", required = true, num_args = 1..)]
        files: Vec<PathBuf>,
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
        Command::Verify { files } => cmd_verify(files),
        Command::Explain { code } => cmd_explain(&code),
    }
}

fn cmd_verify(files: Vec<PathBuf>) -> ExitCode {
    let mut failed = 0usize;
    let mut parsed = 0usize;
    let mut no_frontmatter = 0usize;

    for file in &files {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "error[MDATRON-E0070]: cannot read {}: {e}",
                    file.display()
                );
                failed += 1;
                continue;
            }
        };

        match mdatron_core::frontmatter::parse(&content) {
            Ok(Some(_)) => parsed += 1,
            Ok(None) => no_frontmatter += 1,
            Err(e) => {
                eprintln!(
                    "error[MDATRON-E0001]: frontmatter-parse-failed\n  --> {}:1\n   = note: {e}",
                    file.display()
                );
                failed += 1;
            }
        }
    }

    println!(
        "mdatron verify: {parsed} with frontmatter, {no_frontmatter} without, {failed} failed across {} file(s)",
        files.len()
    );

    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn cmd_explain(code: &str) -> ExitCode {
    eprintln!("mdatron explain {code}: extended docs not yet implemented at v0.1.0");
    ExitCode::from(2)
}
