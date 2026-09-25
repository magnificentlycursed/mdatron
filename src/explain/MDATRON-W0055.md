# MDATRON-W0055 — code-catalog-scope-matches-nothing

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.7.0

## What this means

A `.mdatron/code-catalogs.yaml` is present — so the code-catalog family is
active — but the `code_catalog_globs` list in `.mdatron/config.yaml` matches
none of the walked files. The scan therefore reads nothing and `verify` stays
clean, so a mistyped or stale glob is indistinguishable from "no code token is
cited anywhere". This is the code-catalog counterpart of `MDATRON-W0043` (a
dead `vocabulary_globs` scope) and `W0046` (a dead `file_globs` entry): a scope
that governs nothing is announced, never silently tolerated. It is reported once,
at the config, and only on a whole-tree run — an incremental (`--changed`) run
sees just part of the tree. The envelope's `families.code_catalog` reports
`inert` for the same reason.

`code_catalog_globs` is a scope list separate from `file_globs`; an *empty* (or
absent) list is not this warning — that deliberately falls back to scanning
every walked file.

## How to fix

Correct the `code_catalog_globs` so they cover the files whose code tokens the
catalogs should resolve (check the glob against the paths under your
`file_globs`), or remove the list entirely to scan every walked file.

## Related codes

- MDATRON-E0113 — an adopter code token that resolves to no catalog entry
- MDATRON-W0043 — the vocabulary family's dead-scope warning
- MDATRON-W0046 — a `file_globs` entry that matches no file
