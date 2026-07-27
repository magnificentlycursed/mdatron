# MDATRON-E0094 — numeric-claim-drift

**Severity:** error
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

A prose numeral restates a configured frontmatter field's count and disagrees with it. This drift class was paid for three times in one adopter review cycle (17-vs-18 domain prompts, seven-vs-six core domains, a stale three-OPEN-notes sentence) — each caught only by expensive cold review. The check is scoped to configured field references (numeric_claims entries), never free inference (#80 D3).

## How to fix

Correct the prose numeral — or better, per the adopter authoring rule this check mechanizes: cite the field, never copy the value. If the claim should not be checked, remove the field from numeric_claims.
