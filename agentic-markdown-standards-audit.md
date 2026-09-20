---
title: "agentic-markdown-standards-audit"
tags: ["reference"]
sources:
  - url: "safe_fetch sweep 2026-09-19: agents.md, agentskills.io/specification, llmstxt.org, docs.github.com, code.visualstudio.com, code.claude.com"
    title: ""
    accessed_at: "2026-09-20"
contributors: ["79Ig"]
created: 2026-09-20
updated: 2026-09-20
---

# Agentic-markdown standards audit (verified 2026-09-19 via safe_fetch)

Purpose: the citable inventory of markdown-file standards used by agentic coding tools, tiered by conformance checkability, for the mdatron standards pack — reference-implementation cookbook + shipped verified examples that double as a TOUR OF FEATURES (operator framing 2026-09-19: "real well-known use cases that can give a tour of features"). Operator rulings 2026-09-19: all three tiers in scope; llms.txt included (which entails governing markdown content in a .txt-named carrier — consistent with #66, which declined bare-YAML carriers, not markdown in other extensions).

## Tier 1 — structured, citable (mdatron-native targets)

### Agent Skills SKILL.md — https://agentskills.io/specification (Anthropic-originated open standard; repo github.com/agentskills/agentskills)
Path: <skill-name>/SKILL.md; optional scripts/, references/, assets/. Claude Code locations: .claude/skills/<name>/SKILL.md (project), ~/.claude/skills (user), enterprise managed dir; precedence enterprise > personal > project. REQUIRED frontmatter: name (1-64 chars; lowercase letters, digits, hyphens; no leading/trailing/consecutive hyphens; MUST match parent directory name); description (1-1024, non-empty). OPTIONAL: license; compatibility (1-500 chars); metadata (string->string map); allowed-tools (space-separated, experimental). Body: "There are no format restrictions" (advisory: <500 lines, instructions <5000 tokens). Reference validator exists: skills-ref validate.
TWO CONFORMANCE PROFILES — the interop trap: claude.ai upload / Skills API / package_skill.py hard-error on ANY key beyond the spec six (allowed-tools, compatibility, description, license, metadata, name — verbatim error: "Unexpected key(s) in SKILL.md frontmatter"), while Claude Code accepts ~20 extended keys (when_to_use, argument-hint, arguments, disable-model-invocation, user-invocable, disallowed-tools, model, effort, context, agent, background, hooks, paths, shell). A skill valid locally FAILS upload. GitHub Copilot code review also consumes Agent Skills from the PR head branch.
Claude Code parse rules (checkable): frontmatter read only when opening --- is the file's FIRST line; description+when_to_use truncated at 1,536 chars in listings.

### llms.txt v2 — https://llmstxt.org (Jeremy Howard / Answer.AI; repo AnswerDotAI/llms-txt; published 2024-09-03, modified 2026-08-10)
Website-facing: /llms.txt at site root or any subpath; covers URLs under its path; most-specific wins. Markdown content, .txt filename. REQUIRED ordered structure: optional BOM; H1 with project/site name ("the only required section"); optional blockquote summary; zero+ non-heading markdown sections; zero+ H2-delimited "file list" sections — each entry "a required markdown hyperlink [name](url), then optionally a : and notes". Reserved semantics: "## Optional" H2 = links skippable under short context. Companion: per-page markdown mirrors (page.html.md / page.md / index.html.md); rel="alternate" type="text/markdown", rel="describedby". Adoption: thousands of sites, docs platforms auto-generate, Chrome Lighthouse audits for it, OpenAI/Anthropic/Gemini publish for own docs. Still a proposal — no committed major consumer.

## Tier 2 — frontmatter-only, citable

### GitHub Copilot *.instructions.md — https://docs.github.com/en/copilot/how-tos/custom-instructions/adding-repository-custom-instructions-for-github-copilot
.github/instructions/**/NAME.instructions.md ("must end with .instructions.md"). GitHub.com profile keys: applyTo (comma-separated globs; required for auto-apply) + excludeAgent ("code-review" | "cloud-agent"). VS Code profile (code.visualstudio.com/docs/agent-customization/custom-instructions): name / description / applyTo — ALL optional. TWO PUBLISHERS, DIFFERENT KEY SETS — a conformance target must name which profile it cites. .github/copilot-instructions.md itself: freeform-by-design ("natural language instructions... in Markdown format"). Copilot also reads AGENTS.md (nearest-wins) and root CLAUDE.md/GEMINI.md as alternatives.

### Claude Code subagents .claude/agents/*.md — https://code.claude.com/docs/en/sub-agents
"Only name and description are required." name: lowercase+hyphens, no ":", not starting with "-". Optional: tools, disallowedTools, model (sonnet|opus|haiku|fable|full-ID|inherit), permissionMode, maxTurns, skills, mcpServers, hooks, memory (user|project|local), background, omitClaudeMd, effort, isolation (worktree), color, initialPrompt, experimental.cacheTtl. DOCUMENTED SILENT-SKIP CONDITIONS (= the natural conformance ruleset): no name -> treated as documentation; frontmatter --- not on line 1 -> no frontmatter; name containing ":" or leading "-" -> skipped+logged; name without description -> skipped; unparseable YAML -> skipped. Silent skip is the fail-open class mdatron exists to make loud.

### Claude Code skills extended profile + legacy .claude/commands/*.md — https://code.claude.com/docs/en/skills (the slash-commands URL now redirects here; commands unified into skills). Legacy commands: same frontmatter minus name/paths; command name = filename; subdirectories become ":"-separated prefixes.
### .claude/rules/*.md — https://code.claude.com/docs/en/memory — one documented key: paths (glob list; brace expansion; budget 1,000 expanded patterns / 4 MiB per rule; absent = loads unconditionally). VS Code also reads this format.
(Excluded as targets: VS Code *.prompt.md — deprecated for Agent Host sessions, migration path is agent skills; *.chatmode.md — dead, replaced by *.agent.md.)

## Tier 3 — freeform-by-design (location/existence checks + project-imposed structure)

### AGENTS.md — https://agents.md (stewarded by the Agentic AI Foundation under the Linux Foundation; emerged from OpenAI Codex, Amp, Jules, Cursor, Factory)
FAQ verbatim: "Are there required fields? No. AGENTS.md is just standard Markdown. Use any headings you like." Location contract IS specified: repo root + nested per subproject; "the closest AGENTS.md to the edited file wins; explicit user chat prompts override everything." 20+ adopters incl. Codex, Jules, GitHub Copilot coding agent, Cursor, Zed, VS Code, Devin, Windsurf, Gemini CLI. Suggested-only sections: project overview, build/test commands, code style, testing, security.

### CLAUDE.md — https://code.claude.com/docs/en/memory
In-repo locations: ./CLAUDE.md or ./.claude/CLAUDE.md; CLAUDE.local.md (personal, expected gitignored); <subdir>/CLAUDE.md loaded on demand; ancestor files concatenated root->cwd. Out-of-repo: ~/.claude/CLAUDE.md + managed-policy paths. Content freeform (advisory <200 lines) EXCEPT the checkable @path import micro-syntax: imports must resolve, max depth 4 hops, code spans/fences exempt, block HTML comments stripped. INTEROP TRAP: default mode claude-md-or-agents-md — AGENTS.md is read ONLY when no CLAUDE.md exists in cwd-or-above, so a repo shipping both at root has its AGENTS.md invisible to Claude Code. Names documented as NOT read: AGENTS.local.md, AGENTS.override.md, .agents/. UNVERIFIED: which of ./CLAUDE.md vs ./.claude/CLAUDE.md wins when both exist — pin before shipping a rule about the pair.

## mdatron mapping notes (each recipe doubles as a DSL falsification probe)

Expressible today: frontmatter schema classes (patterns/lengths/required/additionalProperties — the two SKILL.md profiles are literally two schema classes), section_rules (H1/H2 structure, counts), route location globs, marker patterns (file-list item shape), pins (pin an AGENTS.md build-commands section against silent drift). Suspected gaps — write the recipe, hit the wall, file the issue: frontmatter-name == parent-directory (path-aware predicate); cross-file conditional existence (the AGENTS.md shadowing trap; CLAUDE.local.md-committed warning; "glob must NOT exist" rules); @import resolution (non-markdown-link syntax — link-family-adjacent); external https URL verification (link family is tree-confined by design); blockquote-position-after-H1 ordering.

Related: [[reference-architecture-audits]]; tracking issue on the mdatron crosslink tracker (standards pack).
