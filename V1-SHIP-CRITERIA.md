# mdatron v1 Ship Criteria

**Status:** Step 0.5 artifact. SO answer to SO-F2 from the 2026-06-01 multi-domain review.

**Provenance:** Closes the v1 ship-criterion deferred decision named in [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md) § "What this preamble does NOT decide" and required by [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md) § "Acceptance criteria for closing Step 0.5".

**Decision date:** 2026-06-01.

---

## Decision

**v1.0 cut:** mdatron ships as a CLI with agent-loop integration. LSP server and MCP server are reserved namespaces but defer to v1.1.

| Phase | Includes | v1.0? |
|---|---|---|
| A — Foundational | `mdatron-core` engine; DSL parser + evaluator; schema dispatch; cross-file index; 5 built-in patterns; ~30 seeded error codes; falsifiability fixtures | **Yes** |
| B — CLI surface | `mdatron-cli` subcommand dispatch; `mdatron verify` with 4 output formats (TTY, sarif, json, compact); `mdatron explain <code>`; `mdatron verify test-error-catalog` | **Yes** |
| C — Agent integration | PostToolUse hook configuration (deployed by `vsdd init` into `.claude/settings.json`); compact output format optimized for LLM context; registry CLI subcommands (`mdatron registry list/get/diff/validate`); `mdatron skill-preamble` subcommand + adopter-template text | **Yes** |
| D — MCP surface | `mdatron mcp-serve` + 5 structured tools (verify, explain, schema, registry, cross_refs) | **No — v1.1** |
| E — Human integration | `mdatron lsp-serve` LSP server; IDE extension manifests (VS Code, Neovim) | **No — v1.1** |
| F — Onboarding | `mdatron init` adopter companion (deploys `.mdatron/` skeleton); pre-flight check | **Yes** |
| Always-alongside | Release pipeline (cosign + SLSA + per-platform binaries × 4 platforms); documentation (DESIGN-MDATRON; README; DSL reference; adopter onboarding; 2 worked examples — one VSDD-style, one ADR-style) | **Yes** |

## Rationale for the cut

The cut prioritizes **agent-loop integration over editor-LSP integration** because:

1. **VSDD's primary authoring path is agents, not humans-in-editors.** The PostToolUse hook reaches the agent during composition; LSP reaches the human during review. The hook is load-bearing for VSDD's actual usage pattern (per AIE-F1).
2. **The LSP server is the heaviest single workstream** (~2000-5000 LoC + per-IDE configuration). Cutting it from v1.0 reduces operator-time by an estimated 30-40% across mdatron development.
3. **MCP follows similar logic** — depths 1 + 2 of skill integration (preamble + registry CLI queries per AIE-F3) cover the agent-query case without MCP. Depth 3 (MCP tools) is the v1.1 maturation.
4. **Reserved-but-deferred preserves design coherence.** The `mdatron-lsp` namespace (per BOUNDARY-PREAMBLE § 2) and `mdatron mcp-serve` subcommand name are reserved at v1.0; v1.1 lands the implementations without breaking compatibility.
5. **The Phase 3 attention-reallocation discipline works at v1.0.** Mechanical drift is caught at write-time via the hook; skill-integration depths 1 + 2 enable reviewers to focus on judgment-bearing concerns. The discipline matures at v1.1, but it does NOT require LSP or MCP to function.

## v1.0 acceptance criteria

v1.0 declared ready when ALL of:

1. **All Phase A deliverables shipping**:
   - `mdatron-core` library published to crates.io
   - DSL parser + evaluator pass all unit tests
   - 5 built-in patterns implemented + have falsifiability fixtures
   - ~30 error codes in catalog with explain pages + per-code fixture pairs

2. **All Phase B deliverables shipping**:
   - `mdatron-cli` binary published to crates.io
   - `cargo install mdatron` succeeds on Linux x64/arm64 + macOS x64/arm64
   - `mdatron verify`, `mdatron explain`, `mdatron verify test-error-catalog` all functional
   - 4 output formats produce valid output (TTY rustc-style; SARIF 2.1.0; JSON; compact)

3. **All Phase C deliverables shipping**:
   - PostToolUse hook configuration emitted by `vsdd init` (validated by manual integration test with Claude Code)
   - Compact output format reviewed for LLM context-window efficiency
   - `mdatron registry list/get/diff/validate` operational against vsdd-cli's seed registries
   - `mdatron skill-preamble` outputs adopter-template text

4. **All Phase F deliverables shipping**:
   - `mdatron init` deploys `.mdatron/` skeleton with sensible defaults
   - Pre-flight check detects missing prerequisites (git repo present; mdatron version compatible; etc.)

5. **Release infrastructure operational**:
   - Per-platform binaries (Linux x64/arm64 + macOS x64/arm64) signed via cosign + sigstore
   - SLSA provenance generated per build
   - `Cargo.lock` committed; reproducible builds verified
   - GitHub Releases pipeline operational

6. **Documentation complete**:
   - `DESIGN-MDATRON.md` authored (substantive design beyond this preamble)
   - `README.md` with first-sentence Schematron-derived-not-TRON-blockchain disambiguation (per TW-F3)
   - DSL reference page (operator-grade documentation)
   - Adopter onboarding guide
   - 2 worked examples: one VSDD-style + one non-VSDD (suggested: an ADR repo or RFC repo)

7. **Falsifiability test passed**:
   - The cold-context DSL falsifiability check (per [`BOOTSTRAP-MITIGATION.md`](./BOOTSTRAP-MITIGATION.md) § Mitigation 4) run; ≥80% pass-rate achieved; report at `mdatron/dsl-falsifiability-report.md`

8. **vsdd-cli Step 5 release coupling complete** (links bootstrap-period exit):
   - vsdd-cli switches from `mdatron-core` local-path-dep to published-crate-dep (per SO-F3, vsdd-cli has depended on `mdatron-core` as a Rust library since mid-Step-3; this step is the release-coupling change, not a refactor wave)
   - `bootstrap-validator.py` already deleted (mid-Step-3)
   - Coinage log processed into `vocabulary.yaml`
   - `BootstrapPeriodExited` event emitted (per BOOTSTRAP-MITIGATION § Mitigation 3)

Wall-clock estimate omitted per vsdd-cli's operator-time-binding-without-quantification discipline.

## v1.1 trigger criteria

v1.1 development begins (in parallel with v1.0 maintenance) when ANY of:

- **Adopter evidence for LSP**: ≥2 adopters (operator-counted; informal feedback channels) request real-time editor diagnostics that the hook surface cannot provide
- **Adopter evidence for MCP**: ≥2 adopters or skill-authoring patterns demonstrate the need for structured registry queries that the CLI surface cannot ergonomically serve
- **vsdd-cli upgrade demand**: vsdd-cli's domain-skill authoring loop hits a ceiling on what the registry CLI + skill-preamble can deliver; MCP integration depth becomes load-bearing
- **Operator directive**: SO declares v1.1 begins (with rationale that does not require external evidence)

## What this decision does NOT decide

- **Specific timeline for v1.0** — operator-time-bound; exit criteria are objective
- **v1.0 sub-version releases** (v1.0.1 / v1.0.2 patches) — handled per standard semver
- **Whether Phase E (LSP) and Phase D (MCP) ship simultaneously in v1.1, or sequentially** — TBD when v1.1 begins; depends on adopter-evidence shape
- **Whether `mdatron init` deploys IDE configurations for v1.0 adopters who want to use the LSP from v1.1 ahead of time** — defer to v1.1 release engineering
- **Long-tail features** (schema migration, multi-language source repos, user-defined DSL functions, visual catalog explorer, etc.) — remain v1+ candidates as listed in earlier scoping discussions

## Coordination with vsdd-cli (per co-evolution discipline)

Per [`BOUNDARY-PREAMBLE.md`](./BOUNDARY-PREAMBLE.md) § 8, vsdd-cli's Step 2 scope-adjust must reflect this cut. Specifically:

- vsdd-cli's `DESIGN-VERIFICATION.md` must mark hook items that depend on LSP or MCP integration as **v1.1-mdatron-dependent**, NOT v1.0
- vsdd-cli's `DESIGN-VERIFICATION.md` Track 5o (LSP server implementation) was already declared v1+ — that aligns with v1.1 mdatron
- vsdd-cli's `vsdd mcp-serve` is independent of `mdatron mcp-serve` — both exist; both ship at their own cadence. vsdd-cli's MCP server does not require mdatron's

The vsdd-cli Step 2 commit referencing this decision should land in the same Phase 4 → Phase 1a routing as BOUNDARY-PREAMBLE + BOOTSTRAP-MITIGATION adoption.

---

## Outstanding SO decisions

This artifact closes **SO-F2 only**. Step 0.5 still has open SO decisions:

- **SO-F1**: name 2-3 concrete non-VSDD adopter candidates, OR accept the inversion as speculative with sized cost
- **SO-F3**: choose throwaway scaffolding vs. build-in-mdatron-shape for Step 3 (`vsdd init` implementation pre-mdatron)

Step 0.5 closes when SO-F1 and SO-F3 are answered + the artifact-class-split-table row for "Methodology spec section" (per BOUNDARY-PREAMBLE § 5) is settled.
