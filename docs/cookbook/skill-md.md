# Recipe: SKILL.md (Agent Skills)

A skill that is broken in the wrong way does not fail. It loads with the
fields it could read, or with none, and the agent quietly stops using it the
way you meant. Claude Code ignores a frontmatter field it does not recognize
"without reporting an error"; frontmatter whose opening `---` is not on the
first line is read as body text; a skill that works in Claude Code fails
claude.ai upload the moment it carries a Claude-Code-only field. None of
these tells you at the time. Agent output is stochastic, so the effect is a
shifted distribution, never a legible error: the only reliable detection is a
check *of* the configuration, run where it changes. This recipe is that
check for `SKILL.md`.

The complete, runnable project is
[`examples/standards/skill-md/`](../../examples/standards/skill-md/AGENTS.md);
every configuration file and every diagnostic shown below is compared against
it by the test suite, so this page cannot drift from what mdatron does.

## Evaluated against

Each profile's schema records the sources its rules come from in an
`x-source` block, and this table must match those blocks exactly (the test
suite enforces it). The Agent Skills specification is pinned to a commit of
its source file; the Claude Code reference is an unversioned page, pinned to
the date it was read.

<!-- cookbook-provenance-start -->
| Standard | Source | Revision | Revision date | Retrieved |
|---|---|---|---|---|
| Agent Skills specification | https://agentskills.io/specification | agentskills/agentskills@217be548739f21d6008915c29aefe320ea1a90af docs/specification.mdx | 2026-08-04 | 2026-09-28 |
| Claude Code skills reference | https://code.claude.com/docs/en/skills | unversioned documentation page; field table as retrieved | unversioned | 2026-09-28 |
<!-- cookbook-provenance-end -->

To check for upstream drift, compare the pinned commit with the specification
file's latest one
(`gh api 'repos/agentskills/agentskills/commits?path=docs/specification.mdx&per_page=1' --jq '.[0].sha'`)
and re-read the Claude Code field table; when either moved, update the schema,
its `x-source` block, and this table together.

## Two profiles, two directories

The same file format has two conformance profiles, and choosing one is the
first decision:

- **Spec-strict**: the specification's six fields (`name`, `description`,
  `license`, `compatibility`, `metadata`, `allowed-tools`) and nothing else.
  This is what claude.ai skill uploads, the Skills API and `package_skill.py`
  enforce; any other key fails with "Unexpected key(s) in SKILL.md
  frontmatter". Use it for skills you upload, publish, or share with other
  agents: Codex reads `.agents/skills/`.
- **Claude Code extended**: every field Claude Code documents (the six plus
  `when_to_use`, `argument-hint`, `arguments`, `disable-model-invocation`,
  `user-invocable`, `disallowed-tools`, `model`, `effort`, `context`, `agent`,
  `background`, `hooks`, `paths`, `shell`), and nothing else. Use it for
  Claude Code project skills in `.claude/skills/`. These are not upload-safe.

The example binds `.agents/skills/` to the first and `.claude/skills/` to the
second. If you upload the skills in `.claude/skills/` too, bind that
directory to `skill-spec` instead.

## The configuration

The walked set is exactly the `SKILL.md` files; a skill's `references/`,
`scripts/` and `assets/` carry no frontmatter and are left out.

<!-- cookbook-file: .mdatron/config.yaml -->
```yaml
# The walked set: exactly the SKILL.md files. A skill's other files
# (references/, scripts/, assets/) are not frontmatter-bearing and are not
# walked, so the closed-world route check does not demand a route for them.
file_globs:
  - ".agents/skills/*/SKILL.md"
  - ".claude/skills/*/SKILL.md"
```

A skill file cannot say which schema it follows: under the spec-strict
profile, a `schema_class` key would itself be the extra key that fails
upload. So each route binds its files to a schema with `schema:`, and the
spec's "name must match the parent directory" rule is `name_equals_dir:`.

<!-- cookbook-file: .mdatron/routes.yaml -->
```yaml
mdatron_format_version: 1
routes:
# Portable skills: the Agent Skills spec-strict profile. Codex reads
# .agents/skills; the same files are what you upload to claude.ai or the
# Skills API. The spec requires `name` to match the skill directory.
- files: ".agents/skills/*/SKILL.md"
  governed_by: AGENTS.md
  schema: skill-spec
  name_equals_dir: name
# Claude Code project skills: the extended profile. Every documented field is
# allowed, and anything else is a finding (Claude Code ignores unknown fields
# silently). These are NOT upload-safe; bind uploaded skills to skill-spec.
- files: ".claude/skills/*/SKILL.md"
  governed_by: AGENTS.md
  schema: skill-claude-code
```

The schemas are `.mdatron/schemas/skill-spec.json` and
`.mdatron/schemas/skill-claude-code.json` in the example project (JSON Schema
draft 2020-12). Two Claude Code fields only mean something together with
`context: fork`, which a schema cannot say field by field, so a pattern file
states it:

<!-- cookbook-file: .mdatron/patterns/claude-code-skill.yaml -->
```yaml
mdatron_dsl_version: 1
pattern:
  id: claude-code-skill
  description: Claude Code skill fields that only apply together
  rules:
    - id: agent-needs-fork
      context: skill-claude-code
      assert: 'not defined($self.agent) or $self.context == "fork"'
      code: SKILLS-E0001
      message: "`agent` only applies with `context: fork` (Claude Code skills reference); without it the field does nothing"
    - id: background-needs-fork
      context: skill-claude-code
      assert: 'not defined($self.background) or $self.context == "fork"'
      code: SKILLS-E0002
      message: "`background` only applies with `context: fork` (Claude Code skills reference); without it the field does nothing"
```

## What each rule becomes

| Source rule | mdatron construct |
|---|---|
| `name`: 1-64 characters; a-z, 0-9 and hyphens; no leading, trailing or consecutive hyphen | schema `pattern` + `maxLength` |
| `name` must match the parent directory name | route `name_equals_dir: name` (`E0035`) |
| `description`: 1-1024 characters, non-empty (spec) | schema `minLength`/`maxLength`, `required` |
| `compatibility`: 1-500 characters if present | schema `minLength`/`maxLength` |
| `metadata`: string keys to string values (spec) | schema `additionalProperties: {type: string}` |
| Upload and the Skills API refuse any key beyond the six | spec profile `additionalProperties: false` (`E0050`) |
| Claude Code ignores unrecognized fields without an error | extended profile `additionalProperties: false` (`E0050`) |
| Frontmatter is read only when the opening `---` is the first line | same rule in mdatron's parser; a bound file with no frontmatter is checked as empty, so a required field is reported |
| `effort`, `context`, `shell` take fixed values | schema `enum`/`const` |
| Boolean fields accept `true`/`false` and, from v2.1.218, `yes`/`no`/`on`/`off`/`1`/`0` | schema `anyOf` of boolean and a case-insensitive pattern |
| `agent` and `background` apply only with `context: fork` | pattern rules (`SKILLS-E0001`, `SKILLS-E0002`) |

## What this does not check

- The 1,536-character cap applies to `description` and `when_to_use`
  *combined* in Claude Code's skill listing. The extended profile bounds
  `description` alone; the rule language has no arithmetic to add the two.
- The specification's table says "Lowercase letters, numbers, and hyphens"
  while its bullet list says "unicode lowercase alphanumeric characters
  (a-z, 0-9)". The profile uses ASCII `a-z0-9`, the reading its examples
  support.
- Body guidance (under 500 lines; instructions under about 5,000 tokens; file
  references one level deep) is advice, not a conformance rule, and is not
  checked.
- Whether `allowed-tools` names real tools, whether `model` names an
  available model, and the shape of `hooks` are not checked.
- `argument-hint` has no documented type. Written unquoted, `[issue-number]`
  parses as a YAML list rather than a string; the profile accepts either, since
  how Claude Code renders a list is not documented.
- Duplicate skill names across locations, and precedence between enterprise,
  personal and project skills, involve files outside the repository.

## The silent failures, and what mdatron says

Each case below is the example project with one edit. The vendor behaviour is
quoted from the source documentation; the mdatron output is the real output,
compared with the example by the test suite.

**A Claude Code field in a skill you upload.** Claude Code accepts
`argument-hint`; claude.ai upload refuses it ("Unexpected key(s) in SKILL.md
frontmatter: argument-hint. Allowed properties are: allowed-tools,
compatibility, description, license, metadata, name").

<!-- cookbook-case: upload-trap -->
```text
error[MDATRON-E0050]: frontmatter-schema-violation
  --> .agents/skills/pdf-tools/SKILL.md:4:1
   = note: unexpected property not permitted by the schema
   = unexpected:
           > argument-hint
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**A name that does not match its directory.** The specification requires the
match; Claude Code does not, so the skill works locally and fails elsewhere.

<!-- cookbook-case: name-dir -->
```text
error[MDATRON-E0035]: name-dir-mismatch
  --> .agents/skills/pdf-tools/SKILL.md:2
   = note: the route requires this frontmatter field to equal the file's parent directory name, and it does not
   = field:
           > name
   = value:
           > pdf-tool
   = directory:
           > pdf-tools
   = help: rename the directory or change the field so the two agree (tools that key a skill by its directory will not match a differing name)
   = explain: mdatron explain MDATRON-E0035
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**A blank line before the frontmatter.** Claude Code "reads the frontmatter
only when the opening `---` is the file's first line. Otherwise it treats the
whole file, `---` markers included, as skill content." The skill still loads.

<!-- cookbook-case: blank-first-line -->
```text
error[MDATRON-E0050]: frontmatter-schema-violation
  --> .claude/skills/release-notes/SKILL.md:1
   = note: required property "description" is missing
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**A misspelled field.** `when-to-use` instead of `when_to_use`: Claude Code
"ignores a field it doesn't recognize without reporting an error".

<!-- cookbook-case: typo-key -->
```text
error[MDATRON-E0050]: frontmatter-schema-violation
  --> .claude/skills/release-notes/SKILL.md:4:1
   = note: unexpected property not permitted by the schema
   = unexpected:
           > when-to-use
   = explain: mdatron explain MDATRON-E0050
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

**A field that needs another.** `agent` with no `context: fork` does nothing.

<!-- cookbook-case: agent-without-fork -->
```text
error[SKILLS-E0001]: agent-needs-fork
  --> .claude/skills/release-notes/SKILL.md:1
   = note: `agent` only applies with `context: fork` (Claude Code skills reference); without it the field does nothing
mdatron verify: 1 error(s), 0 warning(s) across 1 finding(s)
```

## The loop: finding, explanation, fix

The same upload-trap finding, as an agent reads it from
`mdatron verify --json` (`findings[0]`). The offending key rides in `quoted`,
marked `"trusted": false`: it is text from the repository, escaped, and never
part of the engine's own message, so a configuration file carrying
instruction-shaped text cannot use the diagnostic to reach the agent.

<!-- cookbook-envelope: upload-trap -->
```json
{
  "code": "MDATRON-E0050",
  "severity": "error",
  "summary": "frontmatter-schema-violation",
  "message": "unexpected property not permitted by the schema",
  "help": null,
  "location": {
    "file": ".agents/skills/pdf-tools/SKILL.md",
    "line": 4,
    "column": 1
  },
  "explain_ref": "MDATRON-E0050",
  "quoted": [
    {
      "label": "unexpected",
      "content": "argument-hint",
      "origin": "adopter",
      "trusted": false
    }
  ],
  "fingerprint": "v1:570c79630f29412d89afe81eeeb82f02"
}
```

`mdatron explain MDATRON-E0050` gives the remediation ("either remove the
extra field, or — if the field is genuinely needed — register it in the
schema"). Here the field is not allowed where the skill goes, so it is
removed. The next run is clean, and the finding's `fingerprint`, which is
stable across unrelated edits, is gone from the envelope: the correction is
verified by the check, not assumed.

```text
mdatron verify: clean
```

## Make it yours

- **Organisation policy on top of the standard.** Require `license`, or a
  `metadata.owner`, by adding them to a profile's `required` list (or
  `metadata`'s `required`). Keep the vendor rules and your policy visibly
  apart: the example states the one policy it adds, `description` required
  in the extended profile, in that schema's `description`.
- **Other skill locations.** Plugins keep skills in `<plugin>/skills/<name>/`
  and nested projects in `<subdir>/.claude/skills/`. Add a `file_globs` entry
  and a route per location, bound to the profile the location is held to.
- **Run it where the files change.** `mdatron verify` in CI blocks the merge;
  `mdatron verify --changed <file>` in an editor or agent hook checks one
  edit in place.
