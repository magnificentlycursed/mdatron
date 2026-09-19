# MDATRON-E0110 — dead-link-target

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A markdown link or image in a link-checked artifact points at a relative path
that is missing — or could not be opened (e.g. the open was refused by
permissions) — in the working-tree snapshot the run captured; the finding
carries the underlying OS error in a quoted region. Links are found by a full CommonMark
parse, so **inline** `[t](d)`, **reference-style** `[t][ref]` (with a separate
`[ref]: path` definition), and **image** `![alt](src)` links are all resolved;
a destination inside an inline `` `code` `` span or a fenced code block is a
syntax example, not a live link, and is skipped structurally. Destinations are
**percent-decoded** before resolution, the way GitHub serves them:
`[doc](my%20doc.md)` resolves to `my doc.md` (and a `#caf%C3%A9` fragment
matches the "Café" heading). Decoding is all-or-nothing — a malformed `%`
escape or a non-UTF8 decode leaves the destination literal, never corrupting
a path. Link targets
resolve DOCUMENT-relative, the way GitHub and CommonMark resolve them: a target
is relative to the directory of the file the link lives in, not the project
root — `[api](api.md)` in `docs/guide.md` means `docs/api.md`, and
`[readme](../README.md)` means the repository README. A route may instead opt
into **root-relative** resolution with `link_root: true` (sibling of `links:`
in `.mdatron/routes.yaml`): a leading-slash destination like `/docs/x.md` then
resolves from the project root rather than being refused as absolute
(MDATRON-E0010) — the convention static-site corpora author. Confinement is
unchanged under `link_root`: a `..` climbing above the root is still refused
(MDATRON-E0011), a symlinked component still MDATRON-E0012. The working-tree
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
