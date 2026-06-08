---
schema_class: review-entry
schema_version: 1.0.0
review_number: 10
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session Security review — Phase 2
  surfaces: argv handling (cmd_explain), terminal output encoding,
  file-path interpolation in format_tty, external links in README + explain
  catalog, supply-chain posture for explain pages.
lens: >-
  Security (CIA + secret-handling + input validation + output encoding).
  Five-lens weighting: Attacker 4, Edge-cases 4, Consistency 3,
  Maintainability 3, Usability 2.
source: director-raised
session_note: >-
  Cold-session reviewer mode. Security-domain stance: assume every input
  is hostile until validated; every output is read by something that
  could be exploited; every external link is potentially a redirect.
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  Looking for "what gets interpolated into outputs that crosses a trust
  boundary." Treating defenses as untested until a fixture exercises
  the bad-input path.
---

# Findings

## F1 — `cmd_explain` doesn't validate argv format before reflecting into stderr (Attacker)

**Classification:** accepted-with-remediation
**Routing:** Security + Red Team pair (overlaps with Red Team F2).

Same root as Red Team F2: `cmd_explain` reflects `code` into stderr in the
not-found path. Security view adds: argv may contain shell metacharacters
that, when rendered to terminal, could be misinterpreted by downstream
parsing (CI logs, redirected files, etc.).

Recommended validation: reject `code` arguments that don't match
`^[A-Z]+-[A-Z][0-9]{4}$` at the argv parse layer (clap validator).
Standardizes the input shape + closes ANSI injection + closes
shell-meta injection in one. ~5 LOC clap macro.

## F2 — `format_tty` interpolates file path verbatim; paths may contain control chars (Attacker)

**Classification:** accepted-with-remediation
**Routing:** Security + SE pair (overlaps with SE F6 for the multi-line case).

`format_tty` includes `{}: {}\n  --> {file}:{line}` where `{file}` is
`self.location.file.display()`. On Unix, file paths can contain any byte
except `\0` and `/`. Newline + control chars in paths are legal.

A markdown file named `evil\x1b[2J.md` would, after format_tty, produce
stderr containing a screen-clear escape. The mdatron verify pipeline
discovers files via glob walking + the project root; an attacker who can
write into the project tree (e.g., a malicious PR adding a control-char-
named file) gets the escape into operator terminals on `mdatron verify`.

Defense layers:
1. Sanitize path display: replace control chars with `\xNN` escapes
   before interpolation. Doesn't lose precision (operator can decode).
2. Terminal-level: most modern terminals strip escapes in pipes; in
   interactive mode they execute. Operator-shell discipline.

Recommended Phase 2 polish: a `safe_display()` on Location that returns
String with control chars escaped. Used by format_tty + by the JSON
output stream contract (Phase 0 BC says forward-slash + ASCII;
control-char-containing paths aren't well-defined).

## F3 — External URLs in README + explain catalog (Consistency)

**Classification:** dismissed
**Routing:** Security-internal verification.

URLs in scope:
- README cites `https://gist.github.com/dollspace-gay/d8d3bc3ecf...` (the
  VSDD whitepaper)
- README cites `https://github.com/magnificentlycursed/mdatron` (install)
- Explain pages cite no external URLs (verified by grep)

All HTTPS; both anchored to operator-trusted identities (dollspace's
gist, the magnificentlycursed org). No redirect-chain concerns. Pass.

## F4 — No secret-shaped strings in Phase 2 artifacts (Consistency)

**Classification:** dismissed
**Routing:** Security-internal verification.

Grep across all Phase 2 files for: `sk-ant-`, `Bearer `, `password=`,
`token=`, `api_key`, `secret_`. Zero hits. Pass.

The methodology-spec discipline (auth-method declared via env-var-only;
schema validator rejects credential-shaped fields) covers VSDD's auth
surface; Phase 2 does not introduce any auth surface.

## F5 — `cargo install --locked` is good supply-chain discipline; README cites it ✓ (Consistency)

**Classification:** dismissed (with note)
**Routing:** Security-internal verification.

README install instruction includes `--locked`. Best-practice; matches
DESIGN-MDATRON.md's reproducibility goals. Phase 6 (crates.io publish)
extends this with cosign + SLSA per V1-SHIP-CRITERIA.md.

Note: the README's "Once mdatron 0.1.0 is published, the install command
becomes `cargo install mdatron --locked`" — operator who installs from
crates.io should also pin a version (`--version "0.1.x"`) to avoid
unintentional upgrades. The exact pin can be defined at Phase 6 release
time. v0.1.x.

## F6 — Pre-commit hook executes mdatron with operator-shell privileges (Attacker)

**Classification:** dismissed
**Routing:** Security-internal.

The pre-commit hook is shell-invoked via git's hooks/pre-commit; runs in
the operator's shell context. mdatron's verify pipeline is read-only (no
file writes to the project tree outside its own diagnostic surface).
Trust posture: hook is operator-trusted (the operator installed it).
Standard git-hook discipline; not Phase 2 specific.

## F7 — explain catalog's `mod tests` runs `every_baseline_code_has_a_catalog_page` at test time, not build time (Maintainability)

**Classification:** dismissed (with note)
**Routing:** Security-internal observation.

If a future PR ships an explain page with malformed structure (missing
heading), `cargo build` succeeds. The integrity check runs in `cargo test`.
A build-script-level check would catch malformed pages at build time
(matching Rust's discipline of pushing checks left).

Not Phase 2 work; the test catches the failure mode. The note: Phase 5
candidate for "build-time invariants on embedded resources" — same
pattern as the catalog-emission-asymmetry lint (SA F1).

## F8 — `mdatron explain` doesn't shell-out or fork; bounded blast radius (Attacker)

**Classification:** dismissed
**Routing:** Security-internal verification.

`cmd_explain` is a pure-Rust subcommand: print embedded string to stdout
or stderr; no `Command::new`; no shell invocation; no filesystem write.
Blast radius is bounded to the process's stdout/stderr/exit-code.
Confirmed. Pass.

# Summary

8 findings: 0 resolved-pending, 2 accepted-with-remediation (F1 argv
validation, F2 path control-char escaping), 6 dismissed (F3 URL audit,
F4 secret audit, F5 --locked check, F6 hook trust, F7 test-time check,
F8 blast-radius). The two material findings (F1 + F2) overlap with Red
Team F2 and SE F6; routed together for a single fix. No hallucinations.
