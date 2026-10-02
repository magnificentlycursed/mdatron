# MDATRON-E0123 — section-element-mismatch

**Severity:** error
**Status:** accepted
**Introduced in:** 0.7.0

## What this means

A section-structural **every** rule failed for this element. An every rule in
a route's `section_rules:` block names an `element` class and an `every`
pattern, and optionally a `section`; each element of that class in the
section's span (or in the whole document body — after the frontmatter and a
leading byte-order mark, fenced code excluded — when the rule names no
section) must match the pattern. This one does not. The pattern is tested against the
whole element line by default, or against the element's name (a list item's
text after its marker, a heading's text) with `match_on: name`. There is one
finding per element that fails, located at that element's line; the rule's
pattern and the element are quoted beneath the diagnostic. A `match_on: name`
region means the pattern was tested against the element's name, not the
quoted line: a pattern that opens with `^\[` is then not at odds with an
element that opens with `- `.

A count rule can only say how many elements match. An every rule says none
may differ — the shape of "every item in this list is a link".

Elements are lines: a list item is the line that opens it (a continuation
line is not read), and a line inside a fenced code block is not an element
(a fence indented four spaces or more, as inside a list item, is not
recognised as one). A section with no element of the
class passes; use a count rule to require that some exist.

## How to fix

- **The element is malformed.** Rewrite the quoted line so it matches the
  rule's `every` pattern.
- **The element does not belong here.** Move it out of the section the rule
  covers.
- **The pattern is too narrow.** Widen the route's `every` pattern if the
  element's shape is one the project accepts.

## Related codes

- MDATRON-E0120 — section-count-violation: how many elements match
- MDATRON-E0122 — section-not-found: the rule's section is absent
