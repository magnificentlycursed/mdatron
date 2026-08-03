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

Where the platform provides a handle-relative no-follow open — Unix, via
`openat` with `O_NOFOLLOW` — a link swapped after the check cannot redirect
the read: the handle that passed confinement is the handle that is read.
Platforms without that primitive fall back to a per-component symlink check
followed by an ordinary open, so a component swapped between the check and the
open is followed — the swap-proof guarantee is Unix-only. That weaker fallback
posture is the one place the snapshot model's check-then-read closure is
reopened, and it holds only until a handle-based walk lands on those
platforms.

## How to fix

Replace the symlink with the real file or directory (copy or move the content
into the governed tree at the referenced path), or point the referencing
declaration at the link's target location directly if it already lives inside
the project root.

## Related codes

- MDATRON-E0010 — an absolute source path
- MDATRON-E0011 — a parent-directory (`..`) segment in a source path
