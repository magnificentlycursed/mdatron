# MDATRON-E0030 — unrouted-file

**Severity:** error
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

Route data is supplied (`.mdatron/routes.yaml`) and this file sits inside the
walked jurisdiction, but **no route claims it**. The route table is a
closed-world allowlist over the governed tree: with routes active, every walked
file must have a declared owner — an unclaimed file blocks rather than drifting
along ungoverned.

## How to fix

- **The file should be governed.** Add a route whose `files` glob claims it,
  naming its `governed_by` document.
- **The file should not be walked at all.** Narrow `file_globs` in
  `.mdatron/config.yaml` so the walk no longer reaches it.
- **A route was dropped fail-closed.** If a confinement finding
  (`MDATRON-E0010`/`E0011`) accompanies this error, the claiming route escaped
  the governed tree and was dropped — fix that route and this error clears.
