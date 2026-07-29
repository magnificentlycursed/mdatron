# MDATRON-W0044 — vocabulary-term-status-conflict

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.4.0

## What this means

A term in `.mdatron/vocabulary.yaml` is declared with conflicting statuses —
both `registered` and `draft`. The engine resolves the conflict to `draft`, the
permissive status (a draft term is exempt from strict coinage findings), and
reports this warning so the ambiguous declaration is visible rather than
silently taking whichever entry parsed first. Conflicting adopter data yields a
diagnostic, never a silent pick (`DESIGN.md` § Validation is data-driven).

## How to fix

Declare the term with a single `status`. Use `registered` if its coinages
should pass as an established term, or `draft` if it is still provisional and
its coinages should be exempt while it settles.
