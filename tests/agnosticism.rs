//! Agnosticism-audit residuals (#91; `DESIGN.md` § Validation is data-driven,
//! the Agnosticism-audit acceptance cluster). Four mechanized criteria:
//! the methodology-vocabulary denylist over engine-authored strings, the
//! dependency-manifest allowlist, the no-adopter-data run (families inactive
//! and reported), and the symlink-cycle bounded extras scan.

use std::fs;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(p);
        }
    }
}

// ── 1. Methodology-vocabulary denylist ──────────────────────────────────────
//
// No methodology CONCEPT appears in engine-authored text: production source
// (the region before each file's `#[cfg(test)]` module — fixtures legally use
// methodology-shaped field names) and the engine-authored explain pages. The
// adopter name "vsdd" is NOT on the denylist: DESIGN permits naming vsdd as an
// adopter. The denylist is the methodology's own vocabulary, which the engine
// must never know.
#[test]
fn engine_authored_text_is_methodology_free() {
    // Space/hyphen/underscore variants folded by normalizing separators.
    const DENYLIST: &[&str] = &[
        "sycophancy",
        "phase primer",
        "domain prompt",
        "validator pair",
        "review entry",
        "domain review",
        "cold session",
    ];
    let normalize = |s: &str| s.to_lowercase().replace(['-', '_'], " ");

    let mut offenders: Vec<String> = Vec::new();

    // Production regions of src/*.rs.
    let mut files = Vec::new();
    rs_files(&repo().join("src"), &mut files);
    for f in files {
        let content = fs::read_to_string(&f).unwrap_or_default();
        let prod = normalize(content.split("#[cfg(test)]").next().unwrap_or(&content));
        for term in DENYLIST {
            if prod.contains(term) {
                offenders.push(format!("{}: '{term}'", f.display()));
            }
        }
    }
    // Engine-authored explain pages.
    for e in fs::read_dir(repo().join("src/explain")).unwrap().flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let body = normalize(&fs::read_to_string(&p).unwrap_or_default());
        for term in DENYLIST {
            if body.contains(term) {
                offenders.push(format!("{}: '{term}'", p.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "methodology vocabulary leaked into engine-authored text: {offenders:?}"
    );
}

// ── 2. Dependency-manifest allowlist ────────────────────────────────────────
//
// Every dependency in Cargo.toml has an investigation record under
// docs/dependencies/ (the VSDD-E0100 dependency-approval discipline). The
// consume graph is auditable against the allowlist.
#[test]
fn every_dependency_has_an_investigation_record() {
    let cargo = fs::read_to_string(repo().join("Cargo.toml")).unwrap();
    // Collect dep names from every `[*dependencies*]` table.
    let mut deps: Vec<String> = Vec::new();
    let mut in_deps = false;
    for line in cargo.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            in_deps = t.contains("dependencies");
            continue;
        }
        if !in_deps || t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((name, _)) = t.split_once('=') {
            deps.push(name.trim().trim_matches('"').to_string());
        }
    }
    assert!(!deps.is_empty(), "parsed no dependencies");

    let records: std::collections::HashSet<String> = fs::read_dir(repo().join("docs/dependencies"))
        .unwrap()
        .flatten()
        .filter_map(|e| {
            e.path()
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
        .collect();

    let missing: Vec<&String> = deps.iter().filter(|d| !records.contains(*d)).collect();
    assert!(
        missing.is_empty(),
        "dependencies without a docs/dependencies/<crate>.md record: {missing:?}"
    );
}

// ── 3. No-adopter-data run: families inactive and reported ───────────────────
//
// With `.mdatron/` present but no family data, verify runs to completion, emits
// no findings, and the envelope reports every family inactive (not a panic, not
// a silent no-op).
#[test]
fn no_adopter_data_runs_with_all_families_inactive() {
    use mdatron::output::FamilyActivity;
    use mdatron::verify::{verify_report, VerifyConfig};

    let root = repo().join("target").join(format!(
        "agnostic-empty-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    // .mdatron/ exists with empty schemas + patterns dirs; no adopter data.
    fs::create_dir_all(root.join(".mdatron/schemas")).unwrap();
    fs::create_dir_all(root.join(".mdatron/patterns")).unwrap();
    fs::write(
        root.join(".mdatron/config.yaml"),
        "file_globs:\n  - \"**/*.md\"\n",
    )
    .unwrap();
    fs::write(
        root.join("doc.md"),
        "---\nschema_class: anything\n---\nbody\n",
    )
    .unwrap();

    let cfg = VerifyConfig::from_project(&root).unwrap();
    let report = verify_report(&cfg).expect("verify runs with no adopter data");
    assert!(report.findings.is_empty(), "no data -> no findings");
    let f = report.families;
    assert!(matches!(f.schema, FamilyActivity::Inactive));
    assert!(matches!(f.route, FamilyActivity::Inactive));
    assert!(matches!(f.pin, FamilyActivity::Inactive));
    assert!(matches!(f.vocabulary, FamilyActivity::Inactive));
    assert!(matches!(f.citation, FamilyActivity::Inactive));

    let _ = fs::remove_dir_all(&root);
}

// ── 4. Symlink-cycle bounded extras scan ────────────────────────────────────
//
// The closed-world no-follow enumeration terminates on a symlink cycle: a
// self-referential symlink is listed as a Symlink entry and NOT descended, so
// the scan cannot loop. (DESIGN § Five check families: "symlink cycles cannot
// extend a walk".)
#[cfg(unix)]
#[test]
fn symlink_cycle_terminates_the_extras_scan() {
    use mdatron::confine::{list_dir, EntryType};

    let root = repo().join("target").join(format!(
        "agnostic-cycle-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("a")).unwrap();
    fs::write(root.join("a/real.md"), "x\n").unwrap();
    // A cycle: a/loop -> .. (back to a's parent, which contains a).
    std::os::unix::fs::symlink("..", root.join("a/loop")).unwrap();

    // list_dir enumerates a/ without following the cycle — it TERMINATES and
    // classifies the symlink as a Symlink entry rather than descending it.
    let entries = list_dir(&root, Path::new("a")).expect("list_dir terminates on a cycle");
    let loop_entry = entries
        .iter()
        .find(|e| e.name == "loop")
        .expect("the cyclic symlink is listed");
    assert!(
        matches!(loop_entry.file_type, EntryType::Symlink),
        "the cycle is a no-follow Symlink entry, not descended"
    );
    assert!(entries.iter().any(|e| e.name == "real.md"));

    let _ = fs::remove_dir_all(&root);
}
