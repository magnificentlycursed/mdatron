# MDATRON-E0002 — schema-class-unknown

**Severity:** error
**Status:** accepted
**Introduced in:** 0.1.0

## What this means

The file's frontmatter declares a `schema_class:` field whose value does not
match any schema registered in `.mdatron/schemas/`. mdatron cannot Layer 1
validate the file because the schema it would validate against does not exist.

## How to fix

Three corrective paths, listed in order of likelihood:

1. **You meant a different class.** Check `.mdatron/schemas/` for the available
   classes; correct the typo in the file's frontmatter.
2. **You need to register a new class.** Add a JSON Schema file at
   `.mdatron/schemas/<your-class>.json` — see the project's existing schemas
   for the shape, or DESIGN.md § Five check families.
3. **The class name is intentional but mdatron should ignore the file.** Move
   the file outside the configured `file_globs` in `.mdatron/config.yaml`, or
   remove the `schema_class:` line entirely (a file with no `schema_class`
   skips Layer 1; Layer 2 patterns still apply via path-glob selectors).

## See also

- [`DESIGN.md` § Five check families](../../DESIGN.md#five-check-families)
  — schema routing + `schema_class` frontmatter binding

## Related codes

- MDATRON-E0001 — frontmatter could not be parsed (fires first if the YAML is
  malformed)
- MDATRON-E0050 — schema_class is known but frontmatter violates the schema
