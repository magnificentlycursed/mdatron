//! Cross-file index: compiles `keys:` block declarations into queryable indices.
//!
//! Given a [`KeyDecl`] like:
//! ```yaml
//! - name: domain-prompts
//!   source: .claude/commands/vsdd-domain-*.md
//!   select: $.frontmatter
//!   indexed_by: $.domain_slug
//! ```
//! the index module:
//! 1. resolves `source` (literal path or glob) under `project_root`
//! 2. parses each file (.yaml/.yml/.json/.md)
//! 3. applies `select` (mini-JSONPath) to extract entries
//! 4. uses `indexed_by` (mini-JSONPath or `$key` for object-as-mapping) to bucket
//!    each entry under a string key
//!
//! The resulting [`Index`] is queryable via [`Index::lookup`]; multiple indices
//! live in an [`IndexRegistry`] keyed by `name`. Rules reference indices via the
//! `key()` standard-library function (`key("domain-prompts", "software-engineer")`).
//!
//! Path-confinement (DESIGN.md § Five check families; carried from
//! BOUNDARY-PREAMBLE § 7): sources are confined lexically before any
//! filesystem access — absolute paths and parent segments are rejected
//! whether or not the target exists — and every read goes through a
//! validated no-follow handle from [`crate::confine`], so a symlink at any
//! path component is refused.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::confine;

use super::expr::Value;
use super::types::KeyDecl;

// ── Public surface ─────────────────────────────────────────────────────────────

/// A single compiled cross-file index.
#[derive(Debug, Clone)]
pub struct Index {
    pub name: String,
    pub entries: BTreeMap<String, Value>,
}

impl Index {
    pub fn lookup(&self, key: &str) -> Option<&Value> {
        self.entries.get(key)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Registry of named indices, built once per validation pass.
#[derive(Debug, Default, Clone)]
pub struct IndexRegistry {
    pub indices: BTreeMap<String, Index>,
}

impl IndexRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a registry from a list of key declarations rooted at `project_root`.
    pub fn build(project_root: &Path, decls: &[KeyDecl]) -> Result<Self, IndexError> {
        let mut indices = BTreeMap::new();
        for decl in decls {
            let index = build_index(project_root, decl)?;
            indices.insert(decl.name.clone(), index);
        }
        Ok(Self { indices })
    }

    /// Look up an entry by index name + key. Returns `None` if either is unknown.
    pub fn lookup(&self, index_name: &str, key: &str) -> Option<&Value> {
        self.indices.get(index_name).and_then(|idx| idx.lookup(key))
    }

    /// Insert a pre-built index (for testing or bootstrap scenarios).
    pub fn insert(&mut self, index: Index) {
        self.indices.insert(index.name.clone(), index);
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IndexError {
    #[error("glob error in '{pattern}': {error}")]
    Glob { pattern: String, error: String },

    #[error("file IO at '{path}': {error}")]
    Io { path: String, error: String },

    #[error("file parse at '{path}': {error}")]
    Parse { path: String, error: String },

    /// Maps to MDATRON-E0010: key-source-absolute-path per DESIGN.md § Five
    /// check families (carried from BOUNDARY-PREAMBLE § 7).
    #[error("path confinement: absolute source path '{path}' is rejected; sources resolve relative to the project root (MDATRON-E0010)")]
    AbsoluteSource { path: String },

    /// Maps to MDATRON-E0011: key-source-parent-traversal per DESIGN.md § Five
    /// check families (carried from BOUNDARY-PREAMBLE § 7). Decided lexically,
    /// so non-existent targets are rejected on the same basis as existing ones.
    #[error("path traversal: '{path}' escapes project root (MDATRON-E0011)")]
    PathTraversal { path: String },

    /// Maps to MDATRON-E0012: key-source-symlink-refused. No-follow resolution
    /// refuses a symlink at any component, whatever its target.
    #[error("path confinement: symlink component '{component}' in '{path}' is refused under no-follow resolution (MDATRON-E0012)")]
    SymlinkRefused { path: String, component: String },

    #[error("unsupported file type at '{path}' (extension: '{ext}')")]
    UnsupportedFileType { path: String, ext: String },

    #[error("selection '{select}' on '{path}': {error}")]
    Selection {
        path: String,
        select: String,
        error: String,
    },
}

// ── Build pipeline ─────────────────────────────────────────────────────────────

fn build_index(project_root: &Path, decl: &KeyDecl) -> Result<Index, IndexError> {
    let source = decl.source.trim();
    let mut entries: BTreeMap<String, Value> = BTreeMap::new();

    for rel in resolve_source(project_root, source)? {
        let display = project_root.join(&rel);
        let file = confine::open_confined(project_root, &rel).map_err(|v| match v {
            confine::OpenViolation::Symlink { component } => IndexError::SymlinkRefused {
                path: display.to_string_lossy().into_owned(),
                component: component.to_string_lossy().into_owned(),
            },
            confine::OpenViolation::Io(e) => IndexError::Io {
                path: display.to_string_lossy().into_owned(),
                error: e.to_string(),
            },
        })?;
        extract_into(&display, file, &decl.select, &decl.indexed_by, &mut entries)?;
    }

    Ok(Index {
        name: decl.name.clone(),
        entries,
    })
}

/// Resolve a source declaration to root-relative paths. Confinement is decided
/// lexically here — before glob expansion and without touching the filesystem —
/// so absolute paths and parent segments are rejected whether or not the
/// target exists, and glob patterns may not contain parent segments.
fn resolve_source(project_root: &Path, source: &str) -> Result<Vec<PathBuf>, IndexError> {
    let rel = confine::confine_lexically(Path::new(source)).map_err(|v| match v {
        confine::LexicalViolation::Absolute => IndexError::AbsoluteSource {
            path: source.to_string(),
        },
        confine::LexicalViolation::ParentSegment => IndexError::PathTraversal {
            path: source.to_string(),
        },
    })?;

    let is_glob = source.contains('*') || source.contains('?') || source.contains('[');

    let mut out = Vec::new();
    if is_glob {
        let pattern = project_root.join(&rel);
        let pattern_str = pattern.to_string_lossy().into_owned();
        let glob_paths = glob::glob(&pattern_str).map_err(|e| IndexError::Glob {
            pattern: source.to_string(),
            error: e.to_string(),
        })?;
        for entry in glob_paths {
            let path = entry.map_err(|e| IndexError::Glob {
                pattern: source.to_string(),
                error: e.to_string(),
            })?;
            // Matches are textual expansions of the root-joined pattern;
            // re-derive the root-relative path for the handle-based open.
            let rel = path
                .strip_prefix(project_root)
                .map_err(|_| IndexError::PathTraversal {
                    path: path.to_string_lossy().into_owned(),
                })?;
            out.push(rel.to_path_buf());
        }
    } else {
        out.push(rel);
    }
    Ok(out)
}

fn extract_into(
    path: &Path,
    file: File,
    select: &str,
    indexed_by: &str,
    out: &mut BTreeMap<String, Value>,
) -> Result<(), IndexError> {
    let parsed = parse_file_to_value(path, file)?;
    let selected = apply_select(&parsed, select).map_err(|e| IndexError::Selection {
        path: path.to_string_lossy().into_owned(),
        select: select.to_string(),
        error: e,
    })?;

    if indexed_by.trim() == "$key" {
        match selected {
            Value::Object(obj) => {
                for (k, v) in obj {
                    out.insert(k, v);
                }
                Ok(())
            }
            other => Err(IndexError::Selection {
                path: path.to_string_lossy().into_owned(),
                select: select.to_string(),
                error: format!(
                    "indexed_by '$key' requires the selected value to be an object; got {}",
                    other.type_name()
                ),
            }),
        }
    } else {
        // Selected is one entry; extract its key via indexed_by select.
        let key_value = apply_select(&selected, indexed_by).map_err(|e| IndexError::Selection {
            path: path.to_string_lossy().into_owned(),
            select: indexed_by.to_string(),
            error: e,
        })?;
        let key_str = match key_value {
            Value::Str(s) => s,
            other => {
                return Err(IndexError::Selection {
                    path: path.to_string_lossy().into_owned(),
                    select: indexed_by.to_string(),
                    error: format!(
                        "indexed_by must yield a string key; got {}",
                        other.type_name()
                    ),
                });
            }
        };
        out.insert(key_str, selected);
        Ok(())
    }
}

// ── File parsing ───────────────────────────────────────────────────────────────

/// Parse from an already-confined handle; `path` is display-only. Reading
/// through the handle keeps confinement decided on the handle rather than on
/// a path re-walked at read time (the check-then-read gap, DESIGN.md
/// § Verification is fast where it is invoked).
fn parse_file_to_value(path: &Path, mut file: File) -> Result<Value, IndexError> {
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| IndexError::Io {
            path: path.to_string_lossy().into_owned(),
            error: e.to_string(),
        })?;

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "yaml" | "yml" => {
            let yaml: serde_yaml::Value =
                serde_yaml::from_str(&content).map_err(|e| IndexError::Parse {
                    path: path.to_string_lossy().into_owned(),
                    error: e.to_string(),
                })?;
            Ok(yaml_to_value(&yaml))
        }
        "json" => {
            let json: serde_json::Value =
                serde_json::from_str(&content).map_err(|e| IndexError::Parse {
                    path: path.to_string_lossy().into_owned(),
                    error: e.to_string(),
                })?;
            Ok(json_to_value(&json))
        }
        "md" => {
            // Markdown file: parse frontmatter, expose as $.frontmatter.
            let fm_opt = crate::frontmatter::parse(&content).map_err(|e| IndexError::Parse {
                path: path.to_string_lossy().into_owned(),
                error: e.to_string(),
            })?;
            let fm_value = match fm_opt {
                Some((fm, _body)) => yaml_to_value(&fm),
                None => Value::Null,
            };
            let mut wrapper = BTreeMap::new();
            wrapper.insert("frontmatter".to_string(), fm_value);
            Ok(Value::Object(wrapper))
        }
        _ => Err(IndexError::UnsupportedFileType {
            path: path.to_string_lossy().into_owned(),
            ext,
        }),
    }
}

// ── JSONPath subset ────────────────────────────────────────────────────────────

/// Apply a tiny JSONPath subset: "$" returns the root; "$.field.subfield" walks fields.
fn apply_select(value: &Value, select: &str) -> Result<Value, String> {
    let select = select.trim();
    if select == "$" {
        return Ok(value.clone());
    }
    let path_part = select
        .strip_prefix("$.")
        .ok_or_else(|| format!("select must start with '$.' or be '$'; got '{select}'"))?;
    let mut current = value.clone();
    for segment in path_part.split('.') {
        if segment.is_empty() {
            return Err(format!("empty path segment in '{select}'"));
        }
        match current {
            Value::Object(obj) => {
                current = obj
                    .get(segment)
                    .cloned()
                    .ok_or_else(|| format!("field '{segment}' not found"))?;
            }
            Value::Null => return Ok(Value::Null),
            other => {
                return Err(format!(
                    "cannot access field '{segment}' on {}",
                    other.type_name()
                ));
            }
        }
    }
    Ok(current)
}

// ── Value conversions ──────────────────────────────────────────────────────────

pub(crate) fn yaml_to_value(yaml: &serde_yaml::Value) -> Value {
    match yaml {
        serde_yaml::Value::Null => Value::Null,
        serde_yaml::Value::Bool(b) => Value::Bool(*b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else {
                // Floats not represented in v0.1.x; map to null with explicit lossy semantics.
                Value::Null
            }
        }
        serde_yaml::Value::String(s) => Value::Str(s.clone()),
        serde_yaml::Value::Sequence(seq) => Value::Array(seq.iter().map(yaml_to_value).collect()),
        serde_yaml::Value::Mapping(map) => {
            let mut obj = BTreeMap::new();
            for (k, v) in map {
                if let Some(k_str) = k.as_str() {
                    obj.insert(k_str.to_string(), yaml_to_value(v));
                }
            }
            Value::Object(obj)
        }
        _ => Value::Null,
    }
}

fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => n.as_i64().map(Value::Int).unwrap_or(Value::Null),
        serde_json::Value::String(s) => Value::Str(s.clone()),
        serde_json::Value::Array(a) => Value::Array(a.iter().map(json_to_value).collect()),
        serde_json::Value::Object(o) => {
            let mut obj = BTreeMap::new();
            for (k, v) in o {
                obj.insert(k.clone(), json_to_value(v));
            }
            Value::Object(obj)
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("mdatron-index-{label}-{nanos}"));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, rel: &str, content: &str) -> PathBuf {
            let full = self.0.join(rel);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&full, content).unwrap();
            full
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn decl(name: &str, source: &str, select: &str, indexed_by: &str) -> KeyDecl {
        KeyDecl {
            name: name.to_string(),
            source: source.to_string(),
            select: select.to_string(),
            indexed_by: indexed_by.to_string(),
        }
    }

    // ── apply_select ────────────────────────────────────────────────────────

    #[test]
    fn select_root_returns_whole_value() {
        let v = Value::Int(7);
        assert_eq!(apply_select(&v, "$").unwrap(), Value::Int(7));
    }

    #[test]
    fn select_simple_field() {
        let v = Value::Object(
            [("phase".to_string(), Value::Str("phase-2a".to_string()))]
                .into_iter()
                .collect(),
        );
        assert_eq!(
            apply_select(&v, "$.phase").unwrap(),
            Value::Str("phase-2a".into())
        );
    }

    #[test]
    fn select_nested_field() {
        let inner: BTreeMap<String, Value> = [(
            "required".to_string(),
            Value::Array(vec![Value::Str("se".into())]),
        )]
        .into_iter()
        .collect();
        let outer: BTreeMap<String, Value> = [("matrix".to_string(), Value::Object(inner))]
            .into_iter()
            .collect();
        let v = Value::Object(outer);
        assert_eq!(
            apply_select(&v, "$.matrix.required").unwrap(),
            Value::Array(vec![Value::Str("se".into())])
        );
    }

    #[test]
    fn select_missing_field_errors() {
        let v = Value::Object(BTreeMap::new());
        assert!(apply_select(&v, "$.missing").is_err());
    }

    #[test]
    fn select_on_null_propagates_null() {
        let v = Value::Object([("matrix".to_string(), Value::Null)].into_iter().collect());
        assert_eq!(apply_select(&v, "$.matrix.required").unwrap(), Value::Null);
    }

    #[test]
    fn select_must_start_with_dollar() {
        assert!(apply_select(&Value::Null, "no-dollar").is_err());
    }

    // ── End-to-end: single YAML source with $key index ──────────────────────

    #[test]
    fn builds_index_from_single_yaml_source_with_key_as_index() {
        let temp = TempDir::new("yaml-key");
        temp.write(
            "matrix.yaml",
            "matrix:\n  phase-2a:\n    required: [se, qe]\n    allowed: [sa]\n  phase-1a:\n    required: [so]\n",
        );

        let d = decl("composition-matrix", "matrix.yaml", "$.matrix", "$key");
        let registry = IndexRegistry::build(temp.path(), &[d]).unwrap();
        let idx = registry.indices.get("composition-matrix").unwrap();
        assert_eq!(idx.len(), 2);
        assert!(idx.lookup("phase-2a").is_some());
        assert!(idx.lookup("phase-1a").is_some());

        // Confirm a field on the looked-up entry.
        let phase_2a = idx.lookup("phase-2a").unwrap();
        let required = apply_select(phase_2a, "$.required").unwrap();
        assert_eq!(
            required,
            Value::Array(vec![Value::Str("se".into()), Value::Str("qe".into())])
        );
    }

    // ── End-to-end: glob markdown source with $.frontmatter ─────────────────

    #[test]
    fn builds_index_from_glob_markdown_with_frontmatter_field_index() {
        let temp = TempDir::new("md-glob");
        temp.write(
            ".claude/commands/vsdd-domain-software-engineer.md",
            "---\ndomain_slug: software-engineer\ntier: core\nvalidator_pair: solution-architect\n---\n# body\n",
        );
        temp.write(
            ".claude/commands/vsdd-domain-quality-engineer.md",
            "---\ndomain_slug: quality-engineer\ntier: core\nvalidator_pair: solution-architect\n---\n# body\n",
        );

        let d = decl(
            "domain-prompts",
            ".claude/commands/vsdd-domain-*.md",
            "$.frontmatter",
            "$.domain_slug",
        );
        let registry = IndexRegistry::build(temp.path(), &[d]).unwrap();
        let idx = registry.indices.get("domain-prompts").unwrap();
        assert_eq!(idx.len(), 2);

        let se = idx.lookup("software-engineer").unwrap();
        let tier = apply_select(se, "$.tier").unwrap();
        assert_eq!(tier, Value::Str("core".into()));
    }

    #[test]
    fn empty_glob_yields_empty_index() {
        let temp = TempDir::new("empty");
        let d = decl("nothing", "missing-*.yaml", "$", "$key");
        let registry = IndexRegistry::build(temp.path(), &[d]).unwrap();
        assert!(registry.indices.get("nothing").unwrap().is_empty());
    }

    // ── Path-confinement (MDATRON-E0010/E0011/E0012) ────────────────────────
    //
    // Rejection is lexical, so none of the traversal tests depend on
    // canonicalization divergence (the predecessor test passed only because
    // macOS's temp_dir sits behind the /var → /private/var symlink; see the
    // path-confinement defect issue in this tracker).

    #[test]
    fn path_traversal_via_parent_dots_is_rejected() {
        let temp = TempDir::new("escape");
        let d = decl("escape", "../escape-target.yaml", "$", "$key");
        let err = IndexRegistry::build(temp.path(), &[d]).unwrap_err();
        match err {
            IndexError::PathTraversal { .. } => {}
            other => panic!("expected PathTraversal, got {other:?}"),
        }
    }

    #[test]
    fn parent_traversal_rejected_when_target_exists() {
        // The escaping target really exists one level above the root: the
        // rejection must not depend on the non-existence fallback either.
        let temp = TempDir::new("escape-existent");
        let sibling = temp
            .path()
            .parent()
            .unwrap()
            .join("mdatron-escape-existent.yaml");
        std::fs::write(&sibling, "k: v\n").unwrap();
        let d = decl("escape", "../mdatron-escape-existent.yaml", "$", "$key");
        let err = IndexRegistry::build(temp.path(), &[d]).unwrap_err();
        std::fs::remove_file(&sibling).unwrap();
        assert!(matches!(err, IndexError::PathTraversal { .. }));
    }

    #[test]
    fn deep_parent_traversal_with_nonexistent_target_is_rejected() {
        // The predecessor fell back to the un-normalized textual path for
        // non-existent targets, and component-wise starts_with let
        // `root/../..` pass. Lexical rejection needs no filesystem at all.
        let temp = TempDir::new("escape-absent");
        let d = decl("escape", "../../no-such-file-anywhere.yaml", "$", "$key");
        let err = IndexRegistry::build(temp.path(), &[d]).unwrap_err();
        assert!(matches!(err, IndexError::PathTraversal { .. }));
    }

    #[test]
    fn interior_parent_segment_rejected_even_when_non_escaping() {
        let temp = TempDir::new("interior-dots");
        temp.write("matrix.yaml", "matrix:\n  a: 1\n");
        let d = decl("m", "sub/../matrix.yaml", "$.matrix", "$key");
        let err = IndexRegistry::build(temp.path(), &[d]).unwrap_err();
        assert!(matches!(err, IndexError::PathTraversal { .. }));
    }

    #[test]
    fn glob_pattern_with_parent_segment_is_rejected() {
        let temp = TempDir::new("glob-dots");
        let d = decl("g", "../*.yaml", "$", "$key");
        let err = IndexRegistry::build(temp.path(), &[d]).unwrap_err();
        assert!(matches!(err, IndexError::PathTraversal { .. }));
    }

    #[test]
    fn absolute_source_rejected_even_when_inside_root() {
        // Absolute paths are rejected outright (MDATRON-E0010), including
        // absolute spellings of in-root files.
        let temp = TempDir::new("absolute");
        let inside = temp.write("inside.yaml", "k: v\n");
        let d = decl("abs", &inside.to_string_lossy(), "$", "$key");
        let err = IndexRegistry::build(temp.path(), &[d]).unwrap_err();
        assert!(matches!(err, IndexError::AbsoluteSource { .. }));
    }

    #[test]
    fn absolute_source_with_nonexistent_target_rejected() {
        let temp = TempDir::new("absolute-absent");
        let d = decl("abs", "/no-such-mdatron-target.yaml", "$", "$key");
        let err = IndexRegistry::build(temp.path(), &[d]).unwrap_err();
        assert!(matches!(err, IndexError::AbsoluteSource { .. }));
    }

    #[test]
    fn confined_but_missing_source_is_io_not_traversal() {
        // A non-existent target inside the root is an IO condition, not a
        // confinement violation — no silent degradation, no false traversal.
        let temp = TempDir::new("missing-confined");
        let d = decl("m", "missing.yaml", "$", "$key");
        let err = IndexRegistry::build(temp.path(), &[d]).unwrap_err();
        assert!(matches!(err, IndexError::Io { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_source_refused_even_when_target_is_inside_root() {
        // No-follow is unconditional: a symlink is refused whatever its
        // target, so escape detection never depends on resolving it.
        let temp = TempDir::new("symlink-inside");
        temp.write("real.yaml", "k: v\n");
        std::os::unix::fs::symlink(
            temp.path().join("real.yaml"),
            temp.path().join("alias.yaml"),
        )
        .unwrap();
        let d = decl("s", "alias.yaml", "$", "$key");
        let err = IndexRegistry::build(temp.path(), &[d]).unwrap_err();
        assert!(matches!(err, IndexError::SymlinkRefused { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_source_pointing_outside_root_refused() {
        let temp = TempDir::new("symlink-escape");
        let outside = TempDir::new("symlink-escape-target");
        outside.write("target.yaml", "k: v\n");
        std::os::unix::fs::symlink(
            outside.path().join("target.yaml"),
            temp.path().join("link.yaml"),
        )
        .unwrap();
        let d = decl("s", "link.yaml", "$", "$key");
        let err = IndexRegistry::build(temp.path(), &[d]).unwrap_err();
        assert!(matches!(err, IndexError::SymlinkRefused { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_intermediate_component_refused() {
        // Component-wise resolution: a symlinked directory in the middle of
        // the path is as confined as a symlinked final component.
        let temp = TempDir::new("symlink-mid");
        let outside = TempDir::new("symlink-mid-target");
        outside.write("data.yaml", "k: v\n");
        std::os::unix::fs::symlink(outside.path(), temp.path().join("sub")).unwrap();
        let d = decl("s", "sub/data.yaml", "$", "$key");
        let err = IndexRegistry::build(temp.path(), &[d]).unwrap_err();
        match err {
            IndexError::SymlinkRefused { component, .. } => assert_eq!(component, "sub"),
            other => panic!("expected SymlinkRefused, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn glob_matched_symlink_refused() {
        // Glob expansion may match a symlink; the handle-based open still
        // refuses it (closed-world no-follow, DESIGN.md § Five check families).
        let temp = TempDir::new("glob-symlink");
        let outside = TempDir::new("glob-symlink-target");
        outside.write("target.yaml", "k: v\n");
        std::os::unix::fs::symlink(
            outside.path().join("target.yaml"),
            temp.path().join("linked.yaml"),
        )
        .unwrap();
        let d = decl("g", "*.yaml", "$", "$key");
        let err = IndexRegistry::build(temp.path(), &[d]).unwrap_err();
        assert!(matches!(err, IndexError::SymlinkRefused { .. }));
    }

    // ── File-type errors ────────────────────────────────────────────────────

    #[test]
    fn unsupported_extension_errors() {
        let temp = TempDir::new("unsupported");
        temp.write("data.txt", "just text");
        let d = decl("data", "data.txt", "$", "$key");
        let err = IndexRegistry::build(temp.path(), &[d]).unwrap_err();
        assert!(matches!(err, IndexError::UnsupportedFileType { .. }));
    }

    #[test]
    fn malformed_yaml_errors() {
        let temp = TempDir::new("bad-yaml");
        temp.write("bad.yaml", ": invalid: : :");
        let d = decl("bad", "bad.yaml", "$", "$key");
        let err = IndexRegistry::build(temp.path(), &[d]).unwrap_err();
        assert!(matches!(err, IndexError::Parse { .. }));
    }

    // ── Multi-decl registry ─────────────────────────────────────────────────

    #[test]
    fn registry_holds_multiple_indices() {
        let temp = TempDir::new("multi");
        temp.write("matrix.yaml", "matrix:\n  phase-1a:\n    required: [so]\n");
        temp.write(
            ".claude/commands/vsdd-domain-software-engineer.md",
            "---\ndomain_slug: software-engineer\ntier: core\nvalidator_pair: sa\n---\n",
        );

        let d1 = decl("matrix", "matrix.yaml", "$.matrix", "$key");
        let d2 = decl(
            "domains",
            ".claude/commands/vsdd-domain-*.md",
            "$.frontmatter",
            "$.domain_slug",
        );
        let registry = IndexRegistry::build(temp.path(), &[d1, d2]).unwrap();
        assert!(registry.lookup("matrix", "phase-1a").is_some());
        assert!(registry.lookup("domains", "software-engineer").is_some());
        assert!(registry.lookup("matrix", "missing").is_none());
        assert!(registry.lookup("unknown-index", "anything").is_none());
    }
}
