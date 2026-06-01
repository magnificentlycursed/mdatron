---
declaration_class: phase-composition
phase: phase-2a
declared_at: 2026-06-01
artifact_under_authoring: mdatron-core + mdatron-cli skeleton (stubs) + Red Gate tests
supersedes: c8f6997 (Step 3 commit rolled back due to Phase 2b methodology drift)
---

# Phase 2a Composition Declaration — mdatron Red Gate (Step 3 Remediated)

**Provenance:** Authored as part of the full-reset remediation chosen in response to the Phase 2b methodology drift on the first Step 3 commit (c8f6997, rolled back). The original Step 3 went straight to implementation without declaring composition, loading the Phase 2b primer, consulting the Rust supplement, or following the dependency-approval discipline (per VSDD-E0050 + VSDD-E0100). Full-reset restores the Phase 2a → Phase 2b sequence.

**Manual audit-trail anchor (per BOOTSTRAP-MITIGATION § Mitigation 3):** This composition declaration serves as the manual `PhaseCompositionDeclared` event variant payload until tooling for event emission exists.

---

## Composition

Per [`vsdd-cli/.claude/commands/vsdd-phase-2a.md`](../.claude/commands/vsdd-phase-2a.md) the canonical Phase 2a composition is **Quality Engineer (primary)** plus the always-on baseline. mdatron's per-feature axes (ui-surface=yes via LSP+TTY; ships-code=yes via Rust binary) add UX + Accessibility + Performance Engineer. Operator directive adds AI Engineer.

```yaml
phase: phase-2a
composed_domains:
  # Phase 2a primary
  - quality-engineer
  # Always-on baseline (project ships code)
  - software-engineer
  - solution-architect
  - solution-owner
  - platform-engineer
  - performance-engineer
  # Axes-activated (ui-surface=yes)
  - ux
  - accessibility
  # Operator-directive activation
  - ai-engineer
composition_mode: inline-multi-domain          # M1 recurrence #6 acknowledged
memory_isolation: NONE (main-session inline; no worktree isolation)
operator_confirmation: confirmed
  ("Full reset: roll back the Step 3 commit, redo properly with Phase 2b composition + Red Gate first"
   + "AI Engineer should weigh in" + "AIE should draft a review on mitigation techniques too")
declared_at: 2026-06-01
methodology_deviation: |
  Two deviations from canonical Phase 2a:
  
  (1) Inline-multi-domain composition instead of cluster-batched cold-session per-domain.
      M1 recurrence #6 this conversation. Acknowledged + already finding-tracked.
      AIE-MITIG-F6 (load-bearing): the M1 amendment requires a cost-rubric. Without it,
      M1 keeps recurring. Operator may accept this as known-pattern or escalate.
  
  (2) No Phase 1c equivalent exists for mdatron (no per-layer DESIGN.md with milestone
      acceptance criteria). mdatron's behavioral contracts live at project scope in
      DESIGN-MDATRON.md. Phase 2a-for-project-level-design-doc-style-projects is an
      adaptation analogous to the SA precedent's Phase-5-adapted-for-spec-stage from
      2026-05-27. Red Gate tests assert behaviors named in DESIGN-MDATRON sections rather
      than per-layer Phase 1c acceptance criteria.
sycophancy_compensation: |
  I (the inline-running author of both the rolled-back Step 3 commit AND this remediation
  composition) am biased to declare the composition "comprehensive enough." Compensation:
  every domain in composed_domains is justified by either an explicit phase-2a-primer rule
  (QE primary; baseline), an axes-driven activation (UX, Accessibility from ui-surface=yes;
  PE+PerfE from project-ships-code=yes), or an operator directive (AIE). No domain is
  composed by intuition.
```

---

## Phase 2a deliverables (this composition's scope)

Per DESIGN-MDATRON § Implementation order (Phase A foundational):

### Red Gate test suite

Per-module unit tests in `mdatron-core/src/<module>.rs` `#[cfg(test)]` blocks, asserting behavioral contracts from DESIGN-MDATRON. Tests fail-by-default against stub implementations (`todo!()` bodies in module source).

| Module | Tests assert (from DESIGN-MDATRON) |
|---|---|
| `diagnostic` | `Severity` enum has Error/Warning/Lint variants; `Severity::label()` returns rustc convention (`"error"` / `"warning"` / `"info"`); `Location::whole_file()` returns line 1 column 0; `Finding::format_tty()` produces rustc-style output with code, location, optional help, optional explain ref |
| `frontmatter` | `parse()` on typical frontmatter returns `Some((value, body))`; on missing frontmatter returns `None`; on empty frontmatter (`---\n---\n`) returns empty mapping + body; on missing closing marker returns `None`; on malformed YAML returns `Err`; on dashes-in-body without leading-newline does NOT match as closing |
| `error` | `Error` enum has variants for IO / YAML / JSON / Config; `Display` implementations produce useful messages (smoke test) |

### mdatron-cli skeleton

`mdatron-cli/src/main.rs` with `clap`-based `verify` subcommand. The `verify` subcommand:
- Accepts `--files <FILE>...` with `num_args = 1..` (multi-value per flag)
- For each file: read content; call `mdatron_core::frontmatter::parse`; report success/failure
- Exits 1 if any file fails; 0 otherwise

### Stub bodies (NOT implementation; Phase 2a deliverable)

All module function bodies are `todo!()` or trivial stubs that fail their tests. `cargo check` passes (project compiles); `cargo test` fails (Red Gate is red). Phase 2b turns the Red Gate green.

---

## Per-domain lens-application (proactive)

The inline-multi-domain composition means each domain's lens is applied without independent cold-session evidence. To compensate, this section names what each composed domain contributes:

| Domain | Lens contribution |
|---|---|
| **QE (primary)** | Authors Red Gate tests; ensures each test answers "what would have to be false for this test to fail?" per QE Dim 2 falsifiability. Names edge cases (empty/null/dashes-in-body/malformed-YAML) per Phase 2a primer |
| **SE** | Architecture-of-stubs review: ensures stub signatures match the contracts the tests assert (no test-stub-signature mismatch) |
| **SA** | Crate boundary review: confirms mdatron-core / mdatron-cli split matches BOUNDARY-PREAMBLE § 2 |
| **SO** | Scope-contract review: confirms Phase 2a scope stays within DESIGN-MDATRON-documented behaviors; no test asserts behavior outside the spec |
| **PE** | Dependency declarations review: 5 deps declared (serde, serde_yaml, serde_json, thiserror, clap); each requires `docs/dependencies/<crate>.md` per VSDD-E0100 — **deferred to Phase 2b** since dependency manifest changes belong to Phase 2b commit per Phase 2b primer line 35; this Phase 2a Red Gate uses workspace-declared deps without new entries |
| **PerfE** | Cargo `[profile.release]` settings: not declared at v0.1.0; flagged for Phase 2b consideration when ship-binary performance becomes load-bearing |
| **UX** | Diagnostic UX: `Finding::format_tty()` follows rustc convention (severity[code]: message + arrow + location). Tested |
| **Accessibility** | Output is TTY plain-text; works in any terminal. No color/emoji/special-chars in v0.1.0 outputs (color comes later via `colored` dep) |
| **AIE** | Compact format implementation pending (Phase 2b deliverable per AIE-S3-F1); token budget will pin in Phase 2b. Phase 2a Red Gate tests author the contract for compact format but compact format itself is Phase 2b implementation |

---

## Phase-completion criteria (Phase 2a closure)

Phase 2a closes (per primer) when:

- ✅ Per-module tests author behavioral contracts from DESIGN-MDATRON (this composition's deliverable)
- ✅ `cargo check` returns 0 (project compiles with stub bodies)
- ✅ `cargo test` returns non-zero (Red Gate is red — tests fail against stubs)
- ⚠️ Draft PR is open for the milestone — **deferred during bootstrap** since the per-milestone-PR discipline assumes mdatron-cli (the validator) is operational, which it is not yet (Phase 2a IS the bootstrap of mdatron-cli itself)
- ⚠️ `manual-tests/layer-N.md` checklist — **deferred during bootstrap**; mdatron's `manual-tests/error-catalog/<code>/` fixtures land at Phase A.4 per V1-SHIP-CRITERIA

Two deferrals (draft PR + manual-tests checklist) are noted as known gaps; the bootstrap period's audit trail records them rather than silently skipping.

The Phase 2a commit emits manual `PhaseExited{phase: phase-2a, exit_status: complete}` annotation in commit message (per BOOTSTRAP-MITIGATION § Mitigation 3 — tooling for event emission does not yet exist).

---

## Cross-references

- [vsdd-cli/.claude/commands/vsdd-phase-2a.md](../.claude/commands/vsdd-phase-2a.md) — phase primer (consumed)
- [vsdd-cli/.claude/commands/vsdd-domain-quality-engineer.md](../.claude/commands/vsdd-domain-quality-engineer.md) — primary domain prompt
- [BOOTSTRAP-MITIGATION.md](../BOOTSTRAP-MITIGATION.md) — bootstrap discipline this composition operates under
- [V1-SHIP-CRITERIA.md](../V1-SHIP-CRITERIA.md) § Phase A — foundational deliverables sequence
- [DESIGN-MDATRON.md](../DESIGN-MDATRON.md) — behavioral contracts the Red Gate tests assert
- [review-log/2026-06-01-ai-engineer.md](../review-log/2026-06-01-ai-engineer.md) — AIE mitigation review informing this composition
