# regex-lite

**Status:** Approved.

**Pinned version:** `^0.1` (resolves to 0.1.9)

## Why this dependency

Naming-grammar matching for the route family (#83: `W0041 name-underivable`)
and, later, vocabulary-family label schemes and anti-patterns (#85). DESIGN L17
requires adopter-supplied regular expressions to run on **linear-time engines**
under the step budget — `regex-lite` (like its big sibling `regex`) is a
finite-automata engine with no backtracking, so adopter patterns cannot be
pathological.

**Alternatives considered:**

- `regex` (full): same team, same linear-time guarantee, but pulls
  `aho-corasick` + `memchr` + `regex-syntax` + `regex-automata` and Unicode
  tables. Filename grammars and label schemes are ASCII-scale; the full
  engine's throughput and Unicode classes are not needed here. Revisit if a
  family genuinely needs `\p{...}` classes.
- Hand-rolled glob-ish matcher: rejected — naming grammars are contracts
  adopters author; a bespoke dialect would be its own falsifiability burden.

## PE supply-chain notes

- **Version pin discipline:** workspace `regex-lite = "0.1"` → 0.1.9.
- **Maintainer trust:** rust-lang regex team (same org as `regex`).
- **Transitive deps:** **zero** — the crate's defining feature.
- **`cargo audit`:** clean at pin time.

## Security notes

- **CVE history:** none for `regex-lite`.
- **License:** MIT OR Apache-2.0; compatible.
- **Threat model:** adopter-supplied patterns are untrusted input; linear-time
  matching removes the ReDoS class by construction. Compile errors on load are
  loud config errors, never silent no-ops.

## SO approval

- **Operator-attribution:** family formats + code assignments ratified
  2026-07-27 (#45); the naming-grammar mechanism is part of the ratified route
  shape.
- **Scope justification:** one zero-dependency crate serving two ratified
  families; proportionate.

## Co-authorship attribution

Per VSDD-E0100 discipline:

```
Co-authored-by: Solution Owner <so@vsdd-domains>
Co-authored-by: Platform Engineer <pe@vsdd-domains>
Co-authored-by: Security <security@vsdd-domains>
```
