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
        assert_eq!(pf.mdatron_dsl_version, Some(1));
        assert_eq!(pf.pattern.id, "my-pattern");
        assert_eq!(pf.pattern.rules.len(), 1);
        let rule = &pf.pattern.rules[0];
        assert_eq!(rule.id, "example");
        assert_eq!(rule.code, "TEST-E0001");
        assert!(matches!(&rule.context, ContextSelector::Bare(s) if s == "phase-primer"));
        assert!(rule.let_bindings.is_empty());
    }

    /// Wrap one `context:` value in a minimal pattern file.
    fn pattern_with_context(context_yaml: &str) -> String {
        format!(
            "mdatron_dsl_version: 1\npattern:\n  id: p\n  rules:\n    - id: r\n      \
             context: {context_yaml}\n      assert: \"true\"\n      code: T-E0001\n      \
             message: m\n"
        )
    }

    // RED GATE (GH #52 lane-B review B1): the untagged Combined arm accepted
    // ANY mapping as `{None, None}` — a typo'd key INSIDE `context:` silently
    // rewrote the rule's scope to match-everything (verified pre-fix:
    // `schema_clas:` fired on every governed file, `paht:` dropped the path
    // confinement, `{}` matched everything). Each construction now refuses
    // loudly, naming the unknown or empty field.
    #[test]
    fn context_mapping_typos_and_empty_refuse_loudly() {
        let cases = [
            (
                "typo-schema_class",
                "{ schema_clas: notblog }",
                "schema_clas",
            ),
            (
                "typo-path",
                "{ schema_class: blog, paht: \"docs/**\" }",
                "paht",
            ),
            ("empty", "{}", "empty context"),
        ];
        for (label, context, expect) in cases {
            let err = parse_pattern_file(&pattern_with_context(context))
                .expect_err("a malformed context mapping must refuse");
            let msg = format!("{err}");
            assert!(
                msg.contains(expect),
                "{label}: the refusal names the defect ({expect}); got {msg}"
            );
        }
    }

    // CONTROL MATRIX (B1): every valid context form still parses to the same
    // selector it always did — the strictness closes the typo hole without
    // narrowing the legal surface.
    #[test]
    fn valid_context_forms_still_parse() {
        let bare = parse_pattern_file(&pattern_with_context("phase-primer")).unwrap();
        assert!(matches!(
            &bare.pattern.rules[0].context,
            ContextSelector::Bare(s) if s == "phase-primer"
        ));
        let glob = parse_pattern_file(&pattern_with_context("\"docs/**/*.md\"")).unwrap();
        assert!(
            matches!(&glob.pattern.rules[0].context, ContextSelector::Bare(s) if s == "docs/**/*.md")
        );
        let class_only =
            parse_pattern_file(&pattern_with_context("{ schema_class: blog }")).unwrap();
        assert!(matches!(
            &class_only.pattern.rules[0].context,
            ContextSelector::Combined { schema_class: Some(c), path: None } if c == "blog"
        ));
        let path_only = parse_pattern_file(&pattern_with_context("{ path: \"docs/**\" }")).unwrap();
        assert!(matches!(
            &path_only.pattern.rules[0].context,
            ContextSelector::Combined { schema_class: None, path: Some(p) } if p == "docs/**"
        ));
        let combined = parse_pattern_file(&pattern_with_context(
            "{ schema_class: blog, path: \"docs/**\" }",
        ))
        .unwrap();
        assert!(matches!(
            &combined.pattern.rules[0].context,
            ContextSelector::Combined { schema_class: Some(c), path: Some(p) }
                if c == "blog" && p == "docs/**"
        ));
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
            rule.let_bindings
                .iter()
                .find(|(k, _)| k == "expected")
                .map(|(_, v)| v.as_str()),
            Some(r#"key("composition-matrix", $self.phase)"#)
        );
        assert!(rule.location.is_some());
        assert_eq!(
            rule.location.as_ref().unwrap().field.as_deref(),
            Some("relevant_domains")
        );
    }

    // RED GATE (#89, #47 cold-run finding): let bindings preserve DECLARATION
    // order — `aa` references the earlier-declared `zz`, which alphabetical
    // (BTreeMap) evaluation would visit in the wrong order.
    #[test]
    fn let_bindings_preserve_declaration_order() {
        let yaml = r#"
mdatron_dsl_version: 1
pattern:
  id: order
  rules:
    - id: r
      context: c
      let:
        zz: 'count($self.items)'
        aa: '$zz == 3'
      assert: $aa
      code: T-E0001
      message: m
"#;
        let pf = parse_pattern_file(yaml).expect("parses");
        let names: Vec<&str> = pf.pattern.rules[0]
            .let_bindings
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        assert_eq!(
            names,
            vec!["zz", "aa"],
            "declaration order, not alphabetical"
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
