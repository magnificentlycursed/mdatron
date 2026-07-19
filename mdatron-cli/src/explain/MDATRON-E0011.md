# MDATRON-E0011 — key-source-parent-traversal

**Severity:** error
**Status:** accepted
**Introduced in:** 0.1.0

## What this means

A `keys:` declaration's `source` contains a parent-directory (`..`) segment.
Adopter-supplied paths must resolve inside the governed tree (`DESIGN.md`
§ Five check families, path confinement), and parent segments are rejected
outright — including interior segments that would resolve back inside the
root (`sub/../file.yaml`), and including glob patterns. The check is lexical:
it is decided on the path text component-wise, before any filesystem access,
so a traversal whose target does not exist is rejected exactly like one whose
target does.

## How to fix

Rewrite the `source` as a plain root-relative path with no `..` segments. If
the referenced data lives outside the project root, move it inside the
governed tree; the engine does not follow paths out of it.

## Related codes

- MDATRON-E0010 — an absolute source path, rejected on the same lexical basis
- MDATRON-E0012 — a symlink component refused under no-follow resolution
