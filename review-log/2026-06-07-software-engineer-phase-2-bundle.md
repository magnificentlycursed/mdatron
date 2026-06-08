---
schema_class: review-entry
schema_version: 1.0.0
review_number: 1
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session SE review — bundled Phase 2 of
  binary-first plan: mdatron explain catalog (5 baseline codes), TTY `=
  explain:` line render, Finding::format_tty refactor to rustc-shape, mdatron
  README first-author, mdatron-cli/tests/cli_integration.rs (18 tests).
  Primary files: mdatron-cli/src/main.rs, mdatron-cli/src/explain/mod.rs,
  mdatron-cli/src/explain/MDATRON-E*.md, mdatron-core/src/diagnostic.rs,
  mdatron-cli/tests/cli_integration.rs, mdatron/README.md.
lens: >-
  Software Engineer (idiomatic Rust + error handling + test discipline).
  Five-lens weighting: Maintainability 5, Edge-cases 4, Consistency 5,
  Usability 2, Attacker 1. Cold-session reviewer mode.
source: director-raised
session_note: >-
  Cold-session reviewer mode per Phase 3 primer + 2026-06-02 cluster-batched
  operator-directive. Sycophancy compensation active: Claude authored every
  line under review.
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  Author is Claude across all touched files. Stance: assume the format_tty
  refactor introduced subtle drift until proven otherwise; assume the
  explain-catalog `match` is a v0.1.x premature concern unless a Phase 5
  fuzz target lands. Findings name what the better version looks like.
---

# Findings

## F1 — `cmd_explain` page-print uses `print!` with a conditional newline (Edge-cases, Maintainability)

**Classification:** accepted-with-remediation
**Routing:** SE-side fix; minor.

`mdatron-cli/src/main.rs:cmd_explain` uses `print!("{page}")` then conditionally
appends `println!()` when the page doesn't end in `\n`. Three concerns:

1. The five embedded pages all end with `\n` today (each file ends with a
   trailing newline per Unix convention). The conditional newline is dead at
   v0.1.0. Removing the check + always `println!("{page}")` is fine — markdown's
   trailing newline is non-semantic.
2. If a future page is authored without a trailing newline, the conditional
   adds one — useful. But the symmetric concern (page ending in `\n\n`)
   produces a blank line; not handled.
3. The `print!` + manual newline pattern is the kind of subtle thing that
   slips when refactored. `print_page(page: &str)` helper that owns the
   normalization would surface the invariant.

Corrective pattern: `print_explain_page` helper that trims trailing whitespace
+ writes exactly one trailing newline. ~5 LOC.

## F2 — `explain::lookup` is a 5-arm `match`; consider sorted slice for extensibility (Maintainability)

**Classification:** dismissed
**Routing:** SE-internal.

`mdatron-cli/src/explain/mod.rs:lookup` is a 5-arm `match` against literal
codes. A `&[(&str, &str)]` sorted slice + `binary_search_by_key` would scale
better to a 50-code catalog. At 5 codes the `match` is more readable + the
compiler likely emits a jump-table anyway. Convert if the catalog grows past
~20 entries; not now.

The Phase 1b purity-boundary lists `lookup` as a Phase 5 property-test
candidate. A property test asserting `all_codes()` matches `lookup`'s actual
hit set would also catch a `match`-arm typo at test time. The `all_codes()`
accessor in the Phase 1b spec is NOT actually implemented in the catalog
module — only `lookup` and `is_mdatron_namespace`. Drive-by drift: align
implementation with the Phase 1b spec or amend the spec.

## F3 — `is_mdatron_namespace` is a one-line `starts_with` wrapper (Maintainability)

**Classification:** dismissed (with note)
**Routing:** SE-internal.

`mdatron-cli/src/explain/mod.rs:is_mdatron_namespace(code)` returns
`code.starts_with("MDATRON-")`. Inlining the wrapper at the call site would
trim a function. Counterpoint: the wrapper names the invariant ("does this
code belong to mdatron's namespace") at the call site, making `cmd_explain`'s
control flow readable. Keep the wrapper.

## F4 — `format_tty` now writes `= note: <message>` even when message duplicates summary (Consistency)

**Classification:** accepted-with-remediation
**Routing:** SE-side; QE pair for regression test.

`mdatron-core/src/diagnostic.rs:format_tty` (refactored in Phase 2c) always
writes `= note: {self.message}` after the location line. For findings where
`summary == message` (i.e., the emission site sets both to the same string)
this produces a redundant second line:

```
error[MDATRON-E0050]: frontmatter-schema-violation
  --> bad.md:1
   = note: frontmatter-schema-violation
```

Today's emission sites differentiate: verify.rs:273 sets summary =
`"frontmatter-schema-violation"` and message = the schema-validation error
text. So duplication doesn't fire on the current corpus. But a future call
site that constructs a Finding with summary == message gets the redundant
line. Cheap fix: skip the `= note:` line when `self.message == self.summary`.
Test fixture trivial.

## F5 — `print_finding` now delegates to `format_tty` but `print_pipeline_error` stays open-coded (Consistency)

**Classification:** accepted-with-remediation
**Routing:** SE-side; v0.1.x candidate.

Phase 2c consolidated `print_finding` onto `format_tty`. `print_pipeline_error`
at `mdatron-cli/src/main.rs:185-187` is still open-coded:

```rust
eprintln!("error[MDATRON-E0080]: verify pipeline failed\n   = note: {e}");
```

It uses the same rustc shape but hand-rolls it. Two paths to consolidate:

1. Construct a Finding internally for the pipeline error + delegate to
   `format_tty`. Adds a `Location::nowhere()` constructor (file = "", line
   = 0). Symmetric with `Location::whole_file`.
2. Extract a `format_simple_diag(code, message)` helper in diagnostic.rs.

Option 1 is more uniform but bends the Finding shape (no location); option 2
is honest about the simple-shape case. Pick one for v0.1.x; doesn't block
Phase 2.

## F6 — Multi-line messages will break rustc-shape layout (Edge-cases)

**Classification:** accepted-with-remediation
**Routing:** SE-side; QE pair for fixture.

`format_tty` interpolates `self.message` verbatim into `= note: {message}`. If
the message contains an embedded newline, the second line of the message
loses its rustc-style indentation:

```
error[MDATRON-E0050]: frontmatter-schema-violation
  --> bad.md:1
   = note: Additional properties are not allowed
extra: not allowed  ← unindented; visually a top-level diagnostic
```

The JSON Schema validator's messages today are single-line (verified by
`output_format.rs` integration tests' fixture). But a future validator that
produces multi-line errors (e.g., pattern-match-by-anyOf) breaks the layout.
Corrective pattern: indent every line after the first by "   = note: ".len()
spaces. ~3 LOC change in `format_tty`.

Existing format_tty tests pass any-substring asserts that wouldn't catch this
regression. Add a fixture in diagnostic.rs#[cfg(test)] with an embedded `\n`
in the message; assert the second line is indented to match the first.

## F7 — `extract_fence` test helper allows nested fences (Edge-cases)

**Classification:** dismissed (with note)
**Routing:** SE-internal; test-only.

`cli_integration.rs:extract_fence` finds the first ` ```<lang>` opening then
the next ` ``` ` closing. Nested triple-backtick blocks (e.g., a markdown
example containing a code block) would terminate at the inner fence. The
README's current fences don't nest, so the helper works today. If a future
README has a nested fence, the round-trip test fails noisily — which is
correct behavior (test author addresses the README, not the helper).

## F8 — `walk_rs` in `output_format.rs` parallels `walk_files` in `phase_1_contracts.rs` (Maintainability)

**Classification:** deferred
**Routing:** SA via cross-test-helper extraction.

`mdatron-cli/tests/output_format.rs:walk_rs` walks `.rs` files; the Phase 1
contracts test has the same shape (`walk_files` with extensions). Three test
files now have similar filesystem-walking helpers. Phase 4 of the binary-first
plan collapses the workspace into single crates; the natural Phase 4
follow-up is a shared `tests/common/mod.rs` exposing these helpers. Not now;
single-milestone scope.

# Cross-finding cohesion

F4 + F6 both arise from `format_tty` lacking a "normalize message for
multiline + duplicated summary" pass. A small `format_note_body` helper in
diagnostic.rs would close both. F2 names the Phase 1b spec/impl drift on
`all_codes()` — that's the Phase 4-routing kind of finding (route back to
Phase 1b to amend or to Phase 2b to implement). F1, F3, F5, F7, F8 are
SE-internal.

# Summary

8 findings: 0 resolved-pending, 4 accepted-with-remediation (F1, F4, F5, F6),
3 dismissed-with-note (F2, F3, F7), 1 deferred (F8). Phase 2's core surfaces
(explain catalog, TTY explain line, README, integration tests) match the
Phase 1a/1b/1c specs. No hallucinations.
