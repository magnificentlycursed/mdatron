# MDATRON-E0035 — name-dir-mismatch

**Severity:** error
**Status:** accepted
**Introduced in:** 0.7.0

## What this means

The route claiming this file sets `name_equals_dir: <field>`: the named
frontmatter field must hold a string equal to the name of the directory that
contains the file. Here the field is absent, is not a string, differs from
the directory name, or the file sits at the project root with no directory.
The Agent Skills specification makes this a hard rule for `SKILL.md`, whose
`name` must match its skill directory, and a filename grammar cannot express
it because the file is always called `SKILL.md`. The field, its value, and
the directory are quoted beneath the diagnostic.

## How to fix

- **Rename the directory** or **change the field** so the two agree. Tools that
  key a skill by its directory do not find one whose `name` differs.
- **The field is missing.** Add it; the route requires it on every file it
  claims.

## Related codes

- MDATRON-E0030 — unrouted-file: the route family's closed-world check
- MDATRON-E0050 — frontmatter-schema-violation: what a bound schema reports
