# MDATRON-W0051 — require-frontmatter-scope-matches-nothing

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A `require_frontmatter` glob in `.mdatron/config.yaml` matched none of the walked
files. `require_frontmatter` opts specific files *into* a hard requirement: a file
inside such a glob that carries no frontmatter is flagged (`MDATRON-W0040`), so
"missing frontmatter" cannot masquerade as "passed." A glob that matches nothing
therefore holds **nothing** to that rule — the requirement is silently disabled,
and `verify` exits 0 as if every file were compliant. A mistyped or stale scope
**fails open**: the very files you meant to guard go unchecked.

This is the fail-open counterpart to `MDATRON-W0046` (a dead `file_globs` entry)
and `MDATRON-W0043` (a dead `vocabulary_globs` scope): a scope glob that governs
nothing is announced, never silently tolerated. It is reported once per dead
pattern, and only on a whole-tree run — an incremental (`--changed`) run sees just
part of the tree, so "matched nothing" there is expected, not a defect.

## How to fix

Correct the glob so it covers the files that must carry frontmatter (check the
pattern against the paths under your project root), or remove it if the
requirement genuinely no longer applies to those files.
