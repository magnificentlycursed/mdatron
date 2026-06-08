---
schema_class: review-entry
schema_version: 1.0.0
review_number: 3
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session SA review — Phase 2 of binary-
  first plan: architectural fit of mdatron-cli/src/explain/ as a sub-module,
  format_tty as single source of truth, README's positioning of mdatron vs
  vsdd, Phase 1c single-milestone decomposition, binary-first-plan row 14
  drift.
lens: >-
  Solution Architect (decomposition + cross-boundary integrity + future-
  proofing). Five-lens weighting: Consistency 5, Maintainability 5, Edge-
  cases 3, Usability 2, Attacker 1. Cold-session reviewer mode.
source: director-raised
session_note: >-
  Cold-session reviewer mode. Sycophancy compensation: Claude authored the
  binary-first plan + the Phase 2 specs + the Phase 2 implementation.
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  Looking for "the spec author and implementer agreed because they're the
  same person" failure modes. Treating the binary-first-plan.md and the
  Phase 1a spec as if they were written by different people who could
  reasonably disagree.
---

# Findings

## F1 — explain catalog lives in mdatron-cli but `explain_ref` is set by mdatron-core (Consistency)

**Classification:** deferred
**Routing:** SA + SE pair; Phase 4 binary-first-plan candidate.

The Phase 2 catalog lives at `mdatron-cli/src/explain/`; the `explain_ref`
field on Finding is populated at `mdatron-core/src/verify.rs:250, 283`. The
two-crate boundary makes mdatron-core "knows the catalog has E0001 / E0050"
implicit — there's no compile-time check that an emission site's
`explain_ref` value resolves to a catalog page. Today's drift surface:
mdatron-core could be updated to emit a new code (e.g., E0060) without the
catalog catching up; `mdatron explain MDATRON-E0060` then says "not in
v0.1.0 baseline catalog" silently in production.

Phase 4 of the binary-first plan collapses the workspace into a single
crate; the natural Phase 4 follow-up is to put the catalog in `src/explain/`
and add a compile-time `assert!` (via const fn or build script) that every
`explain_ref` literal resolves to a catalog file. v0.1.x scope.

The Phase 1a spec acknowledges this implicitly ("Phase 4 of the binary-first
refactor collapses to single crate; the explain-catalog files survive the
move unchanged"). Naming explicitly: the cross-crate emission/catalog
asymmetry IS the v0.1.0 drift surface.

## F2 — `Finding::format_tty` refactor is a breaking change for any external mdatron-core consumer (Consistency)

**Classification:** dismissed (with note)
**Routing:** SA-internal.

Phase 2c's `format_tty` refactor:
- Before: `<label>[<code>]: <message>\n  --> <file>:<line>` + optional help/explain
- After: `<label>[<code>]: <summary>\n  --> <file>:<line>\n   = note: <message>`
  + optional help/explain

For any external Rust crate that depends on `mdatron-core` and calls
`format_tty`, this is a string-shape break. mdatron-core is a workspace-
internal crate today (not published; only mdatron-cli depends on it). So no
external consumer exists, no breaking change in practice. Once Phase 4
collapses the workspace, mdatron-core ceases to exist as a separate crate
and the question becomes moot.

The note: future-proofing is fine, but explicit "this is internal-only until
Phase 4" callout in mdatron-core's lib.rs would help anyone tempted to
reach into the crate via path-dep. Not blocking Phase 2.

## F3 — Phase 2c was supposed to refactor format_tty per the spec; it was done in 2b (Methodology drift)

**Classification:** accepted-with-remediation
**Routing:** vsdd-methodology meta-review.

Phase 1a § "`--json` finalization (TTY explain line)" says: "Use
`Finding::format_tty()` directly rather than the open-coded format in
`print_finding` — single source of truth for TTY rendering." The Phase 1c
acceptance criteria say this is a 2b deliverable ("`Finding::format_tty()`
is the single source of truth for TTY diagnostic rendering at the CLI
layer"). The implementation actually:

- Phase 2b: added the explain line to `print_finding` (open-coded, kept
  divergence)
- Phase 2c: switched `print_finding` to delegate to `format_tty` + made
  format_tty match the rustc-shape print_finding used

Methodology-wise, this is fine — Phase 2c is the refactor phase, and the
consolidation IS the refactor. But the Phase 1c acceptance criteria phrasing
implied 2b would land both. Minor spec-implementation alignment slip; the
durable record (this review entry + the eventual commit boundary) names
which phase did what.

Phase 4 routing: amend Phase 1c's acceptance-criteria phrasing to "2b OR 2c"
for refactor-shaped items, OR amend the 2c spec to explicitly call out the
format_tty consolidation. Either is fine.

## F4 — README forward-references `mdatron-examples` library without anchor (Consistency)

**Classification:** accepted-with-remediation
**Routing:** SA + TW pair.

README's "Relationship to vsdd" section says: "The mdatron-examples library
(v1.0 roadmap) ships four generalized artifact-class schemas (DESIGN doc,
manual-test, PR template, CHANGELOG) for non-VSDD adopters." This is true per
DESIGN-MDATRON.md, but the README doesn't link the canonical anchor.
Drive-by: `[mdatron-examples library](./DESIGN-MDATRON.md#mdatron-examples-library)`
would lock the cross-doc reference + survive future doc moves.

## F5 — Phase 1c "single milestone" rationale doesn't acknowledge the test-file-bundle as cross-cutting infrastructure (Decomposition)

**Classification:** dismissed
**Routing:** SA-internal.

Phase 1c § Decomposition rationale defends single-milestone shape on three
grounds (shared verification surface, shared release boundary, shared Phase
3 review). All three are correct. The bundling rationale does NOT explicitly
name the cross-cutting infrastructure point: `cli_integration.rs` itself is
new infrastructure that any of the four bundled changes would need to
introduce. Splitting milestones would force the question "which change
adds the test file?" without producing a clean answer.

Adding this to the rationale would strengthen it; not material.

## F6 — Integration tests rely on `target/debug/mdatron` path; survives Phase 4 collapse? (Maintainability)

**Classification:** accepted-with-remediation
**Routing:** SA + PE pair; Phase 4 candidate.

`cli_integration.rs:mdatron_bin()` resolves the binary via `CARGO_MANIFEST_DIR
+ "../target/debug/mdatron"`. After Phase 4 collapses `mdatron-core` +
`mdatron-cli` into a single `mdatron` crate, the manifest_dir IS the crate
root and `target/debug/mdatron` is at `target/debug/mdatron` directly under
the crate root — same path resolution, no change needed. Verified by
walking the resolved path through the post-Phase-4 layout: same.

So the test survives Phase 4 without changes. Documenting this here so the
Phase 4 work doesn't fall into the "must update test paths" failure mode.

## F7 — explain catalog stops at 5 codes but DESIGN-MDATRON.md reserves ~30 codes (Consistency)

**Classification:** deferred
**Routing:** SO via Raise-to-SO.

DESIGN-MDATRON.md § Reserved mdatron codes lists ~12 ranges of MDATRON-Exxxx /
Wxxxx / Lxxxx codes. The Phase 2 explain catalog covers 5: E0001, E0002,
E0050, E0070, E0080. The catalog-grows-per-emission discipline (Phase 1a
spec) means the gap is fine for v0.1.0 — only emitted codes need pages, and
the rest are reserved-for-future. But the gap isn't visible from inside the
README or the explain subcommand.

Operator-facing affordance candidate: `mdatron explain --list` enumerates
the catalog. Defer to v0.1.x; out of Phase 2 scope. Raised here for
the next planning cycle.

## F8 — binary-first-plan.md row 14 still says "Strip" — Phase 1c required amendment (Spec drift)

**Classification:** resolved-pending
**Routing:** SO + DR pair; Phase 2 closing-commit work.

Phase 1c acceptance criteria § "`binary-first-plan.md` row 14 updated to
reflect the SO disposition reversal — old text 'Strip' replaced with
'Implement explain catalog + retain line'; footnote names the 2026-06-02 SO
disposition + the `OperatorDirectiveApplied` event emission discipline
applied here."

Status at this review: `binary-first-plan.md:78` still reads "Strip the dead
`= explain:` line". The amendment has NOT been applied. This is in scope
for Phase 2's closing commit; calling it out here so it isn't forgotten.

# Cross-finding cohesion

F1 + F7 are both "the catalog/emission asymmetry will surface as a v0.1.x
drift". F2 + F6 are both "Phase 4 considerations the Phase 2 work passes
forward without breakage". F3 is the methodology meta-finding; F8 is the
durable-record-of-amendment-pending finding.

# Summary

8 findings: 1 resolved-pending (F8 row 14 amendment), 4 accepted-with-
remediation (F3, F4, F6, F8), 2 deferred (F1, F7 → SO/Phase 4),
1 dismissed-with-note (F2), 1 dismissed (F5). Architectural shape of Phase
2 is sound; the drift surfaces are forward-routed cleanly.
