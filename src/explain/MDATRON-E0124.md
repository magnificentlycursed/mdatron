# MDATRON-E0124 — section-order-violation

**Severity:** error
**Status:** accepted
**Introduced in:** 0.7.0

## What this means

A section-structural **order** rule failed for this element. An order rule in
a route's `section_rules:` block lists two or more items, each an `element`
class with a `match` pattern, and optionally a `section`. Reading the
section's span (or the whole document body — after the frontmatter and a
leading byte-order mark, fenced code excluded — when the rule names no section)
from top to bottom, an element matching an earlier item must not appear after
an element matching a later item. This element does. It is quoted beneath the
diagnostic as `element`; `must precede` names the later item that had already
appeared, as its element class and pattern (`h2 matching .` is "any H2"),
with `(on its name)` when that item's pattern is tested against the element's
name. The section's own heading line is not one of its elements.

An element belongs to the first item it matches. An item that nothing
matches is not a violation: an order rule asserts sequence, and a count rule
asserts presence. When the section's heading occurs more than once, each
occurrence is ordered on its own.

## How to fix

- **The element is out of place.** Move the quoted element above the elements
  it must precede.
- **The order in the rule is stale.** Reorder the route's `order` items to the
  sequence the project now wants.
- **An item's pattern catches too much.** Narrow the `match` of the item this
  element falls under, if it was never meant to be ordered.

## Related codes

- MDATRON-E0120 — section-count-violation: require that an ordered part exists
- MDATRON-E0122 — section-not-found: the rule's section is absent
