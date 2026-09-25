# MDATRON-E0113 — orphaned-adopter-code

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

An adopter code token cited in a governed artifact (e.g. `VSDD-W0070`) resolves
to no entry in the **comprehensive** catalog declared for its namespace in
`.mdatron/code-catalogs.yaml`. This is the adopter-side twin of the discipline
mdatron holds itself to — every `MDATRON-` code it emits resolves in its own
explain catalog; this family lets an adopter assert the same for their own code
namespace over the governed corpus. A catalog declares a `namespace` (the
ownership prefix), the `codes` it contains, and `comprehensive: true` when it is
the sole authority for that prefix — which is what licenses this finding: under
a comprehensive catalog, a cited token that is not declared is a dangling
reference (the typical drift: codes orphaned across live files after their
documentation is retired). A token detected in prose is a namespace prefix on a word
boundary followed by a code body; tokens inside fenced code blocks are examples,
not citations. English plurals are tolerated: a single trailing lowercase `s`
immediately after the code's final digit ("both `VSDD-E0016s` were fixed") is
read as grammar, not as part of the code — the token resolves (or orphans)
as the singular. Any other trailing run stays part of the token, so a
mistyped class or suffix is still caught by the resolver.

**Scope.** The scan runs over every walked file by default, so a
`comprehensive` catalog cannot coexist with a walked archive that cites retired
codes — the same need `vocabulary_globs` answers for the register. Set
`code_catalog_globs` in `.mdatron/config.yaml` to the files whose tokens the
catalogs should resolve (root-relative globs, confined like every scope glob); a
list that matches no walked file is announced as `MDATRON-W0055`, and the
`families.code_catalog` activity reports `inert`. Two workarounds predate the
key and still work, at a cost: drop `comprehensive: true` (no orphan is ever
reported), or narrow `file_globs` so the archive is not walked at all (it then
loses route and frontmatter governance too). Inline code spans **are** scanned
— a backticked code is a real citation, not an example; only fenced blocks are
examples.

## How to fix

- **The code was retired or renamed.** Update the citation, or add the code to
  the catalog if it is still valid.
- **Typo in the cited code.** Correct it to a declared entry.
- **The catalog is stale.** Add the missing code to
  `.mdatron/code-catalogs.yaml` under its namespace.
- **The namespace is not actually comprehensive.** If codes for this prefix
  legitimately live outside the catalog, set `comprehensive: false` — mdatron
  then leaves unlisted tokens alone.
