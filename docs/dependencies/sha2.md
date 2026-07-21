# sha2

**Status:** Approved.

**Pinned version:** `^0.10` (workspace.dependencies; resolves to 0.10.9 latest within minor)

## Why this dependency

Provides `Sha256` for the `mdatron init` managed-partition manifest (#74). The
manifest records each engine-deployed managed file with its sha256 content hash;
on re-run, `init` recomputes and compares to detect drift (`MDATRON-E0060`).

`DESIGN.md` already ratifies sha256 as the content-hash primitive for the PIN
family; the init manifest reuses that same primitive rather than introducing a
second hash. Standard library ships no cryptographic hash, so a crate is
required.

**Alternatives considered:**

- `ring` / `openssl`: rejected — heavyweight C/assembly crypto suites; drift
  detection needs one hash function, not a TLS-grade crypto stack. Larger
  attack surface and build-toolchain burden (C linker) for no benefit here.
- `blake3`: faster, but sha256 is the primitive `DESIGN.md` already names for
  pins; using one hash family keeps the governance surface uniform and avoids a
  second "why this hash" question.
- Hand-rolled sha256: rejected — writing a cryptographic primitive by hand is
  exactly the kind of thing that should be a vetted, audited dependency.

## PE supply-chain notes

- **Version pin discipline:** workspace-level `sha2 = "0.10"`; resolves to
  0.10.9.
- **Maintainer trust:** maintained by the RustCrypto organization — the same
  org behind `digest`, `crypto-common`, and the broader Rust crypto ecosystem.
  Widely used, actively maintained.
- **Transitive deps:** `cfg-if`, `cpufeatures`, `digest` (→ `block-buffer`,
  `crypto-common`, `generic-array`, `typenum`, `version_check`). All are
  foundational RustCrypto/ecosystem crates; no archived or unmaintained deps.
- **`cargo audit`:** clean at pin time (exit 0; no advisories against sha2
  0.10.9 or its transitive tree).

## Security notes

- **CVE history:** no CVEs for `sha2` 0.10.x.
- **License:** MIT OR Apache-2.0; compatible with mdatron's MIT license.
- **Threat model:** sha256 here is used for drift *detection* of engine-owned
  files, not as a security boundary against an adversary who controls the tree —
  the manifest itself is anchored by commit review, not by the engine
  (`DESIGN.md` § Governance data is governed). Content hashing is
  collision-irrelevant for this use (accidental edits, not forged collisions),
  so even the theoretical sha256 threat surface does not bind here. Pure-Rust
  implementation with runtime CPU-feature detection (`cpufeatures`); no C
  toolchain dependency.

## SO approval

- **Operator-attribution:** Solution Owner confirms sha256 content hashing is
  foundational for the init managed-partition governance contract; reusing the
  PIN-family primitive rather than adding a second hash family is the
  scope-minimal choice.
- **Scope justification:** one hash function for drift detection; `sha2` is the
  minimal, idiomatic RustCrypto crate for it. Dep cost is proportionate.

## Co-authorship attribution

Per VSDD-E0100 discipline:

```
Co-authored-by: Solution Owner <so@vsdd-domains>
Co-authored-by: Platform Engineer <pe@vsdd-domains>
Co-authored-by: Security <security@vsdd-domains>
```
