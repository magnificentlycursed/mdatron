# jsonschema

**Status:** Approved.

**Pinned version:** `^0.18` with `features = ["draft202012"]` + `default-features = false`

## Why this dependency

Layer 1 structural validation per DESIGN-MDATRON § Two-layer architecture requires JSON Schema draft 2020-12 compliance. Hand-rolling a draft-2020-12-complete validator would be ~5000 LoC; this dep is the canonical Rust choice.

**Alternatives considered:**

- `valico`: less actively maintained; supports draft-06/07 only
- `schemars` validation: `schemars` generates schemas from Rust types (inverse direction); does not provide a validator
- Hand-rolled: rejected (~5000 LoC; reinventing well-trodden ground)

## PE supply-chain notes

- Pinned at `^0.18` with the `draft202012` feature enabled (Draft202012 is gated behind that feature in 0.18.x)
- `default-features = false` strips optional features we don't use
- Maintainer: Dmitry Dygalo (Stranger6667); active maintenance; widely adopted
- Transitive deps: brings `fancy-regex`, `regex`, `url`, `percent-encoding`, `ahash`, `base64`, `idna_adapter`, `time` (~30 transitive deps; standard validator footprint)
- `time` and `idna_adapter` pull in MSRV 1.88 — this drove our `rust-toolchain.toml` bump from 1.85 to 1.88
- License: MIT; compatible with mdatron's MIT

## Security notes

- CVE history: jsonschema 0.17 had RUSTSEC-2024-0386 (recursion DoS); fixed in 0.18+
- Pin at 0.18 ensures the fix is present
- Threat model: validates operator-controlled schemas against operator-controlled frontmatter
- Path-confinement discipline (BOUNDARY-PREAMBLE § 7) prevents PR-injected schemas from being loaded

## Operator approval

Layer 1 JSON Schema validation is foundational for mdatron's primary purpose. jsonschema crate is the established Rust choice. Cost is proportionate.

## Attribution

Single-agent (Opus 4.7) authoring per the attribution-honesty discipline (crosslink issue #6 in vsdd-cli filing). No fake co-author trailers; this entry is the actual investigation.
