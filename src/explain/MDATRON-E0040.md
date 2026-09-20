# MDATRON-E0040 — unsupported-schema-dialect

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A frontmatter schema under `.mdatron/schemas/` declares a `$schema` dialect
other than JSON Schema draft 2020-12, and the load refuses it. mdatron
supports exactly one dialect: `https://json-schema.org/draft/2020-12/schema`
(with or without the trailing `#`); a schema with no `$schema` field is
compiled under that dialect by default.

Before this code existed the family failed **open** on this exact value: the
compiler's 2020-12 setting is only a default, the underlying engine honors an
embedded `$schema`, and in this build a schema declaring `draft-07` — the most
widely deployed dialect — compiled without error to a validator that enforced
**nothing**. Documents violating `enum` and `additionalProperties: false`
passed silently with exit 0. The refusal is deliberate doctrine
(as-written-first): silently revalidating a draft-07-authored schema under
2020-12 semantics would reinterpret the author's declared dialect, so an
unsupported declaration is refused loudly at load instead — the run fails
with a pipeline error before any document is judged against a schema that
might enforce nothing.

## How to fix

- **The schema is 2020-12 in all but the declaration.** Change `$schema` to
  `https://json-schema.org/draft/2020-12/schema`, or remove the field to use
  the enforced default. Most simple shape schemas (types, `properties`,
  `required`, `enum`, `additionalProperties`) are identical across dialects.
- **The schema genuinely uses an older dialect's semantics** (e.g. draft-04
  boolean `exclusiveMaximum`, or draft-07 `definitions`). Port it to 2020-12
  (`$defs`, numeric `exclusiveMaximum`) and declare the 2020-12 dialect.
- **The `$schema` value is a typo or not a string.** Correct it to the
  supported URI exactly.
