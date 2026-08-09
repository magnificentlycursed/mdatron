# Input-field rename ledger

Renaming a field on one of mdatron's input contracts breaks adopters who use the
old name — but the *shape* of the breakage differs by parse mode:

- **Strict** (`deny_unknown_fields`): `routes.yaml`, `vocabulary.yaml`,
  `pins.yaml`, `code-catalogs.yaml`. An adopter file that still uses the old field
  name fails to parse with a **loud** unknown-field error.
- **Lenient** (`.mdatron/config.yaml` / `ProjectConfig`): unknown fields are
  tolerated (#80 D1). A rename here is *worse* — the old key is **silently
  ignored** and the field falls back to its `#[serde(default)]` (empty). For
  `require_frontmatter`/`vocabulary_globs` that is precisely the fail-open class
  `MDATRON-W0051`/`W0043` exist to catch, but the rename itself slips in with the
  old data dropped and no error at all.

Either way, a bare rename is an unfriendly break. The discipline below keeps it
non-breaking at introduction and legible at removal.

## Discipline (GH #36 item 3 — Ruff's `REDIRECTS` lesson)

When an input-contract field is renamed:

1. **Keep the old name accepted** with `#[serde(alias = "old-name")]` on the new
   field, so existing adopter files continue to parse for at least one MAJOR
   `mdatron_format_version` cycle. (`#[serde(alias)]` composes with
   `deny_unknown_fields`: an alias is a *known* name, so it is not rejected as
   unknown. On the lenient `config.yaml`, the alias is what prevents the silent
   drop-to-default described above.)
2. **Record the rename in the ledger below** — `old → new`, the input file, the
   version the new name landed in, and the earliest version the alias may be
   dropped.
3. **Dropping an alias** (removing the old name entirely) is itself a MAJOR
   input-contract change, versioned via `mdatron_format_version` (DEF5) per the
   SemVer discipline — never a silent removal.

## Ledger

Format: `old-name → new-name` | input file | aliased since | alias removable in.

| old → new | input file | aliased since | alias removable in |
|-----------|------------|---------------|--------------------|
| _(none yet)_ | — | — | — |
