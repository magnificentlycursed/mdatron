# Recipe: AGENTS.md

`AGENTS.md` is the file coding agents read before they touch a repository,
and agents act on it: when it lists build and test commands, "the agent will
attempt to execute relevant programmatic checks and fix failures before
finishing the task." The format has no rules to break ("just standard
Markdown. Use any headings you like"), so the failures are all quiet:

- An `AGENTS.override.md` that someone committed. Codex reads it *instead
  of* `AGENTS.md` in the same directory, so the file everyone reviewed
  stops being read.
- A one-line change to the build commands. It changes what runs on every
  agent's machine, and in review it looks like any other edit to prose.
- A section that was dropped, or a link to a doc that was renamed. The agent
  works without what was there.
- An empty file. Codex skips empty files.

This recipe uses the route, section, link and pin families. The complete,
runnable project is
[`examples/standards/agents-md/`](../../examples/standards/agents-md/AGENTS.md);
every configuration file and diagnostic below is compared against it by the
test suite.

## Evaluated against

With no schema to carry it, the example keeps its provenance in
`x-source.json`, and this table must match that file (the test suite
enforces it).

<!-- cookbook-provenance-start -->
| Standard | Source | Revision | Revision date | Retrieved |
|---|---|---|---|---|
| AGENTS.md | https://agents.md | agentsmd/agents.md@d1ac7f063d20e70015ed6732664049ae4ba9d74e components/ | 2026-03-12 | 2026-10-01 |
| Codex AGENTS.md discovery | https://learn.chatgpt.com/docs/agent-configuration/agents-md | unversioned documentation page; openai/codex@bb72e151e50cfd4f6282ab86418188af3c83b5df docs/agents_md.md links to it | unversioned | 2026-10-01 |
<!-- cookbook-provenance-end -->

To check for drift, compare the pinned commit with the latest one under
`components/`
(`gh api 'repos/agentsmd/agents.md/commits?path=components&per_page=1' --jq '.[0].sha'`),
and fetch the Codex page again: it has no published source to pin.

## What the sources say

- **Format.** "AGENTS.md is just standard Markdown. Use any headings you
  like; the agent simply parses the text you provide." It has no required
  fields or sections; every section rule below is project policy.
- **Precedence.** "The closest AGENTS.md to the edited file wins; explicit
  user chat prompts override everything." A monorepo keeps one per package.
- **Codex discovery.** From the project root down to the working directory,
  in each directory Codex "checks for `AGENTS.override.md`, then
  `AGENTS.md`", and concatenates what it finds "from the root down". It
  "skips empty files and stops adding files once the combined size reaches
  the limit", `project_doc_max_bytes`, "32 KiB by default".

## The configuration

<!-- cookbook-file: .mdatron/config.yaml -->
```yaml
# The walked set: every AGENTS*.md file, at any depth. Walking the prefix, not
# only AGENTS.md, is what makes a committed AGENTS.override.md visible: it is
# walked, no route claims it, and mdatron reports it.
file_globs:
  - "**/AGENTS*.md"
```

<!-- cookbook-file: .mdatron/routes.yaml -->
```yaml
mdatron_format_version: 1
routes:
# The root file, the one every agent reads. Relative links and #anchors must
# resolve.
- files: "AGENTS.md"
  governed_by: CONTRIBUTING.md
  links: true
  section_rules:
  # Project policy: the format itself is "just standard Markdown" with no
  # required sections. Anchoring on the H1 also reports a missing or renamed
  # H1 (E0122), which is what an empty file has.
  - section: "# Acme"
    element: h2
    match: "^Build and test$"
    match_on: name
    count: "== 1"
  - section: "# Acme"
    element: h2
    match: "^Security$"
    match_on: name
    count: "== 1"
# One AGENTS.md per package: the closest file to an edited path wins. No route
# claims AGENTS.override.md, so a committed one is unrouted (E0030).
- files: "packages/*/AGENTS.md"
  governed_by: CONTRIBUTING.md
  links: true
```

The pin record is written by `mdatron pin --update`, which computes the hash
and rewrites the file in this form (comments you add do not survive it). The
example's `CONTRIBUTING.md` says why the section is pinned.

<!-- cookbook-file: .mdatron/pins.yaml -->
```yaml
# mdatron pin record — governing documents pin sha256 over governed files (#84).
# Recompute with `mdatron pin --update`. This file cannot pin itself; its
# integrity anchor is commit review (DESIGN § Validation is data-driven, governance data is governed).
mdatron_format_version: 1
pins:
- governed_by: CONTRIBUTING.md
  file: AGENTS.md
  section: '## Build and test'
  sha256: d7a675fb4aed195a1b30289728b8de6a85d24c10b560168125aa2fbde9dcf589
```

The example's `.mdatron/schemas/` holds only a `.gitkeep`: mdatron requires a
schemas or patterns directory to exist, and this recipe needs no schema.

## What each rule becomes

| Source rule or policy | mdatron construct |
|---|---|
| Codex reads `AGENTS.override.md` in place of `AGENTS.md` | walk `**/AGENTS*.md`, route only `AGENTS.md` files: an override is unrouted (`E0030`) |
| Agents execute the commands the file lists | section pin on `## Build and test` (`E0061` when it changes, `E0063` when the heading is gone) |
| Required sections (policy; the format has none) | count rules on the H1's span, one per heading, `== 1` (`E0120`; `E0122` when the H1 is missing) |
| The file points agents at other docs | route `links: true` on every `AGENTS.md` (`E0110`, `E0111`) |
| Codex skips an empty file | the H1 anchor and the section pin both report an empty root file |

## What this does not check

These are gaps in mdatron, not in the sources, and each is tracked as an
engine issue:

- **The 32 KiB budget.** Codex stops reading once the files on the path
  total `project_doc_max_bytes`; instructions past it are dropped. mdatron
  has no byte bound per route or across a set of files (mdatron #216).
- **An empty nested file.** The root file is covered by its H1 anchor; a
  package's `AGENTS.md` has no fixed H1 to anchor on, and mdatron has no
  "must not be empty" rule (mdatron #217).
- **`CLAUDE.md` drifting from `AGENTS.md`.** Claude Code reads `CLAUDE.md`,
  not `AGENTS.md`. A `CLAUDE.md` that does not import `@AGENTS.md` can give
  Claude different instructions from every other agent, and mdatron cannot
  assert that one file references another (mdatron #218).
- **Which section rule failed.** Both count rules anchor on `# Acme`, and an
  `E0120` names the section and the element, not the rule's `match`, so the
  finding does not say whether `Build and test` or `Security` is the one
  missing (mdatron #219).

Also out of scope: Codex's `project_doc_fallback_filenames` (configured per
user in `config.toml`, not in the repository) and the global
`~/.codex/AGENTS.md`.

## The silent failures, and what mdatron says

Each case is the example project with one change. The output is mdatron's
real output, compared with the example by the test suite.

**A committed `AGENTS.override.md`.** Codex now reads this file at the root
instead of the reviewed `AGENTS.md`.

<!-- cookbook-case: stray-override -->
```text
error[MDATRON-E0030]: unrouted-file
  --> AGENTS.override.md:1
   = note: this file is inside the walked jurisdiction but no route claims it; the route table is a closed-world allowlist
   = help: add a route whose files glob claims it, or narrow file_globs if it should not be walked at all
   = explain: mdatron explain MDATRON-E0030
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**`cargo test --locked || true`.** The test step can no longer fail, and every
agent that follows the file now finishes with failing tests. The pin turns a
one-line edit into a review: a maintainer reads the new commands, then
re-pins.

<!-- cookbook-case: build-drift -->
```text
error[MDATRON-E0061]: pin-stale
  --> .mdatron/pins.yaml:1
   = note: the governed file changed after its pin was recorded; re-read the governing document, then re-pin with `mdatron pin --update`
   = file:
           > AGENTS.md
   = governed_by:
           > CONTRIBUTING.md
   = recorded:
           > d7a675fb4aed
   = found:
           > 61ac5e8097bd
   = help: the stale pin is the attention loop working: the governing relationship must be reviewed, not just the hash refreshed
   = explain: mdatron explain MDATRON-E0061
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**The Security heading is gone.** Its bullets now read as part of "Code
style", and the section the project requires is missing.

<!-- cookbook-case: no-security -->
```text
error[MDATRON-E0120]: section-count-violation
  --> AGENTS.md:1
   = note: the named section has 0 matching h2 element(s) across its matching span(s); the rule requires the count == 1
   = section:
           > # Acme
   = explain: mdatron explain MDATRON-E0120
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**A renamed design doc.** Both files that pointed agents at it now point at
nothing.

<!-- cookbook-case: dead-link -->
```text
error[MDATRON-E0110]: dead-link-target
  --> AGENTS.md:13
   = note: this link's relative target is missing or could not be opened in the working-tree snapshot (uncommitted content counts; no git history is consulted)
   = link:
           > docs/architecture.md#module-boundaries
   = os error:
           > No such file or directory (os error 2)
   = explain: mdatron explain MDATRON-E0110
error[MDATRON-E0110]: dead-link-target
  --> packages/api/AGENTS.md:13
   = note: this link's relative target is missing or could not be opened in the working-tree snapshot (uncommitted content counts; no git history is consulted)
   = link:
           > ../../docs/architecture.md#the-api
   = os error:
           > No such file or directory (os error 2)
   = explain: mdatron explain MDATRON-E0110
mdatron verify: 2 error(s), 0 warning(s) across 2 finding(s)
```

**An empty `AGENTS.md`.** Codex skips it without a word. Every rule that
needed its content reports instead.

<!-- cookbook-case: empty-file -->
```text
error[MDATRON-E0063]: pin-section-not-found
  --> .mdatron/pins.yaml:1
   = note: a section pin names a heading that is not present in its target file (a section pinned over nothing cannot be verified); the heading is matched by level and text, and a `#` inside a code fence is not a heading
   = file:
           > AGENTS.md
   = section:
           > ## Build and test
   = help: correct the pin's `section` heading to match one in the file, or remove `section` to pin the whole file
   = explain: mdatron explain MDATRON-E0063
error[MDATRON-E0122]: section-not-found
  --> AGENTS.md:1
   = note: no heading in this document matches the section rule's section spec (matching is exact on level and text), so its count assertion cannot be evaluated
   = section:
           > # Acme
   = explain: mdatron explain MDATRON-E0122
error[MDATRON-E0122]: section-not-found
  --> AGENTS.md:1
   = note: no heading in this document matches the section rule's section spec (matching is exact on level and text), so its count assertion cannot be evaluated
   = section:
           > # Acme
   = explain: mdatron explain MDATRON-E0122
mdatron verify: 3 error(s), 0 warning(s) across 3 finding(s)
```

## Make it yours

- **Your project's name and sections.** Change `# Acme` to your H1, and add
  one count rule per heading your team requires.
- **Allow an override on purpose.** If you do use `AGENTS.override.md`, give
  it a route of its own so it is checked, rather than leaving it unrouted.
- **Pin what agents execute.** Pin each section that lists commands,
  including a package's own `## Build and test`.
- **Codex fallback names.** If your team sets `project_doc_fallback_filenames`,
  add those names to `file_globs` and route them like `AGENTS.md`.
