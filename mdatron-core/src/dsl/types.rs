//! AST types for parsed pattern files.

use serde::Deserialize;
use std::collections::BTreeMap;

/// Top-level pattern file: a `mdatron_dsl_version` declaration + a `pattern` block.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct PatternFile {
    pub mdatron_dsl_version: u32,
    pub pattern: Pattern,
}

/// A named group of rules sharing a `keys:` declaration + optional `phases:` selection.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Pattern {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub phases: Vec<String>,
    #[serde(default)]
    pub keys: Vec<KeyDecl>,
    pub rules: Vec<Rule>,
}

/// A cross-file index declaration. Built once per validation pass; queryable from
/// any rule in the pattern via `key("<name>", <value>)` expressions.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct KeyDecl {
    pub name: String,
    /// File path or glob pattern from which entries are extracted.
    pub source: String,
    /// JSONPath into each source file's parsed structure that yields the entries.
    pub select: String,
    /// JSONPath into each entry that yields the index key.
    pub indexed_by: String,
}

/// A single declarative rule.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub context: ContextSelector,
    /// Local bindings evaluated before the assertion. Stored as a string-to-string
    /// map for now; expression parsing lands in the evaluator iteration. Uses BTreeMap
    /// for stable ordering across runs.
    #[serde(default, rename = "let")]
    pub let_bindings: BTreeMap<String, String>,
    /// Assertion expression. Stored as a string; the parser does not validate the
    /// expression's internal syntax. Rule fires when this evaluates to FALSE.
    pub assert: String,
    pub code: String,
    /// Message template with `{{...}}` interpolation slots.
    pub message: String,
    #[serde(default)]
    pub location: Option<LocationSpec>,
}

/// What artifacts a rule applies to. Three forms supported:
/// - schema_class slug as a bare string
/// - file glob as a bare string (contains `*`, `?`, or `/`)
/// - combined `{ schema_class, path }` object
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ContextSelector {
    /// String form: either a schema_class slug or a path glob. Disambiguated by
    /// containing glob metacharacters.
    Bare(String),
    /// Object form: schema_class + optional path constraint.
    Combined {
        #[serde(default)]
        schema_class: Option<String>,
        #[serde(default)]
        path: Option<String>,
    },
}

impl ContextSelector {
    /// Returns true if this selector is a path glob (contains `*`, `?`, or `/`).
    pub fn is_path_glob(&self) -> bool {
        match self {
            Self::Bare(s) => s.contains('*') || s.contains('?') || s.contains('/'),
            Self::Combined { path: Some(p), .. } => {
                p.contains('*') || p.contains('?') || p.contains('/')
            }
            _ => false,
        }
    }
}

/// Override for the source-span location attached to a finding. When absent, the
/// validator defaults to the whole-artifact span.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct LocationSpec {
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub expression: Option<String>,
}
