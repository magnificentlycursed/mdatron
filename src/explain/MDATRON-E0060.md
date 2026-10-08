# MDATRON-E0060 — managed-manifest-drift

**Severity:** error
**Status:** accepted
**Introduced in:** 0.1.0

## What this means

A file that `mdatron init` deploys and manages was modified after
initialization. The managed partition is the set of engine-owned files listed
in `.mdatron/manifest.yaml` with their sha256 content hashes (`DESIGN.md` § Validation is data-driven, governance data is governed). Both commands recompute each
managed file's hash and compare it to the manifest, by one shared check, so
they always agree: a mismatch is drift. `mdatron init` refuses rather than
silently overwrite your edit or trust a changed engine-owned file, and
`mdatron verify` reports it as this finding (exit `1`) — so a CI that runs only
`verify` sees an edited template. (Through 0.7.0 only `init` checked; `verify`
read the manifest for its tombstones and lineage digest alone.) A missing
managed file is not drift: `init` restores it.

The comparison is of bytes: a checkout that rewrites line endings (Git's
`core.autocrlf` on Windows) changes the hash of a committed template and reads
as drift.

This is a governance guardrail, not a corruption check: the managed files are
the engine's own configuration surface. Adopter-authored data — your schemas in
`.mdatron/schemas/` and patterns in `.mdatron/patterns/` — lives outside the
manifest and is never guarded or touched by `init`.

## How to fix

Read the `= note:` line for the drifted file and its recorded-vs-found hashes,
then apply the matching pattern:

- **You edited a `*.example` template.** The templates `init` deploys are
  engine prose to copy from, not to edit: copy one to its real name
  (`routes.yaml.example` → `routes.yaml`) and edit the copy; restore the
  template.
- **You edited a managed file by mistake.** Restore it — `git checkout
  .mdatron/<file>` if it is committed, or re-run `mdatron init` in a clean
  checkout to redeploy the engine defaults.
- **You meant to customize `config.yaml`.** Since its demotion to adopter-owned,
  `config.yaml` is *seeded* by init and adopter-owned — editing it (e.g. its
  `file_globs` jurisdiction) is sanctioned and does not drift. If your tree's
  manifest predates the demotion and still lists `config.yaml` as managed,
  replace that entry with a demotion tombstone (path, reason, owner) in a
  reviewed commit; fresh `mdatron init` deployments already ship the demoted
  shape. Other adopter data belongs in `.mdatron/schemas/` or
  `.mdatron/patterns/`, outside the managed partition.
- **A template was edited, so an upgrade cannot refresh it.** The manifest
  records each managed file's path and sha256, not the version that wrote it.
  An unedited `*.example` template is refreshed forward when a newer mdatron
  runs `init` (from an earlier version's content it knows), and from 0.8.0 on
  left as recorded when an older one does (0.7.0 still moves it back); an
  edited template is this drift. Restore
  it to the recorded content (or delete it and re-run `init`), or copy it to
  its real name — the `.example` files are documentation; the real file is
  yours.

The manifest never lists itself (a fixed point); its own integrity is anchored
by repository commit review, not by the engine.

## See also

- the mdatron design reference, § Validation is data-driven, governance data is governed (in the project repository)
