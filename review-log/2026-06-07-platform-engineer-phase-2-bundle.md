---
schema_class: review-entry
schema_version: 1.0.0
review_number: 7
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session PE review — Phase 2 of binary-
  first plan: install instructions, CI surface, integration test shape +
  cost, pre-commit hook compatibility, Phase 6 publish-readiness.
lens: >-
  Platform Engineer (install + CI + supply-chain + reproducibility).
  Five-lens weighting: Maintainability 4, Consistency 4, Edge-cases 4,
  Attacker 3, Usability 3.
source: director-raised
session_note: >-
  Cold-session reviewer mode. Sycophancy compensation: Claude authored the
  install instructions + tests. PE failure mode: "the install command works
  on the author's machine because of pre-existing state."
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  Treating every install command as if run on a fresh machine with only
  rustup + a clean shell. Checking CI-time cost + flake-surface.
---

# Findings

## F1 — README's `git clone https://github.com/magnificentlycursed/mdatron` path doesn't match the repo layout (Edge-cases)

**Classification:** accepted-with-remediation
**Routing:** PE + SO pair; small but real.

README install instructions:

```
git clone https://github.com/magnificentlycursed/mdatron
cargo install --path mdatron/mdatron-cli --locked
```

A fresh clone of `magnificentlycursed/mdatron` (if such a standalone repo
existed at that GitHub org) would land the code at `./mdatron-cli/`, not
`./mdatron/mdatron-cli/`. The two-level path assumes the user cloned the
parent `magnificentlycursed` monorepo and the mdatron subdir is at
`./mdatron/`.

Reality check: `magnificentlycursed` is the parent monorepo; whether a
standalone `mdatron` repo exists at github.com/magnificentlycursed/mdatron
is uncertain (the operator's intent appears to be standalone repos for
mdatron + vsdd-cli per Phase 6, but pre-Phase 6 the repo is the monorepo
subdir).

Corrective patterns (pick one):
1. README assumes the monorepo: `git clone https://github.com/magnificentlycursed/magnificentlycursed`
   + `cargo install --path magnificentlycursed/mdatron/mdatron-cli --locked`
2. README assumes a standalone mdatron repo: drop the magnificentlycursed
   prefix from the path
3. Defer the install instruction to Phase 6 + README says "bootstrap-period
   install instructions are in [DESIGN-MDATRON.md § Adopter onboarding]"

(2) and (3) match the post-Phase-6 state best. Pre-Phase-6, the operator-
local install is the cargo install --path ../mdatron/mdatron-cli flow per
the existing project_mdatron_install_and_hook memory — which the README
does NOT document. Add the operator-local form alongside.

## F2 — `cargo install --path` without an explicit `--root` may collide with system state (Edge-cases)

**Classification:** dismissed (with note)
**Routing:** PE-internal.

`cargo install --path mdatron/mdatron-cli --locked` installs into
`~/.cargo/bin/mdatron` by default. If the user has an `mdatron` binary on
PATH from another source (rare; the name is unique), the install silently
shadows it. Cargo's behavior is documented; not a Phase 2 issue.

## F3 — `--locked` requires `Cargo.lock` to be committed; verify this is done (Consistency)

**Classification:** dismissed
**Routing:** PE-internal verification.

`mdatron/Cargo.lock` is present at the repo root (verified by the `find`
audit during Phase 1a authoring — it's in the top-level listing alongside
Cargo.toml). `--locked` is functional. Pass.

## F4 — `cli_integration.rs` spawns 18+ subprocesses serially; CI wall-time concern (Maintainability)

**Classification:** dismissed (with note)
**Routing:** PE-internal.

Each test in cli_integration.rs spawns one `mdatron` subprocess. cargo test
runs tests in parallel by default (one thread per logical core, default).
On a 4-core CI runner, 18 tests in 4 batches; ~1 second per batch wall
time. Total cli_integration runtime: ~3-4 seconds. Acceptable for v0.1.0.

If CI minutes become a concern, the `output_format.rs` integration tests
already exist (10 more subprocess spawns); collapse the two into a shared
`tests/common/` helper. v0.1.x. Not Phase 2 work.

## F5 — `cli_integration.rs:mdatron_bin()` resolves to `target/debug/mdatron` — release builds break (Edge-cases)

**Classification:** accepted-with-remediation
**Routing:** PE + QE pair; small.

The binary path is hard-coded: `target/debug/mdatron`. `cargo test
--release` puts the binary at `target/release/mdatron`; the path lookup
fails silently (`Command::new(missing_path)` panics on spawn).

Corrective pattern: `env!("CARGO_BIN_EXE_mdatron")` resolves to the
cargo-built binary regardless of profile. Standard pattern for integration
tests of a workspace binary. ~1 line change. The existing
`tests/output_format.rs` has the same bug; opportunity to fix both.

## F6 — Pre-commit hook (vsdd-cli `.githooks/pre-commit`) runs `mdatron verify`; format_tty refactor changes its output shape (Edge-cases)

**Classification:** dismissed (with note)
**Routing:** PE-internal verification.

Phase 2c's format_tty refactor changes the rustc-shape:
- before: `error[CODE]: <message>` + arrow + optional help
- after: `error[CODE]: <summary>` + arrow + `= note: <message>` + help + explain

The pre-commit hook captures stderr text. Per the project_mdatron_install_
and_hook memory: "the hook caught a deliberate three-finding fixture (two
MDATRON-E0001 schema violations + one VSDD-E0206 cross-reference rule) and
blocked the commit with exit 1; a real clean commit landed normally."

The hook's behavior depends on exit code (blocking vs passing), not on the
text shape. The format_tty refactor preserves exit codes. Pass.

## F7 — Pre-commit hook is sensitive to mdatron binary version drift (Consistency)

**Classification:** dismissed
**Routing:** PE-internal.

The project_mdatron_install_and_hook memory notes: "If mdatron changes,
reinstall via the same `cargo install --path` command to keep the local
binary in step." Phase 2's changes (catalog + format_tty + README) all
require reinstall to take effect for the pre-commit hook. Operator
workflow includes the reinstall step (verified post-Phase-2b). Pass.

## F8 — `cargo install` doesn't verify checksums on transitive deps in workspace mode (Attacker)

**Classification:** dismissed
**Routing:** PE-internal; supply-chain.

`cargo install --locked` honors Cargo.lock for the install target; doesn't
re-verify checksums of the lock entries beyond cargo's standard registry
checksum check. For the v0.1.0 pre-publish period, the install path
(`--path`) sources straight from the local checkout; the lock is the
operator's responsibility. Phase 6 publish will engage cargo's registry
checksum discipline (Cargo.lock + crates.io's sha256). Not a Phase 2
concern.

# Cross-finding cohesion

F1 + F5 are both "the install/test paths assume specific layouts that
break under non-default conditions". F2, F3, F4, F6, F7, F8 are all
verifications-pass. The two material findings are F1 (README install
instruction precision) and F5 (CARGO_BIN_EXE_mdatron).

# Summary

8 findings: 0 resolved-pending, 2 accepted-with-remediation (F1, F5),
6 dismissed (F2, F3, F4, F6, F7, F8). Install + CI + pre-commit hook
discipline is sound; the two specific corrections are mechanically
trivial. No hallucinations.
