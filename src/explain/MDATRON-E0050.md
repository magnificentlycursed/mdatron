# MDATRON-E0050 — frontmatter-schema-violation

**Severity:** error
**Status:** accepted
**Introduced in:** 0.1.0

## What this means

The file's frontmatter parsed cleanly and bound to a known `schema_class`, but
its content violates one or more rules in the JSON Schema for that class. The
violation message names the specific failing field and the schema rule that
rejected it.

## How to fix

Read the violation message carefully — it names the field path (e.g.,
`/relevant_domains/2`) and the failing constraint (e.g., "value must match
pattern `^[a-z][a-z0-9-]*$`"). Three common shapes:

- **Required field missing.** Add the field to the frontmatter.
- **Field type wrong** (e.g., a string where the schema expects an array). Fix
  the value's YAML shape.
- **`additionalProperties: false` rejected an unknown field.** Either remove
  the extra field, or — if the field is genuinely needed — register it in the
  schema at `.mdatron/schemas/<class>.json`.

Run `mdatron verify` after the fix to confirm; schema violations frequently
cluster (one wrong required field cascades into several `additionalProperties`
violations) and the first fix may unlock the rest.

## See also

- [`DESIGN.md` § Five check families](../../DESIGN.md#five-check-families)
  — schema validation semantics + edge cases

## Related codes

- MDATRON-E0001 — frontmatter could not be parsed (fires before Layer 1)
- MDATRON-E0002 — `schema_class` references a class not in `.mdatron/schemas/`
