---
schema_class: review-entry
schema_version: 1.0.0
review_number: 11
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session AIE review — Phase 2
  surfaces from the agent-loop angle: explain pages as LLM context,
  `= explain:` line as agent affordance, README's onboarding fitness
  for AI-driven workflows, catalog-grows-per-emission impact on agent
  diagnostic interpretation, PostToolUse hook composition.
lens: >-
  AI Engineer (LLM-context discipline + agent-loop affordances +
  prompt-context cost + structured-output friendliness). Five-lens
  weighting: Usability 5, Consistency 4, Maintainability 3,
  Edge-cases 3, Attacker 1.
source: director-raised
session_note: >-
  Cold-session reviewer mode. AIE-domain stance: every artifact will be
  read by an LLM at some point; how does it perform under that read?
  Sycophancy compensation: Claude is the LLM author + the LLM consumer
  in the test fixtures — easy to fool oneself about agent ergonomics.
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  Treating the explain pages as if a cold-context LLM reads them mid-loop
  to decide remediation. What ambiguity surfaces? What's missing?
---

# Findings

## F1 — `= explain:` line is an excellent agent affordance (Usability)

**Classification:** dismissed (with note)
**Routing:** AIE-internal observation; load-bearing positive.

The TTY explain line: `   = explain: mdatron explain MDATRON-E0050`. From
an agent-loop perspective, this is exactly the affordance the PostToolUse
hook design (DESIGN-MDATRON.md § Agent integration) calls for:

1. Self-describing — the command to run is in the diagnostic
2. Discoverable — no prompt-engineering needed; the agent reads the line
   and knows to invoke
3. Composes with `--json` — agents that parse JSON see the explain_ref
   field; agents that read stderr see the line

Phase 2 lands this affordance for real. The AIE F1 hypothesis from
earlier sessions ("agent-loop integration needs the diagnostic to tell
the agent what to do next") is now operational. Positive load-bearing
outcome.

## F2 — Explain pages are 50-line markdown; ~500-1000 tokens per page (Consistency)

**Classification:** accepted-with-remediation
**Routing:** AIE-side; v0.1.x.

Each explain page is ~50 lines of markdown. In a PostToolUse hook context
where the page is appended to the tool result, 5 findings each with
explain context would add 2500-5000 tokens to the agent's next inference
step. Manageable but not free.

The DESIGN-MDATRON.md § Compact format design ships a separate compact
format for high-density operator + agent scenarios. The Phase 2 catalog
ships in markdown only. v0.1.x candidate: a compact-form catalog (one
line per code: "MDATRON-E0050 frontmatter-schema-violation: add the
missing field OR remove the extra one") for agent-loop hot paths. Full
markdown stays the default for `mdatron explain CODE`.

## F3 — Explain pages don't cite which `relevant_domains` would help fix the code (Usability)

**Classification:** accepted-with-remediation
**Routing:** AIE + SO pair; v0.1.x.

For an agent operating in VSDD's domain-as-skill mode, knowing "which
domain's lens would help fix this finding" is load-bearing. E0001
(frontmatter-parse-failed) is QE/SE territory; E0050 (schema-violation)
is QE; E0080 (pipeline failure) is PE.

The explain pages don't carry domain-routing hints. Adding a per-page
"Suggested domain to route to:" line under "## How to fix" would:
- Give vsdd-cli's wrapper a fact to surface to the next-level agent
- Let the operator know which domain's skill to load for the fix

This is VSDD-specific value; mdatron is methodology-agnostic. So the
hint would live in vsdd-cli's vsdd-shaped catalog layered atop mdatron's
catalog, not in mdatron itself. v0.1.x: vsdd-side catalog overlay.

## F4 — `cli_integration.rs` uses `target/debug/mdatron` — agent-loop runs in sandboxes can't find this (Edge-cases)

**Classification:** accepted-with-remediation
**Routing:** AIE + PE pair; overlaps with PE F5.

Same finding as PE F5 from the agent angle. Agent-loop integrations
(PostToolUse hook + delegate protocol) spawn mdatron via PATH lookup, not
hard-coded target paths. The integration tests' hard-coded path means a
sandboxed agent runner that runs `cargo test` against the mdatron repo
finds the binary; an agent that wants to verify mdatron behavior via
arbitrary file paths (e.g., a vsdd-side test fixture) can't reuse the
test helpers.

The fix (CARGO_BIN_EXE_mdatron) addresses both PE's CI concern + AIE's
agent-loop sandboxing concern.

## F5 — README's "Where to go next" includes `mdatron explain <code>` — implicit instruction to the agent (Usability)

**Classification:** dismissed (with note)
**Routing:** AIE-internal observation.

The README's "Where to go next" section says:

> `mdatron explain <code>` — per-code prose for any emitted diagnostic;
> the v0.1.0 baseline catalog covers `MDATRON-E0001`, `E0002`, `E0050`,
> `E0070`, `E0080`; the catalog grows by one entry per newly-emitted code

For an agent reading this section to onboard a new mdatron-adopting
project, the instruction is clear: "when you see a code in a diagnostic,
invoke explain to get more context." Good. The five baseline codes are
named explicitly so the agent knows the v0.1.0 coverage horizon.

Pass; the README's prose is agent-friendly already.

## F6 — `not in catalog` message doesn't tell the agent what to do next (Usability)

**Classification:** accepted-with-remediation
**Routing:** AIE + UX pair; overlaps with UX F5.

When `mdatron explain MDATRON-E0010` returns "no explain page found", the
message says "the explain catalog grows by one entry per emitted code;
MDATRON-E0010 is not in the v0.1.0 baseline catalog."

An LLM agent reading this is left at "what now?" Possible next steps:
1. Look up DESIGN-MDATRON.md for the reserved-code table
2. File an issue requesting the catalog page
3. Look at the emitting code path in mdatron-core (if the source is
   available)

Adding even one explicit pointer ("see DESIGN-MDATRON.md § Reserved
mdatron codes for the structural meaning, or file an issue if you'd
like an explain page authored") helps the agent route correctly.

## F7 — `explain --json` is not in v0.1.0; agent-friendly format gap (Consistency)

**Classification:** deferred
**Routing:** AIE via Raise-to-SO; v0.1.x.

The Phase 1a spec § Edge cases declares `mdatron explain --json CODE` is
out of v0.1.0 scope (v0.1.x candidate). For agent-loop integration, JSON
output of the explain page (with parsed sections, structured Severity /
Status / Introduced-in fields) would be more directly consumable than
markdown.

Defer to v0.1.x with the rationale: markdown is human-readable + agent-
readable; the JSON form is a polish that doesn't unblock v0.1.0. The
deferred surface is named in Phase 1a spec; nothing new.

## F8 — The Phase 1a spec explicitly names "tool-author composing mdatron as a substrate" as a README audience — does the agent angle satisfy that? (Consistency)

**Classification:** dismissed (with note)
**Routing:** AIE-internal verification.

Phase 1a § "Required structural sections" names three audiences: "VSDD
downstream user", "mdatron-only adopter", "tool-author composer". The
agent-loop angle is implicit under "tool-author composer" — someone
building an automation around mdatron. README's "Relationship to vsdd"
section reads naturally for this audience (vsdd IS a tool composing
mdatron-as-substrate).

The README doesn't explicitly call out "agent-loop integration"
patterns (PostToolUse hook, compact format, skill-preamble) — those
live in DESIGN-MDATRON.md § Agent integration. The README's "Where to
go next" links DESIGN-MDATRON.md; the agent author finds the agent-loop
discipline one hop away. Acceptable for v0.1.0.

# Summary

8 findings: 0 resolved-pending, 4 accepted-with-remediation (F2 compact
catalog, F3 domain-routing hints in vsdd overlay, F4 CARGO_BIN_EXE, F6
not-found actionable), 3 dismissed-with-note (F1 positive observation,
F5 README agent-friendly, F8 audience coverage), 1 deferred (F7
explain --json → v0.1.x). The agent-loop affordance shape Phase 2 lands
(F1) is the load-bearing positive. No hallucinations.
