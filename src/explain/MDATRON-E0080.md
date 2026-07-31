# MDATRON-E0080 — pipeline-orchestration-failure

**Severity:** error
**Status:** accepted
**Introduced in:** 0.1.0

## What this means

The verify pipeline failed to complete — schemas could not be loaded, patterns
could not be read, the project layout was malformed at the structural level, or
JSON-output serialization itself failed. The pipeline did not run to
completion; no finding-level diagnostics were emitted for the project files.

`E0080` is a single code spanning several failure senses. Under `--json` the
envelope carries a structured `pipeline_error` object that names the specific
sense so a machine consumer need not parse prose (#112):

```json
"pipeline_error": { "code": "MDATRON-E0080", "kind": "config", "message": "…" }
```

`kind` is one of: `config` (jurisdiction/config load), `io` (a read failed),
`schema_load`, `pattern_load`, `glob` (a bad `file_globs` pattern),
`frontmatter`, `index_build`, `expr_parse`, `eval` (a rule expression), and
`bound_exceeded` (a declared input resource bound — per-file or aggregate byte
size, structural or expression nesting depth — was exceeded, #124). The
object is present only when `pipeline_status` is `failed`, and — unlike the
stderr `= note:` — it survives `--quiet`, so `--json --quiet` (the CI mode)
receives the reason in-band.

## How to fix

Read the `pipeline_error.kind` / `= note:` for the specific failure sense, then
apply the matching corrective pattern below.

**Common (operator-fixable):**

- **Missing `.mdatron/` directory.** The project has not been initialized.
  Run `mdatron init` to scaffold `.mdatron/` at the project root — the
  `schemas/` and `patterns/` directories plus the seeded `config.yaml` and the
  init manifest. The single-file check families (`routes.yaml`, `pins.yaml`,
  `vocabulary.yaml`) are files, not directories.
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

- the mdatron design reference, § Summary (in the project repository)
  — how mdatron loads schemas + patterns before the pipeline runs

## Related codes

- MDATRON-E0070 — project root could not be resolved (fires before pipeline
  orchestration)
- MDATRON-E0001 / E0050 — per-file diagnostics that emit when the pipeline
  itself runs to completion
