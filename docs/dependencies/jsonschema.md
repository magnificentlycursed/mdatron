# jsonschema

**Status:** Approved. Upgraded `0.18 → 0.49` (#135, roast C1 — security-driven).

**Pinned version:** `^0.49` with `default-features = false` (drafts are built-in in 0.49; Draft 2020-12 is selected at build via `.with_draft(Draft::Draft202012)`; `default-features = false` strips the `resolve-http`/`resolve-file` remote-reference resolvers we must not enable).

## 0.18 → 0.49 upgrade (#135, roast C1)

**Why (security):** The pre-publish adversarial roast found that 0.18 validated adopter-authored `pattern`/`patternProperties` regexes through the **backtracking** `fancy-regex` engine with no mdatron step/time budget — a ReDoS surface on the verify gate, and a contradiction of DESIGN.md's "linear-time engines" guarantee. 0.49 adds `PatternOptions::regex()`, which routes pattern matching through the **linear-time `regex` crate** (guaranteed O(n), no catastrophic backtracking). We now build with `.with_pattern_options(PatternOptions::regex())`. Trade-off: the linear engine rejects look-around/backreferences (fancy-regex-only features) — refused at schema compile — which frontmatter shape-validation does not need. Proven by `schema.rs` tests `linear_engine_refuses_backreference_pattern` and `catastrophic_pattern_matches_in_linear_time`.

**API migration (0.18 → 0.49):** `JSONSchema` → `Validator`; `JSONSchema::options().compile()` → `jsonschema::options().build()`; `validate()` (all errors) → `iter_errors()`; `ValidationError` accessors are now methods (`instance_path()`/`schema_path()`/`instance()`/`kind()`). Purely mechanical; the full test suite passed unchanged after migration.

**Supply-chain delta (PE + Security):** the 0.49 modularization expands the transitive tree by ~18 crates (notably `referencing`, `jsonschema-regex`, `jsonschema-value`, `regex`, `strum`, `fluent-uri`, `email_address`, `uuid-simd`/`vsimd`/`outref`, `unicode-general-category`, `foldhash`). `fancy-regex` remains linked (the non-default engine) but is never invoked for our patterns. All are reputable, MIT/Apache-2.0-compatible crates from the jsonschema author's own workspace or the rust-lang ecosystem; the expanded license surface is gated by the `cargo-deny` license/bans check added under roast C4 (a follow-up hardening lane).

## Why this dependency

Layer 1 structural validation per DESIGN.md § Five check families requires JSON Schema draft 2020-12 compliance. Hand-rolling a draft-2020-12-complete validator would be ~5000 LoC; this dep is the canonical Rust choice.

**Alternatives considered:**

- `valico`: less actively maintained; supports draft-06/07 only
- `schemars` validation: `schemars` generates schemas from Rust types (inverse direction); does not provide a validator
- Hand-rolled: rejected (~5000 LoC; reinventing well-trodden ground)

## PE supply-chain notes

- Pinned at `^0.49` with `default-features = false` (drafts built-in; Draft 2020-12 selected at build; remote resolvers off)
- Maintainer: Dmitry Dygalo (Stranger6667); active maintenance; widely adopted
- Transitive deps: the 0.49 workspace footprint (`referencing`, `jsonschema-regex`, `jsonschema-value`, `regex`, `fancy-regex`, `fluent-uri`, `strum`, `email_address`, `uuid-simd`, `unicode-general-category`, `foldhash`, …) — ~18 crates larger than the 0.18 tree (see the upgrade section above)
- MSRV: still 1.88 (matches `rust-toolchain.toml`)
- License: MIT; compatible with mdatron's MIT (the expanded tree is license-gated by the C4 `cargo-deny` lane)

## Security notes

- **Pattern ReDoS (roast C1, the driver for this upgrade):** adopter `pattern`s now run on the linear-time `regex` engine (`PatternOptions::regex()`), not backtracking `fancy-regex` — no catastrophic backtracking on contributor-supplied frontmatter
- CVE history: jsonschema 0.17 had RUSTSEC-2024-0386 (recursion DoS); fixed in 0.18+, carried through 0.49
- Threat model: validates operator-controlled schemas against contributor-supplied frontmatter (hence the ReDoS concern that drove the linear-engine move)
- Path-confinement discipline (BOUNDARY-PREAMBLE § 7) prevents PR-injected schemas from being loaded
- `default-features = false` keeps the `resolve-http`/`resolve-file` remote-reference fetchers OFF (no network on the verify gate)

## Operator approval

Layer 1 JSON Schema validation is foundational for mdatron's primary purpose. jsonschema crate is the established Rust choice. Cost is proportionate.

## Attribution

Single-agent (Opus 4.7) authoring per the attribution-honesty discipline (crosslink issue #6 in vsdd-cli filing). No fake co-author trailers; this entry is the actual investigation.
