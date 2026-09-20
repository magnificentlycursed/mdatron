# proptest

**Status:** Approved (dev-dependency only — never in the shipped binary).

**Pinned version:** `^1` (resolves to 1.11.0), `default-features = false`,
`features = ["std"]`

## Why this dependency

The parser-robustness harness (`tests/parser_robustness.rs`, added 2026-09-19
after a pre-release adversarial review). DESIGN declares the input parsers a
trust boundary, and that review's two parser blockers — a char-boundary panic and an
unguarded-recursion abort in the DSL expression parser — reached a green
release candidate because no test fed the parsers malformed input. proptest
drives every input parser (expression parse + evaluate, pattern-file YAML,
frontmatter, JSON-Schema compile, the four `.mdatron/` loaders) with hostile
generated input under the never-panic property, bounded per PR and env-tunable
(`PROPTEST_CASES`, `PROPTEST_RNG_SEED`) for the deep on-demand profile.

`default-features = false` + `std` deliberately drops the `fork`/`timeout`
runners (`rusty-fork`, `wait-timeout`, `tempfile`): an abort inside a property
must fail the harness loudly, not be absorbed by a forked child — the harness
exists to make aborts visible.

**Alternatives considered:**

- `cargo-fuzz` / libFuzzer: coverage-guided byte mutation is strictly stronger
  at discovery, but requires a NIGHTLY toolchain — the whole CI gauntlet is
  pinned to the 1.88 MSRV (`rust-toolchain.toml`, every job `--locked`), and a
  nightly fuzz crate must live outside the workspace with its own lockfile
  posture, invisible to this allowlist audit and rotting unbuilt between
  campaigns. Rejected for the per-PR gate; tracked as the on-demand deep-fuzz
  follow-up if property testing proves insufficient.
- `quickcheck`: lighter, but weaker strategy combinators (no token-mix or
  recursive-value strategies of the shape these parsers need) and less
  maintained; proptest is the de-facto successor.
- Hand-rolled generator loop: rejected — shrinking on failure (proptest's core
  value: a minimal counterexample instead of a 500-byte blob) is exactly what
  a parser-crash triage needs, and a bespoke harness would be its own
  falsifiability burden.

## Supply-chain notes

- **Version pin discipline:** `proptest = "1"` → 1.11.0, `Cargo.lock`
  committed, every CI job `--locked`.
- **Maintainer trust:** the proptest-rs org (originally AltSysrq/Jason
  Lingle); the standard Rust property-testing crate.
- **Transitive deps (with the trimmed feature set), from `Cargo.lock` ground
  truth:** direct — `bitflags`, `num-traits`, `rand`, `rand_chacha`,
  `rand_xorshift`, `regex-syntax`, `unarray`; via the rand family —
  `rand_core`, `ppv-lite86`, `getrandom`. The adoption added exactly **seven
  new packages** to the lock (`proptest`, `rand`, `rand_chacha`, `rand_core`,
  `rand_xorshift`, `ppv-lite86`, `unarray`); `bitflags`, `num-traits`,
  `regex-syntax`, and `getrandom` were already in the tree via existing
  dependencies. The disabled `bit-set` feature keeps `bit-set`/`bit-vec` out
  entirely. All dual MIT/Apache-2.0, all inside the `deny.toml` license
  allowlist, all crates.io. cargo-deny's graph includes dev-dependencies, so
  the tree stays under the bans/licenses/sources gate — the mechanical gate
  over the REAL graph; this prose section is descriptive and is a generation
  candidate (the same invertible-edge shape as the limits table).
- **`cargo audit`:** clean at pin time.
- **MSRV:** well under the pinned 1.88; runs under plain `cargo test` on the
  existing matrix (the decisive advantage over libFuzzer here).

## Security notes

- **CVE history:** none for proptest.
- **License:** MIT OR Apache-2.0; compatible.
- **Threat model:** dev-dependency only — zero bytes of it ship in the
  `mdatron` binary or the crates.io package (`cargo package` excludes
  dev-deps' code by construction). Its randomness is test-input generation,
  not production behavior; CI pins `PROPTEST_RNG_SEED` for a deterministic
  bounded gate.

## Approval

- **Operator-attribution:** operator directive 2026-09-19, closing the
  pre-release review's "no fuzz harness and no CI parser-robustness gate"
  finding.
- **Scope justification:** one dev-only crate closing the declared-trust-
  boundary/no-hostile-input gap that let two parser aborts reach a green RC;
  proportionate.
