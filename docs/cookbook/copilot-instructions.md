# Recipe: GitHub Copilot `.instructions.md`

Path-specific Copilot instructions are Markdown files with a small frontmatter
block, and every way to get one wrong is quiet. A file whose name does not end
in `.instructions.md` is simply not one. A key value from before GitHub renamed
the coding agent to the cloud agent matches nothing. A misspelled `applyTo`
leaves the file with no pattern at all. Neither GitHub's documentation nor VS
Code's says what happens to an unknown or malformed key, and neither tool
reports it. The instructions you wrote are just not applied, and the change you
see is a model that behaves slightly differently, or not at all.

The complete, runnable project is
[`examples/standards/copilot-instructions/`](../../examples/standards/copilot-instructions/AGENTS.md);
every configuration file and diagnostic below is compared against it by the
test suite. The companion recipes are [SKILL.md](./skill-md.md) and
[Claude Code subagents](./subagents.md).

## Evaluated against

Two vendors document this format, and they do not agree; the schema records
both sources in its `x-source` block, and this table must match it (the test
suite enforces it). Both are pinned to a commit of their documentation source.

<!-- cookbook-provenance-start -->
| Standard | Source | Revision | Revision date | Retrieved |
|---|---|---|---|---|
| GitHub Copilot repository custom instructions (GitHub.com) | https://docs.github.com/en/copilot/how-tos/custom-instructions/adding-repository-custom-instructions-for-github-copilot | github/docs@a31ee09ca842e9177e5be8b053dee2f4e8b29f8f data/reusables/copilot/custom-instructions-path.md | 2026-04-07 | 2026-09-28 |
| VS Code custom instructions | https://code.visualstudio.com/docs/agent-customization/custom-instructions | microsoft/vscode-docs@4f4413d9a9d3f7e59284da7cdbb21bc3b9f65c48 docs/agent-customization/custom-instructions.md | 2026-09-23 | 2026-09-28 |
<!-- cookbook-provenance-end -->

To check for drift, compare each pinned commit with the file's latest one, for
example
`gh api 'repos/github/docs/commits?path=data/reusables/copilot/custom-instructions-path.md&per_page=1' --jq '.[0].sha'`,
and update the schema, its `x-source` block, and this table together.

## Where the sources disagree

- **Keys.** GitHub.com documents `applyTo` and `excludeAgent`; VS Code
  documents `applyTo`, `name` and `description`. A file shared by both can
  carry all four, so the profile allows all four and nothing else.
- **Is `applyTo` required?** GitHub.com's steps put it in every file and offer
  no other way for a path-specific file to apply. VS Code marks it optional: a
  file without it loads by task relevance (its `description`) or when attached
  by hand. The profile requires it, as stated project policy; drop it from
  `required` if your team relies on VS Code's other modes.
- **Several globs.** GitHub.com accepts "multiple patterns by separating them
  with commas" in one string; VS Code documents a single "Glob pattern".
- **Precedence.** GitHub.com orders personal, repository and organization
  instructions; VS Code says sources are additive and warns not to depend on
  an order. Neither changes what a single file must look like.

## The configuration

<!-- cookbook-file: .mdatron/config.yaml -->
```yaml
# The walked set: every Markdown file under .github/instructions. Walking all
# of them, not only *.instructions.md, is what lets a file with the wrong
# name be reported instead of silently never loading.
file_globs:
  - ".github/instructions/**/*.md"
```

<!-- cookbook-file: .mdatron/routes.yaml -->
```yaml
mdatron_format_version: 1
routes:
# Path-specific instructions. GitHub.com: "The file name must end with
# .instructions.md"; `*` crosses `/`, so subdirectories are covered.
- files: ".github/instructions/*.md"
  governed_by: AGENTS.md
  schema: copilot-instructions
  naming: "^[^/]+\\.instructions\\.md$"
```

The schema is `.mdatron/schemas/copilot-instructions.json` in the example
project (JSON Schema draft 2020-12).

## What each rule becomes

| Source rule | mdatron construct |
|---|---|
| "The file name must end with `.instructions.md`" (GitHub.com) | route `naming` grammar over every walked file in `.github/instructions/` (`W0041`) |
| Files in subdirectories of `.github/instructions` count | the route's `*` crosses directories |
| `excludeAgent` is `"code-review"` or `"cloud-agent"` | schema `enum` |
| `applyTo`, `excludeAgent`, `name`, `description` are the documented keys | `additionalProperties: false` (`E0050`) |
| A path-specific file applies through its `applyTo` | `applyTo` required (project policy, see above) |

## What this does not check

- Whether an `applyTo` glob is valid, or matches any file in the repository.
  Neither source defines the glob dialect beyond examples (braces, for
  instance, are not mentioned).
- `.github/copilot-instructions.md`, the repository-wide file: it has no
  frontmatter to check, only prose.
- Whether the tool you use reads the file at all: GitHub.com applies
  path-specific files "only" for the cloud agent and code review, and VS
  Code's Local agent needs `chat.includeApplyingInstructions` enabled.
- VS Code's other locations (`.claude/rules`, `~/.copilot/instructions`,
  profile storage), which use their own formats.

## The silent failures, and what mdatron says

Each case is the example project with one edit. The output is mdatron's real
output, compared with the example by the test suite.

**A file named `.instruction.md`.** It is not a path-specific instructions
file, so nothing loads it. The naming grammar reports it as a warning; run
`mdatron verify --deny-warnings` in CI to make warnings fail the build.

<!-- cookbook-case: wrong-extension -->
```text
warning[MDATRON-W0041]: name-underivable
  --> .github/instructions/python.instruction.md:1
   = note: this file's name is not derivable from its route's naming grammar
   = name:
           > python.instruction.md
   = naming grammar:
           > ^[^/]+\.instructions\.md$
   = help: rename the file to match the grammar, or amend the route's naming field
   = explain: mdatron explain MDATRON-W0041
mdatron verify: 0 error(s), 1 warning(s) across 1 finding(s)
```

**`excludeAgent: coding-agent`.** GitHub renamed the coding agent to the cloud
agent; the documented values are now `"code-review"` and `"cloud-agent"`, and
nothing flags a file that still says the old name.

<!-- cookbook-case: renamed-agent -->
```text
error[MDATRON-E0050]: frontmatter-schema-violation
  --> .github/instructions/frontend/react.instructions.md:3:15
   = note: value at /excludeAgent is not one of the schema's allowed options: ["code-review","cloud-agent"]
   = found:
           > "coding-agent"
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**No `applyTo`.** On GitHub.com a path-specific file selects its files with
`applyTo`; without it, there is nothing to match.

<!-- cookbook-case: no-apply-to -->
```text
error[MDATRON-E0050]: frontmatter-schema-violation
  --> .github/instructions/python.instructions.md:2:1
   = note: required property "applyTo" is missing
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**`applyto` for `applyTo`.** The misspelled key is not one either source
documents, and the file is left with no pattern.

<!-- cookbook-case: misspelled-key -->
```text
error[MDATRON-E0050]: frontmatter-schema-violation
  --> .github/instructions/python.instructions.md:2:1
   = note: unexpected property not permitted by the schema
   = unexpected:
           > applyto
   = explain: mdatron explain MDATRON-E0050
error[MDATRON-E0050]: frontmatter-schema-violation
  --> .github/instructions/python.instructions.md:2:1
   = note: required property "applyTo" is missing
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 2 error(s), 0 warning(s) across 2 finding(s)
```

The fix loop is the one the [SKILL.md recipe](./skill-md.md) walks through: the
envelope carries the offending text as untrusted quoted content, `mdatron
explain` gives the remediation, and the finding is gone from the next run once
the file is fixed.

## Make it yours

- **One profile per tool.** A team on GitHub.com only can drop `name` and
  `description` from the schema; a VS Code-only team can drop `excludeAgent`.
  Bind each directory to the profile it is held to.
- **Fail on warnings.** Use `mdatron verify --deny-warnings` so a misnamed file
  blocks the merge.
- **Keep the repository-wide file honest.** `.github/copilot-instructions.md`
  is prose; pin a section of it (`.mdatron/pins.yaml`) if other files depend on
  what it says.
