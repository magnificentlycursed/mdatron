# MDATRON-W0042 — governance-weakening-unjustified

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

An `unpinned:` entry in `.mdatron/pins.yaml` carries no justification. Changes
that weaken governance carry an annotated justification that one search
enumerates (`DESIGN.md` § Governance data is governed) — a weakening that
cannot say why it stands, or who stands behind it, is not a tombstone; it is
an erasure. The `owner` field is unauthenticated by construction; the
operative control is commit review cross-checking the annotation against
authorship.

## How to fix

Add `reason` (why this file left the pin record) and `owner` (who ruled it) to
the entry. It then downgrades to the standing informational lint
(`MDATRON-L0001`).
