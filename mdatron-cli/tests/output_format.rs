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
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .join("target")
        .join("debug")
        .join("mdatron")
}

fn run_verify_json(proj: &TempProject) -> Output {
    Command::new(mdatron_bin())
        .args(["verify", "--project-root"])
        .arg(proj.path())
        .arg("--json")
        .output()
        .expect("mdatron binary executes")
}

fn parse_output(output: &Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "stdout must contain a parseable JSON output (got: {stdout:?}, parse error: {e})"
        )
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
    for count_field in ["error_count", "warning_count", "lint_count", "files_checked"] {
        assert!(
            summary.get(count_field).and_then(|v| v.as_u64()).is_some(),
            "summary missing non-negative integer field: {count_field}"
        );
    }
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
        let severity = f.get("severity").and_then(|v| v.as_str()).expect("severity");
        let prefix = code.chars().nth("MDATRON-".len()).expect("code has prefix letter");
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
    // Lint-style fixture: grep mdatron-core + mdatron-cli source for the literal
    // string "VSDD-E" as a quoted literal. mdatron MUST NOT emit VSDD-Exxxx codes.
    use std::collections::HashSet;

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let mdatron_root = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();
    let scan_roots = [
        mdatron_root.join("mdatron-core").join("src"),
        mdatron_root.join("mdatron-cli").join("src"),
    ];
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
