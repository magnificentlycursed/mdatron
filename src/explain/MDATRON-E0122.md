# MDATRON-E0122 — section-not-found

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A section-structural rule (a `section_rules:` entry on a route) names a
`section`, but no heading in the governed document matches the rule's section
spec — so the span the rule asserts over cannot be located, and the assertion
cannot be evaluated. This covers both rule shapes: a **count** rule whose
`section` matches nothing, and a **disjoint** rule where either operand's
`section` matches nothing (one finding per unmatched operand). Matching is
**exact on level and text** (`"## Requirements"` matches a `##` heading whose
text is exactly `Requirements` — a near-miss like `Requirements.` does not
match), and a `#` inside a fenced code block is not a heading. This fires when
the heading was renamed, mistyped, or its level changed. When the spec **does**
match, the rule evaluates over **all** matching spans — a count sums across
every same-level/same-text occurrence and a disjoint operand's ids are the
union over them, so content under a duplicate heading cannot evade the gate.

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
- **Typo in the `section` spec.** It must be the full ATX heading line with the
  heading's exact text, e.g. `section: "## Requirements"`.
- **The section was retired deliberately.** Remove or retarget the section
  rule; a rule over a section that no longer exists asserts nothing.
