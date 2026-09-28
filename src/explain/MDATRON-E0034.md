# MDATRON-E0034 — route-schema-unserved

**Severity:** error
**Status:** accepted
**Introduced in:** 0.7.0

## What this means

A route in `.mdatron/routes.yaml` binds the files it claims to a schema
class through its `schema` key, but no `.mdatron/schemas/<class>.json`
exists and no pattern rule's `context` names the class. Every file the route
claims would be "validated" by nothing. The route's `files` glob and the
class are quoted beneath the diagnostic.

## How to fix

- **Add the schema.** Create `.mdatron/schemas/<class>.json` (JSON Schema
  draft 2020-12).
- **Or add a rule.** A pattern rule with `context: <class>` also serves the
  class.
- **Or correct the route's `schema`** if the class name is misspelled.

## Related codes

- MDATRON-E0030 — unrouted-file: the route family's closed-world check
- MDATRON-E0050 — frontmatter-schema-violation: what a bound schema reports
