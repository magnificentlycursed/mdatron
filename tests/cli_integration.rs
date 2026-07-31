//! CLI integration tests for Phase 2 of the binary-first refactor
//! (crosslink #13). Covers the operator-facing CLI surfaces added in
//! Phase 2 — `mdatron explain CODE` against the v0.1.0 embedded catalog,
//! the `= explain:` line in TTY diagnostics, `mdatron/README.md`
//! presence + structural shape + round-trip, and the drive-by
//! `--quiet --json` flag-combination coverage from Phase 0 BC-5
//! row 3 that `tests/output_format.rs` did not yet exercise.
//!
//! Specification anchor:
//!   vsdd-cli/docs/refactor/phase-2-mdatron-json/phase-1c-decomposition.md
//!
//! Test groups (mirroring the Phase 1c Red Gate seeds):
//!   1. `--json` finalization — the rendered `= explain:` line
//!   2. `mdatron explain CODE` — five baseline codes + not-found + namespace
//!   3. README presence + structure + round-trip
//!   4. Drive-by `--quiet` / `--quiet --json` coverage
//!
//! Phase 2a Red Gate: every test below fails on current `main` —
//! the explain catalog doesn't exist; the README doesn't exist; the
//! `= explain:` line is not rendered by the CLI's `print_finding`.
//! Phase 2b turns the gate green.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

// ── Test fixtures ──────────────────────────────────────────────────────────────

struct TempProject(PathBuf);

impl TempProject {
    fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("mdatron-cli-int-{label}-{nanos}"));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        // #80 D1: verify refuses a config-less tree; fixtures declare their
        // jurisdiction explicitly like any real adopter tree.
        fs::create_dir_all(path.join(".mdatron")).unwrap();
        fs::write(
            path.join(".mdatron/config.yaml"),
            "file_globs:\n  - \"**/*.md\"\n",
        )
        .unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let p = self.0.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, content).unwrap();
    }

    fn seed_blog_schema(&self) {
        self.write(
            ".mdatron/schemas/blog.json",
            r#"{"type":"object","required":["schema_class"],"properties":{"schema_class":{"const":"blog"}},"additionalProperties":false}"#,
        );
    }

    fn seed_clean_md(&self, name: &str) {
        self.write(name, "---\nschema_class: blog\n---\n# ok\n");
    }

    fn seed_schema_violation_md(&self, name: &str) {
        // Triggers MDATRON-E0050 (frontmatter-schema-violation): `extra` is
        // not allowed by the blog schema.
        self.write(name, "---\nschema_class: blog\nextra: not allowed\n---\n");
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn mdatron_bin() -> PathBuf {
    // CARGO_BIN_EXE_mdatron resolves to the cargo-built binary regardless of
    // profile (debug vs release) or workspace layout — survives Phase 4's
    // single-crate collapse + sandboxed agent-loop runs. Per crosslink #13
    // PE/F5 + AIE/F4 convergence.
    PathBuf::from(env!("CARGO_BIN_EXE_mdatron"))
}

/// The mdatron repo root. Single-crate layout (#81): the manifest dir IS the
/// repo root.
fn mdatron_repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> Output {
    Command::new(mdatron_bin())
        .args(args)
        .output()
        .expect("mdatron binary executes")
}

// #80 D1: a config-less tree refuses with "no jurisdiction declared"
// (exit 2, pipeline failure) — and an explicit --files run is the sanctioned
// ad-hoc escape that needs no config.
#[test]
fn verify_refuses_configless_tree_but_files_flag_escapes() {
    let proj = TempProject::new("d1-refusal");
    std::fs::create_dir_all(proj.path().join(".mdatron/schemas")).unwrap();
    std::fs::remove_file(proj.path().join(".mdatron/config.yaml")).unwrap();
    proj.write("doc.md", "# prose\n");

    let out = run_verify_tty(&proj);
    assert_eq!(
        out.status.code(),
        Some(2),
        "absent config is a pipeline failure"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no jurisdiction declared"),
        "refusal names the problem: {stderr}"
    );

    let out = Command::new(mdatron_bin())
        .args(["verify", "--project-root"])
        .arg(proj.path())
        .args(["--files", "**/*.md"])
        .output()
        .expect("mdatron binary executes");
    assert_eq!(
        out.status.code(),
        Some(0),
        "--files declares jurisdiction ad hoc: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// #112 (vsdd items 5 + 6): a pipeline failure under `--json --quiet` (the
// documented CI mode) must carry the reason IN the envelope — the stderr note is
// suppressed by `-q`, so a machine consumer got `findings: []` with no cause. A
// `pipeline_error{code,kind,message}` fixes it, and `kind` disambiguates the
// many senses E0080 alone conflated.
#[test]
fn pipeline_failure_json_quiet_carries_structured_reason() {
    let proj = TempProject::new("pipeline-err");
    std::fs::create_dir_all(proj.path().join(".mdatron/schemas")).unwrap();
    std::fs::remove_file(proj.path().join(".mdatron/config.yaml")).unwrap();
    proj.write("doc.md", "# prose\n");

    let out = Command::new(mdatron_bin())
        .args(["verify", "--project-root"])
        .arg(proj.path())
        .args(["--json", "-q"])
        .output()
        .expect("mdatron binary executes");
    assert_eq!(out.status.code(), Some(2), "pipeline failure exits 2");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let env: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout is a JSON envelope: {e}; got {stdout:?}"));
    assert_eq!(env["pipeline_status"], "failed");
    let pe = &env["pipeline_error"];
    assert!(
        pe.is_object(),
        "the envelope must carry pipeline_error even under -q; got {env}"
    );
    assert_eq!(pe["code"], "MDATRON-E0080");
    assert_eq!(
        pe["kind"], "config",
        "the config-load failure kind is disambiguated"
    );
    assert!(
        pe["message"]
            .as_str()
            .unwrap_or("")
            .contains("no jurisdiction declared"),
        "the reason rides in the envelope (survives -q): {pe}"
    );
}

// roast round-3 A: a load-path PARSE error (malformed config) bakes an absolute
// path into a free-form Config message that `relativize_paths` cannot reach; the
// cmd_verify chokepoint strips the root so `pipeline_error.message` stays host-
// layout-free — the DEF4 envelope-reproducibility contract, end to end.
#[test]
fn pipeline_error_message_relativizes_a_parse_error_path() {
    let proj = TempProject::new("bad-config");
    // malformed YAML (unclosed flow sequence) -> a config-load parse error
    proj.write(".mdatron/config.yaml", "file_globs: [unclosed\n");
    proj.write("doc.md", "# prose\n");

    let out = Command::new(mdatron_bin())
        .args(["verify", "--project-root"])
        .arg(proj.path())
        .args(["--json", "-q"])
        .output()
        .expect("mdatron binary executes");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let env: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout is a JSON envelope: {e}; got {stdout:?}"));
    let msg = env["pipeline_error"]["message"].as_str().unwrap_or("");
    let abs = proj.path().to_string_lossy().into_owned();
    assert!(
        !msg.contains(&abs),
        "no absolute host path in pipeline_error.message; got {msg:?}"
    );
    assert!(
        msg.contains("config.yaml"),
        "still names the config file (relatively); got {msg:?}"
    );
}

// #121 (requested by vsdd-cli): `--deny-warnings` escalates a warnings-only run
// (exit 0) to a failing exit 1, so a hard-gate consumer need not parse the
// envelope. Errors and pipeline failures are unaffected.
#[test]
fn deny_warnings_escalates_a_warnings_only_run() {
    let proj = TempProject::new("deny-warn");
    proj.seed_blog_schema();
    // A dead `typo-dir/**/*.md` glob emits W0046 (a warning); doc.md is clean.
    proj.write(
        ".mdatron/config.yaml",
        "file_globs:\n  - \"**/*.md\"\n  - \"typo-dir/**/*.md\"\n",
    );
    proj.write("doc.md", "# prose\n");
    let out = run_verify(&proj, &[]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "warnings-only exits 0 by default: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = run_verify(&proj, &["--deny-warnings"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "warnings escalate to exit 1 under --deny-warnings"
    );
}

// #127 (roast SHO10): `mdatron schema` prints the published envelope schema so a
// binary-only consumer can pin/validate without a repo checkout.
#[test]
fn schema_subcommand_prints_the_published_envelope_schema() {
    let out = run(&["schema"]);
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("valid JSON Schema on stdout");
    assert_eq!(
        v["properties"]["mdatron_output_version"]["const"], "2.1.0",
        "schema is in lockstep with OUTPUT_VERSION"
    );
    assert!(v["$id"].as_str().unwrap().contains("mdatron-output"));
}

// #117 (vsdd W4): `mdatron explain` accepts a short code (`E0050` -> the MDATRON
// namespace) and `--list` enumerates the catalog — so an operator can discover
// and recall codes without pasting a full namespaced code.
#[test]
fn explain_accepts_short_code_and_lists_catalog() {
    let out = run(&["explain", "E0050"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "short code E0050 resolves: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("MDATRON-E0050"),
        "the E0050 page renders"
    );

    let out = run(&["explain", "--list"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("MDATRON-E0050") && stdout.contains("MDATRON-W0047"),
        "the catalog listing includes known codes: {stdout}"
    );
    assert!(
        stdout.lines().count() > 10,
        "the listing enumerates the catalog"
    );
}

// #117 (vsdd item, W4): `--json` without `-q` used to emit the machine envelope
// on stdout AND re-render every finding as human TTY text on stderr — a ~1.7x
// token cost for an agent capturing both streams, and fully redundant with the
// envelope. Under `--json` the envelope is authoritative; stderr carries no
// duplicate finding rendering.
#[test]
fn json_without_quiet_does_not_also_emit_human_findings() {
    let proj = TempProject::new("json-no-double-emit");
    proj.seed_blog_schema();
    proj.seed_schema_violation_md("bad.md");
    let out = Command::new(mdatron_bin())
        .args(["verify", "--project-root"])
        .arg(proj.path())
        .arg("--json")
        .output()
        .expect("mdatron binary executes");
    assert_eq!(out.status.code(), Some(1), "the violation still exits 1");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The envelope is on stdout.
    let env: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout is the JSON envelope: {e}; got {stdout:?}"));
    assert_eq!(env["pipeline_status"], "ok");
    assert!(!env["findings"].as_array().unwrap().is_empty());
    // stderr does NOT re-render the finding as TTY text.
    assert!(
        !stderr.contains("= explain:") && !stderr.contains("--> "),
        "under --json, findings must not also render on stderr; got stderr: {stderr:?}"
    );
}

// #113 (vsdd item 7): a pattern-rule finding carries an ADOPTER-defined code
// (e.g. VSDD-E0201) for which mdatron has no explain page. It must NOT advertise
// `mdatron explain VSDD-E0201` — the same binary rejects it (a dead-end). The
// explain pointer is emitted only when a page actually resolves.
#[test]
fn adopter_coded_finding_does_not_advertise_a_dead_explain_page() {
    let proj = TempProject::new("explain-deadend");
    proj.write(
        ".mdatron/patterns/p.yaml",
        r#"mdatron_dsl_version: 1
pattern:
  id: p
  rules:
    - id: must-be-published
      context: note
      assert: $self.status == "published"
      code: VSDD-E0201
      message: "status must be published"
"#,
    );
    proj.write(
        "doc.md",
        "---\nschema_class: note\nstatus: draft\n---\nbody\n",
    );

    // TTY: the finding renders, but with NO dead explain line.
    let out = run_verify_tty(&proj);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("VSDD-E0201"),
        "the rule finding fires: {stderr}"
    );
    assert!(
        !stderr.contains("mdatron explain VSDD-E0201"),
        "must not advertise an explain page the binary rejects: {stderr}"
    );

    // JSON: explain_ref is null (not the unresolvable code).
    let out = Command::new(mdatron_bin())
        .args(["verify", "--project-root"])
        .arg(proj.path())
        .args(["--json", "-q"])
        .output()
        .expect("mdatron binary executes");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let env: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("json envelope: {e}; got {stdout:?}"));
    let f = env["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["code"] == "VSDD-E0201")
        .expect("the VSDD-coded finding is present");
    assert!(
        f["explain_ref"].is_null(),
        "explain_ref must be null for an unresolvable code: {f}"
    );
}

// #44 / #80 D4: `verify --compact` emits size-capped agent-context blocks on
// stdout — every block within the 512-byte contract limit, adopter content
// prefix-marked, exit code unchanged.
#[test]
fn verify_compact_emits_capped_marked_blocks_on_stdout() {
    let proj = TempProject::new("compact");
    proj.write(
        ".mdatron/schemas/note.json",
        r#"{"type":"object","properties":{"schema_class":{"const":"note"},"kind":{"enum":["a","b"]}},"required":["schema_class","kind"],"additionalProperties":false}"#,
    );
    proj.write(
        "doc.md",
        "---\nschema_class: note\nkind: not-a-real-kind\n---\nbody\n",
    );
    let out = Command::new(mdatron_bin())
        .args(["verify", "--project-root"])
        .arg(proj.path())
        .args(["--compact", "-q"])
        .output()
        .expect("mdatron binary executes");
    assert_eq!(out.status.code(), Some(1), "violation still exits 1");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.trim().is_empty(), "compact form lands on stdout");
    for block in stdout.trim_end().split("\n\n") {
        assert!(
            block.len() <= 512,
            "compact block exceeds the contract limit ({} bytes): {block}",
            block.len()
        );
    }
    assert!(
        stdout.contains("E[MDATRON-E0050]"),
        "head line shape: {stdout}"
    );
    assert!(
        stdout.lines().any(|l| l.starts_with("> ")),
        "adopter value rides prefix-marked: {stdout}"
    );
    assert!(
        !stdout.lines().next().unwrap().contains("not-a-real-kind"),
        "adopter value must not sit inline in the head line"
    );
}

// #84: the pin lifecycle through the binary — stale blocks, --dry-run
// previews without writing, --update re-pins, verify returns clean.
#[test]
fn pin_update_repins_and_dry_run_does_not_write() {
    let proj = TempProject::new("pin-cli");
    std::fs::create_dir_all(proj.path().join(".mdatron/schemas")).unwrap();
    proj.write("GOVERNING.md", "# gov\n");
    proj.write("governed.md", "v1\n");
    proj.write(
        ".mdatron/pins.yaml",
        "pins:\n- governing: GOVERNING.md\n  file: governed.md\n  sha256: \"stale\"\n",
    );
    let run_pin = |args: &[&str]| {
        Command::new(mdatron_bin())
            .args(["pin", "--project-root"])
            .arg(proj.path())
            .args(args)
            .output()
            .expect("mdatron binary executes")
    };
    // Stale: check mode exits 1 with E0061.
    let out = run_pin(&[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("MDATRON-E0061"));
    // Dry-run: previews, writes nothing (check still fails).
    let out = run_pin(&["--update", "--dry-run"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        run_pin(&[]).status.code(),
        Some(1),
        "dry-run must not write"
    );
    // Update: re-pins; check is clean.
    let out = run_pin(&["--update"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(run_pin(&[]).status.code(), Some(0), "re-pin restores clean");
    let record = std::fs::read_to_string(proj.path().join(".mdatron/pins.yaml")).unwrap();
    assert!(!record.contains("stale"), "hash rewritten: {record}");
}

fn run_verify_tty(proj: &TempProject) -> Output {
    Command::new(mdatron_bin())
        .args(["verify", "--project-root"])
        .arg(proj.path())
        .output()
        .expect("mdatron binary executes")
}

fn run_verify(proj: &TempProject, extra: &[&str]) -> Output {
    let mut cmd = Command::new(mdatron_bin());
    cmd.args(["verify", "--project-root"]).arg(proj.path());
    for a in extra {
        cmd.arg(a);
    }
    cmd.output().expect("mdatron binary executes")
}

// ── 1. `--json` finalization — the `= explain:` line ───────────────────────────

#[test]
fn verify_tty_renders_explain_line_for_finding_with_explain_ref() {
    let proj = TempProject::new("tty-explain");
    proj.seed_blog_schema();
    proj.seed_schema_violation_md("bad.md");

    let out = run_verify_tty(&proj);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("= explain: mdatron explain MDATRON-E0050"),
        "stderr must render the `= explain:` line for findings whose \
         explain_ref is Some (Phase 2 finalization); got stderr: {stderr}"
    );
}

#[test]
fn verify_tty_suppresses_explain_line_under_quiet() {
    let proj = TempProject::new("tty-explain-quiet");
    proj.seed_blog_schema();
    proj.seed_schema_violation_md("bad.md");

    let out = run_verify(&proj, &["--quiet"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("= explain:"),
        "stderr must be silent under --quiet (no explain line); got: {stderr}"
    );
}

#[test]
fn verify_tty_does_not_render_explain_line_when_explain_ref_is_none() {
    // The pipeline-failure stderr block (E0080) does not set explain_ref;
    // therefore no `= explain:` line should be rendered for it.
    let proj = TempProject::new("tty-pipeline-fail");
    proj.write("post.md", "---\nschema_class: blog\n---\n");

    let out = run_verify_tty(&proj);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("MDATRON-E0080"),
        "stderr must contain the pipeline-failure E0080 prefix; got: {stderr}"
    );
    assert!(
        !stderr.contains("= explain: mdatron explain MDATRON-E0080"),
        "stderr must NOT render an explain line for the bare pipeline-failure block \
         (explain_ref is None at that emission site); got: {stderr}"
    );
}

// ── 2. `mdatron explain CODE` against the v0.1.0 baseline catalog ──────────────

fn assert_explain_page_for(code: &str) {
    let out = run(&["explain", code]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "mdatron explain {code} must exit 0; got {:?}; stderr: {:?}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("## What this means"),
        "explain page for {code} must contain the required heading \
         '## What this means'; got: {stdout}"
    );
    assert!(
        stdout.contains("## How to fix"),
        "explain page for {code} must contain the required heading \
         '## How to fix'; got: {stdout}"
    );
    assert!(
        stdout.contains("**Severity:**"),
        "explain page for {code} must declare a Severity line; got: {stdout}"
    );
    assert!(
        stdout.contains("**Introduced in:**"),
        "explain page for {code} must declare Introduced in: line; got: {stdout}"
    );
}

#[test]
fn explain_e0001_emits_catalog_page_with_what_this_means() {
    assert_explain_page_for("MDATRON-E0001");
}

#[test]
fn explain_e0002_emits_catalog_page() {
    assert_explain_page_for("MDATRON-E0002");
}

#[test]
fn explain_e0050_emits_catalog_page() {
    assert_explain_page_for("MDATRON-E0050");
}

#[test]
fn explain_e0070_emits_catalog_page() {
    assert_explain_page_for("MDATRON-E0070");
}

#[test]
fn explain_e0080_emits_catalog_page() {
    assert_explain_page_for("MDATRON-E0080");
}

#[test]
fn explain_json_emits_structured_page_for_baseline_code() {
    // Per crosslink #13 AIE/F7: --json flag returns the parsed ExplainPage
    // object. Closes the v0.1.x candidate; structured form is the
    // agent-loop-consumer surface.
    let out = run(&["explain", "MDATRON-E0001", "--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "explain --json must exit 0 for baseline codes; stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout must be a single JSON object; got: {stdout:?}; {e}"));
    let obj = value.as_object().expect("top-level JSON must be an object");
    for required in [
        "code",
        "severity",
        "status",
        "introduced_in",
        "what_this_means",
        "how_to_fix",
        "markdown",
    ] {
        assert!(
            obj.contains_key(required),
            "explain --json output missing required field '{required}'; got keys: {:?}",
            obj.keys().collect::<Vec<_>>()
        );
    }
    assert_eq!(
        obj["code"].as_str(),
        Some("MDATRON-E0001"),
        "code field must match the requested code"
    );
}

#[test]
fn explain_compact_emits_one_liner_for_baseline_code() {
    // Per crosslink #13 AIE/F2: --compact emits one line suitable for
    // agent-loop context budgets.
    let out = run(&["explain", "MDATRON-E0050", "--compact"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "explain --compact must exit 0 for baseline codes"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let trimmed = stdout.trim();
    assert!(
        !trimmed.contains('\n'),
        "explain --compact output must be one line; got: {trimmed:?}"
    );
    assert!(
        trimmed.starts_with("MDATRON-E0050 error:"),
        "compact output must start with `<code> <severity>:`; got: {trimmed:?}"
    );
}

#[test]
fn explain_compact_and_json_are_mutually_exclusive() {
    let out = run(&["explain", "MDATRON-E0001", "--compact", "--json"]);
    assert!(
        !out.status.success(),
        "--compact + --json must conflict at clap parse time"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot be used with") || stderr.contains("conflicts"),
        "stderr should name the flag conflict; got: {stderr:?}"
    );
}

#[test]
fn explain_json_on_unknown_code_exits_two() {
    // The --json flag does not change exit-code semantics for unknown codes.
    let unreserved = format!("{}-{}", "MDATRON", "E9999");
    let out = run(&["explain", &unreserved, "--json"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "explain --json on unknown code must still exit 2; got {:?}",
        out.status
    );
}

#[test]
fn explain_unknown_code_exits_two_with_not_found_message() {
    // E9999 is not in any reserved range; therefore not in the catalog.
    // Constructed at runtime to keep the unreserved-literal out of source
    // (per the reserved-codes lint at
    // tests/phase_1_contracts.rs::all_emitted_codes_are_reserved).
    let unreserved = format!("{}-{}", "MDATRON", "E9999");
    let out = run(&["explain", &unreserved]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "explain on an unknown code must exit 2; got {:?}",
        out.status
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("not found")
            || stderr.to_lowercase().contains("unknown")
            || stderr.contains("no explain page"),
        "stderr should name the not-found behavior; got: {stderr}"
    );
}

#[test]
fn explain_vsdd_namespace_code_points_at_namespace_separation() {
    // Constructed at runtime to avoid a literal "VSDD-" prefix in source
    // (the cross-repo namespace-separation lint forbids the literal here).
    let other_ns_code = format!("{}{}-E0207", "VS", "DD");
    let out = run(&["explain", &other_ns_code]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "explain on a foreign-namespace code must exit 2; got {:?}",
        out.status
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("namespace") || stderr.contains("vsdd explain"),
        "stderr should explain the namespace-separation boundary; got: {stderr}"
    );
}

// ── 3. README presence + structure + round-trip ────────────────────────────────

fn readme_text() -> String {
    let p = mdatron_repo_root().join("README.md");
    fs::read_to_string(&p).unwrap_or_else(|e| {
        panic!("mdatron/README.md must exist at the repo root; read error: {e}")
    })
}

#[test]
fn readme_exists_at_repo_root() {
    let p = mdatron_repo_root().join("README.md");
    assert!(
        p.exists(),
        "mdatron/README.md must exist at {}",
        p.display()
    );
}

#[test]
fn readme_contains_seven_required_topic_headings() {
    let readme = readme_text();
    // Filter to markdown heading lines (start with '#') before substring-matching.
    // Per crosslink #13 QE/F1: the prior substring-anywhere match would have
    // passed if the topic appeared only in prose or link text, not as an actual
    // heading.
    let heading_lines: Vec<String> = readme
        .lines()
        .filter(|l| l.trim_start().starts_with('#'))
        .map(|l| l.to_lowercase())
        .collect();
    let required_topics = [
        ("Install", "install"),
        ("First run", "first run"),
        ("Schema example (Layer 1)", "schema"),
        ("Pattern example (Layer 2)", "pattern"),
        ("Relationship to vsdd", "vsdd"),
        ("Where to go next", "next"),
    ];
    for (name, fragment) in required_topics {
        assert!(
            heading_lines.iter().any(|h| h.contains(fragment)),
            "mdatron/README.md must cover topic '{name}' in a heading line \
             (substring '{fragment}' must appear in a `#`-prefixed line); \
             headings found: {heading_lines:?}"
        );
    }
}

#[test]
fn readme_contains_tron_disambiguation_sentence() {
    let readme = readme_text();
    let lower = readme.to_lowercase();
    assert!(
        lower.contains("tron"),
        "mdatron/README.md must contain the TRON-blockchain disambiguation \
         (TW-F3); got readme without 'tron' substring"
    );
    // The discipline per DESIGN.md § Summary (the disambiguation discipline) is the first README sentence
    // states this explicitly — assert the disambiguation appears in the
    // first ~30 lines (section 1 region).
    let head: String = readme.lines().take(30).collect::<Vec<_>>().join("\n");
    let head_lower = head.to_lowercase();
    assert!(
        head_lower.contains("tron")
            && (head_lower.contains("schematron") || head_lower.contains("blockchain")),
        "mdatron/README.md section 1 must disambiguate from TRON blockchain \
         (cite Schematron lineage OR explicitly mention the blockchain); \
         got section 1: {head}"
    );
}

#[test]
fn readme_cites_cargo_install_path_for_bootstrap_period() {
    let readme = readme_text();
    assert!(
        readme.contains("cargo install --path"),
        "mdatron/README.md install section must cite `cargo install --path ...` \
         for the bootstrap period (Phase 6 of the binary-first plan switches \
         to crates.io); got readme without the bootstrap install command"
    );
}

#[test]
fn readme_does_not_overclaim_version() {
    let readme = readme_text();
    // Per [[feedback-no-premature-version-bumps]]: the README ships alongside
    // an unpublished 0.1.0 crate; version language must not assert above 0.1.0
    // in prose. Forward-looking sentences are fine ("at v1.0 …"); explicit
    // version-claims above 0.1.0 are not.
    let forbidden = ["mdatron 0.2", "mdatron 1.0.0 ships", "version 1.0 release"];
    for bad in forbidden {
        assert!(
            !readme.to_lowercase().contains(&bad.to_lowercase()),
            "mdatron/README.md must not overclaim a version above 0.1.0; \
             found forbidden phrase: {bad}"
        );
    }
}

#[test]
fn readme_pattern_example_round_trips_against_mdatron_verify() {
    // Drive-by anti-rot guarantee: extract the first ```yaml pattern fence
    // and the first ```json schema fence; place them into a temp project;
    // run `mdatron verify`; assert the documented diagnostic (or clean
    // behavior the README claims).
    //
    // The implementation lives inline rather than as a shared helper —
    // single-milestone scope; the shared-helper refactor is a Phase 4
    // binary-first-plan candidate when the workspace collapses.
    let readme = readme_text();
    // Per crosslink #13 QE/F8: extract the round-trip-marked fences
    // explicitly, not by "first matching language". This insulates the
    // round-trip test from README reorganization that adds other fences
    // in the same languages.
    let schema_fence = extract_marked_fence(&readme, "schema")
        .expect("README must contain a <!-- mdatron-roundtrip:schema-start --> marker");
    let pattern_fence = extract_marked_fence(&readme, "pattern")
        .expect("README must contain a <!-- mdatron-roundtrip:pattern-start --> marker");
    let md_fence = extract_marked_fence(&readme, "md")
        .expect("README must contain a <!-- mdatron-roundtrip:md-start --> marker");

    let proj = TempProject::new("readme-roundtrip");
    proj.write(".mdatron/schemas/example.json", &schema_fence);
    proj.write(".mdatron/patterns/example.yaml", &pattern_fence);
    proj.write("example.md", &md_fence);

    let out = run_verify_tty(&proj);
    // The README's example is intentionally a clean case (title is non-empty;
    // pattern asserts title != ""). Tighter assertion per crosslink #13 QE/F2:
    // require exit 0 (clean) rather than just "not exit 2" — locks the
    // documented "clean run" claim in the README's First run section.
    assert_eq!(
        out.status.code(),
        Some(0),
        "README example must produce a clean run (exit 0); got exit {:?}; \
         stderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("mdatron verify: clean"),
        "clean run stderr must include the documented `mdatron verify: clean` \
         summary line; got: {stderr}"
    );
}

// ── 4. Drive-by `--quiet` and `--quiet --json` coverage ────────────────────────

#[test]
fn verify_quiet_json_combination_outputs_json_on_stdout_only() {
    let proj = TempProject::new("quiet-json");
    proj.seed_blog_schema();
    proj.seed_clean_md("post.md");

    let out = run_verify(&proj, &["--quiet", "--json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // stdout: single JSON object
    let trimmed = stdout.trim();
    assert!(
        trimmed.starts_with('{') && trimmed.ends_with('}'),
        "--quiet --json must still emit the JSON output on stdout; got: {stdout:?}"
    );
    serde_json::from_str::<serde_json::Value>(trimmed)
        .unwrap_or_else(|e| panic!("stdout must parse as JSON: {e}; got: {stdout:?}"));
    // stderr: silent (Phase 0 BC-5 row 3)
    assert!(
        stderr.is_empty(),
        "--quiet --json must produce empty stderr (Phase 0 BC-5 stream-contract row 3); \
         got: {stderr:?}"
    );
}

#[test]
fn verify_quiet_alone_suppresses_all_stderr_output() {
    let proj = TempProject::new("quiet-alone");
    proj.seed_blog_schema();
    proj.seed_schema_violation_md("bad.md");

    let out = run_verify(&proj, &["--quiet"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.is_empty(),
        "--quiet alone must produce empty stderr even when findings exist \
         (Phase 0 BC-5 stream-contract row 4); got: {stderr:?}"
    );
    // Exit code still reflects the findings (BC-4); --quiet does not change exit
    assert_eq!(
        out.status.code(),
        Some(1),
        "exit code under --quiet must still be 1 when errors exist; got {:?}",
        out.status
    );
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Extract the body of a `<!-- mdatron-roundtrip:<label>-start -->` ...
/// `<!-- mdatron-roundtrip:<label>-end -->` marked region's first code
/// fence. Per crosslink #13 QE/F8: README fence ordering is no longer
/// load-bearing for the round-trip test; only the explicit markers are.
fn extract_marked_fence(content: &str, label: &str) -> Option<String> {
    let start_marker = format!("<!-- mdatron-roundtrip:{label}-start -->");
    let end_marker = format!("<!-- mdatron-roundtrip:{label}-end -->");
    let region_start = content.find(&start_marker)? + start_marker.len();
    let region_end = content[region_start..].find(&end_marker)? + region_start;
    let region = &content[region_start..region_end];
    // Inside the region, find the first ``` fence open + close
    let fence_open = region.find("```")?;
    let after_fence_marker = region[fence_open + 3..].find('\n')? + fence_open + 3 + 1;
    let fence_close = region[after_fence_marker..].find("```")?;
    Some(region[after_fence_marker..after_fence_marker + fence_close].to_string())
}
