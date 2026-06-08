---
schema_class: review-entry
schema_version: 1.0.0
review_number: 8
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session SC review — sanity-check
  validator-of-last-resort across Phase 2: catalog/range asymmetry,
  README first-run E0080 gotcha, print_finding/format_tty redundancy,
  E0080 page's two-failure-mode coverage, test-name vs behavior match.
lens: >-
  Sanity Check (rubber-duck + validator-of-last-resort). Five-lens
  weighting: Edge-cases 5, Consistency 4, Usability 4, Maintainability 3,
  Attacker 1.
source: director-raised
session_note: >-
  Cold-session reviewer mode. Sanity-check looks for "the simple obvious
  failure mode everyone missed because they were thinking about the hard
  cases." Sycophancy compensation: Claude authored the Phase 2 work;
  finding "is the obvious thing right?" is a check against
  over-engineered-but-broken outcomes.
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  Looking for "does mdatron explain MDATRON-E0001 actually work end-to-end?"
  shape concerns over architectural elegance.
---

# Findings

## F1 — 5 codes in catalog, 12+ ranges reserved — the gap is honest but invisible (Usability)

**Classification:** dismissed (with note)
**Routing:** SC-internal observation.

DESIGN-MDATRON.md § Reserved mdatron codes lists ~12 ranges (E0001-E0009,
E0010-E0019, etc.) totaling ~30+ codes potentially in scope. The v0.1.0
explain catalog covers 5 (E0001, E0002, E0050, E0070, E0080). The
"catalog-grows-per-emission" discipline (Phase 1a) means the rest only
land when their emission sites land — sensible.

The gap surfaces when:
1. An operator skims the DESIGN doc, sees E0010 (path-confinement-source-
   absolute-path) in the reserved table, runs `mdatron explain MDATRON-E0010`
   to learn more, gets "not in v0.1.0 baseline catalog"
2. Same for any of the other ~25 reserved-but-not-emitted codes

The "not in catalog" message is honest. It just doesn't help the curious
operator find the spec table. Add to the not-found message: "see
DESIGN-MDATRON.md § Reserved mdatron codes for the full table." v0.1.x.

## F2 — README "First run" sets expectation `mdatron verify: clean` but empty project crashes pipeline (Usability)

**Classification:** accepted-with-remediation
**Routing:** SC + TW pair; small.

README "First run": "A clean run prints `mdatron verify: clean` and exits
0."

An empty project (no `.mdatron/` directory) does NOT print clean — it
exits 2 with `error[MDATRON-E0080]: verify pipeline failed`. A first-time
adopter who runs `mdatron verify` without setting up `.mdatron/` first hits
the pipeline-fail path, sees the error, may bounce off.

The README's structure (`mkdir .mdatron/...` before `mdatron verify`)
prevents this — adopters who follow the README sequence don't hit the
issue. But a reader who skims to "First run" and tries `mdatron verify`
gets the wrong first impression.

Corrective pattern: README "First run" says "First-time adopters: ensure
`.mdatron/schemas/` exists (the prior section's `mkdir -p` covers this).
Running mdatron verify without `.mdatron/` exits 2 with `MDATRON-E0080`."
3 sentences; closes the gotcha. v0.1.x.

## F3 — `print_finding` now == `eprintln!("{}", f.format_tty())` — the wrapper is harmless redundancy (Consistency)

**Classification:** dismissed (with note)
**Routing:** SC-internal.

After Phase 2c's consolidation, `print_finding` is a 1-line wrapper that
calls eprintln! on format_tty's output. The wrapper:
- Pros: names the side-effecting CLI-time choice (eprintln vs return-string);
  isolates `f.format_tty()` from the binary edge
- Cons: trivially inlineable; `eprintln!("{}", f.format_tty())` at the call
  site is one less function

Inlining saves the indirection but loses the naming. Keep the wrapper.
This is the "obvious might-be-redundant" pass-through; the answer is "no,
it's fine."

## F4 — E0080 covers both "missing .mdatron/" AND "serialization failed" — different remediations (Usability)

**Classification:** accepted-with-remediation
**Routing:** SC + DR pair; small.

The E0080 page lists four common patterns under "How to fix":
1. Missing .mdatron/ directory → create it
2. Malformed schema/pattern file → validate out-of-band
3. Permission denied → check filesystem permissions
4. JSON serialization failure under --json → open an issue

#1, #2, #3 are configuration problems; #4 is an internal bug. The page
groups them, but the operator hitting case #4 is in a different mental
mode than case #1. They want to know "is this my fault?" The page's "(rare)
... this is an internal bug not a configuration problem" addresses this
nicely. Pass for the prose.

Minor refinement: the four causes could be split into "common (operator-
fixable)" and "rare (file an issue)" subsections. Sequencing helps. v0.1.x.

## F5 — Test name `verify_quiet_alone_suppresses_all_stderr_output` is accurate (Consistency)

**Classification:** dismissed
**Routing:** SC-internal verification.

The test name says "all stderr output"; the test asserts
`stderr.is_empty()`. Implementation matches description. The fixture (schema-
violation finding) produces non-empty stderr without --quiet (verified by
the parallel `verify_tty_renders_explain_line_for_finding_with_explain_ref`
test). With --quiet, stderr is empty. Behavior matches name. Pass.

## F6 — `mdatron explain MDATRON-E0001` end-to-end smoke (Edge-cases)

**Classification:** dismissed (with note)
**Routing:** SC-internal sanity check.

Manually invoking `mdatron explain MDATRON-E0001` at the shell:
- Exits 0 ✓ (cli_integration.rs:explain_e0001_emits_catalog_page asserts)
- Prints the E0001 page on stdout ✓
- Prints "## What this means" + "## How to fix" + Severity + Introduced
  in ✓

End-to-end works as documented. The rubber-duck pass — the actual user-
visible thing — passes. Confidence-building.

## F7 — Sanity check on the README round-trip: pattern actually fires? (Edge-cases)

**Classification:** dismissed
**Routing:** SC-internal sanity check.

The README's pattern asserts `$self.title != ""`. The README's example
markdown sets `title: hello mdatron` — non-empty. The pattern's `assert`
evaluates true; no finding fires. Test expects exit ≠ 2 (not-crashed).
Test passes. The pattern shape is honest (it would fire on `title: ""` or
omitted title), the example is honest (non-empty), the round-trip is
honest. Pass.

Drive-by question raised: should the README include a "failing example"
that DOES fire the pattern, so the operator sees the diagnostic shape?
F7 of the TW review surfaces this as accepted-with-remediation; the
sanity-check view confirms it'd be useful.

## F8 — `mdatron explain` (no args) — what does it do? (Edge-cases)

**Classification:** dismissed (with note)
**Routing:** SC-internal sanity check.

Tested at shell: `mdatron explain` (no code argument).

Result: clap-level error — "the following required arguments were not
provided: <CODE>" with usage output. Exit non-zero.

Behavior is correct (clap discipline) but the integration tests don't
cover it. The Phase 1a spec doesn't name it as a required test. Pass —
the rubber-duck case is fine.

# Summary

8 findings: 0 resolved-pending, 2 accepted-with-remediation (F2 README
empty-project gotcha, F4 E0080 page subsection split), 6 dismissed
(F1 catalog gap explainer, F3 print_finding wrapper, F5 test-name match,
F6 E0001 smoke, F7 round-trip honest, F8 no-args smoke). The "is the
obvious thing right?" pass returns confidence — Phase 2's surfaces
behave end-to-end as their tests + specs claim. No hallucinations.
