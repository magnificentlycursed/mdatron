---
schema_class: review-entry
schema_version: 1.0.0
review_number: 2
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session QE review — Phase 2 test
  surface: mdatron-cli/tests/cli_integration.rs (18 tests), explain catalog
  module-level tests, format_tty test discipline, README round-trip
  drive-by, Phase 1c Red Gate seed coverage.
lens: >-
  Quality Engineer (test pyramid + falsifiability + mutation survivability).
  Five-lens weighting: Edge-cases 5, Consistency 5, Maintainability 3,
  Attacker 2, Usability 1. Cold-session reviewer mode.
source: director-raised
session_note: >-
  Cold-session reviewer mode. Sycophancy compensation: Claude authored every
  test. Stance: assume each assertion would still pass after a plausible bug
  is introduced — if so, the assertion isn't load-bearing.
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  Trying hard to find tests that pass for the wrong reason. Default suspicion
  is "assertion is too permissive."
---

# Findings

## F1 — `readme_contains_seven_required_topic_headings` matches substrings, not headings (Edge-cases)

**Classification:** accepted-with-remediation
**Routing:** QE-side.

`cli_integration.rs:readme_contains_seven_required_topic_headings` lowercases
the README + calls `.contains("install")` etc. This passes if the substring
appears ANYWHERE — including in URL paths (`cargo install` link), prose
("install the binary"), or markdown link text.

Specific failure mode: if a future README removes the `## Install` heading
but retains an inline mention of `cargo install`, the test still passes.
The DR-F1 contract requires audience-coverage via topic headings; the test
should verify the heading exists, not the substring.

Corrective pattern: filter `readme.lines()` for lines starting with `#`,
then substring-check the heading-text. ~6 LOC change. Reduces false-pass
surface.

## F2 — `readme_pattern_example_round_trips` asserts "not exit 2" — too permissive (Edge-cases)

**Classification:** accepted-with-remediation
**Routing:** QE-side.

The round-trip test asserts the README's example doesn't crash the pipeline
(exit ≠ 2). It does NOT assert exit 0 (clean) or exit 1 (specific finding).
A README example that crashes Layer 2 with a non-pipeline error or emits a
silent no-op both pass.

The current README example is intentionally clean (the pattern fires when
title is empty; the example markdown has a non-empty title). The test should
assert exit 0 + zero findings + JSON parses + summary.error_count == 0.
Asserting the clean path locks the README's documented invariant.

Counter: if the README ever ships a "showing a failure case" example, the
test would need branching. Phase 2's README is single-example; tighten now.

## F3 — Five `explain_eXXXX_emits_catalog_page` tests share identical assertion shape (Maintainability)

**Classification:** dismissed (with note)
**Routing:** QE-internal.

Five tests differ only in the code argument. A single parameterized test
(loop over `&["MDATRON-E0001", ...]`) is denser. Counterpoint: five named
tests produce five distinct failure reports — when E0050's page rots,
`explain_e0050_emits_catalog_page` is the failing test, not "loop iteration
3 of explain_pages." Named-per-code is preferable for diagnostic clarity.
Keep as-is.

## F4 — `verify_tty_suppresses_explain_line_under_quiet` passes for the wrong reason today (Falsifiability)

**Classification:** dismissed
**Routing:** QE-internal.

The test asserts stderr does not contain `= explain:` under `--quiet`. With
Phase 2b's `print_finding` change, the rendered output DOES include the
explain line — but `--quiet` suppresses ALL of stderr. The test passes for
the right reason after Phase 2b lands. Pre-Phase-2b, the test passed
because the explain line was never rendered. The test name describes the
Phase-2-complete semantics; the assertion is correct in both world states.
Not a finding.

## F5 — No test asserts `cmd_explain` rejects `--json` flag (Edge-cases)

**Classification:** accepted-with-remediation
**Routing:** QE-side; v0.1.x candidate.

The Phase 1a spec § Edge cases declares `mdatron explain --json MDATRON-E0001`
is rejected at clap parse time for v0.1.0. The integration tests do NOT
assert this. The clap derive macros put `--json` only on the `Verify`
variant, so today the test would pass — clap rejects unknown flag — but
that's the wrong rejection mode (clap says "unrecognized argument", not the
spec-mandated "explain doesn't accept --json"). Either narrative is fine
for v0.1.0 but a test would lock the surface.

## F6 — Explain pages' format invariants are only tested inside the catalog module (Falsifiability)

**Classification:** accepted-with-remediation
**Routing:** QE-side.

`explain/mod.rs:every_baseline_code_has_a_catalog_page` asserts each page
contains `## What this means`, `## How to fix`, `**Severity:**`, `**Introduced
in:**`. The test runs inside `mod tests` — so it covers only what the catalog
module exposes. An integration test asserting the same via subprocess
(`mdatron explain CODE | grep '## What this means'`) would catch a regression
in `cmd_explain`'s page-print path (e.g., if a future change strips
heading-lines accidentally). cli_integration.rs DOES partially cover this
via `assert_explain_page_for` — which checks `## What this means`,
`## How to fix`, `**Severity:**`, `**Introduced in:**`. Good. (Retracting
the gap; the integration test does cover.)

Restatement: the catalog module test + integration test cover the same
invariants. Redundant on the surface but each catches a different regression
class (catalog module: package-build regressions; integration: subprocess
path regressions). Keep both.

## F7 — No test asserts `MDATRON-E0080` does NOT render `= explain:` when emitted from `print_pipeline_error` (Falsifiability)

**Classification:** accepted-with-remediation
**Routing:** QE-side; small.

`verify_tty_does_not_render_explain_line_when_explain_ref_is_none` asserts the
substring `= explain: mdatron explain MDATRON-E0080` is NOT in stderr after
a pipeline failure. The pipeline failure path (`print_pipeline_error`) is
hand-rolled and doesn't include explain_ref — the test passes today. But it
also passes if a future change adds explain_ref to the pipeline-error
emission path AND `print_pipeline_error` continues to ignore it. The test
shape is OK; just noting the asymmetric coverage.

## F8 — README round-trip extracts the FIRST yaml/json/md fence — README ordering is now load-bearing (Edge-cases)

**Classification:** accepted-with-remediation
**Routing:** QE-side; minor.

`extract_fence(readme, "yaml")` finds the first ```yaml fence. If a future
README author rearranges sections — e.g., showing a config.yaml snippet
BEFORE the pattern example — the round-trip test extracts the wrong fence
and the test fails with a confusing error. The failure surfaces, but the
implicit "first fence is the pattern" assumption is undocumented.

Corrective patterns:
1. Add HTML comment markers in the README around the round-trip fences:
   `<!-- mdatron-roundtrip:pattern --> ```yaml ... ``` <!-- /mdatron-roundtrip -->`
2. Or label fences via fence-info-string: ```yaml mdatron-pattern (not
   commonly supported by markdown renderers but valid)

Cheap; locks the contract. Not blocking for v0.1.0 — README is small +
unlikely to be reorganized soon.

## F9 — Subprocess spawn tests are non-hermetic w.r.t. installed binary version (Edge-cases)

**Classification:** dismissed
**Routing:** QE-internal.

`cli_integration.rs:mdatron_bin()` resolves to `target/debug/mdatron` — the
just-compiled binary, not the system-installed one. So test runs are
hermetic relative to whatever the workspace's build produced. Good. The risk
would be developers running tests after `cargo install --release` thinking
they're testing the release binary. They aren't. The `target/debug/` path
is intentional + correct for cargo test.

# Cross-finding cohesion

F1 + F8 are both "test is too permissive about README structure" — the
README is now a test fixture; treating it as such with explicit markers
would close both. F2 is the most material gap (round-trip asserts only
not-crash); tighten to clean-or-specific-diagnostic. F4 + F6 + F7 are
clarifications of test discipline; no action.

# Summary

9 findings: 0 resolved-pending, 5 accepted-with-remediation (F1, F2, F5,
F7, F8), 3 dismissed (F3, F4, F9), 1 dismissed-with-note (F6 retraction).
The Red Gate landed 18 failing tests matching the Phase 1c seeds; turning
them green did not introduce coverage gaps.
