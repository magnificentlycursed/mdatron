# MDATRON-W0045 — schema-class-unvalidated

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.4.0

## What this means

A governed file declares a `schema_class` in its frontmatter, but that class
matches **no** JSON Schema under `.mdatron/schemas/` **and no** pattern rule's
`context`. Nothing validated the file: neither the schema family nor the rule
DSL acted on it. Without this warning the file would pass as clean, so a typo'd
or unregistered `schema_class` is silently unchecked — a document that claims a
type but is never held to it. A class validated by a schema *or* by any
matching rule stays silent.

Through 0.6.0 this warning was headlined `schema-class-unrouted`. It has
nothing to do with `routes.yaml` (the route family's `unrouted-file` is
`MDATRON-E0030`), so the headline now says what the warning means; its
fingerprints turned over once at 0.7.0.

## How to fix

Give the class something that validates it: add
`.mdatron/schemas/<class>.json` for frontmatter structure, or a pattern rule
with `context: <class>` for a cross-file rule. If the class is a typo, correct
the `schema_class` field to a registered one.

## Related codes

- MDATRON-E0030 — unrouted-file: the ROUTE family's "no route claims this file"
- MDATRON-W0047 — the schemas directory itself is missing (the project-level
  counterpart; the two never double-report one file)
