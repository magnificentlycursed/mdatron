# pulldown-cmark

**Status:** Approved. Adopted for the link-check family (#155, vsdd GH#28 — the lychee reference-arch review).

**Pinned version:** `^0.13.4` (resolves to 0.13.4) with `default-features = false` — the default `getopts`/`html` features exist only for the crate's bundled `pulldown-cmark` example binary (an arg-parsing CLI) and its HTML renderer, neither of which the link family builds. Dropping them removes three crates from the tree (`getopts`, `unicode-width`, `pulldown-cmark-escape`) and leaves the full event-parser API (`Parser`, `Event`, `Tag`, `TagEnd`, `LinkType`) intact — those types live in `lib.rs` unconditionally; only the `html` rendering module is feature-gated.

## Why this dependency

The link-check family (#155) has to recognise CommonMark link constructs to find and resolve link targets — and getting this right requires a real CommonMark event stream, not a scanner:

- **Reference-style links** `[text][ref]` and their separate `[ref]: url` definitions — a two-part construct a regex line-scanner cannot correctly pair.
- **Image links** `![alt](src)` and reference images `![alt][ref]`.
- **Inline-code and code-fence masking** — `` `[not a link](x)` `` inside a code span must NOT be treated as a link. A hand-rolled scanner that does not model code spans silently reports false negatives (misses real links) or false positives (flags text inside code).

Markdown is a declared **trust boundary** in this project. DESIGN.md's threat model lists `untrusted-markdown (parsers are a trust boundary)` among its surfaces. A widely-used, CommonMark-conformant, `forbid(unsafe_code)` parser is *more* correct and *more* hardened than a bespoke scanner: the parser is exactly the boundary-hardening artifact the trust-boundary designation calls for, and it converges on the same grammar every other CommonMark tool uses rather than inventing a private dialect whose disagreements become silent conformance gaps.

## Alternatives considered

- **`comrak`:** a CommonMark + GFM-superset parser. Heavier: it targets the GFM extension set (tables, task lists, autolinks, strikethrough) that the link family does not need, and it pulls a materially larger transitive tree (its own `entities`, `typed-arena`, `derive_builder`/`syn` machinery, plus `unicode-*` tables). It builds a full owned AST rather than streaming events. Rejected as over-scoped for link extraction; revisit only if a future family genuinely needs the GFM superset.
- **`markdown-rs` (a.k.a. `markdown` 1.x):** a capable pure-Rust CommonMark/MDX parser with an AST API. Reasonable, but less battle-tested at scale than pulldown-cmark and oriented toward AST/MDX construction rather than a minimal-allocation event stream. pulldown-cmark's streaming pull model (an `Iterator` of events, minimal allocation and copying) is a better fit for a linear pass that only harvests link/image tags, and its 127M-download install base makes it the lower-risk supply-chain choice.
- **Hand-rolled scanning (regex/byte scanner):** rejected. To correctly mask code spans/fences, pair reference definitions, and handle escapes and nested brackets, a scanner has to re-derive a CommonMark parser — badly. That re-derivation *is* the false-negative source this dependency exists to eliminate, and it would be its own falsifiability burden (every CommonMark edge case becomes a bespoke test we own). Note the project already refuses this pattern elsewhere (regex-lite's record rejects a bespoke matcher dialect for the same reason).

## PE supply-chain notes

- **Version pin discipline:** `pulldown-cmark = { version = "0.13.4", default-features = false }` → resolves to 0.13.4 (latest stable, published 2026-05-20; 55 published versions, active release cadence). `default-features = false` is the deliberate minimal-surface choice (see the pin note above).
- **Maintainer trust:** the `pulldown-cmark` GitHub org (`github.com/raphlinus/pulldown-cmark`). Original author **Raph Levien** (`raphlinus`); since 2023 driven by Martín Pozo, Michael Howell, Roope Salmi, and Martin Geisler. crates.io owners: `raphlinus`, `marcusklaas`, `Martin1887`. A community org (not corporate-owned), and one of the most-depended-upon text-processing crates in the ecosystem: **127.7M all-time downloads, ~38.5M recent** — it is the CommonMark engine behind mdBook, docs.rs / rustdoc, and much of the Rust documentation toolchain.
- **Transitive deps — the real observed tree.** As added by `cargo add` (default features) it locked **5** new crates and shared 2 already present. Under our approved `default-features = false` the tree is minimal:

  With `default-features = false` (our config), the only genuinely-new crates are:
  - `pulldown-cmark` 0.13.4 (MIT)
  - `unicase` 2.9.0 (MIT OR Apache-2.0)
  - (`bitflags` 2.13.1 and `memchr` 2.8.3 appear under it in `cargo tree` but were **already** in the tree via clap/jsonschema/regex — no new crates)

  The default features would additionally have pulled `getopts` 0.2.24 (MIT OR Apache-2.0), `unicode-width` 0.2.2 (MIT OR Apache-2.0), and `pulldown-cmark-escape` 0.11.0 (MIT) — all dropped by `default-features = false`. Verified: `cargo audit` scans 145 crate deps with the trimmed config vs. 148 with defaults.
- **`cargo audit`:** clean at pin time — 145 dependencies scanned, 0 vulnerabilities; no RUSTSEC advisory file exists for `pulldown-cmark`.
- **`cargo deny check bans licenses sources advisories`:** all four pass (`advisories ok, bans ok, licenses ok, sources ok`). Every license in the added subtree (MIT / MIT OR Apache-2.0) is already in `deny.toml`'s allow-list; nothing is pulled from an unknown registry or git source.
- **`cargo build`:** compiles cleanly in this project (mdatron v0.6.0, `dev` profile Finished).

## Security notes

- **CVE / RUSTSEC history:** none. `cargo audit` is clean and the RustSec advisory-db contains no advisory for `pulldown-cmark` (or for `unicase`).
- **License:** `pulldown-cmark` is **MIT**, compatible with mdatron's MIT. Transitive licenses in the subtree: `unicase` MIT OR Apache-2.0; (default-only, not adopted) `pulldown-cmark-escape` MIT, `getopts` / `unicode-width` MIT OR Apache-2.0; shared `bitflags` MIT OR Apache-2.0, `memchr` Unlicense OR MIT. All permissive and allow-listed by `deny.toml`.
- **`unsafe`:** the crate declares `#![cfg_attr(not(feature = "simd"), forbid(unsafe_code))]` — `unsafe` is *forbidden by the compiler* unless the opt-in `simd` feature is enabled. We do **not** enable `simd` (it is not in default features, and we build `default-features = false`), so in mdatron's build pulldown-cmark contains **zero** `unsafe`. The only `unsafe` blocks in the crate are the SIMD fast-path in `firstpass.rs`, which is compiled out for us.
- **MSRV:** `rust-version = 1.71.1`, comfortably below mdatron's toolchain — no floor conflict.
- **Threat model — untrusted markdown, and why it fits DESIGN L17.** The markdown under check is untrusted (contributor- and PR-authored). pulldown-cmark is a **streaming pull parser** — an `Iterator` of events with a bare minimum of allocation and copying — applying the **fixed CommonMark grammar**. This matters for the linear-time discipline in two ways:
  - DESIGN L17's ReDoS clause governs *adopter-supplied pattern engines* (route/vocabulary regexes, JSON Schema `pattern`) — user-authored patterns that a backtracking engine could blow up on. A CommonMark parser applies **engine-controlled structure**, not an adopter-authored pattern, so the "adopter crafts a pathological regex" ReDoS class is structurally out of scope for it.
  - It is nonetheless designed for linear-ish throughput (it is the parser chosen precisely for large-document toolchains like mdBook/docs.rs, and avoids the naive-CommonMark superlinear blowups). Combined with mdatron's existing per-file byte bound (`MAX_FILE_BYTES`) and aggregate-snapshot bound, a bounded input is a bounded parse — there is no unbounded-work surface introduced.
  - Adopting a hardened, CommonMark-conformant parser *is* the trust-boundary hardening the DESIGN "parsers are a trust boundary" designation asks for: it retires bespoke markdown scanning (whose edge-case gaps are silent-false-negative security/correctness bugs) in favour of the ecosystem-standard grammar implementation.

## SO approval

- **Operator-attribution:** the operator ratified adopting `pulldown-cmark` for #155 (vsdd GH#28 — the lychee reference-arch review) on **2026-08-02**.
- **Scope justification:** one streaming parser, adopted `default-features = false` (2 new crates, both permissively licensed and already-audited-clean), retiring the accreting hand-rolled markdown scanners in the link-check family. Proportionate: it replaces bespoke parsing whose false-negative surface is a correctness *and* trust-boundary liability with the ecosystem-standard CommonMark engine.

## Co-authorship attribution

Per VSDD-E0100 discipline:

```
Co-authored-by: Solution Owner <so@vsdd-domains>
Co-authored-by: Platform Engineer <pe@vsdd-domains>
Co-authored-by: Security <security@vsdd-domains>
```
