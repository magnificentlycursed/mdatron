---
schema_class: review-entry
schema_version: 1.0.0
review_number: 6
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session DR review — Phase 2 doc
  surface as a cold reader: mdatron README (first encounter), explain
  catalog (5 pages first-time read), Phase 1a/1b/1c specs (cold reviewer
  of the planning artifacts), cross-doc anchor integrity.
lens: >-
  Documentation Reviewer (cold-reader pass + audience coverage +
  cross-doc consistency). Five-lens weighting: Usability 5,
  Consistency 5, Maintainability 3, Edge-cases 3, Attacker 1.
source: director-raised
session_note: >-
  Cold-session reviewer mode. Sycophancy compensation: Claude authored
  the README + spec + catalog; DR's specific failure mode is "the doc
  reads well to its author but the audience it claims to serve cannot
  actually onboard from it." Treating every paragraph as the first time
  a real adopter reads it.
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  Assumption: a brand-new adopter who has never touched mdatron, vsdd,
  or any Schematron-derived tool reads the README cold. What do they
  understand? What's the first place they get stuck?
---

# Findings

## F1 — README "First run" assumes the adopter knows the mdatron-init pattern is manual (Usability)

**Classification:** accepted-with-remediation
**Routing:** TW + DR pair.

README "First run" says:

> Create a minimal project that mdatron can validate:
> ```
> mkdir my-typed-docs
> cd my-typed-docs
> mkdir -p .mdatron/schemas .mdatron/patterns
> ```

A cold reader (vsdd-cli already exists as a downstream adopter) might
search for "mdatron init" and not find it. The README later says (in the
explain page for E0080): "on Phase 5 of the binary-first plan, `mdatron
init` does this for you." That's an implementation-detail-leaked phrase.

Corrective pattern: README "First run" leads with: "`mdatron init` (v0.1.x
candidate) scaffolds this skeleton automatically; for v0.1.0 the steps
below are manual." Sets expectations without making the v0.1.0 reader feel
they're missing something.

## F2 — Explain page E0080 references "Phase 5 of the binary-first plan" — internal language leaked (Usability)

**Classification:** accepted-with-remediation
**Routing:** DR + TW pair.

E0080 "How to fix" says: "the project has not been initialized with the
mdatron skeleton. Create `.mdatron/schemas/` and `.mdatron/patterns/` at
the project root (or, on Phase 5 of the binary-first plan, `mdatron init`
does this for you)."

The "Phase 5 of the binary-first plan" reference is meaningful inside the
toolkit's planning context but opaque to an end-user reading `mdatron
explain MDATRON-E0080`. Drop the reference; replace with "(v0.1.x: `mdatron
init` will scaffold this for you)" — same forward-looking info, no
internal-roadmap-leakage.

## F3 — Phase 1a spec uses "DR-F1" / "TW-F3" / "M5 F3+F9" — opaque to a future-spec-reader (Cross-doc consistency)

**Classification:** dismissed (with note)
**Routing:** DR-internal.

Phase 1a spec cites findings by their cross-session anchor IDs (DR-F1,
TW-F3, M5 F3+F9). A future-spec-reader who doesn't have the upstream
review entries handy can't trace these. Each anchor IS resolvable via
review-log search — but the friction is real.

Counter: cluster-batched cold-session reviews are stable artifacts under
the forward-only methodology discipline; the anchors don't rot. Future-
spec-reader who needs to chase an anchor `grep -rn "DR-F1" review-log/`
in 90% of cases. Keep the anchor convention; the discipline is sound.
The note: a top-of-spec "anchor convention" callout would help anyone
encountering the spec for the first time.

## F4 — Five explain pages cover only the v0.1.0 emission set; cross-references between pages incomplete (Cross-doc consistency)

**Classification:** accepted-with-remediation
**Routing:** DR-side; v0.1.x.

Each explain page closes with "## Related codes" naming neighboring codes.
Spot check:

- E0001 cites E0002, E0050 — accurate
- E0002 cites E0001, E0050 — accurate
- E0050 cites E0001, E0002 — accurate (forms the "frontmatter trilogy")
- E0070 cites E0080, E0010/E0011 — E0010/E0011 are NOT in the v0.1.0
  catalog; the reader hits "no explain page found for MDATRON-E0010" if
  they follow the link
- E0080 cites E0070, E0001/E0050 — E0001/E0050 ARE in the catalog ✓

The E0070 page's reference to E0010/E0011 is forward-looking — they're
reserved-for-path-confinement (DESIGN-MDATRON.md:508) but not currently
emitted by Phase 1's code. A reader following the link gets the
not-in-catalog error.

Corrective pattern: in the "Related codes" of pages that cite
not-yet-cataloged codes, mark them: "MDATRON-E0010 / E0011 (reserved for
v0.1.x path-confinement check; explain page lands when the check ships)."
Or: only cite codes currently in the catalog. Either fix; the former is
more honest.

## F5 — README's "Relationship to vsdd" section explains vsdd-cli to a reader who may not know what VSDD is (Usability)

**Classification:** accepted-with-remediation
**Routing:** DR + TW pair; v0.1.x.

The section says "vsdd is the first downstream adopter of mdatron and the
source of the methodology vocabulary (phase primers, domain prompts,
finding artifacts, the VSDD whitepaper alignment)." Each named term
(phase primers, domain prompts, finding artifacts, whitepaper) is
meaningful inside VSDD but opaque outside.

A cold reader of mdatron (who is NOT a VSDD adopter) wonders: do I need
to learn all this? The answer is no — but the dense vocabulary obscures
that. Suggested rewrite leads with the relationship arrows:

> vsdd is one downstream adopter of mdatron. mdatron is the validation
> engine; vsdd is the methodology. If you're not adopting VSDD, you can
> ignore this section.
>
> If you ARE adopting VSDD: ...

Same content, ordered to dismiss non-VSDD readers cleanly. v0.1.x.

## F6 — Explain pages don't link to DESIGN-MDATRON.md for deeper context (Cross-doc consistency)

**Classification:** accepted-with-remediation
**Routing:** DR-side; trivial.

E0050 mentions "JSON Schema for that class" and "the schema at
`.mdatron/schemas/<class>.json`" but doesn't link to DESIGN-MDATRON.md
§ Layer 1 where the schema architecture is documented. Same for E0080
(pattern files, mentioned without link).

A cold operator reading an explain page may want to know "what IS this
schema thing?" — currently they have to discover DESIGN-MDATRON.md on
their own. Add per-page "See also: DESIGN-MDATRON.md § <section>" links
under "## Related codes" or as inline references. ~1 line per page.

## F7 — Phase 1c § Drive-by hygiene names two items "not blocking" but both were resolved in Phase 2b (Spec drift)

**Classification:** dismissed
**Routing:** DR-internal verification.

Phase 1c § "Drive-by hygiene" lists:
1. print_finding open-coded TTY rendering drift from format_tty
2. cmd_explain placeholder text

Both were resolved in Phase 2b/2c. The Phase 1c spec is now slightly
stale relative to the implementation — the "Spotted during Phase 1a
authoring; resolve if cost is trivial" framing is accurate; Phase 2c
resolved both as a single commit. Spec wording matches the actual
outcome. Pass.

## F8 — README forward-references `mdatron-examples` library without acknowledging it doesn't exist yet (Usability)

**Classification:** accepted-with-remediation
**Routing:** DR + SO pair; small.

README "Relationship to vsdd" section: "The mdatron-examples library
(v1.0 roadmap) ships four generalized artifact-class schemas (DESIGN doc,
manual-test, PR template, CHANGELOG) for non-VSDD adopters."

"v1.0 roadmap" implies the library is coming. A cold reader wondering
"can I use the DESIGN-doc schema today?" finds no answer. Add: "(v1.0
candidate; not in v0.1.0 — see V1-SHIP-CRITERIA.md for the schedule)."

Honest about the state; forward-pointers preserved.

# Cross-finding cohesion

F1 + F2 + F8 are all "expectation-setting about features that don't
exist yet" — each leak makes the reader feel something is missing. The
unified fix: be explicit about v0.1.0 scope vs v0.1.x / v1.0 candidates
in every forward-reference. F4 + F6 are explain-catalog cross-doc
discipline — close the gaps. F3 is anchor-vocabulary, F5 is
audience-positioning, F7 is verification.

# Summary

8 findings: 0 resolved-pending, 6 accepted-with-remediation (F1, F2, F4,
F5, F6, F8 — most v0.1.x polish), 2 dismissed (F3, F7). The prose
surface achieves the audience-coverage shape DR-F1 named; the v0.1.x
polish opportunities are cluster-routable to the next iteration. No
hallucinations.
