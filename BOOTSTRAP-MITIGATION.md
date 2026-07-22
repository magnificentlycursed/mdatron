# Bootstrap-Period Mitigation Plan

> **Exit record (2026-07-21, #46):** Mitigation 1's `bootstrap-validator.py`
> (with `canonical-counts.yaml` + `live-entities.yaml`) has been **retired and
> deleted** per its own criterion — "deleted when `mdatron-core verify` runs
> cleanly against the project" (§ Mitigation 1; exit-criterion 3). mdatron now
> verifies its own `review-log/*.md` corpus against a committed `.mdatron/`
> config, enforced by the `self-validate` CI gate. This is the bootstrap-
> **validator** retirement, which by design predates the full bootstrap-
> **period** exit; the remaining exit criteria (v1.0 on crates.io, vsdd-cli
> Step 5 coupling, coinage-log processed, `BootstrapPeriodExited` emitted) are
> unmet and tracked under #48. The document below is preserved as the
> historical charter.

**Status:** Step 0.5 artifact — Mitigation 1 exited 2026-07-21 (see exit record above). Sibling to [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md). Closes the validator-absent-state risks raised by SEC-F3, AIE-F2, M2, M4 in the 2026-06-01 multi-domain review.

**Provenance:** Steps 1-4 of the bootstrap plan produce VSDD-methodology artifacts before mdatron's validators exist. This plan declares the bounded mitigations operating during that period.

---

## Problem statement

The bootstrap plan has 5 steps:

1. Write DESIGN.md
2. Adjust vsdd-cli scope for standalone mdatron
3. Implement vsdd-cli init
4. Use vsdd-cli init to develop mdatron
5. Refactor vsdd-cli init to use mdatron

Steps 1-4 produce real VSDD-methodology artifacts (DESIGN docs; review log entries; PR descriptions; CHANGELOGs; per-domain prompts; phase primers; the DSL spec itself). The full hook chain (`check-frontmatter-schema`, `check-cite-resolution`, `check-naming-discipline`, etc.) does NOT exist yet — mdatron is being built.

Risks (from the 2026-06-01 multi-domain review):

| Risk ID | Description |
|---|---|
| SEC-F3 | Audit-trail integrity claim partially false during bootstrap; structural drift can land without any validator firing |
| AIE-F2 | "Easy for agents to author" DSL claim untested |
| M2 | Methodology's "not honor-system; not authoring discipline alone" claim partially untrue during bootstrap |
| M4 | ~30+ new coinages from DESIGN.md authoring land unregistered; would surface as a wave at Step 5 |

The four mitigations below close each risk under bounded scope.

---

## Mitigation 1: Minimal Python validator-of-validators

A small Python script (`bootstrap-validator.py`) at `mdatron/bootstrap-tooling/` covers the 5 most cascade-prone late-detection patterns identified in the 2026-06-01 review evidence:

| Pattern (from review evidence) | Detection approach | Est. LoC |
|---|---|---|
| Cross-document count drift (Class 1) | Regex sweep for `N (artifact\|class\|frontmatter\|hook\|variant)` across docs; compare against an explicit `canonical-counts.yaml` | ~40 |
| Orphaned rules / table rows (Class 2) | Parse markdown tables; check each row's first-column entity against a declared "live entities" list per pattern file | ~50 |
| Broken intra-doc hyperlinks (Class 3) | Parse markdown links; resolve internal anchors against headings in the project tree | ~40 |
| Header-vs-table count inconsistency (Class 4) | Regex for `### Heading (N)` patterns; count rows in the next table/list; compare | ~30 |
| Frontmatter `schema_class` declared but mistyped (Class 6) | Parse frontmatter; validate `schema_class:` value against the registered class list | ~40 |

Total: ~200 lines. Pure Python; uses only `pyyaml`, `markdown-it-py`, `re`. No mdatron dependency. Runs as pre-commit hook + as `python bootstrap-validator.py docs/` ad-hoc.

The script is **explicitly throwaway**: scoped to the validator-absent gap (Step 1 authoring start → `mdatron-core`'s first usable build, partway through Step 3). Deleted when `mdatron-core verify` runs cleanly against the project — typically mid-Step-3, not at Step 5. Per SO-F3 (build-in-mdatron-shape-from-day-one), the gap is bounded to ~weeks rather than the months a Step-5-throwaway would have implied. Bootstrap-validator findings carry codes in the `BOOT-W####` range to be distinct from mdatron's `MDATRON-E####` and vsdd-cli's `VSDD-E####`.

**Scope discipline:** the script does NOT attempt to cover the full 19-hook chain. It covers the 5 patterns most likely to cascade through review cycles. Other structural drift is accepted as a known gap; surfaced when `mdatron-core verify` first runs against the project (~mid-Step-3) as a one-time wave.

**Acceptance criterion:** `bootstrap-validator.py` operational at Step 1 commit; runs cleanly against the current mdatron repo state (including this preamble + mitigation plan) before Step 1 authoring proceeds.

## Mitigation 2: Coinage log (closes M4)

Maintain `mdatron/coinage-log.md` during Steps 1-4. Every new term coined for mdatron's design (DSL vocabulary, pattern names, error codes, file-layout conventions, etc.) gets a one-line entry.

Initial seed (from the originating conversation + BOUNDARY-PREAMBLE):

| Term | First use | Definition | Routes to vocabulary tier |
|---|---|---|---|
| `pattern` (mdatron sense) | DSL spec | A named group of related cross-field rules sharing a `keys:` declaration | candidate |
| `rule` (mdatron sense) | DSL spec | A single declarative constraint with `context`, `assert`, `code`, `message` | candidate |
| `context` (mdatron sense) | DSL spec | A selector declaring what artifacts a rule applies to | candidate |
| `key` (mdatron sense) | DSL spec | A declared cross-file index built once per validation pass | candidate |
| `let` (mdatron sense) | DSL spec | A local binding evaluated before a rule's `assert:` | candidate |
| `phase` (mdatron sense) | DSL spec | A runtime-selectable subset of patterns | candidate |
| delegated check | DSL spec | An imperative escape-hatch rule invoking an external command | candidate |
| built-in pattern | DSL spec | A pattern mdatron ships with that adopters opt into | candidate |
| canonical-schema-path discipline (extension) | BOUNDARY-PREAMBLE § 7 | Extension to delegate registrations + key sources | accepted (extension of vsdd-cli's existing rule) |
| bootstrap period | This document | The Steps 1-4 interval during which mdatron's validators do not yet exist | candidate |
| validator-of-validators | This document | The throwaway Python script covering the 5 most cascade-prone patterns | candidate |
| `mdatron://` scheme | BOUNDARY-PREAMBLE § 4 | Reserved URI scheme for cross-repo citations resolved via release manifest | reserved (v1+) |
| `BootstrapPeriodEntered` / `BootstrapPeriodExited` | This document § Mitigation 3 | Audit-trail event variants bracketing the bootstrap interval | candidate |

**Discipline:**

- Every commit during Steps 1-4 that introduces a new term updates `coinage-log.md` in the same commit
- `bootstrap-validator.py` includes a check: any `**bold-introduced**` term, or any `## Heading` matching a "novel-term" heuristic, that is NOT in the coinage log fires `BOOT-W0010: coinage-not-logged`
- At Step 5, the log is processed: each term promoted to vsdd-cli's `templates/registry/vocabulary.yaml` with `tier: candidate` + `source: mdatron-bootstrap` + `recurrence_count: 1`
- Promotion from candidate to accepted requires second-recurrence evidence per existing vocabulary discipline; the bootstrap-log entry counts as recurrence #1

## Mitigation 3: Audit-trail bootstrap-period markers (closes SEC-F3, M2)

The methodology event log emits two new events bracketing the bootstrap period:

```yaml
event_type: BootstrapPeriodEntered
payload:
  reason: "mdatron-toolkit-development"
  expected_exit_criteria:
    - "mdatron v1.0.0 cargo-installable from crates.io"
    - "vsdd-cli Step 5 refactor merged"
    - "bootstrap-validator.py deleted"
    - "coinage-log.md processed into vocabulary.yaml"
  validator_coverage_gap: "5 of ~19 patterns covered by bootstrap-validator.py; remaining gap accepted"
  entered_at: <ISO 8601>

event_type: BootstrapPeriodExited
payload:
  exit_criteria_met: [...]
  mdatron_version_first_used: <semver>
  bootstrap_validator_removed: true
  retroactive_vocabulary_merge_pr: <URL>
  exited_at: <ISO 8601>
```

These are declared as **candidate event variants** in vsdd-cli's `DESIGN-OBSERVABILITY.md` at Step 2 (scope-adjust commit). Promotion from candidate to accepted occurs at Step 5 when both events have fired (one project's recurrence pair = one recurrence).

Without these markers, future audit reads of `.vsdd/events.jsonl` would see structural-drift findings landing during Steps 1-4 without context. With them, the gap is named, bounded, and exit-criterion-anchored.

**The honest framing recorded in the events:** "validator-coverage gap accepted for the bootstrap period." Forensic auditors see the gap declared, not silenced.

## Mitigation 4: DSL falsifiability check (closes AIE-F2)

Before Step 1 closes (DESIGN.md authoring complete), run the AIE-F2 falsifiability test:

1. **Identify** 5-10 candidate cross-field validation rules from vsdd-cli's existing DESIGN-VERIFICATION (e.g., classification-universe, validator-pair-match, count-drift, header-vs-table, anchor-resolution).
2. **Spin up a cold-context Claude Code session** with ONLY the DSL spec section of DESIGN.md as reference (no prior conversation; no implementation; no examples beyond what's in the spec).
3. **Ask the cold session to author each rule** as DSL YAML.
4. **Measure:** how many rules compile cleanly under the spec's syntax? How many require multiple iterations? Where does the cold session misread the spec?

**Acceptance criterion:** ≥80% of rules authorable in one pass from the spec alone. Below that, the DSL spec needs revision before Step 2.

The test is itself a throwaway artifact; results land in `mdatron/dsl-falsifiability-report.md` and inform a DESIGN.md revision commit. The test does NOT need to pass for Step 1 to be declared complete — it needs to **run** and produce findings that route either to "DSL spec accepted" or to "DSL spec revision required."

**Why this matters beyond AIE-F2:** the falsifiability check itself becomes a recurring practice. Every DSL revision triggers a cold-context author-from-spec pass; the pass-rate is the DSL's quality signal.

---

## Exit criteria summary

The bootstrap period exits when ALL of:

1. mdatron v1.0.0 published to crates.io (per [`V1-SHIP-CRITERIA.md`](./V1-SHIP-CRITERIA.md))
2. vsdd-cli's Step 5 release coupling complete (switches from `mdatron-core` local-path-dep to published-crate-dep). Per SO-F3, vsdd-cli has depended on `mdatron-core` as a Rust library since mid-Step-3; Step 5 is the release-coupling change, not a refactor wave.
3. `bootstrap-validator.py` deleted (deleted partway through Step 3 when `mdatron-core verify` runs against the project; predates the formal bootstrap-period exit by weeks-to-months)
4. Coinage log processed: all entries merged into `templates/registry/vocabulary.yaml` with appropriate tiers
5. `BootstrapPeriodExited` event emitted with the exit_criteria_met manifest

**Wall-clock estimate:** operator-time-binding-without-quantification per vsdd-cli's existing discipline. The exit criteria are objective; the timeline is what it is.

## Scope discipline

This plan does NOT attempt to:

- Cover all 19 vsdd-cli hooks during bootstrap (5 of ~19 is the bounded coverage)
- Achieve full audit-trail integrity (the gap is named, not closed)
- Implement mdatron's features in `bootstrap-validator.py` (it's a structural-pattern-checker; mdatron is a Schematron-derived rule engine — distinct scope)
- Eliminate the validator-absent risk (the four mitigations bound the risk; they do not remove it)

The honest framing: bootstrap exists because mdatron doesn't exist yet. The plan accepts the bounded risk for the bounded period, with the four mitigations limiting its blast radius.

---

## Mitigation summary table

| Mitigation | Closes | Deliverable | When operational |
|---|---|---|---|
| 1. `bootstrap-validator.py` | SEC-F3 (partial), Class 1-4 + 6 drift | ~200-LoC Python script at `mdatron/bootstrap-tooling/` | Before Step 1 authoring begins |
| 2. `coinage-log.md` | M4 | Markdown file at `mdatron/coinage-log.md` + `BOOT-W0010` check in bootstrap-validator | Updated in every Steps 1-4 commit |
| 3. `BootstrapPeriodEntered`/`Exited` events | SEC-F3, M2 | Two new candidate event variants in vsdd-cli's DESIGN-OBSERVABILITY | Entered at Step 1 commit; exited at Step 5 close |
| 4. DSL falsifiability check | AIE-F2 | Cold-context author-from-spec test; report at `mdatron/dsl-falsifiability-report.md` | Run at Step 1 closing review |

When all four are operational, the bootstrap period's risk profile is bounded; Step 1 may begin.
