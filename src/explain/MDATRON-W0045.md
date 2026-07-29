# MDATRON-W0045 — schema-class-unrouted

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.3.0

## What this means

A governed file declares a `schema_class` in its frontmatter, but that class
matches **no** JSON Schema under `.mdatron/schemas/` **and no** pattern rule's
`context`. Nothing validated the file: neither Layer 1 (structural) nor Layer 2
(rules) acted on it. Without this warning the file would pass as clean, so a
typo'd or unregistered `schema_class` is silently unchecked — a document that
claims a type but is never held to it. A class validated by a schema *or* by any
matching rule is routed and stays silent.

## How to fix

Route the class to something that validates it: add
`.mdatron/schemas/<class>.json` for Layer-1 structure, or a pattern rule with
`context: <class>` for Layer-2 checks. If the class is a typo, correct the
`schema_class` field to a registered one.
