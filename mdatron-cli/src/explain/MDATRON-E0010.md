# MDATRON-E0010 — key-source-absolute-path

**Severity:** error
**Status:** accepted
**Introduced in:** 0.1.0

## What this means

A `keys:` declaration's `source` is an absolute path. Adopter-supplied paths
resolve relative to the project root and must stay inside the governed tree
(`DESIGN.md` § Five check families, path confinement); absolute paths are
rejected outright at config load — even absolute spellings of files that are
inside the root — because confinement is decided on the path text, before any
filesystem access. The target's existence does not matter: a non-existent
absolute target is rejected on the same basis as an existing one.

## How to fix

Rewrite the `source` as a path relative to the project root. For example,
replace `/home/me/project/.mdatron/data/matrix.yaml` with
`.mdatron/data/matrix.yaml`. If the data genuinely lives outside the governed
tree, move it inside — the engine does not read outside the project root.

## Related codes

- MDATRON-E0011 — a relative source escaping the root via `..` segments
- MDATRON-E0012 — a symlink component refused under no-follow resolution
