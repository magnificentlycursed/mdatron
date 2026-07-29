# MDATRON-W0043 — vocabulary-scope-matches-nothing

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.3.0

## What this means

A `.mdatron/vocabulary.yaml` registry is present — so the vocabulary family (the
naming register) is active — but the `vocabulary_globs` list in
`.mdatron/config.yaml` matches none of the walked files. The whole family
(`MDATRON-E0090`–`E0094`) therefore scans nothing and `verify` stays clean, so a
mistyped or stale glob is indistinguishable from "there was nothing to flag."
This mirrors the route family's coverage loudness: a scope that governs nothing
is announced, not silently tolerated.

`vocabulary_globs` is a scope list separate from `file_globs`; an *empty* (or
absent) list is not this warning — that deliberately falls back to scanning
every walked file.

## How to fix

Correct the `vocabulary_globs` so they cover the files the register should scan
(check the glob against the paths under your `file_globs`), or remove the list
entirely to apply the register to every walked file.
