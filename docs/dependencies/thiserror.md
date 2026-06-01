# thiserror

**Status:** Approved.

**Pinned version:** `^1.0` (workspace.dependencies; resolves to 1.0.x latest within major; 2.0 available but bootstrap pinned at 1.0 for ecosystem alignment)

## Why this dependency

Provides `#[derive(Error)]` for `mdatron_core::Error` enum variants. Generates `Display` + `Error` trait implementations from struct attributes — eliminating ~30 LoC of boilerplate per error variant.

Per the Rust supplement § Software Engineer extensions: "Functions return `Result<T, E>` over panic-on-error." `thiserror` is the idiomatic way to author the `E` type.

**Alternatives considered:**

- `anyhow`: rejected for library code — `anyhow::Error` is for binary error handling (where any error type can flow). Library error types should be enums for caller pattern-matching, which is what `thiserror` enables.
- Hand-rolled `impl Error`: rejected — ~20 LoC per variant for trait impls; `thiserror` collapses to attribute macros.

## PE supply-chain notes

- **Version pin discipline:** workspace-level `thiserror = "1"`; resolves to 1.0.69. 2.0 is available; bootstrap pinned at 1.0 for compatibility with the broader Rust ecosystem (most production crates still on 1.x).
- **Maintainer trust:** dtolnay maintains; same trust posture as `serde`.
- **Transitive deps:** brings `proc-macro2` + `quote` + `syn` (proc-macro toolkit; foundational; no archived deps).
- **`cargo audit`:** no known CVEs at pin time.

## Security notes

- **CVE history:** no CVEs for `thiserror` 1.x.
- **License:** MIT OR Apache-2.0; compatible with mdatron's MIT license.
- **Threat model:** proc-macro execution at compile-time. Compile-time code execution is intentional; if the toolchain is trusted, the proc-macro is trusted. Standard Rust security model.

## SO approval

- **Operator-attribution:** Solution Owner confirms idiomatic error-type derivation is foundational for `mdatron-core`'s error surface. Migration to 2.0 is API-near-compatible; deferrable.
- **Scope justification:** Single `thiserror` derive on `mdatron_core::Error` saves ~30 LoC of boilerplate; dep cost is proportionate.

## Co-authorship attribution

Per VSDD-E0100 discipline:

```
Co-authored-by: Solution Owner <so@vsdd-domains>
Co-authored-by: Platform Engineer <pe@vsdd-domains>
Co-authored-by: Security <security@vsdd-domains>
```
