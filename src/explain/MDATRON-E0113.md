# MDATRON-E0113 — orphaned-adopter-code

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

An adopter code token cited in a governed artifact (e.g. `VSDD-W0070`) resolves
to no entry in the **comprehensive** catalog declared for its namespace in
`.mdatron/code-catalogs.yaml`. This is the adopter-side twin of the discipline
mdatron holds itself to — every `MDATRON-` code it emits resolves in its own
explain catalog; #148 lets an adopter assert the same for their own code
namespace over the governed corpus. A catalog declares a `namespace` (the
ownership prefix), the `codes` it contains, and `comprehensive: true` when it is
the sole authority for that prefix — which is what licenses this finding: under
a comprehensive catalog, a cited token that is not declared is a dangling
reference (the motivating evidence: nine orphaned codes across six live files
after doc sunsets). A token detected in prose is a namespace prefix on a word
boundary followed by a code body; tokens inside fenced code blocks are examples,
not citations.

## How to fix

- **The code was retired or renamed.** Update the citation, or add the code to
  the catalog if it is still valid.
- **Typo in the cited code.** Correct it to a declared entry.
- **The catalog is stale.** Add the missing code to
  `.mdatron/code-catalogs.yaml` under its namespace.
- **The namespace is not actually comprehensive.** If codes for this prefix
  legitimately live outside the catalog, set `comprehensive: false` — mdatron
  then leaves unlisted tokens alone.
