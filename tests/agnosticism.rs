// Test code: an unwrap IS the assertion — opt out of the [lints.clippy]
// panic-path restrictions production code is held to (#185).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Agnosticism-audit residuals (#91; `DESIGN.md` § Validation is data-driven,
//! the Agnosticism-audit acceptance cluster). Four mechanized criteria:
//! the methodology-vocabulary denylist over engine-authored strings, the
//! dependency-record ↔ manifest bijection (#190), the no-adopter-data run
//! (families inactive and reported), and the symlink-cycle bounded extras scan.

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
// (the region before each file's test MODULE per `codes::production_region` —
// fixtures legally use methodology-shaped field names) and the engine-authored
// explain pages. The
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
        let prod = normalize(
            mdatron::codes::production_region(&content)
                .unwrap_or_else(|e| panic!("{}: {e}", f.display())),
        );
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

// ── 2. Dependency records ↔ Cargo.toml bijection ─────────────────────────────
//
// Every dependency in Cargo.toml has a typed investigation record under
// docs/dependencies/<crate>.md, and every record names a current dependency
// (#190: the orphan direction the old one-way allowlist check lacked). The
// record's frontmatter is the typed edge — `crate` equals the file stem,
// `scope` names the declaring Cargo table, `version_req` is Cargo's requirement
// string verbatim. Its SHAPE is the schema family's job
// (.mdatron/schemas/dependency-record.json, run by mdatron's own
// self-verification); this test holds the two sides in AGREEMENT, which no
// schema can. The consume graph stays auditable against the record set.
#[test]
fn dependency_records_and_cargo_manifest_are_in_bijection() {
    use std::collections::BTreeMap;

    // name -> (scope, version requirement), from every dependency table.
    let cargo = fs::read_to_string(repo().join("Cargo.toml")).unwrap();
    let mut deps: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut scope: Option<&str> = None;
    for line in cargo.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            scope = match t {
                "[dependencies]" => Some("runtime"),
                "[dev-dependencies]" => Some("dev"),
                "[target.'cfg(unix)'.dependencies]" => Some("unix"),
                "[target.'cfg(windows)'.dependencies]" => Some("windows"),
                other
                    if other.starts_with("[dependencies.")
                        || other.starts_with("[dev-dependencies.")
                        || (other.starts_with("[target.") && other.contains(".dependencies.")) =>
                {
                    panic!("Cargo.toml declares a dependency as its own table {other}; this test reads the inline `name = …` form under a dependency table — declare it inline")
                }
                other if other.contains("dependencies") => {
                    panic!("Cargo.toml table {other} has no record scope name; teach this test and the dependency-record schema its scope")
                }
                _ => None,
            };
            continue;
        }
        let Some(scope) = scope else { continue };
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let Some((name, spec)) = t.split_once('=') else {
            continue;
        };
        let name = name.trim().trim_matches('"');
        if name.contains(' ') || name.contains('"') {
            // A continuation line of a multi-line spec (`features = [`), not a key.
            continue;
        }
        let spec = spec.trim();
        let version = if let Some(inline) = spec.strip_prefix('{') {
            // `version = "x"`, whatever the spacing around `=`.
            inline
                .split_once("version")
                .map(|(_, rest)| rest.trim_start())
                .and_then(|rest| rest.strip_prefix('='))
                .and_then(|rest| rest.trim_start().strip_prefix('"'))
                .and_then(|rest| rest.split_once('"'))
                .map(|(v, _)| v.to_string())
                .unwrap_or_else(|| {
                    panic!("dependency {name}: inline table {spec:?} carries no `version = \"…\"` (a git or path dependency has no version requirement for a record to mirror)")
                })
        } else {
            spec.trim_matches('"').to_string()
        };
        assert!(
            deps.insert(name.to_string(), (scope.to_string(), version))
                .is_none(),
            "dependency {name} is declared twice"
        );
    }
    assert!(!deps.is_empty(), "parsed no dependencies");

    // stem -> (crate, scope, version_req) from each record's frontmatter.
    let mut records: BTreeMap<String, (String, String, String)> = BTreeMap::new();
    for e in fs::read_dir(repo().join("docs/dependencies"))
        .unwrap()
        .flatten()
    {
        let path = e.path();
        // Only markdown records are records (a Finder `.DS_Store` is not an
        // orphan; the route's naming grammar governs what else may live here).
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap()
            .to_string();
        let content = fs::read_to_string(&path).unwrap();
        let (fm, _) = mdatron::frontmatter::parse(&content)
            .unwrap_or_else(|err| panic!("{}: malformed frontmatter: {err}", path.display()))
            .unwrap_or_else(|| {
                panic!(
                    "{}: no frontmatter block (the typed record is the edge)",
                    path.display()
                )
            });
        let field = |key: &str| {
            fm.get(key)
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("{}: frontmatter lacks a string `{key}`", path.display()))
                .to_string()
        };
        records.insert(stem, (field("crate"), field("scope"), field("version_req")));
    }

    let mut defects: Vec<String> = Vec::new();
    for (name, (scope, version)) in &deps {
        match records.get(name) {
            None => defects.push(format!(
                "dependency `{name}` has no docs/dependencies/{name}.md record"
            )),
            Some((krate, rscope, rversion)) => {
                if krate != name {
                    defects.push(format!(
                        "docs/dependencies/{name}.md names crate `{krate}`, not its stem"
                    ));
                }
                if rscope != scope {
                    defects.push(format!("docs/dependencies/{name}.md says scope `{rscope}`; Cargo.toml declares it under `{scope}`"));
                }
                if rversion != version {
                    defects.push(format!("docs/dependencies/{name}.md says version_req \"{rversion}\"; Cargo.toml requires \"{version}\""));
                }
            }
        }
    }
    for stem in records.keys() {
        if !deps.contains_key(stem) {
            defects.push(format!(
                "docs/dependencies/{stem}.md is an orphan record: no such dependency in Cargo.toml"
            ));
        }
    }
    assert!(
        defects.is_empty(),
        "dependency records and Cargo.toml disagree:\n  {}",
        defects.join("\n  ")
    );
}

// ── 3. No-adopter-data run: families inactive and reported ───────────────────
//
// With `.mdatron/` present but no family data, verify runs to completion and the
// envelope reports every family inactive (not a panic, not a silent no-op). A
// file that DECLARES a schema_class no schema serves is the one exception: since
// #111 (answered by vsdd-cli) that declaration warrants a W0045 warning even with
// empty infrastructure — the families still all read inactive (nothing validated
// it), but the unserved declaration is no longer a silent false-clean. A file
// with no schema_class at all stays fully clean (fresh-init-clean preserved).
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
    // #111: the declared-but-unserved schema_class now warrants exactly one W0045.
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|f| f.code == "MDATRON-W0045")
            .count(),
        1,
        "a declared schema_class that no schema serves is flagged, not silently clean; got {:?}",
        report.findings
    );
    // The families still all read inactive — nothing actually validated the file;
    // W0045 is the meta-signal that the file asked for validation no active family
    // could provide.
    let f = report.families;
    assert!(matches!(f.schema, FamilyActivity::Inactive { .. }));
    assert!(matches!(f.route, FamilyActivity::Inactive { .. }));
    assert!(matches!(f.pin, FamilyActivity::Inactive { .. }));
    assert!(matches!(f.vocabulary, FamilyActivity::Inactive { .. }));
    assert!(matches!(f.citation, FamilyActivity::Inactive { .. }));

    // Fresh-init-clean: swap the declared-class file for a schema_class-less one —
    // no declaration, no warning, genuinely empty findings.
    fs::remove_file(root.join("doc.md")).unwrap();
    fs::write(root.join("prose.md"), "---\ntitle: prose\n---\nbody\n").unwrap();
    let cfg = VerifyConfig::from_project(&root).unwrap();
    let report = verify_report(&cfg).expect("verify runs with no adopter data");
    assert!(
        report.findings.is_empty(),
        "a file with no declared schema_class stays clean; got {:?}",
        report.findings
    );

    let _ = fs::remove_dir_all(&root);
}

/// Per-platform symlink fixtures (#64): the confinement guarantee is
/// universal, so these gates run on Unix AND Windows. A creation failure
/// FAILS the test loudly (Windows: the runner needs
/// SeCreateSymbolicLinkPrivilege or Developer Mode), never skips it.
#[cfg(any(unix, windows))]
mod symlink_fixture {
    use std::path::Path;

    pub fn dir(target: impl AsRef<Path>, link: impl AsRef<Path>) {
        #[cfg(unix)]
        let result = std::os::unix::fs::symlink(target.as_ref(), link.as_ref());
        #[cfg(windows)]
        let result = std::os::windows::fs::symlink_dir(target.as_ref(), link.as_ref());
        result.unwrap_or_else(|e| {
            panic!("directory symlink fixture must be creatable on this runner: {e}")
        });
    }
}

// ── 4. Symlink-cycle bounded extras scan ────────────────────────────────────
//
// The closed-world no-follow enumeration terminates on a symlink cycle: a
// self-referential symlink is listed as a Symlink entry and NOT descended, so
// the scan cannot loop. (DESIGN § Nine check families: "symlink cycles cannot
// extend a walk".)
#[cfg(any(unix, windows))]
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
    symlink_fixture::dir("..", root.join("a/loop"));

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
