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

mdatron is pre-publish (v0.3.x); the crates.io release is tracked as the
v1.0 publish milestone (tracker #48). For now, install from a checkout:

```
git clone https://github.com/magnificentlycursed/mdatron
cargo install --path mdatron --locked
mdatron --version
```

The `--locked` flag pins transitive dependencies to `Cargo.lock`; recommended
for reproducible builds and CI.

Once mdatron is published to crates.io, the install command becomes:

```
cargo install mdatron --locked --version "0.3.0"
```

(Version-pin to avoid unintentional upgrades when the crate publishes.)

## First run

Scaffold with `mdatron init`, which deploys the `.mdatron/` skeleton — the
`schemas/` and `patterns/` directories, a seeded `config.yaml` (adopter-owned
from then on), and the managed-partition manifest:

```
mkdir my-typed-docs && cd my-typed-docs
mdatron init
```

`config.yaml`'s `file_globs` are your declared **jurisdiction**: mdatron walks
only what they claim, so third-party markdown deployed into your tree by other
tools is never mdatron's to refuse. A tree with no config refuses loudly
(`no jurisdiction declared`) rather than guessing — pass explicit `--files`
globs for an ad-hoc run without one. Re-running `init` is a no-op on an intact
tree; a hand-modified *managed* file is refused with `MDATRON-E0060`.

Drop a JSON Schema at `.mdatron/schemas/blog.json` (a Layer 1 example follows
below), drop a markdown file with matching frontmatter inside your globs, and
run:

```
mdatron verify
```

A clean run looks like:

```
$ mdatron verify
mdatron verify: clean
```

A run with diagnostics emits rustc-shape blocks on stderr and exits 1. Your
document's own text never rides inline in an engine line — it renders as a
prefix-marked quoted block:

```
$ mdatron verify
error[MDATRON-E0050]: frontmatter-schema-violation
  --> bad.md:3:1
   = note: unexpected property not permitted by the schema
   = unexpected:
           > extra
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

The `= explain:` line is copyable: paste `mdatron explain MDATRON-E0050` into
your shell to read the per-code prose. Pipeline failures (missing config,
malformed pattern file, IO failure) print on stderr and exit 2. Two machine
forms share the same findings: `--json` emits a single output object on
stdout, and `--compact` emits one size-capped block per finding (512 bytes,
a contract limit) for agent-context consumers; add `--quiet` to silence the
stderr rendering.

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

## Conformance families (Layer 2 data)

Beyond schemas and rule patterns, four generic engines activate on adopter
data under `.mdatron/` — each inactive until its file exists, each strict-
parsed, every path confined to the governed tree:

**Routes** (`routes.yaml`) — the closed-world allowlist:

```yaml
routes:
- files: "review-log/**/*.md"
  governed_by: DESIGN.md
  naming: "^[0-9]{4}-[0-9]{2}-[0-9]{2}-[a-z0-9-]+\\.md$"   # optional
  citations: true                                        # optional, see below
```

With routes supplied: an unclaimed walked file blocks (`E0030`), a route
citing an absent governing document blocks (`E0031`), two routes claiming one
file is an error (`E0032`), and a filename underivable from the `naming`
grammar warns (`W0041`).

**Pins** (`pins.yaml`) — governing documents pin sha256 over governed files:

```yaml
pins:
- governing: DESIGN.md
  file: src/codes.rs
  sha256: "…"
```

A governed-file change with a stale pin fails (`E0061`) until you re-read the
governing document and re-pin: `mdatron pin --update` (preview with
`--dry-run`; bare `mdatron pin` checks). Un-pinning persists as a justified
`unpinned:` tombstone that stays loud as an informational lint (`L0001`);
an unjustified one warns (`W0042`).

**Vocabulary** (`vocabulary.yaml`) — registry-driven prose scan: unregistered
bold-introduced coinages (`E0090`, draft-status exempt), letter-plus-number
label clusters outside your allowlist (`E0091`), reserved-word use (`E0092`),
listed register anti-patterns (`E0093`), and numeric claims — a prose numeral
restating a configured frontmatter field's count and drifting from it
(`E0094`). By default the scan covers every walked file; set `vocabulary_globs`
in `config.yaml` (a scope list beside `require_frontmatter`) to restrict it —
e.g. to apply the register to your live specs while leaving a historical archive
walked and routed but unscanned. A `vocabulary_globs` that matches nothing is
loud (`W0043`), so a mistyped glob can't silently disable the register. A term
declared both `registered` and `draft` resolves to draft with a warning
(`W0044`), so a conflicting declaration is surfaced, not silently resolved.

**Citations** — data-less; opt a route in with `citations: true` and its
files' `path:line` / `path:start-end` references are verified against the
working tree (uncommitted content counts; no git subprocess): a dead citation
blocks (`E0100`), one past the target's end blocks (`E0101`). Historical
corpora simply don't opt in.

Every family code has an explain page: `mdatron explain MDATRON-E0061`.

## Onboarding: the init-and-hook path

The adoption sequence, each step optional after the first:

1. `mdatron init` — scaffold; scope `file_globs` in `config.yaml` to your
   typed corpus (your jurisdiction).
2. Add a schema per `schema_class` under `.mdatron/schemas/`; opt strictness
   in with `require_frontmatter` globs in `config.yaml` (`W0040` flags a
   governed file that silently lacks frontmatter).
3. Route your corpus (`routes.yaml`) to its governing documents; add a
   `naming` grammar if filenames are a contract.
4. Pin what governs you (`pins.yaml` + `mdatron pin --update`) so governed
   drift blocks instead of rotting.
5. Wire the fail-closed pre-commit hook (next section) and a CI job that
   builds mdatron and runs `mdatron verify --project-root .` — a repository
   that verifies itself is the intended end state (this repo's own
   `self-validate` CI job is the worked example).

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

A generalized examples library (artifact-class schemas for non-VSDD adopters)
is deferred to adopter evidence per the absorption ledger — see
[`DESIGN.md`](./DESIGN.md) § References.

## Where to go next

- [`DESIGN.md`](./DESIGN.md) — the standing design: behavioral contracts,
  the five check families, output marking discipline, path confinement,
  governance-data governance
- [`docs/dsl-reference.md`](./docs/dsl-reference.md) — the complete Layer 2
  construct inventory with evaluation semantics; validated by a cold-context
  authoring campaign at 100% one-pass (`dsl-falsifiability-report.md`)
- `mdatron explain <code>` — per-code prose for every emitted diagnostic
  (frontmatter, confinement, schema, init, jurisdiction, route, pin,
  vocabulary, and citation codes); the catalog grows by one entry per
  newly-emitted code
- [vsdd-cli](../vsdd-cli/) — if you are adopting VSDD, the vsdd toolkit
  composes mdatron + ships the methodology artifacts; `vsdd init` deploys
  both
- [The VSDD whitepaper](
  https://gist.github.com/dollspace-gay/d8d3bc3ecf4188df049d7a4726bb2a00) —
  the methodology vsdd operationalizes; authored by
  [@dollspace.gay](https://bsky.app/profile/dollspace.gay)

## License

See [`LICENSE`](./LICENSE).
