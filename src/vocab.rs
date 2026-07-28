//! Vocabulary family (#85; `DESIGN.md` § Five check families): prose in
//! governed artifacts is scanned against a supplied registry.
//!
//! `.mdatron/vocabulary.yaml` is an engine-defined interface parsed strictly.
//! Each section activates its own scan when present: `terms` drives the
//! coinage and reserved-word checks (`MDATRON-E0090` unregistered coinage —
//! bold-introduced terms absent from the registry, the bootstrap validator's
//! proven heuristic; `MDATRON-E0092` reserved-word use — reserved means held,
//! not usable; draft-status terms are exempt from strict findings per
//! contract). `label_schemes.allow` drives the invented-label-scheme check
//! (`MDATRON-E0091`: letter-plus-number clusters outside the allowlist — the
//! letter-cluster incident's mechanization). `anti_patterns` drives
//! `MDATRON-E0093` (listed register anti-patterns). `numeric_claims` drives
//! `MDATRON-E0094` (#80 D3, the vsdd drift class): a prose numeral restating a
//! configured frontmatter field's count that disagrees with it — configured
//! field references only, no free inference.
//!
//! All adopter patterns compile on the linear-time engine (`regex_lite`,
//! `DESIGN.md` L17); matched prose rides quoted regions, never inline; finding
//! locations carry the precise source line.

use std::path::Path;

use serde::Deserialize;

use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};
use crate::Error;

/// File name of the vocabulary registry under `.mdatron/`.
pub const VOCAB_NAME: &str = "vocabulary.yaml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVocab {
    #[serde(default)]
    terms: Vec<RawTerm>,
    #[serde(default)]
    label_schemes: RawLabelSchemes,
    #[serde(default)]
    anti_patterns: Vec<RawAntiPattern>,
    #[serde(default)]
    numeric_claims: Vec<RawNumericClaim>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTerm {
    term: String,
    status: TermStatus,
    #[allow(dead_code)] // the sense is registry documentation, read by humans
    sense: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TermStatus {
    Registered,
    Draft,
    Reserved,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLabelSchemes {
    #[serde(default)]
    allow: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAntiPattern {
    pattern: String,
    register: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNumericClaim {
    field: String,
}

/// The compiled registry.
pub struct LoadedVocab {
    terms: Vec<(String, TermStatus)>,
    /// `Some` when the `label_schemes` section was supplied (empty allowlist
    /// means every cluster is invented); `None` disables the cluster scan.
    label_allow: Option<Vec<regex_lite::Regex>>,
    anti: Vec<(regex_lite::Regex, String)>,
    numeric: Vec<String>,
    cluster: regex_lite::Regex,
}

/// Load `.mdatron/vocabulary.yaml`. `Ok(None)` when absent (family inactive);
/// `Err` when unreadable, structurally malformed, or carrying a pattern that
/// does not compile (loud, never a silent no-op).
pub fn load(project_root: &Path) -> Result<Option<LoadedVocab>, Error> {
    let path = project_root.join(".mdatron").join(VOCAB_NAME);
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
    let raw: RawVocab = serde_yaml_ng::from_str(&content)
        .map_err(|e| Error::Config(format!("cannot parse '{}': {e}", path.display())))?;

    let compile = |p: &str, what: &str| {
        regex_lite::Regex::new(p)
            .map_err(|e| Error::Config(format!("vocabulary {what} '{p}' does not compile: {e}")))
    };

    let label_allow = if raw.label_schemes.allow.is_empty() {
        None
    } else {
        Some(
            raw.label_schemes
                .allow
                .iter()
                .map(|p| compile(p, "label_schemes allow"))
                .collect::<Result<Vec<_>, _>>()?,
        )
    };
    let anti = raw
        .anti_patterns
        .iter()
        .map(|a| Ok((compile(&a.pattern, "anti_pattern")?, a.register.clone())))
        .collect::<Result<Vec<_>, Error>>()?;

    Ok(Some(LoadedVocab {
        terms: raw.terms.into_iter().map(|t| (t.term, t.status)).collect(),
        label_allow,
        anti,
        numeric: raw.numeric_claims.into_iter().map(|c| c.field).collect(),
        // The engine's cluster detector: uppercase letters, optional dashed
        // second segment, trailing digits — the incident shapes (M2, L2,
        // SEC-F3, AIE-F2, MDATRON-E0050).
        cluster: regex_lite::Regex::new(r"\b[A-Z]{1,7}(?:-[A-Z]{0,7})?[0-9]{1,4}\b")
            .expect("engine cluster detector compiles"),
    }))
}

/// Scan one file's prose. `content` is the whole file; `body_offset` is where
/// prose begins (after the frontmatter block; 0 when there is none);
/// `frontmatter` feeds the numeric-claims comparison (claims are skipped
/// without it).
pub fn check_file(
    vocab: &LoadedVocab,
    path: &Path,
    content: &str,
    body_offset: usize,
    frontmatter: Option<&serde_yaml_ng::Value>,
    findings: &mut Vec<Finding>,
) {
    let body = &content[body_offset..];

    // ── coinage + reserved words (terms section supplied) ─────────────────
    if !vocab.terms.is_empty() {
        for (start, term) in bold_spans(body) {
            let status = vocab
                .terms
                .iter()
                .find(|(t, _)| t == &term)
                .map(|(_, s)| *s);
            if status.is_none() {
                findings.push(prose_finding(
                    path,
                    content,
                    body_offset + start,
                    "MDATRON-E0090",
                    "unregistered-coinage",
                    "a bold-introduced term is not in the vocabulary registry \
                     (draft-status terms are exempt; register the coinage or \
                     unbold the emphasis)",
                    "term",
                    &term,
                ));
            }
        }
        for (term, status) in &vocab.terms {
            if *status != TermStatus::Reserved {
                continue;
            }
            if let Some(pos) = find_word(body, term) {
                findings.push(prose_finding(
                    path,
                    content,
                    body_offset + pos,
                    "MDATRON-E0092",
                    "reserved-word-misuse",
                    "a reserved-status term appears in prose; reserved means \
                     held for a registered future sense, not usable — the use \
                     is surfaced for review",
                    "term",
                    term,
                ));
            }
        }
    }

    // ── invented label schemes (label_schemes section supplied) ───────────
    if let Some(allow) = &vocab.label_allow {
        for m in vocab.cluster.find_iter(body) {
            let cluster = m.as_str();
            if !allow.iter().any(|a| a.is_match(cluster)) {
                findings.push(prose_finding(
                    path,
                    content,
                    body_offset + m.start(),
                    "MDATRON-E0091",
                    "invented-label-scheme",
                    "a letter-plus-number cluster matches no allowed label \
                     scheme; invented schemes proliferate faster than review \
                     can police them",
                    "cluster",
                    cluster,
                ));
            }
        }
    }

    // ── register anti-patterns ─────────────────────────────────────────────
    for (pattern, register) in &vocab.anti {
        for m in pattern.find_iter(body) {
            findings.push(prose_finding(
                path,
                content,
                body_offset + m.start(),
                "MDATRON-E0093",
                "register-anti-pattern",
                &format!("prose matches the listed '{register}' register anti-pattern"),
                "matched",
                m.as_str(),
            ));
        }
    }

    // ── numeric claims (#80 D3: configured field references only) ─────────
    let Some(fm) = frontmatter else { return };
    for field in &vocab.numeric {
        let Some(actual) = field_count(fm, field) else {
            continue;
        };
        let variant = field.replace('_', " ");
        for (line_start, line) in line_spans(body) {
            for needle in [field.as_str(), variant.as_str()] {
                let Some(fpos) = line.find(needle) else {
                    continue;
                };
                // A number within the four words preceding the reference.
                let before = &line[..fpos];
                let claim = before
                    .split_whitespace()
                    .rev()
                    .take(4)
                    .find_map(parse_number);
                if let Some(n) = claim {
                    if n != actual {
                        findings.push(prose_finding(
                            path,
                            content,
                            body_offset + line_start,
                            "MDATRON-E0094",
                            "numeric-claim-drift",
                            &format!(
                                "prose claims {n} where the frontmatter field \
                                 '{field}' holds {actual}; cite the field, \
                                 never copy the value"
                            ),
                            "claim",
                            line.trim(),
                        ));
                    }
                }
                break; // first reference per line per field
            }
        }
    }
}

/// Bold spans `**term**` within one line each; returns (byte offset of the
/// term, term text).
fn bold_spans(body: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if &bytes[i..i + 2] == b"**" {
            if let Some(rel) = body[i + 2..].find("**") {
                let inner = &body[i + 2..i + 2 + rel];
                if !inner.is_empty() && !inner.contains('\n') && inner.len() <= 60 {
                    out.push((i + 2, inner.trim().to_string()));
                }
                i += 2 + rel + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// First whole-word occurrence of `term` (neighbors must not be word chars).
fn find_word(body: &str, term: &str) -> Option<usize> {
    let is_word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let bytes = body.as_bytes();
    let mut from = 0;
    while let Some(rel) = body[from..].find(term) {
        let start = from + rel;
        let end = start + term.len();
        let left_ok = start == 0 || !is_word(bytes[start - 1]);
        let right_ok = end >= bytes.len() || !is_word(bytes[end]);
        if left_ok && right_ok {
            return Some(start);
        }
        from = end;
    }
    None
}

/// The field's countable value: an array's length or an integer's value.
fn field_count(fm: &serde_yaml_ng::Value, field: &str) -> Option<u64> {
    match fm.get(field)? {
        serde_yaml_ng::Value::Sequence(s) => Some(s.len() as u64),
        serde_yaml_ng::Value::Number(n) => n.as_u64(),
        _ => None,
    }
}

/// Digits or the small-number words (the vsdd 'seven core domains' shape).
fn parse_number(token: &str) -> Option<u64> {
    let t = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    if let Ok(n) = t.parse::<u64>() {
        return Some(n);
    }
    const WORDS: &[(&str, u64)] = &[
        ("zero", 0),
        ("one", 1),
        ("two", 2),
        ("three", 3),
        ("four", 4),
        ("five", 5),
        ("six", 6),
        ("seven", 7),
        ("eight", 8),
        ("nine", 9),
        ("ten", 10),
        ("eleven", 11),
        ("twelve", 12),
        ("thirteen", 13),
        ("fourteen", 14),
        ("fifteen", 15),
        ("sixteen", 16),
        ("seventeen", 17),
        ("eighteen", 18),
        ("nineteen", 19),
        ("twenty", 20),
    ];
    let lower = t.to_ascii_lowercase();
    WORDS.iter().find(|(w, _)| *w == lower).map(|(_, n)| *n)
}

/// (byte offset, line) pairs over the body.
fn line_spans(body: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut offset = 0;
    for line in body.split('\n') {
        out.push((offset, line));
        offset += line.len() + 1;
    }
    out
}

/// Build a prose finding at the precise source line of `offset` (into the
/// whole file content). Adopter prose rides the quoted region.
#[allow(clippy::too_many_arguments)]
fn prose_finding(
    path: &Path,
    content: &str,
    offset: usize,
    code: &str,
    summary: &str,
    message: &str,
    label: &str,
    quoted: &str,
) -> Finding {
    let line = 1 + content[..offset.min(content.len())].matches('\n').count() as u32;
    Finding {
        code: code.into(),
        severity: Severity::Error,
        summary: summary.into(),
        message: message.into(),
        help: None,
        location: Location {
            file: path.to_path_buf(),
            line,
            column: 0,
        },
        explain_ref: Some(code.to_string()),
        quoted: vec![QuotedRegion {
            label: label.into(),
            content: quoted.into(),
        }],
    }
}
