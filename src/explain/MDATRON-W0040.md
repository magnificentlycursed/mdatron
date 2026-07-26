# MDATRON-W0040 — governed-file-has-no-frontmatter

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

This file matches one of the `require_frontmatter` globs in
`.mdatron/config.yaml`, but it carries no frontmatter block — so no schema and
no rule can govern it, and without this warning it would pass the walk in a way
indistinguishable from having been validated clean.

This closes the loud-failure/silent-absence asymmetry (the fence-edge consumer
raise): a frontmatter block that fails to *parse* is loud (`MDATRON-E0001`),
but a file with no block at all used to be silently identical to "not
governed". Inside the globs you have declared must-have-frontmatter, absence is
now a warning.

The check is opt-in (#80 D2): with no `require_frontmatter` key in the config,
no file warns. Files outside the declared globs never warn, whatever they
contain.

## How to fix

- **The file should be governed.** Add the frontmatter block — `---` on the
  very first line, YAML, closed by a `---` line — including the `schema_class:`
  field that routes it to its schema.
- **The file is genuinely prose-only.** Narrow the `require_frontmatter` globs
  in `.mdatron/config.yaml` so they cover only the typed corpus.
- **The block exists but starts mid-file.** Frontmatter must open on line 1 (a
  leading UTF-8 BOM and CRLF line endings are tolerated); move the block to the
  top of the file.

## See also

- `mdatron explain MDATRON-E0001` — the loud half: present-but-malformed
  frontmatter.
