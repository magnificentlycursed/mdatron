# MDATRON-E0110 — dead-link-target

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A markdown link or image in a link-checked artifact points at a relative path
that does not exist in the working-tree snapshot the run captured. Links are found by a full CommonMark
parse, so **inline** `[t](d)`, **reference-style** `[t][ref]` (with a separate
`[ref]: path` definition), and **image** `![alt](src)` links are all resolved;
a destination inside an inline `` `code` `` span or a fenced code block is a
syntax example, not a live link, and is skipped structurally. Link targets
resolve DOCUMENT-relative, the way GitHub and CommonMark resolve them: a target
is relative to the directory of the file the link lives in, not the project
root — `[api](api.md)` in `docs/guide.md` means `docs/api.md`, and
`[readme](../README.md)` means the repository README. The working-tree
snapshot is authoritative: uncommitted files count as present, and no git
history is consulted. Link checking is per-route opt-in (`links: true` in
`.mdatron/routes.yaml`), so a route declares it on its own scope. External links
(any URL scheme, or a protocol-relative `//host`) are never resolved — the
engine does not reach the network.

## How to fix

- **The target moved or was renamed.** Update the link to its current path.
- **The path is wrong relative to this file.** Links resolve from the linking
  file's directory — a sibling is `name.md`, a file one directory up is
  `../name.md`.
- **The link is stale prose.** Correct or remove it.
- **The document is historical.** Its route should not opt into link checking;
  archival corpora are records, not living navigation.
