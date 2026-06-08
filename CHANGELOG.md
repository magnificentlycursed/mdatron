# Changelog

All notable changes to mdatron will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

mdatron is descended from Schematron (ISO/IEC 19757-3). It is **not** related to
the TRON blockchain.

## [Unreleased]

### Added

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
