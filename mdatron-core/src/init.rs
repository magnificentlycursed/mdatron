//! `mdatron init`: deploy the `.mdatron/` skeleton and its managed-partition
//! manifest, idempotently, refusing drifted managed files.
//!
//! Per `DESIGN.md` § Init: the skeleton is the schema and pattern directories,
//! an engine-default config, and the init manifest defining the managed
//! partition. Managed files — engine-deployed, listed in the manifest with
//! sha256 content hashes — are drift-refused; adopter-authored data (their
//! schemas and patterns) lives outside the manifest and is never touched. The
//! manifest cannot hash itself (a fixed point, `DESIGN.md` § Governance data is
//! governed); its own integrity is anchored by commit review.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::confine::confine_lexically;
use crate::diagnostic::{Finding, Location, Severity};

/// The engine-default config (a managed file). The verify pipeline's config
/// consumption lands in a later increment; the file is deployed and
/// drift-guarded now so the managed partition is non-empty from first init.
const DEFAULT_CONFIG: &str = "\
# mdatron configuration — managed by `mdatron init`.
# Adopter schemas live in .mdatron/schemas/, patterns in .mdatron/patterns/.
file_globs:
  - \"**/*.md\"
";

/// Files the engine deploys and manages (drift-guarded), relative to
/// `.mdatron/`.
const MANAGED_FILES: &[(&str, &str)] = &[("config.yaml", DEFAULT_CONFIG)];

/// Directories the skeleton creates under `.mdatron/`.
const SKELETON_DIRS: &[&str] = &["schemas", "patterns"];

const MANIFEST_NAME: &str = "manifest.yaml";
const MANIFEST_VERSION: u32 = 1;

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
    /// The manifest never lists itself (fixed point).
    managed: Vec<ManagedEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ManagedEntry {
    path: String,
    sha256: String,
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
/// - No manifest present → first run: deploy the skeleton, managed files, and
///   manifest.
/// - Manifest present, every managed file intact and matching → no-op
///   ([`InitOutcome::AlreadyInitialized`]).
/// - A managed file hand-modified (hash mismatch) → refuse
///   ([`InitError::Drift`]); nothing is written.
/// - A managed file missing → repaired (re-deployed); the manifest is rewritten.
pub fn init(project_root: &Path) -> Result<InitOutcome, InitError> {
    let dir = project_root.join(".mdatron");
    let manifest_path = dir.join(MANIFEST_NAME);

    if manifest_path.exists() {
        let manifest = read_manifest(&manifest_path)?;
        let mut drifts = Vec::new();
        let mut missing = false;
        for entry in &manifest.managed {
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
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => missing = true,
                Err(e) => return Err(io_err(&p, &e)),
            }
        }
        // Drift is a governance refusal — never write over a hand-modified
        // managed file. Missing managed files are repaired below.
        if !drifts.is_empty() {
            return Err(InitError::Drift(drifts));
        }
        if !missing {
            // Intact — but the skeleton dirs may have been removed; recreate
            // them idempotently, then report no-op.
            ensure_dirs(&dir)?;
            return Ok(InitOutcome::AlreadyInitialized);
        }
    }

    // First run or repair: deploy everything.
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

    let mut managed = Vec::new();
    for (name, content) in MANAGED_FILES {
        let p = dir.join(name);
        let existed = p.exists();
        std::fs::write(&p, content).map_err(|e| io_err(&p, &e))?;
        if !existed {
            created.push(format!(".mdatron/{name}"));
        }
        managed.push(ManagedEntry {
            path: (*name).to_string(),
            sha256: sha256_hex(content.as_bytes()),
        });
    }

    let manifest = Manifest {
        version: MANIFEST_VERSION,
        managed,
    };
    let manifest_path = dir.join(MANIFEST_NAME);
    let existed = manifest_path.exists();
    let yaml = serde_yaml_ng::to_string(&manifest).map_err(|e| InitError::Io {
        path: manifest_path.to_string_lossy().into_owned(),
        error: e.to_string(),
    })?;
    let body = format!("# mdatron managed-partition manifest — do not edit by hand.\n{yaml}");
    std::fs::write(&manifest_path, body).map_err(|e| io_err(&manifest_path, &e))?;
    if !existed {
        created.push(format!(".mdatron/{MANIFEST_NAME}"));
    }

    Ok(created)
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

    // DESIGN L142: a hand-modified managed file causes refusal.
    #[test]
    fn drifted_managed_file_is_refused() {
        let root = temp_root("drift");
        init(&root).unwrap();
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
        // The refusal must not overwrite the hand-modified file.
        assert!(std::fs::read_to_string(root.join(".mdatron/config.yaml"))
            .unwrap()
            .contains("tampered"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    // A missing managed file is repaired (idempotent), not refused.
    #[test]
    fn missing_managed_file_is_repaired() {
        let root = temp_root("repair");
        init(&root).unwrap();
        std::fs::remove_file(root.join(".mdatron/config.yaml")).unwrap();
        assert!(matches!(init(&root).unwrap(), InitOutcome::Deployed { .. }));
        assert!(root.join(".mdatron/config.yaml").is_file());
        // And the repaired tree is now a clean no-op.
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
