---
title: "SARIF v2.1.0 — results-contract reference for mdatron (envelope + #148 code-catalog integrity)"
tags: ["sarif", "architecture", "reference", "mdatron", "results-contract", "envelope", "design-doc"]
sources:
  - url: "https://docs.oasis-open.org/sarif/sarif/v2.1.0/os/sarif-v2.1.0-os.html"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://github.com/oasis-tcs/sarif-spec"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://github.com/microsoft/sarif-tutorials"
    title: ""
    accessed_at: "2026-08-02"
  - url: "https://docs.github.com/en/code-security/code-scanning/integrating-with-code-scanning/sarif-support-for-code-scanning"
    title: ""
    accessed_at: "2026-08-02"
contributors: ["79Ig"]
created: 2026-08-02
updated: 2026-08-02
---


## Design Specification

### what this is

A reference-architecture review of **SARIF v2.1.0** (Static Analysis Results Interchange Format, OASIS standard) for **mdatron**, focused on the two threads the Schematron lineage audit ([[schematron-lineage-audit]]) seeded: the **results-contract lineage** (SVRL → mdatron's `verify --json` envelope → SARIF) and the **code-catalog-integrity** design for roadmap **#148** (adopter codes must resolve to a declared catalog). Sourced from the OASIS normative spec, the SARIF JSON schema, Microsoft's tutorials, and GitHub's code-scanning docs; SVRL correspondence from schematron.com. Reviewed against mdatron at envelope 3.0.0 / crate 0.6.0.

### top-level object model

`sarifLog` (root; `version` SHALL be `"2.1.0"`, `$schema` a schema URI) → `runs[]`. Each `run` carries `tool`, `results[]`, `invocations[]`, `artifacts[]`, `taxonomies[]`. `tool.driver` is a single `toolComponent` (SHALL); `tool.extensions[]` are further components (plugins). A `toolComponent` carries `name` (SHALL), `version` (free string) + `semanticVersion` (SHALL be SemVer 2.0.0), `guid`, `rules[]` (the descriptor catalog), `taxa[]` (taxonomy entries), `isComprehensive`. A `result` is one finding; only its `message` is required. `location → physicalLocation → artifactLocation (uri/uriBaseId/index) + region (1-based startLine/Column…, optional snippet)`.

### the rule/descriptor registry — result → descriptor resolution (the #148 core)

`tool.driver.rules[]` is an array of **`reportingDescriptor`** objects: `id` (SHALL, "stable, opaque identifier"), `name`, `shortDescription`/`fullDescription`, `helpUri` + `help`, `messageStrings` (templated `{0}` interpolation table), `defaultConfiguration.level`, `relationships[]`, `deprecatedIds[]`, `properties`.

A `result` addresses its rule three ways: **`ruleIndex`** (int, the canonical unambiguous array index; default −1), **`ruleId`** (string matching a descriptor's `id`), and **`rule`** (a `reportingDescriptorReference` used when the rule lives in an extension). Resolution (§3.52.3): choose the `toolComponent` (driver, or the named extension), then look up by `index` (authoritative), else `guid`, else `id`. If both `ruleId` and `ruleIndex` are present they **SHALL** refer to the same descriptor — the one strict cross-check.

**The resolvability rule is SHOULD, not SHALL.** `ruleId` *should* equal a `reportingDescriptor.id`, but rule metadata is explicitly optional ("a tool can choose not to include it at all… only relevant rules… or all rules"). SARIF **permits a dangling `ruleId`** with no matching descriptor; there is no normative "every cited code MUST resolve" gate. **This is the exact contrast for mdatron:** its tripwire (every emitted `MDATRON-EXXXX` resolves in `code-catalog.json`) is the *SHALL* version SARIF chose not to mandate. (Descriptor `id` uniqueness within one `rules[]` is described/intended — schema calls `ruleId` a "unique identifier" — but not confirmed a normative SHALL; `index` is the canonical locator, tolerating id ambiguity.)

### result: severity, message templating, identity

`level` enum `none|note|warning|error` (default `warning`; resolved explicit → rule default-config → overrides → `warning`). `kind` enum `pass|fail|open|informational|notApplicable|review` (default `fail`) — how SARIF records non-failures. `message` (SHALL): `text` + optional `markdown` + optional `id`+`arguments[]` (looks up `messageStrings[id]`, substitutes `{0}`,`{1}`…). Cross-run identity: `partialFingerprints` (consumer computes identity from components, surviving line churn) and `fingerprints` (tool-computed). `baselineState` (`new|unchanged|updated|absent`), `suppressions[]`, `codeFlows[]`.

### taxonomies & relationships — "code resolves into a declared taxonomy"

SARIF separates a tool's **rules** from external **taxonomies** (CWE/OWASP) and links them. A taxonomy is its own `toolComponent` whose `taxa[]` (reportingDescriptors that are *categories*) enumerate the classification, with `guid`, `semanticVersion`, and `isComprehensive` (true ⇒ enumerates *all* entities). `run.taxonomies[]` references them. `reportingDescriptorRelationship` links a rule to a taxon via `target` (a reference) + `kinds[]` (`relevant`, `equal`, `superset`, `subset`, `disjoint`, …). **Same resolution engine** (`reportingDescriptorReference`, §3.52.3) answers "which rule?" and "which taxonomy entry?" — a citation resolves by id/index/guid into a *declared* array, whether that array is `driver.rules[]` or a taxonomy `toolComponent.taxa[]`.

### versioning — three axes

Format version (`sarifLog.version`, a flat SHALL-`"2.1.0"` string, versioned out-of-band by OASIS, pinned via `$schema` URI); producer version (`toolComponent.version` free / `semanticVersion` SemVer); catalog version (a taxonomy component's `semanticVersion`). **Contrast:** mdatron folds two axes into explicit semver fields — `mdatron_output_version` (format; *stronger* than SARIF's opaque string) and `mdatron_version` (producer). mdatron's one gap: SARIF's machine-resolvable **`$schema`** pin has no mdatron analog.

### aggregation & externalization

`tool.extensions[]` attribute results to sub-tools (a result sets `result.rule.toolComponent` so it resolves into the *extension's* `rules[]`). `sarifLog.runs[]` aggregates multiple tools. `externalPropertyFileReferences` point at external files holding large arrays (`rules`, `taxa`, `results`) by guid + `itemCount` — SARIF's blessed pattern for "the descriptor catalog is huge, ship it once and reference it" (a consumer SHALL treat externalized values as inlined). This is the idiom that matches mdatron's *golden `code-catalog.json` shipped alongside*.

### design tradeoff & critiques

Only a tiny spine is SHALL (`version`, `runs`, `tool`, `driver`, `toolComponent.name`, `result.message`); **everything identifying is optional** (`ruleId`, `locations`, `level`, rule metadata). So minimal SARIF is trivial-but-information-free; useful SARIF is verbose, and "did every finding cite a resolvable rule?" is pushed onto convention/consumers, not the schema. It won because it's vendor-neutral JSON and **GitHub code scanning consumes it natively**. Pain points: verbosity/size (GitHub caps: 10 MB gzip, 25k results/run, 20 runs/file — consumer policy, not spec); soft resolvability; not designed for streaming (externalized `results` is the escape hatch).

### lineage: the svrl correspondence, and the audit-of-what-ran gap

| SVRL | SARIF | mdatron |
|---|---|---|
| `failed-assert` | `result` (`kind:fail`) | `findings[]` |
| `sch:diagnostic` by `@diagnostics` id | `reportingDescriptor` by `ruleId`/`id` | `explain_ref` → explain page by code |
| **`fired-rule` / `active-pattern`** | **(no clean analog)** | **`families` active/inert/inactive** |

**The key finding:** SVRL emits `fired-rule` for *every rule that ran, even finding nothing* — an explicit "what was checked" audit trail; its absence means the check never ran. **SARIF has no first-class equivalent** — it records *findings*, not *coverage-of-checks*. (`result.kind:pass`/`notApplicable` is optional and per-location; `driver.rules[]` lists known rules, not rules that ran; `invocation` carries config/notifications, not a fired/inert signal.) So SARIF answers "what was found"; SVRL additionally answers "what was checked" — and **mdatron's `families` tri-state is squarely on the SVRL side of that line**, the audit signal SARIF structurally lacks. (vsdd's own GH#20 named "families-active-per-verify" as a want — this validates it as a genuine, non-SARIF differentiator.)

### mapping — mdatron ↔ sarif

- **findings[] ↔ results[]:** `code`→`ruleId`(+`ruleIndex`), `severity`→`level` (map onto the 4-value enum), `summary`/`message`→`message.text`, `location{file,line,col}`→`physicalLocation`+`region`, `quoted[]`→`region.snippet`. `help`/`explain_ref` **move up to the descriptor** (SARIF puts help on the rule, not each finding). mdatron has **no per-finding identity** — SARIF's `partialFingerprints` is a gap/opportunity for cross-run result identity.
- **code-catalog.json + explain pages ↔ driver.rules[]:** near-exact twin. Each catalog entry is a `reportingDescriptor` (`code`→`id`, summary→`shortDescription`, explain page→`help`/`helpUri`). mdatron's "golden catalog shipped alongside" is SARIF's **externalized property file** idiom. SARIF's `messageStrings` templating is a feature mdatron's catalog lacks and could adopt for parameterized/localizable text.
- **families tri-state ↔ (nothing in SARIF):** guard it as a deliberate SVRL-side strength; a future SARIF export would carry it only in a lossy `properties` bag.
- **mdatron_output_version ↔ SARIF versioning:** mdatron's format axis is *stronger* (semver vs opaque). The one thing to consider adding: a machine-resolvable `$schema`-style pin keyed to `mdatron_output_version`.
- **A SARIF exporter is a natural, low-risk future** for the mechanical core (findings/results, catalog/rules, locations, snippets, help, severity). Four deliberate misfits to pre-decide: `families`, `pipeline_status`, `summary{counts}`, and the hard "every code resolves" invariant (SARIF can *represent* the rules array but cannot *enforce* resolvability).

### actionable takeaways for #148 (adopter code-catalog integrity)

1. **Adopt SARIF's addressing model, not its enforcement posture.** Model the adopter code set (`VSDD-XNNNN`) as a *named, versioned, guid-bearing component* (SARIF `extension`/taxonomy analog), have each citation name its owning namespace, and resolve by *stable id* into that declared array — but keep mdatron's *SHALL* tripwire (SARIF only does SHOULD). Borrow `isComprehensive` semantics: let the adopter *assert* their catalog is the complete authority, which is what licenses a hard integrity gate.
2. **One resolution engine, parameterized by namespace.** SARIF uses a single `reportingDescriptorReference` resolver for both tool rules and taxonomy entries. mdatron should run *one* catalog-integrity check parameterized by namespace (`MDATRON-*`→`code-catalog.json`; `VSDD-*`→adopter catalog), not two bespoke checks — and it converges with mdatron's existing every-code-resolves-in-explain machinery.
3. **Add a machine-resolvable schema/catalog pin.** SARIF's `$schema` URI is the one versioning affordance mdatron lacks; give the code catalog (and any adopter catalog) an explicit version + optional guid so integrity checks can assert "cited against catalog vX."
4. **Guard `families` as a non-SARIF strength** (the SVRL `fired-rule` audit signal); if a SARIF exporter is built, plan for `families`/`pipeline_status`/`summary` to ride a `properties` bag or be dropped/recomputed, not natively slotted.

### review next (thread status)

- **Results-contract lineage** (SVRL ↔ envelope ↔ SARIF): covered by this page + [[schematron-lineage-audit]]. A SARIF exporter is a candidate feature.
- **Constraint-language lineage** (Schematron ↔ DSL ↔ Cedar/OPA): still open — the queued Cedar/OPA review.
- **Packaging** (Ruff): the queued Ruff review (Rust-CLI rule-registry / `--explain`-from-source / code-meaning stability).

### sources

OASIS *SARIF v2.1.0* (docs.oasis-open.org/sarif/sarif/v2.1.0/os/); the SARIF 2.1.0 JSON schema (github.com/oasis-tcs/sarif-spec); Microsoft SARIF Tutorials (github.com/microsoft/sarif-tutorials); GitHub code-scanning SARIF support (docs.github.com); SVRL correspondence (schematron.com/document/3439.html). Enum values/defaults/required-optional cross-checked against the JSON schema. Flagged as non-normative: GitHub's numeric limits (consumer policy), the "not streamable" critique (practitioner commentary), and exact prose on descriptor-`id` uniqueness (HTML truncation).

