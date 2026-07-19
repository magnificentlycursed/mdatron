# serde_yaml

**Status:** Approved with **deprecation flag** (upstream archived; replacement-path noted).

**Pinned version:** `^0.9` (workspace.dependencies; resolves to 0.9.x)

## Why this dependency

Parses YAML frontmatter in markdown documents (`mdatron-core::frontmatter::parse`) and YAML config files (`.mdatron/config.yaml`, `.mdatron/patterns/*.yaml`, `.mdatron/catalogs/*.yaml`). Required for Layer 1 structural validation per DESIGN.md § Five check families.

**Alternatives considered:**

- `serde_yml` (fork of `serde_yaml`): active fork; could migrate if upstream-deprecation becomes blocking
- `saphyr` / `yaml-rust2`: don't integrate with serde derives; would require hand-rolling deserialization
- Hand-rolled YAML parser: rejected — YAML's complexity makes hand-rolling impractical for v0.1.0

## PE supply-chain notes

- **Version pin discipline:** workspace-level `serde_yaml = "0.9"`; resolves to 0.9.34+deprecated (the last published version).
- **Upstream status:** dtolnay archived the repository in 2024-05; no further updates expected upstream. The `+deprecated` suffix on 0.9.34 is intentional.
- **Migration path:** `serde_yml` fork is API-compatible; mdatron can migrate via `s/serde_yaml/serde_yml/` in Cargo.toml + imports if upstream-archive becomes operational blocking (CVE, broken transitive dep).
- **Transitive deps:** brings `unsafe-libyaml` (the bundled libyaml C-equivalent in pure Rust) and `indexmap`. No upstream-archived transitive deps.
- **`cargo audit`:** no known CVEs at pin time (0.9.34).

## Security notes

- **CVE history:** no CVEs for `serde_yaml` 0.9.x.
- **License:** MIT OR Apache-2.0; compatible with mdatron's MIT license.
- **Threat model:** YAML deserialization of operator-controlled content. The `serde_yaml::from_str` path does not execute embedded code (YAML does not have code-execution constructs analogous to Python pickle); deserialization-into-`serde_yaml::Value` is data-only.
- **`unsafe-libyaml` is `unsafe`-free Rust port:** despite the name, the crate contains no `unsafe` Rust code (it's a pure-Rust port of libyaml's C code).

## SO approval

- **Operator-attribution:** Solution Owner accepts the deprecation flag. Migration to `serde_yml` is API-compatible; deferred until operational blocker (CVE or broken transitive dep) materializes. Bootstrap-period acceptance: pinned-version + `serde_yml` migration path documented.
- **Scope justification:** YAML parsing is foundational for mdatron's Layer 1 + config loading. The dep is the standard Rust YAML solution despite upstream-archive.

## Co-authorship attribution

Per VSDD-E0100 discipline:

```
Co-authored-by: Solution Owner <so@vsdd-domains>
Co-authored-by: Platform Engineer <pe@vsdd-domains>
Co-authored-by: Security <security@vsdd-domains>
```
