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

use crate::Error;
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

/// A compiled JSON Schema ready for validation against many instances.
///
/// Compile once per schema file; validate many times. Wraps the `jsonschema` crate's
/// `JSONSchema` type so adopters never need to depend on it directly.
pub struct Schema {
    compiled: jsonschema::JSONSchema,
}

/// A single validation failure: the JSONPath where the failure occurred + a human-readable
/// message. Adopters wrap this in a `Finding` with appropriate code and severity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// JSONPath into the instance value (e.g. `/relevant_domains/0` or empty string at root).
    pub instance_path: String,
    /// Schema path that produced the violation (e.g. `/properties/phase/enum`).
    pub schema_path: String,
    /// Human-readable message from the validator.
    pub message: String,
}

impl Schema {
    /// Compile a schema from its parsed JSON representation.
    ///
    /// Returns an error if the schema is not valid JSON Schema draft 2020-12.
    pub fn compile(schema_json: &JsonValue) -> Result<Self, Error> {
        let compiled = jsonschema::JSONSchema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .compile(schema_json)
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
                }]
            }
        };

        // Collect into owned strings inside the borrow scope so we don't return
        // an iterator that borrows from `json` (which would drop at end of fn).
        let mut collected = Vec::new();
        if let Err(errors) = self.compiled.validate(&json) {
            for e in errors {
                collected.push(ValidationError {
                    instance_path: e.instance_path.to_string(),
                    schema_path: e.schema_path.to_string(),
                    message: e.to_string(),
                });
            }
        }
        collected
    }
}

/// Convert a `serde_yaml::Value` to a `serde_json::Value` via serde's interconversion.
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
        serde_yaml::from_str(s).unwrap()
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
        assert!(errors.is_empty(), "valid frontmatter should produce zero errors; got {errors:?}");
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
        assert_eq!(errors.len(), 1, "missing one required field should produce one error");
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
        assert!(!errors.is_empty(), "enum violation should produce at least one error");
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
