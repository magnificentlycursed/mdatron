# mdatron

**A Rust CLI that validates markdown documents using JSON Schema (frontmatter)
and a small Schematron-derived DSL (cross-field rules).** Descended from XML's
Schematron (ISO/IEC 19757-3); not related to the TRON blockchain despite the
`-tron` suffix.

mdatron validates markdown documents in two layers:

- **Layer 1 — Structural.** JSON Schema (draft 2020-12) over the frontmatter.
  Required fields, enums, types, `additionalProperties: false`. Universal
  vocabulary; zero learning curve for anyone who has authored an OpenAPI
  schema, a Kubernetes CRD, or a tsconfig.
- **Layer 2 — Semantic.** A small Schematron-derived DSL over cross-field,
  cross-file, and cross-document constraints. The 80% of validation value
  that JSON Schema cannot express: "the number of rows in this table matches
  the count declared in frontmatter," "every domain listed here is registered
  in `.vsdd/registry/`," "every link target resolves to a heading in the
  project."

Where mdatron fits relative to neighbouring tooling: markdownlint enforces
style; Vale catches prose-quality concerns; dprint and mdformat reformat;
mdatron is the only validator built around the typed-frontmatter + cross-
document rules pattern. Errors are rustc-shaped — codes, source spans,
`= help:` hints, `= explain:` references to per-code prose, structured JSON
output for machine consumers.

## Install

mdatron is in its bootstrap period (v0.1.0); the crates.io publish is gated
on Phase 6 of the [binary-first refactor plan](
../vsdd-cli/docs/refactor/binary-first-plan.md). For now, install from a
local checkout of the `magnificentlycursed` monorepo:

```
git clone https://github.com/magnificentlycursed/magnificentlycursed
cargo install --path magnificentlycursed/mdatron/mdatron-cli --locked
mdatron --version
```

(The `mdatron` subdirectory lives inside the parent `magnificentlycursed`
repo; standalone-`mdatron` packaging lands at Phase 6.)

The `--locked` flag pins transitive dependencies to `Cargo.lock`; recommended
for reproducible builds and CI.

Once mdatron 0.1.0 is published to crates.io, the install command becomes:

```
cargo install mdatron --locked --version "0.1.0"
```

(Version-pin to avoid unintentional upgrades when the crate publishes.)

## First run

Create a minimal project that mdatron can validate:

```
mkdir my-typed-docs
cd my-typed-docs
mkdir -p .mdatron/schemas .mdatron/patterns
```

The `.mdatron/` directory is required: running `mdatron verify` against a
project without it exits 2 with `MDATRON-E0080: pipeline-orchestration-
failure`. (`mdatron init`, v0.1.x, will scaffold this automatically.)

Drop a JSON Schema at `.mdatron/schemas/blog.json` (a Layer 1 example follows
below), drop a markdown file at the project root with matching frontmatter,
and run:

```
mdatron verify
```

A clean run looks like:

```
$ mdatron verify
mdatron verify: clean
```

A run with diagnostics emits rustc-shape blocks on stderr and exits 1:

```
$ mdatron verify
error[MDATRON-E0050]: frontmatter-schema-violation
  --> bad.md:1
   = note: Additional properties are not allowed ('extra' was unexpected)
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

The `= explain:` line is copyable: paste `mdatron explain MDATRON-E0050` into
your shell to read the per-code prose. Pipeline failures (missing schema
directory, malformed pattern file, IO failure) print on stderr and exit 2.
The `--json` flag emits a single JSON output object on stdout for machine
consumers; stderr still carries the rustc-shaped diagnostics unless you also
pass `--quiet`.

## Pre-commit integration

Wire `mdatron verify` into your pre-commit hook so typed-document errors block
the commit that introduces them. Make the wrapper **fail closed**: if the
`mdatron` binary is missing — not yet installed, off `PATH`, or absent from the
hook's shell environment — block the commit rather than skip the check
silently. A validator that silently skips is invisible in exactly the moment it
is needed.

```sh
#!/bin/sh
# .git/hooks/pre-commit  (or your pre-commit-framework entry)
if ! command -v mdatron >/dev/null 2>&1; then
    echo "pre-commit: mdatron not found on PATH — refusing to commit unverified." >&2
    echo "  install it (see Install above) or bypass explicitly with 'git commit --no-verify'." >&2
    exit 1
fi
mdatron verify
```

`mdatron verify` exits `0` when clean, `1` on findings, and `2` on a pipeline
failure (see First run). The wrapper above blocks on all three of a missing
binary, findings, and pipeline failure. Reserve `git commit --no-verify` for a
deliberate, visible bypass rather than letting a missing checker pass unseen.

## Schema example (Layer 1)

A minimal blog-post schema that requires `schema_class`, `title`, and
`published_on`; rejects extra frontmatter fields:

<!-- mdatron-roundtrip:schema-start -->
```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["schema_class", "title", "published_on"],
  "properties": {
    "schema_class": { "const": "blog" },
    "title": { "type": "string", "minLength": 1 },
    "published_on": { "type": "string", "format": "date" }
  },
  "additionalProperties": false
}
```
<!-- mdatron-roundtrip:schema-end -->

A markdown post that satisfies it:

<!-- mdatron-roundtrip:md-start -->
```md
---
schema_class: blog
title: hello mdatron
published_on: 2026-06-07
---

# Hello, mdatron

This file's frontmatter binds to the `blog` schema because the
`schema_class:` field selects it.
```
<!-- mdatron-roundtrip:md-end -->

Drop both files into a project, run `mdatron verify`, and the file passes
Layer 1. Add a frontmatter field the schema does not allow (e.g.,
`extra: "nope"`) and Layer 1 emits `MDATRON-E0050:
frontmatter-schema-violation`. Run `mdatron explain MDATRON-E0050` for the
per-code prose.

## Pattern example (Layer 2)

JSON Schema is great for shapes but cannot express "this `published_on` must
not be in the future." That is Layer 2 territory — DSL patterns at
`.mdatron/patterns/<name>.yaml`:

<!-- mdatron-roundtrip:pattern-start -->
```yaml
mdatron_dsl_version: 1
pattern:
  id: blog-validation
  rules:
    - id: title-must-be-set
      context: blog
      assert: $self.title != ""
      code: MDATRON-W0100
      message: "title is empty for the blog post"
```
<!-- mdatron-roundtrip:pattern-end -->

`context: blog` selects every file whose frontmatter `schema_class` is `blog`.
`assert:` fires the diagnostic when the expression evaluates to `false`. See
[`DESIGN.md`](./DESIGN.md) § Cross-file semantics stay narrowed for the
operator + function reference (`every`, `some`, `in`, `defined`, `count`,
`len`, `union`, `intersect`, `difference`, `concat`, `join`, and the
path-confined `key()` cross-file index mechanism). The DSL's scope is
cross-file and registry validation; body-content extraction functions are
out of scope.

## Relationship to vsdd

**mdatron is methodology-agnostic.** The Layer 1 + Layer 2 architecture is
useful for any "typed markdown documents with cross-reference integrity"
project — Architecture Decision Records, RFC collections, structured
changelogs, methodology specs. If you're not adopting VSDD, you can stop
reading this section here.

If you *are* adopting VSDD: [vsdd](../vsdd-cli/) is the first downstream
adopter of mdatron and the source of the methodology vocabulary (phase
primers, domain prompts, finding artifacts, the VSDD whitepaper alignment).
vsdd composes mdatron in two ways:

- **vsdd's `verify` subcommand spawns `mdatron verify --json`** as a
  subprocess and parses the output object on stdout per the
  [Phase 0 output-format contract](
  ../vsdd-cli/docs/refactor/phase-0-output-format/DESIGN.md). Error-code
  namespaces stay strictly separate: mdatron emits `MDATRON-Exxxx`, vsdd
  emits `VSDD-Exxxx`. No proxy, no intercept.
- **vsdd ships its own JSON Schemas and DSL patterns** that adopters deploy
  into `.mdatron/schemas/` and `.mdatron/patterns/` via `vsdd init`. The
  methodology is encoded as mdatron schemas + patterns; mdatron is the
  engine, not the methodology.

The
mdatron-examples library (deferred to adopter evidence; see [`DESIGN.md`](./DESIGN.md) § References, absorption ledger)
(v1.0 candidate; not in v0.1.0 — see
[`V1-SHIP-CRITERIA.md`](./V1-SHIP-CRITERIA.md)) will ship four generalized
artifact-class schemas (DESIGN doc, manual-test, PR template, CHANGELOG) for
non-VSDD adopters.

## Where to go next

- [`DESIGN.md`](./DESIGN.md) — the standing design: agent-first conformance engine, check families, two-layer
  architecture, DSL reference, error catalog format, agent-loop integration,
  built-in patterns, path-confinement discipline
- [`V1-SHIP-CRITERIA.md`](./V1-SHIP-CRITERIA.md) — what v1.0 ships and what
  is reserved for v1.1 (LSP server, MCP server)
- `mdatron explain <code>` — per-code prose for any emitted diagnostic; the
  v0.1.0 baseline catalog covers `MDATRON-E0001`, `E0002`, `E0050`, `E0070`,
  `E0080`; the catalog grows by one entry per newly-emitted code
- [vsdd-cli](../vsdd-cli/) — if you are adopting VSDD, the vsdd toolkit
  composes mdatron + ships the methodology artifacts; `vsdd init` deploys
  both
- [The VSDD whitepaper](
  https://gist.github.com/dollspace-gay/d8d3bc3ecf4188df049d7a4726bb2a00) —
  the methodology vsdd operationalizes; authored by
  [@dollspace.gay](https://bsky.app/profile/dollspace.gay)

## License

See [`LICENSE`](./LICENSE).
