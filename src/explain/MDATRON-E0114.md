# MDATRON-E0114 — marker-target-section-not-found

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A marker rule (`marker_rules` on a route) sets a `target_section`, but that
heading is not present in the rule's target document — so the section that
scopes resolution cannot be located, and none of the rule's references can be
resolved. The engine emits the finding **once per run** per rule *key* — the
`(target document, target section, element class)` triple, shared by every rule
and route that names the same triple — located at the first governed file the
(sorted) walk encounters for it, and skips those rules' marker lines in every
file they govern. The heading is matched exactly
the way member resolution matches it: by **level and name-equality** (a
trailing `.` on the heading tolerated), and a `#` inside a fenced code block is
not a heading. When several headings match, members from **all** matching
spans resolve — a duplicate heading cannot hide members. This fires when the
heading was renamed in the target document, its level changed, or the spec was
mistyped.

Before this code existed the failure mode was worse than silent: a renamed or
unmatched `target_section` left the member set permanently empty, so **every**
matching marker line in the governed file was flagged `MDATRON-E0112`
(dead-marker-reference) — mass-blaming healthy references for a rule
misconfiguration. The absence is now reported once, as itself.

## How to fix

- **The heading was renamed or its level changed in the target document.**
  Update the rule's `target_section` to the heading's current text and level,
  or restore the heading.
- **Typo in the `target_section` spec.** It must be the full ATX heading line,
  e.g. `target_section: "## Requirements"`.
- **You meant to resolve against the whole document.** Remove the
  `target_section` field; resolution then scans every element of the rule's
  `element` class in the target.
