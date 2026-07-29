---
schema_class: review-entry
schema_version: 1.0.0
review_number: 19
date: 2026-07-29
phase: phase-4
scope: >-
  Phase-4 Feedback-Integration routing of the full-app pre-crates.io-publish
  Phase-3 roast (review lane #122). Routes every surviving finding to the
  earliest artifact/phase that would have prevented it, with gate + sequencing,
  ahead of the #48 publish decision.
lens: >-
  Operator-orchestrated routing (no domain composition). Root-cause routing
  question per finding: "what artifact, had it been correct, would have
  prevented this?" — with the anti-pattern guard against dumping to Phase 2b.
source: domain-raised
session_note: >-
  Consolidates the 11-domain cold-session roast (SE, Security, Red Team, TW,
  DR, AI Engineer, SO, PE, SA, Data Engineer, Sanity). MECHANISM CAVEAT: the
  roast used an unfaithful single-round + per-finding-verifier orchestration
  (see magnificentlycursed/vsdd-cli#11); the FINDINGS are valid but the
  re-roast to MVR will use vsdd's iterative-rounds model.
model: claude-opus-4-8
execution_method: workflow-swarm-cold-session-manual
sycophancy_compensation: >-
  The author of the work under review (the 0.4.0 arc) was the same agent that
  orchestrated the review; reviewers ran cold with no author context. 5 severe
  findings were adversarially refuted (SE install/ReDoS/bounds + PE/Sanity one
  each); routing covers only the survivors.
---

# Phase-4 routing — full-app pre-publish roast (#122)

Publish verdict from Phase 3: **fix-blockers-first**; no surviving finding touches validation correctness. Routing per `vsdd-phase-4` table. `Seq` = sequencing vs the `#48` publish.

## Blockers — root cause is a SUITE gap (missing release gate), not implementation

| Finding | Route | Owning artifact | Gate | Seq |
|---|---|---|---|---|
| BLO1 no LICENSE (MIT claimed) — 6 lenses | Suite + artifact-add | `LICENSE`, `Cargo.toml` | LICENSE present + backs the MIT claim; `cargo package --list` shows it | **BLOCK publish** |
| BLO2 unbounded `cargo package` ships 221 files incl. `.crosslink/driver-key.pub` — 5 lenses | Suite + `Cargo.toml` | `Cargo.toml include`, `rm REPLACE` | `cargo package --list` == allowlist (no `.claude/.crosslink/review-log/.vsdd/.mdatron/`) | **BLOCK publish** |
| SHO3 no CI packaging/publish rehearsal — 2 lenses | **Suite** | `.github/workflows/` | CI runs `cargo publish --dry-run --locked` + asserts file-list; required on release tags | **BLOCK publish** (the gate that keeps BLO1/BLO2 fixed) |

## Should-fix — untrusted-input hardening (Security/Red Team/Data Eng)

| Finding | Route | Owning artifact | Gate | Seq |
|---|---|---|---|---|
| SHO1 no parse resource-bounds (depth-bomb 19s / mem 316MB) + DESIGN.md:99 asserts them as shipped (false, packaged) | **6** (DESIGN↔impl converge) → 1a+2a+2b+5 | `DESIGN.md §99`, `frontmatter.rs`, `verify.rs:898` | min: DESIGN drops the false guarantee (publish); full: byte/depth/aggregate guard → bound-exceeded diagnostic + red-gate | DESIGN-claim = publish-relevant; guard = security fast-follow |
| SHO2 DSL expr recursion → uncatchable SIGABRT, zero `--json` (violates #112) | 2a → 2b → 5 | `dsl/expr_parser.rs`, `expr.rs`, `dep.rs` | deep-parens → PipelineError in envelope, no abort | security fast-follow |
| SHO4 non-UTF8 filename blanks the entire `--json` run | 2a → 2b | `output.rs`, `diagnostic.rs` | bad path serialized lossily; one bad file can't poison the array | robustness fast-follow |
| SHO5 U+2028/2029 leak into `-->` location line (safe_display escapes only Cc) | **6** (converge 3 escapers) + 2a | `diagnostic.rs:58-69` | one renderer covers Cc∪Zl∪Zp; regression test | security fast-follow (marking discipline) |
| SHO7 config typo (`file_glob:`) silently broadens jurisdiction (#80-D1 overreach) | **1a** (spec) + 2b + 2a | `verify.rs from_project`, `DESIGN §80-D1` | present-config-with-no-globs refuses/warns; `**/*.md` reserved for explicit `--files` | security-ish / design |

## Should-fix — contract/diagnostic consistency (mostly Phase 6 convergence, not 2b)

| Finding | Route | Owning artifact | Gate | Seq |
|---|---|---|---|---|
| SHO6 `--files` help contradicts the shipped refuse-without-jurisdiction behavior | **6** + 2a (help tripwire) | `main.rs` | help matches behavior; tripwire fixture | polish/docs |
| SHO8 E0012 two divergent names (catalog vs runtime) passes CI | **6** + 2a (extend tripwire) | `explain/`, catalog, tripwire | emitted `Finding.summary` == catalog slug per code | polish |
| SHO9 E0080 explain + DESIGN §Init overclaim what `init` scaffolds | **6/1a** + 2b | `DESIGN §Init`, `explain/E0080` | accurate scaffold description | docs pass |
| SHO11 `--json` help cites a dead `../vsdd-cli/` path | 2b + 2a (denylist) | `main.rs` | help cites the shipped schema | polish |
| DEF8 pins.yaml/manifest `fs::write` truncate-in-place (torn write) | 2b + 2a | `pin.rs`, `init.rs` | temp-file + fsync + atomic rename | fast-follow / defer |

## Should-fix — discoverability + supply-chain

| Finding | Route | Owning artifact | Gate | Seq |
|---|---|---|---|---|
| SHO10 no way for a binary consumer to obtain the schema | 1c → 2b | `main.rs` | `mdatron schema` prints the embedded schema | polish (unblocks DOC2) |
| SHO12 mutable-ref Actions in release job; no cargo-deny/audit-deny | **Suite/5** | `.github/workflows/`, `deny.toml` | `uses:` SHA-pinned; `cargo deny` (advisories+licenses+sources) | release hardening (publish-relevant) |

## Docs / README pass (Technical Writer + Documentation Reviewer)

DOC1 version-stale strings · DOC2 `--json` envelope undocumented · DOC3 dead `../vsdd-cli/` links (x3) · DOC4 `--changed` unmentioned · DOC5 DSL reference omits filter/not_in/let: · DOC6 example uses a reserved MDATRON- code (also opt: warn at pattern-load) · DOC7 four-vs-five families · DOC9 lib.rs "v0.1.0" rustdoc header · DOC10 W0044-W0047 "Introduced in: 0.3.0" (known polish).
→ **Route: Docs pass** (owning `README.md`, `src/explain/*`, `src/lib.rs`), TW/DR re-review gate. **Seq: pre-publish** (README is the crates.io front page) — **governed by DEF1**.
DOC8 DESIGN.md staleness (init tense, missing `pin`, double commit-pin, missing E0062) → **Route: 1a** (frozen-spec amendment path, re-enter phase 1a per project rule).

## Defer — design-questions (Phase 1a decisions)

DEF1 **0.4.0-vs-v1.0 milestone** → SO ruling; **gates the publish + the README version framing — decide first.** · DEF2 aggregate token cap (`--max-findings`) → 1c/new issue · DEF3 `--strict` → #121 disposition · DEF4 absolute-vs-relative paths (agent-reproducibility + host-leak) → 1a (candidate elevate) · DEF5 routes/pins/vocab format-versioning → 1a · DEF6 `mdatron.dev` `$id` ownership → 1a/metadata (candidate elevate, publish) · DEF7 cost-ledger single-family coverage → 5/new issue.

## Anti-2b-collapse check (vsdd Phase-4 completion criterion)

Of ~27 routed findings, **2b-only routes: 0**. Most route to Suite (release gates), Phase 6 (convergence), Phase 1a (spec/design), or 2a (test discipline); implementation fixes ride as the *last* leg of multi-phase chains. The spec/test/suite layers are carrying their share — the routing did **not** collapse to "the implementation is what's wrong."
