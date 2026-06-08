---
schema_class: design-doc
schema_version: 1.0.0
doc_class: design
version: 0.1.0
consumes_from: []
produces_for:
  - vsdd-cli/DESIGN-SCHEMA
  - vsdd-cli/DESIGN-VERIFICATION
last_revision_trigger: Step 1 initial draft per BOOTSTRAP-MITIGATION § Mitigation 3 (2026-06-01)
---

# DESIGN-MDATRON.md

Design document for `mdatron` — a Rust toolkit for typed-markdown validation. Defines the two-layer architecture (JSON Schema structural + DSL semantic), the cross-field rule language, the 5 built-in patterns, the error catalog format, the CLI subcommand surface, the agent-loop integration (PostToolUse hook + compact format + skill-preamble), the mdatron-examples library, the path-confinement discipline for delegate registrations + `key()` sources, and the co-evolution discipline with vsdd-cli.

```yaml
# Pre-authoring composition declaration (per Phase 1a primer + BOUNDARY-PREAMBLE § 8 co-evolution)
authoring_target: DESIGN-MDATRON.md
phase: phase-1a
composed_domains: [solution-owner, ux, accessibility, software-engineer, quality-engineer, solution-architect, platform-engineer, performance-engineer]
composition_mode: inline-single-agent-multi-domain
operator_confirmation: confirmed (operator directive 2026-06-01)
declared_at: 2026-06-01
substrate:
  crosslink: initialized in mdatron (committed; agent identity 79Ig)
  vsdd-cli: phase primers + 14 domain prompts manually deployed to .claude/commands/ (vsdd init does not yet exist; Step 3 work)
  upstream_constraints:
    - BOUNDARY-PREAMBLE.md (8 declarations closing SA/DR/PE/SEC findings)
    - V1-SHIP-CRITERIA.md (Phases A+B+C+F+always-alongside ship in v1.0; D+E in v1.1)
    - BOOTSTRAP-MITIGATION.md (4 mitigations operating during Steps 1-4)
    - mdatron/review-log/2026-06-01-solution-architect.md (24 findings)
methodology_deviation: |
  Phase 1a canonical shape is operator-interactive skill mode with per-domain skills loaded one at a
  time. This authoring runs inline-single-agent-multi-domain (M1 recurrence per the 2026-06-01
  review; methodology amendment to formalize this composition shape proposed in DESIGN-METHODOLOGY).
  Trade-off: same as the 2026-06-01 DR review — domain-pair isolation lost; mitigated by grounding
  every section in citable BOUNDARY-PREAMBLE constraints or review findings.
```

For boundary constraints: see [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md). For v1 scope: [`V1-SHIP-CRITERIA.md`](./V1-SHIP-CRITERIA.md). For bootstrap-period discipline: [`BOOTSTRAP-MITIGATION.md`](./BOOTSTRAP-MITIGATION.md). For the review evidence that motivated this design: [`review-log/2026-06-01-solution-architect.md`](./review-log/2026-06-01-solution-architect.md).

---

## Positioning + identity

mdatron is **a Rust CLI toolkit for typed-markdown validation, descended from XML's Schematron lineage**. It is **not related to the TRON blockchain** despite the `-tron` suffix; the suffix evokes Schematron (ISO/IEC 19757-3), which mdatron adapts for JSON/YAML/markdown documents the same way `jsontron` adapted Schematron for JSON.

### What mdatron is

- A **two-layer validator** for markdown documents with YAML frontmatter: JSON Schema for structural validation (frontmatter shapes, required fields, enums); a Schematron-derived DSL for cross-field, cross-file, and cross-document semantic rules.
- A **single Rust binary** (`mdatron`) installable via `cargo install mdatron`. Subcommands dispatched per cargo / rustup / git ecosystem convention.
- A **methodology-agnostic engine**. mdatron consumes from nothing (per [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md) § 3); adopters supply their own schemas, patterns, error catalogs, and registries.
- A **rustc-shaped diagnostic surface**: error codes with `--explain` pages, SARIF 2.1.0 emission for CI integration, source-spans on findings, candidate / accepted / deprecated status tiers.
- A **pre-commit + agent-loop tool**, not an editor experience (at v1.0). LSP integration is reserved for v1.1 per [`V1-SHIP-CRITERIA.md`](./V1-SHIP-CRITERIA.md).

### What mdatron is NOT

- **Not a prose linter** — adopters who want prose-quality feedback compose with Vale or similar; mdatron's scope is document structure + cross-reference integrity.
- **Not a markdown formatter** — for that, use `dprint` or `mdformat`.
- **Not an editor server** at v1.0 — LSP held for v1.1.
- **Not VSDD-specific** — mdatron is the foundation; VSDD is one adopter (the first, but explicitly not the only design constituency).
- **Not related to the TRON blockchain** — the README's first sentence states this explicitly to disambiguate (per TW-F3 from the 2026-06-01 review).

### Lineage

mdatron's architecture is **20+ years established** (XML's XSD + Schematron, ISO/IEC 19757; Kubernetes' OpenAPI CRDs + OPA Gatekeeper; CUE's unified schema-and-constraint approach). What is novel:

- **Applying the two-layer pattern to markdown documents.** No production-grade tool has the "frontmatter as typed surface + cross-document references over markdown anchors + rule-based validation layer" stack for markdown.
- **rustc-shaped DX layered on top** — codes, `--explain` pages, SARIF emission, source-spans on findings. Validator engines have existed for years; the compiler-diagnostic UX on top of them usually has not.
- **Agent-loop-first integration** — the PostToolUse hook surface that injects diagnostics into LLM-agent tool results during authoring is unusual; most validators target either editor-LSP humans or CI-only batch validation.

---

## Scope + boundary

mdatron owns:

- The validator engine (`mdatron-core` library): DSL parser + evaluator; JSON Schema dispatch; cross-file index lifecycle; error catalog format; SARIF emission; anchor-ID derivation utility; bypass-marker parser
- The CLI binary (`mdatron-cli`): subcommand dispatch (`verify`, `explain`, `registry`, `init`, `skill-preamble`, `verify hook`); output formatting (TTY, SARIF, JSON, compact)
- The 5 built-in patterns (header-count-matches-table, frontmatter-claimed-count-matches-rows, anchor-derived-from-frontmatter-resolves, cross-doc-count-consistency, orphan-rule-detection)
- The error catalog YAML format + per-code explain page convention + status tier discipline (candidate / accepted / deprecated)
- The DSL specification: syntax + standard library + cross-file index semantics + diagnostic message templating + delegated check protocol
- The path-confinement discipline for delegate registrations + `key()` sources (per [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md) § 7)
- The PostToolUse hook configuration template (deployed via `mdatron init` OR consumed by `vsdd init` for vsdd-cli adopters)
- The mdatron-examples library (4 generalized classes: DESIGN doc, Manual-test, PR template, CHANGELOG)
- The release pipeline (cosign + SLSA + per-platform binaries × 4 platforms; reproducible builds)

mdatron does NOT own:

- **VSDD methodology semantics** — phases, domains, classification universes, validator-pair maps. These are vsdd-cli's territory.
- **Adopter-specific schemas** — each adopter authors their own; mdatron-examples ships only the 4 generalized classes.
- **The MCP server surface** (`mdatron mcp-serve`) — namespace reserved per [`V1-SHIP-CRITERIA.md`](./V1-SHIP-CRITERIA.md); not implemented at v1.0.
- **The LSP server** (`mdatron lsp-serve`) — namespace reserved; not implemented at v1.0; v1.1 candidate.
- **User-defined DSL functions** — adopters compose via standard library + `let` bindings + delegated checks; no user-extensible function registry in v1.
- **Prose-quality validation** — Vale or similar covers prose-style; mdatron does not duplicate.
- **Markdown formatting** — dprint or mdformat covers formatting; mdatron emits diagnostics, not reformats.

---

## Two-layer architecture

mdatron implements **the layered validation pattern formalized by ISO/IEC 19757 (Document Schema Definition Languages)** — structural validation + rule-based validation as complementary layers, not competing approaches.

### Layer 1: Structural (JSON Schema)

Adopters ship JSON Schema files at `.mdatron/schemas/<class>.json`. mdatron parses YAML frontmatter from markdown documents matching the configured file globs + class routing; validates the frontmatter against the schema.

Behaviors:

- **JSON Schema draft 2020-12** is the supported schema language. Other drafts rejected at config load with `MDATRON-E0040: schema-draft-unsupported`.
- **Frontmatter format**: YAML between `---` markers at the very top of the file. No frontmatter = no schema-class binding = no Layer 1 validation. Documents may still be reached by Layer 2 rules via path-glob `context:` selectors.
- **Schema routing**: documents bind to a schema via (a) frontmatter `schema_class:` field, OR (b) file-glob mapping in `.mdatron/config.yaml`. Both forms supported; if both present and disagree, `MDATRON-W0050: schema-class-routing-ambiguous` fires.
- **Edge cases**:
  - Empty file → no frontmatter → no Layer 1 violation; Layer 2 may still apply
  - Malformed YAML frontmatter → `MDATRON-E0001: frontmatter-parse-failed`
  - Frontmatter `schema_class:` references unknown class → `MDATRON-E0002: schema-class-unknown`
  - Schema file itself malformed → `MDATRON-E0003: schema-malformed` at config-load time (never silent)
  - Frontmatter violates its declared schema → `MDATRON-E0050: frontmatter-schema-violation` (v0.1.x)

### Layer 2: Rule-based (DSL)

Adopters ship pattern files at `.mdatron/patterns/<name>.yaml`. Each pattern groups related rules sharing a `keys:` declaration; rules express cross-field, cross-file, and cross-document constraints.

Behaviors:

- **Rules fire on FALSE assertions.** mdatron has one verb (`assert`); Schematron's `report` (fires on TRUE) is intentionally absent for spec simplicity.
- **Cross-file indices** declared via `keys:` blocks; built once per validation pass; cached by source-file mtime for incremental runs.
- **Path-confined**: `key()` sources MUST resolve under the project root; absolute paths rejected (`MDATRON-E0010`); parent-traversing paths rejected (`MDATRON-E0011`); symlinks not followed.
- **Edge cases**:
  - Rule references undefined key → `MDATRON-E0020: key-undefined`
  - Rule's `assert:` expression has type mismatch (e.g., `count()` on a string) → `MDATRON-E0021: assert-type-mismatch`
  - Pattern file has no rules → `MDATRON-W0060: pattern-empty`
  - Cross-file lookup misses target → rule's `let` bindings receive null; subsequent operations on null short-circuit to false (rule fires, message includes the missing-target context)

### Why two layers, not one

CUE collapses both layers into one language; the choice for mdatron is two-layer because:

- **JSON Schema is universal.** Adopters from any ecosystem (Node, Python, Go) already author JSON Schema for OpenAPI / CRDs / config validation. Reusing it for the structural layer means zero learning curve for the most common case.
- **The DSL is small.** Cross-field rules are 80% of the value beyond JSON Schema; building a small focused language is cheaper than extending JSON Schema vocabulary.
- **Two layers debug separately.** Structural failures point at the schema; semantic failures point at the rule. Adopters can reason about each independently when diagnostics fire.

The downside: adopters must understand two languages. Mitigated by (a) the rustc-shaped diagnostic surface that distinguishes which layer caught each finding; (b) `.mdatron/config.yaml` declaring both layers in one file for unified onboarding; (c) the two-layer architecture diagram in the README leading every example.

---

## DSL specification

The DSL is YAML-based; expressions use JS/Python-shaped syntax. Designed so a Claude-class agent can author rules from a one-page reference (per AIE-F2 falsifiability discipline; per [`BOOTSTRAP-MITIGATION.md`](./BOOTSTRAP-MITIGATION.md) § Mitigation 4).

### Pattern file structure

```yaml
mdatron_dsl_version: 1
pattern:
  id: <kebab-case-id>                    # required; unique within project
  description: <prose>                   # optional; appears in --explain output
  phases: [<phase-name>, ...]            # optional; defaults to ["default"]

  keys: [<key-declaration>, ...]         # optional; cross-file indices used by this pattern
  rules: [<rule>, ...]                   # required; non-empty
```

### Rule structure

```yaml
- id: <kebab-case-id>                    # required; unique within pattern
  context: <selector>                    # required; what artifacts the rule applies to
  let:                                   # optional; local bindings evaluated before assertion
    <name>: <expression>
  assert: <expression>                   # required; rule fires when this is FALSE
  code: <error-code>                     # required; references a catalog entry
  message: <template>                    # required; diagnostic message
  location:                              # optional; defaults to whole-artifact span
    field: <frontmatter-field-path>      #   pinpoint to a frontmatter field, OR
    expression: <expression>             #   compute the source span
```

### Context selectors

Three forms:

```yaml
context: phase-primer                    # by schema_class (most common)
context: "**/*.md"                       # by file glob
context: { schema_class: finding,        # combined
           path: "review-log/**/*.md" }
```

Behaviors:

- Document with no `schema_class:` is invisible to schema-class selectors; reachable via path globs only
- Document matching multiple rules' contexts: each rule evaluated independently; all matching rules' diagnostics emit
- Edge case: context selector matches zero documents → no warning fired (this is normal during early-bootstrap repos)

### Expression language

Familiar JS/Python-shaped syntax for LLM readability:

```yaml
$self.phase                            # frontmatter field
$self.relevant_domains[0]              # array indexing
$self.body                             # markdown body (string)
$file.path                             # current file's path
$file.line(<frontmatter-field>)        # source line of a frontmatter field
$project.root                          # project root absolute path
$project.config                        # parsed .mdatron/config.yaml
```

Reserved namespaces: `$self`, `$file`, `$project`. No others; adopters use `let:` bindings instead.

Operators: `== != < <= > >=` comparison; `and or not` boolean (lowercase, readable); `in not_in` membership; `+ - * / %` arithmetic.

### `let:` bindings

Define local names inside a rule. Evaluated top-to-bottom; later bindings reference earlier ones.

```yaml
let:
  expected: key("composition-matrix", $self.phase)
  required: $expected.required
  missing: difference($required, $self.relevant_domains)
assert: count($missing) == 0
```

### Standard library

Versioned with mdatron; pure (no side effects); deterministic.

**Collection:** `count(xs)`, `len(s)`, `every(x in xs, pred)`, `some(x in xs, pred)`, `union(a, b)`, `intersect(a, b)`, `difference(a, b)`, `unique(xs)`, `flatten(xs)`, `join(xs, sep)`.

**Lookup:** `key(name, value)` (cross-file index query); `glob(pattern)` (returns array of paths matching the glob); `read_frontmatter(path)` (parsed YAML; null if absent); `frontmatter_field(path, field)` (shorthand); `count_files(pattern, schema_class)` (how many files match glob AND class).

**Markdown structure (read-only AST queries):** `headings($self, level)`, `tables($self)`, `links($self)`, `code_blocks($self, lang)`, `body_sections($self)`, `sections_at_level($self, level)`, `html_comments($self)`. Each returns an array of structured nodes; all nodes share `{text, line, column}` for source-span pinpointing. Per-helper node shapes:

- `headings(...)` nodes: `{text, slug, line, column}` where `slug` is `slug(text)` precomputed (matches the `slug()` standard library function below)
- `links(...)` nodes: `{text, target, title, line, column}` where `target` is the URL or anchor reference (`#anchor`, relative path, or absolute URL) and `title` is the tooltip from `[text](url "title")` syntax or null
- `code_blocks(...)` nodes: `{lang, body, line, column}` where `body` is the literal code content
- `tables(...)` nodes: `{headers, rows, line, column}` where `headers` is the array of header cell strings and `rows` is the array of row arrays
- `sections_at_level($self, level)` returns section nodes at the given level (1-6). Each: `{text, level, slug, line, column, children, body}` where `children` is the array of immediate-child sections at level+1 and `body` is the prose between the section's heading and its first child section (string). Nested `every`/`some` quantifiers traverse the tree (e.g., `every(s in sections_at_level($self, 2), every(h in s.children, h.text in $canonical))`)
- `html_comments(...)` nodes: `{text, line, column}` where `text` is the comment's interior content (excluding `<!--` and `-->` delimiters); `line`/`column` point at the opening `<!--`

**String:** `lower(s)`, `upper(s)`, `match(s, regex)`, `match_all(s, regex)`, `extract(s, regex, group)`, `extract_all(s, regex, group)`, `starts_with(s, prefix)`, `ends_with(s, suffix)`, `contains(s, substr)`, `slug(s)`.

Semantics:

- `match(s, regex)`: true if the regex matches anywhere in `s` (unanchored substring matching). For anchored matching, include `^` and `$` in the regex itself
- `match_all(s, regex)`: array of all full-match strings; empty array if no matches
- `extract(s, regex, group)`: the first capture group's value as a single string, or null if no match
- `extract_all(s, regex, group)`: array of all captured group values across all matches; empty array if no matches
- `slug(s)`: GitHub-flavored Markdown anchor derivation — lowercase; replace contiguous whitespace with single hyphens; strip characters outside `[a-z0-9-]`. Matches the precomputed `slug` field on heading and section nodes

**File system:** `exists(path)`, `is_file(path)`, `is_dir(path)`. All path-confined (must resolve under project root).

**Predicates:** `defined(x)`, `is_null(x)`.

### Authoring discipline for optional fields

`Field`-on-Object-missing-key and `Field`-on-`Null` both return `Null`
(symmetric semantics, v0.1.x). This means a rule like
`every(d in $self.relevant_domains, ...)` silently passes when
`relevant_domains` is absent — the `every` quantifier over `Null` returns
true vacuously.

For optional fields where absence has semantic meaning, the rule MUST
guard with `defined()`:

```yaml
assert: defined($self.relevant_domains) and every(d in $self.relevant_domains, ...)
```

Author discipline: when writing rules against frontmatter fields that
might be omitted, decide upfront whether the rule should fire on absence
(use `defined()` + the actual check) or silently pass (use the
unconditional check; the symmetric semantics make this safe). Per
crosslink #12 AIE/F3 (Field-symmetry shifts authoring discipline from
fail-loud to silent-Null).

### Cross-file indices (`keys:`)

Declared at pattern level; built once per validation pass.

```yaml
keys:
  - name: composition-matrix
    source: .vsdd/registry/phase-domain-matrix.yaml
    select: $.matrix             # JSONPath into the file
    indexed_by: $key             # use the YAML key (phase-2a, phase-1b, ...)

  - name: domain-prompts
    source: .claude/commands/vsdd-domain-*.md
    select: $.frontmatter
    indexed_by: $.domain_slug
```

Behaviors:

- **`select:` extracts entries from each source file's parsed structure.** If the selection resolves to an array, each element becomes an entry; if it resolves to a single object, the object becomes one entry. Applies uniformly to single-source and glob-source.
- **Parsed structure per file type:**
  - YAML / JSON files: parsed value tree; JSONPath navigates from the root
  - Markdown files: an object with two fields:
    - `frontmatter` — the YAML frontmatter (parsed; null if absent)
    - `markdown` — AST query namespace exposing `headings`, `tables`, `links`, `code_blocks`, `html_comments`, and `sections_at_level_<N>` (e.g., `sections_at_level_2`); each returns the same node objects as the standard library helpers
- **Single-source:** `source` is one file path; `select` extracts one or many entries from within (e.g., a file with a top-level array of records yields N entries)
- **Glob-source:** `source` is a glob; each matching file contributes its `select`-extracted entries (one OR many per file)
- **`indexed_by:`** resolves against each entry; duplicate keys collapse (last-wins) and emit `MDATRON-W0085: key-duplicate-collapsed` at index-build time
- **Index lifecycle:** built once per `mdatron verify` invocation; cached by source-file mtime for incremental LSP runs (v1.1)
- **Edge case:** `source` matches zero files → empty index → `key()` always returns null → rules using the index short-circuit on null operations

Example (indexing every heading across all markdown):

```yaml
keys:
  - name: all-heading-slugs
    source: "**/*.md"
    select: $.markdown.headings
    indexed_by: $.slug
```

### Diagnostic message templating

Double-curly interpolation; same expression language as `assert:`.

```yaml
message: >
  Primer {{$self.primer_id}} declares phase {{$self.phase}} but is missing
  required domain(s) {{join(difference($required, $self.relevant_domains), ", ")}}
  per the composition matrix at {{$file.path}}:{{$file.line("phase")}}.
```

Resolved at emission time. Same template language across TTY, SARIF, JSON, compact output formats.

### Phases (runtime selection)

Patterns can declare which phases they belong to. Operator selects active phase at invocation:

```yaml
# in pattern file
pattern:
  id: phase-domain-composition
  phases: [strict, pre-commit]
```

```yaml
# .mdatron/config.yaml
phases:
  default:    [strict, fast]              # mdatron verify (no flag)
  pre-commit: [fast]                       # mdatron verify --phase pre-commit
  strict:     [strict, fast, slow]         # mdatron verify --phase strict
  lsp:        [fast]                       # reserved for v1.1 LSP
```

Use cases:

- `pre-commit` phase skips expensive cross-file index builds for sub-second hook latency
- `strict` phase runs everything (CI gate)
- `lsp` phase reserved for v1.1 LSP server

If a pattern declares no `phases:`, it belongs to `default`.

### Delegated checks (imperative escape hatch)

For checks that can't be expressed declaratively, a rule delegates to an external command:

```yaml
- id: sycophancy-compensation
  context: review-entry
  delegate:
    command: ["vsdd", "verify", "hook", "check-sycophancy-compensation"]
    timeout_ms: 5000
  code: VSDD-W0010
```

**Delegate protocol (mdatron-protocol-version 1):**

mdatron invokes the command with the file path as the first positional argument. JSON envelope on stdin:

```json
{
  "mdatron_protocol_version": 1,
  "rule_id": "sycophancy-compensation",
  "file_path": "review-log/2026-05-27-quality-engineer.md",
  "frontmatter": { ... },
  "project_root": "/path/to/project",
  "config": { ... }
}
```

Findings JSON on stdout:

```json
{
  "mdatron_protocol_version": 1,
  "findings": [
    {
      "code": "VSDD-W0010",
      "severity": "warning",
      "message": "Reviewer's git identity overlaps with artifact author; sycophancy_compensation field required.",
      "location": { "field": "sycophancy_compensation" }
    }
  ]
}
```

Exit code 0 = ran successfully (findings may be empty or present). Non-zero = delegate errored; mdatron emits `MDATRON-E0030: delegate-failed` and treats the check as inconclusive.

Path-confinement applies (per [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md) § 7): delegate `command:` field points to a binary either reachable on `$PATH` (operator-trusted) OR under the project root via relative path. PR-modifiable delegate registrations require operator opt-in via signed `.mdatron/delegates-trusted.yaml`; otherwise `MDATRON-W0030: delegate-registered-from-pr-modifiable-path` fires and the delegate does not execute.

### What's NOT in v1

- **No lambdas.** `every(x in xs, x.foo == 1)` is the only "closure" — `x` is a bound name in scope, not a real lambda.
- **No user-defined functions.** Standard library closed in v1; adopters compose via `let:` + delegated checks.
- **No conditional branching in expressions.** No `if/then/else`; use multiple rules with different `assert:` clauses for branching logic.
- **No regex backreferences in `extract()`.** PCRE-compatible literal regex only.
- **No async / parallel evaluation as observable semantics.** mdatron may evaluate rules in parallel for performance; DSL is pure and deterministic.
- **No file writes from delegates.** Delegates are read-only; quick-fixes apply via the (v1.1) LSP, not the delegate protocol.

These are v1.x or v2 candidates depending on demand evidence per the earned-by-recurrence trigger.

### DSL versioning

```yaml
mdatron_dsl_version: 1
pattern: { ... }
```

mdatron rejects pattern files declaring a version higher than the installed mdatron supports. Patterns omitting the field are assumed v1; emits `MDATRON-L0001: dsl-version-unspecified` (lint).

DSL changes:
- Additive (new function, new optional field): minor bump
- Breaking (renamed function, semantics change): major bump (mdatron v2 retains v1 pattern support indefinitely)

---

## Built-in patterns

mdatron ships 5 built-in patterns adopters opt into via `.mdatron/config.yaml`. These are NOT adopter-authored — they're shipped with mdatron + maintained in `mdatron-core`.

| Built-in | Code | What it catches | Why built-in |
|---|---|---|---|
| `header-count-matches-following-table` | `MDATRON-W0100` | A markdown heading ending `(N)` followed by a table with ≠ N data rows | Class 4 from late-detection review evidence; trivial to express; cascade-prone |
| `frontmatter-claimed-count-matches-rows` | `MDATRON-W0101` | Frontmatter declares an array of N items; companion table or list has ≠ N rows | Common Class 1 (count drift) variant |
| `anchor-derived-from-frontmatter-resolves` | `MDATRON-W0102` | A markdown link references an anchor that doesn't exist in any project file | Class 3 (broken intra-doc hyperlinks); requires project-wide anchor index |
| `cross-doc-count-consistency` | `MDATRON-W0103` | Multiple files cite a count of N for the same entity; counts diverge | Class 1 (cross-document count drift); the canonical late-detection failure |
| `orphan-rule-detection` | `MDATRON-W0104` | A reference-table entry points to an entity (class, hook, code) not in the registered set | Class 2 (orphaned rules after refactors) |

Adopters configure severity, opt-out per file glob, etc., via `.mdatron/config.yaml`:

```yaml
built_ins:
  - header-count-matches-following-table
  - frontmatter-claimed-count-matches-rows
  - anchor-derived-from-frontmatter-resolves
  - cross-doc-count-consistency
  - orphan-rule-detection

built_in_overrides:
  header-count-matches-following-table:
    severity: error                       # promote from warning
    exclude_paths: ["docs/legacy/**"]    # don't enforce on legacy docs
```

**Rationale for built-in (vs. shipping as example patterns):** these 5 cover the 80% of cascade-prone structural-drift cases identified in the 2026-06-01 review evidence. Making each adopter rewrite them invites bugs + inconsistency; the rules themselves require non-trivial markdown AST traversal + project-wide indexing.

---

## Error catalog

### Catalog file format

Each registrant ships a YAML file at `.mdatron/catalogs/<prefix>.yaml`:

```yaml
catalog_schema_version: 1
prefix: VSDD
codes:
  - code: VSDD-E0200
    severity: error                       # error | warning | lint
    status: accepted                      # candidate | accepted | deprecated
    summary: phase-domain-composition-required-domain-missing
    detail: >
      A phase primer must include every required domain declared in the
      phase-domain composition matrix for its phase.
    help: >
      Add the missing domain(s) to relevant_domains, OR update
      .vsdd/registry/phase-domain-matrix.yaml if the matrix is incorrect.
    explain_ref: docs/error-codes/VSDD-E0200.md
    introduced_in: 1.0.0
    deprecated_in: null
    replacement: null
```

### Code namespacing

Each registrant reserves a prefix. Adopter prefixes are free-form. mdatron reserves `MDATRON-` for engine-level codes (~10-20 codes; see § Reserved mdatron codes below).

### Loading multiple catalogs

`.mdatron/config.yaml` lists catalog files:

```yaml
error_catalogs:
  - .mdatron/catalogs/mdatron.yaml          # always loaded; mdatron's own engine codes
  - .mdatron/catalogs/vsdd.yaml             # VSDD's codes (when adopter is VSDD)
  - .mdatron/catalogs/project.yaml          # project-local codes (rare; for project-internal patterns)
```

Behaviors:

- mdatron loads catalogs in declared order; later definitions of the same code override earlier ones
- Same code defined in two catalogs → load-time warning (`MDATRON-W0080: catalog-code-collision`); last-wins
- Adopters resolving collisions: rename a local code via find-replace; no central registry required

### Status tiers

- **`candidate`** — code authored + fixtures present; validator emits warning rather than blocking; awaits 2nd recurrence to graduate
- **`accepted`** — multi-recurrence evidence OR operator-directive trigger; validator blocks per declared severity
- **`deprecated`** — retired forward-only; carries `replacement` field with migration pointer; never reused

Forward-only governance: codes never reused once retired (matches Rust's E0000-series stability). Major-version bump required for renaming codes, removing codes, or changing severity.

### Reserved mdatron codes

mdatron's own engine-level codes use the `MDATRON-` prefix:

| Range | Class |
|---|---|
| `MDATRON-E0001` — `E0009` | Frontmatter parsing failures |
| `MDATRON-E0010` — `E0019` | Path-confinement violations (per [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md) § 7) |
| `MDATRON-E0020` — `E0029` | DSL evaluation failures |
| `MDATRON-E0030` — `E0039` | Delegate protocol failures |
| `MDATRON-E0040` — `E0049` | Schema-document load failures (parsing `.json` schema files themselves) |
| `MDATRON-E0050` — `E0059` | Frontmatter-against-schema validation failures (v0.1.x; the document being validated, not the schema) |
| `MDATRON-E0060` — `E0069` | Reserved for future use |
| `MDATRON-E0070` — `E0079` | IO failures during verify (v0.1.x) |
| `MDATRON-E0080` — `E0089` | Pipeline orchestration failures (v0.1.x) |
| `MDATRON-E0090` — `E0099` | Reserved for future use |
| `MDATRON-W0030` — `W0039` | Delegate-registration warnings (per § 7) |
| `MDATRON-W0050` — `W0099` | Configuration warnings |
| `MDATRON-W0100` — `W0199` | Built-in pattern findings |
| `MDATRON-L0001` — `L0099` | Engine-level lints (e.g., DSL version unspecified) |

Range additions for v0.1.x (Phase 1 of binary-first refactor; crosslink #12):
`E0050-E0059`, `E0070-E0079`, `E0080-E0089`. The `is_reserved_mdatron_code`
function at `mdatron-core/src/codes.rs` is the canonical machine-readable
source; this table is the human-readable doc surface; both must agree.
Range gaps (`E0060-E0069`, `E0090-E0099`) are explicitly reserved for future
use to avoid arbitrary allocation.

Full catalog ships in `mdatron-core/catalogs/mdatron.yaml`; per-code explain pages at `docs/error-codes/MDATRON-*.md`.

### Per-code explain pages

Each code has a markdown page at `docs/error-codes/<code>.md`. Required sections:

```markdown
# <code> — <summary>

**Severity:** <error | warning | lint>
**Status:** <candidate | accepted | deprecated>
**Introduced in:** <semver>

## What this means
<plain-language explanation>

## Failing example
<minimal markdown excerpt that triggers the code>

## How to fix
<corrective pattern; Mentor voice>

## Related codes
- <code>: <summary>

## See also
<links to relevant docs>
```

`mdatron explain <code>` opens this page in the terminal (TTY) with rustc-style formatting + OSC-8 hyperlinks where supported.

---

## CLI surface

Single binary `mdatron` with subcommand dispatch:

```
mdatron verify                            # run all active patterns against the project
mdatron verify --files <file>...          # selective execution
mdatron verify --format <fmt>             # tty | sarif | json | compact
mdatron verify --phase <name>             # active rule subset (default | pre-commit | strict | ...)
mdatron verify hook <hook-id>             # invoked from external hooks (Python wrappers, CI)
mdatron verify test-error-catalog         # run per-code fixture regression suite

mdatron explain <error-code>              # extended documentation (rustc --explain pattern)

mdatron registry list <name>              # list entries in a declared registry
mdatron registry get <name> --key <k>     # fetch specific entry
mdatron registry diff <name>              # show what changed between two snapshots
mdatron registry validate <name>          # check registry against its schema

mdatron init                              # adopter companion (deploy .mdatron/ skeleton; pre-flight)
mdatron init --check                      # dry-run; report what would be deployed

mdatron schema <class>                    # output JSON Schema for a class (discoverability)

mdatron skill-preamble                    # output the standard skill-preamble text for agent-loop integration

# Reserved for v1.1:
mdatron lsp-serve                         # LSP server (NAMESPACE RESERVED; not implemented v1.0)
mdatron mcp-serve                         # MCP server (NAMESPACE RESERVED; not implemented v1.0)
```

### Behaviors

- **`mdatron verify`** with no `--files`: uses `git diff --name-only` to scope to staged + modified files when run inside a git repo with changes; otherwise scans all configured paths
- **`mdatron verify`** with `--files`: validates only the listed files; useful for hook integration where the caller knows what changed
- **`mdatron explain <code>`**: opens the explain page in `$PAGER` (or `less` default); fails with `MDATRON-E0050: code-unknown` if the code is not in any loaded catalog
- **`mdatron registry list <name>`**: prints registry entries one per line, format configurable via `--format json | tty`
- **`mdatron init`**: pre-flight checks (git repo present; no existing `.mdatron/` content that would conflict); deploys `.mdatron/schemas/`, `.mdatron/patterns/`, `.mdatron/catalogs/`, `.mdatron/config.yaml`, `.mdatron/registries/` skeleton; appends managed sections to `.gitignore`
- **`mdatron init --check`**: emits the deployment plan as JSON (created files, merged files, skipped files); exits 0 if plan is clean; non-zero if conflicts detected

### Edge cases

- mdatron run outside a git repo + no `--files` flag → `MDATRON-W0070: no-git-repo-using-all-files` warning, then validates all files matching configured globs
- mdatron run with no `.mdatron/config.yaml` → `MDATRON-E0060: no-config-file` (project not initialized); exit code 2
- mdatron run with `--phase <unknown>` → `MDATRON-E0061: unknown-phase`; exit code 2
- mdatron run when a pattern's keys index file doesn't exist → `MDATRON-W0090: key-source-missing` warning; rules using the index treat lookups as null

---

## Output formats

Same diagnostics rendered four ways. Format chosen via `--format` flag.

### TTY (default; rustc-style)

Color-coded; source-span carets; OSC-8 hyperlinks where supported.

```
error[VSDD-E0200]: phase-domain-composition-required-domain-missing
  --> .claude/commands/vsdd-phase-2a-red-gate.md:7:1
   |
 7 | relevant_domains:
   |
   = note: Primer vsdd-phase-2a-red-gate declares phase phase-2a but is
           missing required domain(s) [quality-engineer] from the
           phase-domain composition matrix.
   = help: Add the missing domain(s) to relevant_domains, OR update
           .vsdd/registry/phase-domain-matrix.yaml if the matrix is incorrect.
   = explain: mdatron explain VSDD-E0200
```

### SARIF (`--format sarif`)

SARIF 2.1.0 output for GitHub Code Scanning + GitLab + Azure DevOps. SARIF rule definitions generated from the error catalog YAML at release time; ship in `mdatron-core/sarif-rules.json`.

### JSON (`--format json`)

Structured findings stream for programmatic consumers (custom CI tooling, scripted post-processing).

```json
{
  "mdatron_version": "1.0.0",
  "findings": [
    {
      "code": "VSDD-E0200",
      "severity": "error",
      "summary": "phase-domain-composition-required-domain-missing",
      "message": "Primer vsdd-phase-2a-red-gate...",
      "location": {
        "file": ".claude/commands/vsdd-phase-2a-red-gate.md",
        "line": 7,
        "column": 1
      },
      "help": "Add the missing domain(s)...",
      "explain_ref": "docs/error-codes/VSDD-E0200.md"
    }
  ]
}
```

### Compact (`--format compact`)

Terse single-line per finding; optimized for LLM context windows + high-density operator scenarios.

```
.claude/commands/vsdd-phase-2a-red-gate.md:7 error VSDD-E0200 missing required domain quality-engineer; help: add to relevant_domains or update matrix
```

The compact format is **the load-bearing output format for agent-loop integration** (per AIE-F1 expansion). PostToolUse hooks emit compact-format diagnostics; the LLM agent reads them in its next inference step without context-window bloat.

### Mentor voice across all formats

Per TW-F1: Mentor voice is the default tone discipline across operator-facing surfaces. The `message:` template's content sets the tone; mdatron does not transform the adopter's text. Adopters authoring error catalogs follow the Mentor-voice pattern: direct + specific + Mentor-voiced help-text that names what to do, not what was wrong.

Exception: SARIF + JSON outputs strip prose decorations (no rustc-style help-text formatting) since they're machine-consumed. The `help:` field is preserved as a string; downstream tooling renders it.

---

## Agent integration

The PostToolUse hook + compact format + skill-preamble subcommand together form mdatron's **agent-loop integration surface** — load-bearing for VSDD's primary usage pattern per AIE-F1 (Claude Code agents author methodology documents).

### Architecture

Three cooperating mechanisms:

1. **PostToolUse hook** (deployed by `vsdd init` into `.claude/settings.json`): fires after every `Edit` / `Write` / `MultiEdit` tool call on `*.md` files matching mdatron's configured purview. Synchronously invokes `mdatron verify --files <edited-file> --format compact`; appends diagnostics to the tool result.
2. **Compact output format**: terse, single-line-per-finding; optimized for LLM context windows. ~50 characters per finding; no rustc-style multi-line formatting.
3. **`mdatron skill-preamble`** subcommand: outputs standard text adopters paste into domain skill prompts. The preamble instructs the agent to run `mdatron verify` + read explain pages BEFORE producing a review, so mechanical findings don't re-fire as reviewer findings.

### The hook configuration

`vsdd init` deploys this into `.claude/settings.json`:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write|MultiEdit",
        "hooks": [
          {
            "type": "command",
            "command": "mdatron verify --files \"${tool_input.file_path}\" --format compact",
            "timeout_ms": 10000,
            "include_in_tool_result": true,
            "fire_on": {
              "file_patterns": ["*.md"]
            }
          }
        ]
      }
    ]
  }
}
```

Behaviors:

- Hook runs synchronously per tool call; 10s timeout (typical run is <500ms for a single file)
- Output appended to the tool result so the agent's next inference step sees the diagnostics
- Compact format keeps the appended text small (typically <500 tokens for a clean file; <2000 tokens for a file with multiple findings)
- File-pattern filter on `*.md` prevents the hook firing on irrelevant edits (Cargo.toml, source code, etc.)

### Edge cases

- **mdatron not installed**: hook fails; tool result includes a clear error ("mdatron not on PATH; install via `cargo install mdatron`"); does NOT block the Edit/Write from completing
- **mdatron exits non-zero (validation findings exist)**: tool result includes the findings; the Edit/Write itself is treated as successful (the file was written; validation findings are advisory until the agent acts on them)
- **Hook timeout (10s)**: tool result notes the timeout; subsequent file's hook still fires normally
- **Agent edits a file not in mdatron's configured purview**: hook skips silently (no output appended)

### Skill-preamble integration

`mdatron skill-preamble` outputs:

```markdown
## Before producing your review (mdatron-managed projects)

Run these commands and read the output before authoring findings:

1. `mdatron verify --files <target-artifact>` — see what mdatron already mechanically caught
2. `mdatron explain <code>` — for each finding mdatron emitted, read the explain page
3. `mdatron registry list <relevant-registry>` — when your review needs project-wide entity context

Your review focuses on judgment-bearing concerns. Anything mdatron caught is NOT a finding for you to re-raise; it's already routed to the appropriate fix.
```

Adopters paste this preamble (or a customized variant) into each domain skill prompt. vsdd-cli's `vsdd init` deploys the standard preamble into each `.claude/commands/vsdd-domain-*.md` file automatically.

### What's NOT in v1.0 agent integration

- **No MCP server** (`mdatron mcp-serve`). Reserved namespace; v1.1.
- **No LSP server** (`mdatron lsp-serve`). Reserved namespace; v1.1.
- **No bidirectional streaming hook**. The PostToolUse hook is one-shot per tool call; no persistent connection.
- **No tool-result-driven auto-fix**. Diagnostics are advisory; the agent decides whether and how to address them.

---

## mdatron-examples library

mdatron ships 4 generalized artifact-class schemas as the example library, per [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md) § 5. The library is **opt-in** — not loaded by default; adopters reference example schemas explicitly in their `.mdatron/config.yaml`.

| Class | What it validates | Generality |
|---|---|---|
| **DESIGN doc** | Frontmatter with `consumes_from` / `produces_for` cross-references; required body sections (Scope + boundary, Cross-doc coordination, Implementation order) | Any architecture-doc repo benefits |
| **Manual-test** | Frontmatter declaring `test_class`, `target_artifact`, `tested_against`, `expected_outcomes`, `falsifiability_check` | Useful for any project with documented manual-testing discipline |
| **PR template** | PR body conforms to required-fields structure; credential-shaped patterns absent | Any GitHub-using project |
| **CHANGELOG (structural)** | Keep-a-Changelog 1.0.0 compliance: header pattern, `[Unreleased]` section, version-section dates, canonical categories | Any project following Keep-a-Changelog |

### Adoption pattern

```yaml
# .mdatron/config.yaml
examples:
  - design-doc
  - changelog

schemas:
  - source: mdatron-examples://design-doc
    file_glob: "DESIGN-*.md"
  - source: mdatron-examples://changelog
    file_glob: "CHANGELOG.md"

# adopter can also override / extend examples via local schemas:
  - source: .mdatron/schemas/local-pr-template.json
    file_glob: ".github/PULL_REQUEST_TEMPLATE.md"
```

The `mdatron-examples://` scheme resolves to the example schemas embedded in the mdatron binary at build time. Adopters never modify these; if customization is needed, copy + extend in `.mdatron/schemas/`.

### What's NOT in the example library (per SO-F1 speculative-cost discipline)

- **No Methodology spec section schema** (deferred to v1.1 per BOUNDARY-PREAMBLE § 5 row resolution)
- **No methodology-specific schemas** (Review entry, Finding, Phase primer, etc.) — these stay in vsdd-cli
- **No schema-composition mechanism** for v1.0 (JSON Schema `allOf` extension not load-bearing until a second adopter surfaces concrete need)
- **No worked example beyond the 4 above** — each example schema ships with a 1-page README showing the pattern; substantive adopter onboarding lives in the 2 worked-example projects (one VSDD-style, one ADR-style)

---

## Adopter onboarding

### `mdatron init`

The adopter companion. Run once per project:

```
$ cargo install mdatron
$ cd my-typed-docs-project
$ mdatron init
[deploys .mdatron/ skeleton + auto-detects existing schemas + appends .gitignore]
$ mdatron verify
[runs against the project; emits diagnostics in TTY]
```

Deploys:

- `.mdatron/config.yaml` (declares active patterns, schemas, catalogs, registries, phases)
- `.mdatron/schemas/` (empty; adopter places their JSON Schema files here)
- `.mdatron/patterns/` (empty; adopter writes patterns here)
- `.mdatron/catalogs/mdatron.yaml` (mdatron's own engine codes; auto-deployed)
- `.mdatron/registries/` (empty; for cross-file index source data)
- `.gitignore` managed section (excludes `.mdatron/cache/` and similar)

Pre-flight (`mdatron init --check`):

- Git repo present? If not, fail with `MDATRON-E0070: no-git-repo`
- `.mdatron/` directory already exists with conflicting content? If so, fail with `MDATRON-E0220: existing-file-malformed-refuse-to-overwrite` (mirrors vsdd-cli's existing-file-malformed discipline)
- mdatron version compatible with adopter's existing patterns? Reads any existing `.mdatron/patterns/*.yaml` files and checks `mdatron_dsl_version:` declarations
- Report cleanly OR fail with explicit message; never silently partial

### Two worked examples

Per [`V1-SHIP-CRITERIA.md`](./V1-SHIP-CRITERIA.md), the v1.0 documentation includes two worked examples:

1. **VSDD-style example**: a small project demonstrating frontmatter schemas + cross-field rules over methodology-style documents (review entries with classifications; phase primers with composition matrices; the lightweight version of what vsdd-cli itself uses)
2. **ADR-style example** (non-VSDD): an Architecture Decision Record repo demonstrating mdatron applied to a methodology-agnostic adopter — ADR templates, status transitions, cross-reference resolution between ADRs

Both examples live in `mdatron-cli/examples/` as runnable projects. Each ships with:

- `README.md` (5-minute onboarding walkthrough)
- `.mdatron/` (full configuration)
- ~5-10 example documents demonstrating both passing and failing states
- A small test script verifying `mdatron verify` produces expected diagnostics

---

## Path-confinement discipline

Per [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md) § 7. This section operationalizes the discipline; the preamble declares it.

### Why path-confinement matters

mdatron reads files from two PR-modifiable surfaces:

1. **Pattern files** (`.mdatron/patterns/*.yaml`) declare `keys:` whose `source:` field names a file to index
2. **Delegate registrations** (`.mdatron/delegates/*.yaml`) declare `command:` whose value is a binary to invoke

Both are attack surfaces for malicious PRs (per SEC-F1 + SEC-F2 from the 2026-06-01 review).

### Rules

**`key()` sources**:

- Absolute paths rejected at config load: `MDATRON-E0010: key-source-absolute-path`
- Parent-traversing relative paths (`..` segments) rejected: `MDATRON-E0011: key-source-parent-traversal`
- Symlinks not followed; `lstat()` checked at config load; symlink source → `MDATRON-E0012: key-source-symlink`
- Glob patterns may not contain `..` segments; `MDATRON-E0013: key-source-glob-traversal`

**Delegate `command:` registrations**:

- Delegate file committed to project's canonical tree → loadable
- Delegate `command:` reachable on `$PATH` (operator-trusted binary) → loadable
- Delegate `command:` is project-root-relative path → loadable
- Delegate `command:` is absolute path outside project root → rejected; `MDATRON-E0014: delegate-command-outside-project`
- PR-modifiable delegate registration not in operator-signed `.mdatron/delegates-trusted.yaml` → loaded but warning fires: `MDATRON-W0030: delegate-registered-from-pr-modifiable-path`; delegate does NOT execute

**Operator-signed delegate trust**:

```yaml
# .mdatron/delegates-trusted.yaml
trusted_delegates:
  - delegate_id: sycophancy-compensation
    content_hash: sha256:abc123...
    signed_by: <operator-ssh-fingerprint>
    signed_at: <ISO 8601>
```

Mdatron verifies the SSH signature against the operator's registered key (from `.mdatron/config.yaml` `signing_key_fingerprint`). PR cannot self-trust a delegate; only operator-signed entries activate.

### Edge cases

- **Glob source matches a symlink**: symlink targets ignored; emit `MDATRON-W0035: glob-skipped-symlink`
- **Glob source matches a file outside project root** (shouldn't be possible per the no-`..` rule; defensive): hard fail with `MDATRON-E0015: glob-resolved-outside-project`
- **Delegate command is on `$PATH` but resolves to a path outside project root** (e.g., `/usr/local/bin/vsdd`): allowed (operator-trusted; `$PATH` itself is an operator-trusted surface)
- **Symlink in the project tree that mdatron READS for Layer 1 schema validation** (not via `key()`): allowed; schema sources are not under the same threat model as `key()` sources (schema files are operator-controlled at toolkit release time per canonical-schema-path discipline)

---

## Co-evolution with vsdd-cli

Per [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md) § 8. Operationalization below.

### Joint-pass commits

When a change in mdatron requires a corresponding change in vsdd-cli (or vice versa):

- Both PRs reference each other in description bodies
- Neither merges before the other passes CI
- A single Phase 4 → Phase 1a routing commit may touch BOTH repos when boundary-spanning decisions land (cross-doc coordination changes; class-split table revisions; install-order updates)

### Cross-repo citation

vsdd-cli cites mdatron via stable URL form:

```markdown
[DESIGN-MDATRON § Delegate protocol](https://github.com/magnificentlycursed/mdatron-cli/blob/v1.0.0/DESIGN-MDATRON.md#delegate-protocol)
```

Version-pinned URLs only. HEAD-pointing citations rot silently.

The `mdatron://` scheme is reserved (per BOUNDARY-PREAMBLE § 4) for v1+ adopter tooling. v1.0 ships URL-form only.

### Version-coordination event

`MdatronVersionPinned` (declared as candidate event variant in vsdd-cli's `DESIGN-OBSERVABILITY.md` at Step 0.5 mirror commit). vsdd-cli reads its `.vsdd/config.yaml` for the pinned mdatron version; emits the event at `vsdd init` OR `vsdd init --update-mdatron`.

### Drift hook extension

vsdd-cli's `check-methodology-version-drift` hook (`VSDD-W0200`) is extended at Step 2 to also check mdatron-version-drift between vsdd-cli's pin and the installed mdatron's actual version. Same severity + dispatch shape as the methodology-version-drift case.

### Bootstrap-period coordination

During Steps 1-4 (mdatron's bootstrap), vsdd-cli does NOT yet depend on mdatron. The dependency lands at Step 5 (release coupling: vsdd-cli switches from `mdatron-core` local-path-dep to published-crate-dep). The bootstrap-period markers (`BootstrapPeriodEntered` / `BootstrapPeriodExited`) bracket this interval per [`BOOTSTRAP-MITIGATION.md`](./BOOTSTRAP-MITIGATION.md) § Mitigation 3.

---

## Cross-DESIGN-doc coordination

### What this doc produces

| Consumer | Consumes from this doc |
|---|---|
| **vsdd-cli/DESIGN-SCHEMA** | DSL specification + cross-field rule semantics + error catalog format; informs how vsdd-cli's per-class schemas reference mdatron's rule mechanism |
| **vsdd-cli/DESIGN-VERIFICATION** | Hook configuration template + delegate protocol + agent-loop integration (PostToolUse + compact format + skill-preamble); informs how vsdd-cli's 19 hooks compose with mdatron's CLI surface |
| **(Future: any non-VSDD adopter's DESIGN doc)** | Two-layer architecture + DSL spec + error catalog format + path-confinement discipline |

### What this doc consumes

Formal: none. mdatron's design is decidable from its own first principles.

Informal (acknowledged but not declared as `consumes_from:`):

- VSDD's review evidence (2026-06-01 multi-domain review) informed the 5 built-in patterns + the agent-loop integration prioritization
- The Schematron + DSDL precedent informed the two-layer architecture + DSL vocabulary
- OPA Gatekeeper + CUE informed the rule-based-validation-on-modern-formats lineage framing

### What this doc forward-references

| Sibling | This doc forward-references |
|---|---|
| **v1.1-mdatron** | LSP server feature scope (Phase E of V1-SHIP-CRITERIA); MCP server feature scope (Phase D); MCP tool surface (verify, explain, schema, registry, cross_refs) |
| **mdatron-cli/examples/** | Two worked examples (VSDD-style + ADR-style) authored during the documentation phase of V1-SHIP-CRITERIA |
| **vsdd-cli/DESIGN-METHODOLOGY** | Methodology amendment for inline-multi-domain composition shape (M1 from the 2026-06-01 review); methodology-version-drift hook extension |

---

## Implementation order

Per [`V1-SHIP-CRITERIA.md`](./V1-SHIP-CRITERIA.md). v1.0 sequencing:

```
Phase A — Foundational (blocks everything)
  A.1 — mdatron-core engine: DSL parser + evaluator + schema dispatch + cross-file index
  A.2 — Error catalog format + ~30 seeded engine + reserved-prefix codes
  A.3 — 5 built-in patterns
  A.4 — Falsifiability fixtures per code (positive + negative)

Phase B — First usable surface
  B.1 — mdatron-cli subcommand dispatch
  B.2 — `mdatron verify` with 4 output formats (TTY, SARIF, JSON, compact)
  B.3 — `mdatron explain <code>`
  B.4 — `mdatron verify test-error-catalog`
  ──→ Cold-context DSL falsifiability test runs here per BOOTSTRAP-MITIGATION § Mitigation 4

Phase C — Agent integration (load-bearing for VSDD's usage)
  C.1 — PostToolUse hook configuration template + compact format polish
  C.2 — `mdatron registry list/get/diff/validate` subcommands
  C.3 — `mdatron skill-preamble` subcommand + adopter-template text

Phase F — Onboarding
  F.1 — `mdatron init` adopter companion
  F.2 — Pre-flight check (`mdatron init --check`)

Always-alongside
  Release pipeline (cosign + SLSA + per-platform binaries × 4 platforms)
  Documentation (this DESIGN-MDATRON; README with TRON-blockchain disambiguation per TW-F3; DSL reference; adopter onboarding; 2 worked examples)
```

### Held for v1.1

```
Phase D — MCP surface
  `mdatron mcp-serve` + 5 structured tools (verify, explain, schema, registry, cross_refs)

Phase E — Human integration
  `mdatron lsp-serve` LSP server
  IDE extension manifests (VS Code, Neovim)
```

Both namespaces reserved at v1.0; implementations land in v1.1 per adopter-evidence trigger.

### Implementation note (per SO-F3: build-in-mdatron-shape-from-day-one)

`mdatron-core` development begins at Step 3 simultaneously with vsdd-cli's `vsdd init` implementation. vsdd-cli depends on `mdatron-core` as a Rust **local-path-dependency** from Step 3 onward. Step 5 is the release-coupling change (local-path-dep → published-crate-dep), not a refactor wave.

The bootstrap-validator.py (per [`BOOTSTRAP-MITIGATION.md`](./BOOTSTRAP-MITIGATION.md) § Mitigation 1) covers structural drift during Steps 1 through ~mid-Step-3, then deletes once `mdatron-core verify` runs against the project.

---

## v1.1 surfaces (reserved)

### LSP server (`mdatron lsp-serve`)

Reserved namespace. v1.1 deliverable. Specifics deferred until adopter-evidence trigger; sketched here for design-coherence.

Expected capabilities:

- Document parsing on open + edit
- Diagnostics push (rules + schemas evaluated incrementally)
- Hover content from explain pages
- Quick-fix code actions for common errors
- Autocomplete from schemas + registries (e.g., the LSP experience mockups from the originating conversation)
- Cross-document anchor resolution for go-to-definition
- Real-time index update on file change
- IDE configurations: VS Code extension manifest + Neovim LSP config snippet (at minimum)

### MCP server (`mdatron mcp-serve`)

Reserved namespace. v1.1 deliverable. Tool surface sketched:

| Tool | Purpose |
|---|---|
| `verify` | Validate a file or files; return structured findings |
| `explain` | Get explain content for a code |
| `schema` | Query a schema by class |
| `registry` | Query the vocabulary / composition-matrix / classification-universe registries |
| `cross_refs` | Resolve cross-document anchor references |

MCP server runs as a long-running stdio process. Skill integration depth 3 (per AIE-F3 expansion) lands here.

### v1.1 trigger criteria

Per [`V1-SHIP-CRITERIA.md`](./V1-SHIP-CRITERIA.md): v1.1 begins when (a) ≥2 adopters request LSP, OR (b) ≥2 adopters or skill-authoring patterns demonstrate MCP need, OR (c) vsdd-cli's domain-skill authoring loop hits a ceiling on the registry CLI + skill-preamble integration, OR (d) operator directive declares v1.1.

---

## Open decisions deferred

| Decision | Routing |
|---|---|
| Specific timeline for v1.0 | Operator-time-bound; exit criteria are objective per V1-SHIP-CRITERIA |
| Whether Phase D (MCP) and Phase E (LSP) ship simultaneously in v1.1 or sequentially | TBD when v1.1 begins; depends on adopter-evidence shape |
| Visual catalog explorer (HTML report of the error catalog) | v1.x candidate; CLI is sufficient for v1.0 |
| Schema migration subcommand (`mdatron verify migrate <class>`) | v1+ candidate; defer until first major-version-bump cycle surfaces concrete need |
| User-defined DSL functions | v1.x candidate; observe what the v1 standard library does not cover |
| Cross-language hook execution beyond Python | Standard cross-language pattern; not v1.0-blocking |
| Multi-language source repos (Python + JS + Rust mixed) | YAGNI; revisit if adoption evidence shows mixed-language docs projects |
| Methodology spec section class (per BOUNDARY-PREAMBLE § 5 resolution) | Deferred to v1.1; revisit when a real non-VSDD adopter surfaces the need |
| Backend-specific output formats (Honeycomb, Datadog presets) | Adopter-configuration; v1 emits SARIF + JSON + TTY; covers standard ingestion paths |

---

## Closing

DESIGN-MDATRON declares mdatron's v1.0 architecture: a two-layer validator (JSON Schema structural + Schematron-derived DSL semantic) shipped as a single Rust binary with rustc-shaped diagnostics, 5 built-in patterns covering cascade-prone structural drift, a path-confined delegate protocol for imperative escape-hatches, agent-loop integration via PostToolUse hook + compact format + skill-preamble subcommand, and an example library of 4 generalized artifact-class schemas.

The design is methodology-agnostic by construction. VSDD is the first adopter; the toolkit's value as foundation for any "typed-markdown documents with cross-reference integrity" project is the deliberate v1.0 framing per SO-F1.

LSP + MCP surfaces are reserved namespaces; v1.1 candidates pending adopter-evidence trigger.

The bootstrap period (Steps 1-4) operates under the bounded mitigations declared in [`BOOTSTRAP-MITIGATION.md`](./BOOTSTRAP-MITIGATION.md). Step 1 (this document) closes the substantive design surface. Step 2 (vsdd-cli scope-adjust) interleaves per [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md) § 8 co-evolution discipline. Steps 3-5 implement, validate, and release-couple.

**Next:** Step 1 falsifiability check (per [`BOOTSTRAP-MITIGATION.md`](./BOOTSTRAP-MITIGATION.md) § Mitigation 4) — cold-context Claude session authors 5-10 DSL rules using only this document's DSL specification as reference; ≥80% pass rate validates the DSL spec; below 80% triggers revision.
