# MDATRON-E0100 — dead-citation

**Severity:** error
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

A `file:line` citation in a citation-checked artifact names content that does
not exist in the working-tree snapshot. The working tree is authoritative:
uncommitted content counts as live, no git history is consulted, and a
citation into nothing is rejected — the motivating evidence is a review round
where 7 of 8 findings cited absent code. Citation checking is per-route
opt-in (`citations: true` in `.mdatron/routes.yaml`), so historical corpora
whose citations were true when written stay archival.

## How to fix

- **The target moved.** Update the citation to the current path.
- **The citation is stale prose.** Correct or remove it.
- **The document is historical.** Its route should not opt into citation
  checking; archival corpora are records, not living claims.
