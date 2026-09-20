---
title: "observability-engineering-ch31-vocabulary"
tags: ["reference"]
sources:
  - url: "Observability Engineering 2e Ch31 pp557-569, read 2026-09-19; Ch3 pp40-42, Ch25 pp451-452"
    title: ""
    accessed_at: "2026-09-20"
contributors: ["79Ig"]
created: 2026-09-20
updated: 2026-09-20
---

# Observability Engineering 2e — Ch31 vocabulary/semantic-conventions capture (read 2026-09-19)

Sibling to [[observability-engineering-mdatron-chapters]]; Ch31 "Instrumentation for Observability Teams" (Weakly + Parker, book pp 557-569, PDF = book+36) is the book's ONE semantic-conventions/naming/schema-governance chapter — previously in the not-read set on both projects.

## The chapter's substance
- Constructed-language thesis (p557-558): the team's north star is "being able to define and manage the constructed language for their organization" — archeology over ossified informal terms + a bridge to the wider lingua franca.
- Semantic conventions (p559): "a shared vocabulary and grammar" — vocabulary = the names; grammar = reusable patterns (dot namespaces, families like http.request.header.<key>, requirement/maturity levels, stability). "More a dialect than an encyclopedia"; goal = telemetry as a trainable skill. Two heuristics do the heavy lifting: domains namespaced specificity-first left-to-right; actors defined client/server-producer/consumer.
- Telemetry schemas (pp559-561): API-contract discipline for telemetry — single source of truth living near the code; compile-time AND runtime verification ("builds can be failed... pipelines can drop mystery attributes"); safe evolution; AI assistants ingest the schemas for safer editing. Authoring rules: start from the questions users will ask; namespace org-shared attributes at the shared level (explicitly because "AI will pattern-match very strongly"); abstract actors over protocol; DRY + cardinality-aware naming with low-cardinality companions layered.
- Anti-pattern (p562): never locally rename a universal vocabulary for local consistency.
- Enforcement (pp563, 568-569): OTel Weaver in CI live-checks emitted telemetry against registered schemas; in regulated settings schema control gates existence; language is promulgated by "education, dictionaries, rites, and repetition (namely, culture)." The too-heavyweight objection is pre-rebutted (p561: misaligned incentives, not inherent flaw).
- Definitional-rigor cautionary tale (Ch3 pp40-42; Ch25 pp451-452): the authors coined "observability" precisely and "lost that fight in a landslide" to vendor rebranding — "we're sorry. This is not what we intended." A coined term with a definition but NO codified governance drifts and is lost. Complements Ch17 p314 "an ontology remains an academic exercise until it is codified."

## Mapped to mdatron
- STRONG VALIDATION of the vocabulary family: the telemetry-schema pattern IS the vocab family, point for point — registry near the content, single source of truth, deny_unknown_fields ("drop mystery attributes"), digest lineage, CI enforcement (mdatron = a prose-domain Weaver; the book names the function but no tool for prose). label_schemes regex registries already match "dialect, not encyclopedia" family-pattern shape.
- Refinement candidate (filed): per-term maturity/stability levels — semconv entries carry requirement/maturity/stability; adopter registries may deserve a first-class deprecation flow (status -> alias -> removal), the same discipline the field-rename ledger applies to input contracts.
- Namespace grammar as a registerable convention: the two heuristics are a SCHEME an adopter could register; label_schemes can express much of it; note only, no adopter demand yet.
- mdatron's own surfaces: envelope flat snake_case is NOT a violation (dot-namespacing solves open-space cross-emitter collision; the envelope is a closed single-emitter SemVer contract — the Ch6 width counter-argument's sibling); cardinality layering already conformant (low-card `code` for GROUP BY + high-card `fingerprint` for instance identity); per-field schema descriptions (Ch21 imputed meaning) already shipped with the envelope batch — Ch31 is a third corroboration.
- POSITIONING TEXT: the Ch3/Ch25 lost-the-fight story is the best available motivation for why the vocab family exists (coined term + definition − enforcement = drift); routed to the docs lane and the case study.
- Honest scope: the book carries NO prose/documentation-writing guidance (register, tone, error-message wording) and no registry file-format prescription; Ch4's "Vocabulary of Telemetry" is signal taxonomy only. Outside Ch31 + fragments, the remaining ~20 chapters carry nothing for vocab or prose.
