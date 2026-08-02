//! Adopter-namespace code-catalog integrity (#148, vsdd GH#20 P4): every
//! adopter code token cited in the governed corpus (e.g. `VSDD-X0001`) must
//! resolve to an entry the adopter declares in `.mdatron/code-catalogs.yaml` —
//! the adopter-side twin of mdatron's own every-code-resolves-in-explain
//! tripwire. The fourth reference-resolution check (citation/link/marker/code),
//! on the shared `markup` scan spine.
//!
//! Design shaped by the SARIF + Ruff reference-arch review (knowledge pages
//! `sarif-envelope-audit` / `ruff-registry-audit`): a catalog is a NAMED set
//! keyed on the ownership **prefix** (`namespace`, Ruff's `Linter` model — the
//! prefix owns the codes, not a numeric range); `comprehensive` asserts the
//! catalog is the sole authority for that prefix (SARIF's `isComprehensive`),
//! which is what licenses the hard gate — a cited token under the prefix that is
//! not declared is an orphan (`MDATRON-E0113`). One namespace-parameterized
//! resolver answers "does this token resolve?" the same way for every namespace.
//!
//! The legal token grammar is declared by the CATALOG, not the detector:
//! [`candidate_tokens`] is deliberately BROADER than the grammar — the namespace
//! prefix on a word boundary followed by an alphanumeric run that must contain a
//! digit — so a mistyped or unknown class is still detected and caught as an
//! orphan by the resolver rather than silently skipped (vsdd-cli#27's explicit
//! request: the detector must not narrow to the legal classes, or a typo escapes
//! the gate). Ordinary prose after the prefix (no digit) is not a token.

use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;

use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};
use crate::markup::non_fenced_lines;
use crate::Error;

/// File name of the adopter code-catalog registry under `.mdatron/`.
pub const CATALOGS_NAME: &str = "code-catalogs.yaml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCatalogs {
    /// Input-format version (DEF5, #131). Required — code-catalogs.yaml is new in
    /// 0.6.0, so it is born versioned. Read by the format-version probe; declared
    /// here only so `deny_unknown_fields` accepts the field.
    #[serde(default)]
    #[allow(dead_code)]
    mdatron_format_version: Option<u32>,
    catalogs: Vec<RawCatalog>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCatalog {
    /// The ownership prefix every code in this catalog shares, e.g. `"VSDD-"`.
    namespace: String,
    /// Whether this catalog is the sole authority for its prefix. When true, a
    /// cited token under the prefix that is not declared here is an orphan; when
    /// false, unlisted tokens are left alone (the prefix's codes may live
    /// elsewhere). SARIF's `isComprehensive`.
    #[serde(default)]
    comprehensive: bool,
    /// The declared code bodies (the part after the namespace, e.g. `"X0001"`).
    #[serde(default)]
    codes: Vec<String>,
}

/// A compiled adopter code catalog.
pub struct CodeCatalog {
    pub namespace: String,
    pub comprehensive: bool,
    /// The full declared tokens (namespace + each body), for O(1) resolution.
    pub tokens: HashSet<String>,
}

/// The loaded catalog registry plus any load-time findings.
pub struct LoadedCatalogs {
    pub catalogs: Vec<CodeCatalog>,
    pub findings: Vec<Finding>,
}

/// Load `.mdatron/code-catalogs.yaml`. `Ok(None)` when absent (family inactive);
/// `Err` when unreadable or structurally malformed (loud, strict parse).
pub fn load(project_root: &Path) -> Result<Option<LoadedCatalogs>, Error> {
    let path = project_root.join(".mdatron").join(CATALOGS_NAME);
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
    // DEF5 (#131): code-catalogs.yaml is new in 0.6.0 → born versioned (required).
    // Probe the version leniently before the strict parse (a legible break).
    crate::format_version::check_input_format_version(&content, CATALOGS_NAME, true)?;
    let raw: RawCatalogs = serde_yaml_ng::from_str(&content)
        .map_err(|e| Error::Config(format!("cannot parse '{}': {e}", path.display())))?;

    let catalogs = raw
        .catalogs
        .into_iter()
        .map(|c| {
            let tokens = c
                .codes
                .iter()
                .map(|body| format!("{}{}", c.namespace, body))
                .collect();
            CodeCatalog {
                namespace: c.namespace,
                comprehensive: c.comprehensive,
                tokens,
            }
        })
        .collect();

    Ok(Some(LoadedCatalogs {
        catalogs,
        findings: Vec::new(),
    }))
}

/// Scan one governed file's body for adopter code tokens and flag any that do
/// not resolve to a declared entry in a `comprehensive` catalog. `content` is
/// the whole file; `body_offset` is where the prose body begins.
pub fn check_file(
    catalogs: &[CodeCatalog],
    path: &Path,
    content: &str,
    body_offset: usize,
    findings: &mut Vec<Finding>,
) {
    if catalogs.is_empty() {
        return;
    }
    let body = &content[body_offset..];
    for (line_start, line) in non_fenced_lines(body) {
        for cat in catalogs {
            // Only a catalog that claims sole authority for its prefix can call
            // an unlisted token an orphan (SARIF isComprehensive).
            if !cat.comprehensive {
                continue;
            }
            // NOTE (#154): inline code spans are NOT masked here — unlike links,
            // an adopter code token is routinely FORMATTED as `code` and is a
            // real citation (vsdd's live `VSDD-W0070` orphan is backticked). So a
            // backticked code still resolves-or-orphans.
            for (at_in_line, token) in candidate_tokens(line, &cat.namespace) {
                if !cat.tokens.contains(token) {
                    findings.push(orphan_finding(
                        path,
                        content,
                        body_offset + line_start + at_in_line,
                        token,
                    ));
                }
            }
        }
    }
}

/// Find candidate code tokens for `namespace` on `line`: each occurrence of the
/// prefix on a word boundary, followed by an alphanumeric run that contains at
/// least one digit (so ordinary prose after the prefix is not read as a code).
/// Returns `(byte offset in line, full token)`. Conservative pending the exact
/// grammar (vsdd-cli#27).
fn candidate_tokens<'a>(line: &'a str, namespace: &str) -> Vec<(usize, &'a str)> {
    let mut out = Vec::new();
    if namespace.is_empty() {
        return out;
    }
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(rel) = line[from..].find(namespace) {
        let start = from + rel;
        // Require a word boundary before the prefix so `xVSDD-1` is not a hit.
        let boundary = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let body_start = start + namespace.len();
        let mut end = body_start;
        while end < line.len() && bytes[end].is_ascii_alphanumeric() {
            end += 1;
        }
        let body = &line[body_start..end];
        if boundary && !body.is_empty() && body.bytes().any(|b| b.is_ascii_digit()) {
            out.push((start, &line[start..end]));
        }
        // Advance past this match (never stall).
        from = end.max(start + namespace.len());
    }
    out
}

fn orphan_finding(path: &Path, content: &str, offset: usize, token: &str) -> Finding {
    let line = 1 + content[..offset.min(content.len())].matches('\n').count() as u32;
    Finding {
        code: "MDATRON-E0113".into(),
        severity: Severity::Error,
        summary: "orphaned-adopter-code".into(),
        message: "this adopter code token resolves to no entry in the comprehensive \
                  catalog declared for its namespace; a cited code that names nothing \
                  is a dangling reference (the adopter-side twin of every mdatron code \
                  resolving in explain)"
            .into(),
        help: None,
        location: Location {
            file: path.to_path_buf(),
            line,
            column: 0,
        },
        explain_ref: Some("MDATRON-E0113".into()),
        quoted: vec![QuotedRegion {
            label: "code".into(),
            content: token.into(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(namespace: &str, comprehensive: bool, codes: &[&str]) -> CodeCatalog {
        CodeCatalog {
            namespace: namespace.into(),
            comprehensive,
            tokens: codes.iter().map(|b| format!("{namespace}{b}")).collect(),
        }
    }

    #[test]
    fn candidate_tokens_require_a_digit_and_word_boundary() {
        let toks: Vec<_> = candidate_tokens("see VSDD-X0001 and VSDD-related here", "VSDD-")
            .into_iter()
            .map(|(_, t)| t)
            .collect();
        // The digit-bearing token is a candidate; the prose word `VSDD-related` is not.
        assert_eq!(toks, vec!["VSDD-X0001"]);
        // No false hit when the prefix is mid-word.
        assert!(candidate_tokens("xVSDD-1", "VSDD-").is_empty());
    }

    #[test]
    fn declared_token_resolves_orphan_flagged() {
        let catalogs = [cat("VSDD-", true, &["E0010"])];
        let mut f = Vec::new();
        check_file(
            &catalogs,
            Path::new("d.md"),
            "cites VSDD-E0010 ok\n",
            0,
            &mut f,
        );
        assert!(f.is_empty(), "a declared code resolves clean");

        let mut f2 = Vec::new();
        check_file(
            &catalogs,
            Path::new("d.md"),
            "cites VSDD-E9999 gone\n",
            0,
            &mut f2,
        );
        assert_eq!(f2.len(), 1);
        assert_eq!(f2[0].code, "MDATRON-E0113");
        // Built at runtime so the source carries no literal VSDD-E code (the
        // cross-repo namespace-separation lint, tests/output_format.rs).
        assert!(f2[0]
            .quoted
            .iter()
            .any(|q| q.content == format!("VSDD-{}", "E9999")));
    }

    // vsdd-cli#27: the detector is deliberately BROADER than the legal grammar
    // (classes E/W today), so a mistyped/unknown class (`X`) is still detected
    // and caught as an orphan by the resolver — never silently skipped.
    #[test]
    fn unknown_class_token_is_caught_as_orphan() {
        let catalogs = [cat("VSDD-", true, &["E0010", "W0180"])];
        let mut f = Vec::new();
        check_file(
            &catalogs,
            Path::new("d.md"),
            "typo VSDD-X0016 here\n",
            0,
            &mut f,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].code, "MDATRON-E0113");
        assert!(f[0].quoted.iter().any(|q| q.content == "VSDD-X0016"));
    }

    #[test]
    fn non_comprehensive_catalog_does_not_flag() {
        let catalogs = [cat("VSDD-", false, &["X0001"])];
        let mut f = Vec::new();
        check_file(
            &catalogs,
            Path::new("d.md"),
            "cites VSDD-X9999\n",
            0,
            &mut f,
        );
        assert!(
            f.is_empty(),
            "a non-comprehensive catalog cannot call an unlisted token an orphan"
        );
    }
}
