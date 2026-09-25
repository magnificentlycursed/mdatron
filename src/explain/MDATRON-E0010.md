# MDATRON-E0010 — absolute-path-refused

**Severity:** error
**Status:** accepted
**Introduced in:** 0.1.0

## What this means

An adopter-supplied path is absolute. This code fires from every surface that
names a path — a `keys:` declaration's `source`, a pin's `governing` or
`file`, a route's `files` glob or `governed_by`, a marker rule's `target_doc`,
a citation path, a configured `file_globs`/scope glob (a governed path), and a
link or image destination in a governed body — the quoted region (or the
failing config load) names which one.

Two postures apply, depending on who authors the path:

- **Config-named paths** (every surface except body links) get the categorical
  refusal: they resolve relative to the project root and must stay inside the
  governed tree (`DESIGN.md` path confinement), so an absolute path is
  rejected outright at load — even an absolute spelling of a file that is
  inside the root — because confinement is decided on the path text, before
  any filesystem access. The target's existence does not matter: a
  non-existent absolute target is rejected on the same basis as an existing
  one.
- **Body links** (semantics amended in 0.6.0): a destination is
  percent-decoded and resolved document-relatively; this code refuses a
  destination that is absolute — a leading slash (`/docs/x.md`) or a
  drive/UNC prefix. A leading-slash "root-relative" link has a sanctioned
  remedy: set `link_root: true` on the link-checked route in
  `.mdatron/routes.yaml`, and the destination resolves from the project root
  instead of being refused — the static-site convention, opt-in per route. A
  drive/UNC prefix stays refused even under `link_root` on platforms whose
  paths carry such prefixes (Windows); on Unix a `C:\...` destination is not
  an absolute path at all — it resolves as an ordinary relative name and
  surfaces as a missing target (`MDATRON-E0110`) instead.

## How to fix

For a config-named path, rewrite it relative to the project root. For example,
replace `/home/me/project/.mdatron/data/matrix.yaml` with
`.mdatron/data/matrix.yaml`. If the data genuinely lives outside the governed
tree, move it inside — the engine does not read outside the project root.

For a leading-slash body link, either rewrite the destination
document-relatively (a sibling is `name.md`, one directory up is
`../name.md`), or set `link_root: true` on the route if the corpus
deliberately authors root-relative links.

## Related codes

- MDATRON-E0011 — a `..` path escaping the root (note the link family's
  different `..` posture on that page)
- MDATRON-E0012 — a symlink component refused under no-follow resolution
- MDATRON-E0110 — a link whose in-tree target does not exist
