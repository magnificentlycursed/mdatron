# MDATRON-E0012 — symlinked-component-refused

**Severity:** error
**Status:** accepted
**Introduced in:** 0.1.0

## What this means

Resolving an adopter-supplied path encountered a symbolic link. This applies
to EVERY path the engine reads — a governed file matched by `file_globs`, a
`keys:` declaration's `source`, a pinned file, a citation or link target, a
marker rule's `target_doc`, a route's `governed_by` — the quoted region names
which one. The engine reads adopter-supplied paths through validated no-follow
handles, resolved per path component (`DESIGN.md` § Verification is fast where
it is invoked), so a symlink is refused at whatever depth it appears — a
symlinked intermediate directory is as confined as a symlinked final file —
and whatever its target, inside or outside the governed tree. Refusing
symlinks unconditionally means escape detection never depends on resolving
where a link points.

The open is handle-relative and no-follow on every supported platform — Unix
via `openat` with `O_NOFOLLOW`, Windows via `NtCreateFile` with a
`RootDirectory` handle and `FILE_OPEN_REPARSE_POINT` — so a link swapped after
the check cannot redirect the read: the handle that passed confinement is the
handle that is read. On Windows the refusal is decided on the handle's reparse
attribute and is tag-agnostic: every reparse point — symlink, junction, volume
mount point — is refused alike, never followed. The swap-proof guarantee is
universal.

## How to fix

Replace the symlink with the real file or directory (copy or move the content
into the governed tree at the referenced path), or point the referencing
declaration at the link's target location directly if it already lives inside
the project root.

## Related codes

- MDATRON-E0010 — an absolute source path
- MDATRON-E0011 — a parent-directory (`..`) segment in a source path
