//! YAML frontmatter parsing for markdown documents.
//!
//! Frontmatter is the YAML block delimited by `---` markers at the very top of the file.
//! [`parse`] returns the parsed frontmatter value and the body slice; `Ok(None)` if no
//! frontmatter is present; `Err` if the frontmatter is present but malformed YAML.
//!
//! Edge cases the contract covers (asserted by Red Gate tests below):
//! - File with no frontmatter at all → `Ok(None)`
//! - Empty frontmatter (`---\n---\n`) → returns the empty mapping plus the body
//! - Trailing newline immediately after closing `---` → stripped from body's leading position
//! - Malformed YAML inside the markers → `Err`
//! - File where closing `---` is missing → `Ok(None)` (no frontmatter detected)
//! - Dashes inside body without leading newline → not matched as closing
//!
//! Phase 2a Red Gate: body stubbed with `todo!()`. Tests below assert the contracts.

use crate::Error;
use serde_yaml::Value;

/// Parse YAML frontmatter from markdown content.
///
/// Returns:
/// - `Ok(Some((frontmatter, body)))` when frontmatter is present and parses cleanly
/// - `Ok(None)` when no frontmatter is present (or no closing marker found)
/// - `Err` when the frontmatter is present but malformed YAML
pub fn parse(_content: &str) -> Result<Option<(Value, &str)>, Error> {
    todo!(
        "Phase 2b: parse YAML frontmatter between --- markers; \
         return Ok(None) if absent or no closing marker; Err on malformed YAML"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_frontmatter_and_returns_body() {
        let content = "---\nschema_class: design-doc\nversion: 0.1.0\n---\n\n# Title\nbody\n";
        let result = parse(content).expect("typical frontmatter should parse cleanly");
        let (fm, body) = result.expect("typical case returns Some, not None");
        assert_eq!(fm["schema_class"].as_str(), Some("design-doc"));
        assert_eq!(fm["version"].as_str(), Some("0.1.0"));
        assert_eq!(body, "\n# Title\nbody\n");
    }

    #[test]
    fn returns_none_for_no_frontmatter() {
        let content = "# Title\nbody\n";
        let result = parse(content).expect("no-frontmatter must not error");
        assert!(result.is_none(), "no frontmatter present; expected None");
    }

    #[test]
    fn returns_none_when_closing_marker_absent() {
        let content = "---\nschema_class: design-doc\n# Title body without closing\n";
        let result = parse(content).expect("missing-close must not error");
        assert!(
            result.is_none(),
            "no closing --- marker; should treat as no-frontmatter"
        );
    }

    #[test]
    fn handles_empty_frontmatter_as_empty_mapping() {
        let content = "---\n---\n# Title\n";
        let result = parse(content).expect("empty frontmatter is well-formed");
        let (fm, body) = result.expect("empty frontmatter still returns Some");
        assert!(
            fm.is_mapping(),
            "empty frontmatter should deserialize to a mapping value"
        );
        assert!(
            fm.as_mapping().expect("mapping").is_empty(),
            "empty frontmatter mapping should have no entries"
        );
        assert_eq!(body, "# Title\n");
    }

    #[test]
    fn dashes_inside_body_without_leading_newline_not_a_closer() {
        let content =
            "---\nfoo: bar\n---\n\n## Section\n---\nNot a closer because not at line start of body.\n";
        let (_fm, body) = parse(content)
            .expect("well-formed frontmatter parses")
            .expect("frontmatter present");
        // The body should still contain the `---` inside it (not be cut off at it).
        assert!(
            body.contains("---\nNot a closer"),
            "body should include the body-internal --- since it is not a real closing marker; got: {body:?}"
        );
    }

    #[test]
    fn malformed_yaml_returns_err() {
        let content = "---\n: invalid: : :\n---\n";
        let result = parse(content);
        assert!(
            result.is_err(),
            "malformed YAML between markers should return Err"
        );
    }
}
