---
schema_class: review-entry
schema_version: 1.0.0
review_number: 5
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session TW review — Phase 2 prose
  surfaces: mdatron README (190 lines, first-author), 5 explain catalog
  pages (~50 lines each), Phase 1a/1b/1c spec docs, integration-test
  assertion strings (operator-facing on failure).
lens: >-
  Technical Writer (voice + audience + clarity + Mentor-stance discipline).
  Five-lens weighting: Usability 5, Consistency 5, Maintainability 3,
  Edge-cases 2, Attacker 1.
source: director-raised
session_note: >-
  Cold-session reviewer mode. Sycophancy compensation: Claude authored every
  prose surface. TW domain's specific failure mode: "the prose reads well
  to its author but assumes prior-conversation context the reader doesn't
  have." Looking for orphan terms + missing onboarding bridges.
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  Treating the README as if a brand-new adopter (zero prior mdatron context)
  opens it cold. Treating each explain page as if seen for the first time
  when a diagnostic fires.
---

# Findings

## F1 — README first sentence captures install target, not differentiation (Usability)

**Classification:** accepted-with-remediation
**Routing:** TW + SO pair; minor.

README opens: "A Rust CLI for typed-markdown validation. Descended from XML's
Schematron (ISO/IEC 19757-3) and applied to markdown documents with YAML
frontmatter; not related to the TRON blockchain despite the `-tron` suffix."

The TRON-blockchain disambiguation (per TW-F3 / DESIGN-MDATRON.md:63) is
load-bearing — covered. The Schematron lineage is named — covered. The
**differentiator** vs other markdown linters (markdownlint, Vale, remark,
mdformat) is not. A reader who finds mdatron via search needs to know in
sentence 1: what does this do that other markdown tools don't?

Suggested replacement opening: "A Rust CLI that validates markdown
documents using JSON Schema (frontmatter) and a Schematron-derived DSL
(cross-field rules). Descended from XML's Schematron (ISO/IEC 19757-3);
not related to the TRON blockchain."

That captures the two-layer pitch + the lineage + the disambiguation in
two sentences. v0.1.x polish; not blocking Phase 2.

## F2 — Explain pages mix second-person and passive voice (Consistency)

**Classification:** accepted-with-remediation
**Routing:** TW-side; minor.

Spot check across the 5 pages:

- E0001 "How to fix": "Open the file at the reported line and review the
  frontmatter block." (second-person, imperative — Mentor voice ✓)
- E0050 "What this means": "The file's frontmatter parsed cleanly and
  bound to a known `schema_class`, but its content violates one or more
  rules..." (third-person, declarative — Formal voice)
- E0070 "How to fix": "Pass `--project-root` with an absolute path..."
  (second-person, imperative ✓)
- E0080 "What this means": "The verify pipeline failed to complete..."
  (third-person declarative)

The "What this means" sections lean declarative; the "How to fix" sections
lean imperative. That asymmetry is FINE — describing the state vs the
remediation are different rhetorical acts. The within-section consistency
matters more. Within a single "How to fix" section, mix is rare; pass.

One specific drift: E0080 "How to fix" mixes imperative ("Create
`.mdatron/schemas/`...") with declarative ("The accompanying `= note:` line
names the specific failure shape."). Both are correct but jarring next to
each other. Smooth with: "Read the accompanying `= note:` line for the
specific failure shape; then apply the matching corrective pattern below."

## F3 — README's ```markdown fence is non-portable; prefer ```md (Consistency)

**Classification:** accepted-with-remediation
**Routing:** TW-side; trivial.

README uses ` ```markdown` for the example post and ` ```yaml` for the
pattern. The CommonMark spec accepts any info string; `markdown` is widely
recognized but `md` is more portable in older markdown renderers. The
integration test (`extract_fence("markdown") or extract_fence("md")`)
handles both. Future README author may use `md`; both work. Document the
convention in a top-level edit-guide (or simply pick one). v0.1.x.

## F4 — README "Where to go next" hyperlinks rot if mdatron is read standalone (Cross-doc)

**Classification:** accepted-with-remediation
**Routing:** TW + SA pair; v0.1.x.

README links:
- `[vsdd-cli](../vsdd-cli/)` — relative path assumes the
  magnificentlycursed monorepo layout
- `[DESIGN-MDATRON.md](./DESIGN-MDATRON.md)` — sibling-file, fine
- `[V1-SHIP-CRITERIA.md](./V1-SHIP-CRITERIA.md)` — sibling-file, fine
- `https://gist.github.com/dollspace-gay/...` — absolute, fine

The `../vsdd-cli/` link breaks when:
1. mdatron is published as a standalone crate (the vsdd-cli source isn't
   adjacent on the reader's machine)
2. Someone reads the README on GitHub from the mdatron repo's main page
   (GitHub renders `../` as a parent-org-relative path that 404s)

Phase 6 of the binary-first plan publishes mdatron to crates.io and likely
splits the repos. Replace the relative link with an absolute GitHub URL
pinned to a stable ref (per the DESIGN-MDATRON.md § Cross-repo citation
discipline: version-pinned URLs only). At v0.1.0 with the monorepo intact,
the link works; flagging for the Phase 6 follow-up.

## F5 — Explain page E0080 names "JSON serialization failure" as an internal bug — Mentor voice ✓ (Consistency)

**Classification:** dismissed (with note)
**Routing:** TW-internal verification.

E0080 "How to fix" closes with: "JSON serialization failure under `--json`
(rare). Open an issue with the finding payload that caused the failure; this
is an internal bug not a configuration problem."

This is exemplary Mentor-voice: names the failure mode, gives the
operator a clear action (open an issue, not "try harder"), distinguishes
operator-fixable from internal-bug. Verifies the prose discipline is
healthy. Pass.

## F6 — Phase 1a spec's "audience-served" table is dense (Maintainability)

**Classification:** dismissed
**Routing:** TW-internal.

Phase 1a spec § "Required structural sections" has a 4-column table:
Section / Audience-served / Why required. The "Audience-served" column
lists "All" or "mdatron-only adopter + tool-author" with comma-separated
audiences. Future-spec-reader can parse this, but a denser table is harder
to skim. Not blocking; would prefer one-audience-per-row but the
multi-audience cases collapse cleanly.

## F7 — README's "First run" section doesn't preview clean-vs-dirty TTY output (Usability)

**Classification:** accepted-with-remediation
**Routing:** TW-side; minor.

README "First run" describes the run result but doesn't show a sample TTY
output. Adding ~5 lines of sample output (clean run + an example
diagnostic with the `= explain:` line visible) would:
- Demonstrate the rustc-shaped diagnostic visually before the reader
  encounters one in the wild
- Let the reader confirm their first run "looks right" (vs "is something
  broken with my install?")
- Showcase the `= explain:` line — which is otherwise undocumented in
  the README outside the implicit `mdatron explain CODE` mention

Trivial addition (~5 lines).

## F8 — README "Relationship to vsdd" calls vsdd "the first downstream adopter" — accurate but doesn't position mdatron's value for non-VSDD adopters (Usability)

**Classification:** accepted-with-remediation
**Routing:** TW + SO pair; v0.1.x.

README: "vsdd is the first downstream adopter of mdatron and the source
of the methodology vocabulary." True. The section then says "mdatron itself
is methodology-agnostic" — true. But a non-VSDD adopter scanning the README
sees a lot of VSDD-named content before reaching the agnostic claim.

A future README enhancement: lead "Relationship to vsdd" with the
methodology-agnostic framing ("mdatron is the engine; it has no opinion
about methodology") then introduce VSDD as the first adopter. Same content,
re-ordered. Defer to v0.1.x; not blocking.

# Cross-finding cohesion

F1 + F8 are both "README's positioning could lead with the differentiator
more clearly". F2 + F5 are within-section voice consistency — Mentor
voice mostly holds; one rough section (E0080 how-to-fix). F3 + F4 are
markup/link-portability. F6 is spec-doc readability; F7 is README sample-
output gap.

No findings are blockers. All are v0.1.x or Phase 6 polish-class. The
prose surface achieves the DR-F1 contract (install + first run + schema +
pattern + vsdd + next) cleanly.

# Summary

8 findings: 0 resolved-pending, 6 accepted-with-remediation (F1, F2, F3,
F4, F7, F8 — all v0.1.x polish), 2 dismissed (F5, F6). No hallucinations.
Mentor voice is the prevailing tone discipline; minor smoothing
opportunities named.
