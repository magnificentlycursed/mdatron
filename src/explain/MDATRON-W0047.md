# MDATRON-W0047 — schema-dir-missing

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.3.0

## What this means

The `.mdatron/schemas/` directory does not exist, **and** a governed file declares
a `schema_class` that nothing serves — no schema (the directory is absent) and no
pattern rule context. The schema family (Layer 1, JSON Schema over frontmatter)
therefore validated that file against nothing, yet the run exits 0 as if its
frontmatter were conformant. That is a false-clean, and without this warning only
the `families.schema` activity field would hint at it.

`mdatron init` deploys `.mdatron/schemas/` as part of the skeleton, so an absent
directory is drift (a broken or incomplete install, or a deleted directory). When
a file expects Layer 1 (it declares a `schema_class`) and nothing can serve that
request, the missing directory is the structural root cause, reported here at the
project level.

This is the project-level companion to `MDATRON-W0045` (a per-file unrouted
`schema_class`). W0045 stays its hand when there is no validation infrastructure
at all — an empty `schemas/` **and** empty `patterns/` — to preserve the
"no adopter data → clean" property. W0047 covers exactly that gap for the case
that is *not* no-adopter-data: a file that declared a class and expected to be
validated, in a project whose schemas directory is entirely gone.

Deliberately quiet cases:

- A **Schematron-only** project — a `schema_class` selected by a pattern rule
  `context:` — is validated by Layer 2, so the absent `schemas/` is not a
  false-clean and does not warn.
- A file with **no `schema_class`** requests no Layer 1 validation, so a missing
  `schemas/` is nothing to it.
- A directory that is **present but empty** is a deliberate opt-out, not drift.
- When **both** `schemas/` and `patterns/` are absent the run fails outright with
  a pipeline error rather than this warning — there is nothing to validate with.

## How to fix

Run `mdatron init` to restore the `.mdatron/` skeleton and add
`.mdatron/schemas/<class>.json` for the declared class; or add a pattern rule with
`context: <class>` to validate it via Layer 2; or correct the `schema_class` on
the file if it is a typo.
