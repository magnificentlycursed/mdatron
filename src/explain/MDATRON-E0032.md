# MDATRON-E0032 — route-conflict

**Severity:** error
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

Two or more routes' `files` globs claim this file. Ownership must be
unambiguous — conflicting adopter data is an error by contract (`DESIGN.md`
§ Validation is data-driven: conflict outcomes are asserted by value and
severity), not a silent first-match or last-match resolution.

## How to fix

Disjoint the `files` globs in `.mdatron/routes.yaml` so exactly one route
claims each file. The conflicting globs are quoted beneath the diagnostic.
