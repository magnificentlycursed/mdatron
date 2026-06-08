# MDATRON-E0001 — frontmatter-parse-failed

**Severity:** error
**Status:** accepted
**Introduced in:** 0.1.0

## What this means

The file's YAML frontmatter (the block between `---` markers at the top of the
markdown document) could not be parsed as valid YAML. mdatron stops Layer 1 and
Layer 2 processing for this file because both layers depend on a well-formed
frontmatter document; subsequent diagnostics for the file would be cascade
noise.

## How to fix

Open the file at the reported line and review the frontmatter block. Common
shapes for this failure:

- A missing closing `---` marker — the parser reads to end-of-file looking for
  the close
- An unquoted string containing a YAML reserved character (`:`, `#`, `&`, `*`,
  `!`) — quote the value with double quotes
- Tab characters used for indentation — YAML rejects tabs; use spaces
- A list item or mapping value with inconsistent indentation across siblings

Round-trip the frontmatter through a YAML parser locally (`yq`, `python -c
"import yaml; yaml.safe_load(open('your-file.md').read())"`, or any YAML linter)
to surface the precise parse error.

## See also

- [`DESIGN-MDATRON.md` § Layer 1: Structural (JSON Schema)](../../DESIGN-MDATRON.md#layer-1-structural-json-schema)
  — frontmatter format + parsing rules

## Related codes

- MDATRON-E0002 — `schema_class` references an unknown class (next layer up;
  fires only when E0001 passes)
- MDATRON-E0050 — frontmatter parses but violates its schema
