# Recipe: AGENTS.md

`AGENTS.md` is the file coding agents read before they touch a repository,
and agents act on it. Do they run the build and test commands it lists?
"Yes—if you list them. The agent will attempt to execute relevant
programmatic checks and fix failures before finishing the task." The format
has no rules to break ("just standard Markdown. Use any headings you like"),
so the failures are all quiet:

- An `AGENTS.override.md` that someone committed. Codex reads it *instead
  of* `AGENTS.md` in the same directory, so the file everyone reviewed
  stops being read.
- A one-line change to the build commands. It changes what runs on every
  agent's machine, and in review it looks like any other edit to prose.
- A section that was dropped, or a link to a doc that was renamed. The agent
  works without what was there.
- An empty file, which Codex skips, or one grown past the 32 KiB at which
  Codex stops reading.
- A `CLAUDE.md` beside or above it. Where one exists, Claude Code reads it
  and not `AGENTS.md`, so Claude follows different instructions from every
  other agent. The remedy is a `CLAUDE.md` next to each `AGENTS.md` that
  imports it.

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
| Codex AGENTS.md discovery (source) | https://github.com/openai/codex/blob/bb72e151e50cfd4f6282ab86418188af3c83b5df/codex-rs/core/src/agents_md.rs | openai/codex@bb72e151e50cfd4f6282ab86418188af3c83b5df codex-rs/core/src/agents_md.rs | 2026-06-18 | 2026-10-02 |
| Claude Code project instructions (CLAUDE.md and AGENTS.md) | https://code.claude.com/docs/en/memory | unversioned documentation page; version gates stated through v2.1.283 | unversioned | 2026-10-02 |
<!-- cookbook-provenance-end -->

To check for drift, compare the pinned commit with the latest one under
`components/`
(`gh api 'repos/agentsmd/agents.md/commits?path=components&per_page=1' --jq '.[0].sha'`),
do the same for `codex-rs/core/src/agents_md.rs` in openai/codex, and fetch
the Codex and Claude Code documentation pages again: neither has a
published source to pin.

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
  the limit", `project_doc_max_bytes`, "32 KiB by default". The source shows
  what "stops" means: a file that does not fit in what remains is truncated
  to it, with a log line and nothing shown to the user.
- **Claude Code.** From v2.1.277, by default "Claude reads `AGENTS.md` only
  when you have no `CLAUDE.md` in your working directory or above it". With
  both files present it reads "Your `CLAUDE.md` files only", and a `CLAUDE.md`
  in `.claude/`, a `CLAUDE.local.md`, or one in a directory above counts
  too. The documented remedy is "putting an `@AGENTS.md` import in a
  `CLAUDE.md` next to it". A subdirectory's `CLAUDE.md` is "included when
  Claude reads files in those subdirectories". "Import parsing skips
  Markdown code spans and fenced code blocks." Two things here are this
  page's reading, not quotations: that a root import does not bring in a
  package's `AGENTS.md`, and that an import in a package's `CLAUDE.md` is
  expanded when that file loads.

## The configuration

<!-- cookbook-file: .mdatron/config.yaml -->
```yaml
# The walked set: every AGENTS*.md file, at any depth. Walking the prefix, not
# only AGENTS.md, is what makes a committed AGENTS.override.md visible: it is
# walked, no route claims it, and mdatron reports it. CLAUDE*.md is walked
# the same way, for CLAUDE.md and CLAUDE.local.md: wherever one sits, Claude
# Code reads it instead of the AGENTS.md beside or below it. (One glob, not
# one per name: a glob that matches no file is itself a warning.) Any other
# file whose name starts with AGENTS or CLAUDE is walked too and needs a
# route.
file_globs:
  - "**/AGENTS*.md"
  - "**/CLAUDE*.md"
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
  # Codex stops reading at 32 KiB across the files on one path. With one
  # AGENTS.md per package, a path is the root file plus one package's:
  # 24 KiB here and 8 KiB per package file keeps that inside the budget.
  # (A route's `*` crosses `/`: a second AGENTS.md deeper in a package gets
  # its own 8 KiB, and the split no longer holds.)
  max_bytes: 24576
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
# A package's AGENTS.md: the closest file to an edited path wins. No route
# claims AGENTS.override.md, so a committed one is unrouted (E0030).
- files: "packages/*/AGENTS.md"
  governed_by: CONTRIBUTING.md
  links: true
  max_bytes: 8192
  section_rules:
  # No section: the whole document body. Codex skips an empty file without a
  # word.
  - element: line
    match: "."
    count: ">= 1"
# A CLAUDE.md switches AGENTS.md off for Claude Code unless it imports it, so
# each routed one holds the import. No route claims .claude/CLAUDE.md, a
# CLAUDE.local.md, or a CLAUDE.md outside packages/: a committed one is
# unrouted (E0030). A route's `*` crosses `/`, so the packages route also
# claims a CLAUDE.md deeper inside a package.
- files: "CLAUDE.md"
  governed_by: CONTRIBUTING.md
  section_rules:
  - element: line
    match: "(^|\\s)@(\\./)?AGENTS\\.md(\\s|$)"
    count: ">= 1"
- files: "packages/*/CLAUDE.md"
  governed_by: CONTRIBUTING.md
  section_rules:
  - element: line
    match: "(^|\\s)@(\\./)?AGENTS\\.md(\\s|$)"
    count: ">= 1"
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
| Codex skips an empty file | root: the H1 anchor and the section pin; packages: a whole-document count of `line` elements, `>= 1` (`E0120`) |
| Codex stops reading at 32 KiB combined | route `max_bytes`, split so the root file plus one package file fits (`E0036`) |
| Claude Code reads a `CLAUDE.md` in place of `AGENTS.md` | each routed `CLAUDE.md` holds a line with the `@AGENTS.md` import (a whole-document count, `E0120`); walking `**/CLAUDE*.md` makes one the routes do not claim unrouted (`E0030`) |

## What this does not check

- **A `CLAUDE.md` beside every `AGENTS.md`.** The recipe checks each
  `CLAUDE.md` it finds. It cannot require one to exist: with a root
  `CLAUDE.md` present, a new package's `AGENTS.md` is invisible to Claude
  Code until someone adds a `CLAUDE.md` next to it. Nor can it check that
  the `AGENTS.md` an import names exists beside the importing file. mdatron
  has no rule that one file must have a sibling (mdatron #221).
- **Stray files inside a package.** A route's `*` crosses `/`, so
  `packages/*/CLAUDE.md` also claims `packages/api/sub/CLAUDE.md` and
  `packages/api/.claude/CLAUDE.md`. Such a file is held to the import rule
  instead of being reported as unrouted, and an `@AGENTS.md` in it passes
  though it points at no file and still hides the package's `AGENTS.md`
  from Claude Code. Only strays outside `packages/` are caught. mdatron has
  no route glob whose `*` stops at a directory (mdatron #220).
- **Whether the import is live.** The rule looks for `@AGENTS.md` or
  `@./AGENTS.md`, with whitespace or a line end on both sides, on a line
  outside fenced code. A tight code span (`` `@AGENTS.md` ``) is rightly not
  counted, but the same text inside an HTML comment, an indented code block,
  a fence nested in a list item (indented four spaces or more) or a code
  span padded with spaces is counted though nothing is imported.
  An import followed directly by punctuation (`@AGENTS.md.`) is not counted.
- **What "empty" means.** The package rule counts non-blank lines in the
  body: frontmatter and fenced code blocks are not lines. A file holding
  only a fenced block of commands is reported though Codex would read it,
  and a file holding only an invisible character passes.
- **Deeper files and the budget.** A route's `*` crosses `/`, so
  `packages/*/AGENTS.md` also claims `packages/api/sub/AGENTS.md`, with its
  own 8 KiB. The split is sound only while each package has one `AGENTS.md`
  (mdatron #220).
  `max_bytes` bounds each file; mdatron does not add up the files along a
  path. The count is the file's bytes as checked out, so keep bounded files
  on LF line endings (`eol=lf` in `.gitattributes`).
- **A symlinked `CLAUDE.md`.** `ln -s AGENTS.md CLAUDE.md` is a setup the
  Claude Code documentation offers. mdatron refuses symlinks in the governed
  tree (`E0012`), so this recipe needs the import form.
- **Local files.** A committed `CLAUDE.local.md` is walked and reported as
  unrouted. A developer's uncommitted one also stops Claude Code reading
  `AGENTS.md`, and mdatron sees only the tree it is run on.

Also out of scope: Codex's `project_doc_fallback_filenames` (set in Codex's
`config.toml`, which this recipe does not read), the global
`~/.codex/AGENTS.md`, and Claude Code's **Project instructions** setting.

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
style", and the section the project requires is missing. The finding quotes
the rule's `match`, so it says which required heading it is.

<!-- cookbook-case: no-security -->
```text
error[MDATRON-E0120]: section-count-violation
  --> AGENTS.md:1
   = note: the named section has 0 matching h2 element(s) across its matching span(s); the rule requires the count == 1
   = section:
           > # Acme
   = match:
           > ^Security$
   = match_on:
           > name
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

**An empty root `AGENTS.md`.** Codex skips it without a word. Every rule that
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
   = match:
           > ^Build and test$
   = match_on:
           > name
   = explain: mdatron explain MDATRON-E0122
error[MDATRON-E0122]: section-not-found
  --> AGENTS.md:1
   = note: no heading in this document matches the section rule's section spec (matching is exact on level and text), so its count assertion cannot be evaluated
   = section:
           > # Acme
   = match:
           > ^Security$
   = match_on:
           > name
   = explain: mdatron explain MDATRON-E0122
mdatron verify: 3 error(s), 0 warning(s) across 3 finding(s)
```

**An empty package `AGENTS.md`.** It has no fixed heading to anchor on, so
the rule counts lines over the whole document.

<!-- cookbook-case: empty-nested -->
```text
error[MDATRON-E0120]: section-count-violation
  --> packages/api/AGENTS.md:1
   = note: the document has 0 matching line element(s); the rule requires the count >= 1
   = match:
           > .
   = explain: mdatron explain MDATRON-E0120
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**A root file past its share of the budget.** Five hundred style reminders
were appended. At 27,134 bytes the root file leaves 5,634 of Codex's 32,768
for a package's file, which Codex would truncate past that point.

<!-- cookbook-case: over-budget -->
```text
error[MDATRON-E0036]: file-over-byte-budget
  --> AGENTS.md:1
   = note: this file is 27134 bytes; its route allows at most 24576 (max_bytes)
   = help: shorten the file or move detail into a document it links to; raise the route's max_bytes only if the consumer's budget really is larger
   = explain: mdatron explain MDATRON-E0036
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**A `CLAUDE.md` that mentions `AGENTS.md` in words.** The import was replaced
by a sentence. Claude Code now reads `CLAUDE.md` alone: it "sees `AGENTS.md`
only if it decides to open the file".

<!-- cookbook-case: claude-without-import -->
```text
error[MDATRON-E0120]: section-count-violation
  --> CLAUDE.md:1
   = note: the document has 0 matching line element(s); the rule requires the count >= 1
   = match:
           > (^|\s)@(\./)?AGENTS\.md(\s|$)
   = explain: mdatron explain MDATRON-E0120
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**A `.claude/CLAUDE.md` appears.** It counts as a `CLAUDE.md` above every
file in the repository, and it imports nothing. No route allows it, so it is
reported the way a stray override is.

<!-- cookbook-case: stray-claude -->
```text
error[MDATRON-E0030]: unrouted-file
  --> .claude/CLAUDE.md:1
   = note: this file is inside the walked jurisdiction but no route claims it; the route table is a closed-world allowlist
   = help: add a route whose files glob claims it, or narrow file_globs if it should not be walked at all
   = explain: mdatron explain MDATRON-E0030
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

## Make it yours

- **Your project's name and sections.** Change `# Acme` to your H1, and add
  one count rule per heading your team requires.
- **Allow an override on purpose.** If you do use `AGENTS.override.md`, give
  it a route of its own so it is checked, rather than leaving it unrouted.
- **Pin what agents execute.** Pin each section that lists commands,
  including a package's own `## Build and test`.
- **No `CLAUDE.md`.** If your repository has none, drop the two `CLAUDE.md`
  routes and keep the `**/CLAUDE*.md` glob, so that a `CLAUDE.md` added later
  is reported as unrouted instead of silently taking over. Until one exists
  the glob matches nothing and every run prints warning `W0046`, which fails
  a run under `--deny-warnings`; drop the glob too if you cannot have that.
  Claude Code then reads `AGENTS.md` directly, from v2.1.277, with the
  exceptions its documentation lists (some sessions before v2.1.281, a
  disabled `agents-md` plugin, the first session after an upgrade).
- **Other files named `AGENTS…` or `CLAUDE…`.** Both globs walk a prefix, so
  a file such as `docs/AGENTS-guide.md` or `docs/CLAUDE_CODE_SETUP.md` is
  walked and reported as unrouted. Give it a route, or narrow the glob to
  the names you use (a glob per exact name works once each name exists; a
  glob that matches no file is warning `W0046`).
- **Codex fallback names.** If your team sets `project_doc_fallback_filenames`,
  add those names to `file_globs` and route them like `AGENTS.md`.
