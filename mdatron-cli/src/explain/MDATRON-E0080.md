# MDATRON-E0080 — pipeline-orchestration-failure

**Severity:** error
**Status:** accepted
**Introduced in:** 0.1.0

## What this means

The verify pipeline failed to complete — schemas could not be loaded, patterns
could not be read, the project layout was malformed at the structural level, or
JSON-output serialization itself failed. The pipeline did not run to
completion; no finding-level diagnostics were emitted for the project files.

## How to fix

Read the accompanying `= note:` line for the specific failure shape, then
apply the matching corrective pattern below.

**Common (operator-fixable):**

- **Missing `.mdatron/` directory.** The project has not been initialized.
  Create `.mdatron/schemas/` and `.mdatron/patterns/` at the project root
  (v0.1.x: `mdatron init` will scaffold this for you).
- **Malformed schema or pattern file.** A `.mdatron/schemas/*.json` or
  `.mdatron/patterns/*.yaml` file failed to parse. Validate the file
  out-of-band (`jq < schema.json`, `yq < pattern.yaml`) to surface the
  parse error.
- **Permission denied on a file mdatron tried to read.** Check filesystem
  permissions on the project's `.mdatron/` tree and the markdown files it
  references.

**Rare (file an issue):**

- **JSON serialization failure under `--json`.** Open an issue with the
  finding payload that caused the failure; this is an internal bug, not a
  configuration problem.

## See also

- [`DESIGN-MDATRON.md` § Two-layer architecture](../../DESIGN-MDATRON.md#two-layer-architecture)
  — how mdatron loads schemas + patterns before the pipeline runs

## Related codes

- MDATRON-E0070 — project root could not be resolved (fires before pipeline
  orchestration)
- MDATRON-E0001 / E0050 — per-file diagnostics that emit when the pipeline
  itself runs to completion
