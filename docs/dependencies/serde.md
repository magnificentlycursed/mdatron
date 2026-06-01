# serde

**Status:** Approved (retroactive — Phase 2a added via workspace declaration before VSDD-E0100 discipline applied; this entry retroactively complies per the bootstrap-period exception declared in BOOTSTRAP-MITIGATION).

**Pinned version:** `^1.0` (workspace.dependencies; resolves to 1.0.x latest within major)

## Why this dependency

The de-facto Rust serialization framework. Used for `Serialize`/`Deserialize` derives on `mdatron-core`'s public types (`Finding`, `Severity`, `Location`) so they can round-trip across JSON / YAML / SARIF output formats per DESIGN-MDATRON § Output formats.

**Alternatives considered:**

- Hand-rolled per-type serialization: rejected — bespoke serialization for `mdatron-core`'s ~10 public types would be ~500 LoC of boilerplate vs. ~10 LoC of derives + dep
- `rkyv` / `bincode`: rejected — binary-only formats; doesn't serve mdatron's JSON/YAML/SARIF text-output use cases

## PE supply-chain notes

- **Version pin discipline:** workspace-level `serde = { version = "1", features = ["derive"] }`; resolves to latest 1.x. Pinned via `Cargo.lock`.
- **Signed releases:** crates.io does not sign releases at the per-package level; supply chain rests on crates.io infrastructure trust.
- **Maintainer trust:** dtolnay is the lead maintainer; serde is one of the most-audited Rust crates (~70k reverse dependencies).
- **Transitive deps:** serde itself has no runtime deps. `serde_derive` (proc-macro) brings `proc-macro2` + `quote` + `syn` — standard proc-macro toolkit.
- **`cargo audit`:** no known CVEs at pin time (1.0.228). Re-check at every dep bump.

## Security notes

- **CVE history:** serde itself has had no historical CVEs through 1.0.x. Two informational advisories in 2023-2024 concerned `serde_derive`'s shipped binary blob (since reverted); resolved upstream.
- **License:** MIT OR Apache-2.0; compatible with mdatron's MIT license.
- **Threat model:** deserialization-of-untrusted-data is the canonical threat; mdatron deserializes from operator-controlled config files (`.mdatron/`) + operator-authored markdown frontmatter. The threat surface is "malicious adopter content" — out of mdatron's threat model for the trusted-operator-authoring case; for adopter-PR cases, the path-confinement discipline (BOUNDARY-PREAMBLE § 7) is the load-bearing mitigation.

## SO approval

- **Operator-attribution:** Solution Owner confirms `mdatron-core`'s public-API derive shape justifies the dependency. Serde is foundational for any Rust crate emitting structured output (SARIF, JSON, YAML) — making it the cleanest dependency choice rather than hand-rolling.
- **Scope justification:** `mdatron-core` derives `Serialize` + `Deserialize` on 3 public types; the dep cost is proportionate.

## Co-authorship attribution

Per VSDD-E0100 discipline, the originating commit should carry:

```
Co-authored-by: Solution Owner <so@vsdd-domains>
Co-authored-by: Platform Engineer <pe@vsdd-domains>
Co-authored-by: Security <security@vsdd-domains>
```

(The Phase 2a commit that originally added the dep predates this investigation; this entry serves as retroactive compliance.)
