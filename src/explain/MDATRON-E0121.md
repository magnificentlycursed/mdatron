# MDATRON-E0121 — section-ids-not-disjoint

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A section-structural **disjoint** rule found an id present in both of its two
sections. A disjoint rule in `.mdatron/section-rules.yaml` names two sections,
each with an `id_from` (`h3-heading` — the id comes from H3 heading text; or
`bullet-lead` — from the `**bold**` lead of a `- ` list item) and an
`id_pattern` (a regex whose first capture is the id); the engine extracts an id
set from each and asserts the two share no element. Live case (vsdd): a "slice"
is open **xor** complete — its id appears as an open-phase H3 in one section or
as a completed bullet in another, never both.

The extraction is **element-scoped, never a full-span text scan**: ids come only
from the declared element (H3 heading text, or a bullet's bold lead), so a
mention of an id in surrounding prose (e.g. a `Provenance:` line naming another
slice) does not enter the set and cannot cause a false overlap.

## How to fix

- **The same id is in both sections.** Move it to exactly one — an item that has
  moved from open to complete should be *removed* from the open section as it is
  *added* to the completed one, not left in both.
- **A miscopied id.** Correct the id (the captured `id_pattern` group) in one
  section so the two sets no longer collide.
