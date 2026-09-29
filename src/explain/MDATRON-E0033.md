# MDATRON-E0033 — schema-class-route-conflict

**Severity:** error
**Status:** accepted
**Introduced in:** 0.7.0

## What this means

This file declares a `schema_class` in its frontmatter, and the route that
claims it binds it to a different class through the route's `schema` key. A
file has one class. mdatron applies the route's class, because the route
table is governance data and the file is the governed thing, and reports the
disagreement rather than resolving it silently. The file's declared class is
quoted beneath the diagnostic beside the route's.

## How to fix

- **The route binds it on purpose.** Remove `schema_class` from the file; the
  route supplies the class. This is the point of a route-attached schema for
  formats whose frontmatter you do not own.
- **The file is right and the route is wrong.** Correct the route's `schema`
  in `.mdatron/routes.yaml`, or narrow its `files` glob so it no longer claims
  this file.

## Related codes

- MDATRON-E0030 — unrouted-file: the route family's closed-world check
- MDATRON-E0050 — frontmatter-schema-violation: what a bound schema reports
