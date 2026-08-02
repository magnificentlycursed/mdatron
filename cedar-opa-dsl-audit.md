---
title: "Cedar + OPA/Rego — constraint-language reference for the mdatron DSL (#149 + rule validation)"
tags: ["cedar", "opa", "rego", "architecture", "reference", "mdatron", "dsl", "design-doc"]
sources:
  - url: "https://arxiv.org/abs/2403.04651"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://docs.cedarpolicy.com/policies/validation.html"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://docs.cedarpolicy.com/policies/templates.html"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://www.openpolicyagent.org/docs/policy-language"
    title: ""
    accessed_at: "2026-08-02"
contributors: ["79Ig"]
created: 2026-08-02
updated: 2026-08-02
---


## Design Specification

### what this is

A reference-architecture review of **Cedar** (cedar-policy/cedar, Rust, AWS, formally grounded) and **OPA/Rego** for **mdatron**'s narrowed rule DSL — the *constraint-language lineage* thread the Schematron audit ([[schematron-lineage-audit]]) seeded (Schematron ↔ mdatron DSL ↔ Cedar/OPA). Focus: the **bounded/analyzable-evaluation** axis and what it teaches the DSL and roadmap #149. Sourced from the Cedar OOPSLA'24 paper (arXiv 2403.04651) + cedar-spec Lean model, docs.cedarpolicy.com, and openpolicyagent.org.

### what each is, and its evaluation model

- **Cedar** — an authorization language: a policy is an effect (`permit`/`forbid`) + a mandatory scope (`principal`, `action`, `resource`) + optional `when`/`unless` conditions over those + a `context`; any matching `forbid` overrides all `permit`s. Not a general language — authorization decisions only.
- **Rego (OPA)** — declarative, Datalog-descended over JSON: rules (`head IF body`) define virtual documents; variables are existentially quantified and resolved by unification; iteration is implicit (var references) or explicit (`some x in`, `every`); comprehensions build arrays/sets/objects.
- **Both guarantee termination** (a framing correction to a common misconception): Cedar has **no iteration constructs at all**; Rego **prohibits recursion** ("evaluation should be known to terminate… recursive references… are therefore not allowed"). So Rego is *not* Turing-complete. The axis that separates them is **analyzability, not termination**.

### the bounded/analyzable axis (the mdatron lens)

- **Cedar deliberately restricts expressiveness to be *statically analyzable*.** Its symbolic compiler translates policies to **SMT** for "a decidable, sound, and complete encoding of their semantics… the first result of its kind for a non-trivial policy language, made possible by Cedar's careful control over expressiveness and by… Cedar's policy validator." Modeled + proven in **Lean**. What it gives up: no arbitrary iteration/recursion, no user functions, only `long` `+ - *` (no `/`) — "Cedar cannot express arbitrary computation, only authorization decisions."
- **Rego is far more expressive, far less analyzable:** 150+ builtins, arithmetic, aggregates, regex, unbounded traversal, comprehensions, negation. Termination (no recursion) does *not* buy Cedar's decidable-equivalence. So: Rego = expressive + terminating + weakly-analyzable; Cedar = restricted + terminating + **strongly** analyzable.
- **mdatron sits with Cedar in *instinct*, but its guarantee is operational, not logical.** The DSL is narrowed to bounded, non-backtracking evaluation: linear-time engines (regex-lite; the `regex` crate for JSON-Schema `pattern`) so no input forces ReDoS, plus a DSL expression-nesting bound (a `ParseError`, not a stack-overflow) and per-file/aggregate/structural size bounds. This is a **worst-case-cost bound**, not a decidability claim. **Claim "bounded, agent-safe evaluation," never "decidable"** — while noting the narrowness deliberately keeps a Cedar-style analysis path open.

### schema & validation

- **Cedar schema** declares entity types + attribute types, the entity hierarchy (`in` for groups), and actions with `appliesTo` (principal/resource types + `context` shape). The **validator** type-checks each policy against the schema **before deployment** — catches typo'd attributes, **type mismatches** (`string == number`), unsafe optional-attribute access (needs a `has` guard), action-applicability errors, and **warns on always-false (dead) conditions**. A proven validation-soundness guarantee.
- **Rego** has no mandatory schema; type-safety is opt-in (`# schemas` annotations + `opa check --strict`), advisory and off by default.
- **mdatron** already parses its DSL + engine-shipped data schemas **strictly** (unknown fields refused) — structurally the Cedar-validator posture, but missing the piece in Mapping (a).

### constraint surface (side by side)

| | Cedar | Rego | mdatron |
|---|---|---|---|
| Equality | `== !=` | `== !=` | `== !=` **only** |
| Ordering | `< <= > >=` | full | **none** (removed #47) |
| Arithmetic | `long +-*` (no `/`) | full + aggregates | **none** |
| Membership | `in` (hierarchy), `is` | `x in` | `in`/`not_in` |
| Presence | `has` | `not` | `defined` |
| Set ops | `.contains*`, `.isEmpty` | set ops, `count` | `union`/`intersect`/`difference`, `count`, `len` |
| Quantifiers | (none) | `some`/`every`/comprehensions | `every`/`some`/**`filter`** |
| Iteration | **none** | comprehensions | `filter` (bounded) |
| Cross-doc index | entity store | `data.*` (unrestricted) | **`key()` — path-confined** |

mdatron is a **strict subset of even Cedar's expression grammar** (no ordering, no arithmetic) plus a Rego-flavored quantifier/aggregate core — but with a **path-confined** cross-file index where Rego has unrestricted `data.*`.

### abstract / parameterized reuse

- **Cedar policy templates:** slots for **only** `?principal`/`?resource`, and only on the RHS of `==`/`in` in the scope; a template-linked policy binds concrete entities at link time. A *tightly restricted* parameterization (not macros), deliberately so analyzability survives.
- **Rego:** packages + `import` + **user-defined functions** (parameterized, must stay non-recursive) — general reuse.
- **mdatron: no parameterized rule templates today** — the open question from the Schematron audit. Cedar answers it: *yes, but with a tightly restricted slot grammar substituted at link time — not macros, not functions.*

### analysis tooling

- **Cedar** ships `cedar-policy-symcc` (→ SMT, Lean-modeled): `check_equivalent` (policy-set equivalence), `check_implies` (subsumption / "is A a subset of B"), dead-policy detection — answers **over all possible inputs**, no test corpus.
- **Rego** offers **partial evaluation** (residual policies) + `opa test` with **line coverage** — optimization + finite testing, not proof-over-all-inputs.
- mdatron has **neither** — it has bounded runtime, not rule-set analysis.

### mapping → mdatron

- **(a) "Narrowed for linear-time safety" ↔ Cedar's "restricted for decidability."** Same instinct (restrict up front, don't sandbox a general language), different depth. **Highest-value borrow: Cedar-style static validation of adopter rule *expressions* against the engine-shipped frontmatter schema** at rule-load — field existence, operand types, `==`/`!=` type-compatibility, dead-clause warnings. Today a misspelled field or type-mismatched `==` surfaces as a runtime evaluator error or a **silent no-op**; Cedar's validator turns exactly this class into a pre-gate diagnostic. Cheaper for mdatron than Cedar (the grammar is already smaller), and it keeps a future symcc-style rule-comparison latent.
- **(b) Missing rule templates ↔ Cedar templates / Schematron abstract patterns.** If templates land, copy Cedar's restraint: named slots bound to values / section-keys / `key()` targets, substituted at link time with a tightly-restricted slot grammar. **Explicitly reject the Rego model** (user functions + unrestricted composition) — it reintroduces the unbounded surface mdatron narrowed away. Templates = safe reuse; functions = not.
- **(c) `key()` + `filter`/`count` ↔ Rego comprehensions/aggregates.** Already mirror the useful Rego core: `filter` ≡ bounded comprehension-with-predicate; `count`/`len` ≡ aggregate; set ops ≡ Rego set ops. Keep the one deliberate divergence: **`key()` is path-confined** where Rego's `data.*` is unrestricted.
- **(d) Stay narrower than *both*:** keep no-arithmetic/no-ordering (narrower than Cedar), no recursion/no functions (narrower than Rego), `key()` path-confined, the closed ~9-builtin inventory, and body-content extraction **gated** until the 80% falsifiability bar. The still-open honest gap: no global wall-clock budget — the bound stays **structural** (size/depth/count), which is right for agent/hook-time-over-a-tree.

### takeaways

1. **Borrow Cedar's validator, not its SMT (yet).** Static validation of adopter rule expressions against the frontmatter schema at rule-load — the cheapest high-value item; converts runtime errors / silent no-ops into pre-gate diagnostics.
2. **Split #149 into two features.** (i) **Count-with-predicate over frontmatter arrays already ships** — document the ergonomics: exactly-one = `count(filter(...)) == 1`; **disjoint sections** = `count(intersect(a,b)) == 0`. (ii) The **body-content** leg ("exactly one open-phase H3 in ## Requirements") needs body-content-derived collections, which stay behind the **43%→80% falsifiability gate**. Don't ship them as one — the frontmatter half needs no gate; the body half is blocked.
3. **If templates land, copy Cedar's restraint** (named slots, link-time, restricted grammar) — the Schematron-abstract-pattern instinct executed the Cedar way. Reject Rego-style functions.
4. **Treat the closed inventory + no-arithmetic/no-ordering as mdatron's "careful control over expressiveness"** — the same lever Cedar pulls; it gates every new builtin behind bounded-eval review + the falsifiability gate, and keeps a future rule-comparison analysis tractable.
5. **Framing:** claim "bounded, agent-safe evaluation," never "decidable." Both Cedar and Rego terminate; only Cedar is decidably analyzable; mdatron's guarantees are operational bounds, not logic.

### review next (thread status)

- **Constraint-language lineage** (Schematron ↔ DSL ↔ Cedar/OPA): covered by this page. Candidate follow-ups: (a) Cedar-style rule-expression validation; (b) restricted rule templates; (c) the #149 frontmatter/body split.
- The reference-arch set is now complete: [[schematron-lineage-audit]], [[sarif-envelope-audit]], [[ruff-registry-audit]], this page, and the lychee link-family review.

### sources

Cedar OOPSLA'24 / arXiv 2403.04651 + ACM PDF; cedar-spec Lean model; docs.cedarpolicy.com (operators, validation, schema, templates); cedar-policy-symcc (docs.rs); OPA docs (policy-language, partial-evaluation, policy-testing, style-guide) + Styra rego-recursion-error. Flagged: no verbatim "Cedar has no loops" sentence extracted from the paper PDF (grounded via "careful control over expressiveness" + absence of iteration constructs); the "Rego ≈ first-order logic" characterization is secondary (the no-recursion/termination fact is authoritative).

