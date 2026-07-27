# MDATRON-W0041 — name-underivable

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

This file's name is not derivable from its route's `naming` grammar (a
linear-time regular expression over the filename). Naming grammars exist
because ad-hoc filename schemes proliferate faster than review can police them
— the review-log slug incident is the motivating evidence — so a route may pin
what its artifacts are allowed to be called.

## How to fix

- **Rename the file** to match the grammar (the offending name is quoted
  beneath the diagnostic; the grammar is in the message).
- **The grammar is wrong or too strict.** Amend the route's `naming` field in
  `.mdatron/routes.yaml`.
