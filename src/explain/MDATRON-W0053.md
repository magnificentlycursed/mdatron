# MDATRON-W0053 — route-table-empty

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.7.0

## What this means

`.mdatron/routes.yaml` is present but declares no routes (`routes: []`, or a
table whose every route was dropped fail-closed by a confinement finding).
The route family is **supplied the moment the file exists**: the closed-world
allowlist is active, and every walked file is unrouted (`MDATRON-E0030`) until
a route claims it. An adopter staging routes incrementally used to meet dozens
of `E0030` errors with no finding naming the cause; this warning names it, once,
at the table. The envelope's `families.route.reason` says the same thing:
`.mdatron/routes.yaml supplied (0 routes)`.

Routes are also the gateway to four other families — `citations`, `links`,
`marker_rules`, and `section_rules` exist only on a route — so an empty table
also means none of those can be active.

## How to fix

Add the first route: a `files` glob and the `governed_by` document it answers
to (`mdatron docs inputs` shows the shape). If you do not want the route family
at all, delete `routes.yaml`: absence deactivates it; emptiness does not.

## Related codes

- MDATRON-E0030 — the per-file consequence: a walked file no route claims
- MDATRON-W0054 — a route whose glob claims no walked file
