//! Project configuration: `.mdatron/config.yaml`.
//!
//! The config file is **seeded** by `mdatron init` with engine defaults and is
//! adopter-owned from then on (#77): `file_globs` declare what is in mdatron's
//! jurisdiction — the consumer-authored allowlist. Everything outside the globs
//! is untouched, so third-party markdown deployed into the tree (chassis
//! command files, unrelated tooling) is not mdatron's to refuse. The
//! `MDATRON-E0002` explain page has pointed adopters at these globs since the
//! v0.1.0 catalog; this module is what makes that pointer true.

use std::path::Path;

use serde::Deserialize;

use crate::Error;

/// File name of the project config under `.mdatron/`.
pub const CONFIG_NAME: &str = "config.yaml";

/// Parsed `.mdatron/config.yaml`. Unknown fields are tolerated so the config
/// can grow without breaking older engines.
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectConfig {
    /// Globs (relative to the project root) of files mdatron validates.
    #[serde(default)]
    pub file_globs: Vec<String>,
}

/// Load `<project_root>/.mdatron/config.yaml`.
///
/// - `Ok(None)` when the file is absent (callers fall back to engine defaults).
/// - `Err` when the file exists but cannot be read or parsed — a malformed
///   governance file is loud, never a silent fallback to defaults (the same
///   absence-vs-failure asymmetry discipline as frontmatter parsing).
pub fn load(project_root: &Path) -> Result<Option<ProjectConfig>, Error> {
    let path = project_root.join(".mdatron").join(CONFIG_NAME);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(Error::Config(format!(
                "cannot read '{}': {e}",
                path.display()
            )))
        }
    };
    let cfg: ProjectConfig = serde_yaml_ng::from_str(&content)
        .map_err(|e| Error::Config(format!("cannot parse '{}': {e}", path.display())))?;
    Ok(Some(cfg))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("mdatron-config-{label}-{nanos}"));
        std::fs::create_dir_all(p.join(".mdatron")).unwrap();
        p
    }

    #[test]
    fn absent_config_is_ok_none() {
        let root = temp_root("absent");
        std::fs::remove_dir_all(root.join(".mdatron")).unwrap();
        assert!(load(&root).unwrap().is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn config_file_globs_are_loaded() {
        let root = temp_root("globs");
        std::fs::write(
            root.join(".mdatron/config.yaml"),
            "file_globs:\n  - \"docs/**/*.md\"\n  - \"notes/*.md\"\n",
        )
        .unwrap();
        let cfg = load(&root).unwrap().expect("config present");
        assert_eq!(cfg.file_globs, vec!["docs/**/*.md", "notes/*.md"]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn unknown_fields_are_tolerated() {
        let root = temp_root("unknown");
        std::fs::write(
            root.join(".mdatron/config.yaml"),
            "file_globs: [\"**/*.md\"]\nfuture_option: true\n",
        )
        .unwrap();
        assert!(load(&root).unwrap().is_some());
        std::fs::remove_dir_all(&root).unwrap();
    }

    // Malformed config is loud — never a silent fallback to engine defaults.
    #[test]
    fn malformed_config_is_err_not_silent_default() {
        let root = temp_root("malformed");
        std::fs::write(root.join(".mdatron/config.yaml"), ": not: yaml: [").unwrap();
        assert!(matches!(load(&root), Err(Error::Config(_))));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
