# MDATRON-E0070 — project-root-unresolved

**Severity:** error
**Status:** accepted
**Introduced in:** 0.1.0

## What this means

mdatron could not resolve the project root before the verify pipeline opened.
This typically means the current working directory is invalid (deleted out from
under the process, permission denied, or otherwise un-statable) and no
`--project-root` flag was passed to override.

## How to fix

Two corrective paths:

- **Pass `--project-root`** with an absolute path: `mdatron verify
  --project-root /path/to/project`. This is the recommended shape when invoking
  mdatron from a hook, CI step, or wrapper script — the explicit path avoids
  any dependency on the caller's working directory.
- **`cd` into a valid directory** first and re-run `mdatron verify`. The
  current-directory default is operator-ergonomic but works only when the
  process's working directory is reachable.

## See also

- [`DESIGN.md` § Summary](../../DESIGN.md#summary)
  — `--project-root` flag semantics

## Related codes

- MDATRON-E0080 — pipeline orchestration failed AFTER project root was resolved
- MDATRON-E0010 / E0011 — path-confinement violations within an otherwise valid
  project root (reserved for v0.1.x path-confinement check; explain pages land
  when the check ships)
