---
schema_class: review-entry
schema_version: 1.0.0
review_number: 12
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session UX review — Phase 2
  operator-facing surfaces: TTY diagnostic shape (rustc-style), explain
  command output, README first-encounter flow, error messaging for
  unknown-code path, copy-paste-ability of the explain affordance.
lens: >-
  UX (operator experience + cognitive load + first-encounter ergonomics).
  Five-lens weighting: Usability 5, Edge-cases 3, Consistency 3,
  Maintainability 2, Attacker 1.
source: director-raised
session_note: >-
  Cold-session reviewer mode. UX-domain stance: the operator's first 30
  seconds determine adoption fitness. What hurts in those 30 seconds?
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  Looking for "the moment the operator goes 'wait, what?' and bounces."
  The rustc-shape diagnostics + cargo conventions are familiar to most
  Rust devs; checking the cases where the conventions don't help.
---

# Findings

## F1 — `mdatron explain CODE` outputs raw markdown to terminal (Usability)

**Classification:** accepted-with-remediation
**Routing:** UX + TW pair; v0.1.x.

When the operator runs `mdatron explain MDATRON-E0001`, the output is the
literal markdown:

```
# MDATRON-E0001 — frontmatter-parse-failed

**Severity:** error
**Status:** accepted
**Introduced in:** 0.1.0

## What this means
...
```

A rustc `--explain` pretty-prints with ANSI bold for `**...**` and proper
heading rendering. mdatron emits the raw markdown. Operator sees `## What
this means` as a literal — readable, but the prose intent is partially
hidden under the markdown syntax.

Phase-cost analysis: pretty-print requires a markdown-to-ANSI library
(e.g., `termimad` or `bat`). Adds 1-2 deps + a few hundred LOC. v0.1.x
candidate; v0.1.0 raw-markdown is operator-readable + machine-readable
(pipe to `glow`, `mdcat`, `bat -l md`). Document the pipe pattern in the
README's "Where to go next" section to set the expectation.

## F2 — The `= explain:` line is copy-paste-friendly ✓ (Usability)

**Classification:** dismissed (with note)
**Routing:** UX-internal positive observation.

The diagnostic format: `   = explain: mdatron explain MDATRON-E0050`. The
operator can triple-click + copy + run. Matches rustc's `note: ... try
running` convention. Load-bearing positive — this IS the "copy-paste
ergonomic affordance" rustc landed in clippy.

## F3 — README "First run" section doesn't show what a sample diagnostic looks like (Usability)

**Classification:** accepted-with-remediation
**Routing:** UX + TW pair (overlaps with TW F7).

Same as TW F7. The README describes "stderr carries rustc-shaped
diagnostics" but doesn't show an example. A 6-line `mdatron verify`
fixture-output sample would:
- Show the new operator what the diagnostic looks like
- Demonstrate the `= explain:` affordance visually
- Anchor the "run mdatron explain CODE" advice with a concrete trigger

Small markdown block; v0.1.x polish.

## F4 — `mdatron explain MDATRON-E0010` (reserved-but-not-cataloged) returns confusing "not in catalog" (Usability)

**Classification:** accepted-with-remediation
**Routing:** UX + AIE pair (overlaps with AIE F6).

Today's not-found error:

```
error[MDATRON-E0080]: no explain page found for MDATRON-E0010
   = note: the explain catalog grows by one entry per emitted code;
           MDATRON-E0010 is not in the v0.1.0 baseline catalog
```

Operator interpretation: "the catalog is incomplete; this code is
reserved but not yet documented." OK so far. But the operator doesn't
know what to do — there's no pointer to the spec table where the code
IS documented at the architectural level.

Refinement: add a third line after the note: "   = help: see
DESIGN-MDATRON.md § Reserved mdatron codes for the structural meaning
of E0010 (path-confinement)." This is a small ergonomic win. The
implementation has to know the rough class of each unreserved code — or
generically pointer to the section header (good enough).

## F5 — Foreign-namespace path (`mdatron explain VSDD-E0207`) emits useful redirection ✓ (Usability)

**Classification:** dismissed
**Routing:** UX-internal positive observation.

The implementation says:

```
error[MDATRON-E0080]: VSDD-E0207 is outside the mdatron namespace
   = note: mdatron explain covers MDATRON-Exxxx codes only;
           see `vsdd explain VSDD-E0207` for the VSDD namespace
```

Operator scope is clear, the "what to run instead" is named, the
namespace separation is visible. Excellent UX.

(Side note: `vsdd explain` is itself a future feature — mentioned but
not yet implemented in vsdd-cli. The advisory message is forward-looking;
when vsdd-cli's explain subcommand lands, the message stays accurate.)

## F6 — README's first encounter: "what IS mdatron and why would I use it?" answered in sentence 1 (Usability)

**Classification:** dismissed (with note)
**Routing:** UX-internal positive observation.

README opens: "A Rust CLI for typed-markdown validation. Descended from
XML's Schematron..."

For an operator who lands on the README via search or a recommendation,
the sentence answers:
- What language is it? Rust
- What format does it process? Markdown
- What does it do? Validation
- What's the lineage / mental model? Schematron-style

In ~20 words. Good first-impression density. TW F1 raises differentiation
vs other markdown tools — that's a polish refinement; the v0.1.0 opener
is functional.

## F7 — `mdatron --version` works; standard cargo discipline ✓ (Consistency)

**Classification:** dismissed
**Routing:** UX-internal verification.

`mdatron --version` prints `mdatron 0.1.0`. Standard clap-derived output.
Pass.

## F8 — Pre-commit hook output is operator-readable when it fires (Usability)

**Classification:** dismissed
**Routing:** UX-internal positive observation.

Per the operator-verified end-to-end behavior in
project_mdatron_install_and_hook memory: "the hook caught a deliberate
three-finding fixture and blocked the commit with exit 1." The blocking
behavior + the rustc-shaped output gives the operator a clear "fix
these, then commit again" loop. Standard pre-commit ergonomic.

After Phase 2c the output now includes `= note:` and `= explain:` lines
per finding — slightly longer per-finding but more actionable. Net UX
improvement.

# Summary

8 findings: 0 resolved-pending, 3 accepted-with-remediation (F1 markdown
pretty-print, F3 README sample output, F4 not-found-helpful-pointer),
5 dismissed-with-note (F2 copy-paste positive, F5 namespace redirect
positive, F6 README opener positive, F7 --version positive, F8 hook
positive). Phase 2's UX shape is net-positive — three of five dismisses
are explicit positives. The polish opportunities are v0.1.x. No
hallucinations.
