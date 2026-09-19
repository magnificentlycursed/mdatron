//! Marker-line reference family (#147; vsdd GH#20 P3 / GH#22): a body line
//! matching an adopter-declared pattern names a reference whose captured
//! `<name>` must resolve to an existing element in a rule-config-named target
//! document. The name-anchor sibling of the citation family (which covers
//! `file:line`) and the link family (which covers `[text](target#anchor)`).
//!
//! Data-less, per-route opt-in via `marker_rules` on a route. Each rule declares
//! `{ pattern, element, target_doc, target_section? }`:
//!
//! - `pattern` — a linear-time regex (`regex_lite`); its first capture group is
//!   the referenced `<name>`.
//! - `element` — the class the name resolves against: a markdown `heading`, or a
//!   `list-item-bold-name` (the leading `**bold**` of a `- ` list item — vsdd's
//!   live shape: `- **Slice 1 — …** …`, referenced by that leading name).
//! - `target_doc` — the document the reference resolves INTO, named in the rule
//!   config (not derived from `governed_by`, not carried per line — vsdd GH#22
//!   Q1). Held to the confinement contract like a citation path: project-root-
//!   relative, categorical `..`-refusal (`E0011`), absolute refused (`E0010`),
//!   symlinked component refused (`E0012`).
//! - `target_section?` — an optional heading whose span (until the next heading
//!   of the same or higher level) scopes resolution.
//!
//! Resolution is **name-equality** with a trailing `.` tolerated on the target
//! (vsdd GH#22 Q2) — deliberately NOT slug-based (the divergence from the link
//! anchor resolver). A reference that resolves to nothing is `MDATRON-E0112`
//! (dead-marker-reference). A `target_section` whose heading is never matched
//! in the target document is `MDATRON-E0114` (marker-target-section-not-found,
//! GH #48): one finding per (rule, governed file), and the rule's lines are
//! skipped for that file — never mass-flagged E0112 for a rule misconfig or a
//! renamed target heading.

use std::collections::HashSet;
use std::path::Path;

use crate::confine::{confine_lexically, LexicalViolation};
use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};
use crate::markup::{atx_heading, list_item_bold_name, non_fenced_lines};
use crate::route::{ElementClass, MarkerRule};
use crate::snapshot::{Captured, Snapshot};

/// Scan one opted-in file's body for marker-line references and resolve each
/// against its rule's target doc. `content` is the whole file; `body_offset` is
/// where the prose body begins. `rules` are the marker rules active for this
/// file (every rule on every route claiming it).
pub fn check_file(
    snapshot: &Snapshot,
    path: &Path,
    content: &str,
    body_offset: usize,
    rules: &[&MarkerRule],
    findings: &mut Vec<Finding>,
) {
    if rules.is_empty() {
        return;
    }
    let body = &content[body_offset..];

    // Resolve each rule's target member-set once. `None` = the rule is disabled
    // because its target_doc failed confinement or its target_section heading is
    // absent from the target (a finding was emitted); its matching lines are
    // then skipped rather than spuriously flagged E0112.
    let member_sets: Vec<Option<HashSet<String>>> = rules
        .iter()
        .map(|rule| resolve_members(snapshot, path, rule, findings))
        .collect();

    for (line_start, line) in non_fenced_lines(body) {
        for (rule, members) in rules.iter().zip(&member_sets) {
            let Some(members) = members else { continue };
            let Some(caps) = rule.pattern.captures(line) else {
                continue;
            };
            // The first capture group is the referenced name. Route load refuses
            // a pattern with NO capture group (GH #48 finding 2), but a
            // load-accepted OPTIONAL group (`^Provenance:( .+)?$`) can still
            // match a line without participating — that was a silent per-line
            // skip; it is now a loud E0112 (GH #48 round 2): the line matched a
            // marker pattern but names nothing to resolve.
            let Some(name_match) = caps.get(1) else {
                findings.push(marker_finding(
                    path,
                    content,
                    body_offset + line_start,
                    "MDATRON-E0112",
                    "dead-marker-reference",
                    "the marker pattern matched this line but its capture group \
                     captured no name, so the reference cannot be resolved; \
                     check the pattern for an optional capture group",
                    "pattern",
                    rule.pattern.as_str(),
                ));
                continue;
            };
            let name = name_match.as_str();
            if !members.contains(&normalize_name(name)) {
                findings.push(marker_finding(
                    path,
                    content,
                    body_offset + line_start,
                    "MDATRON-E0112",
                    "dead-marker-reference",
                    "this marker line names a reference that resolves to no element \
                     in the rule's target document (name-equality, a trailing `.` on \
                     the target tolerated)",
                    "marker",
                    name,
                ));
            }
        }
    }
}

/// A rule's target document from the captured snapshot (#103), scoped to
/// `target_section` if named, as the set of normalized member names for the
/// rule's element class. `None` means the target failed confinement or the
/// named `target_section` heading is absent from the target (a finding was
/// emitted — `E0114` for the latter, GH #48). A missing/unreadable target
/// yields an empty set, so its references surface loudly as `E0112` rather
/// than degrading silently.
fn resolve_members(
    snapshot: &Snapshot,
    path: &Path,
    rule: &MarkerRule,
    findings: &mut Vec<Finding>,
) -> Option<HashSet<String>> {
    let confined = match confine_lexically(Path::new(&rule.target_doc)) {
        Ok(c) => c,
        Err(v) => {
            let (code, summary) = match v {
                LexicalViolation::Absolute => ("MDATRON-E0010", "absolute-path-refused"),
                LexicalViolation::ParentSegment => ("MDATRON-E0011", "parent-segment-refused"),
            };
            findings.push(marker_finding(
                path,
                "",
                0,
                code,
                summary,
                "a marker rule's target_doc escapes the governed tree",
                "target_doc",
                &rule.target_doc,
            ));
            return None;
        }
    };

    // The target's capture-time state (#103): marker targets come from route
    // config, so discovery always captures a confined target_doc; a miss is an
    // engine defect and reports as one (the None arm), never a filesystem
    // fallback.
    let target: &str = match snapshot.get(confined.as_path()) {
        Some(Captured::Content(c)) => match c.text() {
            Some(text) => text,
            // Unreadable (non-UTF8) target: empty member set → references fail.
            None => return Some(HashSet::new()),
        },
        // Unreadable, or (defensively) over the size cap — config-scoped
        // discovery escalates TooLarge before the seam, but if one reaches
        // here the empty set keeps its references loud (E0112), not silent.
        Some(Captured::OpenedUnreadable { .. }) | Some(Captured::TooLarge { .. }) => {
            return Some(HashSet::new())
        }
        Some(Captured::SymlinkRefused { .. }) => {
            findings.push(marker_finding(
                path,
                "",
                0,
                "MDATRON-E0012",
                "symlinked-component-refused",
                "a marker rule's target_doc resolves through a symbolic link; \
                 no-follow resolution refuses it",
                "target_doc",
                &rule.target_doc,
            ));
            return None;
        }
        // Missing target: empty set → references surface as E0112 (loud, not silent).
        Some(Captured::OpenIo { .. }) => return Some(HashSet::new()),
        // Never captured: an ENGINE defect in target discovery — report it as
        // one and disable the rule rather than flag healthy references.
        None => {
            findings.push(marker_finding(
                path,
                "",
                0,
                "MDATRON-E0080",
                "pipeline-orchestration-failure",
                "this rule's target_doc was never captured into the run \
                 snapshot — an engine defect in target discovery, not a defect \
                 in this document; please report it upstream",
                "target_doc",
                &rule.target_doc,
            ));
            return None;
        }
    };

    // Strip any frontmatter so a YAML `# comment` in the target is not read as a
    // heading, then extract the element names (optionally section-scoped).
    let doc_body = match crate::frontmatter::parse(target) {
        Ok(Some((_, b))) => b,
        _ => target,
    };
    match extract_members(doc_body, rule.element, rule.target_section.as_deref()) {
        Some(members) => Some(members),
        // GH #48 finding 3: the named target_section heading is never matched in
        // the target's body (renamed, or a misconfigured spec). Previously the
        // member set stayed permanently empty and EVERY matching line in the
        // governed file was mass-flagged E0112, blaming healthy references. One
        // E0114 per (rule, governed file) instead, and the rule's lines are
        // skipped for this file.
        None => {
            findings.push(marker_finding(
                path,
                "",
                0,
                "MDATRON-E0114",
                "marker-target-section-not-found",
                "a marker rule's target_section names a heading that is not \
                 present in the rule's target document, so its references cannot \
                 be resolved; the rule is skipped for this file",
                "target_section",
                rule.target_section.as_deref().unwrap_or_default(),
            ));
            None
        }
    }
}

/// The normalized member names of `body` for `element`, optionally scoped to the
/// span of the heading named by `section` (until the next heading of the same or
/// higher level). Returns `None` when a section IS named but its heading is
/// never matched in `body` (GH #48 — loud absence, decided by the SAME matching
/// logic the member scan uses: level equality + [`normalize_name`] equality);
/// with no `section` it always returns `Some`.
fn extract_members(
    body: &str,
    element: ElementClass,
    section: Option<&str>,
) -> Option<HashSet<String>> {
    let mut members = HashSet::new();

    // Section gating: when a section is named, collect only between its heading
    // and the next heading of the same-or-higher level.
    let want = section.and_then(atx_heading);
    let mut in_section = section.is_none();
    let mut section_matched = section.is_none();

    for (_, line) in non_fenced_lines(body) {
        if let Some((level, text)) = atx_heading(line) {
            if let Some((want_lvl, want_text)) = want {
                // The wanted heading opens the section — or RE-opens it when an
                // adjacent duplicate is also the heading that would have closed
                // it (GH #48 lane-A round 3: the close arm must not swallow a
                // re-open, mirroring `markup::section_spans`' sequential arms —
                // else members under a back-to-back duplicate are hidden and
                // their references false-flag E0112).
                if level == want_lvl && normalize_name(text) == normalize_name(want_text) {
                    in_section = true;
                    section_matched = true;
                    continue; // the section header itself is not a member
                }
                if !in_section {
                    continue;
                }
                if level <= want_lvl {
                    in_section = false; // a same-or-higher heading ends the section
                    continue;
                }
            }
            if in_section && matches!(element, ElementClass::Heading) {
                members.insert(normalize_name(text));
            }
            continue;
        }
        if !in_section {
            continue;
        }
        if let ElementClass::ListItemBoldName = element {
            if let Some(name) = list_item_bold_name(line) {
                members.insert(normalize_name(name));
            }
        }
    }
    section_matched.then_some(members)
}

/// Normalize a name for equality: trim surrounding whitespace and tolerate a
/// trailing `.` on either side (the target's list-item bold names carry one; the
/// marker line usually omits it — vsdd GH#22 Q2).
fn normalize_name(s: &str) -> String {
    s.trim().trim_end_matches('.').trim_end().to_string()
}

#[allow(clippy::too_many_arguments)]
fn marker_finding(
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

    #[test]
    fn normalize_name_tolerates_trailing_period() {
        assert_eq!(normalize_name("Slice 1 — self gov."), "Slice 1 — self gov");
        assert_eq!(normalize_name("Slice 1 — self gov"), "Slice 1 — self gov");
        assert_eq!(normalize_name("  Name.  "), "Name");
    }

    // GH #48 lane-A round 3: a back-to-back duplicate heading both closes the
    // previous span and re-opens the next — members under the second occurrence
    // must resolve (pre-fix they were hidden, so their references false-flagged
    // E0112). Separated duplicates (`## A … ## B … ## A`) already merged; this
    // pins the adjacent case the close arm used to swallow.
    #[test]
    fn extract_members_merges_adjacent_duplicate_sections() {
        let body = "# T\n\n## A\n\n- **First.** x\n\n## A\n\n- **Second.** y\n";
        let members = extract_members(body, ElementClass::ListItemBoldName, Some("## A"))
            .expect("the section matches");
        assert!(
            members.contains("First") && members.contains("Second"),
            "members under an adjacent duplicate heading must resolve: {members:?}"
        );
    }

    #[test]
    fn extract_members_scopes_to_section() {
        let body = "# T\n\n## A\n\n- **In A.** x\n\n## B\n\n- **In B.** y\n";
        let in_a = extract_members(body, ElementClass::ListItemBoldName, Some("## A"))
            .expect("## A is present, the scan resolves");
        assert!(in_a.contains("In A"));
        assert!(
            !in_a.contains("In B"),
            "a member under ## B is out of section A"
        );
        let whole = extract_members(body, ElementClass::ListItemBoldName, None)
            .expect("no section named: always Some");
        assert!(whole.contains("In A") && whole.contains("In B"));
    }

    // RED GATE (GH #48 finding 3): a named target_section whose heading is never
    // matched in the body is `None` (→ E0114 upstream), decided by the SAME
    // matcher the member scan uses — level equality + normalize_name equality
    // (trailing `.` tolerated) — never by a different detector.
    #[test]
    fn extract_members_is_none_when_named_section_never_matches() {
        let body = "# T\n\n## Renamed\n\n- **In A.** x\n";
        assert!(
            extract_members(body, ElementClass::ListItemBoldName, Some("## A")).is_none(),
            "a renamed heading must not yield a silently-empty member set"
        );
        // Level mismatch is a non-match too: `### A` does not satisfy `## A`.
        let deeper = "# T\n\n### A\n\n- **In A.** x\n";
        assert!(extract_members(deeper, ElementClass::ListItemBoldName, Some("## A")).is_none());
        // normalize_name tolerance: a trailing `.` on the heading still matches.
        let dotted = "# T\n\n## A.\n\n- **In A.** x\n";
        assert!(
            extract_members(dotted, ElementClass::ListItemBoldName, Some("## A"))
                .expect("normalize_name equality matches the dotted heading")
                .contains("In A")
        );
    }
}
