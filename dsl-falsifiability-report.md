# DSL Falsifiability Report — 2026-06-01

**Status:** Per [`BOOTSTRAP-MITIGATION.md`](./BOOTSTRAP-MITIGATION.md) § Mitigation 4. Runs the cold-context author-from-spec test on [DESIGN-MDATRON § DSL specification](./DESIGN-MDATRON.md#dsl-specification).

**Test methodology:** A general-purpose Claude session with no prior conversation context was given ONLY the `## DSL specification` section of DESIGN-MDATRON as reference (instructed to read no other section, no other file). It was asked to author 7 cross-field validation rules covering different DSL features.

**Acceptance criterion (per Mitigation 4):** ≥80% of rules authored confidently from the spec alone in one pass.

**Result:** 3 of 7 authored confidently = **43% pass rate**. Acceptance criterion **NOT MET**.

**Routing decision:** Revise DESIGN-MDATRON's DSL specification to address the 7 surfaced gaps (enumerated below); accept the revised spec without a second cold-context test (cost-bounded; can re-test in a future cycle if gaps re-surface). Step 2 (vsdd-cli scope-adjust) proceeds against the revised spec.

---

## Per-rule results

| # | Rule | Self-assessment | Notes |
|---|---|---|---|
| 1 | classification-universe | **AUTHORED, confident** | Maps cleanly to `key()` cross-file lookup + `in` membership |
| 2 | phase-composition | **AUTHORED, confident** | Mirrors the spec's worked example almost verbatim; split into two rules per the two error codes |
| 3 | validator-pair-match | **AUTHORED, confident** | Simple equality with `or` fallback |
| 4 | cite-resolution | **BLOCKED-LATENT** | No multi-match string extraction; `extract()` is singular |
| 5 | changelog-canonical-categories | **BLOCKED-LATENT** | No section-scoping primitive for markdown structure |
| 6 | bypass-rationale-required | **BLOCKED-LATENT** | No HTML-comment AST helper; `match()` regex semantics ambiguous |
| 7 | anchor-derived-from-frontmatter | **BLOCKED-LATENT** | Link node shape undocumented; slugification undefined; glob-source keys contradict many-per-file case |

## The 7 gaps surfaced

Each gap is a specific revision target for DESIGN-MDATRON's DSL section.

### Gap 1 — No multi-match string extraction

The spec gives `extract(s, regex, group)` returning a single value. Rules that need "every occurrence of pattern P in body B" (Rule 4: cite-resolution; Rule 6: bypass-rationale) cannot iterate over matches.

**Revision required:** Add `match_all(s, regex)` returning array of all match strings, and `extract_all(s, regex, group)` returning array of all captured groups.

### Gap 2 — No section-scoping primitive

`headings($self, level)` returns flat array of all level-N headings. Rules that need "level-3 headings under a particular level-2 section" (Rule 5: changelog categories under release sections) cannot scope.

**Revision required:** Add `sections_at_level($self, level)` returning nested section nodes. Each section node has `{text, level, line, column, children}` where `children` is the array of immediate-child sections at level N+1. Rules can traverse: `sections_at_level($self, 2)` returns level-2 sections; iterate over each, access its `.children` for level-3.

### Gap 3 — No HTML-comment AST helper

Markdown AST helpers stop at `headings`, `tables`, `links`, `code_blocks`, `body_sections`. HTML comments (which carry bypass markers per the bypass-marker schema) are inaccessible.

**Revision required:** Add `html_comments($self)` returning array of comment nodes with `{text, line, column}`. Content excludes the `<!--` / `-->` delimiters; line/column point at the opening `<!--`.

### Gap 4 — Link node shape underspecified

`links($self)` documented as returning nodes with the generic `{text, line, column}` convention. The cold author guessed `link.target` exists; the spec doesn't say.

**Revision required:** Document explicitly: link nodes have `{text, target, title, line, column}`. `text` is the link's visible text; `target` is the URL or anchor reference (`#anchor` or relative path or absolute URL); `title` is the tooltip-string from `[text](url "title")` syntax or null.

### Gap 5 — Slugification semantics undefined

Rule 7 (anchor-derived-from-frontmatter) requires comparing link targets like `#some-anchor` against heading slugs. The spec mentions slugs but doesn't document how they're computed.

**Revision required:** Add `slug(s)` function: lowercase the input, replace contiguous whitespace with single hyphens, strip characters outside `[a-z0-9-]`. Matches GitHub-flavored Markdown's anchor derivation. Also document heading node shape: `{text, slug, line, column}` where `slug` is `slug(text)` precomputed.

### Gap 6 — Glob-source keys paragraph forbids many-per-file

Spec says: "When `source` is a glob, each matching file contributes one entry". This is wrong/restrictive for cases like indexing all headings across all files (many entries per file).

**Revision required:** Loosen the paragraph: `select:` extracts entries; if the selection resolves to an array, each element becomes an entry; if it resolves to a single object, the object becomes one entry. Applies uniformly to single-source and glob-source.

### Gap 7 — `match()` regex semantics unclear

Spec says `match(s, regex)` returns boolean but doesn't specify anchored vs unanchored. Rule 6's negation-based assertion is correct only if `match()` is unanchored substring matching.

**Revision required:** Document explicitly: `match(s, regex)` returns true if the regex matches anywhere in `s` (unanchored substring matching). For anchored matching, the regex itself should include `^` and `$`.

## What worked well in the spec

The cold agent praised:

- The `keys:` / `key()` cross-file index abstraction (called it "genuinely ergonomic")
- The `let:` block with top-to-bottom evaluation
- Context selectors covering schema_class + glob + combined forms
- Set primitives (`difference`, `union`, `intersect`, `count`) with the worked example removing guesswork
- Reserved namespaces (`$self`, `$file`, `$project`) being explicit
- The "no invented error codes" property — all needed codes were supplied; no need to fabricate

## What the test did NOT cover

Out of scope for this run:

- `phases:` selection (runtime rule activation)
- `delegate:` escape hatch (imperative checks)
- Built-in patterns (covered by mdatron's own examples, not adopter authoring)
- Output format selection
- LSP integration (deferred to v1.1)

These remain candidates for a future cold-context test cycle once the v1.1 surfaces land.

## Net assessment

The DSL is **strong for cross-file frontmatter validation against registries** — the load-bearing case for VSDD's primary usage. It is **weak for body-content extraction, markdown-section scoping, and anchor/slug resolution** — gaps closed by the 7 revisions above.

The revisions are all additive (no breaking changes); they extend the standard library + clarify ambiguous nodes. After revision, Rules 4-7 become straightforwardly authorable.

**Decision:** Revise spec; accept revised spec without re-running the cold-context test (cost-bounded); proceed to Step 2. Re-test deferred until either:

- Adopter evidence surfaces additional gaps (run a fresh cold-context test against the gap surface)
- v1.1 surfaces add new DSL features (run a focused test covering those)
- Methodology amendment requires re-validation (operator directive)
