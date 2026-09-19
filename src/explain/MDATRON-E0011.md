# MDATRON-E0011 — key-source-parent-traversal

**Severity:** error
**Status:** accepted
**Introduced in:** 0.1.0

## What this means

An adopter-supplied path carries parent-directory (`..`) traversal that the
engine refuses. This code fires from every surface that names a path — a
`keys:` declaration's `source`, a pin's `governing` or `file`, a route's
`files` glob or `governed_by`, a marker rule's `target_doc`, a citation path,
a configured `file_globs`/scope glob (a governed path), and a link or image
destination in a governed body — the quoted region (or the failing config
load) names which one.

Two `..` postures apply, depending on who authors the path:

- **Config-named paths** (every surface except body links) get the
  categorical refusal: any `..` segment is rejected outright — including an
  interior segment that would resolve back inside the root
  (`sub/../file.yaml`), and including glob patterns. The check is lexical:
  it is decided on the path text component-wise, before any filesystem
  access, so a traversal whose target does not exist is rejected exactly
  like one whose target does.
- **Body links** (semantics amended in 0.6.0): a link destination is
  percent-decoded and its `..` segments are collapsed document-relatively —
  `[readme](../README.md)` in `docs/guide.md` is the legitimate in-tree
  repository README, and is NOT refused. This code fires for a link only
  when the resolved path escapes ABOVE the project root (the `../` segments
  climb past it). So "rewrite the path root-relative with no `..`" is the
  wrong advice for a link — an in-tree `..` link is already fine.

## How to fix

For a config-named path, rewrite it as a plain root-relative path with no
`..` segments. If the referenced data lives outside the project root, move it
inside the governed tree; the engine does not follow paths out of it.

For a body link, count the `../` hops against the linking file's directory:
the link is refused only because it climbs above the project root. Point it
at a target inside the tree (or move the target in); an in-tree `../` link
needs no change.

## Related codes

- MDATRON-E0010 — an absolute path, refused on the config surfaces and for
  leading-slash links (where `link_root: true` is the sanctioned remedy)
- MDATRON-E0012 — a symlink component refused under no-follow resolution
- MDATRON-E0110 — a link whose in-tree target does not exist
