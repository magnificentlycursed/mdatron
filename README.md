# mdatron

**A Rust CLI for typed-markdown validation.** Descended from XML's Schematron
(ISO/IEC 19757-3) and applied to markdown documents with YAML frontmatter; not
related to the TRON blockchain despite the `-tron` suffix.

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

Errors are rustc-shaped — codes, source spans, `= help:` hints, `= explain:`
references to per-code prose, structured JSON output for machine consumers.

## Install

mdatron is in its bootstrap period (v0.1.0); the crates.io publish is gated
on Phase 6 of the [binary-first refactor plan](
../vsdd-cli/docs/refactor/binary-first-plan.md). Install from a local checkout
for now:

```
git clone https://github.com/magnificentlycursed/mdatron
cargo install --path mdatron/mdatron-cli --locked
mdatron --version
```

The `--locked` flag pins transitive dependencies to `Cargo.lock`; recommended
for reproducible builds and CI.

Once mdatron 0.1.0 is published, the install command becomes:

```
cargo install mdatron --locked
```

## First run

Create a minimal project that mdatron can validate:

```
mkdir my-typed-docs
cd my-typed-docs
mkdir -p .mdatron/schemas .mdatron/patterns
```

Drop a JSON Schema at `.mdatron/schemas/blog.json` (a Layer 1 example follows
below), drop a markdown file at the project root with matching frontmatter,
and run:

```
mdatron verify
```

A clean run prints `mdatron verify: clean` and exits 0. A run with diagnostics
prints them in rustc-shape on stderr and exits 1; pipeline failures (missing
schema directory, malformed pattern file, IO failure) print on stderr and exit
2. The `--json` flag emits a single JSON output object on stdout for machine
consumers; stderr still carries the rustc-shaped diagnostics unless you also
pass `--quiet`.

## Schema example (Layer 1)

A minimal blog-post schema that requires `schema_class`, `title`, and
`published_on`; rejects extra frontmatter fields:

```json
{
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

A markdown post that satisfies it:

```markdown
---
schema_class: blog
title: hello mdatron
published_on: 2026-06-07
---

# Hello, mdatron

This file's frontmatter binds to the `blog` schema because the
`schema_class:` field selects it.
```

Drop both files into a project, run `mdatron verify`, and the file passes
Layer 1. Add a frontmatter field the schema does not allow (e.g.,
`extra: "nope"`) and Layer 1 emits `MDATRON-E0050:
frontmatter-schema-violation`. Run `mdatron explain MDATRON-E0050` for the
per-code prose.

## Pattern example (Layer 2)

JSON Schema is great for shapes but cannot express "this `published_on` must
not be in the future." That is Layer 2 territory — DSL patterns at
`.mdatron/patterns/<name>.yaml`:

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

`context: blog` selects every file whose frontmatter `schema_class` is `blog`.
`assert:` fires the diagnostic when the expression evaluates to `false`. See
[`DESIGN-MDATRON.md`](./DESIGN-MDATRON.md) § DSL specification for the full
operator + function reference (`every`, `some`, `key`, `match`, `slug`,
`headings`, `tables`, `links`, and the path-confined `key()` cross-file index
mechanism).

## Relationship to vsdd

[vsdd](../vsdd-cli/) is the first downstream adopter of mdatron and the source
of the methodology vocabulary (phase primers, domain prompts, finding
artifacts, the VSDD whitepaper alignment). vsdd composes mdatron in two ways:

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

mdatron itself is methodology-agnostic. The Layer 1 + Layer 2 architecture is
useful for any "typed markdown documents with cross-reference integrity"
project — Architecture Decision Records, RFC collections, structured
changelogs, methodology specs. The mdatron-examples library (v1.0
candidate; not in v0.1.0 — see [`V1-SHIP-CRITERIA.md`](./V1-SHIP-CRITERIA.md))
will ship four generalized artifact-class schemas (DESIGN doc, manual-test,
PR template, CHANGELOG) for non-VSDD adopters.

## Where to go next

- [`DESIGN-MDATRON.md`](./DESIGN-MDATRON.md) — the canonical design: two-layer
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
