# MDATRON-E0031 — governing-document-absent

**Severity:** error
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

A route in `.mdatron/routes.yaml` cites a `governed_by` document that cannot be
opened inside the governed tree. The files that route claims are governed by
nothing — the governance edge dangles, so the route blocks.

The path is resolved with no-follow semantics inside the project root; a
symlinked governing document is refused separately (`MDATRON-E0012`).

## How to fix

- **The document moved or was renamed.** Correct the route's `governed_by`
  path.
- **The document does not exist yet.** Create it before routing files to it —
  governance is not declared against future intentions.
