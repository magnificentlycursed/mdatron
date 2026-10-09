# MDATRON-E0037 — sibling-file-missing

**Severity:** error
**Status:** accepted
**Introduced in:** 0.7.0

## What this means

The route claiming this file sets `requires_sibling: <name>`: a file of that
name must exist in the same directory as every file the route claims, and
this file's directory has none. The name may hold `{stem}` (from 0.8.0), the
claimed file's name without its final extension, for a per-file sidecar:
`{stem}.pipeline.json` beside `auth.md` means `auth.pipeline.json`. The name
looked for — with `{stem}` expanded — is quoted beneath the diagnostic.
Only a regular file counts, judged on the snapshot the run captured: a
directory or a symlink in the sibling's place does not, and nor does a file
created after the run read the tree.

The rule exists for pairings no reference check can see, because the missing
file refers to nothing. The case that produced it: once a repository has a
root `CLAUDE.md`, Claude Code (under its default project-instructions
setting) reads a package's `AGENTS.md` only through a `CLAUDE.md` beside it,
so a package without one is silently invisible to Claude Code while every
other agent reads it.

## How to fix

- **Create the sibling.** Add the named file next to this one; for the
  `AGENTS.md` case, a `CLAUDE.md` holding the line `@AGENTS.md`.
- **The sibling is a symlink or a directory.** Replace it with a regular
  file; the governed tree does not follow links, and a directory holds
  nothing a consumer can read as the sibling.
- **The pairing is no longer wanted.** Remove `requires_sibling` from the
  route.

## Related codes

- MDATRON-E0030 — unrouted-file: the route family's closed-world check
- MDATRON-E0110 — dead-link-target: what an `@path` import that names a missing file reports
