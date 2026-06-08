---
schema_class: review-entry
schema_version: 1.0.0
review_number: 15
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session Localization review —
  Phase 2 prose surfaces: explain catalog (English-only), README
  (English-only), TTY diagnostic messages (English; rustc-shape
  convention).
lens: >-
  Localization (translation-readiness + locale-bound text + i18n
  framework presence).
source: director-raised
session_note: >-
  Cold-session reviewer mode. Honest scope-dismissal pattern: mdatron is
  English-only by methodology-spec discipline; checking whether Phase 2
  introduces additional locale-bound content that could ship at v1.x.
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  The easy dismissal ("everything is English; nothing to do") masks the
  honest discussion of translation-readiness. Checking whether the prose
  surfaces are structured for future translation.
---

# Findings

## F1 — Domain-wide honest scope-dismissal: mdatron is English-only by spec (Edge-cases)

**Classification:** dismissed-domain-wide
**Routing:** Localization-internal observation.

DESIGN-MDATRON.md does not mandate i18n; the toolkit ships English-only
strings (diagnostics, explain pages, README, log lines). VSDD's methodology
spec follows the same posture. Phase 2 introduces:
- 5 explain pages: English prose (Mentor voice)
- README: English prose
- TTY `= explain:` line: English structural keyword ("explain")

None of these introduces a new translation surface that wasn't already in
mdatron's pre-Phase-2 prose set.

## F2 — Explain pages use second-person voice; translates well to most locales (Consistency)

**Classification:** dismissed (with note)
**Routing:** Localization-internal observation.

The Mentor voice ("Open the file...", "Pass `--project-root`...") translates
cleanly to most Indo-European languages with explicit subjects. Some East
Asian languages prefer omitted subjects or formality registers; a future
translation effort would need editorial discretion per target locale. Not
a v0.1.0 concern.

Note: the explain catalog uses `**Severity:**` / `**Introduced in:**` as
structured frontline labels. These are translatable. The semantic keys
(`error`, `warning`, `lint`, `accepted`, `candidate`, `deprecated`) are
machine-parsed enums — not translatable by design. Pass.

## F3 — No findings at this layer (Domain-wide)

**Classification:** dismissed-domain-wide
**Routing:** Localization-internal closeout.

Substantive localization-domain findings for Phase 2: zero. The English-
only posture is consistent with mdatron's spec discipline (and matches
rustc / cargo / clippy convention — Rust's CLI tooling is English-only;
documentation translation happens at the docs-rendering layer, not in
the diagnostic surface).

If v1.x ships a translation framework (per DESIGN-METHODOLOGY.md if such
amendment lands), the Phase 2 prose structure (Mentor voice + structured
frontlines + per-code page model) translates per-page rather than as a
monolith — translation-friendly by construction. Forward-friendly.

# Summary

3 findings: 0 resolved-pending, 0 accepted-with-remediation, 3 dismissed
(F1 + F2 + F3 — all domain-scope-honest). No hallucinations. The
domain-wide dismissal carries substantive rationale.
