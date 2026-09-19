# MDATRON-E0122 — section-not-found

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A section-structural rule (a `section_rules:` entry on a route) names a
`section` whose heading is not present in the governed document — so the span
the rule asserts over cannot be located, and the assertion cannot be evaluated.
This covers both rule shapes: a **count** rule whose `section` heading is
absent, and a **disjoint** rule where either operand's `section` heading is
absent (one finding per absent operand). The heading is matched by **level and
text** (`"## Requirements"` matches a `##` heading, not a `###` one), and a `#`
inside a fenced code block is not a heading. This finding fires when the
heading was renamed, mistyped, or its level changed.

Before this code existed the family failed **open**: an absent section counted
as 0 — so a count predicate satisfied by 0 passed silently — and a disjoint
operand's id set fell back to empty, and empty-vs-empty is trivially disjoint,
so a renamed heading made the rule pass forever. The absence is now loud (the
same posture as the pin family's `MDATRON-E0063`), and the count/disjointness
verdict is not issued at all.

## How to fix

- **The heading was renamed or its level changed.** Update the rule's `section`
  spec to the heading's current text and level, or restore the heading in the
  document.
- **Typo in the `section` spec.** It must be the full ATX heading line, e.g.
  `section: "## Requirements"`.
- **The section was retired deliberately.** Remove or retarget the section
  rule; a rule over a section that no longer exists asserts nothing.
