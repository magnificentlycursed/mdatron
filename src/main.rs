//! mdatron CLI binary.
//!
//! `mdatron verify` runs the full pipeline from `mdatron::verify`: loads schemas
//! from `<root>/.mdatron/schemas/`, patterns from `<root>/.mdatron/patterns/`, walks
//! the project per `--files` globs, and applies the schema family (JSON Schema) + the rule DSL
//! against every matched markdown file.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mdatron::diagnostic::{Finding, Location, QuotedRegion, Severity};
use mdatron::verify::{verify_incremental, verify_report, VerifyConfig, VerifyError};

mod explain;

#[derive(Parser, Debug)]
#[command(name = "mdatron", about, version, long_about = None)]
#[command(after_help = "The working loop:
  mdatron init                     scaffold .mdatron/ in a new project
  mdatron verify                   check the tree; rustc-shaped diagnostics
  mdatron explain <code>           the fix for any diagnostic (--list for all)
  mdatron verify --json            the versioned machine envelope (agents/CI)
  mdatron docs                     the bundled DSL reference (also: limits, faq, inputs)
  mdatron envelope-schema          the published envelope JSON Schema

Exit contract: 0 clean, 1 findings, 2 pipeline failure — anything else is an
engine defect; please report it.

Descended from Schematron (ISO/IEC 19757-3).")]
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

        /// File globs (relative to project root) — an explicit, ad-hoc
        /// jurisdiction. Without --files, jurisdiction comes from
        /// .mdatron/config.yaml's `file_globs`; an absent or globless config is
        /// refused (jurisdiction is never guessed).
        #[arg(long = "files", visible_alias = "file-globs", value_name = "GLOB", num_args = 1..)]
        files: Vec<String>,

        /// Emit the versioned JSON output envelope on stdout
        /// (schema: schema/mdatron-output.schema.json).
        #[arg(long = "json")]
        json: bool,

        /// Emit the compact agent-context form on stdout: one size-capped block
        /// per finding (512 bytes, a contract limit), adopter content
        /// prefix-marked, truncation at line boundaries with an elision marker.
        #[arg(long = "compact", conflicts_with = "json")]
        compact: bool,

        /// Suppress stderr human-readable diagnostics (machine-only consumers).
        #[arg(long = "quiet", short = 'q')]
        quiet: bool,

        /// Incremental mode: verify only this changed file and its
        /// transitive dependents, reporting the same findings a whole-tree run
        /// would for those files. A change under `.mdatron/` falls back to a
        /// whole-tree run. The verified (visited) file set prints to stderr.
        #[arg(long = "changed", value_name = "FILE")]
        changed: Option<PathBuf>,

        /// Escalate warnings to a failing exit: a
        /// warnings-only run exits `1` instead of `0`, so an adopter wiring
        /// `verify` as a hard gate need not parse the envelope. Errors still exit
        /// `1`, a clean run `0`, and a pipeline failure `2`, unchanged.
        #[arg(long = "deny-warnings", visible_alias = "strict")]
        deny_warnings: bool,

        /// Include run-phase wall-clock timings in the JSON envelope: an
        /// optional `timings` object with `total_ms`/`load_ms`/`capture_ms`/
        /// `check_ms`. Requires --json (timings ride only in the envelope) and
        /// conflicts with --compact. Off by default so the default envelope
        /// stays deterministic (timings are its sole non-deterministic zone).
        // The explicit --compact conflict closes clap's requires-waiver:
        // `compact` conflicts with `json`, and clap 4.5 waives an arg's
        // `requires` when another present arg conflicts the required arg away
        // — so `--timings --compact` was once accepted and silently dropped
        // timings (#175 cold-review R4/R6).
        #[arg(long = "timings", requires = "json", conflicts_with = "compact")]
        timings: bool,
    },

    /// Show extended documentation for a diagnostic code (rustc --explain pattern).
    Explain {
        /// The diagnostic code, e.g. MDATRON-E0001 or VSDD-E0017.
        /// Must match `^[A-Z][A-Z0-9]*-[ELW][0-9]{4}$` — operator-pasted from
        /// diagnostic output. Rejects ANSI escapes and shell-meta injection.
        #[arg(value_parser = parse_explain_code, required_unless_present = "list")]
        code: Option<String>,

        /// List every code in mdatron's explain catalog (`code — summary`),
        /// sorted, then exit — so an operator can discover codes without a full
        /// code in hand.
        #[arg(long = "list")]
        list: bool,

        /// Emit the explain page as a structured JSON object on stdout; with
        /// --list, the catalog as a JSON array of {code, summary} objects.
        /// Without this flag, the markdown body (or the plain list) is printed
        /// verbatim.
        #[arg(long = "json")]
        json: bool,

        /// Emit a one-line compact form: `<code> <severity>: <summary> —
        /// <first-sentence-of-fix>`. Suitable for agent-loop hot paths +
        /// PostToolUse hook context budgets.
        #[arg(long = "compact", conflicts_with = "json")]
        compact: bool,
    },

    /// Verify the pin record, or recompute it with --update.
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

    /// Scaffold `.mdatron/`: the schemas/ and patterns/ directories, a seeded
    /// config.yaml (adopter-owned from then on), the init manifest, and four
    /// inert *.example templates (routes, pins, vocabulary, code-catalogs) that
    /// activate nothing until copied to their real names. Creates no family
    /// file itself. Idempotent; refuses a hand-modified managed file with
    /// MDATRON-E0060. `mdatron docs inputs` documents every input file.
    Init {
        /// Project root. Defaults to the current directory.
        #[arg(long = "project-root", value_name = "DIR")]
        project_root: Option<PathBuf>,

        /// Suppress stderr human-readable output.
        #[arg(long = "quiet", short = 'q')]
        quiet: bool,
    },

    /// Print the published `verify --json` output-envelope JSON Schema on stdout
    /// — so a binary-only consumer can pin and validate against it without
    /// a repo checkout. Kept in lockstep with `mdatron_output_version`. (`schema`
    /// is the retired 0.6.0 name, kept as an alias — the bare word otherwise
    /// means the frontmatter schema family.)
    #[command(name = "envelope-schema", visible_alias = "schema")]
    EnvelopeSchema,

    /// Print bundled documentation on stdout: the
    /// complete DSL reference (default), the declared-limits table, the FAQ,
    /// or the adopter-input reference (one section per .mdatron/ input:
    /// shape, keys, activation, scope, codes) — the same files the crate ships, so a binary-only `cargo install`
    /// consumer reads them without a repo checkout (`mdatron docs | less`).
    Docs {
        /// Which document to print.
        #[arg(value_parser = ["dsl", "limits", "faq", "inputs"], default_value = "dsl")]
        topic: String,
    },
}

fn parse_explain_code(s: &str) -> Result<String, String> {
    // Short form (#117, vsdd W4): a bare `E0050` / `W0043` / `L0001` normalizes to
    // mdatron's namespace, so an operator need not paste the full `MDATRON-`
    // prefix. Anything already namespaced falls through to full validation.
    if !s.contains('-')
        && s.len() == 5
        && matches!(s.as_bytes().first(), Some(b'E' | b'L' | b'W'))
        && s.as_bytes()[1..].iter().all(u8::is_ascii_digit)
    {
        return Ok(format!("MDATRON-{s}"));
    }
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
            deny_warnings,
            timings,
        } => cmd_verify(
            project_root,
            schemas,
            patterns,
            files,
            json,
            compact,
            quiet,
            changed,
            deny_warnings,
            timings,
        ),
        Command::Explain {
            code,
            list,
            json,
            compact,
        } => cmd_explain(code.as_deref(), list, json, compact),
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
        Command::EnvelopeSchema => cmd_schema(),
        Command::Docs { topic } => cmd_docs(&topic),
    }
}

/// Print the embedded output-envelope schema to stdout (#127). A binary-only
/// consumer can `mdatron envelope-schema > mdatron-output.schema.json` and validate the
/// `verify --json` envelope against it.
fn cmd_schema() -> ExitCode {
    print_page(mdatron::output::OUTPUT_SCHEMA)
}

/// Write a full page to stdout, tolerating a closed pipe (consolidated-review
/// F4): `mdatron docs | less` quit early — the README's own documented usage —
/// used to panic on EPIPE and exit 101, violating the 0/1/2 exit contract the
/// same branch pins. A broken pipe on a print-only surface is the CONSUMER
/// saying "enough": graceful success, not an engine defect. Any other write
/// error stays loud (exit 2).
fn print_page(body: &str) -> ExitCode {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    match out.write_all(body.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!(
                "error[MDATRON-E0080]: pipeline-orchestration-failure\n   = note: writing to stdout failed: {}",
                stderr_safe(&e, &[])
            );
            ExitCode::from(2)
        }
    }
}

/// Print a bundled documentation file to stdout (#180 discoverability). The
/// files are embedded at build time from the same paths the crate package
/// ships, so a `cargo install` consumer reads them with no repo checkout.
/// clap's value_parser closes the topic set, so the match is total.
fn cmd_docs(topic: &str) -> ExitCode {
    let body = match topic {
        "limits" => include_str!("../docs/limits.md"),
        "faq" => include_str!("../docs/faq.md"),
        "inputs" => include_str!("../docs/inputs.md"),
        _ => include_str!("../docs/dsl-reference.md"),
    };
    print_page(body)
}

/// Resolve the as-passed project root to its canonical absolute form ONCE,
/// before any path is derived from it, so every derived path — the
/// `.mdatron/` dirs the config joins, the loaders' free-form messages, the
/// pipeline's own root — is canonical-rooted and relativizes
/// deterministically whatever spelling (`.`, `docs`, `/abs/docs`) the caller
/// used, and so the confine walk never sees a reparse point AS the root: on
/// Windows the walk refuses one, so a junction-rooted project
/// (`mklink /J C:\proj D:\real`) must be resolved here, for EVERY subcommand
/// (#64 cold-review W2; the same helper #185 H1 adds on main). A root that
/// does not exist cannot be canonicalized; it is made absolute lexically
/// instead (cwd-joined, `.` components dropped), and only if even that fails
/// (no cwd) is the spelling kept as passed.
fn canonical_project_root(root: PathBuf) -> PathBuf {
    root.canonicalize()
        .or_else(|_| std::path::absolute(&root))
        .unwrap_or(root)
}

fn cmd_pin(project_root: Option<PathBuf>, update: bool, dry_run: bool, quiet: bool) -> ExitCode {
    let root = match project_root.map(Ok).unwrap_or_else(std::env::current_dir) {
        Ok(r) => r,
        Err(e) => {
            if !quiet {
                // #167: no resolved root here — escape only.
                eprintln!(
                    "error[MDATRON-E0070]: cannot resolve project root: {}",
                    stderr_safe(&e, &[])
                );
            }
            return ExitCode::from(2);
        }
    };
    // #64 W2: canonical root before ANY derived path or confine walk.
    let root = canonical_project_root(root);

    if update {
        match mdatron::pin::update(&root, dry_run) {
            Ok(changed) => {
                if !quiet {
                    let verb = if dry_run { "would re-pin" } else { "re-pinned" };
                    eprintln!("mdatron pin: {verb} {} entr(ies)", changed.len());
                    // GH #48 finding 4: `file` and the recorded sha are adopter-
                    // authored (pins.yaml, not validated as hex or printable) —
                    // truncate char-boundary-safe (the #165 byte-slice panic
                    // class) and escape control bytes before stderr, the same
                    // marking discipline the finding renderer applies.
                    use mdatron::diagnostic::escape_path_text;
                    for (file, old, new) in &changed {
                        eprintln!(
                            "  {}: {} -> {}",
                            escape_path_text(file),
                            escape_path_text(mdatron::init::short(old)),
                            escape_path_text(mdatron::init::short(new))
                        );
                    }
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                if !quiet {
                    // #167: pin errors interpolate adopter pins.yaml values —
                    // escape at the print boundary and strip the resolved root
                    // (DEF4) from the rendered note.
                    eprintln!(
                        "error[MDATRON-E0080]: pipeline-orchestration-failure\n   = note: pin update failed: {}",
                        stderr_safe(&e, &[root.as_path()])
                    );
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
                // Standalone `mdatron pin` builds its own capture (#103): the
                // same read-once, confined, bounded path the verify pipeline
                // uses — pin::check never touches the filesystem itself.
                let mut snapshot = mdatron::snapshot::Snapshot::new(
                    mdatron::verify::MAX_FILE_BYTES,
                    mdatron::verify::MAX_AGGREGATE_BYTES,
                );
                for pin in &loaded.pins {
                    if let Ok(confined) =
                        mdatron::confine::confine_lexically(std::path::Path::new(&pin.file))
                    {
                        match snapshot.capture(&root, &confined) {
                            // Config-scoped posture: an oversized pinned file
                            // is the declared-bounds abort, as in verify.
                            Ok(mdatron::snapshot::Captured::TooLarge { limit, dimension }) => {
                                let e = mdatron::snapshot::Snapshot::too_large_error(
                                    confined.as_path(),
                                    *limit,
                                    *dimension,
                                );
                                if !quiet {
                                    eprintln!(
                                        "error[MDATRON-E0080]: pipeline-orchestration-failure\n   = note: pin check failed: {}",
                                        stderr_safe(&e, &[root.as_path()])
                                    );
                                }
                                return ExitCode::from(2);
                            }
                            Ok(_) => {}
                            Err(e) => {
                                if !quiet {
                                    eprintln!(
                                        "error[MDATRON-E0080]: pipeline-orchestration-failure\n   = note: pin check failed: {}",
                                        stderr_safe(&e, &[root.as_path()])
                                    );
                                }
                                return ExitCode::from(2);
                            }
                        }
                    }
                }
                mdatron::pin::check(&root, &loaded.pins, &snapshot, &mut findings);
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
                    // #167: pin::load errors interpolate the pins.yaml path —
                    // root-relativize (DEF4) + escape at the print boundary.
                    eprintln!(
                        "error[MDATRON-E0080]: pipeline-orchestration-failure\n   = note: pin check failed: {}",
                        stderr_safe(&e, &[root.as_path()])
                    );
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
                // #167: no resolved root here — escape only.
                eprintln!(
                    "error[MDATRON-E0070]: cannot resolve project root: {}",
                    stderr_safe(&e, &[])
                );
            }
            return ExitCode::from(2);
        }
    };
    // #64 W2: canonical root before ANY derived path or confine walk.
    let root = canonical_project_root(root);

    match init(&root) {
        Ok(InitOutcome::Deployed { created }) => {
            if !quiet {
                eprintln!(
                    "mdatron init: deployed .mdatron/ ({} path(s))",
                    created.len()
                );
                for p in &created {
                    // #167 audit find: a repaired MANAGED path echoes the
                    // manifest's (adopter-editable) entry text — escape it.
                    eprintln!("  + {}", stderr_safe(p, &[]));
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
                // #167: InitError::Io/ManifestParse interpolate manifest-derived
                // paths — root-relativize (DEF4) + escape at the print boundary.
                eprintln!(
                    "error[MDATRON-E0080]: pipeline-orchestration-failure\n   = note: init failed: {}",
                    stderr_safe(&e, &[root.as_path()])
                );
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
    deny_warnings: bool,
    timings: bool,
) -> ExitCode {
    use mdatron::output::{Families, Output, PipelineError, PipelineStatus};

    let root = match project_root.map(Ok).unwrap_or_else(std::env::current_dir) {
        Ok(r) => r,
        Err(e) => {
            if !quiet {
                // #167: no resolved root here — escape only.
                eprintln!(
                    "error[MDATRON-E0070]: cannot resolve project root: {}",
                    stderr_safe(&e, &[])
                );
            }
            return ExitCode::from(2);
        }
    };
    // #185 cold-review H1: resolve the root to its canonical absolute form ONCE,
    // before ANY path is derived from it. `VerifyConfig::new` joins schemas_dir /
    // patterns_dir from the root AS PASSED, and the loaders (`config::load`,
    // `route::load`, ...) bake that spelling into their free-form messages,
    // while the pipeline canonicalizes only its own copy later — so a relative
    // root (`.`, `docs`) produced error paths (`./.mdatron/schemas`) that the
    // canonical-root `strip_prefix` relativizer could not strip: the same failure
    // rendered differently by root spelling. The published 0.6.0 hid that behind
    // the substring catch-all below (and under `docs` LEAKED the host path when
    // the raw `docs/` strip ate a segment of the canonical path first). With one
    // canonical root every derived path is canonical-rooted and relativizes
    // deterministically; `--files` globs and `--changed` were always resolved
    // against the pipeline's canonical root, so they are unaffected.
    // #64 W2: the same canonical root fronts every confine walk (pin, init).
    let root = canonical_project_root(root);

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
    let (mut findings, families, files_checked, pipeline_status, pipeline_err, inputs, run_timings) =
        match config_result {
            Err(e) => (
                Vec::new(),
                Families::all_inactive(),
                0,
                PipelineStatus::Failed,
                Some(VerifyError::Config(e.to_string())),
                std::collections::BTreeMap::new(),
                None,
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
                    Ok(r) => (
                        r.findings,
                        r.families,
                        r.files_checked,
                        PipelineStatus::Ok,
                        None,
                        r.inputs,
                        Some(r.timings),
                    ),
                    // A failed pipeline reports no family as invoked, and its
                    // #176 lineage is ALWAYS empty — some inputs may have been
                    // read before the failure, but partial lineage is
                    // deliberately not attested (cold-review R3).
                    Err(e) => (
                        Vec::new(),
                        Families::all_inactive(),
                        0,
                        PipelineStatus::Failed,
                        Some(e),
                        std::collections::BTreeMap::new(),
                        None,
                    ),
                }
            }
        };

    // #113 (vsdd item 7): advertise `mdatron explain <code>` only when a page
    // actually resolves. A pattern-rule finding carries an adopter-defined code
    // (e.g. VSDD-Exxxx) mdatron has no page for; a dead pointer the same binary
    // rejects is worse than none. The explain catalog lives in this binary, so
    // this is where the pointer is validated — nulling it corrects the tty,
    // compact, and JSON forms alike (all read finding.explain_ref).
    for f in &mut findings {
        let unresolvable = f
            .explain_ref
            .as_deref()
            .is_some_and(|code| explain::lookup(code).is_none());
        if unresolvable {
            f.explain_ref = None;
        }
    }

    // DEF4 completion (#134, roast B1): relativize any absolute path the failure
    // carries against the canonical root (the one every derived path is rooted
    // at, see above), so pipeline_error.message and the stderr note below do
    // not leak the host layout — matching the relativized findings.
    let pipeline_err = pipeline_err.map(|e| e.relativize_paths(&root));

    // A failed pipeline carries its reason INTO the envelope (#112): the stderr
    // note is suppressed by --quiet, so a --json --quiet consumer needs the cause
    // in-band. `kind` disambiguates E0080's senses.
    //
    // Chokepoint relativization (roast round-3 A): `relativize_paths` covers the
    // structured `{path}` variants, but several load-path errors bake an absolute
    // path into a FREE-FORM `Config` message (a malformed config/pin/vocab/route
    // file: `cannot parse '<abs>'`). Strip any root prefix from the final message
    // as a catch-all so no `pipeline_error.message` leaks the host layout — a
    // completeness guarantee no per-source fix can promise for future messages.
    // Uses a trailing-separator match, so it relativizes `<root>/x` to `x`. Only
    // the canonical root is offered (#185 H1): the as-passed spelling no longer
    // reaches any message, and a relative spelling's short prefix (`docs/`) used
    // to mangle unrelated adopter text mid-string (#185 L1).
    let relativize_message = |msg: String| relativize_root_prefix(msg, &[root.as_path()]);
    let pipeline_error = pipeline_err.as_ref().map(|e| PipelineError {
        code: "MDATRON-E0080".into(),
        kind: e.kind().into(),
        message: relativize_message(e.to_string()),
    });

    // files_checked (#105) is the true count of files this run VALIDATED,
    // threaded from the report — a clean run over N files reports N, not a count
    // of files that happened to produce findings (the old v0.1.0 stub).
    let output = Output::build(
        findings,
        files_checked,
        pipeline_status,
        pipeline_error,
        families,
        env!("CARGO_PKG_VERSION"),
    )
    .with_inputs(inputs)
    // #175: timings are the envelope's sole non-deterministic zone — emitted
    // only under --timings, so the default envelope stays byte-identical
    // across runs on an unchanged tree.
    .with_timings(if timings { run_timings } else { None });

    // BC-5 stream contract: --json puts the output on stdout; otherwise diagnostics
    // are rustc-shaped on stderr.
    if json {
        match mdatron::output::to_js_safe_json(&output) {
            Ok(line) => println!("{line}"),
            Err(e) => {
                if !quiet {
                    // #167: a serde error's Display can echo data bytes —
                    // escape + root-strip at the print boundary like every
                    // other non-Finding stderr note.
                    eprintln!(
                        "error[MDATRON-E0080]: pipeline-orchestration-failure\n   = note: output serialization failed: {}",
                        stderr_safe(&e, &[root.as_path()])
                    );
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
            println!(
                "{}",
                pipeline_error_finding(e, &[root.as_path()]).format_compact()
            );
        } else {
            for (i, f) in output.findings.iter().enumerate() {
                if i > 0 {
                    println!();
                }
                println!("{}", f.format_compact());
            }
        }
    }

    // Under --json the envelope on stdout is the authoritative output; re-rendering
    // findings (and the pipeline error, now carried in the envelope, #112) as human
    // TTY text on stderr is a redundant ~1.7x token cost for a machine consumer
    // (#117, vsdd W4). The human block is emitted only when NOT in --json mode.
    if !quiet && !json {
        if let Some(e) = &pipeline_err {
            print_pipeline_error(e, &[root.as_path()]);
        } else {
            for f in &output.findings {
                print_finding(f);
            }
            if output.summary.error_count == 0 && output.summary.warning_count == 0 {
                eprintln!("mdatron verify: clean");
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

    // #121: --deny-warnings escalates a warnings-only run (exit 0) to exit 1, so
    // a hard gate need not parse the envelope. Errors (1) and pipeline failure
    // (2) are unchanged; the escalation applies only to an otherwise-clean run
    // that carries warnings.
    let mut code = output.derive_exit_code();
    if deny_warnings && code == 0 && output.summary.warning_count > 0 {
        code = 1;
    }
    ExitCode::from(code)
}

fn print_finding(f: &Finding) {
    // Delegate to Finding::format_tty so the engine + CLI render TTY
    // diagnostics through one code path. Per Phase 1a behavioral spec
    // (vsdd-cli/docs/refactor/phase-2-mdatron-json/phase-1a-behavioral-spec.md).
    eprintln!("{}", f.format_tty());
}

/// Strip any ROOTED `<root>/` prefix from a free-form error message so no
/// agent-facing render leaks the host layout (the DEF4 contract, #134/#140). A
/// trailing-separator match relativizes `<root>/x` to `x`. Callers pass the
/// canonical root only (#185 H1), which is always rooted, so the guard is
/// belt-and-braces: a relative spelling must never reach the replace — a
/// `--project-root docs` once yielded the prefix `docs/`, which the substring
/// replace cut out of arbitrary adopter content mid-string (an invalid glob
/// `docs/a**b` rendered as `a**b`, #185 L1). `has_root` rather than
/// `is_absolute` (#185 H3): a drive-less rooted Windows path (`\dir\proj`)
/// names a host location and must be stripped, and no rooted path is the kind
/// of short prefix that mangles adopter text. Shared by the JSON envelope and
/// the compact/tty pipeline-error render so all three forms are
/// host-layout-free.
fn relativize_root_prefix(mut msg: String, roots: &[&Path]) -> String {
    for r in roots {
        if !r.has_root() {
            continue;
        }
        let pref = format!("{}{}", r.to_string_lossy(), std::path::MAIN_SEPARATOR);
        msg = msg.replace(&pref, "");
    }
    msg
}

/// Render a non-Finding error (or any value that can carry adopter/OS bytes)
/// for a raw stderr line (#167): strip the resolved project root(s) so the
/// note does not leak the host layout (the DEF4 contract, #134), then escape
/// control bytes to inert `\xNN` at the PRINT boundary (#165 marking
/// discipline — no construction-site sweep can be complete). Pass no roots
/// where none is resolved (the E0070 sites). Finding-rendered lines never come
/// through here — `format_tty`/`format_compact` are safe by construction.
fn stderr_safe(text: impl std::fmt::Display, roots: &[&Path]) -> String {
    mdatron::diagnostic::escape_path_text(&relativize_root_prefix(text.to_string(), roots))
}

/// Construct the Finding for a pipeline error so every output form renders it
/// through the same single-source-of-truth paths (format_tty / format_compact /
/// JSON envelope). Per crosslink #13 SE/F5.
fn pipeline_error_finding(e: &VerifyError, roots: &[&Path]) -> Finding {
    // #165 marking discipline: the message is engine-authored per error kind; the
    // full `Display` detail — which interpolates adopter pattern/rule ids, globs,
    // file paths, and parser output — rides in an escaped `quoted[]` region, so the
    // agent-facing compact/tty renders can't be injected by a crafted id, glob,
    // filename, or parser byte. The detail is root-relativized exactly as the JSON
    // `pipeline_error.message` is (the DEF4 host-layout contract, #134): the
    // structured `{path}` variants via `relativize_paths` upstream, and any
    // free-form `Config` absolute path stripped here — so compact/tty match the
    // envelope. (The JSON envelope is serde-escaped independently.)
    let kind: &str = match e {
        VerifyError::Io { .. } => "an input could not be read during verification",
        VerifyError::SchemaLoad { .. } => "a schema file failed to load",
        VerifyError::PatternLoad { .. } => "a pattern file failed to load",
        VerifyError::IndexBuild(_) => "a cross-file index failed to build",
        VerifyError::ExprParse { .. } => "a pattern rule expression failed to parse",
        VerifyError::Eval { .. } => "a pattern rule expression failed to evaluate",
        VerifyError::Glob(_) => "a configured glob pattern is invalid",
        VerifyError::Config(_) => "the run could not be configured",
        VerifyError::Frontmatter { .. } => "a file's frontmatter failed to parse",
        VerifyError::BoundExceeded { .. } => "a declared resource bound was exceeded",
    };
    // The TTY/compact render of a pipeline failure carries the catalog headline
    // as its summary like every other finding (#204 round-2 m8: the new
    // summary<->catalog tripwire caught this one prose summary); the failure
    // sense rides in the message.
    Finding {
        code: "MDATRON-E0080".into(),
        severity: Severity::Error,
        summary: "pipeline-orchestration-failure".into(),
        message: format!("verify pipeline failed: {kind}"),
        help: None,
        location: Location {
            file: std::path::PathBuf::new(),
            line: 0,
            column: 0,
        },
        explain_ref: None,
        quoted: vec![QuotedRegion {
            platform_variant: true,
            label: "detail".into(),
            content: relativize_root_prefix(e.to_string(), roots),
        }],
    }
}

fn print_pipeline_error(e: &VerifyError, roots: &[&Path]) {
    eprintln!("{}", pipeline_error_finding(e, roots).format_tty());
}

fn cmd_explain(code: Option<&str>, list: bool, json: bool, compact: bool) -> ExitCode {
    // `--list` enumerates the catalog and exits (#117, vsdd W4). A malformed
    // embedded catalog is LOUD (GH #48 lane G) — a silently empty list would
    // read as "no codes exist".
    if list {
        match explain::catalog() {
            Ok(entries) => {
                if json {
                    // #180: --list previously ignored --json silently — the
                    // same silent-no-op class the --timings gate closed.
                    let arr: Vec<serde_json::Value> = entries
                        .iter()
                        .map(|(c, s)| serde_json::json!({ "code": c, "summary": s }))
                        .collect();
                    match mdatron::output::to_js_safe_json(&arr) {
                        Ok(line) => return print_page(&format!("{line}\n")),
                        Err(e) => {
                            eprintln!(
                                "error[MDATRON-E0080]: pipeline-orchestration-failure\n   = note: output serialization failed: {}",
                                stderr_safe(&e, &[])
                            );
                            return ExitCode::from(2);
                        }
                    }
                } else if compact {
                    // One compact line per code (the same form `explain
                    // --compact <code>` emits), so an agent can bulk-load the
                    // catalog into a context budget.
                    let mut buf = String::new();
                    for (c, _) in &entries {
                        if let Some(line) = explain::lookup_compact(c) {
                            buf.push_str(&line);
                            buf.push('\n');
                        }
                    }
                    return print_page(&buf);
                } else {
                    let mut buf = String::new();
                    for (c, summary) in entries {
                        buf.push_str(&format!("{c} — {summary}\n"));
                    }
                    return print_page(&buf);
                }
            }
            Err(e) => {
                // #167: routed through the print-boundary escape for
                // uniformity (no resolved root in this command).
                eprintln!(
                    "error[MDATRON-E0080]: pipeline-orchestration-failure\n   = note: explain --list failed: {}",
                    stderr_safe(&e, &[])
                );
                return ExitCode::from(2);
            }
        }
    }
    // clap's `required_unless_present = "list"` guarantees a code here.
    let code = code.expect("a code is required unless --list");
    if compact {
        if let Some(line) = explain::lookup_compact(code) {
            return print_page(&format!("{line}\n"));
        }
    } else if json {
        if let Some(structured) = explain::lookup_structured(code) {
            match mdatron::output::to_js_safe_json(&structured) {
                Ok(line) => {
                    return print_page(&format!("{line}\n"));
                }
                Err(e) => {
                    eprintln!(
                        "error[MDATRON-E0080]: pipeline-orchestration-failure\n   = note: output serialization failed: {}",
                        stderr_safe(&e, &[])
                    );
                    return ExitCode::from(2);
                }
            }
        }
    } else if let Some(page) = explain::lookup(code) {
        // Normalize trailing whitespace + write exactly one trailing newline.
        // Per crosslink #13 SE/F1.
        let mut buf = format!("{}\n", page.trim_end());
        // Per crosslink #12 UX/F1: if this code has a migration note (its
        // semantic shifted across emission sites), surface it AFTER the
        // page so operators recalling the prior meaning see the bridge.
        if let Some(note) = explain::migration_note(code) {
            buf.push_str(&format!("\n## Migration note\n\n{note}\n"));
        }
        return print_page(&buf);
    }
    // #167 audit find: `code` is operator argv echoed to stderr — escape it at
    // the print boundary like every other non-engine interpolation.
    let code = stderr_safe(code, &[]);
    if explain::is_mdatron_namespace(&code) {
        eprintln!(
            "error[MDATRON-E0080]: pipeline-orchestration-failure\n   \
             = note: no explain page found for {code}: the explain catalog grows \
             by one entry per emitted code, and {code} is not in it\n   \
             = help: see DESIGN.md \u{00A7} Diagnostics are a versioned contract for \
             the structural meaning of unimplemented codes"
        );
        return ExitCode::from(2);
    }
    // Non-MDATRON namespace (e.g., VSDD-Exxxx): mdatron's catalog covers
    // its own namespace only per phase-0-output-format/DESIGN.md
    // namespace-separation contract.
    eprintln!(
        "error[MDATRON-E0080]: pipeline-orchestration-failure\n   \
         = note: {code} is outside the mdatron namespace: mdatron explain covers \
         MDATRON-Exxxx codes only; \
         see `vsdd explain {code}` for the VSDD namespace"
    );
    ExitCode::from(2)
}
