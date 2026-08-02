---
title: "Schematron (ISO/IEC 19757-3) — lineage conformance + divergence audit for mdatron"
tags: ["schematron", "architecture", "reference", "mdatron", "lineage", "design-doc"]
sources:
  - url: "https://schematron.com/document/205.html"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://schematron.com/download/ps/public_site/website/documents/schematron/abstract.html"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://schematron.com/document/141.html"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://github.com/Schematron/schematron"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://phax.github.io/ph-schematron/"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://www.xml.com/articles/2022/10/17/schematron-qlb-xslt/"
    title: ""
    accessed_at: "2026-08-02"
contributors: ["79Ig"]
created: 2026-08-02
updated: 2026-08-02
---


## Design Specification

### what this is

A **lineage conformance + divergence audit** of mdatron against its ancestor, ISO Schematron (ISO/IEC 19757-3, "DSDL Part 3: Rule-based validation"). mdatron's crate description and project identity claim descent from Schematron; this page audits that claim — what mdatron **faithfully carries**, where it **deliberately diverges** (and whether the divergence is justified), and what it **dropped that is worth reconsidering** — and then maps the findings onto the live roadmap.

Reviewed against mdatron at **envelope 3.0.0 / crate 0.6.0** (six check families: schema, route, pin, vocabulary, citation, link; the narrowed rule DSL). Schematron side worked from authoritative public sources (Jelliffe's schematron.com, the ISO "skeleton" reference implementation, ph-schematron, the SVRL schema); the ISO PDF itself is paywalled and was not consulted.

### the lineage in one line

Schematron is **rule/assertion-based validation of the constraints a grammar cannot express** — co-occurrence, cross-reference, context-dependent, cardinality-with-predicate — asserted as XPath tests against selected contexts, layered *on top of* a grammar (DTD/XSD/RELAX NG). **mdatron is that model, retargeted:** the same "assert what the schema can't" thesis, over **typed markdown + JSON Schema** instead of XML + XSD, reimplemented as a bounded, agent-first Rust CLI with a versioned diagnostic contract. The identity claim holds — the core thesis is carried verbatim ("structural checks the schema language cannot express", DESIGN § Six check families).

### mapping: schematron primitive → mdatron analog → verdict

| # | Schematron | mdatron analog | Verdict |
|---|---|---|---|
| 1 | **Validation thesis** — rule-based, additive/partial, layered on a grammar; expresses co-occurrence, cross-ref, context-dependent, cardinality-with-predicate | The whole tool: families as generic engines over supplied data, over JSON Schema (the "grammar") | **Faithful carry** — the identity |
| 2 | **`assert` (fire on test FALSE) vs `report` (fire on test TRUE)** — two polarities | DSL `assert:` **only**; no fire-on-true construct | **Divergence — dropped.** The sharpest gap (see below) |
| 3 | **One rule per node per pattern** — first matching `@context` wins; later overlapping rules silently shadowed (documented footgun) | Route family: a file routed by ≤1 route; **two routes claiming one file is a loud error (E0032)** | **Divergence — improvement.** mdatron makes overlap loud where Schematron silently shadows (aligns with "no silent degradation") |
| 4 | **Abstract patterns / abstract rules** — parameterized, reusable rule templates (`abstract`, `is-a`, `<param>`, `extends`) | None. Rules are concrete per-schema; `let:`/`key()` factor expressions but not rule *templates* | **Divergence — dropped/narrowed.** No rule reuse across structural variants (see convergence finding) |
| 5 | **`phase` / `active`** — named, run-time-selected subsets of patterns (validation profiles) | Data-driven activation + per-route opt-in flags (`citations:true`, `links:true`); families tri-state | **Partial divergence.** mdatron's activation is finer-grained (per file-glob) but has no named-profile selection axis |
| 6 | **`diagnostic`/`diagnostics` by id; `@role` severity; `@flag`; `@see`; `properties` (computed data)** — message/metadata decoupled from the test | `Finding.explain_ref` → explain page keyed by code; severity error/warning/lint; `quoted` regions (adopter data); code-catalog | **Faithful carry — strongest lineage.** Diagnostics-as-versioned-contract *is* Schematron's message decoupling, formalized (and taken further: published schema + tripwires) |
| 7 | **`let` variables** — lexical scope (schema/phase/pattern/rule) | DSL `let:` bindings (#89) — ordered, chainable | **Faithful carry** |
| 8 | **Query-language binding** — abstract over XSLT/XPath/XQuery (`@queryBinding`) | One fixed, engine-owned narrowed DSL (quantifiers, `==`/`!=`, set builtins, `key()`); linear-time engines | **Divergence — justified.** A pluggable XPath/XSLT backend would reintroduce the unbounded-evaluation/ReDoS risk mdatron explicitly designed out (bounds, linear engines, agent-first). Deliberate loss of flexibility for bounded evaluation |
| 9 | **Processing model** — compile schema → XSLT transform → run → **SVRL** results (`active-pattern`, `fired-rule`, `failed-assert`, `successful-report`, `diagnostic-reference`) | Snapshot read → direct Rust family checks → **`verify --json` envelope** | **Divergence (modernization) + a striking carry** — see below |

### verdict

**Faithful carries (the identity holds):**
- The validation thesis (assert what the grammar can't) — verbatim.
- The **diagnostic/message decoupling** (`@diagnostics`-by-id → explain-page-by-code). This is where mdatron is *most* faithful, and it went further: a versioned published schema + contract tripwires.
- `let:` variables.
- **The "report what actually ran" discipline.** Schematron's SVRL emits `active-pattern` / `fired-rule` — which patterns activated and which rules fired, as a falsifiable audit trail. mdatron's `families` **active/inert/inactive** tri-state + `files_checked` is the same idea. mdatron reached it *independently* (via vsdd-cli's #107 review), and it turns out the ancestor had it all along — a strong lineage validation of the tri-state.

**Deliberate, justified divergences:**
- Loud double-claim (E0032) vs Schematron's silent rule-shadowing — an improvement.
- Fixed narrow DSL vs pluggable query binding — required by bounded-evaluation / agent-first.
- Direct Rust engine vs compile-to-XSLT — modernization; no intermediate transform.

**Dropped, worth reconsidering:**
- **`report` polarity** (fire-on-true) — the single sharpest gap. mdatron can say "X must be present" (assert) but not "flag that X *is* present" (report) — e.g. surface an anomaly/deprecated-marker occurrence. Latent DSL capability gap.
- **Abstract / parameterized rule templates** — no reuse of one rule shape across variants.
- **Named phases / verification profiles** — no run-time-selected check subset.

### the sharpest finding: the reference-resolution convergence

Schematron's **abstract-pattern** lens (row 4) exposes something the roadmap is doing implicitly. Look at the accreting checks:

- **citation** (#86): a `path:line` token must resolve to existing working-tree content.
- **link** (#145): a `[text](target#anchor)` token must resolve to an existing in-tree path + heading.
- **marker-line reference** (#147, P3): a `Provenance: <name>` line must resolve to an existing heading/member.
- **adopter code-catalog** (#148, P4): a `VSDD-XNNNN` token must resolve to a declared catalog entry.

These are **four instances of one Schematron abstract pattern**: *"a declared-shape body token must resolve to an existing target, else emit a finding."* Schematron would express this as one parameterized rule template instantiated four ways over `(detector, target-resolver, code)`. mdatron is building them as separate bespoke checks. `cite.rs` and `link.rs` already share the same spine (body-offset scan, confinement on the target, `*_finding` emitter) by hand — the convergence is real and observable in the source. The audit's recommendation is **not** to stop the roadmap but to build the remaining reference checks on a **shared resolution core** rather than a fifth/sixth hand-rolled copy.

### roadmap impact (vsdd#20)

- **#146 (P2) section-scoped content pins** — *No Schematron impact.* Content hashing is not a Schematron concern; build as specified. (Loose tie: identifying heading-delimited spans reuses the slug/heading machinery link.rs introduced.)
- **#147 (P3) marker-line reference rules** — *Reinforced + reshaped.* A canonical Schematron reference-integrity assertion. **Build it on the link/citation resolution spine, not a fresh copy** — reuse the body-line scan, fenced-code skipping, and heading-slug extraction from `link.rs`. Keep a common resolver in view so #148 rides it.
- **#148 (P4) adopter code-catalog integrity** — *Reinforced.* Another instance of the same reference-resolution pattern; also a pleasing lineage symmetry — it exposes *adopter-side* the exact "every reference resolves to a declared entry" invariant mdatron already enforces *internally* (every-code-resolves-in-explain, Schematron's `@diagnostics`-must-resolve). Strong candidate to share #147's resolver.
- **#149 (P5) DSL filter/count-with-predicate** — *Reinforced — this is core Schematron.* "exactly one open-phase H3", "phase ids disjoint between sections" are precisely Schematron's **cardinality-with-predicate** and cross-section set constraints, the thesis's headline examples. The DSL primitive largely landed (#93/#73: `filter` composing with `count`/`len`, plus `union`/`intersect`/`difference`); #149 is the applied rule authoring. Schematron validates the direction.

**Net:** the roadmap is (unknowingly) reconstructing Schematron's expressiveness — that's validation, not a course change. The one actionable reshaping is the **reference-resolution convergence**: #147/#148 should be built on a shared resolver, which also happens to confirm doing #147 next while `link.rs` is warm.

### review next (chaining threads)

- **Results-contract lineage:** SVRL (`active-pattern`/`fired-rule`/`failed-assert`) ↔ mdatron's `verify --json` envelope ↔ **SARIF**. Anchor: the SVRL ↔ families-tri-state parallel. Review SARIF for the modern-interop end (a SARIF export is a candidate).
- **Constraint-language lineage:** Schematron query-binding + abstract-patterns ↔ mdatron's narrowed DSL ↔ **Cedar / OPA-Rego** (bounded, analyzable constraint languages). The report-polarity and abstract-pattern gaps feed this thread.
- **Packaging:** **Ruff** for the Rust-CLI rule-code/`--explain`/category layer (the *how*, complementary to Schematron's *what*).
- **Link family (domain-specific):** **lychee** / markdownlint — orthogonal to Schematron; pressure-tests #145's document-relative resolution.

### candidate issues surfaced (design questions, not yet filed)

1. **DSL `report` polarity** — a fire-on-true assertion form, to surface anomaly *presence* (not only requirement *absence*). Low urgency — no current roadmap item needs it.
2. **Shared reference-resolution engine** — factor cite/link/marker-ref/adopter-code onto one parameterized resolver (the abstract-pattern insight). Highest-leverage architectural note; actionable at #147/#148.
3. **Named verification profiles / phases** — run-time-selected check subsets (Schematron's `phase`). No current demand; backlog note.

### sources

Rick Jelliffe / schematron.com (design rationale, assert-vs-report, abstract patterns, diagnostics); the ISO Schematron "skeleton" reference implementation (`Schematron/schematron`, the include → abstract-expand → SVRL-compile pipeline); ph-schematron (native + XSLT backends); the SVRL schema (`Schematron/schema`); xml.com "Schematron query language bindings" (2022); NISO JATS4R "Schematron — a handy XML tool" (one-rule-per-node); Wikipedia "Schematron" (history/editions cross-check). Load-bearing semantics (polarity, one-rule-per-node, abstract expansion, SVRL) corroborated by ≥2 sources each.

