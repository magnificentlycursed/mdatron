# MDATRON-E0120 — section-count-violation

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A section-structural rule's **count** predicate failed. A count rule in
a route's `section_rules:` block names a `section` (a heading), an `element`
class (a heading level such as `h3`, a `list-item`, a `blockquote`, any
`line`), a `match` (a regex the element must match),
and a `count` predicate — one of `>=`, `<=`, `==`, `!=`, `>`, `<`, then an
integer (`>= 1`, `== 1`, `< 3`); the engine counts the
elements of that class inside that section's span (from its heading through
just before the next heading of the same or higher level, fence-aware) that
match, and asserts the predicate. The section's own heading line is the
container, not one of its elements, so it is never counted. A rule with no `section` counts over the
whole document body, which is how "this file is not empty" (`element: line`,
`match: "."`, `count: ">= 1"`) and "this file contains this line" are
written. The body starts after the frontmatter and a leading byte-order
mark, and excludes fenced code blocks. A `match_on: name` region beneath the
diagnostic means the pattern was tested against the element's name (a
heading's text, a list item's text), not its whole line. The rule's `match` pattern is quoted beneath the diagnostic, so two
rules on one section can be told apart. This is the body-content counterpart of
the DSL's frontmatter arity rule (`count(filter(...)) == N`), delivered as a
fixed-semantics check because body-content extraction is excluded from the rule
DSL. Typical case: "at least one open H3 in `## Requirements`" — an
empty section is the retire trigger, so the rule is `>= 1`, and this fires when
the count leaves the required range.

## How to fix

- **Too few (e.g. `>= 1` with 0).** The section is empty or its elements no
  longer match — add the expected heading, item or line, or retire/repurpose
  the section if it is genuinely done.
- **Too many (e.g. `== 1` with 2).** A duplicate or stray matching element is
  present; remove or move it.
- **The elements changed shape.** If a rename made them stop matching `match`,
  update the element's text or the rule's `match` pattern.
- **The file is empty or lost a required line.** For a rule with no section
  (the message says "the document has"), read the quoted `match`: a pattern
  of `.` means the file has no non-blank line outside its frontmatter and
  fenced code, so add content; any other pattern names a line the file must
  hold, so restore that line.
