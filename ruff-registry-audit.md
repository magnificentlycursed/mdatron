---
title: "Ruff — code-registry/catalog/explain reference for mdatron (#148 + code plumbing)"
tags: ["ruff", "architecture", "reference", "mdatron", "code-catalog", "rust-cli", "design-doc"]
sources:
  - url: "https://github.com/astral-sh/ruff"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://docs.astral.sh/ruff/versioning/"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://docs.astral.sh/ruff/linter/"
    title: ""
    accessed_at: "2026-08-02"
contributors: ["79Ig"]
created: 2026-08-02
updated: 2026-08-02
---


## Design Specification

### what this is

A reference-architecture review of **Ruff** (astral-sh/ruff, the Rust CLI linter/formatter for Python) for **mdatron** — its structural cousin (both Rust CLIs emitting namespaced diagnostic codes with per-code explanations). Focus: the **code-registry / catalog / explain architecture** and what it teaches roadmap **#148** (adopter code-catalog integrity) and mdatron's own code plumbing. Grounded in Ruff's source (`crates/…` on `main`, ~2026-08) and docs.astral.sh; complements [[schematron-lineage-audit]] and [[sarif-envelope-audit]].

### the rule-code registry (the load-bearing idea)

A Ruff code is an **alpha prefix + three digits** (`F401`, `E501`, `RUF100`), the prefix naming the **source linter/owner** (`F` Pyflakes, `E`/`W` pycodestyle, `B` bugbear, `PL` Pylint, `RUF` Ruff-native, `TID` tidy-imports…). The prefix is the primary namespacing axis **and** the selection unit.

Single source of truth = `crates/ruff_linter/src/codes.rs`: one big match, `(Linter, "code") => rules::…::TheRuleStruct`, wrapped by the proc-macro `#[ruff_macros::map_codes]`. From that one match the macro **generates**: the `Rule` enum (`#[repr(u16)]`), the `RuleCodePrefix` enum, and `impl Rule` accessors (`message_formats()`, `explanation() -> Option<&str>`, `fixable()`, `group()`, `from_name()`), plus `RuleCodePrefix::{parse,rules,iter}` and `Linter::{code_for_rule,noqa_code}`. The `Linter` enum declares its prefixes as attributes (`#[prefix="E"] #[prefix="W"] Pycodestyle`) and derives `RuleNamespace` (`parse_code(code) -> Option<(Self,&str)>` strips the prefix, then matches the suffix).

**Two build-time guards make orphan/ambiguous codes impossible, not merely detected:** `map_codes` **panics at compile time** if one rule is reachable from two codes ("mapped to multiple codes… disabled due to UX concerns"), and a round-trip test asserts `Rule::from_code(rule.noqa_code())` for every rule. So the code↔rule mapping is enforced 1:1 by codegen; the code list, prefixes, docs, and JSON schema are all *downstream of* the registry — no code can exist without a registry arm.

### explain — one source, many renderings

Each rule is a struct with `#[derive(ViolationMetadata)]` and a doc-comment in a fixed convention (`## What it does`, `## Why is this bad?`, `## Example`, `## Fix safety`, `## References`) plus a lifecycle attribute (`#[violation_metadata(stable_since="v0.0.183")]` / `preview_since=…`). `Rule::explanation()` returns that docstring. `ruff rule <CODE>` (and `--all`, `--output-format json`) render it; `cargo dev generate-all` regenerates `ruff.schema.json`, `docs/configuration.md`, and `docs/rules/` from the same structs. So `ruff rule`, the website table, and the schema **cannot disagree — drift is impossible by construction**, enforced in CI by regenerate-then-`git status --porcelain`-must-be-empty. (Doc *completeness* is process-enforced, not type-enforced: `explanation()` is `Option`.)

### selection, diagnostics, fixes

- **Selectors** take **codes or prefixes**: `select=["E","F"]`, `ignore=["F401"]`, `ALL`; `--select TID` toggles a whole plugin. Ruff deliberately remaps plugin codes (`TID252` ≠ upstream `I252`) so each owner gets one clean prefix. `per-file-ignores` (glob→codes), `# noqa: F401`, file-level `# ruff: noqa`.
- **Diagnostic model** (`ruff_diagnostics`, rule-independent): `Fix` = ordered `Edit`s + an `Applicability` (`Safe` apply-by-default / `Unsafe` opt-in / `DisplayOnly` manual); `Edit` = insertion/deletion/replacement over byte `TextRange`s. Rules implement `Violation` (`FIX_AVAILABILITY`, `#[derive_message_formats] message()`, `fix_title()`).
- **Output formats**: `full`/`concise`/`grouped`/`json`/`json-lines`/`junit`/`github`/`gitlab`/`pylint`/`rdjson`/**`sarif`** — Ruff emits SARIF for GitHub code scanning at low cost.

### versioning, code-meaning stability, redirects

Ruff uses a **custom scheme (minor = breaking, patch = fix)**, and Rust library APIs carry **no** stability promise (only CLI behavior is the contract). Code meaning is an explicit stability surface: a **minor** bump fires when *"a rule is promoted to stable"* or *"the behavior of a stable rule is changed"* — **excluding** *"bug fixes that follow the original intent."* New rules must land in **preview** (the unstable channel) ≥1 minor before promotion. Renames/merges **never break old codes** — `crates/ruff_linter/src/rule_redirects.rs` holds a `static REDIRECTS: HashMap<old,new>` (`("U","UP")`, `("TRIO","ASYNC1")`, `("TCH","TC")`…); `RuleGroup::Removed`→error, `Deprecated`→warning. **Codes are never recycled.**

### crate architecture & packaging

~45+ workspace crates shared by Ruff **and** the `ty` type-checker: `ruff` (binary), `ruff_linter` (engine + `codes.rs` + rules), `ruff_diagnostics` (rule-independent `Diagnostic`/`Edit`/`Fix`/`Applicability`), `ruff_python_ast`/`_parser`, `ruff_formatter`, `ruff_macros` (`map_codes`, `ViolationMetadata`, `derive_message_formats`, `RuleNamespace`), `ruff_dev` (the `cargo dev` codegen tool). Motivation is **build parallelism + cross-product reuse** (parser/AST reused across Ruff + `ty` + LSP), *not* a published library API. Distributed as one prebuilt native binary, primarily a **Python wheel** (`pip install ruff`, `uvx`). **Contrast:** mdatron's single-crate binary-first stance is compatible — Ruff *also* refuses a lib-API promise; mdatron has none of Ruff's split motivations (one binary, no second product), so single-crate is defensible. The transferable lesson is Ruff's **codegen boundary**, not its crate count.

### mapping → mdatron

- **codes.rs ranges + golden `code-catalog.json` ↔ Ruff's codegen registry.** mdatron *hand-maintains two artifacts* and reconciles them with runtime **tripwires** (`is_reserved_mdatron_code` + every-emitted-code-in-range + catalog-matches-pages). Ruff derives the whole surface from *one* match and makes an unregistered code a **compile error**. Lesson: promote the registry to the **generator** — one canonical registry emits the catalog, the range table, and explain stubs; and, where feasible, make emitting an unregistered code structurally impossible (Ruff's `report_diagnostic(Violation,…)` can't name a code with no arm), converting the "is-reserved" *runtime tripwire* into a *compile-time* guarantee. mdatron's CI tripwire is the moral twin of Ruff's `map_codes` uniqueness panic — but Ruff also removes the *possibility* of an orphan code, which mdatron doesn't yet.
- **explain pages + drift tripwires ↔ `ruff rule` doc-from-struct.** Keep mdatron's rich standalone pages (better than a docstring for rustc-style output), but **generate `code-catalog.json` FROM the page front-matter** (`Severity`, `Introduced in`, summary) so catalog↔page is a generation step, not a cross-check. mdatron `**Introduced in:**` ↔ Ruff `stable_since`/`preview_since`.
- **per-family numeric E-ranges ↔ Ruff alpha prefixes — the namespacing lesson for #148.** mdatron's numeric ranges (E0090–99 vocabulary…) are a *weak adopter-facing axis*: not a selectable/ownable token, exhaustible, family encoded in brittle arithmetic. **For #148 the adopter namespace is naturally a prefix** (`VSDD-`), which is exactly Ruff's model: `VSDD-` is a `Linter`-like namespace and the adopter declares a catalog that `parse_code` resolves against. So treat the **alpha token** (`MDATRON-`/adopter prefix + class letter) as the primary namespace/ownership axis; keep numeric ranges as an *internal* ledger only. Generalize `is_reserved_mdatron_code` into a **per-prefix ownership check** — the direct analogue of `Linter::parse_code`.
- **code-meaning stability ↔ Ruff's policy + `REDIRECTS`.** mdatron already CI-enforces "code-meaning changes fail CI" + migration notes, and is arguably *stronger* (real SemVer → bind meaning-change to **MAJOR**, cleaner than Ruff's minor=breaking). Two concrete adoptions: (1) make migration notes a **machine-readable redirect map** so `mdatron explain OLD-CODE` and adopter tooling auto-resolve renames; (2) codify "never recycle a retired code" as a tripwire, not just convention.
- **SARIF output** (see [[sarif-envelope-audit]]): Ruff emits SARIF cheaply; mdatron's stable codes + catalog map cleanly (`ruleId`=code, SARIF `rules[]`=the catalog, `level`=`**Severity:**`, `region`=range). The catalog you already maintain *is* the SARIF metadata table.

### actionable takeaways for #148 + the code/catalog/explain architecture

1. **Make the registry the generator, not a peer artifact.** One canonical code registry emits `code-catalog.json`, the range table, and explain stubs; gate with regenerate-then-`git diff` (Ruff's idiom). Where feasible make emitting an unregistered code a compile error (Ruff's `map_codes` panic + `report_diagnostic(Violation,…)`), so "is-reserved" becomes structurally impossible rather than a runtime check.
2. **Namespace on the alpha prefix; demote numeric ranges to internal allocation.** For #148 the adopter prefix (`VSDD-`) is the namespace/ownership/selection unit. Generalize `is_reserved_mdatron_code` into per-prefix ownership modeled on Ruff's `Linter`/`RuleNamespace::parse_code`; each adopter declares a catalog and every emitted `VSDD-XNNNN` must resolve into it. (Converges with SARIF's `reportingDescriptorReference` resolution — same engine, parameterized by namespace.)
3. **Generate the catalog from explain-page front-matter** so code↔explain is single-source generation, replacing the "pages match catalog" tripwire with a build step that makes mismatch impossible.
4. **Turn migration notes into a machine-readable redirect map**, never recycle retired codes, and bind code-meaning changes to a **MAJOR** bump — mdatron's real SemVer states this more cleanly than Ruff's minor=breaking.
5. **Add `--output-format sarif` (and `json`)**, reusing `code-catalog.json` as the SARIF `rules[]` metadata — low cost, unlocks CI code-scanning, and stress-tests the catalog as a reusable metadata contract, reinforcing #148.

### sources

astral-sh/ruff source on `main`: `crates/ruff_linter/src/codes.rs`, `registry.rs`, `rule_redirects.rs`; `crates/ruff_macros/src/map_codes.rs`; `crates/ruff_diagnostics/src/fix.rs`; `CONTRIBUTING.md`; `.github/workflows/ci.yaml`. docs.astral.sh/ruff: linter, settings, versioning, preview, faq. Flagged non-verified: exact `cargo dev generate-all --mode check` flag (confirmed regenerate-then-diff pattern); no per-rule "all sections present" test (`explanation()` is `Option`); `--explain` is the older spelling of `ruff rule`; the shared-`Diagnostic` migration into `ruff_db` may move crate homes; rule count ~800–900+.

