//! mdatron CLI binary.
//!
//! `mdatron verify` runs the full pipeline from `mdatron::verify`: loads schemas
//! from `<root>/.mdatron/schemas/`, patterns from `<root>/.mdatron/patterns/`, walks
//! the project per `--files` globs, and applies Layer 1 (JSON Schema) + Layer 2 (DSL)
//! against every matched markdown file.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mdatron::diagnostic::{Finding, Location, Severity};
use mdatron::verify::{verify_incremental, verify_report, VerifyConfig, VerifyError};

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

        /// Emit the compact agent-context form on stdout: one size-capped block
        /// per finding (512 bytes, DESIGN §Output; #80 D4), adopter content
        /// prefix-marked, truncation at line boundaries with an elision marker.
        #[arg(long = "compact", conflicts_with = "json")]
        compact: bool,

        /// Suppress stderr human-readable diagnostics (machine-only consumers).
        #[arg(long = "quiet", short = 'q')]
        quiet: bool,

        /// Incremental mode (#102): verify only this changed file and its
        /// transitive dependents, reporting the same findings a whole-tree run
        /// would for those files. A change under `.mdatron/` falls back to a
        /// whole-tree run. The verified (visited) file set prints to stderr.
        #[arg(long = "changed", value_name = "FILE")]
        changed: Option<PathBuf>,
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

    /// Verify the pin record, or recompute it with --update (#84).
    Pin {
        /// Project root. Defaults to the current directory.
        #[arg(long = "project-root", value_name = "DIR")]
        project_root: Option<PathBuf>,

        /// Recompute every pin's sha256 from current content and rewrite
        /// .mdatron/pins.yaml (the single-command re-pin).
        #[arg(long = "update")]
        update: bool,

        /// With --update: report what would change without writing.
        #[arg(long = "dry-run", requires = "update")]
        dry_run: bool,

        /// Suppress stderr human-readable output.
        #[arg(long = "quiet", short = 'q')]
        quiet: bool,
    },

    /// Scaffold the `.mdatron/` skeleton and its managed manifest. Idempotent;
    /// refuses a hand-modified managed file with MDATRON-E0060.
    Init {
        /// Project root. Defaults to the current directory.
        #[arg(long = "project-root", value_name = "DIR")]
        project_root: Option<PathBuf>,

        /// Suppress stderr human-readable output.
        #[arg(long = "quiet", short = 'q')]
        quiet: bool,
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
    if !prefix
        .iter()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return Err(format!("code namespace must be [A-Z][A-Z0-9]*; got: {s}"));
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
            compact,
            quiet,
            changed,
        } => cmd_verify(
            project_root,
            schemas,
            patterns,
            files,
            json,
            compact,
            quiet,
            changed,
        ),
        Command::Explain {
            code,
            json,
            compact,
        } => cmd_explain(&code, json, compact),
        Command::Pin {
            project_root,
            update,
            dry_run,
            quiet,
        } => cmd_pin(project_root, update, dry_run, quiet),
        Command::Init {
            project_root,
            quiet,
        } => cmd_init(project_root, quiet),
    }
}

fn cmd_pin(project_root: Option<PathBuf>, update: bool, dry_run: bool, quiet: bool) -> ExitCode {
    let root = match project_root.map(Ok).unwrap_or_else(std::env::current_dir) {
        Ok(r) => r,
        Err(e) => {
            if !quiet {
                eprintln!("error[MDATRON-E0070]: cannot resolve project root: {e}");
            }
            return ExitCode::from(2);
        }
    };

    if update {
        match mdatron::pin::update(&root, dry_run) {
            Ok(changed) => {
                if !quiet {
                    let verb = if dry_run { "would re-pin" } else { "re-pinned" };
                    eprintln!("mdatron pin: {verb} {} entr(ies)", changed.len());
                    for (file, old, new) in &changed {
                        eprintln!(
                            "  {file}: {} -> {}",
                            &old[..old.len().min(12)],
                            &new[..new.len().min(12)]
                        );
                    }
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                if !quiet {
                    eprintln!("error[MDATRON-E0080]: pin update failed\n   = note: {e}");
                }
                ExitCode::from(2)
            }
        }
    } else {
        // Check mode: load + verify the pins, print findings rustc-shaped.
        match mdatron::pin::load(&root) {
            Ok(None) => {
                if !quiet {
                    eprintln!(
                        "mdatron pin: no pin record (.mdatron/pins.yaml absent); nothing to do"
                    );
                }
                ExitCode::SUCCESS
            }
            Ok(Some(loaded)) => {
                let mut findings = loaded.findings;
                mdatron::pin::check(&root, &loaded.pins, &mut findings);
                let errors = findings
                    .iter()
                    .filter(|f| f.severity == Severity::Error)
                    .count();
                if !quiet {
                    for f in &findings {
                        print_finding(f);
                    }
                    eprintln!(
                        "mdatron pin: {} pin(s) checked, {} error(s)",
                        loaded.pins.len(),
                        errors
                    );
                }
                if errors > 0 {
                    ExitCode::from(1)
                } else {
                    ExitCode::SUCCESS
                }
            }
            Err(e) => {
                if !quiet {
                    eprintln!("error[MDATRON-E0080]: pin check failed\n   = note: {e}");
                }
                ExitCode::from(2)
            }
        }
    }
}

fn cmd_init(project_root: Option<PathBuf>, quiet: bool) -> ExitCode {
    use mdatron::init::{drift_findings, init, InitError, InitOutcome};

    let root = match project_root.map(Ok).unwrap_or_else(std::env::current_dir) {
        Ok(r) => r,
        Err(e) => {
            if !quiet {
                eprintln!("error[MDATRON-E0070]: cannot resolve project root: {e}");
            }
            return ExitCode::from(2);
        }
    };

    match init(&root) {
        Ok(InitOutcome::Deployed { created }) => {
            if !quiet {
                eprintln!(
                    "mdatron init: deployed .mdatron/ ({} path(s))",
                    created.len()
                );
                for p in &created {
                    eprintln!("  + {p}");
                }
            }
            ExitCode::SUCCESS
        }
        Ok(InitOutcome::AlreadyInitialized) => {
            if !quiet {
                eprintln!("mdatron init: already initialized (no changes)");
            }
            ExitCode::SUCCESS
        }
        Err(InitError::Drift(drifts)) => {
            if !quiet {
                for f in drift_findings(&root, &drifts) {
                    print_finding(&f);
                }
                eprintln!(
                    "mdatron init: refused — {} managed file(s) drifted from the manifest",
                    drifts.len()
                );
            }
            ExitCode::from(1)
        }
        Err(e) => {
            if !quiet {
                eprintln!("error[MDATRON-E0080]: init failed\n   = note: {e}");
            }
            ExitCode::from(2)
        }
    }
}

/// Escape control and line/paragraph-separator code points in a visited-file
/// trace path so an adverse filename cannot inject a fake trace line (#102).
fn escape_trace_path(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control() || c == '\u{2028}' || c == '\u{2029}' {
                format!("\\u{{{:04x}}}", c as u32)
            } else {
                c.to_string()
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn cmd_verify(
    project_root: Option<PathBuf>,
    schemas: Option<PathBuf>,
    patterns: Option<PathBuf>,
    files: Vec<String>,
    json: bool,
    compact: bool,
    quiet: bool,
    changed: Option<PathBuf>,
) -> ExitCode {
    use mdatron::output::{Families, Output, PipelineStatus};

    let root = match project_root.map(Ok).unwrap_or_else(std::env::current_dir) {
        Ok(r) => r,
        Err(e) => {
            if !quiet {
                eprintln!("error[MDATRON-E0070]: cannot resolve project root: {e}");
            }
            return ExitCode::from(2);
        }
    };

    // The committed .mdatron/config.yaml's file_globs are the consumer-authored
    // jurisdiction (#77); an ABSENT config refuses (#80 D1) — jurisdiction is
    // explicit, never guessed. `--files` declares jurisdiction on the command
    // line for an ad-hoc run and needs no config. A present-but-malformed
    // config is a pipeline failure (loud), not a silent fallback.
    let config_result = if files.is_empty() {
        VerifyConfig::from_project(&root)
    } else {
        let mut c = VerifyConfig::new(&root);
        c.file_globs = files;
        Ok(c)
    };
    let (findings, families, pipeline_status, pipeline_err) = match config_result {
        Err(e) => (
            Vec::new(),
            Families::all_inactive(),
            PipelineStatus::Failed,
            Some(VerifyError::Config(e.to_string())),
        ),
        Ok(mut config) => {
            if let Some(s) = schemas {
                config.schemas_dir = s;
            }
            if let Some(p) = patterns {
                config.patterns_dir = p;
            }
            let result = match &changed {
                // Incremental (#102): verify the changed file + dependents and
                // emit the visited-file trace to stderr (control-escaped so an
                // adverse filename cannot inject trace lines). A .mdatron/ change
                // falls back to whole-tree (visited is None).
                Some(c) => verify_incremental(&config, c).map(|inc| {
                    if !quiet {
                        match &inc.visited {
                            Some(visited) => {
                                eprintln!(
                                    "mdatron verify --changed: {} file(s) verified (incremental scope)",
                                    visited.len()
                                );
                                for p in visited {
                                    eprintln!("  visited: {}", escape_trace_path(&p.to_string_lossy()));
                                }
                                // #102: a stale pin over an in-scope governed
                                // file IS included (by the pinned file's scope
                                // membership). The remaining omissions are the
                                // config-level checks located under .mdatron/
                                // (route-config, vocabulary-registry), which a
                                // .mdatron/ change forces whole-tree anyway.
                                eprintln!(
                                    "  note: incremental mode omits config-level findings under \
                                     .mdatron/ (route-config, vocabulary-registry); a stale pin is \
                                     included when its pinned file is in scope"
                                );
                            }
                            None => eprintln!(
                                "mdatron verify --changed: change under .mdatron/ — whole-tree run"
                            ),
                        }
                    }
                    inc.report
                }),
                None => verify_report(&config),
            };
            match result {
                Ok(r) => (r.findings, r.families, PipelineStatus::Ok, None),
                // A failed pipeline reports no family as invoked.
                Err(e) => (
                    Vec::new(),
                    Families::all_inactive(),
                    PipelineStatus::Failed,
                    Some(e),
                ),
            }
        }
    };

    // BC-2: files_checked count. v0.1.0 stub: 0 when pipeline failed; otherwise the
    // number of unique files referenced in findings (approximation pending a
    // verify()-level file-count return value in v0.1.x).
    let files_checked: u32 = if matches!(pipeline_status, PipelineStatus::Failed) {
        0
    } else {
        let mut seen: std::collections::BTreeSet<&std::path::Path> =
            std::collections::BTreeSet::new();
        for f in &findings {
            seen.insert(&f.location.file);
        }
        u32::try_from(seen.len()).unwrap_or(u32::MAX)
    };

    let output = Output::build(
        findings,
        files_checked,
        pipeline_status,
        families,
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

    // Compact agent-context form (#44, #80 D4): one 512-byte-capped block per
    // finding on stdout, blank-line separated (no line inside a block is empty,
    // so the delimiter is unambiguous). Pipeline failures render compact too.
    if compact {
        if let Some(e) = &pipeline_err {
            println!("{}", pipeline_error_finding(e).format_compact());
        } else {
            for (i, f) in output.findings.iter().enumerate() {
                if i > 0 {
                    println!();
                }
                println!("{}", f.format_compact());
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

/// Construct the Finding for a pipeline error so every output form renders it
/// through the same single-source-of-truth paths (format_tty / format_compact /
/// JSON envelope). Per crosslink #13 SE/F5.
fn pipeline_error_finding(e: &VerifyError) -> Finding {
    Finding {
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
        quoted: Vec::new(),
    }
}

fn print_pipeline_error(e: &VerifyError) {
    eprintln!("{}", pipeline_error_finding(e).format_tty());
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
