# MDATRON-E0060 — managed-manifest-drift

**Severity:** error
**Status:** accepted
**Introduced in:** 0.1.0

## What this means

A file that `mdatron init` deploys and manages was modified after
initialization. The managed partition is the set of engine-owned files listed
in `.mdatron/manifest.yaml` with their sha256 content hashes (`DESIGN.md` §
Governance data is governed). On a re-run, `mdatron init` recomputes each
managed file's hash and compares it to the manifest; a mismatch is drift, and
`init` refuses rather than silently overwrite your edit or trust a changed
engine-owned file.

This is a governance guardrail, not a corruption check: the managed files are
the engine's own configuration surface. Adopter-authored data — your schemas in
`.mdatron/schemas/` and patterns in `.mdatron/patterns/` — lives outside the
manifest and is never guarded or touched by `init`.

## How to fix

Read the `= note:` line for the drifted file and its recorded-vs-found hashes,
then apply the matching pattern:

- **You edited a managed file by mistake.** Restore it — `git checkout
  .mdatron/<file>` if it is committed, or re-run `mdatron init` in a clean
  checkout to redeploy the engine defaults.
- **You meant to customize engine configuration.** Managed files are not the
  place for it in v0.1.x; put adopter data in `.mdatron/schemas/` or
  `.mdatron/patterns/`, which are outside the managed partition.
- **The engine defaults changed across an mdatron upgrade.** The manifest
  records the version that deployed each file; restore the file to match the
  manifest, or reinitialize in a clean tree to pick up new defaults.

The manifest never lists itself (a fixed point); its own integrity is anchored
by repository commit review, not by the engine.

## See also

- [`DESIGN.md` § Governance data is governed](../../DESIGN.md#summary)
