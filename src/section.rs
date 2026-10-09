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
//! - **Every** `{ section, element, every }` (#213) — every element of the
//!   class in the section must match the `every` pattern; each one that does
//!   not is `MDATRON-E0123`, located at its own line.
//! - **Order** `{ section, order: [item, item, …] }`, `item = { element, match }`
//!   (#214) — an element matching an earlier item must not appear after one
//!   matching a later item; each one that does is `MDATRON-E0124`. Order
//!   asserts sequence only: an item nothing matches is not a violation
//!   (presence is a count rule's job).
//!
//! - **Budget** `{ section, max_bytes }` / `{ section?, max_bytes, per: paragraph }`
//!   (#253) — every span of the section (its heading line through its nested
//!   subsections, what an injector reads) must fit in `max_bytes`, else
//!   `MDATRON-E0125`; with `per: paragraph`, each paragraph — a run of adjacent
//!   non-blank, non-heading lines outside code fences — in the section (or the
//!   whole body) must fit, else `MDATRON-E0126` at the paragraph's first line.
//!   Bytes are counted as checked out, like the route's per-file `max_bytes`.
//!
//! On a count, every or order rule `section` is optional (#217/#218): absent,
//! the rule evaluates over the whole document body — so "the file is not
//! empty" is a count of `line` elements, with no heading to anchor on.
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
    /// What a count rule's `match` is tested against: the whole element
    /// `line` (the default, and the only behavior through 0.6.0) or the
    /// element's `name` — the heading text or the list item's bold name, the same text a
    /// disjoint operand's `id_pattern` and a marker target's member lookup
    /// see. Additive (#201 ruling): absent keeps every existing rule's meaning.
    #[serde(default)]
    match_on: Option<MatchOn>,
    /// The pattern EVERY element of the class in the section must match (#213);
    /// tested per `match_on`, like a count rule's `match`.
    #[serde(default)]
    every: Option<String>,
    /// The sequence the section's elements must keep (#214).
    #[serde(default)]
    order: Option<Vec<RawOrderItem>>,
    #[serde(default)]
    disjoint: Option<Vec<RawOperand>>,
    /// The byte budget of a section span, or of each paragraph (#253).
    #[serde(default)]
    max_bytes: Option<usize>,
    /// What `max_bytes` bounds: absent = each span of the section; `paragraph`
    /// = each paragraph in the scope (#253).
    #[serde(default)]
    per: Option<BudgetUnit>,
}

/// What a budget rule's `max_bytes` bounds (#253).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BudgetUnit {
    /// Each paragraph: a run of adjacent non-blank, non-heading lines outside
    /// code fences.
    Paragraph,
}

/// One step of an `order` rule: the elements of `element` matching `match`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawOrderItem {
    element: ElementClass,
    #[serde(rename = "match")]
    match_pattern: String,
    #[serde(default)]
    match_on: Option<MatchOn>,
}

/// What a count rule's `match` regex is tested against (see [`RawRule`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchOn {
    /// The whole element line, marker included (`### REQ-1`).
    #[default]
    Line,
    /// The element's name: the heading text or the list item's bold name (`REQ-1`).
    Name,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawOperand {
    section: String,
    /// The element class ids come from (`h3`, `heading`, `list-item-bold-name`,
    /// …) — the same vocabulary as marker and count `element`. `id_from` is the
    /// retired 0.6.0 spelling of this key, and `h3-heading` / `bullet-lead` its
    /// retired values; all three are accepted as aliases
    /// (`DESIGN.md` § Input-field renames are aliased and ledgered).
    #[serde(alias = "id_from")]
    element: ElementClass,
    id_pattern: String,
}

/// A compiled section-structural rule. `section: None` on a count, every or
/// order rule means the whole document body.
pub enum Rule {
    Count {
        section: Option<String>,
        element: ElementClass,
        matcher: regex_lite::Regex,
        on: MatchOn,
        pred: CountPred,
    },
    Every {
        section: Option<String>,
        element: ElementClass,
        matcher: regex_lite::Regex,
        on: MatchOn,
    },
    Order {
        section: Option<String>,
        items: Vec<OrderItem>,
    },
    Disjoint {
        a: Operand,
        b: Operand,
    },
    /// `per: None` bounds each span of `section` (required then); `per:
    /// Some(Paragraph)` each paragraph of the section, or of the body.
    Budget {
        section: Option<String>,
        max: usize,
        per: Option<BudgetUnit>,
    },
}

/// One compiled step of an order rule.
pub struct OrderItem {
    element: ElementClass,
    matcher: regex_lite::Regex,
    on: MatchOn,
}

impl OrderItem {
    fn matches(&self, line: &str) -> bool {
        element_matches(self.element, &self.matcher, self.on, line)
    }

    /// The item as a reader names it: its element class and its pattern.
    fn describe(&self) -> String {
        let on = match self.on {
            MatchOn::Line => "",
            MatchOn::Name => " (on its name)",
        };
        format!(
            "{} matching {}{on}",
            self.element.as_str(),
            self.matcher.as_str()
        )
    }

    fn same_as(&self, other: &Self) -> bool {
        self.element == other.element
            && self.on == other.on
            && self.matcher.as_str() == other.matcher.as_str()
    }
}

/// Whether `line` is an element of `element` that `matcher` accepts, tested
/// against the whole line or the element's name per `on`.
fn element_matches(
    element: ElementClass,
    matcher: &regex_lite::Regex,
    on: MatchOn,
    line: &str,
) -> bool {
    match (element.name_in(line), on) {
        (None, _) => false,
        (Some(_), MatchOn::Line) => matcher.is_match(line),
        (Some(name), MatchOn::Name) => matcher.is_match(name),
    }
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
    /// No count is below zero.
    fn never_holds(&self) -> bool {
        matches!(self.op, CmpOp::Lt) && self.n == 0
    }
    /// Every count is at least zero.
    fn always_holds(&self) -> bool {
        matches!(self.op, CmpOp::Ge) && self.n == 0
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
        // A spec holding a line break can never equal a heading line.
        Some((_, text)) if !text.is_empty() && !s.contains(['\n', '\r']) => Ok(()),
        _ => Err(Error::Config(format!(
            "section-rules section spec '{s}' is not a heading; a section \
             spec must be the full ATX heading line with non-empty heading \
             text (e.g. '## Requirements')"
        ))),
    };
    // The whole-document form: `section` is optional on count, every and order
    // rules, but a section that IS given must be a real heading spec.
    let optional_section = |s: Option<String>| -> Result<Option<String>, Error> {
        if let Some(spec) = &s {
            heading_spec(spec)?;
        }
        Ok(s)
    };
    if r.per.is_some() && r.max_bytes.is_none() {
        return Err(Error::Config(
            "a section-rule with `per` needs `max_bytes`; `per` says what the budget bounds".into(),
        ));
    }
    if let Some(max) = r.max_bytes {
        if r.element.is_some()
            || r.match_pattern.is_some()
            || r.count.is_some()
            || r.match_on.is_some()
            || r.every.is_some()
            || r.order.is_some()
            || r.disjoint.is_some()
        {
            return Err(Error::Config(
                "a section-rule with `max_bytes` takes only `section` and `per` beside it; \
                 a budget bounds bytes, it does not match elements"
                    .into(),
            ));
        }
        if max == 0 {
            return Err(Error::Config(
                "a section-rule `max_bytes` of 0 would refuse every section and paragraph; \
                 give the budget the reader actually has"
                    .into(),
            ));
        }
        if r.section.is_none() && r.per.is_none() {
            return Err(Error::Config(
                "a section-rule `max_bytes` with no `section` and no `per: paragraph` would \
                 bound the whole body; bound the whole file with the route's own `max_bytes`"
                    .into(),
            ));
        }
        return Ok(Rule::Budget {
            section: optional_section(r.section)?,
            max,
            per: r.per,
        });
    }
    if let Some(items) = r.order {
        if r.element.is_some()
            || r.match_pattern.is_some()
            || r.count.is_some()
            || r.match_on.is_some()
            || r.every.is_some()
            || r.disjoint.is_some()
        {
            return Err(Error::Config(
                "a section-rule with `order` takes only `section` beside it; each order item \
                 carries its own element/match/match_on"
                    .into(),
            ));
        }
        if items.len() < 2 {
            return Err(Error::Config(
                "an `order` rule takes at least two items; one item orders nothing".into(),
            ));
        }
        let items = items
            .into_iter()
            .map(|i| {
                // The empty pattern matches every element of the class, so
                // (first match wins) it shadows every later item the class
                // covers: the trivially decidable dead step, refused like an
                // empty `every`.
                if i.match_pattern.is_empty() {
                    return Err(Error::Config(
                        "an `order` item has an empty `match`, which every element of its \
                         class matches; use \".\" for \"any non-empty\" or a pattern that \
                         names the elements this step is for"
                            .into(),
                    ));
                }
                Ok(OrderItem {
                    matcher: compile(&i.match_pattern)?,
                    element: i.element,
                    on: i.match_on.unwrap_or_default(),
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        // An element belongs to the first item it matches, so a repeated item
        // can never be reached: a dead step, refused like any dead knob.
        for (i, item) in items.iter().enumerate() {
            if items[..i].iter().any(|earlier| earlier.same_as(item)) {
                return Err(Error::Config(format!(
                    "an `order` rule repeats the item `{}`; an element belongs to the first \
                     item it matches, so the repeat can never be reached",
                    item.describe()
                )));
            }
        }
        return Ok(Rule::Order {
            section: optional_section(r.section)?,
            items,
        });
    }
    if let Some(pattern) = r.every {
        if r.match_pattern.is_some() || r.count.is_some() || r.disjoint.is_some() {
            return Err(Error::Config(
                "a section-rule with `every` must not also carry match, count or disjoint; \
                 `every` is the pattern each element must match"
                    .into(),
            ));
        }
        let Some(element) = r.element else {
            return Err(Error::Config(
                "an `every` section-rule requires element (the class whose every member \
                 must match)"
                    .into(),
            ));
        };
        if pattern.is_empty() {
            return Err(Error::Config(
                "an `every` section-rule has an empty pattern, which every element matches; \
                 the rule could never report anything"
                    .into(),
            ));
        }
        return Ok(Rule::Every {
            matcher: compile(&pattern)?,
            section: optional_section(r.section)?,
            element,
            on: r.match_on.unwrap_or_default(),
        });
    }
    match r.disjoint {
        Some(ops) => {
            if r.section.is_some()
                || r.element.is_some()
                || r.match_pattern.is_some()
                || r.count.is_some()
                || r.match_on.is_some()
            {
                return Err(Error::Config(
                    "a section-rule with `disjoint` must not also carry count-rule fields \
                     (section/element/match/match_on/count)"
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
            let (element, pattern, count) = match (r.element, r.match_pattern, r.count) {
                (Some(e), Some(m), Some(c)) => (e, m, c),
                _ => {
                    return Err(Error::Config(
                        "a count section-rule requires element, match, and count (section is \
                         optional: absent means the whole document); or use `every`, `order` \
                         or `disjoint` for the other rule shapes"
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
            // A predicate that cannot fail, or cannot hold, makes a rule that
            // reports never or always. `>= 0` on a SECTION still asserts the
            // section exists (E0122); on the whole document it asserts nothing.
            if pred.never_holds() {
                return Err(Error::Config(format!(
                    "section-rule count predicate '{count}' can never hold (no count is \
                     below zero); the rule would report every file"
                )));
            }
            if r.section.is_none() && pred.always_holds() {
                return Err(Error::Config(format!(
                    "section-rule count predicate '{count}' can never fail, and with no \
                     `section` there is no heading for the rule to require; it would \
                     assert nothing"
                )));
            }
            Ok(Rule::Count {
                matcher: compile(&pattern)?,
                section: optional_section(r.section)?,
                element,
                on: r.match_on.unwrap_or_default(),
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
    // A leading byte-order mark is not content: left in place it glues itself
    // to the first line, so a heading or a required line there never matches.
    let body_offset = if body_offset == 0 && content.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        body_offset
    };
    let body = &content[body_offset..];
    for &rule in rules {
        // Per-rule: a rule's findings are located in document order, so the
        // cursor counts each newline once, not once per finding.
        let mut lines = LineCursor::new(content);
        match rule {
            Rule::Count {
                section,
                element,
                matcher,
                on,
                pred,
            } => {
                // GH #48 round 2: evaluate over ALL spans matching the spec, so
                // content under a DUPLICATE same-level/same-text heading cannot
                // evade the gate (the marker family already merges same-name
                // spans; this keeps the families aligned).
                let Some(spans) = rule_spans(body, section.as_deref()) else {
                    // GH #48 finding 1 (fail-open): an ABSENT section used to
                    // count as 0, so a predicate satisfied by 0 passed silently
                    // after a heading rename. Loud absence instead (the pin
                    // family's E0063 posture): the predicate is NOT evaluated.
                    findings.push(section_finding(
                        path,
                        content,
                        1,
                        "MDATRON-E0122",
                        "section-not-found",
                        // #165: the section name is adopter-derived — it rides in
                        // the quoted region, not inline in the message.
                        "no heading in this document matches the section rule's \
                         section spec (matching is exact on level and text), so \
                         its count assertion cannot be evaluated",
                        // #219: the rule's own pattern tells two rules on one
                        // section apart.
                        with_section(section, pattern_regions("match", matcher, *on)),
                    ));
                    continue;
                };
                let sectioned = section.is_some();
                let count: usize = spans
                    .iter()
                    .map(|s| count_matching_elements(s, sectioned, *element, matcher, *on))
                    .sum();
                if !pred.holds(count) {
                    // #165: the section name is adopter-derived — it rides in
                    // the quoted region, not inline in the message.
                    // Round 3 wording: the count sums over EVERY span of the
                    // named section (a duplicated heading has several), so the
                    // message must not imply a single region.
                    let message = if sectioned {
                        format!(
                            "the named section has {count} matching {} \
                             element(s) across its matching span(s); the rule \
                             requires the count {}",
                            element.as_str(),
                            pred.describe()
                        )
                    } else {
                        format!(
                            "the document has {count} matching {} element(s); \
                             the rule requires the count {}",
                            element.as_str(),
                            pred.describe()
                        )
                    };
                    findings.push(section_finding(
                        path,
                        content,
                        section_line(content, body_offset, section.as_deref()),
                        "MDATRON-E0120",
                        "section-count-violation",
                        &message,
                        with_section(section, pattern_regions("match", matcher, *on)),
                    ));
                }
            }
            Rule::Every {
                section,
                element,
                matcher,
                on,
            } => {
                let Some(spans) = rule_spans(body, section.as_deref()) else {
                    findings.push(section_finding(
                        path,
                        content,
                        1,
                        "MDATRON-E0122",
                        "section-not-found",
                        "no heading in this document matches the section rule's \
                         section spec (matching is exact on level and text), so \
                         its every-element assertion cannot be evaluated",
                        // The class tells two every rules with one pattern apart.
                        with_section(section, {
                            let mut regions = pattern_regions("every", matcher, *on);
                            regions.push(quoted("element class", element.as_str()));
                            regions
                        }),
                    ));
                    continue;
                };
                for span in spans {
                    for (offset, line) in element_lines(span, section.is_some()) {
                        let Some(name) = element.name_in(line) else {
                            continue;
                        };
                        let subject = match on {
                            MatchOn::Line => line,
                            MatchOn::Name => name,
                        };
                        if matcher.is_match(subject) {
                            continue;
                        }
                        findings.push(section_finding(
                            path,
                            content,
                            lines.line_of(abs_offset(body_offset, body, span, offset)),
                            "MDATRON-E0123",
                            "section-element-mismatch",
                            // #165: the element's text is governed content and
                            // the pattern adopter data; both ride quoted.
                            &format!(
                                "this {} element does not match the pattern the \
                                 rule requires of every such element in its scope",
                                element.as_str()
                            ),
                            with_section(section, {
                                let mut regions = pattern_regions("every", matcher, *on);
                                regions.push(quoted("element", line));
                                regions
                            }),
                        ));
                    }
                }
            }
            Rule::Order { section, items } => {
                let Some(spans) = rule_spans(body, section.as_deref()) else {
                    findings.push(section_finding(
                        path,
                        content,
                        1,
                        "MDATRON-E0122",
                        "section-not-found",
                        "no heading in this document matches the section rule's \
                         section spec (matching is exact on level and text), so \
                         its order assertion cannot be evaluated",
                        // The items tell two order rules on one section apart.
                        with_section(
                            section,
                            vec![quoted(
                                "order",
                                &items
                                    .iter()
                                    .map(OrderItem::describe)
                                    .collect::<Vec<_>>()
                                    .join(", then "),
                            )],
                        ),
                    ));
                    continue;
                };
                // Each span is ordered on its own: a duplicated heading opens a
                // new sequence.
                for span in spans {
                    // The latest order step seen so far in this span.
                    let mut reached: Option<usize> = None;
                    for (offset, line) in element_lines(span, section.is_some()) {
                        // An element belongs to the FIRST item it matches.
                        let Some(step) = items.iter().position(|i| i.matches(line)) else {
                            continue;
                        };
                        match reached {
                            Some(later) if step < later => findings.push(section_finding(
                                path,
                                content,
                                lines.line_of(abs_offset(body_offset, body, span, offset)),
                                "MDATRON-E0124",
                                "section-order-violation",
                                "this element appears after an element the rule's \
                                 order places later",
                                with_section(
                                    section,
                                    vec![
                                        quoted("element", line),
                                        quoted("must precede", &items[later].describe()),
                                    ],
                                ),
                            )),
                            _ => reached = Some(step),
                        }
                    }
                }
            }
            Rule::Budget { section, max, per } => {
                let Some(spans) = rule_spans(body, section.as_deref()) else {
                    findings.push(section_finding(
                        path,
                        content,
                        1,
                        "MDATRON-E0122",
                        "section-not-found",
                        "no heading in this document matches the section rule's \
                         section spec (matching is exact on level and text), so \
                         its byte budget cannot be evaluated",
                        with_section(section, vec![quoted("max_bytes", &max.to_string())]),
                    ));
                    continue;
                };
                for span in spans {
                    match per {
                        None => {
                            if span.len() > *max {
                                findings.push(section_finding(
                                    path,
                                    content,
                                    lines.line_of(abs_offset(body_offset, body, span, 0)),
                                    "MDATRON-E0125",
                                    "section-over-byte-budget",
                                    &format!(
                                        "this section is {} bytes, over the rule's budget of \
                                         {max} bytes (its heading through its subsections)",
                                        span.len()
                                    ),
                                    with_section(section, Vec::new()),
                                ));
                            }
                        }
                        Some(BudgetUnit::Paragraph) => {
                            for (offset, len, first) in paragraphs(span, section.is_some()) {
                                if len <= *max {
                                    continue;
                                }
                                findings.push(section_finding(
                                    path,
                                    content,
                                    lines.line_of(abs_offset(body_offset, body, span, offset)),
                                    "MDATRON-E0126",
                                    "paragraph-over-byte-budget",
                                    &format!(
                                        "this paragraph is {len} bytes, over the rule's \
                                         per-paragraph budget of {max} bytes"
                                    ),
                                    with_section(section, vec![quoted("first line", first)]),
                                ));
                            }
                        }
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
                            section_line(content, body_offset, Some(&op.section)),
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
                        section_line(content, body_offset, Some(&a.section)),
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
    span: &str,
    sectioned: bool,
    element: ElementClass,
    matcher: &regex_lite::Regex,
    on: MatchOn,
) -> usize {
    element_lines(span, sectioned)
        .filter(|(_, line)| element_matches(element, matcher, on, line))
        .count()
}

/// The spans a count, every or order rule evaluates over: every span of the
/// named section, or the whole body when the rule names none. `None` when a
/// named section matches no heading (the caller reports `E0122`).
fn rule_spans<'a>(body: &'a str, section: Option<&str>) -> Option<Vec<&'a str>> {
    match section {
        None => Some(vec![body]),
        Some(spec) => {
            let spans = section_spans(body, spec);
            (!spans.is_empty()).then_some(spans)
        }
    }
}

/// The paragraphs of one span (#253): runs of adjacent non-blank lines that are
/// not headings, outside code fences, as `(offset, byte length, first line)`.
/// A blank line, a heading, or a fence ends a paragraph; the length runs from
/// the first line's start to the last line's end (inner newlines included).
fn paragraphs(span: &str, sectioned: bool) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    // (start offset, end of the last line's text, end of its raw line — past
    // a CRLF's `\r` — and the first line). `element_lines` yields lines with
    // their line ending already trimmed, so adjacency and length are judged
    // on the raw ends: a CRLF file's next line starts at raw end + 1 too, and
    // an inner `\r\n` counts (#253 review: a CRLF file had read as one
    // paragraph per line).
    let mut open: Option<(usize, usize, usize, &str)> = None;
    for (offset, line) in element_lines(span, sectioned) {
        let breaks = line.trim().is_empty() || atx_heading(line).is_some();
        let text_end = offset + line.len();
        let raw_end = text_end + usize::from(span[text_end..].starts_with('\r'));
        // A line not directly after the previous one (a fence or the span's
        // own heading lay between) starts a new paragraph too.
        let adjacent = open.is_some_and(|(_, _, prev_raw, _)| offset == prev_raw + 1);
        if let Some((start, end, _, first)) = open {
            if breaks || !adjacent {
                out.push((start, end - start, first));
                open = None;
            }
        }
        if breaks {
            continue;
        }
        open = Some(match open {
            Some((start, _, _, first)) => (start, text_end, raw_end, first),
            None => (offset, text_end, raw_end, line),
        });
    }
    if let Some((start, end, _, first)) = open {
        out.push((start, end - start, first));
    }
    out
}

/// The candidate element lines of one span, with their byte offsets in it. A
/// SECTION span opens with the section's own heading line (offset 0): it is
/// the container, never one of its elements — the marker family's
/// `extract_members` skips it the same way. A whole-document span has no
/// container line.
fn element_lines(span: &str, sectioned: bool) -> impl Iterator<Item = (usize, &str)> {
    non_fenced_lines(span)
        .into_iter()
        .filter(move |(offset, _)| !sectioned || *offset != 0)
}

/// The byte offset in the file of `offset` in `span`, a subslice of `body`,
/// which starts at `body_offset`.
fn abs_offset(body_offset: usize, body: &str, span: &str, offset: usize) -> usize {
    let span_start = (span.as_ptr() as usize).saturating_sub(body.as_ptr() as usize);
    body_offset + span_start + offset
}

/// 1-based line numbers for ascending byte offsets, counting each newline of
/// the file once. Recounting from the top for every finding made a file with
/// many findings quadratic (cold review round 1: 80,000 failing list items
/// took 30 s). An offset behind the cursor restarts the count.
pub(crate) struct LineCursor<'a> {
    bytes: &'a [u8],
    at: usize,
    line: u32,
}

impl<'a> LineCursor<'a> {
    pub(crate) fn new(content: &'a str) -> Self {
        Self {
            bytes: content.as_bytes(),
            at: 0,
            line: 1,
        }
    }

    pub(crate) fn line_of(&mut self, abs: usize) -> u32 {
        let abs = abs.min(self.bytes.len());
        if abs < self.at {
            self.at = 0;
            self.line = 1;
        }
        let newlines = self.bytes[self.at..abs]
            .iter()
            .filter(|b| **b == b'\n')
            .count();
        self.line += newlines as u32;
        self.at = abs;
        self.line
    }
}

/// A rule's pattern as quoted regions: the pattern, and — when it is tested
/// against the element's NAME — a `match_on` region saying so. Without it a
/// reader sees a pattern anchored on the name beside a quoted element that
/// opens with its marker, and concludes the pattern could never match.
fn pattern_regions(label: &str, matcher: &regex_lite::Regex, on: MatchOn) -> Vec<QuotedRegion> {
    let mut regions = vec![quoted(label, matcher.as_str())];
    if on == MatchOn::Name {
        regions.push(quoted("match_on", "name"));
    }
    regions
}

fn quoted(label: &str, content: &str) -> QuotedRegion {
    QuotedRegion {
        platform_variant: false,
        label: label.into(),
        content: content.into(),
    }
}

/// A finding's quoted regions: the rule's section (when it names one) first,
/// then `rest`.
fn with_section(section: &Option<String>, rest: Vec<QuotedRegion>) -> Vec<QuotedRegion> {
    let mut out: Vec<QuotedRegion> = section.iter().map(|s| quoted("section", s)).collect();
    out.extend(rest);
    out
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
/// if the section is absent or the rule names none.
fn section_line(content: &str, body_offset: usize, section: Option<&str>) -> u32 {
    if let Some((want_level, want_text)) = section.and_then(atx_heading) {
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

    fn budget(yaml: &str) -> Result<Rule, Error> {
        compile_rule(crate::yaml::from_str::<RawRule>(yaml).unwrap())
    }

    // RED GATE (#253, GH #79 item 8): a section's span — heading through its
    // subsections — over `max_bytes` is E0125 at its heading; each paragraph
    // over a `per: paragraph` budget is E0126 at its first line. Fences,
    // headings and blank lines end a paragraph.
    #[test]
    fn budget_rules_bound_sections_and_paragraphs() {
        let body = "# Doc\n\n## Intro\nshort.\n\n## Long\naaaaaaaaaa bbbbbbbbbb\n\
                    cccccccccc dddddddddd\n\n```\nnot a paragraph line at all, long long\n```\n\
                    tail\n### Sub\nsub text\n";
        let check = |rule: Rule| {
            let mut f = Vec::new();
            check_file(&[&rule], Path::new("d.md"), body, 0, &mut f);
            f
        };
        let f = check(budget("section: \"## Long\"\nmax_bytes: 40\n").unwrap());
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(
            (f[0].code.as_str(), f[0].location.line),
            ("MDATRON-E0125", 6)
        );
        assert!(check(budget("section: \"## Intro\"\nmax_bytes: 40\n").unwrap()).is_empty());

        let f = check(budget("max_bytes: 30\nper: paragraph\n").unwrap());
        let hits: Vec<(&str, u32)> = f
            .iter()
            .map(|x| (x.code.as_str(), x.location.line))
            .collect();
        // Only the two-line paragraph (43 bytes) is over 30; the fenced line,
        // the headings and the short paragraphs are not.
        assert_eq!(hits, vec![("MDATRON-E0126", 7)], "{f:?}");
        assert!(f[0]
            .quoted
            .iter()
            .any(|q| q.label == "first line" && q.content.starts_with("aaaa")));

        // CRLF (#253 review): lines are still one paragraph, and the inner
        // `\r\n` counts — 21 + 2 + 10 = 33 bytes.
        let crlf = "aaaaaaaaaa bbbbbbbbbb\r\ncccccccccc\r\n\r\nshort\r\n";
        let mut f = Vec::new();
        let rule = budget("max_bytes: 32\nper: paragraph\n").unwrap();
        check_file(&[&rule], Path::new("c.md"), crlf, 0, &mut f);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].message.contains("33 bytes"), "{:?}", f[0].message);
        let rule = budget("max_bytes: 33\nper: paragraph\n").unwrap();
        let mut f = Vec::new();
        check_file(&[&rule], Path::new("c.md"), crlf, 0, &mut f);
        assert!(f.is_empty(), "{f:?}");

        // A missing section is E0122, as for every other shape.
        let f = check(budget("section: \"## Gone\"\nmax_bytes: 10\n").unwrap());
        assert_eq!(f[0].code, "MDATRON-E0122");
    }

    #[test]
    fn budget_rules_are_validated_at_load() {
        for bad in [
            "section: \"## A\"\nmax_bytes: 0\n",
            "max_bytes: 100\n",
            "per: paragraph\nsection: \"## A\"\n",
            "section: \"## A\"\nmax_bytes: 10\nelement: line\n",
            "section: \"## A\"\nmax_bytes: 10\ncount: \">= 1\"\n",
        ] {
            assert!(budget(bad).is_err(), "{bad:?}");
        }
        assert!(budget("max_bytes: 100\nper: paragraph\n").is_ok());
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
    // the same variants as aliases (DESIGN.md § Input-field renames are aliased and ledgered).
    #[test]
    fn element_aliases_parse_to_the_unified_class() {
        let parse = |y: &str| crate::yaml::from_str::<RawOperand>(y).unwrap().element;
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
            crate::yaml::from_str("section: '## A'\nelement: heading\nmatch: .\ncount: '>= 1'")
                .unwrap();
        assert_eq!(rule.element, Some(ElementClass::Heading));
        assert!(
            crate::yaml::from_str::<RawOperand>("section: '## A'\nelement: h7\nid_pattern: x")
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
            count_matching_elements(span, true, ElementClass::Heading, &any, MatchOn::Line),
            2,
            "`heading` counts every level inside the span, not the section's own heading"
        );
        assert_eq!(
            count_matching_elements(span, true, ElementClass::H2, &any, MatchOn::Line),
            0
        );
        assert_eq!(
            count_matching_elements(span, true, ElementClass::H4, &any, MatchOn::Line),
            1
        );
        assert_eq!(
            count_matching_elements(
                span,
                true,
                ElementClass::ListItemBoldName,
                &any,
                MatchOn::Line
            ),
            2,
            "bold-lead bullets only, never the plain bullet"
        );
        assert_eq!(
            count_matching_elements(
                span,
                true,
                ElementClass::ListItemBoldName,
                &rx("Item x"),
                MatchOn::Line
            ),
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
            count_matching_elements(span, true, ElementClass::H3, &m, MatchOn::Line),
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
            section: Some("## Open questions".into()),
            element: ElementClass::H3,
            matcher: rx(r"^### Q\d+"),
            on: MatchOn::Line,
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
            match_on: None,

            every: None,

            order: None,
            section: Some("##".into()),
            element: Some(ElementClass::H3),
            match_pattern: Some(r"^### .*$".into()),
            count: Some(">= 1".into()),
            disjoint: None,
            max_bytes: None,
            per: None,
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
            match_on: None,

            every: None,

            order: None,
            section: Some("Requirements".into()),
            element: Some(ElementClass::H3),
            match_pattern: Some(r"^### .*$".into()),
            count: Some(">= 1".into()),
            disjoint: None,
            max_bytes: None,
            per: None,
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
            match_on: None,

            every: None,

            order: None,
            max_bytes: None,
            per: None,
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
            section: Some("## Requirements".into()),
            element: ElementClass::H3,
            matcher: rx(r"^### .*$"),
            on: MatchOn::Line,
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
            section: Some("## Requirements".into()),
            element: ElementClass::H3,
            matcher: rx(r"^### .*$"),
            on: MatchOn::Line,
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

    // #201 ruling: `match_on` is additive. Absent keeps the whole-line meaning
    // every 0.6.0 rule has; `name` tests the element's name (heading text or
    // list item's bold name), the same text a disjoint `id_pattern` sees.
    #[test]
    fn match_on_selects_line_or_name_and_defaults_to_line() {
        let rule =
            |yaml: &str| compile_rule(crate::yaml::from_str::<RawRule>(yaml).unwrap()).unwrap();
        let body = "## Members\n### REQ-1\n- **REQ-2**: x\n### Other\n";
        let count = |r: &Rule| match r {
            Rule::Count {
                section,
                element,
                matcher,
                on,
                ..
            } => rule_spans(body, section.as_deref())
                .unwrap()
                .iter()
                .map(|s| count_matching_elements(s, true, *element, matcher, *on))
                .sum::<usize>(),
            _ => panic!("a count rule was compiled"),
        };
        let base =
            "section: '## Members'\nelement: heading\nmatch: '^REQ-[0-9]+$'\ncount: '>= 1'\n";
        // Default (line): the anchored id regex never matches `### REQ-1`.
        assert_eq!(count(&rule(base)), 0);
        assert_eq!(count(&rule(&format!("{base}match_on: line\n"))), 0);
        // By name: it does.
        assert_eq!(count(&rule(&format!("{base}match_on: name\n"))), 1);
        // Name for the bold-lead class too.
        let lead = "section: '## Members'\nelement: list-item-bold-name\nmatch: '^REQ-[0-9]+$'\ncount: '>= 1'\nmatch_on: name\n";
        assert_eq!(count(&rule(lead)), 1);
        // A line regex keeps working under the default.
        let line = "section: '## Members'\nelement: h3\nmatch: '^### REQ-'\ncount: '>= 1'\n";
        assert_eq!(count(&rule(line)), 1);
        // An unknown value and a disjoint rule carrying it are refused.
        assert!(crate::yaml::from_str::<RawRule>(&format!("{base}match_on: heading\n")).is_err());
        let disjoint = "match_on: name\ndisjoint:\n  - {section: '## A', element: h3, id_pattern: '(x)'}\n  - {section: '## B', element: h3, id_pattern: '(x)'}\n";
        assert!(compile_rule(crate::yaml::from_str::<RawRule>(disjoint).unwrap()).is_err());
    }

    fn compiled(yaml: &str) -> Rule {
        compile_rule(crate::yaml::from_str::<RawRule>(yaml).unwrap()).unwrap()
    }

    fn refused(yaml: &str) -> bool {
        crate::yaml::from_str::<RawRule>(yaml)
            .map_err(|e| e.to_string())
            .and_then(|r| compile_rule(r).map(|_| ()).map_err(|e| e.to_string()))
            .is_err()
    }

    fn run(rule: &Rule, content: &str, body_offset: usize) -> Vec<Finding> {
        let mut findings = Vec::new();
        check_file(
            &[rule],
            Path::new("d.md"),
            content,
            body_offset,
            &mut findings,
        );
        findings
    }

    fn quote<'a>(f: &'a Finding, label: &str) -> Option<&'a str> {
        f.quoted
            .iter()
            .find(|q| q.label == label)
            .map(|q| q.content.as_str())
    }

    // #219: two count rules on one section used to produce indistinguishable
    // findings; each now quotes its own `match`, on E0120 and on E0122.
    #[test]
    fn count_findings_quote_the_rules_match_pattern() {
        let build = compiled(
            "section: '# Acme'\nelement: h2\nmatch: '^Build$'\nmatch_on: name\ncount: '== 1'\n",
        );
        let security = compiled(
            "section: '# Acme'\nelement: h2\nmatch: '^Security$'\nmatch_on: name\ncount: '== 1'\n",
        );
        let mut findings = Vec::new();
        check_file(
            &[&build, &security],
            Path::new("d.md"),
            "# Acme\n\n## Build\n",
            0,
            &mut findings,
        );
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, "MDATRON-E0120");
        assert_eq!(quote(&findings[0], "section"), Some("# Acme"));
        assert_eq!(quote(&findings[0], "match"), Some("^Security$"));
        let absent = run(&security, "# Other\n", 0);
        assert_eq!(absent[0].code, "MDATRON-E0122");
        assert_eq!(quote(&absent[0], "match"), Some("^Security$"));
    }

    // #217/#218: `section` is optional on a count rule. Absent, the rule runs
    // over the whole body — line 1 included, since there is no container
    // heading to skip — and never reports E0122.
    #[test]
    fn a_count_rule_without_a_section_covers_the_whole_document() {
        let not_empty = compiled("element: line\nmatch: '.'\ncount: '>= 1'\n");
        for empty in ["", "\n\n  \n", "```\nonly code\n```\n"] {
            let f = run(&not_empty, empty, 0);
            assert_eq!(f.len(), 1, "{empty:?}: {f:?}");
            assert_eq!(f[0].code, "MDATRON-E0120");
            assert!(f[0].message.starts_with("the document has 0 matching line"));
            assert_eq!(quote(&f[0], "section"), None);
            assert_eq!(f[0].location.line, 1);
        }
        assert!(run(&not_empty, "one line, no heading", 0).is_empty());
        // The body starts after the frontmatter: frontmatter lines are not content.
        let fm = "---\nname: x\n---\n";
        assert_eq!(run(&not_empty, fm, fm.len()).len(), 1);
        let imports = compiled("element: line\nmatch: '^@AGENTS\\.md$'\ncount: '>= 1'\n");
        assert!(run(&imports, "@AGENTS.md\n\nMore.\n", 0).is_empty());
        assert_eq!(run(&imports, "See AGENTS.md.\n", 0).len(), 1);
        // A reference inside a code fence is an example, not an import.
        assert_eq!(run(&imports, "```\n@AGENTS.md\n```\n", 0).len(), 1);
    }

    // #213: every element of the class in scope must match; each one that does
    // not is its own E0123 at its own line.
    #[test]
    fn every_rule_reports_each_element_that_does_not_match() {
        let rule = compiled(
            "section: '## Docs'\nelement: list-item\nevery: '^\\[[^\\]]+\\]\\([^)]+\\)(: .+)?$'\nmatch_on: name\n",
        );
        let body = "# Acme\n\n## Docs\n\n- [Start](a.md): first steps\n- plain text\n* [Ref](b.md)\n1. see c.md\n\n## Other\n\n- not checked\n";
        let f = run(&rule, body, 0);
        assert_eq!(
            f.iter()
                .map(|f| (f.code.as_str(), f.location.line))
                .collect::<Vec<_>>(),
            vec![("MDATRON-E0123", 6), ("MDATRON-E0123", 8)],
            "{f:?}"
        );
        assert_eq!(quote(&f[0], "element"), Some("- plain text"));
        assert_eq!(quote(&f[0], "section"), Some("## Docs"));
        assert!(quote(&f[0], "every").is_some());
        assert!(
            !f[0].message.contains("plain"),
            "governed text rides quoted, not in the message"
        );
        // Vacuous truth: a section with no element of the class is clean
        // (presence is a count rule's job); an absent section is E0122.
        assert!(run(&rule, "## Docs\n\nprose only\n", 0).is_empty());
        assert_eq!(run(&rule, "## Other\n", 0)[0].code, "MDATRON-E0122");
        // Lines are located in the file, not the body.
        let fm = "---\na: 1\n---\n";
        let f = run(&rule, &format!("{fm}## Docs\n- bad\n"), fm.len());
        assert_eq!(f[0].location.line, 5);
        // Whole-document form.
        let all = compiled("element: blockquote\nevery: '^Note: '\nmatch_on: name\n");
        let f = run(&all, "> Note: fine\n\n> stray\n", 0);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].location.line, 3);
    }

    // #214: an element matching an earlier order item must not follow one
    // matching a later item. Order is sequence only: a missing step is fine.
    #[test]
    fn order_rule_reports_an_element_after_a_later_step() {
        let rule = compiled(
            "section: '# Acme'\norder:\n  - {element: blockquote, match: '.'}\n  - {element: h2, match: '.'}\n",
        );
        assert!(run(
            &rule,
            "# Acme\n\n> Summary.\n\nDetails.\n\n## Docs\n\n- item\n\n## More\n",
            0
        )
        .is_empty());
        // A missing step is not a violation.
        assert!(run(&rule, "# Acme\n\n## Docs\n", 0).is_empty());
        // The summary after the first file list.
        let f = run(&rule, "# Acme\n\n## Docs\n\n> Summary.\n", 0);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].code, "MDATRON-E0124");
        assert_eq!(f[0].location.line, 5);
        assert_eq!(quote(&f[0], "element"), Some("> Summary."));
        // The later item is named by class and pattern: a bare `.` says nothing.
        assert_eq!(quote(&f[0], "must precede"), Some("h2 matching ."));
        // Each span of a duplicated heading is its own sequence.
        let h2 = compiled(
            "section: '## A'\norder:\n  - {element: h3, match: '^One$', match_on: name}\n  - {element: h3, match: '^Two$', match_on: name}\n",
        );
        assert!(run(&h2, "## A\n### One\n### Two\n## A\n### One\n", 0).is_empty());
        assert_eq!(run(&h2, "## A\n### Two\n### One\n", 0).len(), 1);
        // An absent section: the items tell two order rules on it apart (#219's
        // defect, not repeated for the new shape).
        let absent = run(&h2, "## B\n", 0);
        assert_eq!(absent[0].code, "MDATRON-E0122");
        assert_eq!(
            quote(&absent[0], "order"),
            Some("h3 matching ^One$ (on its name), then h3 matching ^Two$ (on its name)")
        );
    }

    // The order state machine, pinned: the latest step reached does NOT move
    // back on a violation, so every element behind it is reported; and an
    // element belongs to the FIRST item it matches.
    #[test]
    fn order_rule_keeps_the_latest_step_and_assigns_first_match() {
        let abc = compiled(
            "order:\n  - {element: h3, match: '^A$', match_on: name}\n  - {element: h3, match: '^B$', match_on: name}\n  - {element: h3, match: '^C$', match_on: name}\n",
        );
        // C, A, B: both A and B sit behind C. A mutant that lowered the
        // reached step to A on the first violation would miss B.
        let f = run(&abc, "### C\n### A\n### B\n", 0);
        assert_eq!(
            f.iter().map(|f| f.location.line).collect::<Vec<_>>(),
            vec![2, 3],
            "{f:?}"
        );
        assert!(f
            .iter()
            .all(|f| quote(f, "must precede") == Some("h3 matching ^C$ (on its name)")));
        // A, C, B, C: only B is out of place.
        let f = run(&abc, "### A\n### C\n### B\n### C\n", 0);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].location.line, 3);
        // First match: `### Docs` matches both items and belongs to the first,
        // so it is never the later step and nothing is out of order.
        let overlap = compiled(
            "order:\n  - {element: heading, match: '.'}\n  - {element: h3, match: 'Docs'}\n",
        );
        assert!(run(&overlap, "### Docs\n# Title\n", 0).is_empty());
    }

    // Cold review round 1 (IMPL-1/ADV-1): line numbers come from one pass over
    // the file per rule. Every finding of a long run is located correctly —
    // the first, one in the middle, the last — including in a second span.
    #[test]
    fn many_findings_are_each_located_on_their_own_line() {
        let rule = compiled("section: '## L'\nelement: list-item\nevery: '^- ok$'\n");
        let mut body = String::from("---\na: 1\n---\n## L\n");
        for i in 0..5000 {
            body.push_str(&format!("- bad {i}\n"));
        }
        body.push_str("## Other\n- not checked\n## L\n- bad again\n");
        let f = run(&rule, &body, "---\na: 1\n---\n".len());
        assert_eq!(f.len(), 5001);
        assert_eq!(f[0].location.line, 5);
        assert_eq!(f[2500].location.line, 2505);
        assert_eq!(f[4999].location.line, 5004);
        assert_eq!(f[5000].location.line, 5008);
        assert_eq!(quote(&f[5000], "element"), Some("- bad again"));
    }

    // Cold review round 2 (R2E-4): the cursor itself. Ascending offsets count
    // each newline once; an offset behind the cursor restarts, never
    // underflows; an offset past the end clamps.
    #[test]
    fn line_cursor_counts_forward_and_restarts_on_a_backward_offset() {
        let text = "a\nb\nc\n";
        let mut cursor = LineCursor::new(text);
        assert_eq!(cursor.line_of(0), 1);
        assert_eq!(cursor.line_of(2), 2);
        assert_eq!(cursor.line_of(2), 2);
        assert_eq!(cursor.line_of(4), 3);
        assert_eq!(cursor.line_of(0), 1, "a backward offset restarts the count");
        assert_eq!(cursor.line_of(3), 2);
        assert_eq!(
            cursor.line_of(999),
            4,
            "past the end clamps to the last line"
        );
        // The cursor only ever advances by the gap: after locating the last
        // line of a large text, locating it again scans nothing more.
        let big = "x\n".repeat(100_000);
        let mut cursor = LineCursor::new(&big);
        assert_eq!(cursor.line_of(big.len()), 100_001);
        assert_eq!(cursor.at, big.len());
    }

    // Cold review round 2 (R2D-7, R2E-6): a finding says how its pattern was
    // tested, and an every rule's section-not-found names the class.
    #[test]
    fn findings_say_when_the_pattern_is_tested_on_the_name() {
        let by_name =
            compiled("section: '## L'\nelement: list-item\nevery: '^ok$'\nmatch_on: name\n");
        let f = run(&by_name, "## L\n- bad\n", 0);
        assert_eq!(quote(&f[0], "match_on"), Some("name"));
        let absent = run(&by_name, "## Other\n", 0);
        assert_eq!(quote(&absent[0], "element class"), Some("list-item"));
        assert_eq!(quote(&absent[0], "match_on"), Some("name"));
        let by_line = compiled("section: '## L'\nelement: list-item\nevery: '^- ok$'\n");
        let f = run(&by_line, "## L\n- bad\n", 0);
        assert_eq!(quote(&f[0], "match_on"), None);
        let count =
            compiled("section: '## L'\nelement: h3\nmatch: '^A$'\nmatch_on: name\ncount: '== 1'\n");
        let f = run(&count, "## L\n", 0);
        assert_eq!(quote(&f[0], "match_on"), Some("name"));
    }

    // Cold review round 1 (ADV-5): a leading byte-order mark is not content.
    // With it glued to line 1, `# Acme` matched no section spec and a required
    // first line never matched.
    #[test]
    fn a_leading_byte_order_mark_is_not_part_of_the_first_line() {
        let h1 = compiled("section: '# Acme'\nelement: h2\nmatch: '.'\ncount: '>= 1'\n");
        assert!(run(&h1, "\u{feff}# Acme\n## Docs\n", 0).is_empty());
        let import = compiled("element: line\nmatch: '^@AGENTS\\.md$'\ncount: '>= 1'\n");
        assert!(run(&import, "\u{feff}@AGENTS.md\n", 0).is_empty());
        // A file holding only the mark is empty.
        let not_empty = compiled("element: line\nmatch: '.'\ncount: '>= 1'\n");
        assert_eq!(run(&not_empty, "\u{feff}", 0).len(), 1);
        // Line numbers are unchanged by the mark.
        let every = compiled("element: list-item\nevery: '^- ok$'\n");
        assert_eq!(run(&every, "\u{feff}- ok\n- bad\n", 0)[0].location.line, 2);
    }

    // The new shapes are refused when malformed, never loaded as a no-op.
    #[test]
    fn malformed_every_and_order_rules_are_refused_at_load() {
        // every: needs element; excludes match/count.
        assert!(refused("section: '## A'\nevery: x\n"));
        assert!(refused(
            "section: '## A'\nelement: h3\nevery: x\nmatch: y\n"
        ));
        assert!(refused(
            "section: '## A'\nelement: h3\nevery: x\ncount: '>= 1'\n"
        ));
        assert!(refused("section: '## A'\nelement: h3\nevery: '('\n"));
        // order: at least two items; nothing but section beside it.
        assert!(refused("order:\n  - {element: h2, match: x}\n"));
        assert!(refused(
            "element: h2\norder:\n  - {element: h2, match: x}\n  - {element: h2, match: y}\n"
        ));
        assert!(refused(
            "order:\n  - {element: h2, match: '('}\n  - {element: h2, match: y}\n"
        ));
        // A given section must still be a heading spec; a count rule still
        // needs element, match and count.
        assert!(refused(
            "section: Docs\nelement: line\nmatch: x\ncount: '>= 1'\n"
        ));
        assert!(refused("section: Docs\nelement: line\nevery: x\n"));
        assert!(refused("element: line\ncount: '>= 1'\n"));
        // Cold review round 1 (IMPL-4/ADV-13): rules that could never report.
        // An empty `every` pattern matches everything; a repeated order item
        // is unreachable under first-match; a multi-line section spec equals
        // no heading line.
        assert!(refused("element: line\nevery: ''\n"));
        assert!(refused(
            "order:\n  - {element: h2, match: x}\n  - {element: h2, match: x}\n"
        ));
        assert!(!refused(
            "order:\n  - {element: h2, match: x}\n  - {element: h2, match: x, match_on: name}\n"
        ));
        assert!(refused(
            "section: \"# Acme\\n## Docs\"\nelement: line\nmatch: x\ncount: '>= 1'\n"
        ));
        // Cold review round 2 (R2E-2/R2E-3): a count that cannot fail on the
        // whole document, one that cannot hold anywhere, and an order item
        // whose empty pattern takes every element of its class.
        assert!(refused("element: line\nmatch: x\ncount: '>= 0'\n"));
        assert!(!refused(
            "section: '## A'\nelement: line\nmatch: x\ncount: '>= 0'\n"
        ));
        assert!(refused(
            "section: '## A'\nelement: line\nmatch: x\ncount: '< 0'\n"
        ));
        assert!(!refused("element: line\nmatch: ''\ncount: '== 3'\n"));
        assert!(refused(
            "order:\n  - {element: line, match: ''}\n  - {element: h2, match: x}\n"
        ));
    }

    // The three line-based classes added for #213/#214/#217.
    #[test]
    fn list_item_blockquote_and_line_classes_name_their_elements() {
        use ElementClass::{Blockquote, Line, ListItem};
        assert_eq!(ListItem.name_in("- [a](b)"), Some("[a](b)"));
        assert_eq!(ListItem.name_in("  * nested"), Some("nested"));
        assert_eq!(ListItem.name_in("12. twelve"), Some("twelve"));
        assert_eq!(ListItem.name_in("3) three"), Some("three"));
        assert_eq!(ListItem.name_in("-no space"), None);
        assert_eq!(ListItem.name_in("**bold** text"), None);
        assert_eq!(ListItem.name_in("1234567890. too long"), None);
        // Cold review round 1 (IMPL-2/ADV-2/ADV-3). A tab after the marker
        // opens an item — unrecognised, a tab-separated item escaped every
        // `every` rule. A thematic break opens with a marker and is not one.
        assert_eq!(ListItem.name_in("-\t[b](x)"), Some("[b](x)"));
        assert_eq!(ListItem.name_in("1.\tone"), Some("one"));
        assert_eq!(ListItem.name_in("\t- tabbed in"), Some("tabbed in"));
        for rule in ["* * *", "- - -", "***", "---", "  * * * *", "-\t-\t-"] {
            assert_eq!(ListItem.name_in(rule), None, "{rule:?}");
        }
        assert_eq!(ListItem.name_in("- - two"), Some("- two"));
        assert_eq!(ListItem.name_in("+ + +"), Some("+ +"));
        // A bare marker and a no-break-space "indent" are not items.
        assert_eq!(ListItem.name_in("-"), None);
        assert_eq!(ListItem.name_in("\u{a0}- nbsp"), None);
        assert_eq!(ListItem.name_in("-é"), None);
        assert_eq!(Blockquote.name_in("> quoted"), Some("quoted"));
        assert_eq!(Blockquote.name_in("   >tight"), Some("tight"));
        assert_eq!(Blockquote.name_in("    > code"), None);
        assert_eq!(Line.name_in("anything"), Some("anything"));
        assert_eq!(Line.name_in("   "), None);
        assert_eq!(Line.name_in(""), None);
    }
}
