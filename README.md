# mdatron

**A Rust CLI conformance engine for typed markdown: JSON Schema over
frontmatter, nine data-driven check families, and a small Schematron-derived
DSL for cross-file rules.** Descended from XML's Schematron (ISO/IEC 19757-3).

mdatron checks markdown documents on two axes:

- **Structure.** JSON Schema (draft 2020-12) over the frontmatter — the schema
  family. Required fields, enums, types, `additionalProperties: false`.
  Universal vocabulary; zero learning curve for anyone who has authored an
  OpenAPI schema, a Kubernetes CRD, or a tsconfig.
- **Semantics.** Cross-field, cross-file, and cross-document constraints — the
  eight further check families, driven by adopter data, plus a small
  Schematron-derived rule DSL over frontmatter. The 80% of validation value
  that JSON Schema cannot express: "the number of rows in this table matches
  the count declared in frontmatter," "every owner listed here appears in the
  team registry," "every link target resolves to a heading in the project."

Where mdatron fits relative to neighbouring tooling: markdownlint enforces
style; Vale catches prose-quality concerns; dprint and mdformat reformat;
mdatron is the only conformance engine built around the typed-frontmatter + cross-
document rules pattern. Errors are rustc-shaped — codes, source spans,
`= help:` hints, `= explain:` references to per-code prose, structured JSON
output for machine consumers.

Why it matters now: when agents write and read your documents, a document
stops being prose and becomes **cache** — derived state that everything
downstream trusts. *Observability Engineering* (2nd ed.) frames the test for
any such document: can you validate it where it lives? Can your coworker? Can
an agent? mdatron is the "yes" to all three — the validation runs in CI, from
one committed configuration, with output built for both human and machine
readers.

## Install

mdatron ships as an early **0.x** crate — usable now, with the CLI and the
`--json` diagnostic contract still evolving under SemVer (pre-1.0, a breaking
change moves the `0.MINOR` position, so `^0.5` will not silently pull a breaking
`0.6`). Install from crates.io:

```
cargo install mdatron --locked
```

Pin the minor to avoid an unintended breaking upgrade in CI:

```
cargo install mdatron --locked --version "0.6"
```

`--locked` pins transitive dependencies to the shipped `Cargo.lock`; recommended
for reproducible builds. To build from a checkout instead (the clone creates a
`mdatron/` directory, so `--path mdatron` resolves to it):

```
git clone https://github.com/magnificentlycursed/mdatron
cargo install --path mdatron --locked
mdatron --version
```

## First run

Scaffold with `mdatron init`, which deploys the `.mdatron/` skeleton — the
`schemas/` and `patterns/` directories, a seeded `config.yaml` (adopter-owned
from then on), and the init manifest (the record of the engine-managed
partition):

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

Drop a JSON Schema at `.mdatron/schemas/blog.json` (a schema example follows
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
your shell to read the per-code prose — it names the fix (here: remove the
undeclared property, or declare it in the schema). Apply it and re-run:
`mdatron verify` → `clean`. That loop — violation, coded diagnostic, explain
page, fix, clean — is the working rhythm, for you and equally for an agent
reading the machine forms below. Pipeline failures (missing config,
malformed pattern file, IO failure) print on stderr and exit 2. The exit
contract is exactly three values: `0` clean, `1` findings at error severity,
`2` pipeline failure — anything else (a raw panic code, a signal abort) is an
engine defect, never a documented mode; please report it. Two machine
forms share the same findings: `--json` emits a single output object on
stdout, and `--compact` emits one size-capped block per finding (512 bytes,
a contract limit) for agent-context consumers; add `--quiet` to silence the
stderr rendering (and, under `--json`, to keep stdout the only stream).

The `--json` envelope is a published, versioned contract (`mdatron_output_version`,
currently `3.1.0`). The load-bearing fields for a machine consumer:

- `pipeline_status` — `"ok"` or `"failed"`; on failure, `pipeline_error`
  `{code, kind, message}` carries the reason **in-band** (it survives `--quiet`),
  `kind` disambiguating the failure class (`config`, `io`, `bound_exceeded`, …).
- `summary.files_checked` — the true count of files **validated** (a clean run
  over N reports N, not 0).
- `families` — each of the nine check families (schema, route, pin,
  vocabulary, citation, link, marker, code_catalog, section) and the rule-DSL
  lane (`rule_dsl`) as
  `{state, reason}` (`active` / `inert` / `inactive`), so "checked N, all
  clean" is distinguishable from "checked nothing"; the object is
  forward-extensible, so a consumer must tolerate unknown family keys.
- `findings[].code` / `.severity` / `.location`; and `findings[].quoted[]`,
  which carries adopter-derived text marked `origin: "adopter"`, `trusted: false`
  so a consumer never mistakes document content for engine output.
- `findings[].fingerprint` — a line-churn-stable `v1:` identity for cross-run
  trending: the same defect keeps the same fingerprint across reflows and
  across operating systems, so a baseline or a trend line keys on it, not on
  line numbers.
- `envelope_schema` — the published schema's `$id`, verbatim: the exact
  contract this envelope was produced under.
- `inputs` — governance-input lineage: each configuration input the run
  actually read, mapped to a `sha256:` digest of the bytes it consumed —
  attest *which* configuration produced a verdict, and detect config drift
  between runs.
- `timings` — optional, only under `verify --json --timings`: per-phase
  wall-clock (`load`/`capture`/`check`/`total`, milliseconds). Flag-gated so
  the default envelope stays byte-identical across runs on an unchanged tree
  (determinism is a contract property).

Pin and validate against the schema at
[`schema/mdatron-output.schema.json`](schema/mdatron-output.schema.json); a
binary-only install can print it with `mdatron envelope-schema`. `mdatron explain --list`
enumerates every diagnostic code; `mdatron explain <code>` (the short form
`E0050` works too) shows a code's page.

For fast local runs, `mdatron verify --changed <file>` verifies only the changed
file plus its transitive dependents (a `.mdatron/` change falls back to
whole-tree); whole-tree verify remains the gate and CI mode.

## Pre-commit integration

Wire `mdatron verify` into your pre-commit hook so typed-document errors block
the commit that introduces them. Make the wrapper **fail closed**: if the
`mdatron` binary is missing — not yet installed, off `PATH`, or absent from the
hook's shell environment — block the commit rather than skip the check
silently. A checker that silently skips is invisible in exactly the moment it
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
failure (see First run). Add `--deny-warnings` (alias `--strict`) to also fail a
warnings-only run (exit `0 → 1`) — the switch a hard CI gate wants when warnings
must block. In particular, `MDATRON-W0048` (a reference check *skipped* over a
present-but-unverifiable target — non-UTF8, unreadable, oversized) is a warning
by design; a gate that must not pass an unverified reference needs this switch
(this repo's own self-validation CI job uses it). The wrapper above blocks on all three of a missing
binary, findings, and pipeline failure. Reserve `git commit --no-verify` for a
deliberate, visible bypass rather than letting a missing checker pass unseen.

## Schema example (the schema family)

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
the schema family. Add a frontmatter field the schema does not allow (e.g.,
`extra: "nope"`) and it emits `MDATRON-E0050:
frontmatter-schema-violation`. Run `mdatron explain MDATRON-E0050` for the
per-code prose.

## Pattern example (the rule DSL)

JSON Schema is great for shapes but cannot express "this `published_on` must
not be in the future." That is rule-DSL territory — DSL patterns at
`.mdatron/patterns/<name>.yaml`:

<!-- mdatron-roundtrip:pattern-start -->
```yaml
mdatron_dsl_version: 1      # optional (absent = v1); unsupported versions refuse at load
pattern:
  id: blog-validation
  rules:
    - id: title-must-be-set
      context: blog
      assert: $self.title != ""
      code: BLOG-W0001
      message: "title is empty for the blog post"
```
<!-- mdatron-roundtrip:pattern-end -->

`context: blog` selects every file whose frontmatter `schema_class` is `blog`.
`assert:` fires the diagnostic when the expression evaluates to `false`. See
[`docs/dsl-reference.md`](./docs/dsl-reference.md) for the **complete,
canonical** operator + function inventory — including `every`, `some`, `in`,
`not_in`, `filter`, `let:` bindings, `defined`, `count`, `len`, `union`,
`intersect`, `difference`, `concat`, `join`, and the path-confined `key()`
cross-file index mechanism (this list is illustrative, not exhaustive). The
DSL's scope is
cross-file and registry validation; body-content extraction functions are
out of scope.

## Check families

The schema family is the first of **nine** check families. Beyond it, eight
generic families activate on adopter data under `.mdatron/` — each
inactive until its data exists, each strict-parsed, every path confined to the
governed tree:

**Routes** (`routes.yaml`) — the closed-world allowlist:

```yaml
routes:
- files: "docs/adr/**/*.md"
  governed_by: DESIGN.md
  naming: "^[0-9]{4}-[a-z0-9-]+\\.md$"                   # optional
  citations: true                                        # optional, see below
  links: true                                            # optional, see below
  link_root: true                                        # optional: resolve /root-relative links (needs links)
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
- governing: contract.md            # optional: pin ONE section, not the whole file
  file: plan/build-plan.md
  section: "## Decomposition"       # the heading whose span is hashed
  sha256: "…"
```

A governed-file change with a stale pin fails (`E0061`) until you re-read the
governing document and re-pin: `mdatron pin --update` (preview with
`--dry-run`; bare `mdatron pin` checks). Add `section:` to scope a pin to one
**heading-delimited span** — the hash covers that heading through just before
the next heading of the same or higher level, so an edit elsewhere in the file
doesn't trip it; a `section:` whose heading can't be found is loud (`E0063`).
Un-pinning persists as a justified `unpinned:` tombstone — justified means it
carries both a `reason` and an `owner` — that stays loud as an informational
lint (`L0001`); one missing either field warns (`W0042`):

```yaml
unpinned:
- governing: contract.md
  file: plan/build-plan.md
  reason: "plan superseded by ROADMAP.md 2026-09-01; kept for history"
  owner: "docs-maintainers"
```

**Vocabulary** (`vocabulary.yaml`) — registry-driven prose scan. A coined term
with a definition but no codified governance drifts until it is lost — the
authors of *Observability Engineering* coined "observability" precisely and
watched vendors redefine it out from under them; this family is that
codification for your project's own terms. The registry:

```yaml
mdatron_format_version: 1
terms:
  - term: "jurisdiction"
    status: registered        # a coined term, formally registered
    sense: "the file set the declared file_globs walk"
  - term: "spend shape"
    status: draft             # draft terms are exempt from strict findings
    sense: "how a review round's agent budget is declared"
  - term: "contract"
    status: reserved          # reserved: use outside the sense flags E0092
    sense: "a versioned behavioral commitment, never a soft promise"
coinage_globs:                # where **bold** introduces a term (E0090);
  - "docs/spec/**/*.md"       # absent = wherever the register scans
label_schemes:
  allow:
    - "^C\\d+$"               # local scheme: C1, C2, ...
anti_patterns:
  - pattern: "very unique"
    guidance: "say 'unique' — uniqueness does not grade"
```

What it flags: unregistered
bold-introduced coinages (`E0090`, draft-status exempt — inside `coinage_globs`
when the registry sets them, since bold-means-coinage rarely holds across a
whole corpus of `**Label:**` lead-ins and emphasis), letter-plus-number
label clusters outside your allowlist (`E0091` — structured reference-IDs
`REQ-<n>`/`AC-<n>`/`ADR-<n>`/`RFC-<n>`/`Q<n>` are exempt by default and unioned
with your allowlist, so specs validate out of the box; add local schemes like
`^C\d+$` to `label_schemes.allow`), reserved-word use (`E0092`),
listed register anti-patterns (`E0093`), and numeric claims — a prose numeral
restating a configured frontmatter field's count and drifting from it
(`E0094`). By default the scan covers every walked file; set `vocabulary_globs`
in `config.yaml` (a scope list beside `require_frontmatter`) to restrict it —
e.g. to apply the register to your live specs while leaving a historical archive
walked and routed but unscanned. A `vocabulary_globs` or `coinage_globs` that
matches nothing is loud (`W0043`), so a mistyped glob can't silently disable a
check. A term
declared both `registered` and `draft` resolves to draft with a warning
(`W0044`), so a conflicting declaration is surfaced, not silently resolved.

**Citations** — data-less; opt a route in with `citations: true` and its
files' `path:line` / `path:start-end` references are verified against the
working-tree snapshot (uncommitted content counts; no git subprocess): a dead citation
blocks (`E0100`), one past the target's end blocks (`E0101`). Historical
corpora simply don't opt in.

**Links** — data-less; opt a route in with `links: true` and its files' inline
markdown links are resolved against the working-tree snapshot via a CommonMark parse
(`pulldown-cmark`), so **inline** `[text](target)`, **reference-style**
`[text][ref]`, and **image** `![alt](src)` links are all checked, while a link
inside an inline `` `code` `` span or a fenced block is a syntax example and
skipped. A link to a relative path that isn't there blocks (`E0110`); an existing
markdown target — or the same document — whose `#fragment` matches no heading
blocks (`E0111`, fragments resolved with GitHub's heading-slug rules, covering
ATX + setext headings, duplicate-heading `-N` suffixes, and explicit HTML
anchors). Targets resolve **document-relative** (as GitHub renders them):
`[api](api.md)` from `docs/guide.md` is `docs/api.md`, and a one-level
`../README.md` that stays in-tree is fine — only a target that escapes the
governed tree is refused (`E0010`/`E0011`/`E0012`). Destinations are
percent-decoded first (`my%20doc.md` → `my doc.md`), and `link_root: true`
opts a route into resolving a leading-slash `/docs/x.md` from the project root
(still confined) for static-site corpora that author links that way. External
links (any URL scheme) are left alone.

**Markers** — data-less; give a route one or more `marker_rules` and each body
line matching a rule's `pattern` names a reference whose captured `<name>` must
resolve to an existing element in a rule-named target doc — the name-anchor
sibling of citations. A rule is `{ pattern, element, target_doc, target_section? }`:

```yaml
routes:
- files: "plan/**/*.md"
  governed_by: contract.md
  marker_rules:
    - pattern: "^Provenance: (.+)$"       # first capture = the referenced name
      element: list-item-bold-name        # or: heading
      target_doc: contract.md
      target_section: "## Decomposition"  # optional: scope to this heading's span
```

`element: list-item-bold-name` resolves the name against the leading `**bold**`
of a `- ` list item (`- **Slice 1 — …the guardrail.**` ← `Provenance: Slice 1 —
…the guardrail`); `heading` resolves against heading text. Resolution is
name-equality, a trailing `.` on the target tolerated (not slug-based). A
reference that resolves to nothing blocks (`E0112`) — as does a matched line
whose capture group captured nothing (an optional group that didn't
participate); the `target_doc` is project-root-relative and confined
(`E0010`/`E0011`/`E0012`). A `target_section` whose heading is absent from the
target doc blocks once per governed file (`E0114`, marker-target-section-not-
found) and the rule's lines are skipped there — a renamed target heading never
mass-flags healthy references. Two misconfigs are refused at load: a `pattern`
with no capture group (nothing to resolve), and a `target_section` that is not
a full ATX heading line with non-empty text.

**Code catalogs** (`code-catalogs.yaml`) — the adopter-side twin of mdatron's
own every-code-resolves-in-explain: declare your code namespace and every code
token cited in the corpus must resolve to it.

```yaml
mdatron_format_version: 1     # required on this file (born in 0.6.0)
catalogs:
  - namespace: "ADOPTER-"     # the ownership prefix
    comprehensive: true       # this catalog is the sole authority for the prefix
    codes: ["E0010", "W0180"] # the declared legal set (class letter + digits)
```

Every adopter input file carries `mdatron_format_version` — the **input**
contract's own version axis (independent of the DSL's `mdatron_dsl_version`
and the JSON `mdatron_output_version`), so a future format change breaks
legibly instead of mis-parsing silently. It is **required** on files born in
0.6.0 (this one) and **optional** on `routes.yaml`/`vocabulary.yaml`/
`pins.yaml` (absent = the v1 legacy baseline; a 0.5.0-authored file still
parses); `pin --update` stamps it going forward.

Under a `comprehensive` catalog, a cited token that isn't declared blocks
(`E0113`, orphaned-adopter-code) — the fix for codes left dangling after a
defining doc is sunset. The detector is intentionally broader than the legal
grammar (prefix + a digit-bearing tail), so a mistyped or unknown class is
caught rather than silently skipped; the `codes:` list, not the detector, is
the closed legal set. Set `comprehensive: false` if codes for the prefix may
legitimately live outside the catalog.

**Section rules** (a `section_rules:` block on a route in `routes.yaml`, the
sibling of `marker_rules`) — declarative **count** and **disjointness**
assertions over markdown body sections (the body-content counterpart of the
DSL's frontmatter arity rules), **scoped by the route's `files` glob** so a
file-specific structural invariant lives on the route that claims those files
and can't misfire corpus-wide:

```yaml
routes:
  - files: "plan/**/*.md"
    governed_by: ROADMAP.md
    section_rules:
      # at least one open-phase H3 in ## Requirements
      - section: "## Requirements"
        element: h3
        match: '^### Phase \d+: .*\((parallel|sequential)\)$'
        count: ">= 1"
      # a slice is open XOR complete — ids extracted per element, never a full-span scan
      - disjoint:
          - section: "## Requirements"
            element: h3                  # ids from the H3 heading text
            id_pattern: 'Slice (\d+)'
          - section: "## Completed phases"
            element: list-item-bold-name # ids from the `- **bold**` lead
            id_pattern: 'Slice (\d+)'
```

A count rule counts the elements of the `element` class in the `section`'s span
(until the next heading of the same or higher level) whose line matches `match`,
and asserts the `count` predicate (`>= 1`, `== 1`, …); a violation is `E0120`.
`element` is the one vocabulary marker rules use too: `heading` (a heading of
any level), `h1`…`h6` (one level), or `list-item-bold-name` (the `**bold**` lead
of a `- ` list item). A `disjoint` rule extracts an id (the `id_pattern`'s first
capture) from each section's declared element and asserts the two sets share
none; an overlap is `E0121`. Ids come **only** from the declared element (an `h3`
heading's text, or a `list-item-bold-name` bullet's bold name), never
surrounding prose — so a body mention of an id doesn't cause a false overlap. A `section` spec that matches no heading in the
document blocks (`E0122`, section-not-found; matching is exact on level and
text) instead of silently passing, and the assertion is not evaluated; when a
heading occurs more than once, the rule evaluates over **all** matching spans
(counts sum, ids union), so content under a duplicate heading can't evade the
gate. A spec that is not a full ATX heading line with non-empty text is
refused at load.

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

## Where to go next

- [`DESIGN.md`](./DESIGN.md) — the standing design: behavioral contracts,
  the nine check families, output marking discipline, path confinement,
  governance-data governance
- [`docs/dsl-reference.md`](./docs/dsl-reference.md) — the complete rule-DSL
  construct inventory with evaluation semantics; held to the implementation
  continuously by CI tripwires (the construct-inventory check and the
  operator-semantics pins), so an engine construct absent from the reference —
  or reference semantics the engine does not implement — fails the build
- [`docs/faq.md`](./docs/faq.md) — prior-art comparisons, influences, and
  frequently asked questions
- [`docs/limits.md`](./docs/limits.md) — every declared input and enumeration
  bound (file sizes, nesting depths, walk budgets, the concurrent-invocation
  count), shipped as data and held to the implementation by a test
- `mdatron explain <code>` — per-code prose for every emitted diagnostic
  (frontmatter, confinement, schema, init, jurisdiction, route, pin,
  vocabulary, and citation codes); the catalog grows by one entry per
  newly-emitted code

## License

See [`LICENSE`](./LICENSE).
