# MDATRON-E0003 — governed-file-unreadable

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A file matched by the configured `file_globs` was captured for verification,
but its content cannot be verified as text — its bytes are not valid UTF-8, or
reading it failed after the confined open (for example, an I/O error mid-read).
Nothing validated this file: no schema, rule, vocabulary, or body check ran on
it, and it does not count toward `files_checked`.

The same code reports a **managed** file listed in `.mdatron/manifest.yaml`
that `verify` cannot check against its recorded hash (from 0.8.0): not a
regular file (a FIFO or device), over the per-file input limit, or unreadable.
A managed file reached through a symlink is `MDATRON-E0012` instead. `init`
refuses to run over either; delete the file and re-run `init` to restore it.

This is a per-file finding by design. One unreadable file must not deny
verification of the rest of the tree (a whole-run abort here would be a
denial-of-verification lever: any writer able to drop one hostile file into the
governed tree could silence every other finding).

## How to fix

1. **The file should be governed.** Re-encode it as UTF-8 (mdatron verifies
   typed markdown; UTF-8 is the input contract), or repair whatever made the
   read fail.
2. **The file should not be governed.** Narrow the `file_globs` in
   `.mdatron/config.yaml` so binary or non-document files are not walked.

## See also

- the mdatron design reference, § Verification is fast where it is invoked (in
  the project repository) — the immutable-snapshot capture this finding is
  raised from

## Related codes

- MDATRON-E0001 — the file read as text, but its frontmatter YAML is malformed
- MDATRON-E0012 — the file was refused at capture because a path component is a
  symbolic link
