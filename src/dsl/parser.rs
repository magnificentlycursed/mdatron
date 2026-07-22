//! YAML → AST parsing for pattern files.

use super::types::PatternFile;
use crate::Error;

/// Parse a YAML pattern file into a [`PatternFile`] AST.
///
/// Returns an [`Error::Yaml`] when the YAML is malformed or does not match the
/// expected shape. Expression strings inside rules are NOT validated; that is the
/// evaluator's responsibility.
pub fn parse_pattern_file(yaml: &str) -> Result<PatternFile, Error> {
    let parsed: PatternFile = serde_yaml_ng::from_str(yaml)?;
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::super::types::ContextSelector;
    use super::*;

    #[test]
    fn parses_minimal_pattern() {
        let yaml = r#"
mdatron_dsl_version: 1
pattern:
  id: my-pattern
  rules:
    - id: example
      context: phase-primer
      assert: $self.phase == "phase-1a"
      code: TEST-E0001
      message: "phase must be phase-1a"
"#;
        let pf = parse_pattern_file(yaml).expect("minimal pattern parses");
        assert_eq!(pf.mdatron_dsl_version, 1);
        assert_eq!(pf.pattern.id, "my-pattern");
        assert_eq!(pf.pattern.rules.len(), 1);
        let rule = &pf.pattern.rules[0];
        assert_eq!(rule.id, "example");
        assert_eq!(rule.code, "TEST-E0001");
        assert!(matches!(&rule.context, ContextSelector::Bare(s) if s == "phase-primer"));
        assert!(rule.let_bindings.is_empty());
    }

    #[test]
    fn parses_pattern_with_phases_and_keys() {
        let yaml = r#"
mdatron_dsl_version: 1
pattern:
  id: phase-domain-composition
  description: Phase primers must declare relevant_domains consistent with the matrix.
  phases: [strict, pre-commit, lsp]
  keys:
    - name: composition-matrix
      source: .vsdd/registry/phase-domain-matrix.yaml
      select: $.matrix
      indexed_by: $key
  rules:
    - id: required-domains-present
      context: phase-primer
      let:
        expected: key("composition-matrix", $self.phase)
        missing: difference($expected.required, $self.relevant_domains)
      assert: count($missing) == 0
      code: VSDD-E0200
      message: "missing required domain(s)"
      location:
        field: relevant_domains
"#;
        let pf = parse_pattern_file(yaml).expect("complex pattern parses");
        assert_eq!(pf.pattern.id, "phase-domain-composition");
        assert_eq!(pf.pattern.phases, vec!["strict", "pre-commit", "lsp"]);
        assert_eq!(pf.pattern.keys.len(), 1);
        assert_eq!(pf.pattern.keys[0].name, "composition-matrix");
        assert_eq!(pf.pattern.rules.len(), 1);
        let rule = &pf.pattern.rules[0];
        assert_eq!(rule.let_bindings.len(), 2);
        assert_eq!(
            rule.let_bindings.get("expected"),
            Some(&r#"key("composition-matrix", $self.phase)"#.to_string())
        );
        assert!(rule.location.is_some());
        assert_eq!(
            rule.location.as_ref().unwrap().field.as_deref(),
            Some("relevant_domains")
        );
    }

    #[test]
    fn parses_combined_context_selector() {
        let yaml = r#"
mdatron_dsl_version: 1
pattern:
  id: scoped-rule
  rules:
    - id: example
      context:
        schema_class: review-entry
        path: "review-log/**/*.md"
      assert: "true"
      code: TEST-E0001
      message: "ok"
"#;
        let pf = parse_pattern_file(yaml).expect("combined context parses");
        let rule = &pf.pattern.rules[0];
        match &rule.context {
            ContextSelector::Combined { schema_class, path } => {
                assert_eq!(schema_class.as_deref(), Some("review-entry"));
                assert_eq!(path.as_deref(), Some("review-log/**/*.md"));
            }
            other => panic!("expected combined context, got {other:?}"),
        }
    }

    #[test]
    fn detects_path_glob_in_bare_context() {
        let yaml = r#"
mdatron_dsl_version: 1
pattern:
  id: globby
  rules:
    - id: example
      context: "**/*.md"
      assert: "true"
      code: TEST-E0001
      message: "ok"
"#;
        let pf = parse_pattern_file(yaml).expect("glob context parses");
        assert!(pf.pattern.rules[0].context.is_path_glob());
    }

    #[test]
    fn does_not_detect_glob_in_plain_schema_class() {
        let yaml = r#"
mdatron_dsl_version: 1
pattern:
  id: plain
  rules:
    - id: example
      context: phase-primer
      assert: "true"
      code: TEST-E0001
      message: "ok"
"#;
        let pf = parse_pattern_file(yaml).expect("plain context parses");
        assert!(!pf.pattern.rules[0].context.is_path_glob());
    }

    #[test]
    fn rejects_malformed_yaml() {
        let yaml = "this: is: not: valid: yaml: structure: at: : :";
        let result = parse_pattern_file(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_missing_required_pattern_field() {
        let yaml = r#"
mdatron_dsl_version: 1
pattern:
  id: incomplete
"#;
        let result = parse_pattern_file(yaml);
        assert!(result.is_err(), "missing rules field should fail to parse");
    }

    #[test]
    fn rejects_missing_required_rule_field() {
        let yaml = r#"
mdatron_dsl_version: 1
pattern:
  id: missing-code
  rules:
    - id: example
      context: phase-primer
      assert: "true"
      message: "no code field"
"#;
        let result = parse_pattern_file(yaml);
        assert!(result.is_err(), "rule missing code should fail to parse");
    }
}
