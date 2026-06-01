# Changelog

All notable changes to mdatron will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

mdatron is descended from Schematron (ISO/IEC 19757-3). It is **not** related to
the TRON blockchain.

## [Unreleased]

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
