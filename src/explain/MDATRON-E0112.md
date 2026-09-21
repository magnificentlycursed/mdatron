# MDATRON-E0112 — dead-marker-reference

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A body line matching a route's declared `marker_rules` pattern names a reference
whose captured `<name>` resolves to no element in the rule's target document.
Marker rules are the name-anchor sibling of `citations` (which covers
`file:line`) and `links` (which covers `[text](target#anchor)`): a line such as
`Provenance: <name>` must point at something that exists. Resolution is
**name-equality** — a trailing `.` on the target is tolerated, so
`- **Slice 1 — …the guardrail.**` resolves from `Provenance: Slice 1 — …the
guardrail`. The name is matched against the target document's elements of the
rule's `element` class — a markdown `heading`, or the leading `**bold**` name of
a `- ` list item (`list-item-bold-name`) — optionally scoped to a
`target_section` (that heading's span). The target document is named in the rule
config, project-root-relative and held to the confinement contract. Marker
checking is per-route opt-in.

A second shape of this finding: the pattern **matched a line but its
capture group captured no name** — an optional capture group
(`^Provenance:( .+)?$`) that did not participate in the match. Such a line
names nothing to resolve, which previously was a silent skip; it now reports
here, with the rule's pattern quoted. (A pattern with **no** capture group at
all is refused at route load.)

An **absent** target document also reports here: every reference into a missing
target is dead. A target that is **present but unverifiable** (non-UTF8,
opened-but-unreadable) is different — those references report
`MDATRON-W0048` (reference-target-unverified) instead, because the check was
skipped, not failed. One conflation is accepted residue: a target whose
*open itself* is refused (e.g. permission denied) is indistinguishable from an
absent one at capture time and reports as dead here — if this finding surprises
you, check the target's permissions as well as its existence.

## How to fix

- **The reference is a typo.** Correct the name to match the target element
  exactly (a trailing `.` aside).
- **The target moved or was renamed.** Update the marker line, or rename the
  element in the target document to match.
- **Wrong section.** If the rule sets `target_section`, the name must live inside
  that section's span — widen the section, or move the target element into it.
- **Wrong element class or target doc.** Confirm the rule's `element`
  (`heading` vs `list-item-bold-name`) and `target_doc` name the right place.
- **The capture group captured nothing.** If the finding says the pattern
  matched but captured no name, the pattern has an optional capture group —
  make it mandatory (`^Provenance: (.+)$`), or fix the line so the group
  participates.
