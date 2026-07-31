//! Layer 1: JSON Schema dispatch for frontmatter validation.
//!
//! Adopters ship JSON Schema files (draft 2020-12) at `.mdatron/schemas/<class>.json`.
//! mdatron parses YAML frontmatter from markdown files via [`crate::frontmatter::parse`],
//! then validates the parsed value against a compiled [`Schema`] via this module.
//!
//! Validation failures produce [`ValidationError`]s that the caller maps to
//! [`crate::Finding`]s with appropriate `code`/`severity`/`location` per the adopter's
//! error catalog. This module is content-neutral about the codes; it only describes
//! WHAT failed and WHERE in the value tree.

use crate::diagnostic::QuotedRegion;
use crate::Error;
use serde_json::Value as JsonValue;
use serde_yaml_ng::Value as YamlValue;

/// A compiled JSON Schema ready for validation against many instances.
///
/// Compile once per schema file; validate many times. Wraps the `jsonschema` crate's
/// `Validator` type so adopters never need to depend on it directly.
pub struct Schema {
    compiled: jsonschema::Validator,
}

/// A single validation failure: the JSONPath where the failure occurred + a human-readable
/// message. Adopters wrap this in a `Finding` with appropriate code and severity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// JSONPath into the instance value (e.g. `/relevant_domains/0` or empty string at root).
    pub instance_path: String,
    /// Schema path that produced the violation (e.g. `/properties/phase/enum`).
    pub schema_path: String,
    /// Engine-authored message describing WHAT failed. Carries no adopter-derived
    /// document content inline: the failing value and any adopter-authored keys
    /// live in [`Self::quoted`] instead (per `DESIGN.md` § Output).
    pub message: String,
    /// Adopter-derived text from the failing document (the value that failed, or
    /// the unexpected keys it carried), for prefix-marked rendering rather than
    /// inline interpolation.
    pub quoted: Vec<QuotedRegion>,
}

impl Schema {
    /// Compile a schema from its parsed JSON representation.
    ///
    /// Returns an error if the schema is not valid JSON Schema draft 2020-12.
    pub fn compile(schema_json: &JsonValue) -> Result<Self, Error> {
        // #135 (roast C1): validate `pattern`/`patternProperties` with the
        // LINEAR-time `regex` engine, not the default backtracking `fancy-regex`.
        // A backtracking engine on adopter-authored patterns over contributor
        // frontmatter is a ReDoS surface on the verify gate; the linear engine
        // guarantees O(n) matching (DESIGN § Validation: linear-time engines), at
        // the cost of rejecting look-around/backreferences (which we do not need
        // for frontmatter shape validation).
        let compiled = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .with_pattern_options(jsonschema::PatternOptions::regex())
            .build(schema_json)
            .map_err(|e| Error::Config(format!("schema compile failed: {e}")))?;
        Ok(Self { compiled })
    }

    /// Validate a YAML frontmatter value against this schema.
    ///
    /// Returns an empty `Vec` on success; one or more [`ValidationError`]s when the
    /// frontmatter does not conform.
    pub fn validate(&self, frontmatter: &YamlValue) -> Vec<ValidationError> {
        let json = match yaml_to_json(frontmatter) {
            Some(j) => j,
            None => {
                return vec![ValidationError {
                    instance_path: String::new(),
                    schema_path: String::new(),
                    message: "frontmatter could not be converted to JSON for validation".into(),
                    quoted: Vec::new(),
                }]
            }
        };

        // Collect into owned strings inside the borrow scope so we don't return
        // an iterator that borrows from `json` (which would drop at end of fn).
        // `iter_errors` yields every violation (0.49 API; `validate` returns only
        // the first).
        let mut collected = Vec::new();
        for e in self.compiled.iter_errors(&json) {
            let (message, quoted) = describe(&e);
            collected.push(ValidationError {
                instance_path: e.instance_path().to_string(),
                schema_path: e.schema_path().to_string(),
                message,
                quoted,
            });
        }
        collected
    }
}

/// Reconstruct an engine-authored, adopter-value-free message plus the quoted
/// adopter content for a `jsonschema` validation error.
///
/// The `jsonschema` crate's own `Display` interpolates the failing document
/// value straight into the message; that is exactly the inline, unmarked adopter
/// content `DESIGN.md` § Output forbids (and the agent-context injection surface).
/// Here the message is built from the error *kind* using only schema-side data
/// (keyword, allowed options, limits — the adopter's committed config), while the
/// failing document value and any adopter-authored keys are routed to
/// [`QuotedRegion`]s for prefix-marked rendering. Uncommon kinds fall back to a
/// generic engine message that still quotes the value.
fn describe(e: &jsonschema::ValidationError) -> (String, Vec<QuotedRegion>) {
    use jsonschema::error::ValidationErrorKind as K;

    let at = at_pointer(&e.instance_path().to_string());
    // The failing document value, compactly serialized, as a quoted region.
    let found = || {
        vec![QuotedRegion {
            label: "found".into(),
            content: serde_json::to_string(e.instance())
                .unwrap_or_else(|_| "<unserializable value>".into()),
        }]
    };

    match e.kind() {
        K::AdditionalProperties { unexpected } | K::UnevaluatedProperties { unexpected } => {
            let plural = if unexpected.len() == 1 {
                "property"
            } else {
                "properties"
            };
            (
                format!("unexpected {plural} not permitted by the schema{at}"),
                vec![QuotedRegion {
                    label: "unexpected".into(),
                    content: unexpected.join("\n"),
                }],
            )
        }
        K::Enum { options } => (
            format!(
                "value{at} is not one of the schema's allowed options: {}",
                compact(options)
            ),
            found(),
        ),
        K::Constant { expected_value } => (
            format!(
                "value{at} must equal the schema constant {}",
                compact(expected_value)
            ),
            found(),
        ),
        K::Type { .. } => (
            format!("value{at} is not of the type the schema requires"),
            found(),
        ),
        K::Pattern { pattern } => (
            format!("value{at} does not match the schema's required pattern /{pattern}/"),
            found(),
        ),
        K::Required { property } => (
            format!("required property {} is missing{at}", compact(property)),
            Vec::new(),
        ),
        K::MinLength { limit } | K::MaxLength { limit } => (
            format!("value{at} violates the schema length bound ({limit})"),
            found(),
        ),
        K::Minimum { limit }
        | K::Maximum { limit }
        | K::ExclusiveMinimum { limit }
        | K::ExclusiveMaximum { limit } => (
            format!(
                "value{at} violates the schema numeric bound ({})",
                compact(limit)
            ),
            found(),
        ),
        K::MinItems { limit } | K::MaxItems { limit } => (
            format!("array{at} violates the schema item-count bound ({limit})"),
            found(),
        ),
        K::MinProperties { limit } | K::MaxProperties { limit } => (
            format!("object{at} violates the schema property-count bound ({limit})"),
            found(),
        ),
        K::Format { format } => (
            format!("value{at} does not conform to the '{format}' format"),
            found(),
        ),
        // Generic fallback: still engine-authored + value quoted, never inline.
        _ => (
            format!(
                "value{at} does not satisfy the schema ({})",
                at_pointer(&e.schema_path().to_string()).trim_start()
            ),
            found(),
        ),
    }
}

/// Render an instance/schema JSON pointer for inline use with control characters
/// escaped to inert `\xNN` (a pointer can carry adopter-chosen key names). Empty
/// pointer (root) yields an empty string so messages read naturally.
fn at_pointer(pointer: &str) -> String {
    if pointer.is_empty() {
        return String::new();
    }
    let mut out = String::from(" at ");
    for ch in pointer.chars() {
        // Match the ratified rendering partition (roast B1, #140): a JSON pointer
        // carries adopter-chosen KEY names, and this pointer is interpolated inline
        // into an engine-authored message — so it must escape every code point a
        // consumer may treat as a line break, i.e. the Cc controls AND the Zl/Zp
        // separators U+2028/U+2029, exactly as `diagnostic.rs::safe_display` /
        // `escape_label` do (#125 SHO5). Escaping only `is_control()` let a raw
        // U+2028 in a frontmatter key forge an unprefixed line in the E0050 message.
        if ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}') {
            use std::fmt::Write;
            let _ = write!(out, "\\x{:02X}", ch as u32);
        } else {
            out.push(ch);
        }
    }
    out
}

/// Compact one-line JSON rendering of schema-side data (allowed options, limits,
/// expected constants). Schema content is adopter *config*, not document content.
fn compact(v: &JsonValue) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "<?>".into())
}

/// Convert a `serde_yaml_ng::Value` to a `serde_json::Value` via serde's interconversion.
/// Returns `None` only for YAML values that have no JSON representation (rare for
/// frontmatter content).
fn yaml_to_json(yaml: &YamlValue) -> Option<JsonValue> {
    serde_json::to_value(yaml).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn yaml(s: &str) -> YamlValue {
        serde_yaml_ng::from_str(s).unwrap()
    }

    #[test]
    fn compiles_valid_schema() {
        let schema_json = json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": { "type": "string" }
            }
        });
        Schema::compile(&schema_json).expect("simple object schema should compile");
    }

    #[test]
    fn compile_returns_err_on_malformed_schema() {
        // A schema whose "type" value is invalid (must be string or array of strings).
        let schema_json = json!({
            "type": 42
        });
        assert!(
            Schema::compile(&schema_json).is_err(),
            "schema with invalid `type` should fail to compile"
        );
    }

    #[test]
    fn validate_passes_conforming_frontmatter() {
        let schema = Schema::compile(&json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": { "type": "string" }
            }
        }))
        .unwrap();
        let fm = yaml("name: example");
        let errors = schema.validate(&fm);
        assert!(
            errors.is_empty(),
            "valid frontmatter should produce zero errors; got {errors:?}"
        );
    }

    #[test]
    fn validate_reports_missing_required_field() {
        let schema = Schema::compile(&json!({
            "type": "object",
            "required": ["name", "version"],
            "properties": {
                "name": { "type": "string" },
                "version": { "type": "string" }
            }
        }))
        .unwrap();
        let fm = yaml("name: example");
        let errors = schema.validate(&fm);
        assert_eq!(
            errors.len(),
            1,
            "missing one required field should produce one error"
        );
        // The validator's message should name the missing field.
        assert!(
            errors[0].message.contains("version"),
            "error message should name the missing field 'version'; got: {}",
            errors[0].message
        );
    }

    #[test]
    fn validate_reports_wrong_type() {
        let schema = Schema::compile(&json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" }
            }
        }))
        .unwrap();
        let fm = yaml("count: not-an-integer");
        let errors = schema.validate(&fm);
        assert_eq!(errors.len(), 1, "type mismatch should produce one error");
        assert_eq!(
            errors[0].instance_path, "/count",
            "instance_path should pinpoint the failing field"
        );
    }

    // #135 (roast C1): adopter `pattern`s run on the LINEAR-time `regex` engine
    // (PatternOptions::regex), not backtracking fancy-regex. Proof 1: a
    // backreference — a fancy-regex-only feature — is refused at compile, so an
    // adopter pattern can never reach a backtracking matcher.
    #[test]
    fn linear_engine_refuses_backreference_pattern() {
        let backref = json!({
            "type": "object",
            "properties": { "x": { "type": "string", "pattern": "(a)\\1" } }
        });
        assert!(
            Schema::compile(&backref).is_err(),
            "a backreference pattern must be refused by the linear regex engine"
        );
    }

    // A classic catastrophic-backtracking pattern uses no fancy features, so it is
    // accepted and returns promptly rather than hanging the verify gate. Honest
    // scope (roast B2): this is a SMOKE test, not a linear-vs-backtracking
    // discriminator — even the default fancy-regex would terminate here via its
    // built-in ~1M-step backtrack limit. The load-bearing linearity guarantee is
    // the fancy-feature rejection in the test above (a backreference the linear
    // `regex` crate cannot express is refused at compile), plus the engine choice
    // itself (`PatternOptions::regex()`); this test only asserts the adversarial
    // input does not stall and yields the expected non-match.
    #[test]
    fn catastrophic_pattern_matches_in_linear_time() {
        let schema = Schema::compile(&json!({
            "type": "object",
            "properties": { "x": { "type": "string", "pattern": "^(a+)+$" } }
        }))
        .unwrap();
        // 40 'a's then a non-'a': the classic ReDoS trigger for `(a+)+`.
        let evil = format!("x: {}!", "a".repeat(40));
        let errors = schema.validate(&yaml(&evil));
        assert_eq!(
            errors.len(),
            1,
            "the adversarial value does not match -> exactly one pattern error"
        );
    }

    // roast B1 (#140): a frontmatter KEY carrying a Zl/Zp separator (U+2028) must
    // render escaped in the engine-authored message's inline JSON pointer, not as
    // a raw line break — matching the diagnostic.rs partition (#125 SHO5).
    #[test]
    fn instance_pointer_escapes_line_separators() {
        let schema = Schema::compile(&json!({
            "type": "object",
            "additionalProperties": { "type": "string" }
        }))
        .unwrap();
        // A key with an embedded U+2028; its value 123 is not a string, so the Type
        // error's instance_path (the key) is interpolated inline into the message.
        // Built directly (a YAML source scanner would itself treat U+2028 as a line
        // break — the very reason the rendered pointer must escape it).
        let mut m = serde_yaml_ng::Mapping::new();
        m.insert("evil\u{2028}key".into(), 123.into());
        let fm = YamlValue::Mapping(m);
        let errors = schema.validate(&fm);
        assert!(
            !errors.is_empty(),
            "the non-string value violates the schema"
        );
        let msg = &errors[0].message;
        assert!(
            !msg.contains('\u{2028}'),
            "no raw line separator survives inline; got {msg:?}"
        );
        assert!(
            msg.contains("\\x2028"),
            "the separator is escaped to \\x2028; got {msg:?}"
        );
    }

    #[test]
    fn validate_reports_enum_violation() {
        let schema = Schema::compile(&json!({
            "type": "object",
            "properties": {
                "status": { "enum": ["open", "closed"] }
            }
        }))
        .unwrap();
        let fm = yaml("status: pending");
        let errors = schema.validate(&fm);
        assert!(
            !errors.is_empty(),
            "enum violation should produce at least one error"
        );
        assert_eq!(errors[0].instance_path, "/status");
    }

    #[test]
    fn validate_multiple_errors_returns_all() {
        let schema = Schema::compile(&json!({
            "type": "object",
            "required": ["name", "version"],
            "properties": {
                "name": { "type": "string" },
                "version": { "type": "string" },
                "count": { "type": "integer" }
            }
        }))
        .unwrap();
        let fm = yaml("count: not-an-integer");
        let errors = schema.validate(&fm);
        // Missing name, missing version, count type wrong → at least 3 errors
        assert!(
            errors.len() >= 3,
            "multiple violations should produce multiple errors; got {} ({errors:?})",
            errors.len()
        );
    }
}
