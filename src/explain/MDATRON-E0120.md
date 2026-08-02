# MDATRON-E0120 — section-count-violation

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A section-structural rule's **count** predicate failed. A count rule in
`.mdatron/section-rules.yaml` names a `section` (a heading), an `element`
(a heading level, e.g. `h3`), a `match` (a regex the heading line must match),
and a `count` predicate (`>= 1`, `== 1`, `< 3`, …); the engine counts the
`element`-level headings inside that section's span (from its heading through
just before the next heading of the same or higher level, fence-aware) whose
line matches, and asserts the predicate. This is the body-content counterpart of
the DSL's frontmatter arity rule (`count(filter(...)) == N`), delivered as a
fixed-semantics check because body-content extraction is excluded from the rule
DSL. Live case (vsdd): "at least one open-phase H3 in `## Requirements`" — an
empty section is the retire trigger, so the rule is `>= 1`, and this fires when
the count leaves the required range.

## How to fix

- **Too few (e.g. `>= 1` with 0).** The section is empty or its headings no
  longer match — add the expected heading(s), or retire/repurpose the section if
  it is genuinely done.
- **Too many (e.g. `== 1` with 2).** A duplicate or stray matching heading is
  present; remove or move it.
- **The headings changed shape.** If a rename made them stop matching `match`,
  update the heading text or the rule's `match` pattern.
