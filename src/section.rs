//! Section-structural check family (#157; vsdd GH#20 P5, the body-content leg of
//! #149): declarative **count** and **disjointness** assertions over markdown
//! body sections. This is the body-content realization of the DSL's
//! frontmatter count-with-predicate — the DSL half ships over frontmatter (#149,
//! `count(filter(...))` / `count(intersect(...))`), and this is the gated body
//! half, delivered as a dedicated *fixed-semantics* check family on the shared
//! `markup`/section spine rather than as an extension of the (body-content-gated)
//! rule DSL.
//!
//! `.mdatron/section-rules.yaml` declares rules of two shapes (semantics pinned
//! on vsdd-cli#29 against the live `.design/build-plan.md`):
//!
//! - **Count** `{ section, element, match, count }` — count the headings of
//!   `element` level inside `section` (its span until the next heading of the
//!   same or higher level) whose line matches `match`, and assert the `count`
//!   predicate (`>= 1`, `== 1`, `< 3`, …). A failure is `MDATRON-E0120`. Live
//!   case: "at least one open-phase H3 in `## Requirements`" — an empty section
//!   is the retire trigger, so the invariant is `>= 1`, not `== 1`.
//! - **Disjoint** `{ disjoint: [op, op] }`, `op = { section, id_from, id_pattern }`
//!   — extract an id set from each section (`id_from: h3-heading` from the H3
//!   heading text, `bullet-lead` from `- **bold**` bullet leads; `id_pattern`
//!   captures the id in group 1), and assert the two sets share no element. An
//!   overlap is `MDATRON-E0121`.
//!
//! The **id extraction is deliberately heading-scoped / bullet-lead-scoped, not a
//! full-span text scan** (vsdd-cli#29's load-bearing trap): a body line under an
//! open phase (`Provenance: Slice 3 …`) would else collide with a completed
//! `Slice 3` bullet and report a false overlap. Ids come only from the declared
//! element, never the surrounding prose.
//!
//! Adopter patterns compile on the linear-time engine (`regex_lite`, DESIGN L17).

use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;

use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};
use crate::markup::{atx_heading, list_item_bold_name, non_fenced_lines, section_span};
use crate::Error;

/// File name of the section-rules registry under `.mdatron/`.
pub const SECTION_RULES_NAME: &str = "section-rules.yaml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRules {
    rules: Vec<RawRule>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    #[serde(default)]
    section: Option<String>,
    #[serde(default)]
    element: Option<HeadingLevel>,
    #[serde(default, rename = "match")]
    match_pattern: Option<String>,
    #[serde(default)]
    count: Option<String>,
    #[serde(default)]
    disjoint: Option<Vec<RawOperand>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOperand {
    section: String,
    id_from: IdSource,
    id_pattern: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum HeadingLevel {
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
}

impl HeadingLevel {
    fn level(self) -> usize {
        self as usize + 1
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum IdSource {
    /// Ids come from the text of H3 headings in the section (vsdd's open phases).
    H3Heading,
    /// Ids come from the `**bold**` lead of `- ` list items (completed items).
    BulletLead,
}

/// A compiled section-structural rule.
pub enum Rule {
    Count {
        section: String,
        level: usize,
        matcher: regex_lite::Regex,
        pred: CountPred,
    },
    Disjoint {
        a: Operand,
        b: Operand,
    },
}

pub struct Operand {
    section: String,
    id_source: IdSource,
    id_pattern: regex_lite::Regex,
}

#[derive(Clone, Copy)]
enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// A count predicate — a comparison operator and a bound (`>= 1`).
pub struct CountPred {
    op: CmpOp,
    n: usize,
}

impl CountPred {
    fn holds(&self, count: usize) -> bool {
        match self.op {
            CmpOp::Eq => count == self.n,
            CmpOp::Ne => count != self.n,
            CmpOp::Lt => count < self.n,
            CmpOp::Le => count <= self.n,
            CmpOp::Gt => count > self.n,
            CmpOp::Ge => count >= self.n,
        }
    }
    fn describe(&self) -> String {
        let op = match self.op {
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
        };
        format!("{op} {}", self.n)
    }
}

/// Parse a count predicate string (`">= 1"`, `"== 1"`, …). Longest operators
/// first so `>=` is not read as `>`.
fn parse_count_pred(s: &str) -> Option<CountPred> {
    let s = s.trim();
    let (op, rest) = if let Some(r) = s.strip_prefix(">=") {
        (CmpOp::Ge, r)
    } else if let Some(r) = s.strip_prefix("<=") {
        (CmpOp::Le, r)
    } else if let Some(r) = s.strip_prefix("==") {
        (CmpOp::Eq, r)
    } else if let Some(r) = s.strip_prefix("!=") {
        (CmpOp::Ne, r)
    } else if let Some(r) = s.strip_prefix('>') {
        (CmpOp::Gt, r)
    } else if let Some(r) = s.strip_prefix('<') {
        (CmpOp::Lt, r)
    } else {
        return None;
    };
    let n: usize = rest.trim().parse().ok()?;
    Some(CountPred { op, n })
}

/// The loaded section-rules registry.
pub struct LoadedRules {
    pub rules: Vec<Rule>,
}

/// Load `.mdatron/section-rules.yaml`. `Ok(None)` when absent (family inactive);
/// `Err` when unreadable, malformed, or carrying a rule that is neither a
/// well-formed count rule nor a well-formed disjoint rule (loud, strict).
pub fn load(project_root: &Path) -> Result<Option<LoadedRules>, Error> {
    let path = project_root.join(".mdatron").join(SECTION_RULES_NAME);
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
    let raw: RawRules = serde_yaml_ng::from_str(&content)
        .map_err(|e| Error::Config(format!("cannot parse '{}': {e}", path.display())))?;

    let compile = |p: &str| {
        regex_lite::Regex::new(p).map_err(|e| {
            Error::Config(format!("section-rules pattern '{p}' does not compile: {e}"))
        })
    };

    let mut rules = Vec::with_capacity(raw.rules.len());
    for r in raw.rules {
        let rule =
            match r.disjoint {
                Some(ops) => {
                    if r.section.is_some()
                        || r.element.is_some()
                        || r.match_pattern.is_some()
                        || r.count.is_some()
                    {
                        return Err(Error::Config(
                            "a section-rule with `disjoint` must not also carry count-rule fields \
                         (section/element/match/count)"
                                .into(),
                        ));
                    }
                    let [a, b]: [RawOperand; 2] = ops.try_into().map_err(|_| {
                        Error::Config("a `disjoint` rule takes exactly two sections".into())
                    })?;
                    Rule::Disjoint {
                        a: Operand {
                            id_pattern: compile(&a.id_pattern)?,
                            section: a.section,
                            id_source: a.id_from,
                        },
                        b: Operand {
                            id_pattern: compile(&b.id_pattern)?,
                            section: b.section,
                            id_source: b.id_from,
                        },
                    }
                }
                None => {
                    let (section, element, pattern, count) =
                        match (r.section, r.element, r.match_pattern, r.count) {
                            (Some(s), Some(e), Some(m), Some(c)) => (s, e, m, c),
                            _ => return Err(Error::Config(
                                "a count section-rule requires section, element, match, and count \
                                 (or use `disjoint` for a disjointness rule)"
                                    .into(),
                            )),
                        };
                    let pred = parse_count_pred(&count).ok_or_else(|| {
                        Error::Config(format!(
                        "section-rule count predicate '{count}' is not `<op> <n>` (e.g. \">= 1\")"
                    ))
                    })?;
                    Rule::Count {
                        matcher: compile(&pattern)?,
                        section,
                        level: element.level(),
                        pred,
                    }
                }
            };
        rules.push(rule);
    }
    Ok(Some(LoadedRules { rules }))
}

/// Apply the section rules to one governed file. `content` is the whole file;
/// `body_offset` is where the prose body begins.
pub fn check_file(
    rules: &[Rule],
    path: &Path,
    content: &str,
    body_offset: usize,
    findings: &mut Vec<Finding>,
) {
    if rules.is_empty() {
        return;
    }
    let body = &content[body_offset..];
    for rule in rules {
        match rule {
            Rule::Count {
                section,
                level,
                matcher,
                pred,
            } => {
                let count = section_span(body, section)
                    .map(|s| count_matching_headings(s, *level, matcher))
                    .unwrap_or(0);
                if !pred.holds(count) {
                    findings.push(section_finding(
                        path,
                        content,
                        section_line(content, body_offset, section),
                        "MDATRON-E0120",
                        "section-count-violation",
                        &format!(
                            "section '{section}' has {count} matching h{level} heading(s); the \
                             rule requires the count {}",
                            pred.describe()
                        ),
                        "section",
                        section,
                    ));
                }
            }
            Rule::Disjoint { a, b } => {
                let ids_a = extract_ids(body, a);
                let ids_b = extract_ids(body, b);
                let mut overlap: Vec<&String> = ids_a.intersection(&ids_b).collect();
                if !overlap.is_empty() {
                    overlap.sort();
                    let shared = overlap
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    findings.push(section_finding(
                        path,
                        content,
                        section_line(content, body_offset, &a.section),
                        "MDATRON-E0121",
                        "section-ids-not-disjoint",
                        &format!(
                            "sections '{}' and '{}' must have disjoint ids but share: {shared}",
                            a.section, b.section
                        ),
                        "shared-ids",
                        &shared,
                    ));
                }
            }
        }
    }
}

/// Count the `level`-level headings in a section span whose line matches `matcher`.
fn count_matching_headings(section: &str, level: usize, matcher: &regex_lite::Regex) -> usize {
    non_fenced_lines(section)
        .into_iter()
        .filter(|(_, line)| {
            atx_heading(line).map(|(l, _)| l == level).unwrap_or(false) && matcher.is_match(line)
        })
        .count()
}

/// Extract the id set for one disjoint operand — HEADING-SCOPED or BULLET-LEAD-
/// SCOPED per `id_source`, never a full-span scan, so a body mention of an id is
/// not collected (vsdd-cli#29's false-overlap trap).
fn extract_ids(body: &str, op: &Operand) -> HashSet<String> {
    let mut ids = HashSet::new();
    let Some(section) = section_span(body, &op.section) else {
        return ids;
    };
    for (_, line) in non_fenced_lines(section) {
        let source_text = match op.id_source {
            IdSource::H3Heading => atx_heading(line).filter(|(l, _)| *l == 3).map(|(_, t)| t),
            IdSource::BulletLead => list_item_bold_name(line),
        };
        if let Some(text) = source_text {
            if let Some(caps) = op.id_pattern.captures(text) {
                if let Some(id) = caps.get(1) {
                    ids.insert(id.as_str().to_string());
                }
            }
        }
    }
    ids
}

/// The 1-based file line of a section's heading (for the finding location), or 1
/// if the section is absent.
fn section_line(content: &str, body_offset: usize, section: &str) -> u32 {
    if let Some((want_level, want_text)) = atx_heading(section) {
        let body = &content[body_offset..];
        for (offset, line) in non_fenced_lines(body) {
            if let Some((l, t)) = atx_heading(line) {
                if l == want_level && t == want_text {
                    let abs = body_offset + offset;
                    return 1 + content[..abs.min(content.len())].matches('\n').count() as u32;
                }
            }
        }
    }
    1
}

#[allow(clippy::too_many_arguments)]
fn section_finding(
    path: &Path,
    _content: &str,
    line: u32,
    code: &str,
    summary: &str,
    message: &str,
    label: &str,
    quoted: &str,
) -> Finding {
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

    fn rx(p: &str) -> regex_lite::Regex {
        regex_lite::Regex::new(p).unwrap()
    }

    #[test]
    fn count_pred_parses_and_holds() {
        let p = parse_count_pred(">= 1").unwrap();
        assert!(p.holds(5) && p.holds(1) && !p.holds(0));
        assert_eq!(p.describe(), ">= 1");
        assert!(parse_count_pred("== 1").unwrap().holds(1));
        assert!(!parse_count_pred("== 1").unwrap().holds(2));
        assert!(parse_count_pred("bogus").is_none());
    }

    #[test]
    fn counts_matching_headings_in_span() {
        let body = "## Requirements\n\n### Phase 2: Slice 2 (sequential)\ntext\n### Phase 3: Slice 4 (sequential)\n\n## Other\n### Phase 9: nope (sequential)\n";
        let span = section_span(body, "## Requirements").unwrap();
        let m = rx(r"^### Phase \d+: .*\((parallel|sequential)\)$");
        assert_eq!(
            count_matching_headings(span, 3, &m),
            2,
            "only the two in-section H3s"
        );
    }

    #[test]
    fn ids_are_scoped_not_full_span() {
        // Phase 2's BODY mentions Slice 3, which must NOT enter the open-id set.
        let body = "## Requirements\n\n### Phase 2: Slice 2 (sequential)\nProvenance: Slice 3 — Install\n\n## Completed phases\n\n- **Slice 3's static half (complete):** done\n- **The engine bullet:** no id\n";
        let open = Operand {
            section: "## Requirements".into(),
            id_source: IdSource::H3Heading,
            id_pattern: rx(r"Slice (\d+)"),
        };
        let done = Operand {
            section: "## Completed phases".into(),
            id_source: IdSource::BulletLead,
            id_pattern: rx(r"Slice (\d+)"),
        };
        let open_ids = extract_ids(body, &open);
        let done_ids = extract_ids(body, &done);
        assert_eq!(
            open_ids,
            HashSet::from(["2".to_string()]),
            "only the heading Slice, not the body mention"
        );
        assert_eq!(done_ids, HashSet::from(["3".to_string()]));
        assert!(
            open_ids.is_disjoint(&done_ids),
            "no false overlap on Slice 3"
        );
    }
}
