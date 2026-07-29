# MDATRON-E0094 — numeric-claim-drift

**Severity:** error
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

A prose numeral restates a configured frontmatter field's count and disagrees with it — e.g. a sentence saying "17 items" over a list frontmatter of 18, or "seven entries" over six. An adopter reported paying for this drift class three times in one review cycle, each instance caught only by expensive manual review. The check is scoped to configured field references (numeric_claims entries), never free inference.

## How to fix

Correct the prose numeral — or better, per the adopter authoring rule this check mechanizes: cite the field, never copy the value. If the claim should not be checked, remove the field from numeric_claims.
