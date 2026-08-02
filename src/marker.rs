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
//! (dead-marker-reference).

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

use crate::confine::{confine_lexically, open_confined, LexicalViolation, OpenViolation};
use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};
use crate::markup::{atx_heading, non_fenced_lines};
use crate::route::{ElementClass, MarkerRule};

/// Scan one opted-in file's body for marker-line references and resolve each
/// against its rule's target doc. `content` is the whole file; `body_offset` is
/// where the prose body begins. `rules` are the marker rules active for this
/// file (every rule on every route claiming it).
pub fn check_file(
    project_root: &Path,
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
    // because its target_doc failed confinement (a finding was emitted); its
    // matching lines are then skipped rather than spuriously flagged E0112.
    let member_sets: Vec<Option<HashSet<String>>> = rules
        .iter()
        .map(|rule| resolve_members(project_root, path, rule, findings))
        .collect();

    for (line_start, line) in non_fenced_lines(body) {
        for (rule, members) in rules.iter().zip(&member_sets) {
            let Some(members) = members else { continue };
            let Some(caps) = rule.pattern.captures(line) else {
                continue;
            };
            // The first capture group is the referenced name. A pattern with no
            // capture group cannot name a reference — skip it.
            let Some(name_match) = caps.get(1) else {
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

/// Read a rule's target document (confined, no-follow), scope it to
/// `target_section` if named, and return the set of normalized member names for
/// the rule's element class. `None` means the target failed confinement (a
/// finding was emitted). A missing/unreadable target yields an empty set, so its
/// references surface loudly as `E0112` rather than degrading silently.
fn resolve_members(
    project_root: &Path,
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

    let mut target = String::new();
    match open_confined(project_root, &confined) {
        Ok(mut handle) => {
            if handle.read_to_string(&mut target).is_err() {
                // Unreadable (non-UTF8) target: empty member set → references fail.
                return Some(HashSet::new());
            }
        }
        Err(OpenViolation::Symlink { .. }) => {
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
        Err(OpenViolation::Io(_)) => return Some(HashSet::new()),
    }

    // Strip any frontmatter so a YAML `# comment` in the target is not read as a
    // heading, then extract the element names (optionally section-scoped).
    let doc_body = match crate::frontmatter::parse(&target) {
        Ok(Some((_, b))) => b,
        _ => &target,
    };
    Some(extract_members(
        doc_body,
        rule.element,
        rule.target_section.as_deref(),
    ))
}

/// The normalized member names of `body` for `element`, optionally scoped to the
/// span of the heading named by `section` (until the next heading of the same or
/// higher level).
fn extract_members(body: &str, element: ElementClass, section: Option<&str>) -> HashSet<String> {
    let mut members = HashSet::new();

    // Section gating: when a section is named, collect only between its heading
    // and the next heading of the same-or-higher level.
    let want = section.and_then(atx_heading);
    let mut in_section = section.is_none();

    for (_, line) in non_fenced_lines(body) {
        if let Some((level, text)) = atx_heading(line) {
            if let Some((want_lvl, want_text)) = want {
                if !in_section {
                    if level == want_lvl && normalize_name(text) == normalize_name(want_text) {
                        in_section = true;
                    }
                    continue; // the section header itself is not a member
                } else if level <= want_lvl {
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
    members
}

/// The leading `**bold**` name of a `- ` (or `*`/`+`) list item, or `None`.
fn list_item_bold_name(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let rest = t
        .strip_prefix("- ")
        .or_else(|| t.strip_prefix("* "))
        .or_else(|| t.strip_prefix("+ "))?
        .trim_start();
    let after_open = rest.strip_prefix("**")?;
    let end = after_open.find("**")?;
    Some(&after_open[..end])
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
    fn list_item_bold_name_extracts_leading_bold() {
        assert_eq!(
            list_item_bold_name("- **Slice 1 — self gov.** detail"),
            Some("Slice 1 — self gov.")
        );
        assert_eq!(list_item_bold_name("- plain item"), None);
        assert_eq!(list_item_bold_name("not a list item"), None);
        assert_eq!(
            list_item_bold_name("  * **Indented.** x"),
            Some("Indented.")
        );
    }

    #[test]
    fn normalize_name_tolerates_trailing_period() {
        assert_eq!(normalize_name("Slice 1 — self gov."), "Slice 1 — self gov");
        assert_eq!(normalize_name("Slice 1 — self gov"), "Slice 1 — self gov");
        assert_eq!(normalize_name("  Name.  "), "Name");
    }

    #[test]
    fn extract_members_scopes_to_section() {
        let body = "# T\n\n## A\n\n- **In A.** x\n\n## B\n\n- **In B.** y\n";
        let in_a = extract_members(body, ElementClass::ListItemBoldName, Some("## A"));
        assert!(in_a.contains("In A"));
        assert!(
            !in_a.contains("In B"),
            "a member under ## B is out of section A"
        );
        let whole = extract_members(body, ElementClass::ListItemBoldName, None);
        assert!(whole.contains("In A") && whole.contains("In B"));
    }
}
