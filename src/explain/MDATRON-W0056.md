# MDATRON-W0056 — link-register-inert

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.8.0

## What this means

A `.mdatron/links.yaml` register is present, but no walked file is on a route
with `links: true`, so no absolute URL was checked against it. The register's
scope is the link family's: it governs the links of the files a route opts
in, and nothing else. With nothing opted in, every entry is inert and
`verify` stays clean, so a register nobody consults is indistinguishable from
a corpus whose every link is declared. This is the link-register counterpart
of `MDATRON-W0055` (a dead code-catalog scope) and `W0043` (a dead vocabulary
scope): data that governs nothing is announced, never silently tolerated. It
is reported once, at the register, and only on a whole-tree run whose
jurisdiction came from `config.yaml` — an incremental (`--changed`) run sees
just part of the tree, and an ad-hoc `--files` run replaces the jurisdiction
from the command line, so the link-checked files may simply be outside it.

Either no route sets `links: true`, or the routes that do claim no walked
file (announced on their own as `MDATRON-W0054`).

## How to fix

Opt the files whose outbound links the register should govern into the link
family (`links: true` on their route in `.mdatron/routes.yaml`), check that
those routes' `files` globs claim walked files, or delete `links.yaml` if no
link is to be governed.

## Related codes

- MDATRON-W0057 — a register entry no link uses
- MDATRON-W0054 — a route whose `files` glob claims no walked file
- MDATRON-W0055 — the code-catalog family's dead-scope warning
