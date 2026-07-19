# serde_json

**Status:** Approved.

**Pinned version:** `^1.0` (workspace.dependencies; resolves to 1.0.x latest within major)

## Why this dependency

JSON Schema files (Layer 1 structural validation per DESIGN.md § Five check families) ship as `.json` files at `.mdatron/schemas/<class>.json`. Adopters consuming SARIF output (per DESIGN.md § Diagnostics are a versioned contract) need JSON serialization. Both paths require a JSON library.

**Alternatives considered:**

- `simd-json`: rejected for v0.1.0 — SIMD acceleration adds complexity unjustified at current throughput needs (mdatron processes tens-to-hundreds of files, not millions)
- `json` (legacy crate): rejected — no serde integration

## PE supply-chain notes

- **Version pin discipline:** workspace-level `serde_json = "1"`; resolves to latest 1.x. Pinned via `Cargo.lock`.
- **Maintainer trust:** dtolnay maintains alongside `serde`; same trust posture (~50k reverse deps).
- **Transitive deps:** `itoa` + `ryu` + `memchr` + `serde`. All foundational Rust crates; no archived deps.
- **`cargo audit`:** no known CVEs at pin time.

## Security notes

- **CVE history:** no CVEs for `serde_json` 1.x.
- **License:** MIT OR Apache-2.0; compatible with mdatron's MIT license.
- **Threat model:** JSON deserialization of operator-controlled schemas + machine-generated SARIF output. Schemas are loaded only from `.mdatron/schemas/` per canonical-schema-path discipline (BOUNDARY-PREAMBLE § 7); not PR-modifiable in the operational threat model.

## SO approval

- **Operator-attribution:** Solution Owner confirms JSON Schema + SARIF emission are foundational v0.1.0 deliverables; the dep is well-justified.
- **Scope justification:** Both Layer 1 (JSON Schema input) and SARIF (output) workflows depend on JSON serialization; one dep serves two use cases.

## Co-authorship attribution

Per VSDD-E0100 discipline:

```
Co-authored-by: Solution Owner <so@vsdd-domains>
Co-authored-by: Platform Engineer <pe@vsdd-domains>
Co-authored-by: Security <security@vsdd-domains>
```
