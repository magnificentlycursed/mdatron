//! `mdatron init`: deploy the `.mdatron/` skeleton and its managed-partition
//! manifest, idempotently, refusing drifted managed files.
//!
//! Per `DESIGN.md` § Init: the skeleton is the schema and pattern directories,
//! a seeded engine-default config, and the init manifest defining the managed
//! partition. Managed files — listed in the manifest with sha256 content
//! hashes — are drift-refused; the manifest is the authority on WHAT is
//! managed (its entries are honored as data). Seeded files (config.yaml, #77)
//! are written once and adopter-owned from then on — never hashed, never
//! overwritten; their demotion from the managed partition persists as
//! tombstones in trees that had them managed. Adopter-authored data (schemas
//! and patterns) lives outside the manifest and is never touched. The manifest
//! cannot hash itself (a fixed point, `DESIGN.md` § Governance data is
//! governed); its own integrity is anchored by commit review.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::confine::confine_lexically;
use crate::diagnostic::{Finding, Location, Severity};

/// The engine-default config, **seeded** at init and adopter-owned from then
/// on. Its `file_globs` are the consumer-authored jurisdiction the verify
/// pipeline honors (#77) — which is exactly why it cannot be drift-guarded:
/// the original guard (placed "until config consumption lands") would make
/// customization impossible. Its demotion from the managed partition is
/// recorded as a tombstone in trees that had it managed (DESIGN § Governance
/// data is governed).
const DEFAULT_CONFIG: &str = "\
# mdatron configuration — seeded by `mdatron init`; adopter-owned.
# file_globs declare what is in mdatron's jurisdiction; files outside them
# are not walked. Adopter schemas live in .mdatron/schemas/, patterns in
# .mdatron/patterns/.
file_globs:
  - \"**/*.md\"
";

/// Files the engine seeds at init, relative to `.mdatron/`: written when
/// absent, never overwritten, never hashed — adopter-owned once deployed.
const SEED_FILES: &[(&str, &str)] = &[("config.yaml", DEFAULT_CONFIG)];

/// Engine-known content for manifest-listed managed paths, used to repair a
/// missing managed file in trees whose manifest still lists it (v1 trees
/// predating the config.yaml demotion). The manifest is the authority on WHAT
/// is managed; this is only the authority on what the engine can redeploy.
fn engine_content(path: &str) -> Option<&'static str> {
    match path {
        "config.yaml" => Some(DEFAULT_CONFIG),
        _ => None,
    }
}

/// Directories the skeleton creates under `.mdatron/`.
const SKELETON_DIRS: &[&str] = &["schemas", "patterns"];

const MANIFEST_NAME: &str = "manifest.yaml";
/// Manifest shape version. v2 adds demotion tombstones and empties the default
/// managed set (config.yaml demoted to a seed, #77); v1 manifests — which may
/// still list config.yaml as managed — parse and are honored as data.
const MANIFEST_VERSION: u32 = 2;

/// Outcome of a successful init run.
#[derive(Debug, PartialEq, Eq)]
pub enum InitOutcome {
    /// First run (or a repair of missing managed files): paths created.
    Deployed { created: Vec<String> },
    /// Re-run on an intact, unmodified tree: nothing to do.
    AlreadyInitialized,
}

/// A managed-file drift: a manifest-listed file whose content no longer matches
/// its recorded hash. Surfaced to the CLI as an `MDATRON-E0060` diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    pub file: String,
    pub expected_sha256: String,
    pub actual_sha256: String,
}

/// Errors from an init run. `Drift` is a governance refusal (a managed file was
/// hand-modified); the others are IO/parse failures.
#[derive(Debug)]
pub enum InitError {
    Io { path: String, error: String },
    ManifestParse { path: String, error: String },
    Drift(Vec<Drift>),
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InitError::Io { path, error } => write!(f, "init IO at '{path}': {error}"),
            InitError::ManifestParse { path, error } => {
                write!(f, "init manifest parse at '{path}': {error}")
            }
            InitError::Drift(drifts) => {
                write!(
                    f,
                    "{} managed file(s) drifted from the init manifest",
                    drifts.len()
                )
            }
        }
    }
}

impl std::error::Error for InitError {}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    /// Engine-deployed files (path relative to `.mdatron/`) with their sha256.
    /// The manifest never lists itself (fixed point). The manifest DEFINES the
    /// managed partition (DESIGN § Governance data is governed): drift checks
    /// run over these entries as data, not over an engine-side list.
    managed: Vec<ManagedEntry>,
    /// Demotion tombstones: standing records of entries removed from the
    /// managed partition, with their justification (DESIGN: removals persist
    /// as tombstones; a naive demotion trips drift, a tombstoned one stays
    /// loud through this record; the terminal anchor is commit review).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    demoted: Vec<DemotionTombstone>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ManagedEntry {
    path: String,
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DemotionTombstone {
    path: String,
    reason: String,
    owner: String,
}

/// Lowercase hex sha256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Run `mdatron init` at `project_root`.
///
/// - No manifest present → first run: deploy the skeleton, seed files, and a
///   fresh (v2, empty-managed) manifest.
/// - Manifest present, every manifest-listed managed file intact and every
///   seed present → no-op ([`InitOutcome::AlreadyInitialized`]).
/// - A manifest-listed managed file hand-modified (hash mismatch) → refuse
///   ([`InitError::Drift`]); nothing is written. Seeds are adopter-owned:
///   an edited seed is neither refused nor overwritten.
/// - A manifest-listed managed file missing → repaired from engine-known
///   content; the manifest is rewritten preserving entries and tombstones.
///   A missing seed is re-seeded.
pub fn init(project_root: &Path) -> Result<InitOutcome, InitError> {
    let dir = project_root.join(".mdatron");
    let manifest_path = dir.join(MANIFEST_NAME);

    if manifest_path.exists() {
        let mut manifest = read_manifest(&manifest_path)?;
        let mut drifts = Vec::new();
        let mut missing: Vec<usize> = Vec::new();
        for (i, entry) in manifest.managed.iter().enumerate() {
            // The manifest is read from the tree, and it cannot hash itself
            // (fixed point) — so a hand-edit to it is not drift-caught. Hold its
            // managed paths to the same confinement contract as any governed
            // path (DESIGN.md § Five check families): a path escaping .mdatron/
            // is a manifest-integrity failure — refuse, read nothing outside the
            // partition.
            let confined = confine_lexically(Path::new(&entry.path)).map_err(|v| {
                InitError::ManifestParse {
                    path: manifest_path.to_string_lossy().into_owned(),
                    error: format!(
                        "managed path '{}' escapes the .mdatron/ partition ({v:?})",
                        entry.path
                    ),
                }
            })?;
            let p = dir.join(confined.as_path());
            match std::fs::read(&p) {
                Ok(bytes) => {
                    let actual = sha256_hex(&bytes);
                    if actual != entry.sha256 {
                        drifts.push(Drift {
                            file: entry.path.clone(),
                            expected_sha256: entry.sha256.clone(),
                            actual_sha256: actual,
                        });
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => missing.push(i),
                Err(e) => return Err(io_err(&p, &e)),
            }
        }
        // Drift is a governance refusal — never write over a hand-modified
        // managed file. Missing managed files are repaired below.
        if !drifts.is_empty() {
            return Err(InitError::Drift(drifts));
        }

        let mut created = Vec::new();

        // Repair missing MANAGED files from engine-known content. The manifest
        // is the authority on what is managed (its entries are data — v1 trees
        // still listing config.yaml are honored); the engine only supplies the
        // bytes. The manifest is rewritten preserving its entries and
        // tombstones, with repaired entries re-hashed from the written content.
        if !missing.is_empty() {
            for &i in &missing {
                let entry = &mut manifest.managed[i];
                let Some(content) = engine_content(&entry.path) else {
                    return Err(InitError::Io {
                        path: entry.path.clone(),
                        error: "missing managed file has no engine-known content to redeploy"
                            .into(),
                    });
                };
                let p = dir.join(&entry.path);
                std::fs::write(&p, content).map_err(|e| io_err(&p, &e))?;
                entry.sha256 = sha256_hex(content.as_bytes());
                created.push(format!(".mdatron/{}", entry.path));
            }
            write_manifest(&dir, &manifest)?;
        }

        // Skeleton dirs and SEED files are re-created when absent — idempotent
        // repair without touching anything that exists (seeds are
        // adopter-owned; an edited seed is never overwritten, never refused).
        ensure_dirs(&dir)?;
        for (name, content) in SEED_FILES {
            let p = dir.join(name);
            if !p.exists() {
                std::fs::write(&p, content).map_err(|e| io_err(&p, &e))?;
                created.push(format!(".mdatron/{name}"));
            }
        }

        return if created.is_empty() {
            Ok(InitOutcome::AlreadyInitialized)
        } else {
            Ok(InitOutcome::Deployed { created })
        };
    }

    // First run: deploy the skeleton, seeds, and a fresh (v2) manifest.
    let created = deploy(&dir)?;
    Ok(InitOutcome::Deployed { created })
}

fn deploy(dir: &Path) -> Result<Vec<String>, InitError> {
    let mut created = Vec::new();

    if !dir.exists() {
        std::fs::create_dir_all(dir).map_err(|e| io_err(dir, &e))?;
        created.push(".mdatron/".to_string());
    }
    for sub in SKELETON_DIRS {
        let p = dir.join(sub);
        if !p.exists() {
            std::fs::create_dir_all(&p).map_err(|e| io_err(&p, &e))?;
            created.push(format!(".mdatron/{sub}/"));
        }
    }

    // Seeds: written on first run; adopter-owned from then on (never hashed,
    // never overwritten by later runs).
    for (name, content) in SEED_FILES {
        let p = dir.join(name);
        if !p.exists() {
            std::fs::write(&p, content).map_err(|e| io_err(&p, &e))?;
            created.push(format!(".mdatron/{name}"));
        }
    }

    // Fresh manifests start with an empty managed partition: config.yaml is a
    // seed, not a managed file (#77), and no other engine-managed file exists
    // yet. The manifest still DEFINES the partition — future engine-managed
    // files (and v1 trees' existing entries) are honored as data.
    let manifest = Manifest {
        version: MANIFEST_VERSION,
        managed: Vec::new(),
        demoted: Vec::new(),
    };
    let existed = dir.join(MANIFEST_NAME).exists();
    write_manifest(dir, &manifest)?;
    if !existed {
        created.push(format!(".mdatron/{MANIFEST_NAME}"));
    }

    Ok(created)
}

/// Serialize + write the manifest with its do-not-edit header, preserving
/// whatever entries and tombstones the given manifest carries.
fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<(), InitError> {
    let manifest_path = dir.join(MANIFEST_NAME);
    let yaml = serde_yaml_ng::to_string(manifest).map_err(|e| InitError::Io {
        path: manifest_path.to_string_lossy().into_owned(),
        error: e.to_string(),
    })?;
    let body = format!("# mdatron managed-partition manifest — do not edit by hand.\n{yaml}");
    std::fs::write(&manifest_path, body).map_err(|e| io_err(&manifest_path, &e))
}

fn ensure_dirs(dir: &Path) -> Result<(), InitError> {
    for sub in SKELETON_DIRS {
        let p = dir.join(sub);
        if !p.exists() {
            std::fs::create_dir_all(&p).map_err(|e| io_err(&p, &e))?;
        }
    }
    Ok(())
}

fn read_manifest(path: &Path) -> Result<Manifest, InitError> {
    let content = std::fs::read_to_string(path).map_err(|e| io_err(path, &e))?;
    serde_yaml_ng::from_str(&content).map_err(|e| InitError::ManifestParse {
        path: path.to_string_lossy().into_owned(),
        error: e.to_string(),
    })
}

fn io_err(path: &Path, e: &std::io::Error) -> InitError {
    InitError::Io {
        path: path.to_string_lossy().into_owned(),
        error: e.to_string(),
    }
}

/// Render a drift set as `MDATRON-E0060` findings (pin family, #50) for the CLI
/// diagnostic surface. One finding per drifted managed file.
pub fn drift_findings(project_root: &Path, drifts: &[Drift]) -> Vec<Finding> {
    drifts
        .iter()
        .map(|d| Finding {
            code: "MDATRON-E0060".into(),
            severity: Severity::Error,
            summary: "managed-manifest-drift".into(),
            message: format!(
                "managed file '{}' was modified after init (recorded {}, found {}); \
                 restore it or re-init in a clean tree",
                d.file,
                short(&d.expected_sha256),
                short(&d.actual_sha256),
            ),
            help: Some(
                "managed files are engine-owned; adopter data belongs in \
                 .mdatron/schemas/ or .mdatron/patterns/, outside the manifest"
                    .into(),
            ),
            location: Location {
                file: project_root.join(".mdatron").join(&d.file),
                line: 1,
                column: 0,
            },
            explain_ref: Some("MDATRON-E0060".into()),
            quoted: Vec::new(),
        })
        .collect()
}

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("mdatron-init-{label}-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    // DESIGN L142: first run deploys the skeleton and the init manifest.
    #[test]
    fn first_run_deploys_skeleton() {
        let root = temp_root("deploy");
        let outcome = init(&root).unwrap();
        assert!(matches!(outcome, InitOutcome::Deployed { .. }));
        assert!(root.join(".mdatron/schemas").is_dir());
        assert!(root.join(".mdatron/patterns").is_dir());
        assert!(root.join(".mdatron/config.yaml").is_file());
        assert!(root.join(".mdatron/manifest.yaml").is_file());
        std::fs::remove_dir_all(&root).unwrap();
    }

    // DESIGN L142: a second run on the unmodified tree is a no-op.
    #[test]
    fn second_run_is_noop() {
        let root = temp_root("noop");
        init(&root).unwrap();
        assert_eq!(init(&root).unwrap(), InitOutcome::AlreadyInitialized);
        std::fs::remove_dir_all(&root).unwrap();
    }

    // Security: a hand-edited manifest that lists a managed path escaping the
    // .mdatron/ partition is refused. The manifest can't hash itself, so drift
    // detection won't catch such an edit — the confinement guard must.
    #[test]
    fn manifest_path_escaping_partition_is_refused() {
        let root = temp_root("escape");
        init(&root).unwrap();
        std::fs::write(
            root.join(".mdatron/manifest.yaml"),
            "version: 1\nmanaged:\n  - path: \"../../escape.yaml\"\n    sha256: \"00\"\n",
        )
        .unwrap();
        match init(&root) {
            Err(InitError::ManifestParse { .. }) => {}
            other => panic!("expected ManifestParse refusal for escaping path, got {other:?}"),
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Hand-author a v1-style manifest (config.yaml still managed) — the shape
    /// existing adopter trees carry from before the #77 demotion.
    fn write_v1_manifest(root: &std::path::Path, sha: &str) {
        std::fs::write(
            root.join(".mdatron/manifest.yaml"),
            format!("version: 1\nmanaged:\n- path: config.yaml\n  sha256: \"{sha}\"\n"),
        )
        .unwrap();
    }

    // RED GATE FLIP (#77): config.yaml is a SEED — adopter-owned. Editing its
    // file_globs (the point of consumer-authored jurisdiction) must be neither
    // refused nor overwritten. Pre-fix this exact sequence was refused as
    // MDATRON-E0060 drift, making scoping impossible.
    #[test]
    fn edited_seed_config_is_not_refused() {
        let root = temp_root("seed-edit");
        init(&root).unwrap();
        std::fs::write(
            root.join(".mdatron/config.yaml"),
            "file_globs:\n  - \"docs/**/*.md\"\n",
        )
        .unwrap();
        assert_eq!(init(&root).unwrap(), InitOutcome::AlreadyInitialized);
        assert!(std::fs::read_to_string(root.join(".mdatron/config.yaml"))
            .unwrap()
            .contains("docs/**/*.md"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    // The drift machinery is manifest-driven data, not gone: a tree whose
    // manifest lists config.yaml as managed (v1 shape) still refuses a
    // hand-modified copy — the managed-partition contract holds wherever the
    // manifest declares it.
    #[test]
    fn manifest_listed_managed_file_drift_is_refused() {
        let root = temp_root("v1-drift");
        init(&root).unwrap();
        write_v1_manifest(&root, &sha256_hex(DEFAULT_CONFIG.as_bytes()));
        // Intact: the v1 tree is a clean no-op.
        assert_eq!(init(&root).unwrap(), InitOutcome::AlreadyInitialized);
        // Tampered: refused, not overwritten.
        std::fs::write(
            root.join(".mdatron/config.yaml"),
            "file_globs: [tampered]\n",
        )
        .unwrap();
        match init(&root) {
            Err(InitError::Drift(drifts)) => {
                assert_eq!(drifts.len(), 1);
                assert_eq!(drifts[0].file, "config.yaml");
                assert_ne!(drifts[0].expected_sha256, drifts[0].actual_sha256);
            }
            other => panic!("expected Drift refusal, got {other:?}"),
        }
        assert!(std::fs::read_to_string(root.join(".mdatron/config.yaml"))
            .unwrap()
            .contains("tampered"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    // A v1 tree's missing managed file is repaired from engine-known content,
    // preserving the manifest's own entries (the manifest defines the
    // partition; the engine only supplies bytes).
    #[test]
    fn v1_managed_missing_file_is_repaired() {
        let root = temp_root("v1-repair");
        init(&root).unwrap();
        write_v1_manifest(&root, &sha256_hex(DEFAULT_CONFIG.as_bytes()));
        std::fs::remove_file(root.join(".mdatron/config.yaml")).unwrap();
        assert!(matches!(init(&root).unwrap(), InitOutcome::Deployed { .. }));
        assert!(root.join(".mdatron/config.yaml").is_file());
        let manifest = std::fs::read_to_string(root.join(".mdatron/manifest.yaml")).unwrap();
        assert!(
            manifest.contains("config.yaml"),
            "repair must preserve the manifest's managed entry: {manifest}"
        );
        assert_eq!(init(&root).unwrap(), InitOutcome::AlreadyInitialized);
        std::fs::remove_dir_all(&root).unwrap();
    }

    // Demotion tombstones (DESIGN: removals persist) parse and survive a
    // manifest rewrite triggered by managed-file repair.
    #[test]
    fn tombstones_survive_repair() {
        let root = temp_root("tombstone");
        init(&root).unwrap();
        std::fs::write(
            root.join(".mdatron/manifest.yaml"),
            format!(
                "version: 2\nmanaged:\n- path: config.yaml\n  sha256: \"{}\"\ndemoted:\n- path: old.yaml\n  reason: \"retired at v2\"\n  owner: operator\n",
                sha256_hex(DEFAULT_CONFIG.as_bytes())
            ),
        )
        .unwrap();
        std::fs::remove_file(root.join(".mdatron/config.yaml")).unwrap();
        assert!(matches!(init(&root).unwrap(), InitOutcome::Deployed { .. }));
        let manifest = std::fs::read_to_string(root.join(".mdatron/manifest.yaml")).unwrap();
        assert!(
            manifest.contains("old.yaml") && manifest.contains("retired at v2"),
            "tombstone must survive the rewrite: {manifest}"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    // A missing seed is re-seeded (idempotent), and the tree returns to no-op.
    #[test]
    fn missing_seed_is_reseeded() {
        let root = temp_root("reseed");
        init(&root).unwrap();
        std::fs::remove_file(root.join(".mdatron/config.yaml")).unwrap();
        assert!(matches!(init(&root).unwrap(), InitOutcome::Deployed { .. }));
        assert!(root.join(".mdatron/config.yaml").is_file());
        assert_eq!(init(&root).unwrap(), InitOutcome::AlreadyInitialized);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sha256_hex_is_stable_and_lowercase() {
        // NIST FIPS-180-2 test vector for the empty input.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let h = sha256_hex(b"mdatron");
        assert_eq!(h.len(), 64);
        assert!(h
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        // Deterministic across calls.
        assert_eq!(h, sha256_hex(b"mdatron"));
    }
}
