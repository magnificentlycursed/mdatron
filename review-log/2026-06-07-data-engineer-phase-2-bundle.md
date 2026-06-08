---
schema_class: review-entry
schema_version: 1.0.0
review_number: 17
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session Data Engineer review —
  Phase 2 from the data-shape angle: explain catalog as data structure,
  output object shape (Phase 0 contract Phase 2 honors), schema/pattern
  example in README, cross-doc data flow.
lens: >-
  Data Engineer (schema integrity + data-flow boundaries + persistence
  + serialization discipline + cross-system data shape).
source: director-raised
session_note: >-
  Cold-session reviewer mode. DE-domain stance: every artifact has a
  schema; check whether Phase 2's data shapes are coherent. The DE
  domain found load-bearing issues in #12 (output-format contract
  drift); checking whether Phase 2 introduces similar drift.
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  Looking for "the data shape passes typecheck but means something
  different than the spec says." Phase 2 is mostly prose surfaces —
  checking whether the prose surfaces' implicit data contracts hold.
---

# Findings

## F1 — Explain catalog is per-code prose; no schema for the catalog itself (Maintainability)

**Classification:** accepted-with-remediation
**Routing:** DE + SO pair; v0.1.x.

Each explain page is a markdown file with four required structural
elements: `**Severity:**`, `**Introduced in:**`, `## What this means`,
`## How to fix`. The Phase 1a spec declares these as required; the
catalog module's test (`every_baseline_code_has_a_catalog_page`) checks
their presence.

But: no JSON Schema or formal grammar for the explain page format
exists. A page could violate the prose contract in subtle ways (e.g.,
"## What this means" present but empty) and the integrity check passes.

Corrective pattern: a `.mdatron/schemas/explain-page.yaml`-shaped
contract describing the four required elements + minimum prose length
per section, dispatched by mdatron itself against the catalog at
build time. Tighter integrity than substring-presence checks.

v0.1.x; not blocking. The dogfooding aspect (mdatron validates its own
catalog) is appealing — would surface ANY explain page authored without
the discipline.

## F2 — `mdatron_output_version` stays at "1.0.0" per Phase 0 + Phase 2 honors this (Consistency)

**Classification:** dismissed
**Routing:** DE-internal verification.

Phase 2's changes to mdatron-cli don't modify the output object shape.
The `mdatron_output_version` constant at `mdatron-core/src/output.rs:18`
remains "1.0.0". The integration test
`output_version_is_consumer_pinnable_semver` (output_format.rs) asserts
the version is parseable semver. Pass.

Per the [[feedback-no-premature-version-bumps]] discipline: mdatron is
unpublished v0.1.0; the output object's version is similarly forward-only
+ unbumped. Correct posture.

## F3 — README's schema example uses JSON Schema draft 2020-12 (Consistency)

**Classification:** dismissed (with note)
**Routing:** DE-internal verification.

README's `.mdatron/schemas/blog.json` example uses draft-2020-12 shape
(`"type": "object"`, `"required": [...]`, `"properties": {...}`,
`"additionalProperties": false`). Matches DESIGN-MDATRON.md § Layer 1's
"JSON Schema draft 2020-12 is the supported schema language."

No `$schema` declaration in the example. mdatron's schema loader doesn't
require it (verified by code path — `mdatron-core/src/schema.rs` parses
JSON via serde_json without a schema-meta-validation step). The omission
is fine but worth noting: an adopter who tools their JSON Schemas
against a validator outside mdatron may want `$schema` for editor
integration.

v0.1.x README polish: add `"$schema": "https://json-schema.org/draft/2020-12/schema"`
to the example for editor-tooling fitness.

## F4 — Pattern example uses `mdatron_dsl_version: 1` ✓ (Consistency)

**Classification:** dismissed
**Routing:** DE-internal verification.

README's pattern example sets `mdatron_dsl_version: 1`. Matches
DESIGN-MDATRON.md § DSL versioning convention. Pass.

The `mdatron_dsl_version` is a top-level pattern-file field carrying the
DSL version the pattern targets. v0.1 of the DSL is the only version
shipping; the version field is forward-only — when v2 lands, mdatron
retains v1 pattern support indefinitely.

## F5 — The output object's `findings` array is unbounded; for many-finding runs the JSON could be GB-sized (Edge-cases)

**Classification:** dismissed (with note)
**Routing:** DE-internal observation; Phase 0 territory.

Same finding as the Phase 0 audit invariants table notes implicitly
("Output's `summary` counts match `findings` array"). Phase 2 does not
change the unboundedness. If a 100k-finding run produces a 100MB JSON
object, stdout buffering becomes the limiting factor. Operator can
limit via `--files` or pre-commit hook scope.

Not a Phase 2 concern; documented per the methodology-discipline of
naming-rather-than-silent.

## F6 — Phase 2 does NOT introduce a persistent-storage surface (Consistency)

**Classification:** dismissed
**Routing:** DE-internal verification.

mdatron remains stateless across invocations. The catalog is embedded
in the binary; the README/explain pages are read-only artifacts. No
`.mdatron/cache/` writes; no database; no persistent index.

The DESIGN-MDATRON.md "cached by source-file mtime for incremental LSP
runs (v1.1)" forward-references LSP-server territory; not Phase 2 work.
Pass.

# Summary

6 findings: 0 resolved-pending, 1 accepted-with-remediation (F1 explain-
page schema), 5 dismissed-with-note (F2-F6 — all verifications-pass /
honest-observations). The data shape Phase 2 introduces (explain catalog
as embedded markdown) is coherent; the unimplemented schema is the only
material follow-up and is v0.1.x. No hallucinations.
