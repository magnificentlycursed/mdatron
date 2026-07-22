# Changelog

All notable changes to mdatron will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

mdatron is descended from Schematron (ISO/IEC 19757-3). It is **not** related to
the TRON blockchain.

## [Unreleased]

### Fixed
- test(confine): `confine_ops_do_not_leak_fds` was flaky under parallel execution — it took a single `/proc/self/fd` snapshot (process-global, shared across threads) and tolerated only +2, so descriptors briefly held by unrelated concurrent tests failed it despite zero leak. Now min-samples the count to filter transient churn and tolerates bounded parallel noise (32) while staying an order of magnitude below the per-op leak signal (300); a deterministic red-gate test reproduces the race (#75)
- Path-confinement helper: fallback and starts_with flaws; traversal test passes only via macOS symlink (L2)

### Security
- index: complete the diagnostic marking discipline at the extract/parse stage — Parse/Selection/UnsupportedFileType/read-Io messages now escape adopter path text (control bytes render as inert `\xNN`), consistent with the confinement-stage variants (#67)
- index: glob enumeration semantics unpinned — closed-world discipline not enforced or tested; replaced glob::glob with an engine-owned no-follow bounded walk (absorbs #54: glob metacharacters in the project-root path parsed as pattern syntax, and non-UTF8 roots degraded before matching) (#55)
- confine: open_confined silently sanitizes non-Normal components instead of rejecting them (#53)
- Competent-demotion refusal asserts an engine signal the engine cannot emit (L41)
- Demotion falsification and seed conflate pin drift with annotation refusal (L39)
- Demotion annotation has no specified home (L38)
- Competent manifest demotions are silent; the loud bar fails for removals (L37)
- Seeds exercise only the split leg (L36)
- Completeness lemma false: consumers split on FS/GS/RS (escape set) (L35)
- Budget scope over dependent-closure traversal ambiguous (L25)
- Partition demotions escape the loud-governance channels (L24)
- Manifest self-hash is a fixed point; the anchor must be stated (L23)
- Compact truncation vs marking invariant unspecified (L22)
- Escaping and line-splitting cannot share one alphabet; a partition is required (L21)
- Handle confinement component-wise unstated (L11)
- Governance weakening is grep-loud only (L10)
- Init manifest self-pinning unstated (L9)
- Annotation owner unauthenticated; annotation text unmarked channel (L8)
- Extras scan bound dimension unnamed; no real-tree fixture (L7)
- Aggregate snapshot memory and concurrent count unbounded (L6)
- Escaping and line-splitting alphabet excludes Zl/Zp separators (L5)
- Inline interpolation does not fit block prefix marking (L4)

### Added
- diagnostics: `MDATRON-E0050` frontmatter-schema-violation now reports the violation's precise source line (`--> file:line`) instead of the frontmatter block start, resolving the schema JSON pointer via an error-path-only position-tracking parse — directly actionable for the fixing agent (#65)
- CI: GitHub Actions gauntlet — fmt + clippy (`-D warnings`), workspace tests on Linux, macOS, and Windows, and `cargo audit`, all on the pinned MSRV toolchain (crosslink/Thermite-shaped) (#68, #64)
- docs(README): pre-commit integration guide recommending a fail-closed wrapper (block the commit when the `mdatron` binary is missing, rather than skip silently), from the first external-adopter field report (#60)
- Design increment: conformance-check families (phase 1a cycle) (#1)

- `mdatron explain CODE` v0.1.0 baseline catalog — five per-code prose pages
  embedded via `include_str!` (MDATRON-E0001, E0002, E0050, E0070, E0080); each
  page carries the four required structural elements (`**Severity:**`,
  `**Introduced in:**`, `## What this means`, `## How to fix`) per
  crosslink #13 Phase 1a behavioral spec
- TTY `= explain: mdatron explain <code>` line in rustc-shape diagnostic
  output when the finding's `explain_ref` is `Some` — surfaces the explain
  subcommand affordance at the per-finding diagnostic
- `mdatron/README.md` — first-author README per DR-F1: install + first run
  + Layer 1 schema example + Layer 2 pattern example + relationship to vsdd
  + where to go next; TRON-blockchain disambiguation in section 1 per TW-F3
- `mdatron-cli/tests/cli_integration.rs` — 18 CLI integration tests
  covering the `explain` subcommand (5 baseline + unknown + foreign-
  namespace), the rendered `= explain:` line, README presence + structure
  + round-trip, and `--quiet --json` flag-combination drive-by

### Changed
- docs: the E0012 explain page and DESIGN.md § Verification is fast qualify the swap-proof confinement guarantee as Unix-only (`openat`/`O_NOFOLLOW`); the non-unix fallback's weaker check-to-open window is stated as a tolerated, carved-out posture until a handle-based walk lands (#56)
- ops: repair the hollow chassis install (crosslink init --force) (#58)
- mdatron cleanup: truth reconciliation + chassis onboarding (layer 0, pulled forward) (#2)
- Spec-review round 7 (conformance-engine rev 7, confirmation: the claim reduction) (L7)
- Spec-review round 6 (conformance-engine rev 6, final: security, two paragraphs) (L6)
- Spec-review round 5 (conformance-engine rev 5, final targeted: security) (L5)
- Spec-review round 4 (conformance-engine rev 4, targeted) (L4)
- Spec-review round 3 (conformance-engine rev 3) (L3)
- Revision line and evidence base stale (L33)
- Compact output not pinned to the verify command (L32)
- Reverse rule-reference direction unfixtured (L31)
- Trace-escaping falsification unowned by any criterion (L30)
- Mutation fixtures need a declared test seam (L29)
- Weakening lint universality contradicts incremental soundness (L28)
- Alphabet seeds omit Zp (U+2029) (L27)
- Concurrent-invocation-count bound has no exceedance seed (L26)
- Fixture-corpus completeness clause is partly circular and unaudited (L19)
- Agnosticism scope word documentation unqualified (L18)
- Normal-case performance rests on measurement without threshold (L17)
- Milestone conversion criterion omission is silent (L16)
- Governance-edge dependent propagation unfixtured; reverse direction unexercised (L15)
- Conflicting-data precedence outcomes unasserted (L14)
- Concurrency criterion passes trivially without in-flight mutation (L13)
- Snapshot property has no mutation-in-flight falsification (L12)

- `Finding::format_tty` consolidated to the rustc shape: first line is
  `<label>[<code>]: <summary>` (was `: <message>`); `<message>` now flows
  through a `   = note: <message>` line. The rendered output is closer
  to rustc / clippy convention. `mdatron-cli/src/main.rs:print_finding`
  delegates to `format_tty` as single source of truth (Phase 2c
  consolidation per crosslink #13)
- `cmd_explain`'s not-found message now points to DESIGN-MDATRON.md §
  Reserved mdatron codes for the structural meaning of unimplemented
  codes (crosslink #13 Phase 4 Convergence 6: SC F1 + UX F4 + AIE F6
  + DR F4)

### Methodology notes

- Phase 2 of the binary-first refactor honors the SO disposition recorded
  2026-06-02 at `vsdd-cli/docs/refactor/phase-0-output-format/DESIGN.md` open
  question #2: implement explain for v0.1.0; retain the `= explain:` line.
  This reverses DR-F2's earlier "strip the line" finding
- `binary-first-plan.md` row 14 amended in this cycle to reflect the
  disposition reversal (was "Strip"; now "Implement explain catalog +
  retain line")
- `OperatorDirectiveApplied{directive: phase-2-explain-disposition-honored}`
  captured at this commit per the M5 F3+F9 audit-trail-on-act discipline;
  prose record at `vsdd-cli/docs/refactor/phase-2-mdatron-json/phase-1a-
  behavioral-spec.md § Operator-directive housekeeping`
- crosslink #13 cluster-batched Phase 3 cold-session: 18 reviews
  (`review-log/2026-06-07-<domain>-phase-2-bundle.md`); ~137
  findings; 0 hallucinations; 4 honest domain-wide dismissals
  (Accessibility, Localization, Privacy, Performance Engineer-mostly)

## [0.0.1] - 2026-06-01

### Added

- Cargo workspace structure with `mdatron-core` library + `mdatron` binary crates per [BOUNDARY-PREAMBLE](./BOUNDARY-PREAMBLE.md) § 2
- `mdatron-core::diagnostic` — `Finding`, `Severity`, `Location` types with rustc-style TTY formatting (`format_tty` matches `severity[code]: message\n  --> file:line[:column]` convention; optional `= help:` and `= explain:` lines)
- `mdatron-core::error` — internal `Error` enum (IO / YAML / JSON / Config variants) via `thiserror`
- `mdatron-core::frontmatter` — YAML frontmatter parser handling typical, empty (`---\n---\n`), malformed-YAML, no-closing-marker, and body-internal-dashes cases
- `mdatron verify --files <FILE>...` — CLI subcommand parsing frontmatter from given files; reports per-file count + rustc-style diagnostics on failure
- `mdatron explain <code>` — stub (returns "extended docs not yet implemented at v0.1.0")
- `rust-toolchain.toml` pinning channel 1.85 + `clippy` + `rustfmt` components per Rust supplement Platform Engineer extensions
- `rustfmt.toml` declaring `edition = "2021"` + `max_width = 100`
- `docs/dependencies/{serde,serde_yaml,serde_json,thiserror,clap}.md` — retroactive VSDD-E0100 investigation entries for the 5 workspace deps
- `.vsdd/events/` bootstrap-period audit-trail entries for Phase 2a + Phase 2b composition declarations (canonical event-log analog per BOOTSTRAP-MITIGATION § Mitigation 3)
- AIE domain review on mitigation techniques at [`review-log/2026-06-01-ai-engineer.md`](./review-log/2026-06-01-ai-engineer.md)

### Methodology notes

- Phase 2a Red Gate authored before implementation (14 failing tests + 1 passing legitimate test); Phase 2b implementation closes all 14 (Red → Green)
- M1 inline-multi-domain composition recurrence #7 acknowledged; vsdd-cli Step 2.3 amendment with cost-rubric is the load-bearing resolution path (AIE-MITIG-F6)
- AIE-S3-F1 (compact output format with pinned token budget) deferred to v0.0.x once compact format is implemented
- AIE-MITIG-F4 commit-message convention for sub-agent cost capture: applied prospectively from Phase 2a commit onward
- Bootstrap-period state: `mdatron-core verify` not yet operational against this repo; structural drift caught by `bootstrap-tooling/bootstrap-validator.py` per BOOTSTRAP-MITIGATION § Mitigation 1

### Deferred

- Layer 1 JSON Schema validation (Phase A.1 next iteration) — needs `jsonschema` dep approval
- Layer 2 DSL parser + evaluator + standard library (Phase A.1)
- 5 built-in patterns (Phase A.3)
- Error catalog loader + ~30 seeded codes (Phase A.2)
- SARIF / JSON / compact output formats (Phase B)
- Cross-file index lifecycle (Phase A.1)
- Anchor-ID derivation utility (Phase A.1)
- Bypass-marker parsing (Phase A.1)
- `mdatron registry`, `mdatron init`, `mdatron schema`, `mdatron skill-preamble`, `mdatron verify hook`, `mdatron verify test-error-catalog` subcommands (Phase B+)
- PostToolUse hook configuration template (Phase C)
- `mdatron lsp-serve`, `mdatron mcp-serve` — namespaces reserved; implementations in v1.1 per [V1-SHIP-CRITERIA](./V1-SHIP-CRITERIA.md)
