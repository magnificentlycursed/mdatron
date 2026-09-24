//! Integration tests for `mdatron verify --json` against the output-format
//! contract documented at
//! `vsdd-cli/docs/refactor/phase-0-output-format/DESIGN.md`.
//!
//! Three test groupings:
//!   1. Output shape — top-level fields, output version, finding structure
//!   2. Process behavior — exit codes, stream contract, flag vocabulary
//!   3. Contract discipline — error-code namespace separation, output-version
//!      pinnable shape

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
        let path = std::env::temp_dir().join(format!("mdatron-out-{label}-{nanos}"));
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

    fn seed_minimal(&self) {
        self.write(
            ".mdatron/schemas/blog.json",
            r#"{"type":"object","required":["schema_class"],"properties":{"schema_class":{"const":"blog"}},"additionalProperties":false}"#,
        );
    }

    fn seed_clean_md(&self, name: &str) {
        self.write(name, "---\nschema_class: blog\n---\n# ok\n");
    }

    fn seed_failing_md(&self, name: &str) {
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
    // profile or workspace layout. Per crosslink #13 PE/F5 + AIE/F4 convergence.
    PathBuf::from(env!("CARGO_BIN_EXE_mdatron"))
}

fn run_verify_json(proj: &TempProject) -> Output {
    run_verify_json_with(proj, &[])
}

fn run_verify_json_with(proj: &TempProject, extra: &[&str]) -> Output {
    Command::new(mdatron_bin())
        .args(["verify", "--project-root"])
        .arg(proj.path())
        .arg("--json")
        .args(extra)
        .output()
        .expect("mdatron binary executes")
}

fn parse_output(output: &Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("stdout must contain a parseable JSON output (got: {stdout:?}, parse error: {e})")
    })
}

// ── Output shape ───────────────────────────────────────────────────────────────

#[test]
fn output_carries_mdatron_output_version_field() {
    let proj = TempProject::new("bc1");
    proj.seed_minimal();
    proj.seed_clean_md("post.md");

    let out = run_verify_json(&proj);
    let env = parse_output(&out);
    let version = env
        .get("mdatron_output_version")
        .and_then(|v| v.as_str())
        .expect("output must carry mdatron_output_version field");
    let semver_re = regex_lite_match(version, r"^\d+\.\d+\.\d+$");
    assert!(
        semver_re,
        "mdatron_output_version must match semver (got: {version})"
    );
}

// RED GATE (#90, vsdd rank-1 ask): the envelope reports per-family activity so
// a consumer can audit that a required family was actually invoked. Active =
// data supplied and the family ran (invoked, not fired).
#[test]
fn families_field_reports_per_verify_activity() {
    let proj = TempProject::new("families");
    proj.seed_minimal(); // schema data present
    proj.write("GOVERNING.md", "# gov\n");
    proj.write(
        ".mdatron/routes.yaml",
        "routes:\n- files: \"**/*.md\"\n  governed_by: GOVERNING.md\n",
    );
    proj.write(
        ".mdatron/vocabulary.yaml",
        "terms:\n- term: mdatron\n  status: registered\n  sense: the engine\n",
    );
    // No pins.yaml, and the route does not opt into citations.
    proj.seed_clean_md("post.md");

    let env = parse_output(&run_verify_json(&proj));
    let fam = env.get("families").and_then(|v| v.as_object()).unwrap();
    for k in ["schema", "route", "pin", "vocabulary", "citation"] {
        let v = fam
            .get(k)
            .and_then(|v| v.get("state"))
            .and_then(|v| v.as_str())
            .unwrap_or("MISSING");
        assert!(
            matches!(v, "active" | "inert" | "inactive"),
            "family {k} state must be tri-state; got {v}"
        );
        assert!(
            fam[k].get("reason").and_then(|r| r.as_str()).is_some(),
            "family {k} carries a falsifiable reason"
        );
    }
    assert_eq!(fam["schema"]["state"], "active", "schema data supplied");
    assert_eq!(fam["route"]["state"], "active", "routes.yaml supplied");
    assert_eq!(
        fam["vocabulary"]["state"], "active",
        "vocabulary.yaml supplied"
    );
    assert_eq!(fam["pin"]["state"], "inactive", "no pins.yaml");
    assert_eq!(
        fam["citation"]["state"], "inactive",
        "no route opted into citations"
    );
}

// #90: a family with no data reports inactive — the DESIGN "inactivity is
// reported" criterion satisfied by the same field, and the output version
// moved to reflect the additive field.
#[test]
fn absent_family_data_reports_inactive_and_version_bumped() {
    let proj = TempProject::new("fam-inactive");
    proj.seed_minimal();
    proj.seed_clean_md("post.md");
    let env = parse_output(&run_verify_json(&proj));
    let fam = env.get("families").and_then(|v| v.as_object()).unwrap();
    assert_eq!(fam["schema"]["state"], "active");
    for k in ["route", "pin", "vocabulary", "citation", "link"] {
        assert_eq!(fam[k]["state"], "inactive", "{k} has no data supplied");
    }
    assert_eq!(
        env.get("mdatron_output_version").and_then(|v| v.as_str()),
        Some("3.0.0"),
        "envelope 3.0.0 for 0.6.0: MAJOR — the sixth family (link, required, closed member) plus the forward-extensibility reshape of `families` (#145)"
    );
}

#[test]
fn output_top_level_shape_is_complete() {
    let proj = TempProject::new("bc2");
    proj.seed_minimal();
    proj.seed_clean_md("post.md");

    let out = run_verify_json(&proj);
    let env = parse_output(&out);

    for required in [
        "mdatron_output_version",
        "mdatron_version",
        "pipeline_status",
        "summary",
        "families",
        "findings",
    ] {
        assert!(
            env.get(required).is_some(),
            "output missing required top-level field: {required}"
        );
    }

    let status = env.get("pipeline_status").and_then(|v| v.as_str()).unwrap();
    assert!(
        matches!(status, "ok" | "failed"),
        "pipeline_status must be 'ok' or 'failed' (got: {status})"
    );

    let summary = env.get("summary").and_then(|v| v.as_object()).unwrap();
    for count_field in [
        "error_count",
        "warning_count",
        "lint_count",
        "files_checked",
    ] {
        assert!(
            summary.get(count_field).and_then(|v| v.as_u64()).is_some(),
            "summary missing non-negative integer field: {count_field}"
        );
    }
}

// RED GATE (#175): `--timings` adds the optional `timings` object with the
// four flat millisecond keys and sane values; WITHOUT the flag the key is
// absent entirely — flag-gating is the determinism guardrail (timings are the
// envelope's sole non-deterministic zone), so the default envelope on an
// unchanged tree stays byte-identical across runs.
#[test]
fn timings_are_flag_gated_and_sane() {
    let proj = TempProject::new("timings");
    proj.seed_minimal();
    proj.seed_clean_md("post.md");

    let default_env = parse_output(&run_verify_json(&proj));
    assert!(
        default_env.get("timings").is_none(),
        "no --timings, no timings key; got {default_env}"
    );
    // Determinism: two default runs on an unchanged tree emit identical bytes.
    let a = run_verify_json(&proj).stdout;
    let b = run_verify_json(&proj).stdout;
    assert_eq!(a, b, "the default envelope is byte-identical across runs");

    let timed = parse_output(&run_verify_json_with(&proj, &["--timings"]));
    let t = timed
        .get("timings")
        .and_then(|v| v.as_object())
        .expect("--timings emits the timings object");
    let ms = |k: &str| {
        t.get(k)
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| panic!("{k} is u64"))
    };
    let (total, load, capture, check) = (
        ms("total_ms"),
        ms("load_ms"),
        ms("capture_ms"),
        ms("check_ms"),
    );
    assert!(
        total >= load && total >= capture && total >= check,
        "total covers each phase: total={total} load={load} capture={capture} check={check}"
    );
    assert_eq!(t.len(), 4, "flat fixed keys only; got {t:?}");
}

// RED GATE (#176): the envelope carries the governance-input lineage — each
// input the run read, keyed to a sha256 of those bytes; digests are stable
// across runs on an unchanged tree, change when an input changes, and inputs
// that were never loaded carry no key.
#[test]
fn inputs_lineage_is_stable_keyed_and_change_sensitive() {
    let proj = TempProject::new("inputs");
    proj.seed_minimal();
    proj.seed_clean_md("post.md");

    let env1 = parse_output(&run_verify_json(&proj));
    let inputs1 = env1.get("inputs").and_then(|v| v.as_object()).unwrap();
    for key in ["config.yaml", "schemas"] {
        let d = inputs1
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("loaded input {key} carries a digest; got {inputs1:?}"));
        assert!(
            d.starts_with("sha256:") && d.len() == 7 + 64,
            "digest form: {d}"
        );
    }
    // Never-loaded inputs carry no key (this fixture has none of these).
    for key in [
        "routes.yaml",
        "vocabulary.yaml",
        "pins.yaml",
        "code-catalogs.yaml",
        "patterns",
    ] {
        assert!(
            inputs1.get(key).is_none(),
            "unloaded input {key} must be absent; got {inputs1:?}"
        );
    }

    // Byte-identical across two runs on an unchanged tree.
    let env2 = parse_output(&run_verify_json(&proj));
    assert_eq!(
        env1.get("inputs"),
        env2.get("inputs"),
        "lineage is deterministic"
    );

    // A config change moves config.yaml's digest; a routes.yaml appearance
    // adds its key.
    proj.write(
        ".mdatron/config.yaml",
        "file_globs:\n  - \"**/*.md\"\n# a comment changes the bytes\n",
    );
    proj.write(
        ".mdatron/routes.yaml",
        "routes:\n- files: \"**/*.md\"\n  governed_by: GOVERNING.md\n",
    );
    proj.write("GOVERNING.md", "# gov\n");
    let env3 = parse_output(&run_verify_json(&proj));
    let inputs3 = env3.get("inputs").and_then(|v| v.as_object()).unwrap();
    assert_ne!(
        inputs1.get("config.yaml"),
        inputs3.get("config.yaml"),
        "an input change moves its digest"
    );
    assert!(
        inputs3
            .get("routes.yaml")
            .and_then(|v| v.as_str())
            .is_some_and(|d| d.starts_with("sha256:")),
        "a newly present input gains its key; got {inputs3:?}"
    );
}

// #176: an ad-hoc `--files` run reads no config.yaml — the lineage reflects
// what was actually loaded (schemas here; no config key). And the envelope
// pins its exact contract via envelope_schema (lockstep with the schema $id is
// unit-tripwired; this pins the end-to-end emission).
#[test]
fn ad_hoc_files_run_emits_only_loaded_inputs() {
    let proj = TempProject::new("inputs-adhoc");
    proj.seed_minimal();
    proj.seed_clean_md("post.md");
    let out = Command::new(mdatron_bin())
        .args(["verify", "--project-root"])
        .arg(proj.path())
        .args(["--files", "**/*.md", "--json"])
        .output()
        .expect("mdatron binary executes");
    let env = parse_output(&out);
    let inputs = env.get("inputs").and_then(|v| v.as_object()).unwrap();
    assert!(
        inputs.get("config.yaml").is_none(),
        "an ad-hoc --files run reads no config.yaml; got {inputs:?}"
    );
    assert!(inputs.get("schemas").is_some(), "schemas were loaded");
    assert!(
        env.get("envelope_schema")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.ends_with("/3.0.0")),
        "the envelope pins its schema $id; got {env}"
    );
}

// RED GATE (#177, end-to-end): every emitted finding carries a v1 fingerprint,
// and the fingerprint survives line churn — the same violation moved to a
// different line across two runs keeps its identity (that is its purpose).
#[test]
fn finding_fingerprints_survive_line_churn_across_runs() {
    let proj = TempProject::new("fingerprint");
    proj.seed_minimal();
    proj.seed_failing_md("bad.md");
    let env1 = parse_output(&run_verify_json(&proj));
    let f1 = env1["findings"].as_array().unwrap();
    assert!(!f1.is_empty());
    let fp1 = f1[0]["fingerprint"].as_str().expect("fingerprint present");
    assert!(fp1.starts_with("v1:") && fp1.len() == 35, "shape: {fp1}");

    // Regenerate the document around the SAME violation (body content added;
    // code/path/summary/quoted all unchanged): the fingerprint must hold.
    proj.write(
        "bad.md",
        "---\nschema_class: blog\nextra: not allowed\n---\n\nnew body line\n",
    );
    let env2 = parse_output(&run_verify_json(&proj));
    let fp2 = env2["findings"].as_array().unwrap()[0]["fingerprint"]
        .as_str()
        .unwrap();
    assert_eq!(fp1, fp2, "the identity survives document churn");
}

// GH #52 lane-B review B5: the `in` array-only narrowing, driven END-TO-END
// through the binary (parse → verify → envelope): a refused haystack (`"x" in
// $self.<absent>` — a Null right side) surfaces as the E0080 eval pipeline
// error in the envelope, kind "eval", exit 2 — the loud surface the reference
// documents, never the silent `false` it used to be.
#[test]
fn refused_in_haystack_is_an_eval_pipeline_error_end_to_end() {
    let proj = TempProject::new("in-null-e2e");
    proj.write(
        ".mdatron/patterns/p.yaml",
        "mdatron_dsl_version: 1\npattern:\n  id: p\n  rules:\n    - id: r\n      \
         context: \"**/*.md\"\n      assert: '\"x\" in $self.absent'\n      \
         code: T-E0001\n      message: m\n",
    );
    proj.write("doc.md", "---\nfoo: bar\n---\n");
    let out = run_verify_json(&proj);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a refused haystack is a loud pipeline failure; stderr={:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let env = parse_output(&out);
    assert_eq!(env["pipeline_status"], "failed");
    assert_eq!(env["pipeline_error"]["code"], "MDATRON-E0080");
    assert_eq!(
        env["pipeline_error"]["kind"], "eval",
        "the eval failure class rides the envelope; got {env}"
    );
}

// ── #185 L1: contract hygiene (post-0.6.0; envelope 3.0.0 is a live contract) ──

/// A verify run WITHOUT --json: rustc-shaped findings on stderr.
fn run_verify_tty(proj: &TempProject, extra: &[&str]) -> Output {
    Command::new(mdatron_bin())
        .args(["verify", "--project-root"])
        .arg(proj.path())
        .args(extra)
        .output()
        .expect("mdatron binary executes")
}

/// The published envelope schema, compiled from the repo file (the output.rs
/// helper is #[cfg(test)]-private to the lib).
fn published_schema() -> jsonschema::Validator {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schema/mdatron-output.schema.json")).unwrap();
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&schema)
        .expect("published schema compiles")
}

fn assert_validates(env: &serde_json::Value, label: &str) {
    let errs: Vec<String> = published_schema()
        .iter_errors(env)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect();
    assert!(
        errs.is_empty(),
        "{label}: real envelope failed the published schema:\n{}",
        errs.join("\n")
    );
}

/// A fixture yielding three DISTINCT codes across three severities: E0050
/// (schema violation), W0041 (naming grammar the files fail), L0001 (a
/// justified unpinned tombstone).
fn three_code_fixture(label: &str) -> TempProject {
    let proj = TempProject::new(label);
    proj.seed_minimal();
    proj.seed_failing_md("bad.md");
    proj.write("GOVERNING.md", "# gov\n");
    proj.write(
        ".mdatron/routes.yaml",
        "routes:\n- files: \"**/*.md\"\n  governed_by: GOVERNING.md\n  naming: \"^post-[0-9]+\\\\.md$\"\n",
    );
    proj.write(
        ".mdatron/pins.yaml",
        "pins: []\nunpinned:\n- file: old.md\n  governing: GOVERNING.md\n  reason: retired\n  owner: op\n",
    );
    proj
}

/// The code token of a compact block's head line (`E[CODE] …`) or a TTY
/// header line (`error[CODE]: …`), if the line is one.
fn code_in_header(line: &str) -> Option<&str> {
    let is_compact =
        line.len() > 1 && line.starts_with(['E', 'W', 'L']) && line[1..].starts_with('[');
    let is_tty =
        line.starts_with("error[") || line.starts_with("warning[") || line.starts_with("info[");
    if !(is_compact || is_tty) {
        return None;
    }
    let open = line.find('[')?;
    let close = line[open..].find(']')? + open;
    let code = &line[open + 1..close];
    // TTY headers carry a `:` right after the bracket; compact heads a space.
    (is_compact || line[close + 1..].starts_with(':')).then_some(code)
}

// RED GATE (#185 L1 item 1 — the #52 minor / lane-A A2): the machine JSON text
// escapes the Unicode line/paragraph separators and the C1 controls that
// serde_json leaves raw, so an envelope is safe to embed in JS or echo to a
// terminal — while a decoding consumer receives the IDENTICAL string (the
// round-trip below), so envelope 3.0.0's semantics are untouched.
#[test]
fn envelope_json_text_escapes_separators_and_c1_but_decodes_identically() {
    let proj = TempProject::new("js-safe-quoted");
    proj.write(
        ".mdatron/schemas/blog.json",
        r#"{"type":"object","required":["schema_class"],"properties":{"schema_class":{"const":"blog"},"name":{"enum":["a"]}},"additionalProperties":false}"#,
    );
    // YAML decodes the escapes: the VALUE carries a raw U+2028 and a raw NEL.
    proj.write(
        "post.md",
        "---\nschema_class: blog\nname: \"x\\u2028y\\u0085z\"\n---\n# ok\n",
    );
    let out = run_verify_json(&proj);
    assert_eq!(out.status.code(), Some(1), "the enum violation fires");
    let raw = &out.stdout;
    assert!(
        !raw.windows(3).any(|w| w == [0xE2, 0x80, 0xA8])
            && !raw.windows(2).any(|w| w == [0xC2, 0x85]),
        "no raw U+2028 / NEL bytes in the JSON text"
    );
    let text = String::from_utf8_lossy(raw);
    assert!(
        text.contains("\\u2028") && text.contains("\\u0085"),
        "the separators/C1 ride as \\u escapes; got {text}"
    );
    // Round-trip: the decoded quoted content carries the original chars.
    let env = parse_output(&out);
    let quoted: Vec<&str> = env["findings"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|f| f["quoted"].as_array().into_iter().flatten())
        .filter_map(|q| q["content"].as_str())
        .collect();
    assert!(
        quoted.iter().any(|c| c.contains("x\u{2028}y\u{85}z")),
        "a decoding consumer gets the identical string; quoted={quoted:?}"
    );
    assert_validates(&env, "js-safe findings run");
}

// RED GATE (#185 L1 item 1, the lane-A A2 repro): a separator smuggled into a
// rule expression surfaces in `pipeline_error.message` escaped, not raw.
#[test]
fn pipeline_error_message_escapes_separators() {
    let proj = TempProject::new("js-safe-pipeline");
    proj.write(
        ".mdatron/patterns/p.yaml",
        "mdatron_dsl_version: 1\npattern:\n  id: p\n  rules:\n    - id: r\n      \
         context: \"**/*.md\"\n      assert: \"$a b\\u2028c\"\n      code: T-E0001\n      \
         message: m\n",
    );
    proj.write("doc.md", "---\nfoo: bar\n---\n");
    let out = run_verify_json(&proj);
    assert_eq!(
        out.status.code(),
        Some(2),
        "an unparseable assert is a pipeline error"
    );
    let raw = &out.stdout;
    assert!(
        !raw.windows(3).any(|w| w == [0xE2, 0x80, 0xA8]),
        "no raw U+2028 bytes in the JSON text"
    );
    assert!(String::from_utf8_lossy(raw).contains("\\u2028"));
    let env = parse_output(&out);
    let msg = env["pipeline_error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("b\u{2028}c"),
        "decodes to the original separator; got {msg:?}"
    );
    assert_validates(&env, "js-safe pipeline-error run");
}

// RED GATE (#185 L1 item 2): with a RELATIVE --project-root whose name
// coincides with adopter text (`docs`), the host-layout relativizer used to
// substring-strip `docs/` out of a free-form error mid-string — the invalid
// glob `docs/a**b` rendered as `a**b` in BOTH the envelope and the TTY note.
// Only an absolute root is a host-layout leak; relative roots are skipped.
#[test]
fn relative_project_root_never_mangles_adopter_text() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let parent = std::env::temp_dir().join(format!("mdatron-relroot-{nanos}"));
    let root = parent.join("docs");
    fs::create_dir_all(root.join(".mdatron")).unwrap();
    fs::write(
        root.join(".mdatron/config.yaml"),
        "file_globs:\n  - \"docs/a**b\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join(".mdatron/schemas")).unwrap();
    fs::write(root.join(".mdatron/schemas/.keep.json"), "{}").unwrap();

    let json = Command::new(mdatron_bin())
        .current_dir(&parent)
        .args(["verify", "--project-root", "docs", "--json"])
        .output()
        .expect("mdatron binary executes");
    assert_eq!(
        json.status.code(),
        Some(2),
        "an invalid glob is a pipeline failure"
    );
    let env = parse_output(&json);
    let msg = env["pipeline_error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("docs/a**b"),
        "the adopter's glob text is intact in the envelope; got {msg:?}"
    );
    let tty = Command::new(mdatron_bin())
        .current_dir(&parent)
        .args(["verify", "--project-root", "docs"])
        .output()
        .expect("mdatron binary executes");
    let stderr = String::from_utf8_lossy(&tty.stderr);
    assert!(
        stderr.contains("docs/a**b"),
        "the adopter's glob text is intact in the TTY note; got {stderr}"
    );
    let _ = fs::remove_dir_all(&parent);
}

// RED GATE (#185 L1 item 3a): the three output forms agree on the FINDING SET
// end to end — the unit tripwire checks each finding against its own render
// and so cannot see a form that DROPS a finding. Real run, >= 3 distinct codes
// across three severities, code MULTISETS compared across --json, --compact,
// and TTY.
#[test]
fn three_output_forms_agree_on_a_real_run() {
    let proj = three_code_fixture("three-forms");
    let json_codes: Vec<String> = {
        let env = parse_output(&run_verify_json(&proj));
        env["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["code"].as_str().unwrap().to_string())
            .collect()
    };
    let compact_codes: Vec<String> = {
        let out = run_verify_tty(&proj, &["--compact"]);
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| code_in_header(l).map(str::to_string))
            .collect()
    };
    let tty_codes: Vec<String> = {
        let out = run_verify_tty(&proj, &[]);
        String::from_utf8_lossy(&out.stderr)
            .lines()
            .filter_map(|l| code_in_header(l).map(str::to_string))
            .collect()
    };
    let sorted = |mut v: Vec<String>| {
        v.sort();
        v
    };
    let (j, c, t) = (sorted(json_codes), sorted(compact_codes), sorted(tty_codes));
    let distinct: std::collections::BTreeSet<&str> = j.iter().map(String::as_str).collect();
    assert!(
        distinct.len() >= 3
            && distinct.contains("MDATRON-E0050")
            && distinct.contains("MDATRON-W0041")
            && distinct.contains("MDATRON-L0001"),
        "the fixture yields three distinct codes; got {j:?}"
    );
    assert_eq!(j, c, "compact drops or invents a finding vs json");
    assert_eq!(j, t, "tty drops or invents a finding vs json");
}

// RED GATE (#185 L1 item 3b): REAL run envelopes validate against the
// published schema — the unit tripwire validates hand-built Outputs only.
// Four shapes: a findings run, a clean run, a failed-pipeline run, and a
// --timings run.
#[test]
fn real_envelopes_validate_against_the_published_schema() {
    let findings = three_code_fixture("real-validate-findings");
    assert_validates(&parse_output(&run_verify_json(&findings)), "findings run");

    let clean = TempProject::new("real-validate-clean");
    clean.seed_minimal();
    clean.seed_clean_md("post.md");
    assert_validates(&parse_output(&run_verify_json(&clean)), "clean run");
    assert_validates(
        &parse_output(&run_verify_json_with(&clean, &["--timings"])),
        "timings run",
    );

    let failed = TempProject::new("real-validate-failed");
    failed.write(".mdatron/config.yaml", "file_globs:\n  - \"docs/a**b\"\n");
    failed.write(".mdatron/schemas/.keep.json", "{}");
    let out = run_verify_json(&failed);
    assert_eq!(out.status.code(), Some(2));
    assert_validates(&parse_output(&out), "failed-pipeline run");
}

#[test]
fn finding_code_prefix_matches_severity() {
    let proj = TempProject::new("bc3");
    proj.seed_minimal();
    proj.seed_failing_md("bad.md");

    let out = run_verify_json(&proj);
    let env = parse_output(&out);
    let findings = env.get("findings").and_then(|v| v.as_array()).unwrap();

    for (i, f) in findings.iter().enumerate() {
        let code = f.get("code").and_then(|v| v.as_str()).expect("code");
        let severity = f
            .get("severity")
            .and_then(|v| v.as_str())
            .expect("severity");
        let prefix = code
            .chars()
            .nth("MDATRON-".len())
            .expect("code has prefix letter");
        let expected_sev = match prefix {
            'E' => "error",
            'W' => "warning",
            'L' => "lint",
            other => panic!("unknown code prefix letter {other} in {code} (finding #{i})"),
        };
        assert_eq!(
            severity, expected_sev,
            "finding #{i} code {code} has severity {severity}; expected {expected_sev} (code prefix and severity must agree: E=error, W=warning, L=lint)"
        );
    }
}

// ── Process behavior ───────────────────────────────────────────────────────────

#[test]
fn exit_zero_when_clean() {
    let proj = TempProject::new("bc4-clean");
    proj.seed_minimal();
    proj.seed_clean_md("post.md");

    let out = run_verify_json(&proj);
    assert_eq!(
        out.status.code(),
        Some(0),
        "clean run must exit 0; got {:?}; stdout: {:?}",
        out.status,
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn exit_one_when_error_findings_exist() {
    let proj = TempProject::new("bc4-errors");
    proj.seed_minimal();
    proj.seed_failing_md("bad.md");

    let out = run_verify_json(&proj);
    assert_eq!(
        out.status.code(),
        Some(1),
        "error-severity findings present must exit 1; got {:?}",
        out.status
    );
}

#[test]
fn exit_two_when_pipeline_failed() {
    let proj = TempProject::new("bc4-pipeline-fail");
    // No .mdatron/ at all — pipeline cannot load schemas/patterns
    proj.write("post.md", "---\nschema_class: blog\n---\n");

    let out = run_verify_json(&proj);
    assert_eq!(
        out.status.code(),
        Some(2),
        "pipeline failure (missing .mdatron/) must exit 2; got {:?}",
        out.status
    );
}

#[test]
fn stdout_under_json_contains_only_the_output_object() {
    let proj = TempProject::new("bc5");
    proj.seed_minimal();
    proj.seed_clean_md("post.md");

    let out = run_verify_json(&proj);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let trimmed = stdout.trim();
    // Single JSON object on stdout under --json
    assert!(
        trimmed.starts_with('{') && trimmed.ends_with('}'),
        "stdout under --json must contain only the JSON output; got: {stdout:?}"
    );
    // No diagnostic text on stdout
    assert!(
        !stdout.contains("error[MDATRON"),
        "stdout must not contain rustc-shaped diagnostic text under --json (diagnostics belong on stderr)"
    );
}

// RED GATE (#175 cold-review R4 + R6): `--timings` rides ONLY in the JSON
// envelope, so any non-JSON combination would silently do nothing — clap
// refuses each loudly instead. R6 pinned the `--compact` leak specifically:
// clap 4.5 WAIVES an arg's `requires` when another present arg conflicts the
// required arg away (`compact` conflicts with `json`), so `--timings
// --compact` was accepted and silently dropped timings; the explicit
// `conflicts_with = "compact"` on the timings arg closes it.
#[test]
fn timings_without_json_is_a_usage_error() {
    let proj = TempProject::new("timings-nojson");
    proj.seed_minimal();
    proj.seed_clean_md("post.md");

    for (extra, expect_in_err) in [
        (None, "--json"),
        // R6: the requires-waiver leak — must be a loud conflict error.
        (Some("--compact"), "--compact"),
    ] {
        let mut cmd = Command::new(mdatron_bin());
        cmd.args(["verify", "--project-root"])
            .arg(proj.path())
            .arg("--timings");
        if let Some(flag) = extra {
            cmd.arg(flag);
        }
        let out = cmd.output().expect("mdatron binary executes");
        assert!(
            !out.status.success(),
            "--timings {} must be refused; got success with stdout: {:?}",
            extra.unwrap_or("(bare)"),
            String::from_utf8_lossy(&out.stdout)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(expect_in_err),
            "the usage error names {expect_in_err}; got {stderr:?}"
        );
    }
}

#[test]
fn unknown_flag_is_rejected() {
    let proj = TempProject::new("bc6");
    proj.seed_minimal();
    proj.seed_clean_md("post.md");

    let out = Command::new(mdatron_bin())
        .args(["verify", "--project-root"])
        .arg(proj.path())
        .arg("--definitely-not-a-real-flag-xyz")
        .output()
        .expect("mdatron binary executes");
    assert!(
        !out.status.success(),
        "unknown flag must cause non-zero exit; got success with stdout: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("definitely-not-a-real-flag-xyz")
            || stderr.to_lowercase().contains("unexpected"),
        "stderr should name the unknown flag; got: {stderr:?}"
    );
}

// ── Contract discipline ────────────────────────────────────────────────────────

#[test]
fn mdatron_source_never_emits_vsdd_code_prefix() {
    // Lint-style fixture: grep the mdatron source for the literal string
    // "VSDD-E" as a quoted literal. mdatron MUST NOT emit VSDD-Exxxx codes.
    use std::collections::HashSet;

    // Single-crate layout (#81): CARGO_MANIFEST_DIR is the repo root.
    let mdatron_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scan_roots = [mdatron_root.join("src")];
    let mut allowlist: HashSet<String> = HashSet::new();
    allowlist.insert(
        // The lint MAY hit its own marker string; allowlist tests/ files explicitly.
        "output_format.rs".to_string(),
    );

    let mut offenders: Vec<String> = Vec::new();
    for root in scan_roots {
        for entry in walk_rs(&root) {
            if allowlist.iter().any(|skip| entry.ends_with(skip)) {
                continue;
            }
            let content = fs::read_to_string(&entry).unwrap_or_default();
            if content.contains("\"VSDD-E") || content.contains("\"VSDD-W") {
                offenders.push(entry.display().to_string());
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "mdatron source emits a VSDD-Exxxx code literal (namespace separation violated) in: {offenders:?}"
    );
}

#[test]
fn output_version_is_consumer_pinnable_semver() {
    // Smoke check: emitted version is parseable + non-trivial. Phase 2b
    // implementation provides the actual version constant; this asserts the
    // shape so that a downstream consumer (vsdd) can pin against it.
    let proj = TempProject::new("bc8");
    proj.seed_minimal();
    proj.seed_clean_md("post.md");

    let out = run_verify_json(&proj);
    let env = parse_output(&out);
    let version = env
        .get("mdatron_output_version")
        .and_then(|v| v.as_str())
        .unwrap();
    let parts: Vec<&str> = version.split('.').collect();
    assert_eq!(
        parts.len(),
        3,
        "output version must be semver triple; got {version}"
    );
    for p in parts {
        assert!(
            p.parse::<u32>().is_ok(),
            "output version component must be a non-negative integer; got {p}"
        );
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn walk_rs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }
    out
}

/// Minimal regex-free check: returns true if `s` matches `^\d+\.\d+\.\d+$`.
fn regex_lite_match(s: &str, _pat: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| p.parse::<u32>().is_ok())
}
