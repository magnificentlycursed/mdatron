//! AST types for parsed pattern files.

use serde::Deserialize;
use std::fmt;

/// Top-level pattern file: a `mdatron_dsl_version` declaration + a `pattern` block.
/// Strict like the five sibling input formats (GH #52 major 4,
/// `deny_unknown_fields`): a typo'd key — `locaton:` for `location:` — used to
/// be silently dropped on the highest-risk adopter input. The version field is
/// OPTIONAL (absent = the v1 legacy baseline, GH #52 major 3 — pattern files
/// predate 0.6.0, consistent with routes/vocab/pins per DEF5) and is GATED by
/// the lenient two-pass probe in `format_version` before this strict parse, so
/// an unknown-future-version file breaks legibly, not atomically.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PatternFile {
    #[serde(default)]
    pub mdatron_dsl_version: Option<u32>,
    pub pattern: Pattern,
}

/// A named group of rules sharing a `keys:` declaration.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Pattern {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Deprecated, inert (#204 R1): parsed for DSL-v1 compatibility, never
    /// read — no phase selector exists. Announced at load as `MDATRON-W0052`;
    /// retired at DSL v2.
    #[serde(default)]
    pub phases: Vec<String>,
    #[serde(default)]
    pub keys: Vec<KeyDecl>,
    pub rules: Vec<Rule>,
}

/// A cross-file index declaration. Built once per validation pass; queryable from
/// any rule in the pattern via `key("<name>", <value>)` expressions.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub context: ContextSelector,
    /// Local bindings evaluated before the assertion, in DECLARATION order —
    /// a later binding may reference an earlier one (#89: the former BTreeMap
    /// storage evaluated alphabetically, which broke naturally chained
    /// bindings; the #47 cold-context run caught it).
    #[serde(default, rename = "let", deserialize_with = "de_let_bindings")]
    pub let_bindings: Vec<(String, String)>,
    /// Assertion expression. Stored as a string; the parser does not validate the
    /// expression's internal syntax. Rule fires when this evaluates to FALSE.
    pub assert: String,
    pub code: String,
    /// Message template with `{{...}}` interpolation slots.
    pub message: String,
    /// Deprecated, inert (#204 R2): parsed for DSL-v1 compatibility, never
    /// consumed — finding locations are the whole artifact. Announced at load
    /// as `MDATRON-W0052`; retired at DSL v2.
    #[serde(default)]
    pub location: Option<LocationSpec>,
}

/// Deserialize a YAML mapping of let-bindings preserving document order
/// (serde_yaml_ng mappings iterate in insertion order).
fn de_let_bindings<'de, D>(deserializer: D) -> Result<Vec<(String, String)>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;
    let mapping = serde_yaml_ng::Mapping::deserialize(deserializer)?;
    mapping
        .into_iter()
        .map(|(k, v)| {
            let k = k
                .as_str()
                .ok_or_else(|| D::Error::custom("let binding name must be a string"))?
                .to_string();
            let v = v
                .as_str()
                .ok_or_else(|| D::Error::custom("let binding value must be an expression string"))?
                .to_string();
            Ok((k, v))
        })
        .collect()
}

/// What artifacts a rule applies to. Three forms supported:
/// - schema_class slug as a bare string
/// - file glob as a bare string (contains `*`, `?`, or `/`)
/// - combined `{ schema_class, path }` object
///
/// Deserialized by hand (GH #52 lane-B review B1): the old `#[serde(untagged)]`
/// derive made the Combined arm — both fields optional, no
/// `deny_unknown_fields` — accept ANY mapping as `Combined { None, None }`, so
/// a typo'd key INSIDE `context:` (`schema_clas:`, `paht:`) or an empty `{}`
/// silently rewrote the rule's scope to match-everything AND suppressed the
/// W0045 unrouted-context signal. The custom impl sidesteps the
/// untagged-buffering obstacle: a string is `Bare`; a mapping deserializes
/// through a named `deny_unknown_fields` struct, and an all-`None` mapping is
/// refused (a context must select something).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextSelector {
    /// String form: either a schema_class slug or a path glob. Disambiguated by
    /// containing glob metacharacters.
    Bare(String),
    /// Object form: schema_class + optional path constraint.
    Combined {
        schema_class: Option<String>,
        path: Option<String>,
    },
}

impl<'de> serde::Deserialize<'de> for ContextSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// The strict mapping form: unknown keys refuse (B1 — a typo'd
        /// `schema_clas:`/`paht:` must never silently widen the scope).
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct CombinedRaw {
            #[serde(default)]
            schema_class: Option<String>,
            #[serde(default)]
            path: Option<String>,
        }

        struct SelectorVisitor;

        impl<'de> serde::de::Visitor<'de> for SelectorVisitor {
            type Value = ContextSelector;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    f,
                    "a context selector: a string (schema_class slug or path glob), \
                     or a mapping with schema_class and/or path"
                )
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(ContextSelector::Bare(v.to_string()))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                let raw =
                    CombinedRaw::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                if raw.schema_class.is_none() && raw.path.is_none() {
                    // `{}` has no meaning: it used to match EVERYTHING (B1).
                    return Err(serde::de::Error::custom(
                        "an object context must set schema_class and/or path \
                         (an empty context selects nothing)",
                    ));
                }
                Ok(ContextSelector::Combined {
                    schema_class: raw.schema_class,
                    path: raw.path,
                })
            }
        }

        deserializer.deserialize_any(SelectorVisitor)
    }
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
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocationSpec {
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub expression: Option<String>,
}
