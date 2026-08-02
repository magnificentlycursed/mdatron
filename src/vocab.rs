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
    /// Input-format version (DEF5, #131). Optional; absent = v1 (legacy
    /// baseline). Read by the format-version probe; declared here only so
    /// `deny_unknown_fields` accepts the field.
    #[serde(default)]
    #[allow(dead_code)]
    mdatron_format_version: Option<u32>,
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
    /// The active cluster allowlist: `Some` when a non-empty `label_schemes.allow`
    /// opts the scan in — the engine's [`DEFAULT_REF_ID_SCHEMES`] unioned with the
    /// consumer's patterns (#159). `None` (an absent/empty allowlist) leaves the
    /// cluster scan disabled.
    label_allow: Option<Vec<regex_lite::Regex>>,
    anti: Vec<(regex_lite::Regex, String)>,
    numeric: Vec<String>,
    cluster: regex_lite::Regex,
    /// Terms declared both `registered` and `draft` (#95). They resolve to
    /// draft (the permissive status); each names a `MDATRON-W0044` warning.
    draft_conflicts: Vec<String>,
}

/// Structured reference-ID schemes exempt from the `E0091` invented-label-scheme
/// check by default (#159, vsdd GH#28 Gap 1). These are decades-old, widely
/// standardized spec conventions — IEEE/ISO requirement IDs, architecture
/// decision records, RFCs — whose IDs *reference* numbered items rather than coin
/// a label scheme, so flagging them as invented is a false positive (327 of 336
/// E0091 findings on the first consumer's spec corpus were this class). Unioned
/// into every active cluster allowlist (see [`load`]) so spec corpora validate
/// out of the box; a consumer extends the set for local schemes via
/// `label_schemes.allow` (e.g. `^C\d+$`). Deliberately conservative — multi-
/// character, well-known prefixes only; ambiguous single-letter forms (`C8`,
/// `T3`) stay consumer-opt-in to avoid masking a genuine coinage. Each is
/// full-anchored (`^…$`) against the whole extracted cluster token.
const DEFAULT_REF_ID_SCHEMES: &[&str] = &[
    r"^REQ-\d+$", // requirements (IEEE/ISO)
    r"^AC-\d+$",  // acceptance criteria
    r"^ADR-\d+$", // architecture decision records
    r"^RFC-\d+$", // RFCs
    r"^Q-?\d+$",  // design questions (Q1 or Q-1)
];

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
    crate::format_version::check_input_format_version(&content, VOCAB_NAME, false)?;
    let raw: RawVocab = serde_yaml_ng::from_str(&content)
        .map_err(|e| Error::Config(format!("cannot parse '{}': {e}", path.display())))?;

    let compile = |p: &str, what: &str| {
        regex_lite::Regex::new(p)
            .map_err(|e| Error::Config(format!("vocabulary {what} '{p}' does not compile: {e}")))
    };

    // The cluster scan activates only on a non-empty consumer allowlist (an
    // absent/empty one leaves it off — unchanged, so the defaults never
    // newly-activate the scan on anyone). When active, the engine's default
    // reference-ID schemes are UNIONED with the consumer's patterns (#159), so a
    // spec's REQ-N/AC-N/… IDs are exempt out of the box and a consumer supplies
    // only its local schemes.
    let label_allow = if raw.label_schemes.allow.is_empty() {
        None
    } else {
        let mut compiled: Vec<regex_lite::Regex> = DEFAULT_REF_ID_SCHEMES
            .iter()
            .map(|p| regex_lite::Regex::new(p).expect("engine default ref-id scheme compiles"))
            .collect();
        for p in &raw.label_schemes.allow {
            compiled.push(compile(p, "label_schemes allow")?);
        }
        Some(compiled)
    };
    let anti = raw
        .anti_patterns
        .iter()
        .map(|a| Ok((compile(&a.pattern, "anti_pattern")?, a.register.clone())))
        .collect::<Result<Vec<_>, Error>>()?;

    // #95 (DESIGN § agnosticism conflict outcome): group terms by name; a term
    // declared both `registered` and `draft` resolves to draft (the permissive
    // status) and names a W0044 warning. Non-draft duplicates keep first-wins.
    let mut order: Vec<String> = Vec::new();
    let mut statuses: std::collections::BTreeMap<String, Vec<TermStatus>> =
        std::collections::BTreeMap::new();
    for t in &raw.terms {
        if !statuses.contains_key(&t.term) {
            order.push(t.term.clone());
        }
        statuses.entry(t.term.clone()).or_default().push(t.status);
    }
    let mut terms: Vec<(String, TermStatus)> = Vec::new();
    let mut draft_conflicts: Vec<String> = Vec::new();
    for term in order {
        let sts = &statuses[&term];
        let resolved = if sts.contains(&TermStatus::Registered) && sts.contains(&TermStatus::Draft)
        {
            draft_conflicts.push(term.clone());
            TermStatus::Draft
        } else {
            sts[0]
        };
        terms.push((term, resolved));
    }

    Ok(Some(LoadedVocab {
        terms,
        label_allow,
        anti,
        numeric: raw.numeric_claims.into_iter().map(|c| c.field).collect(),
        // The engine's cluster detector: uppercase letters, optional dashed
        // second segment, trailing digits — the incident shapes (M2, L2,
        // SEC-F3, AIE-F2, MDATRON-E0050).
        cluster: regex_lite::Regex::new(r"\b[A-Z]{1,7}(?:-[A-Z]{0,7})?[0-9]{1,4}\b")
            .expect("engine cluster detector compiles"),
        draft_conflicts,
    }))
}

/// Registry-level findings independent of any one file (#95): a term declared
/// both `registered` and `draft` resolves to draft and names a `MDATRON-W0044`
/// warning. The term is adopter content, so it rides a quoted region.
pub fn registry_findings(vocab: &LoadedVocab, vocab_path: &Path, findings: &mut Vec<Finding>) {
    for term in &vocab.draft_conflicts {
        findings.push(Finding {
            code: "MDATRON-W0044".into(),
            severity: Severity::Warning,
            summary: "vocabulary-term-status-conflict".into(),
            message: "a term is declared both registered and draft in \
                      .mdatron/vocabulary.yaml; it resolves to draft, the \
                      permissive status — declare it with a single status"
                .into(),
            help: Some("keep one status entry for the term".into()),
            location: Location {
                file: vocab_path.to_path_buf(),
                line: 1,
                column: 0,
            },
            explain_ref: Some("MDATRON-W0044".into()),
            quoted: vec![QuotedRegion {
                label: "term".into(),
                content: term.clone(),
            }],
        });
    }
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

    // A term backticked in prose is a CITATION, not a use (a historical commit
    // subject, or naming a deprecated term as `chassis` to reference it) — the
    // register/coinage prose checks skip inline code spans, reusing the same
    // masking the link check applies (#158, vsdd GH#28 Gap 2). Computed once for
    // both E0091 and E0093. (A code token in a code-catalog citation stays a real
    // reference — see codecat's deliberate non-masking — but a vocabulary term
    // in code is a reference, not a use, exactly like a link.)
    let code_ranges = crate::markup::body_inline_code_ranges(body);

    // ── invented label schemes (label_schemes section supplied) ───────────
    if let Some(allow) = &vocab.label_allow {
        for m in vocab.cluster.find_iter(body) {
            if crate::markup::in_code_span(&code_ranges, m.start()) {
                continue;
            }
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
            if crate::markup::in_code_span(&code_ranges, m.start()) {
                continue;
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A temp project holding one `.mdatron/vocabulary.yaml`, removed on drop.
    struct TempProj(std::path::PathBuf);

    impl TempProj {
        fn new(label: &str, vocab_yaml: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("mdatron-vocab-{label}-{nanos}"));
            std::fs::create_dir_all(root.join(".mdatron")).unwrap();
            std::fs::write(root.join(".mdatron").join(VOCAB_NAME), vocab_yaml).unwrap();
            Self(root)
        }
    }

    impl Drop for TempProj {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The `E0091` clusters flagged in `body` under `vocab_yaml`, sorted.
    fn e0091_clusters(vocab_yaml: &str, body: &str, label: &str) -> Vec<String> {
        let proj = TempProj::new(label, vocab_yaml);
        let vocab = load(&proj.0).expect("vocab loads").expect("vocab present");
        let mut findings = Vec::new();
        check_file(&vocab, Path::new("doc.md"), body, 0, None, &mut findings);
        let mut out: Vec<String> = findings
            .into_iter()
            .filter(|f| f.code == "MDATRON-E0091")
            .flat_map(|f| {
                f.quoted
                    .into_iter()
                    .filter(|q| q.label == "cluster")
                    .map(|q| q.content)
            })
            .collect();
        out.sort();
        out
    }

    // ── #159: structured reference-ID default exemption (vsdd GH#28 Gap 1) ──────

    #[test]
    fn default_ref_id_schemes_all_compile() {
        // Guards the `.expect()` in `load`: the engine defaults are static and
        // must always compile.
        for p in DEFAULT_REF_ID_SCHEMES {
            regex_lite::Regex::new(p)
                .unwrap_or_else(|e| panic!("default scheme {p} must compile: {e}"));
        }
    }

    #[test]
    fn structured_ref_ids_exempt_by_default() {
        // A non-empty consumer allowlist activates the scan; the engine defaults
        // union in. Standard ref-ids (REQ/AC/ADR/RFC/Q) + the consumer's own FOO
        // are exempt out of the box; a coinage (W0070) and an out-of-default
        // single-letter id (C8) still flag.
        let vocab = "label_schemes:\n  allow:\n    - '^FOO-\\d+$'\n";
        let body = "See REQ-10, AC-4, ADR-7, RFC-2119, Q1, Q-1, and FOO-99. \
                    But W0070 and C8 are unknown schemes.";
        assert_eq!(
            e0091_clusters(vocab, body, "default-exempt"),
            vec!["C8".to_string(), "W0070".to_string()],
            "only the coinage + out-of-default id flag; standard ref-ids + FOO exempt"
        );
    }

    #[test]
    fn consumer_pattern_extends_the_default_set() {
        // Adding `^C\d+$` exempts the local C-id scheme; a coinage still flags —
        // the documented extension path for schemes outside the conservative
        // default set.
        let vocab = "label_schemes:\n  allow:\n    - '^C\\d+$'\n";
        let body = "C8 and C12 are constraints; W0070 is a coinage.";
        assert_eq!(
            e0091_clusters(vocab, body, "extend"),
            vec!["W0070".to_string()],
            "consumer pattern exempts C-ids; coinage still flagged"
        );
    }

    #[test]
    fn defaults_do_not_activate_scan_without_a_consumer_allowlist() {
        // No `label_schemes` → scan OFF, unchanged. REQ-10 AND W0070 both pass:
        // the engine defaults must never newly-activate the scan on a consumer
        // who did not opt in (that would surface brand-new E0091 findings).
        let vocab = "terms:\n  - term: Widget\n    status: registered\n    sense: a thing\n";
        let proj = TempProj::new("scan-off", vocab);
        let vocab = load(&proj.0).expect("loads").expect("present");
        let mut findings = Vec::new();
        check_file(
            &vocab,
            Path::new("doc.md"),
            "REQ-10 and W0070 appear here.",
            0,
            None,
            &mut findings,
        );
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.code == "MDATRON-E0091")
                .count(),
            0,
            "cluster scan stays off without a consumer allowlist"
        );
    }
}
