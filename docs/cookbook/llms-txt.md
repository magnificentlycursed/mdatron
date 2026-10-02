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
  # The H1 with the project's name is the one required section. Anchoring on
  # it reports a missing or renamed H1 (E0122); the span runs to the end of
  # the file, so this also requires at least one H2 file list (E0120).
  - section: "# Acme"
    element: h2
    match: ".+"
    count: ">= 1"
  # Every file-list item is "[name](url)", then optionally ": notes" (E0123).
  - section: "# Acme"
    element: list-item
    every: "^- \\[[^\\]]+\\]\\([^)]+\\)(: .+)?$"
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
| Each item is "a required markdown hyperlink `[name](url)`, then optionally a `:` and notes" | an `every` rule over `list-item` elements (`E0123`, one per malformed item) |
| The blockquote summary precedes the file lists | an `order` rule: `blockquote`, then `h2` (`E0124`) |
| Links should point to agent-friendly Markdown pages | route `links: true`: every relative link and `#anchor` must resolve (`E0110`, `E0111`) |
| The file is named `llms.txt` | route `naming` grammar (`W0041`) |

## What this does not check

- **Absolute URLs.** The link family resolves links inside the repository;
  `https://` links, which most published `llms.txt` files use, are not
  fetched or checked. This is a gap in mdatron, tracked as mdatron #215.
- **Items that span lines.** Section rules read elements a line at a time: a
  list item is the line that opens it, so an item whose link is wrapped onto
  a second line is reported as malformed.
- **Anything before the H1.** The rules cover the H1's span; text above the
  H1 is outside it.

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
the rules expect. Each of the three rules anchored on it reports that it
could not run.

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
   = note: no heading in this document matches the section rule's section spec (matching is exact on level and text), so its every-element assertion cannot be evaluated
   = section:
           > # Acme
   = every:
           > ^- \[[^\]]+\]\([^)]+\)(: .+)?$
   = explain: mdatron explain MDATRON-E0122
error[MDATRON-E0122]: section-not-found
  --> llms.txt:1
   = note: no heading in this document matches the section rule's section spec (matching is exact on level and text), so its order assertion cannot be evaluated
   = section:
           > # Acme
   = explain: mdatron explain MDATRON-E0122
mdatron verify: 3 error(s), 0 warning(s) across 3 finding(s)
```

**No file lists left.** An edit removed every H2 section: the file still
parses, but it no longer points anywhere.

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
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**An item that is not a link.** Someone wrote the path as text. A tool that
expands the file's links skips the item.

<!-- cookbook-case: item-not-a-link -->
```text
error[MDATRON-E0123]: section-element-mismatch
  --> llms.txt:15
   = note: this list-item element does not match the pattern the rule requires of every such element in its scope
   = section:
           > # Acme
   = every:
           > ^- \[[^\]]+\]\([^)]+\)(: .+)?$
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
           > .
   = explain: mdatron explain MDATRON-E0124
error[MDATRON-E0124]: section-order-violation
  --> llms.txt:15
   = note: this element appears after an element the rule's order places later
   = section:
           > # Acme
   = element:
           > > repository and its issue tracker.
   = must precede:
           > .
   = explain: mdatron explain MDATRON-E0124
mdatron verify: 2 error(s), 0 warning(s) across 2 finding(s)
```

## Make it yours

- **Your project's name.** Change `# Acme` in the section rule to your H1.
- **One `llms.txt` per subtree.** The specification lets a file at
  `/docs/llms.txt` cover the pages under it. Add a `file_globs` entry and a
  route per file, each anchored on its own H1.
- **Links that must be Markdown.** Pair `links: true` with a repository
  layout whose linked pages are `.md`, and every link an agent follows is
  checked.
