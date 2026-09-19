---
title: "Observability Engineering 2e — applicability audit for mdatron (via vsdd-cli's read)"
tags: ["architecture", "reference", "mdatron", "observability"]
sources:
  - url: "http://oreilly.com/catalog/errata.csp?isbn=9781098179922"
    title: ""
    accessed_at: "2026-09-19"
contributors: ["79Ig"]
created: 2026-09-19
updated: 2026-09-19
---

# Observability Engineering 2e — applicability audit for mdatron

Derived from vsdd-cli's chapter reads (its knowledge pages `observability-engineering-2e-reading-map`, `oe2e-impact-analysis-2026-09-18`, and the three thematic captures; chapters 17/18/21/22/27/10/8/7), re-evaluated against mdatron at envelope 3.0.0 / crate 0.6.0-unreleased. In the book's own "AI sandwich" framing (Ch17), **mdatron is the deterministic validation layer**: the book's dispatch/cost/agent-reliability material belongs to vsdd-cli; what transfers here is the record-design and audit-signal material. A supplementary mdatron-side read of the chapters vsdd skipped (Ch1 §guardrails, Ch6 wide events, Ch32) is captured separately.

## Already satisfied or ahead (grounding only)

- **Signal parity** (Ch17: the CI assertion IS the production query; "fixing the build is indistinguishable from fixing the system") — mdatron's founding design: one `verify --json` envelope at hook-time, CI, and agent loop.
- **Coverage, not just findings** — the `families` tri-state + `files_checked` (the SVRL `fired-rule` carry, per `schematron-lineage-audit`/`sarif-envelope-audit`) is Ch17's coverage invariant, falsifiable via per-family `reason`.
- **Could-not-check as a first-class state** — W0048/E0003/inert-with-reason (made family-consistent in GH #48 lane G) covers and exceeds the book's pass/fail/maybe; the book has no dormant-vs-clean distinction.
- **Codified ontology** (Ch17: "an ontology remains an academic exercise until it is codified") — code catalog + explain pages + versioned envelope schema, catalog generated from the pages (#164).
- **Invariants constrain output, not input** — the Schematron thesis mdatron carries verbatim.
- **The eval flywheel** (Ch21: promote real failures to golden fixtures; re-run on every change) — the roast discipline: every phase-3 finding becomes a red-gate test; the escape corpus is the negative-fixture set.
- **Determinism/reproducibility** — DEF4 byte-identical envelopes cross-platform; the capture-once snapshot; linear-time engines.

## Design inputs (routed to tracker issues, post-0.6.0)

1. **Envelope run telemetry** (Ch18: CI/CD are production systems for developer feedback; end-to-end user time is the first SLI). The envelope carries no `duration`; DESIGN already names latency measurement a **declared deferral** ("thresholds banded from actuals in phase 1b"). Input: wall-clock (total, possibly per-family) as envelope dimensions so a consumer (vsdd-cli) can band SLOs — e.g. the Ch18 6-7 minute attention threshold — over real records.
2. **Ruleset lineage + schema pin** (Ch17: "lineage in every output transforms a figure into a traceable audit log"; universal payload = identity + versions + invariant results). The envelope versions the engine and the format but not the governance data that produced the findings. Input: a lineage block (content hashes of the `.mdatron/` inputs — machinery the pin/manifest families already have) + the machine-resolvable `$schema` pin `sarif-envelope-audit` already flagged as mdatron's one versioning gap (second corroboration), + per-field descriptions in the published schema (Ch21's imputed-meaning lesson: a field description beats a prompt).
3. **Per-finding fingerprints** (Ch17 close-the-loop + Ch21 flywheel need stable identity; SARIF `partialFingerprints` — the gap `sarif-envelope-audit` named). A consumer trending envelopes across runs cannot tell new from known findings across line churn. Second corroboration promotes it to a tracked input.
4. **Repo-ops, immediate**: Ch18's "retries are a tail-latency tax; flakes hide failure in the tail" is mdatron's own CI concurrency-cancel ritual — every PR in the 0.6.0 cycle required a manual rerun of a cancelled duplicate `push` run before merge. Fix the workflow triggers (no `push` builds on PR branches), not the reruns.

## Deliberately does not transfer (vsdd-cli's layer)

Dispatch plans/ActionPlan validation, token-cost attribution + cost SLOs, proposal-rejection-rate reliability signals, operator-workload boundaries (Rasmussen), context layers, commander/caretaker personas — routed in vsdd-cli's own impact analysis to its Slices 6/7. mdatron serves that stack by staying the deterministic layer; note vsdd's B5 ruling makes its future dispatch records **mdatron-governed**, so inputs 1-2 above directly serve that consumer. (Observed in passing: vsdd's `mdatron-capability-surface` page is at 0.5.0/five-families vintage — stale by an envelope MAJOR and four families.)
