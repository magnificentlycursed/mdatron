# Recipe: llms.txt

`llms.txt` tells agents which pages of a site or repository to read, as a
short Markdown file of links. When it rots, nothing notices: a renamed page
leaves a dead link the agent follows to nothing, an anchor that no longer
exists lands it at the top of the wrong page, and a file edited out of shape
stops being the structure the specification describes. Version 2 of the
specification names no validator at all (it dropped the `llms_txt2ctx`
tooling), so the only check is one you run yourself.

This recipe has no frontmatter to validate. It shows mdatron's other side:
route-attached section rules and the link family, applied to a Markdown file
whose name ends in `.txt`. The complete, runnable project is
[`examples/standards/llms-txt/`](../../examples/standards/llms-txt/AGENTS.md);
every configuration file and diagnostic below is compared against it by the
test suite.

## Evaluated against

With no schema to carry it, the example keeps its provenance in
`x-source.json`, and this table must match that file (the test suite
enforces it). The specification is pinned to a commit of its source.

<!-- cookbook-provenance-start -->
| Standard | Source | Revision | Revision date | Retrieved |
|---|---|---|---|---|
| llms.txt (v2) | https://llmstxt.org | AnswerDotAI/llms-txt@6e55a65a7a4b541779dd0585f508717a6ea89a9e nbs/index.qmd | 2026-08-31 | 2026-09-29 |
<!-- cookbook-provenance-end -->

To check for drift, compare the pinned commit with the file's latest one
(`gh api 'repos/AnswerDotAI/llms-txt/commits?path=nbs/index.qmd&per_page=1' --jq '.[0].sha'`)
and read `nbs/changes.qmd` for what changed.

## What the specification requires

In order: an optional byte-order mark; "An H1 with the name of the project or
site. This is the only required section"; a blockquote summary; zero or more
sections "of any type except headings"; and zero or more sections delimited
by H2 headers, each a "file list" whose items contain "a required markdown
hyperlink `[name](url)`, then optionally a `:` and notes about the file". An
H2 named "Optional" is, in v2, only a convention for secondary links.

## The configuration

<!-- cookbook-file: .mdatron/config.yaml -->
```yaml
# The walked set: the llms.txt file (Markdown content, .txt name). The docs it
# links to are link targets, captured when checked, not walked themselves.
file_globs:
  - "llms.txt"
```

<!-- cookbook-file: .mdatron/routes.yaml -->
```yaml
mdatron_format_version: 1
routes:
- files: "llms.txt"
  governed_by: AGENTS.md
  naming: "^llms\\.txt$"
  # Relative links and #anchors must resolve (absolute https:// URLs are
  # outside the tree and not checked).
  links: true
  section_rules:
  # One H1 in the whole file: the rules anchored on "# Acme" cover that H1's
  # span, which a second H1 would end.
  - element: h1
    match: "."
    count: "== 1"
  # The H1 with the project's name is the one required section. Anchoring on
  # it reports a missing or renamed H1 (E0122), and its span must hold at
  # least one H2 file list (E0120).
  - section: "# Acme"
    element: h2
    match: ".+"
    count: ">= 1"
  # In each file list, every item is "[name](url)", then optionally ": notes"
  # (E0123). One rule per list; `match_on: name` tests the item's text, so the
  # bullet marker and indentation do not matter.
  - section: "## Docs"
    element: list-item
    every: "^\\[([^\\[\\]]|\\[[^\\[\\]]*\\])+\\]\\(\\S+\\)(:.*)?$"
    match_on: name
  - section: "## Optional"
    element: list-item
    every: "^\\[([^\\[\\]]|\\[[^\\[\\]]*\\])+\\]\\(\\S+\\)(:.*)?$"
    match_on: name
  # The blockquote summary comes before the H2 file lists (E0124).
  - section: "# Acme"
    order:
    - element: blockquote
      match: "."
    - element: h2
      match: "."
```

The example's `.mdatron/schemas/` holds only a `.gitkeep`: mdatron requires a
schemas or patterns directory to exist, and this recipe needs no schema.

## What each rule becomes

| Specification rule | mdatron construct |
|---|---|
| An H1 with the project's name is the one required section | section rule anchored on `# Acme` (`E0122` when missing or renamed) |
| File lists are sections delimited by H2 headers | a count of `h2` in that span, `>= 1` (`E0120`); requiring at least one list is policy, the specification allows zero |
| Each item is "a required markdown hyperlink `[name](url)`, then optionally a `:` and notes" | an `every` rule over the `list-item` elements of each named file list (`E0123`, one per malformed item) |
| The blockquote summary precedes the file lists | an `order` rule: `blockquote`, then `h2` (`E0124`) |
| One H1 (inferred: the specification describes a single H1 and never says "exactly one") | a whole-document count of `h1`, `== 1` (`E0120`): the rules anchored on the H1 cover its span, which a second H1 would end |
| Links should point to agent-friendly Markdown pages | route `links: true`: every relative link and `#anchor` must resolve (`E0110`, `E0111`) |
| The file is named `llms.txt` | route `naming` grammar (`W0041`) |

## What this does not check

- **Absolute URLs.** The link family resolves links inside the repository;
  `https://` links, which most published `llms.txt` files use, are not
  fetched or checked. This is a gap in mdatron, tracked as mdatron #215.
- **File lists the rules do not name.** An `every` rule names one section,
  so each file list needs its own rule: a new `## Guides` list is unchecked
  until you add one, and removing `## Optional`, which the specification
  allows, reports the rule's section as missing (`E0122`). Lists above the
  first H2 are free-form, as the specification has them, and are not checked.
- **What a line cannot show.** Section rules read one line at a time, and
  only ATX (`#`) headings. An item whose link is wrapped onto a second line
  is reported as malformed, and a wrapped item's continuation lines are not
  read. A setext heading (text underlined with `---`), an HTML `<ul>` or
  `<blockquote>`, and a blockquote indented four spaces or more are not seen
  as a heading, items or a quote.
- **Blockquotes inside a file list.** The order rule places every blockquote
  before the first H2, so a quoted note inside a list section is reported.
  That is this recipe's policy, stricter than the specification.
- **An empty file list.** `## Docs` with no items under it passes: the rules
  require the heading and check the items that exist.
- **A missing summary.** The order rule asserts sequence only, so a file with
  no blockquote passes. The specification lists the summary without calling
  it required.
- **Other content.** A heading below H2 in the free-form part (the
  specification allows sections "of any type except headings" there) and a
  paragraph inside a file list both pass.
- **The exact link syntax.** The item pattern asks for `[name](url)` with no
  whitespace in the URL, then nothing or a `:` and notes; the name may hold
  one level of brackets (`[Array[T] reference]`). A link with a title
  (`[name](url "title")`) is reported, and so is a task item (`- [x] [name](url)`);
  a name with an escaped or unpaired bracket (`[a\]b]`), or brackets nested
  two deep, is reported too. Text glued to the link with no space
  (`[a](x)y(z)`) passes.
- **Commented-out items.** A list item inside a multi-line HTML comment is
  still a list item to mdatron, and is reported if malformed.
- **Anything before the H1.** The rules anchored on the H1 cover its span;
  text above the H1 is outside it. A leading byte-order mark, which the
  specification allows, is ignored.

Also out of scope: the recommended `rel="alternate"`/`rel="describedby"` link
relations (HTTP and HTML, not the file), and "most specific file applies" when
several `llms.txt` files cover one path.

## The silent failures, and what mdatron says

Each case is the example project with one edit to `llms.txt`. The output is
mdatron's real output, compared with the example by the test suite.

**A renamed page.** The link still reads fine; the agent gets nothing.

<!-- cookbook-case: dead-link -->
```text
error[MDATRON-E0110]: dead-link-target
  --> llms.txt:10
   = note: this link's relative target is missing or could not be opened in the working-tree snapshot (uncommitted content counts; no git history is consulted)
   = link:
           > docs/getting-started.html
   = os error:
           > No such file or directory (os error 2)
   = explain: mdatron explain MDATRON-E0110
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**A stale anchor.** The page exists, but the heading the link points at was
renamed, so the agent lands at the top of the page.

<!-- cookbook-case: dead-anchor -->
```text
error[MDATRON-E0111]: dead-anchor
  --> llms.txt:11
   = note: this link's `#fragment` matches no heading in the target file (fragments resolve via the GitHub heading-slug algorithm)
   = link:
           > docs/configuration.md#config-file
   = explain: mdatron explain MDATRON-E0111
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**The H1 changed.** The one required section no longer names the project
the rules expect. Each rule anchored on it reports that it could not run.

<!-- cookbook-case: renamed-h1 -->
```text
error[MDATRON-E0122]: section-not-found
  --> llms.txt:1
   = note: no heading in this document matches the section rule's section spec (matching is exact on level and text), so its count assertion cannot be evaluated
   = section:
           > # Acme
   = match:
           > .+
   = explain: mdatron explain MDATRON-E0122
error[MDATRON-E0122]: section-not-found
  --> llms.txt:1
   = note: no heading in this document matches the section rule's section spec (matching is exact on level and text), so its order assertion cannot be evaluated
   = section:
           > # Acme
   = order:
           > blockquote matching ., then h2 matching .
   = explain: mdatron explain MDATRON-E0122
mdatron verify: 2 error(s), 0 warning(s) across 2 finding(s)
```

**No file lists left.** An edit removed every H2 section: the file still
parses, but it no longer points anywhere. The count rule reports it, and so
does each list's own rule.

<!-- cookbook-case: no-file-lists -->
```text
error[MDATRON-E0120]: section-count-violation
  --> llms.txt:1
   = note: the named section has 0 matching h2 element(s) across its matching span(s); the rule requires the count >= 1
   = section:
           > # Acme
   = match:
           > .+
   = explain: mdatron explain MDATRON-E0120
error[MDATRON-E0122]: section-not-found
  --> llms.txt:1
   = note: no heading in this document matches the section rule's section spec (matching is exact on level and text), so its every-element assertion cannot be evaluated
   = section:
           > ## Docs
   = every:
           > ^\[([^\[\]]|\[[^\[\]]*\])+\]\(\S+\)(:.*)?$
   = match_on:
           > name
   = element class:
           > list-item
   = explain: mdatron explain MDATRON-E0122
error[MDATRON-E0122]: section-not-found
  --> llms.txt:1
   = note: no heading in this document matches the section rule's section spec (matching is exact on level and text), so its every-element assertion cannot be evaluated
   = section:
           > ## Optional
   = every:
           > ^\[([^\[\]]|\[[^\[\]]*\])+\]\(\S+\)(:.*)?$
   = match_on:
           > name
   = element class:
           > list-item
   = explain: mdatron explain MDATRON-E0122
mdatron verify: 3 error(s), 0 warning(s) across 3 finding(s)
```

**An item that is not a link.** Someone wrote the path as text. A tool that
expands the file's links skips the item.

<!-- cookbook-case: item-not-a-link -->
```text
error[MDATRON-E0123]: section-element-mismatch
  --> llms.txt:15
   = note: this list-item element does not match the pattern the rule requires of every such element in its scope
   = section:
           > ## Optional
   = every:
           > ^\[([^\[\]]|\[[^\[\]]*\])+\]\(\S+\)(:.*)?$
   = match_on:
           > name
   = element:
           > - Changelog: docs/changelog.md
   = explain: mdatron explain MDATRON-E0123
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**The summary moved below the file lists.** The blockquote is still there,
but it now reads as part of the last H2 section. Each of its two lines is
reported.

<!-- cookbook-case: summary-after-lists -->
```text
error[MDATRON-E0124]: section-order-violation
  --> llms.txt:14
   = note: this element appears after an element the rule's order places later
   = section:
           > # Acme
   = element:
           > > Acme is a command-line tool for syncing project metadata between a
   = must precede:
           > h2 matching .
   = explain: mdatron explain MDATRON-E0124
error[MDATRON-E0124]: section-order-violation
  --> llms.txt:15
   = note: this element appears after an element the rule's order places later
   = section:
           > # Acme
   = element:
           > > repository and its issue tracker.
   = must precede:
           > h2 matching .
   = explain: mdatron explain MDATRON-E0124
mdatron verify: 2 error(s), 0 warning(s) across 2 finding(s)
```

**A second H1.** An appendix was added under its own H1. The rules anchored
on `# Acme` stop there, so a plain list below it is not checked (a list under
an H2 the rules name still would be); the count of H1s is what reports it.

<!-- cookbook-case: second-h1 -->
```text
error[MDATRON-E0120]: section-count-violation
  --> llms.txt:1
   = note: the document has 2 matching h1 element(s); the rule requires the count == 1
   = match:
           > .
   = explain: mdatron explain MDATRON-E0120
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

## Make it yours

- **Your project's name.** Change `# Acme` in the section rules to your H1.
- **Your file lists.** Keep one `every` rule per H2 list, named for your
  headings; drop the `## Optional` rule if you have no such section.
- **One `llms.txt` per subtree.** The specification lets a file at
  `/docs/llms.txt` cover the pages under it. Add a `file_globs` entry and a
  route per file, each anchored on its own H1.
- **Links that must be Markdown.** Pair `links: true` with a repository
  layout whose linked pages are `.md`, and every link an agent follows is
  checked.
