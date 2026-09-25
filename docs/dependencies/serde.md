# serde

**Status:** Approved (retroactive — the dependency was added early in the project, June 2026, before the dependency-approval discipline existed; this record was written under the project's bootstrap-period exception, since discharged, to bring it into compliance).

**Pinned version:** `^1.0` (workspace.dependencies; resolves to 1.0.x latest within major)

## Why this dependency

The de-facto Rust serialization framework. Used for `Serialize`/`Deserialize` derives on `mdatron-core`'s public types (`Finding`, `Severity`, `Location`) so they can round-trip across the JSON envelope and the YAML inputs per DESIGN.md § Diagnostics are a versioned contract.

**Alternatives considered:**

- Hand-rolled per-type serialization: rejected — bespoke serialization for `mdatron-core`'s ~10 public types would be ~500 LoC of boilerplate vs. ~10 LoC of derives + dep
- `rkyv` / `bincode`: rejected — binary-only formats; doesn't serve mdatron's JSON/YAML text use cases

## Supply-chain notes

- **Version pin discipline:** workspace-level `serde = { version = "1", features = ["derive"] }`; resolves to latest 1.x. Pinned via `Cargo.lock`.
- **Signed releases:** crates.io does not sign releases at the per-package level; supply chain rests on crates.io infrastructure trust.
- **Maintainer trust:** dtolnay is the lead maintainer; serde is one of the most-audited Rust crates (~70k reverse dependencies).
- **Transitive deps:** serde itself has no runtime deps. `serde_derive` (proc-macro) brings `proc-macro2` + `quote` + `syn` — standard proc-macro toolkit.
- **`cargo audit`:** no known CVEs at pin time (1.0.228). Re-check at every dep bump.

## Security notes

- **CVE history:** serde itself has had no historical CVEs through 1.0.x. Two informational advisories in 2023-2024 concerned `serde_derive`'s shipped binary blob (since reverted); resolved upstream.
- **License:** MIT OR Apache-2.0; compatible with mdatron's MIT license.
- **Threat model:** deserialization-of-untrusted-data is the canonical threat; mdatron deserializes from operator-controlled config files (`.mdatron/`) + operator-authored markdown frontmatter. The threat surface is "malicious adopter content" — out of mdatron's threat model for the trusted-operator-authoring case; for adopter-PR cases, the path-confinement discipline (BOUNDARY-PREAMBLE § 7) is the load-bearing mitigation.

## Approval

- **Operator-attribution:** Solution Owner confirms `mdatron-core`'s public-API derive shape justifies the dependency. Serde is foundational for any Rust crate emitting structured output (SARIF, JSON, YAML) — making it the cleanest dependency choice rather than hand-rolling.
- **Scope justification:** `mdatron-core` derives `Serialize` + `Deserialize` on 3 public types; the dep cost is proportionate.


(The commit that originally added the dependency, June 2026, predates this investigation; this entry serves as retroactive compliance.)
