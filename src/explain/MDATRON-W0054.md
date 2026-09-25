# MDATRON-W0054 — route-glob-matches-nothing

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.7.0

## What this means

A route's `files` glob claims none of the walked files. The route governs
nothing: its `governed_by` attests nothing, its `naming` grammar checks nothing,
and every family it opts in with (`citations`, `links`, `marker_rules`,
`section_rules`) is inert for the files it meant to claim. A mistyped or stale
glob therefore **fails open** — the files you meant to govern are either
unrouted (`MDATRON-E0030`, if no other route claims them) or governed by some
other route than the one you wrote. This is the route-table counterpart of
`MDATRON-W0046` (a dead `file_globs` entry) and `W0043` (a dead scope glob): a
glob that governs nothing is announced, never silently tolerated. It is
reported once per dead route, quoting the glob, and only on a whole-tree run
whose jurisdiction came from `config.yaml` — an incremental (`--changed`) run
sees just part of the tree, and an ad-hoc `--files` run replaces the
jurisdiction from the command line, so a route outside that glob is not dead.

## How to fix

Correct the glob so it claims the files it should govern (check it against the
paths under `file_globs` in `.mdatron/config.yaml`; route globs are
root-relative and `*` crosses `/`), or remove the route if those files are
gone. A route kept for files that do not exist yet should be added when they
do.

## Related codes

- MDATRON-E0030 — a walked file no route claims
- MDATRON-W0053 — a route table that declares no routes at all
- MDATRON-W0046 — a `file_globs` entry that matches no file
