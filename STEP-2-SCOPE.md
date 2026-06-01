# Step 2 Scope — vsdd-cli scope-adjust

**Status:** Scope artifact for Step 2 of the bootstrap plan. Identifies what changes land in vsdd-cli's DESIGN docs to reflect mdatron's separation. Step 0.5 already landed the mirror commits (design-doc frontmatter with `consumes_from: [mdatron://DESIGN-MDATRON]`, candidate event variants, queued additions); Step 2 is the substantive revisions that surface across the vsdd-cli design tree.

**Provenance:** Per [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md) § 8 co-evolution discipline + the 5-step bootstrap plan declaration in the originating conversation.

**Why this is a scope artifact, not the edits themselves:** the substantive revisions are real work (~2-3 hours per doc × 4 docs = ~10-15 hours of operator-time). Concentrated editing in a fresh focused session produces better quality than rushed patches at the end of a long conversation. This artifact makes the work explicit so it can be executed deliberately.

---

## What landed in Step 0.5 (already done)

| File | Change | Reference |
|---|---|---|
| vsdd-cli/DESIGN-SCHEMA.md | Added design-doc frontmatter with `consumes_from: [DESIGN-METHODOLOGY, mdatron://DESIGN-MDATRON]` + `produces_for: [DESIGN-VERIFICATION, DESIGN-OBSERVABILITY, DESIGN-METHODOLOGY]` | BOUNDARY-PREAMBLE § 3 |
| vsdd-cli/DESIGN-VERIFICATION.md | Added design-doc frontmatter; added § "Step 2 queued additions" declaring `VSDD-W0200` extension + `MdatronVersionPinned` companion variant | DR-F1 + SO mirror |
| vsdd-cli/DESIGN-OBSERVABILITY.md | Added design-doc frontmatter; added § "Candidate event variants reserved for Step 2" with 3 variants (`MdatronVersionPinned`, `BootstrapPeriodEntered`, `BootstrapPeriodExited`) | BOOTSTRAP-MITIGATION § Mitigation 3 |

These are the formal cross-doc declarations. Step 2 below is the substantive content shift.

---

## What Step 2 must add or revise

### DESIGN-SCHEMA.md

**Must-have for Step 2 close:**

- **§ Schema source + tooling**: reframe to "the validation engine is `mdatron-core`; vsdd-cli ships JSON Schema files that mdatron-core consumes". Move/cross-reference DSL-related content to [`DESIGN-MDATRON.md`](./DESIGN-MDATRON.md) § DSL specification rather than duplicating.
- **§ Validation modes**: reframe to "mdatron provides two layers (JSON Schema structural + DSL semantic); vsdd-cli ships schemas + patterns in mdatron's registered formats". Point to DESIGN-MDATRON for layer details.
- **§ Anchor-ID generation conventions**: keep the conventions here (VSDD-specific patterns) but note mdatron's anchor-derivation utility implements them.
- **§ Error catalog structure**: reframe to "vsdd-cli registers VSDD-E#### codes in mdatron's catalog format; mdatron's own MDATRON-#### engine codes coexist".

**Nice-to-have:** none for Step 2.

**Deferred:** removing the full DSL spec from DESIGN-SCHEMA — the DSL spec is canonical in DESIGN-MDATRON now; vsdd-cli's DESIGN-SCHEMA points to it without duplication. Removal can land in Step 2.3 once the cross-link is verified non-broken.

### DESIGN-VERIFICATION.md

**Must-have for Step 2 close:**

- **§ Canonical-schema-path discipline**: extend to note that mdatron's schemas + patterns are loaded from the installed mdatron binary (per `BOUNDARY-PREAMBLE` § 7 path-confinement; mirror of vsdd-cli's existing canonical-schema-path).
- **§ One source; two enforcement surfaces (Python subprocess to Rust binary)**: retarget the subprocess to `mdatron verify hook <id>` instead of `vsdd verify hook`. Update the Python wrapper sketch (~10-line snippet).
- **§ Per-hook deployment matrix**: each row's hook implementation moves from vsdd-cli's own Rust binary to (a) mdatron declarative pattern, OR (b) delegate registration calling `vsdd verify hook <id>` per mdatron's delegate protocol. Categorize each of the 19 hooks.
- **§ CI workflow templates**: update to install mdatron BEFORE vsdd in the bootstrap step (`cargo install mdatron --locked` before `cargo install vsdd --locked`).

**Nice-to-have:**

- Decompose the 19-hook list explicitly: which hooks are pure-declarative (mdatron patterns) vs which need imperative escape (delegate registrations). Approximate split based on the 2026-06-01 review's DSL coverage analysis: ~14 declarative + ~5 delegated.

**Deferred:**

- Revising bypass-marker enforcement to use mdatron's bypass-marker parsing utility instead of vsdd-cli's own. Cosmetic change; Step 2.3.

### DESIGN-METHODOLOGY.md

**Must-have for Step 2 close:**

- **§ Adoption + distribution**: update install order to include mdatron per [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md) § 6. Add explicit pre-flight rule that `vsdd init --check` fails with `VSDD-E0221: mdatron-not-installed` if mdatron is missing.
- **Methodology amendment**: formally acknowledge M1 (inline-multi-domain composition shape recurrence) per the 2026-06-01 review's earned-by-recurrence trigger. Either accept InlineMultiDomain as first-class composition shape OR declare the recurrent deviation a methodology defect.

**Nice-to-have:**

- Add a § Methodology amendments section formally declaring the InlineMultiDomain composition shape with applicability rules.

**Deferred:**

- M3 (Phase 5 adaptation for spec-stage purity-boundary audit) — defer until a second adaptation case surfaces. Single case is not yet earned-by-recurrence.

### DESIGN-OBSERVABILITY.md

**Must-have for Step 2 close:** none beyond Step 0.5 mirror.

**Nice-to-have:**

- Add subsection on coordination with mdatron's audit-trail emission: mdatron emits diagnostic findings; vsdd-cli's `.vsdd/events.jsonl` sink ingests mdatron's findings as `ValidationFailed` / `ValidationPassed` event payloads when vsdd-cli is in use.

**Deferred:**

- Specific event-variant payload definitions for the 3 candidates (MdatronVersionPinned, BootstrapPeriodEntered, BootstrapPeriodExited) — defer payload-shape design to implementation time when concrete usage clarifies field needs.

### README.md

**Must-have for Step 2 close:**

- **§ Adoption + distribution**: update install order to:

  ```sh
  cargo install crosslink
  cargo install mdatron
  cargo install vsdd
  cd <project-root>
  crosslink init
  vsdd init       # vsdd init pre-flights for crosslink AND mdatron
  ```

- **§ Center of gravity**: reframe item #4 ("The schema enforcement layer") to "vsdd-cli's VSDD-specific schemas + patterns shipping against mdatron as the validator engine".
- **§ Binary surface**: the table currently says "Principle: any CLI the suite ships is Rust" — update to acknowledge mdatron is a separately-shipped Rust binary; vsdd composes against it as a sibling tool.

**Nice-to-have:**

- Add a § Relationship to mdatron section explaining the inversion + the v1.0/v1.1 boundary + the cross-repo coordination discipline.

**Deferred:** none.

---

## Recurring pattern across all docs

vsdd-cli's existing docs predominantly speak in "vsdd-cli owns the schema/validation/etc" voice. After the inversion, the voice shifts to "vsdd-cli composes against mdatron for the validation engine; vsdd-cli's content is the VSDD-methodology-specific schemas, patterns, hooks, and prompts."

This is more than a sed-replace — it's a tonal shift across the document tree. Specific recurring substitutions:

| Before (vsdd-cli owns) | After (vsdd-cli composes against mdatron) |
|---|---|
| "the toolkit's validator engine" | "mdatron's validator engine, which vsdd-cli composes against" |
| "vsdd-cli's JSON Schema discipline" | "the JSON Schema discipline mdatron implements, which vsdd-cli's schemas conform to" |
| "the Rust mirror at CI-side" | "mdatron-cli running CI-side via the same engine as the Python hook wrapper" |
| "vsdd-core" (as a Rust crate VSDD owns) | "mdatron-core (consumed as a Rust library dependency)" |
| "the error catalog at vsdd-core/error-catalog.yaml" | "vsdd-cli's catalog at .mdatron/catalogs/vsdd.yaml, registered with mdatron's catalog loader" |

---

## Recommended sub-step sequencing

1. **Sub-step 2.1** (this artifact): land `STEP-2-SCOPE.md` in mdatron's repo to make the work explicit. **DONE on commit of this file.**
2. **Sub-step 2.2** (vsdd-cli must-haves): make the must-have edits in each vsdd-cli DESIGN doc. Single Phase 4 → Phase 1a commit on vsdd-cli.
3. **Sub-step 2.3** (vsdd-cli nice-to-haves + DESIGN-METHODOLOGY amendments): subsequent commit on vsdd-cli including the InlineMultiDomain methodology amendment + cosmetic deferrals from 2.2.
4. **Sub-step 2.4** (cross-doc validation): once all edits land, run mdatron's bootstrap-validator.py (per [`BOOTSTRAP-MITIGATION.md`](./BOOTSTRAP-MITIGATION.md) § Mitigation 1) on both repos to catch any cross-document count drift introduced by the revisions.

Step 2 closes when sub-step 2.4 reports clean.

---

## Estimated effort

| Sub-step | Operator-time estimate |
|---|---|
| 2.1 (this artifact) | 30 min (already complete) |
| 2.2 (must-haves across 4 docs) | 6-10 hours focused editing |
| 2.3 (nice-to-haves + methodology amendment) | 3-5 hours |
| 2.4 (cross-doc validation + iteration) | 1-2 hours |
| **Total** | **~10-17 hours** |

Per vsdd-cli's operator-time-binding-without-quantification discipline, no wall-clock commitment; the exit criteria are objective (sub-step 2.4 passes cleanly).

---

## Bootstrap-period coordination

Step 2 happens entirely DURING the bootstrap period (per [`BOOTSTRAP-MITIGATION.md`](./BOOTSTRAP-MITIGATION.md)). The 4 mitigations apply:

- **Mitigation 1** (`bootstrap-validator.py`): operational during Step 2 to catch structural drift from the vsdd-cli edits
- **Mitigation 2** (`coinage-log.md`): any new terms coined during Step 2 documentation revisions land here for retroactive merge at Step 5
- **Mitigation 3** (bootstrap-period markers): the audit trail already entered at Step 1's commit; remains entered through Step 5
- **Mitigation 4** (DSL falsifiability check): completed at Step 1; if Step 2 surfaces new DSL needs (unlikely; Step 2 is documentation work), re-run

`mdatron-core` development (Step 3 per SO-F3 build-in-mdatron-shape-from-day-one) can begin in parallel with Step 2 sub-steps 2.2 and 2.3. They don't block each other.
