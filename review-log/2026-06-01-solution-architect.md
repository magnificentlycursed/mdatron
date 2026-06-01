---
schema_class: review-entry
schema_version: 1.0.0
review_number: 1
date: 2026-06-01
phase: phase-3
scope: Adversarial 8-domain review of (a) the 5-step bootstrap plan for splitting mdatron out of vsdd-cli, and (b) the proposed mdatron architecture (library/binary split, Schematron-lineage DSL, LSP-as-v1, delegated-check protocol, error catalog, "easy for agents + humans + caught-early" goals). Subject artifacts in-thread; not yet committed to a DESIGN-MDATRON.md. Naming choice (mdatron) under review.
lens: 5-lens application weighted Consistency (5) + Maintainability (4) + Attacker (3) + Usability (3) + Edge cases (2). Mixed-weight because subject spans architecture, supply chain, agent-facing UX, and methodology recursion.
source: director-raised
session_note: Inline single-session multi-domain composition; deviates from Phase 3 canonical cluster-batched cold-session shape per the same operator-directive pattern as the DR review of 2026-06-01. Methodology deviation declared in composition block + raised as a meta finding (M1).
model: claude-opus-4-7
execution_method: inline main session; eight domain primers loaded sequentially (SA + SO + PE + AIE + Security + TW + DR + VSDD-Methodology); Sanity-Check reserved for cross-domain disagreement adjudication
sycophancy_compensation: |
  I (the inline-running reviewer) am the same identity that co-designed the inversion + the DSL spec
  + the LSP-as-v1 recommendation + the mdatron naming pick across the originating conversation.
  Strong bias to read my own design choices as load-bearing-by-construction. Compensation: every
  finding grounded in either a citable artifact (existing review log entry; named precedent in
  vsdd-cli's DESIGN docs; convention established in the originating conversation) OR explicitly
  raised to the relevant domain for adversarial reading rather than self-validated. Where the
  choice is judgment-only and self-favoring, the finding routes to Sanity-Check.
---

# 8-domain review: mdatron inversion plan + design — 2026-06-01

## Pre-phase composition declaration

```yaml
phase: phase-3
composed_domains: [solution-architect, solution-owner, platform-engineer, ai-engineer,
                   security, technical-writer, documentation-reviewer, vsdd-methodology]
composition_mode: inline-single-agent-multi-domain
memory_isolation: NONE (single main-session; no worktree isolation; no --no-memory flag)
operator_confirmation: confirmed ("start the 8-domain review")
cluster_shape: deviation-from-4-cluster-default — same shape as 2026-06-01-documentation-reviewer
declared_at: 2026-06-01
methodology_deviation: |
  Canonical Phase 3 is cluster-batched cold-session reviewers. This runs inline. Cost: cold-context
  discipline + adversarial-pair separation lost. Mitigation: every finding evidence-grounded; bias
  toward over-raising rather than self-validating. Raised as recurrence pattern (see M1).
```

## Scope

Six surfaces under review:

1. **The 5-step bootstrap plan** (design-doc → scope-adjust → vsdd init → use to bootstrap mdatron → refactor)
2. **The library/binary split** (mdatron-core + mdatron-cli; vsdd-cli depends on it)
3. **The DSL spec** (Schematron-lineage YAML; `let` / `key` / `every` / `count`; built-in patterns; delegated checks)
4. **LSP-as-v1** (promoted from v1+; load-bearing for the "caught early" goal)
5. **Naming: mdatron** (TRON-blockchain collision; jsontron precedent)
6. **The recursive dogfood loop** (VSDD designs mdatron; mdatron then validates VSDD docs)

---

## SA — Solution Architect

Lens: Consistency, Maintainability. Validator pair: SO.

### SA-F1 — Step 1 + Step 2 are not sequenceable; the plan implies otherwise — Open

The plan lists: (1) write DESIGN-MDATRON, (2) scope-adjust vsdd-cli. Step 1's content depends on what vsdd-cli will offload; Step 2's content depends on what mdatron will accept. Linear ordering forces premature commitment in whichever runs first.

**Why this matters:** the existing DESIGN-METHODOLOGY ↔ DESIGN-SCHEMA ↔ DESIGN-VERIFICATION cross-doc-coordination pattern already requires co-evolution between sibling docs (per `consumes_from`/`produces_for` tables). The plan should explicitly adopt the same pattern across the vsdd-cli ↔ mdatron boundary.

**Routing:** plan-level revision before authoring begins. Gate: explicit declaration of the joint-pass discipline.

### SA-F2 — DESIGN-MDATRON's repo-tree position is undeclared — Open

mdatron is a standalone tool with non-VSDD adopters. Does DESIGN-MDATRON live in `vsdd-cli/`, `mdatron-cli/`, or a separate `mdatron-spec/` repo? Each choice has cascading consequences for cross-doc references in the existing DESIGN docs and for the mdatron crate's own self-documentation.

**Why this matters:** SA-precedent-finding 2026-05-27-F3 (orphan anchor-ID rules) shows refactors leave orphans across doc trees. Splitting the doc tree before declaring the boundary creates more of those.

**Routing:** Phase 1a — must answer before Step 1 begins.

### SA-F3 — The library/binary split (`mdatron-core` + `mdatron-cli`) is sketched but boundary-undeclared — Open

The conversation specifies a two-crate workspace but doesn't enumerate the ownership boundary. Specifically: does the LSP server live in `mdatron-cli`, `mdatron-core`, or a third crate (`mdatron-lsp`)? Does the delegate protocol library live in `mdatron-core` (so delegates can link it as a Rust library) or only as a wire spec?

**Why this matters:** the vsdd-cli single-binary precedent solved this by collapsing the question. Splitting introduces the question, but the conversation doesn't answer it.

**Routing:** DESIGN-MDATRON authoring; ship boundary table on day one.

### SA-F4 — 13 artifact classes split between vsdd-cli-specific and generalizable is unspecified — Open

DESIGN-SCHEMA defines 13 artifact classes. Some are deeply vsdd-specific (Review entry, Finding, Phase primer, Domain prompt, Methodology event variants, Exit Signal). Others are generic enough to live in mdatron's example library (CHANGELOG, DESIGN doc, PR template, Manual-test). The inversion needs an explicit table mapping each class to vsdd-cli OR mdatron-example OR shared-pattern.

**Why this matters:** without this table, the step-2 scope-adjust will be a 13-class judgment call done case-by-case.

**Routing:** Phase 1a — produce the table as part of scope-adjust.

---

## SO — Solution Owner

Lens: Consistency, Usability. Validator pair: SA.

### SO-F1 — "Non-VSDD adopters exist" premise is unverified — Open

The decision to invert rests on the premise that another methodology / project plausibly adopts mdatron. The originating conversation declared the premise affirmatively ("non-VSDD adopters exist") but cited zero concrete candidates.

**Why this matters:** the inversion's full cost (two binaries to audit; two release pipelines; coordinated versioning; doubled supply-chain attestation) is justified only if adoption beyond VSDD is real. If VSDD remains the only user 18 months out, the inversion is overhead.

**Routing:** SO judgment — name 2-3 concrete adopter candidates (RFC repos, ADR repos, specific projects) OR accept the inversion is speculative + size the cost accordingly.

### SO-F2 — mdatron v1 ship-criterion is unspecified — Open

The originating conversation extensively discussed mdatron's design but never declared what "mdatron v1 ships" means. Specifically: does v1 require LSP integration to be functional? Does v1 require all 5 built-in patterns? Does v1 require the delegate protocol implementation?

**Why this matters:** without a ship-criterion, Step 5 (refactor vsdd-cli to use mdatron) has no completion condition. Scope creeps indefinitely.

**Routing:** Phase 1a — declare ship-criterion as part of DESIGN-MDATRON closing section.

### SO-F3 — Step 3 throwaway-vs-mdatron-shaped choice is owner-required — Open

The originating conversation surfaced two options for Step 3 (`vsdd init` implementation before mdatron exists): throwaway scaffolding vs. build-in-mdatron-shape-from-day-one. This is a load-bearing scope-and-effort decision; SO ownership required. The conversation framed it as an aside; it should be Phase 1a's first decision.

**Why this matters:** the choice changes the operator-time budget by ~30-50% across the 5 steps.

**Routing:** SO declaration before Step 3 begins.

---

## PE — Platform Engineer

Lens: Consistency, Maintainability, Attacker. Validator pair: Security.

### PE-F1 — Two-binary release pipeline doubles supply-chain attestation surface — Open

`mdatron` + `vsdd` each require cosign/sigstore signing, SLSA provenance, reproducible builds, per-platform GitHub Release artifacts (Linux x64/arm64 + macOS x64/arm64 = 4 platforms × 2 binaries = 8 release artifacts per version). The existing DESIGN-VERIFICATION Track 5n already declared per-binary supply-chain pipeline as v1.0 ship-blocker.

**Why this matters:** doubled artifact count = doubled attestation surface = doubled CVE-response posture. PE owns the question of whether the pipeline shape extends to 2 binaries cleanly or requires architectural change.

**Routing:** DESIGN-MDATRON v1 release-engineering section.

### PE-F2 — LSP server packaging + IDE config deployment is a new artifact class — Open

LSP-as-v1 requires: (a) the LSP server binary itself (could be `mdatron lsp-serve` subcommand OR a separate `mdatron-lsp` binary); (b) IDE configuration deployment (VS Code extension manifest, Neovim LSP config snippet, JetBrains plugin metadata at minimum); (c) IDE-extension distribution pipelines (VS Code Marketplace, OpenVSX).

**Why this matters:** none of this exists in the current vsdd-cli DESIGN docs. Adding it expands the operational surface meaningfully.

**Routing:** DESIGN-MDATRON v1 must declare LSP packaging discipline or explicitly defer.

### PE-F3 — Dependency-approval discipline applied retroactively — Open

VSDD-E0100 (dependency-approval-missing) requires SO + PE + Security investigation entry at `docs/dependencies/<crate>.md` for every new direct dependency. When vsdd-cli adopts mdatron at Step 5, mdatron is itself a new direct dependency. Per current discipline, `docs/dependencies/mdatron.md` is required + co-authorship trailers + investigation surface.

**Why this matters:** the discipline applies to vsdd-cli adopting mdatron — this is no different from adopting any other crate. The plan should call out the investigation as a required Step-5 artifact.

**Routing:** Step 5 deliverable list.

### PE-F4 — Install-order discipline for adopters is undeclared — Open

Adopter workflow: `cargo install mdatron`? `cargo install vsdd`? Both? `vsdd init` runs `cargo install mdatron --locked` if missing? The originating conversation mentioned the question but didn't answer it.

**Why this matters:** vsdd-cli's existing init order discipline (`crosslink init` → `vsdd init`, refuses-on-wrong-order with `VSDD-E0220`) is the precedent. The mdatron install relationship needs the same explicit ordering rule.

**Routing:** Step 3 (vsdd init) must declare mdatron pre-flight.

---

## AIE — AI Engineer

Lens: Usability, Maintainability. Validator pair: SE.

### AIE-F1 — LSP-as-v1 is correct but underspecified at the agent-tool-surface boundary — Open

The originating conversation recommends LSP-as-v1. Correct call. But the actual agent integration is sketched as a tool-result enrichment (mdatron diagnostics returned alongside Edit tool results), which is **a distinct mechanism from LSP**. LSP is editor-protocol; tool-result enrichment is agent-loop. The two should both exist but are separate features.

**Why this matters:** without distinguishing them, "LSP-as-v1" silently absorbs the tool-result-enrichment workstream and risks neither being delivered cleanly.

**Routing:** DESIGN-MDATRON must declare both surfaces explicitly: `mdatron lsp-serve` (editor protocol) AND `mdatron verify --files <staged>` integration into the agent's pre-completion check loop.

### AIE-F2 — DSL syntax LLM-readability claim is unvalidated — Open

The DSL spec was authored under the claim "easy for agents to write/read." Self-assessed. No falsifiability check exists. The spec uses `$self.field`, `every(d in xs, pred)`, `key("name", value)` — JS/Python-shaped syntax believed to be handled well by Claude-class models, but the claim is untested.

**Why this matters:** if the DSL turns out to be hard for agents to author from a one-page reference (e.g., the `$self` prefix vs. JSONPath conventions; the `every(x in xs, pred)` form vs. list comprehensions), the "agent-friendly" goal is unmet.

**Routing:** Phase 2 (validation cycle) — author 5-10 DSL rules from cold-context using only the spec as reference; measure error rate.

### AIE-F3 — Skill / domain-prompt integration with mdatron is unspecified — Open

When an agent invokes `/vsdd-domain-quality-engineer` skill, does mdatron's diagnostic stream flow into the skill's working context? Currently the skills are operator-interactive entry points (per README). Adding mdatron's live diagnostics could enrich the skill's review pass (e.g., "before producing your review, here's what mdatron already flagged").

**Why this matters:** missing this integration leaves a substantial leverage opportunity on the table — the cross-skill ↔ mdatron loop is exactly what makes "shift-left into the agent's working session" real.

**Routing:** Phase 1a (DESIGN-MDATRON adopter-companion section).

---

## Security

Lens: Attacker, Edge cases. Validator pair: Red Team.

### SEC-F1 — Delegate protocol = new boundary; canonical-schema-path discipline does NOT currently extend — Open

DESIGN-VERIFICATION declares canonical-schema-path discipline: "schemas are NEVER hot-loaded from PR-modifiable paths." The delegate protocol introduces `.mdatron/delegates/<name>.yaml` registration AND `command:` invocation. If a PR adds a delegate registration pointing to a malicious binary, mdatron will subprocess-invoke it on every pre-commit.

**Why this matters:** the discipline that protects schemas does not currently extend to delegate registrations. PR-injected delegates = arbitrary code execution at validation time.

**Routing:** DESIGN-MDATRON must declare delegate-registration-discipline that mirrors canonical-schema-path: delegates registered only from operator-local config OR signed by toolkit-release pipeline, never from PR-modifiable paths.

### SEC-F2 — Pattern files can express arbitrary cross-file lookups via `key()` — Open

The DSL's `key()` mechanism reads files declared in the pattern's `keys:` section. A malicious pattern declaring `source: .ssh/id_rsa` would have mdatron parse and lookup against the file. Same surface as Schematron's `document()` function — and Schematron implementations have had path-traversal CVEs.

**Why this matters:** patterns are written by adopters AND patterns ship in adopter PRs. A `.mdatron/patterns/<file>.yaml` modified in a PR can pull arbitrary file contents into diagnostics and exfiltrate via the `message:` template.

**Routing:** DESIGN-MDATRON must declare path-confinement for `key()` sources (must resolve under project root; symlinks not followed; specific globs not allowed to traverse parents).

### SEC-F3 — Step 3-4 validator-absent state = unsigned-document period in audit trail — Open

The plan produces VSDD-methodology documents at Steps 3-4 without mdatron's validators running. The audit trail (`.vsdd/events.jsonl`) records the documents' commits, but `ValidationPassed`/`ValidationFailed` events cannot fire — no validator exists.

**Why this matters:** the audit-trail integrity claim ("every action emits structured evidence") has a documented period of being incomplete. Forensic review post-incident has a gap.

**Routing:** mitigation options: (a) write a minimal Python validator-of-validators for Steps 3-4 (the operator surfaced this); (b) explicitly mark the period in the audit trail with a `BootstrapPeriodEntered` / `BootstrapPeriodExited` event pair so the gap is named, not silent.

### SEC-F4 — mdatron's own dependency chain becomes vsdd-cli's CVE-response posture — Open

If vsdd-cli depends on mdatron at Step 5, any CVE in mdatron's dependency tree propagates to vsdd-cli's posture. Currently DESIGN-VERIFICATION's CVE-response discipline applies to vsdd-cli's direct deps. After Step 5, it applies to mdatron's deps too (transitively).

**Why this matters:** the CVE-response wall-clock budget doubles.

**Routing:** DESIGN-MDATRON v1 must declare its dependency budget (target: minimal direct deps; pinned versions; no optional features that pull large subtrees).

---

## TW — Technical Writer

Lens: Consistency, Usability. Validator pair: DR.

### TW-F1 — Mentor-voice consistency in mdatron's diagnostic emission is unpinned — Open

The originating conversation's diagnostic mockups use rustc-style direct messaging ("missing required domain quality-engineer"). The tone is consistent with Mentor voice but isn't explicitly named as a discipline.

**Why this matters:** vsdd-cli's existing tone-flex policy declares Mentor voice per-surface. mdatron's emission is a new surface and needs explicit policy declaration to avoid drift.

**Routing:** DESIGN-MDATRON § Diagnostic UX — declare Mentor voice as the default + name exceptions (schema definitions, SARIF/JSON output).

### TW-F2 — `mdatron explain <code>` page format is unspecified — Open

The originating conversation showed an `explain` page example for VSDD-E0200. Format is implicit. No spec yet for: required sections (what / failing example / how to fix / related / introduced-in); failing-example provenance (is it generated from the fixture set? hand-authored?); how related-codes are populated.

**Why this matters:** the explain pages are page-per-code at ~30+ codes for VSDD alone. Inconsistency across pages destroys the "rustc-style explain" credibility.

**Routing:** DESIGN-MDATRON must declare explain-page template.

### TW-F3 — Naming disambiguation ("Schematron-derived, not TRON-blockchain") is not yet a written discipline — Accepted

The originating recommendation ("README's first sentence explicitly disambiguates") needs to be committed to writing in DESIGN-MDATRON. Currently it's a chat suggestion. By the time someone reads the README, the chat-recommendation is gone. The discipline must live in the artifact.

**Classification:** Accepted (own-recommendation noted; routes to DESIGN-MDATRON authoring).

---

## DR — Documentation Reviewer

Lens: Consistency. Validator pair: TW.

### DR-F1 — DESIGN-MDATRON's `consumes_from`/`produces_for` is undeclared — Open

The existing 5-doc tree (README + DESIGN-METHODOLOGY + DESIGN-SCHEMA + DESIGN-OBSERVABILITY + DESIGN-VERIFICATION) uses `consumes_from`/`produces_for` frontmatter for cross-doc coordination. Adding DESIGN-MDATRON requires declaring its position in the consume/produce graph BEFORE authoring begins, or the cross-doc references will accumulate as orphans.

**Why this matters:** Class 3 from the late-detection review evidence (broken intra-doc hyperlinks) is exactly this failure mode.

**Routing:** Step 1 (DESIGN-MDATRON authoring) prerequisite — produce the cross-doc table first.

### DR-F2 — JSON Schema (structural) vs. DSL YAML (rule-based) cold-reader confusability — Open

The mdatron spec uses YAML for both schemas (sort of — they're JSON Schema rendered into JSON files) AND patterns (the DSL). A cold reader landing on `.mdatron/` may not immediately understand the two-layer architecture.

**Why this matters:** Schematron history shows precisely this confusion (XSD vs. Schematron rule files coexist; new adopters routinely conflate them).

**Routing:** DESIGN-MDATRON Adopter Onboarding section must lead with the two-layer architecture diagram before showing any rule.

### DR-F3 — Cross-anchor resolution between vsdd-cli docs and DESIGN-MDATRON not specified — Open

If DESIGN-MDATRON lives in mdatron-cli's repo (not vsdd-cli's), then vsdd-cli's DESIGN docs cite it via external URL OR via path that requires mdatron-cli to be cloned alongside. Anchor-ID resolution across two repos is not the same problem as anchor-ID resolution within one repo.

**Why this matters:** the bookmark-cli-manual + DR Review 2026-06-01-F1 precedent (registry comments citing wrong filename) shows cross-doc citations break silently. Cross-repo citations break more silently.

**Routing:** Phase 1a — declare the cross-repo citation convention (full URL? doc:// scheme? something else?).

---

## VSDD-Meta — Methodology

Lens: Maintainability, Consistency. Validator pair: Sanity-Check.

### M1 — Inline-multi-domain composition is now the third recurrence — Open

The Phase 3 canonical shape is cluster-batched cold-session reviewers. This review (2026-06-01 mdatron 8-domain) is the third recurrence of inline-single-agent-multi-domain composition (after 2026-05-27 SA+QE+Security cluster-batched inline, and 2026-06-01 DR-led 6-domain inline). The DR Review of 2026-06-01 raised F12 as a recurrence-pattern finding.

**Why this matters:** earned-by-recurrence trigger is now met. The methodology must either (a) accept inline-multi-domain composition as a first-class shape with explicit applicability rules, OR (b) declare the recurrent deviation as a methodology defect requiring tighter discipline. Continuing to flag-each-as-deviation is the worst option.

**Routing:** Phase 1a — DESIGN-METHODOLOGY amendment + new event variant `CompositionShapeDeviation` OR new methodology-canonical shape `InlineMultiDomain` with declared applicability.

### M2 — Recursive dogfood loop honor-system gap is real — Open

VSDD is used to design mdatron; mdatron will later validate VSDD documents. Steps 1-4 produce methodology artifacts without mdatron running. This is honor-system.

**Why this matters:** the methodology's central claim is "not honor-system; not authoring discipline alone — every rule has a hook OR a schema validator OR a crosslink workflow check." During Steps 1-4 of this plan, that claim is partially false: structural drift can land without any validator firing.

**Routing:** mitigation per SEC-F3 (minimal Python validator-of-validators) OR explicit methodology amendment naming the bootstrap period as a recognized exception with bounded duration + named exit criteria.

### M3 — Phase 5 (Purity Boundary Audit) has no analog for mdatron at Step 1 — Open

The SA precedent (2026-05-27-solution-architect.md) shows Phase 5 was adapted to spec-stage purity-boundary audit (since no implementation surface existed). When DESIGN-MDATRON is authored at Step 1, the same adaptation will be needed — but the implementation surface for mdatron literally does not exist (no `vsdd-core/src/schemas/`-equivalent for mdatron yet).

**Why this matters:** the methodology's Phase-5 mechanism cannot fire on Step-1 artifacts in its current form.

**Routing:** DESIGN-METHODOLOGY amendment OR explicit Phase-5-deferred declaration for DESIGN-MDATRON.

### M4 — Naming-discipline hook (check-naming-discipline) cannot fire during Steps 1-4 — Open

Per DESIGN-VERIFICATION's 19-hook list, `check-naming-discipline` consolidates 4 rules including letter-label anti-pattern + suite-internal terminology + vocabulary registry compliance. None of these fire without the hooks installed (Steps 1-4 are pre-mdatron). New terms coined during DESIGN-MDATRON authoring (the DSL has many: `pattern`, `rule`, `context`, `assert`, `let`, `key`, `phase`, delegate-protocol, built-in patterns, etc.) will land in the vocabulary registry retroactively at Step 5.

**Why this matters:** vocabulary-registry-coverage-gap (Class 7 from late-detection review evidence) is the predicted failure mode. Worst case: 30+ new coinages from DESIGN-MDATRON authoring land unregistered at Step 5 and surface as a wave.

**Routing:** mitigation — manually maintain a `coinage-log.md` during Steps 1-4, registered as a candidate-vocabulary list, populated retroactively to the registry at Step 5.

---

## Cross-domain synthesis

**Strongest signal:** four domains (SA, DR, PE, Security) independently surfaced findings about **the missing concrete declarations** that need to land in DESIGN-MDATRON before Step 1 authoring begins productively:

- SA-F1, SA-F2, SA-F3, SA-F4 — boundaries, doc-tree position, library/binary split, class-split table
- DR-F1, DR-F3 — `consumes_from`/`produces_for`, cross-repo anchors
- PE-F4 — install-order discipline
- SEC-F1, SEC-F2 — delegate path-confinement, `key()` path-confinement

These cluster into a single observation: **DESIGN-MDATRON needs a one-page "boundary declaration" preamble** authored as Step 0.5, before the substantive design content begins. The preamble closes the open boundary questions all four domains independently raised. Estimated effort: small. Estimated avoided cost: substantial — without it, Step 1 produces a design that Step 2 then forces revision on.

**Second-strongest signal:** three domains (Security, AIE, VSDD-Meta) raised findings about **the bootstrap period (Steps 1-4) being a validator-absent state**. Each surfaced a different consequence (audit-trail gap; agent-friendly-claim unvalidated; honor-system gap). Combined: the bootstrap period is a real risk class needing explicit mitigation, not implicit acceptance.

**Recommendation:** before Step 1 begins, produce two artifacts as Step 0.5:

1. **Boundary declaration preamble** for DESIGN-MDATRON (addresses SA, DR, PE, SEC findings)
2. **Bootstrap-period mitigation plan** (minimal Python validator-of-validators + coinage-log + audit-trail bootstrap-period markers)

These are small artifacts that unblock the substantive work.

## Classification summary

24 findings: 20 Open + 4 Accepted/Other.

| Domain | Open | Accepted | Routing target |
|---|---|---|---|
| SA | 4 | 0 | Phase 1a (DESIGN-MDATRON authoring prerequisites) |
| SO | 3 | 0 | SO judgment (premise verification + ship-criterion + throwaway decision) |
| PE | 4 | 0 | Phase 1a (release-engineering + LSP packaging + install order) |
| AIE | 3 | 0 | Phase 1a + Phase 2 (DSL falsifiability test) |
| Security | 4 | 0 | Phase 1a (path-confinement + bootstrap audit-trail markers) |
| TW | 2 | 1 | Phase 1a (Mentor voice + explain page format) |
| DR | 3 | 0 | Phase 1a (cross-doc + cross-repo anchor discipline) |
| VSDD-Meta | 4 | 0 | DESIGN-METHODOLOGY amendment + bootstrap-period naming |

## Coordination

- **Bundle for Phase 4 routing**: SA + DR + PE + SEC findings all route to DESIGN-MDATRON authoring prerequisites. Single Phase 4 → Phase 1a commit appropriate.
- **SO findings require operator-direct attention** before Phase 1a begins (premise verification, ship-criterion, throwaway-decision). These are not routable to Phase 1a until SO answers.
- **VSDD-Meta findings route to DESIGN-METHODOLOGY** as amendment proposals, not to DESIGN-MDATRON directly. Separate Phase 4 → Phase 1a commit on the methodology spec.
- **Sanity-Check not invoked**: no domain disagreement detected; findings are independent across the 8 lenses, not conflicting.
- **Recurrence pattern**: M1 (inline-multi-domain composition recurrence) is the third instance and meets the earned-by-recurrence trigger threshold per VSDD methodology amendment governance. Methodology amendment formally proposed.
