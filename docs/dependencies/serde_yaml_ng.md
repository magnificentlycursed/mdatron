# serde_yaml_ng

**Status:** Approved.

**Pinned version:** `^0.10` (resolves to 0.10.x)

## Why this dependency

The happy-path YAML parser: frontmatter (`frontmatter::parse`), pattern files,
the config/route/pin/vocabulary family data, and the init manifest all parse
through it. A maintained, API-compatible fork of the archived `serde_yaml`
(0.9.34+deprecated); mdatron migrated from `serde_yaml` to it in tracker #69
after the original was archived.

**Alternatives considered:**

- `serde_yaml` (original): archived/unmaintained — the reason for the migration.
- `serde_yml`: rejected — RUSTSEC-flagged at evaluation time (#69 investigation).
- `serde-saphyr`: the future span-capable option (alpha); tracked for a later
  cycle. `saphyr` is already used span-only on the error path (see saphyr.md).

## PE supply-chain notes

- **Version pin:** workspace `serde_yaml_ng = "0.10"`.
- **Maintainer trust:** actively maintained community fork; drop-in for the
  archived original.
- **`cargo audit`:** clean at pin time.

## Security notes

- **License:** MIT OR Apache-2.0; compatible.
- **Threat model:** parses untrusted adopter YAML (a trust boundary). Alias and
  nesting-depth bounds are a declared hook-time limit (DESIGN § Verification is
  fast); malformed input yields a diagnostic, never a panic.

## SO approval

- **Scope justification:** the load-bearing structured-data parser for every
  `.mdatron/` input; foundational. Migration ratified at #69.

## Co-authorship attribution

Per VSDD-E0100 discipline:

```
Co-authored-by: Solution Owner <so@vsdd-domains>
Co-authored-by: Platform Engineer <pe@vsdd-domains>
Co-authored-by: Security <security@vsdd-domains>
```
