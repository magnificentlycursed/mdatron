//! Section-structural check family (#157; vsdd GH#20 P5, the body-content leg of
//! #149): declarative **count** and **disjointness** assertions over markdown
//! body sections. This is the body-content realization of the DSL's
//! frontmatter count-with-predicate — the DSL half ships over frontmatter (#149,
//! `count(filter(...))` / `count(intersect(...))`), and this is the gated body
//! half, delivered as a dedicated *fixed-semantics* check family on the shared
//! `markup`/section spine rather than as an extension of the (body-content-gated)
//! rule DSL.
//!
//! Rules are **route-attached** (#34, vsdd GH#34): a `section_rules:` block on a
//! route (the sibling of `marker_rules:`), so each rule is scoped by that route's
//! `files` glob and a file-specific structural invariant cannot misfire
//! corpus-wide. Two rule shapes (semantics pinned on vsdd-cli#29 against the live
//! `.design/build-plan.md`):
//!
//! - **Count** `{ section, element, match, count }` — count the elements of the
//!   `element` class (`h3`, any `heading`, a `list-item-bold-name` bullet — the
//!   one [`ElementClass`] vocabulary shared with marker rules) inside `section`
//!   (its span until the next heading of the same or higher level) whose line
//!   matches `match`, and assert the `count` predicate (`>= 1`, `== 1`, `< 3`,
//!   …). A failure is `MDATRON-E0120`. Live case: "at least one open-phase H3
//!   in `## Requirements`" — an empty section is the retire trigger, so the
//!   invariant is `>= 1`, not `== 1`.
//! - **Disjoint** `{ disjoint: [op, op] }`, `op = { section, element, id_pattern }`
//!   — extract an id set from each section (`element: h3` from the H3 heading
//!   text, `list-item-bold-name` from `- **bold**` bullet leads; `id_pattern`
//!   captures the id in group 1), and assert the two sets share no element. An
//!   overlap is `MDATRON-E0121`. (`id_from`, `h3-heading`, and `bullet-lead` are
//!   the retired 0.6.0 spellings, accepted as aliases.)
//!
//! The **id extraction is deliberately element-scoped, not a full-span text
//! scan** (vsdd-cli#29's load-bearing trap): a body line under an
//! open phase (`Provenance: Slice 3 …`) would else collide with a completed
//! `Slice 3` bullet and report a false overlap. Ids come only from the declared
//! element, never the surrounding prose.
//!
//! Adopter patterns compile on the linear-time engine (`regex_lite`, DESIGN § Project declarations (linear-time pattern engines)).

use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;

use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};
use crate::markup::{atx_heading, non_fenced_lines, section_spans, ElementClass};
use crate::Error;

/// One section-structural rule as declared under a route's `section_rules:`
/// (#34, route-attached — sibling of `marker_rules`). A `disjoint` rule and a
/// count rule are the two shapes; validated + compiled by [`compile_rule`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawRule {
    #[serde(default)]
    section: Option<String>,
    #[serde(default)]
    element: Option<ElementClass>,
    #[serde(default, rename = "match")]
    match_pattern: Option<String>,
    #[serde(default)]
    count: Option<String>,
    #[serde(default)]
    disjoint: Option<Vec<RawOperand>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawOperand {
    section: String,
    /// The element class ids come from (`h3`, `heading`, `list-item-bold-name`,
    /// …) — the same vocabulary as marker and count `element`. `id_from` is the
    /// retired 0.6.0 spelling of this key, and `h3-heading` / `bullet-lead` its
    /// retired values; all three are accepted as aliases
    /// (`docs/field-rename-ledger.md`).
    #[serde(alias = "id_from")]
    element: ElementClass,
    id_pattern: String,
}

/// A compiled section-structural rule.
pub enum Rule {
    Count {
        section: String,
        element: ElementClass,
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
    element: ElementClass,
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

/// Validate + compile one route-attached raw section-rule (#34) into a [`Rule`].
/// Loud on a malformed rule — one that is neither a well-formed count rule nor a
/// well-formed disjoint rule (strict; a governance file never degrades silently).
pub(crate) fn compile_rule(r: RawRule) -> Result<Rule, Error> {
    let compile = |p: &str| {
        regex_lite::Regex::new(p).map_err(|e| {
            Error::Config(format!("section-rules pattern '{p}' does not compile: {e}"))
        })
    };
    // GH #48: a `section` spec that does not parse as an ATX heading — or
    // parses with EMPTY heading text (`"##"`) — can never usefully match
    // (`markup::section_spans` starts by parsing the spec as a heading), so the
    // rule would be a silent no-op — refused at load, the same hard posture as
    // a non-compiling pattern.
    let heading_spec = |s: &str| match atx_heading(s) {
        Some((_, text)) if !text.is_empty() => Ok(()),
        _ => Err(Error::Config(format!(
            "section-rules section spec '{s}' is not a heading; a section \
             spec must be the full ATX heading line with non-empty heading \
             text (e.g. '## Requirements')"
        ))),
    };
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
            heading_spec(&a.section)?;
            heading_spec(&b.section)?;
            Ok(Rule::Disjoint {
                a: Operand {
                    id_pattern: compile(&a.id_pattern)?,
                    section: a.section,
                    element: a.element,
                },
                b: Operand {
                    id_pattern: compile(&b.id_pattern)?,
                    section: b.section,
                    element: b.element,
                },
            })
        }
        None => {
            let (section, element, pattern, count) =
                match (r.section, r.element, r.match_pattern, r.count) {
                    (Some(s), Some(e), Some(m), Some(c)) => (s, e, m, c),
                    _ => {
                        return Err(Error::Config(
                            "a count section-rule requires section, element, match, and count \
                         (or use `disjoint` for a disjointness rule)"
                                .into(),
                        ))
                    }
                };
            let pred = parse_count_pred(&count).ok_or_else(|| {
                Error::Config(format!(
                    "section-rule count predicate '{count}' is not `<op> <n>`: op is one of \
                     >= <= == != > < and n an integer (e.g. \">= 1\")"
                ))
            })?;
            heading_spec(&section)?;
            Ok(Rule::Count {
                matcher: compile(&pattern)?,
                section,
                element,
                pred,
            })
        }
    }
}

/// Apply the route-attached section rules for one governed file (#34). `rules`
/// are the section rules of every route claiming this file (borrowed, like
/// `marker::check_file`). `content` is the whole file; `body_offset` is where the
/// prose body begins.
pub fn check_file(
    rules: &[&Rule],
    path: &Path,
    content: &str,
    body_offset: usize,
    findings: &mut Vec<Finding>,
) {
    if rules.is_empty() {
        return;
    }
    let body = &content[body_offset..];
    for &rule in rules {
        match rule {
            Rule::Count {
                section,
                element,
                matcher,
                pred,
            } => {
                // GH #48 round 2: evaluate over ALL spans matching the spec, so
                // content under a DUPLICATE same-level/same-text heading cannot
                // evade the gate (the marker family already merges same-name
                // spans; this keeps the families aligned).
                let spans = section_spans(body, section);
                if spans.is_empty() {
                    // GH #48 finding 1 (fail-open): an ABSENT section used to
                    // count as 0, so a predicate satisfied by 0 passed silently
                    // after a heading rename. Loud absence instead (the pin
                    // family's E0063 posture): the predicate is NOT evaluated.
                    findings.push(section_finding(
                        path,
                        content,
                        section_line(content, body_offset, section),
                        "MDATRON-E0122",
                        "section-not-found",
                        // #165: the section name is adopter-derived — it rides in
                        // the quoted region, not inline in the message.
                        "no heading in this document matches the section rule's \
                         section spec (matching is exact on level and text), so \
                         its count assertion cannot be evaluated",
                        vec![QuotedRegion {
                            platform_variant: false,
                            label: "section".into(),
                            content: section.clone(),
                        }],
                    ));
                } else {
                    let count: usize = spans
                        .iter()
                        .map(|s| count_matching_elements(s, *element, matcher))
                        .sum();
                    if !pred.holds(count) {
                        findings.push(section_finding(
                            path,
                            content,
                            section_line(content, body_offset, section),
                            "MDATRON-E0120",
                            "section-count-violation",
                            // #165: the section name is adopter-derived — it rides in
                            // the quoted region, not inline in the message.
                            // Round 3 wording: the count sums over EVERY span of
                            // the named section (a duplicated heading has several),
                            // so the message must not imply a single region.
                            &format!(
                                "the named section has {count} matching {} \
                                 element(s) across its matching span(s); the rule \
                                 requires the count {}",
                                element.as_str(),
                                pred.describe()
                            ),
                            vec![QuotedRegion {
                                platform_variant: false,
                                label: "section".into(),
                                content: section.clone(),
                            }],
                        ));
                    }
                }
            }
            Rule::Disjoint { a, b } => {
                // GH #48 finding 1 (fail-open): a renamed/absent operand section
                // used to yield an empty id set, and empty-vs-empty is disjoint —
                // the rule passed forever after a heading rename. Each absent
                // operand is loud (E0122), and the disjointness comparison runs
                // ONLY when both operands have at least one matching span. Round
                // 2: each operand's ids are the UNION over ALL spans matching its
                // spec, so an id under a duplicate heading cannot evade the gate.
                let spans_a = section_spans(body, &a.section);
                let spans_b = section_spans(body, &b.section);
                for (op, spans) in [(a, &spans_a), (b, &spans_b)] {
                    if spans.is_empty() {
                        findings.push(section_finding(
                            path,
                            content,
                            section_line(content, body_offset, &op.section),
                            "MDATRON-E0122",
                            "section-not-found",
                            // #165: the section name is adopter-derived — it rides
                            // in the quoted region, not inline in the message.
                            "no heading in this document matches a disjointness \
                             operand's section spec (matching is exact on level \
                             and text), so the disjointness assertion cannot be \
                             evaluated",
                            vec![QuotedRegion {
                                platform_variant: false,
                                label: "section".into(),
                                content: op.section.clone(),
                            }],
                        ));
                    }
                }
                if spans_a.is_empty() || spans_b.is_empty() {
                    continue;
                }
                let union = |spans: &[&str], op: &Operand| {
                    let mut ids = HashSet::new();
                    for span in spans {
                        ids.extend(extract_ids(span, op));
                    }
                    ids
                };
                let ids_a = union(&spans_a, a);
                let ids_b = union(&spans_b, b);
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
                        // #165: the two section names are adopter-derived and the
                        // shared ids are captured from the governed body (the
                        // primary trust boundary) — all ride in quoted regions,
                        // never inline in the engine-authored message.
                        "two sections that must have disjoint ids share one or more",
                        vec![
                            QuotedRegion {
                                platform_variant: false,
                                label: "section a".into(),
                                content: a.section.clone(),
                            },
                            QuotedRegion {
                                platform_variant: false,
                                label: "section b".into(),
                                content: b.section.clone(),
                            },
                            QuotedRegion {
                                platform_variant: false,
                                label: "shared ids".into(),
                                content: shared,
                            },
                        ],
                    ));
                }
            }
        }
    }
}

/// Count the lines of a section span that are elements of `element` (an `h3`
/// heading, any `heading`, a `list-item-bold-name` bullet, …) AND match
/// `matcher` — the regex is matched against the whole element line.
fn count_matching_elements(
    section: &str,
    element: ElementClass,
    matcher: &regex_lite::Regex,
) -> usize {
    non_fenced_lines(section)
        .into_iter()
        // A span opens with the section's own heading line (offset 0); it is
        // the container, never one of the counted elements — the marker
        // family's `extract_members` skips it the same way.
        .filter(|(offset, _)| *offset != 0)
        .filter(|(_, line)| element.name_in(line).is_some() && matcher.is_match(line))
        .count()
}

/// Extract the id set for one disjoint operand from its RESOLVED section span —
/// ELEMENT-SCOPED per the operand's `element` class, never a full-span scan,
/// so a body mention of an id is not collected (vsdd-cli#29's false-overlap
/// trap). The caller resolves the span first (GH #48): an absent section is a
/// loud `E0122`, never an empty set that trivially satisfies disjointness.
fn extract_ids(section: &str, op: &Operand) -> HashSet<String> {
    let mut ids = HashSet::new();
    for (offset, line) in non_fenced_lines(section) {
        if offset == 0 {
            continue; // the span's own heading line is the container, not an id source
        }
        if let Some(text) = op.element.name_in(line) {
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
    quoted: Vec<QuotedRegion>,
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
        quoted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup::section_span;

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

    // #204 D2-1: one element vocabulary for count and disjoint rules, and the
    // retired 0.6.0 spellings (`id_from`, `h3-heading`, `bullet-lead`) parse to
    // the same variants as aliases (docs/field-rename-ledger.md).
    #[test]
    fn element_aliases_parse_to_the_unified_class() {
        let parse = |y: &str| serde_yaml_ng::from_str::<RawOperand>(y).unwrap().element;
        assert_eq!(
            parse("section: '## A'\nid_from: h3-heading\nid_pattern: x"),
            ElementClass::H3
        );
        assert_eq!(
            parse("section: '## A'\nid_from: bullet-lead\nid_pattern: x"),
            ElementClass::ListItemBoldName
        );
        assert_eq!(
            parse("section: '## A'\nelement: list-item-bold-name\nid_pattern: x"),
            ElementClass::ListItemBoldName
        );
        assert_eq!(
            parse("section: '## A'\nelement: h2\nid_pattern: x"),
            ElementClass::H2
        );
        let rule: RawRule =
            serde_yaml_ng::from_str("section: '## A'\nelement: heading\nmatch: .\ncount: '>= 1'")
                .unwrap();
        assert_eq!(rule.element, Some(ElementClass::Heading));
        assert!(
            serde_yaml_ng::from_str::<RawOperand>("section: '## A'\nelement: h7\nid_pattern: x")
                .is_err(),
            "an unknown element class is refused at load"
        );
    }

    // #204 D2-1: a count rule counts elements of ANY class — a level-specific
    // heading, any heading, or a bold-lead bullet — never the span's own heading.
    #[test]
    fn count_rules_count_any_element_class() {
        let body = "## A\n\n### One\n#### Deeper\n- **Item x.** a\n- **Item y.** b\n- plain\n\n## B\n### Not in A\n";
        let span = section_span(body, "## A").unwrap();
        let any = rx(".");
        assert_eq!(
            count_matching_elements(span, ElementClass::Heading, &any),
            2,
            "`heading` counts every level inside the span, not the section's own heading"
        );
        assert_eq!(count_matching_elements(span, ElementClass::H2, &any), 0);
        assert_eq!(count_matching_elements(span, ElementClass::H4, &any), 1);
        assert_eq!(
            count_matching_elements(span, ElementClass::ListItemBoldName, &any),
            2,
            "bold-lead bullets only, never the plain bullet"
        );
        assert_eq!(
            count_matching_elements(span, ElementClass::ListItemBoldName, &rx("Item x")),
            1,
            "the regex runs over the element's line"
        );
    }

    // Round-2 M3: an operand's own section heading is the container, never an
    // id source — `element: heading` on a `## Slice 9 group` operand must not
    // collect `9`.
    #[test]
    fn extract_ids_never_reads_the_operands_own_heading() {
        let body = "## Slice 9 group\n### Slice 1\n- **Slice 2.** x\n";
        let span = section_span(body, "## Slice 9 group").unwrap();
        let mk = |element| Operand {
            section: "## Slice 9 group".into(),
            element,
            id_pattern: rx(r"Slice (\d+)"),
        };
        assert_eq!(
            extract_ids(span, &mk(ElementClass::Heading)),
            HashSet::from(["1".to_string()])
        );
        assert_eq!(
            extract_ids(span, &mk(ElementClass::H2)),
            HashSet::new(),
            "the span's own h2 is not an element inside it"
        );
        assert_eq!(
            extract_ids(span, &mk(ElementClass::ListItemBoldName)),
            HashSet::from(["2".to_string()])
        );
    }

    #[test]
    fn counts_matching_headings_in_span() {
        let body = "## Requirements\n\n### Phase 2: Slice 2 (sequential)\ntext\n### Phase 3: Slice 4 (sequential)\n\n## Other\n### Phase 9: nope (sequential)\n";
        let span = section_span(body, "## Requirements").unwrap();
        let m = rx(r"^### Phase \d+: .*\((parallel|sequential)\)$");
        assert_eq!(
            count_matching_elements(span, ElementClass::H3, &m),
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
            element: ElementClass::H3,
            id_pattern: rx(r"Slice (\d+)"),
        };
        let done = Operand {
            section: "## Completed phases".into(),
            element: ElementClass::ListItemBoldName,
            id_pattern: rx(r"Slice (\d+)"),
        };
        let open_ids = extract_ids(section_span(body, &open.section).unwrap(), &open);
        let done_ids = extract_ids(section_span(body, &done.section).unwrap(), &done);
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

    // RED GATE (GH #48 round 2, reviewer repro): content under a DUPLICATE
    // same-level/same-text heading must NOT evade the gate. An empty first
    // `## Open questions` followed by a second one containing `### Q1` used to
    // pass `count: "== 0"` clean (only the first span was counted); the count
    // is now the sum over ALL matching spans → E0120.
    #[test]
    fn duplicate_heading_content_counts_toward_the_predicate() {
        let rule = Rule::Count {
            section: "## Open questions".into(),
            element: ElementClass::H3,
            matcher: rx(r"^### Q\d+"),
            pred: parse_count_pred("== 0").unwrap(),
        };
        let body = "# Doc\n\n## Open questions\n\nnone right now.\n\n## Other\n\nx\n\n\
                    ## Open questions\n\n### Q1 sneaky\n";
        let mut findings = Vec::new();
        check_file(&[&rule], Path::new("d.md"), body, 0, &mut findings);
        let f = findings
            .iter()
            .find(|f| f.code == "MDATRON-E0120")
            .unwrap_or_else(|| {
                panic!("the Q1 under the duplicate heading violates == 0; got {findings:?}")
            });
        assert!(
            f.message.contains("has 1 matching"),
            "the count sums across all matching spans: {:?}",
            f.message
        );
    }

    // RED GATE (GH #48 round 2): a disjoint operand's ids are the UNION over all
    // spans matching its spec — an id under a second-occurrence span
    // participates in the overlap check.
    #[test]
    fn disjoint_ids_union_across_duplicate_heading_spans() {
        let mk = |section: &str, src: ElementClass| Operand {
            section: section.into(),
            element: src,
            id_pattern: rx(r"Slice (\d+)"),
        };
        let rule = Rule::Disjoint {
            a: mk("## Requirements", ElementClass::H3),
            b: mk("## Completed phases", ElementClass::ListItemBoldName),
        };
        // The overlap (Slice 2) lives under the SECOND `## Requirements` span.
        let body = "## Requirements\n\n### Phase 1: Slice 1 (sequential)\n\n\
                    ## Completed phases\n\n- **Slice 2 done (complete):** x\n\n\
                    ## Requirements\n\n### Phase 2: Slice 2 (sequential)\n";
        let mut findings = Vec::new();
        check_file(&[&rule], Path::new("d.md"), body, 0, &mut findings);
        assert!(
            findings.iter().any(|f| f.code == "MDATRON-E0121"),
            "the Slice 2 overlap under the duplicate span must be detected; got {findings:?}"
        );
    }

    // RED GATE (GH #48 round 2 nit): a heading marker with EMPTY heading text
    // (`"##"`) parses as a heading but can never usefully match — refused at
    // load like a bare spec.
    #[test]
    fn compile_rule_rejects_heading_marker_with_empty_text() {
        let raw = RawRule {
            section: Some("##".into()),
            element: Some(ElementClass::H3),
            match_pattern: Some(r"^### .*$".into()),
            count: Some(">= 1".into()),
            disjoint: None,
        };
        let err = match compile_rule(raw) {
            Err(e) => e,
            Ok(_) => panic!("an empty-text heading spec must be refused"),
        };
        assert!(
            format!("{err}").contains("non-empty"),
            "the error demands non-empty heading text; got {err}"
        );
    }

    // RED GATE (GH #48 finding 1, load-time leg): a count rule's `section` spec
    // that does not parse as an ATX heading can never match anything — refused
    // at load, not shipped as a silent no-op.
    #[test]
    fn compile_rule_rejects_count_section_spec_without_heading_marker() {
        let raw = RawRule {
            section: Some("Requirements".into()),
            element: Some(ElementClass::H3),
            match_pattern: Some(r"^### .*$".into()),
            count: Some(">= 1".into()),
            disjoint: None,
        };
        let err = match compile_rule(raw) {
            Err(e) => e,
            Ok(_) => panic!("a bare 'Requirements' spec must be refused"),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("Requirements") && msg.contains("ATX heading"),
            "the error names the spec and the required shape; got {msg}"
        );
    }

    // RED GATE (GH #48 finding 1, load-time leg): same refusal for each
    // disjoint operand's `section` spec.
    #[test]
    fn compile_rule_rejects_disjoint_operand_spec_without_heading_marker() {
        let raw = RawRule {
            section: None,
            element: None,
            match_pattern: None,
            count: None,
            disjoint: Some(vec![
                RawOperand {
                    section: "## Requirements".into(),
                    element: ElementClass::H3,
                    id_pattern: r"Slice (\d+)".into(),
                },
                RawOperand {
                    section: "Completed phases".into(),
                    element: ElementClass::ListItemBoldName,
                    id_pattern: r"Slice (\d+)".into(),
                },
            ]),
        };
        let err = match compile_rule(raw) {
            Err(e) => e,
            Ok(_) => panic!("a bare operand spec must be refused"),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("Completed phases") && msg.contains("ATX heading"),
            "the error names the offending operand spec; got {msg}"
        );
    }

    // RED GATE (GH #48 finding 1, the CRITICAL silent-pass case): a count
    // predicate satisfied by 0 (`== 0`) used to PASS silently when the named
    // section is absent. It must now be E0122, and the predicate must not be
    // evaluated.
    #[test]
    fn absent_section_with_zero_satisfiable_predicate_is_e0122_not_silent() {
        let rule = Rule::Count {
            section: "## Requirements".into(),
            element: ElementClass::H3,
            matcher: rx(r"^### .*$"),
            pred: parse_count_pred("== 0").unwrap(),
        };
        let body = "# Doc\n\n## Renamed Requirements\n\n### Phase 1: x\n";
        let mut findings = Vec::new();
        check_file(&[&rule], Path::new("d.md"), body, 0, &mut findings);
        let f = findings
            .iter()
            .find(|f| f.code == "MDATRON-E0122")
            .unwrap_or_else(|| panic!("expected E0122 on the absent section; got {findings:?}"));
        assert_eq!(f.summary, "section-not-found");
        assert!(
            !f.message.contains("Requirements"),
            "the spec rides in quoted[], not the message: {:?}",
            f.message
        );
        assert!(f
            .quoted
            .iter()
            .any(|q| q.label == "section" && q.content == "## Requirements"));
    }

    // RED GATE (GH #48 finding 1): an absent section under `>= 1` is E0122
    // (section-not-found), NOT an E0120 with count 0 — the absence is reported
    // as absence, never as a count.
    #[test]
    fn absent_section_is_e0122_not_e0120_with_count_zero() {
        let rule = Rule::Count {
            section: "## Requirements".into(),
            element: ElementClass::H3,
            matcher: rx(r"^### .*$"),
            pred: parse_count_pred(">= 1").unwrap(),
        };
        let body = "# Doc\n\nno such section here.\n";
        let mut findings = Vec::new();
        check_file(&[&rule], Path::new("d.md"), body, 0, &mut findings);
        assert!(
            findings.iter().any(|f| f.code == "MDATRON-E0122"),
            "absent section is E0122; got {findings:?}"
        );
        assert!(
            findings.iter().all(|f| f.code != "MDATRON-E0120"),
            "absence must not masquerade as a count violation; got {findings:?}"
        );
    }

    // RED GATE (GH #48 finding 1): a disjoint rule whose operand section was
    // renamed used to compare empty-vs-empty (= disjoint = silent pass). It must
    // now be exactly one E0122 naming THAT operand, no E0121, no silent pass.
    #[test]
    fn disjoint_with_renamed_operand_section_is_e0122_for_that_operand() {
        let mk = |section: &str| Operand {
            section: section.into(),
            element: ElementClass::H3,
            id_pattern: rx(r"Slice (\d+)"),
        };
        let rule = Rule::Disjoint {
            a: mk("## Requirements"),
            b: mk("## Completed phases"),
        };
        // `## Completed phases` was renamed to `## Done` in the doc.
        let body = "## Requirements\n\n### Slice 1\n\n## Done\n\n### Slice 1\n";
        let mut findings = Vec::new();
        check_file(&[&rule], Path::new("d.md"), body, 0, &mut findings);
        let e0122: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "MDATRON-E0122")
            .collect();
        assert_eq!(
            e0122.len(),
            1,
            "exactly one E0122, for the renamed operand; got {findings:?}"
        );
        assert!(e0122[0]
            .quoted
            .iter()
            .any(|q| q.label == "section" && q.content == "## Completed phases"));
        assert!(
            findings.iter().all(|f| f.code != "MDATRON-E0121"),
            "no disjointness verdict when an operand span is missing; got {findings:?}"
        );
    }

    // #165: E0121's two section names (adopter) and the shared ids (captured from
    // the governed body — the primary trust boundary) ride in quoted regions,
    // never inline in the engine-authored message.
    #[test]
    fn e0121_message_is_engine_authored_governed_ids_quoted() {
        let body = "## Alpha\n\n### zqxshared\n\n## Beta\n\n### zqxshared\n";
        let mk = |section: &str| Operand {
            section: section.into(),
            element: ElementClass::H3,
            id_pattern: rx(r"(zqx\w+)"),
        };
        let rule = Rule::Disjoint {
            a: mk("## Alpha"),
            b: mk("## Beta"),
        };
        let mut findings = Vec::new();
        check_file(&[&rule], Path::new("d.md"), body, 0, &mut findings);
        let f = findings
            .iter()
            .find(|f| f.code == "MDATRON-E0121")
            .unwrap_or_else(|| panic!("expected E0121; got {findings:?}"));
        assert!(
            !f.message.contains("Alpha")
                && !f.message.contains("Beta")
                && !f.message.contains("zqxshared"),
            "no adopter/governed token echoes into the message: {:?}",
            f.message
        );
        assert!(f
            .quoted
            .iter()
            .any(|q| q.label == "section a" && q.content == "## Alpha"));
        assert!(f
            .quoted
            .iter()
            .any(|q| q.label == "section b" && q.content == "## Beta"));
        assert!(
            f.quoted
                .iter()
                .any(|q| q.label == "shared ids" && q.content.contains("zqxshared")),
            "shared ids quoted: {:?}",
            f.quoted
        );
    }
}
