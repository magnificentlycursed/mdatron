# Recipe: Claude Code subagents

A subagent file that Claude Code cannot use does not fail. The subagents
reference lists five kinds of broken file that Claude Code "skips ... without
reporting it in the session": no `name`, a `name` but no `description`, a
`name` that starts with `-` or contains `:`, YAML that does not parse, and an
opening `---` that is not on the first line. The reason, when there is one,
goes to the debug log. On top of those, a misspelled or misplaced field is
ignored "without reporting an error", and a plugin's subagent silently drops
`hooks`, `mcpServers` and `permissionMode`. The delegation you configured just
never happens, and nothing says so. This recipe checks those files where they
change.

The complete, runnable project is
[`examples/standards/subagents/`](../../examples/standards/subagents/AGENTS.md);
every configuration file and every diagnostic below is compared against it by
the test suite. The companion recipe for skills is
[SKILL.md](./skill-md.md).

## Evaluated against

Each profile's schema records its source in an `x-source` block, and this
table must match those blocks exactly (the test suite enforces it). The
subagents reference is an unversioned page: it carries no edit date, so it is
pinned to the date it was read and to the newest version gate it states.

<!-- cookbook-provenance-start -->
| Standard | Source | Revision | Revision date | Retrieved |
|---|---|---|---|---|
| Claude Code subagents reference | https://code.claude.com/docs/en/sub-agents | unversioned documentation page; version gates stated through v2.1.281 | unversioned | 2026-09-29 |
<!-- cookbook-provenance-end -->

To check for drift, fetch the page again (its Markdown form is
`https://code.claude.com/docs/en/sub-agents.md`), compare the frontmatter
table and the "Subagent files Claude Code skips" list with the schemas, and
note the newest version gate; update the schemas, their `x-source` blocks,
and this table together.

## Two profiles, two locations

- **Project, user and managed subagents** (`.claude/agents/`,
  `~/.claude/agents/`, the managed settings directory): every documented
  field, and nothing else.
- **Plugin subagents** (a plugin's `agents/` directory): the same, minus the
  fields a plugin ignores. "For security reasons, plugin subagents don't
  support the `hooks`, `mcpServers`, or `permissionMode` frontmatter fields.
  These fields are ignored when loading agents from a plugin." `initialPrompt`
  is ignored there too.

A subagent's identity is its `name`, not its path ("The filename doesn't have
to match"), and Claude Code scans agents directories recursively. The routes
use a single `*`, which crosses directory boundaries, so nested folders are
covered.

## The configuration

<!-- cookbook-file: .mdatron/config.yaml -->
```yaml
# The walked set: the subagent definition files. Claude Code scans agents
# directories recursively and a subagent's identity is its `name`, not its path.
file_globs:
  - ".claude/agents/**/*.md"
  - "plugin/agents/**/*.md"
```

<!-- cookbook-file: .mdatron/routes.yaml -->
```yaml
mdatron_format_version: 1
routes:
# Project subagents (the same format applies to ~/.claude/agents and to managed
# agents). `**` reaches every nested folder.
- files: ".claude/agents/**/*.md"
  governed_by: AGENTS.md
  schema: subagent
# A plugin's agents: hooks, mcpServers, permissionMode and initialPrompt are
# ignored there, so the plugin profile does not allow them.
- files: "plugin/agents/**/*.md"
  governed_by: AGENTS.md
  schema: subagent-plugin
```

The schemas are `.mdatron/schemas/subagent.json` and
`.mdatron/schemas/subagent-plugin.json` in the example project (JSON Schema
draft 2020-12).

## What each rule becomes

| Source rule | mdatron construct |
|---|---|
| "Only `name` and `description` are required"; a file missing either is skipped | schema `required` (`E0050`) |
| A `name` that starts with `-` or contains `:` is skipped | schema `pattern` on `name` |
| YAML that does not parse: the file is skipped | frontmatter parse (`E0001`) |
| An opening `---` not on the first line: read as documentation | same rule in mdatron's parser; the bound file is checked as empty, so the required fields are reported |
| Unrecognized fields are ignored without an error; field names are camelCase "and must match the table exactly" | `additionalProperties: false` (`E0050`) |
| `cacheTtl` belongs inside `experimental`, and only `5m` or `1h` | `experimental` object with a `cacheTtl` enum and no other keys |
| `model`, `permissionMode`, `memory`, `effort`, `isolation`, `color` take fixed values | schema `enum`/`const` (`model` also takes a full `claude-` model ID) |
| Plugin subagents ignore `hooks`, `mcpServers`, `permissionMode`, `initialPrompt` | a plugin profile without them, bound by route |

## What this does not check

- Duplicate `name`s. Claude Code loads only one of two files in the same
  agents tree with the same name, "chosen by filesystem read order", and
  resolves names across locations by priority. mdatron checks each file on
  its own.
- The 15,000-token budget for the combined descriptions (Claude Code warns at
  startup when it is exceeded).
- Whether `tools` entries resolve to real tools, whether `skills` exist, and
  whether a named MCP server is configured.
- The reference states no type for `maxTurns`; the profile requires a
  positive integer. It states no rule for `model` beyond the aliases and "a
  full model ID such as `claude-opus-5-5`"; the profile accepts those, and a
  model given in another `--model` form would be reported.
- The shape of `hooks` and of an inline `mcpServers` server beyond "an object".

## The silent skips, and what mdatron says

Each case is the example project with one edit, to
`.claude/agents/code-reviewer.md` unless noted. The quoted behaviour is the
subagents reference's; the output is mdatron's real output, compared with the
example by the test suite.

**No `name`.** "Claude Code treats the file as documentation kept beside your
agents."

<!-- cookbook-case: no-name -->
```text
error[MDATRON-E0050]: frontmatter-schema-violation
  --> .claude/agents/code-reviewer.md:2:1
   = note: required property "name" is missing
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**A `name` but no `description`.** "Claude Code skips the file and writes the
reason to the debug log."

<!-- cookbook-case: no-description -->
```text
error[MDATRON-E0050]: frontmatter-schema-violation
  --> .claude/agents/code-reviewer.md:2:1
   = note: required property "description" is missing
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**A `name` with a `:`.** Reserved for plugin-scoped identifiers; "Claude Code
doesn't load a file whose name contains one and logs an error to the debug
log."

<!-- cookbook-case: colon-name -->
```text
error[MDATRON-E0050]: frontmatter-schema-violation
  --> .claude/agents/code-reviewer.md:2:7
   = note: value at /name does not match the schema's required pattern /^[^:-][^:]*$/
   = found:
           > "team:code-reviewer"
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**YAML that does not parse.** "Claude Code reads no fields from the file,
skips it, and writes the parse error to the debug log."

<!-- cookbook-case: unparseable-yaml -->
```text
error[MDATRON-E0001]: frontmatter-parse-failed
  --> .claude/agents/code-reviewer.md:1
   = note: the file's YAML frontmatter could not be parsed; the file cannot be governed until it is valid YAML
   = parse error:
           > yaml parse error: did not find expected ',' or ']' at line 4 column 16, while parsing a flow sequence at line 3 column 8
   = explain: mdatron explain MDATRON-E0001
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**A blank line before the frontmatter.** "Claude Code reads the file as
having no frontmatter and treats it as documentation."

<!-- cookbook-case: blank-first-line -->
```text
error[MDATRON-E0050]: frontmatter-schema-violation
  --> .claude/agents/code-reviewer.md:1
   = note: required property "name" is missing
   = explain: mdatron explain MDATRON-E0050
error[MDATRON-E0050]: frontmatter-schema-violation
  --> .claude/agents/code-reviewer.md:1
   = note: required property "description" is missing
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 2 error(s), 0 warning(s) across 2 finding(s)
```

**`cacheTtl` at the top level.** The reference: "Write `cacheTtl` inside the
`experimental` map, not at the top level of the frontmatter." At the top level
it is an unrecognized field, ignored.

<!-- cookbook-case: top-level-cachettl -->
```text
error[MDATRON-E0050]: frontmatter-schema-violation
  --> .claude/agents/code-reviewer.md:10:1
   = note: unexpected property not permitted by the schema
   = unexpected:
           > cacheTtl
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**A snake_case field.** `max_turns` for `maxTurns`: field names "must match the
table exactly", and an unrecognized one is ignored, so the turn limit is
never applied.

<!-- cookbook-case: snake-case-key -->
```text
error[MDATRON-E0050]: frontmatter-schema-violation
  --> .claude/agents/code-reviewer.md:8:1
   = note: unexpected property not permitted by the schema
   = unexpected:
           > max_turns
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**`permissionMode` in a plugin's subagent** (`plugin/agents/review/security.md`).
Ignored when the agent loads from a plugin; the subagent runs in the session's
mode, not the one written in its file.

<!-- cookbook-case: plugin-permission-mode -->
```text
error[MDATRON-E0050]: frontmatter-schema-violation
  --> plugin/agents/review/security.md:9:1
   = note: unexpected property not permitted by the schema
   = unexpected:
           > permissionMode
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

The fix loop is the one the [SKILL.md recipe](./skill-md.md) walks through:
the finding's envelope view carries the offending text as untrusted quoted
content, `mdatron explain` gives the remediation, and the finding's
fingerprint is gone from the next run once the file is fixed.

## Make it yours

- **Policy on top of the reference.** Require `tools` on every project
  subagent (no subagent inherits every tool by accident), cap `maxTurns`, or
  forbid `permissionMode: bypassPermissions`, by adding `required`, `maximum`,
  or `not` to the project profile. Say in the schema's `description` which
  constraints are policy rather than the vendor's rules.
- **User and managed subagents.** They use the same format; point a route at
  the managed settings directory's `.claude/agents/` in the repository that
  distributes it.
- **Run it where the files change.** `mdatron verify` in CI blocks the merge;
  `mdatron verify --changed <file>` checks one edit in place.
