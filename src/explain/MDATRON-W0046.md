# MDATRON-W0046 — jurisdiction-glob-matches-nothing

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.3.0

## What this means

A `file_globs` entry in `.mdatron/config.yaml` matched none of the files under
the project root. That glob contributes nothing to the checked corpus, so
`verify` walks fewer files than the declared jurisdiction implies and still exits
0 — a mistyped or stale path is indistinguishable from "that slice was clean."
`file_globs` is the adopter-authored jurisdiction (a governed tree is never
guessed); a member that governs nothing is announced, not silently tolerated.

This is the file-level counterpart to `MDATRON-W0043` (a `vocabulary_globs` scope
that matches nothing): both make a dead glob loud so narrowing the corpus is a
visible decision rather than a silent shrink. It is reported once per dead
pattern, and only on a whole-tree run — an incremental (`--changed`) run sees
just part of the tree, so "matched nothing" there is expected, not a defect.

An empty markdown tree trips this on the default `**/*.md`: that is intentional —
running `verify` over a corpus of zero files is exactly the "checked nothing
looks like checked clean" case the warning exists to surface. Add the governed
files, or narrow `file_globs` to the paths that actually exist.

## How to fix

Correct the glob so it covers the files it should govern (check the pattern
against the paths under your project root), or remove it if the jurisdiction
genuinely no longer includes those files.
