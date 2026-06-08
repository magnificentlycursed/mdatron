---
schema_class: review-entry
schema_version: 1.0.0
review_number: 9
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session Red Team review — Phase 2
  attack surfaces: explain catalog as supply-chain target, argv injection
  into not-found error messages, README install instructions as
  typosquat target, integration test helper as code path that could be
  weaponized in delegate context.
lens: >-
  Red Team (attacker's mindset + assumption-breaking + supply-chain).
  Five-lens weighting: Attacker 5, Edge-cases 4, Consistency 2,
  Usability 2, Maintainability 2.
source: director-raised
session_note: >-
  Cold-session reviewer mode. Red-team stance: assume an attacker controls
  every PR-modifiable surface and a subset of the install supply chain.
  What's the minimum-effort path to harm?
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  Looking for "the safe-looking thing that becomes unsafe under a
  plausible attacker model" failure modes. Treating defenses as untested
  until a fixture exercises them.
---

# Findings

## F1 — `include_str!` of explain pages: PR-modifiable; attacker injects misleading prose (Attacker)

**Classification:** dismissed (with note)
**Routing:** Red Team-internal observation.

Each explain page is `mdatron-cli/src/explain/MDATRON-E*.md`; embedded via
`include_str!` at build time. A malicious PR that modifies a page can ship
misleading remediation prose with the next binary release — e.g., E0070's
"How to fix" could be modified to suggest `--project-root /tmp` (lossless
in a normal context, but in an attacker-crafted environment, /tmp could be
under attacker control).

Mitigation: PR review on `src/explain/*.md` is the binary's existing
governance; same trust posture as any code under PR. Note: a CODEOWNERS
entry for `src/explain/` could surface the prose-as-binary-payload nature
to reviewers (signals "review the prose"). v0.1.x cheap win.

Catalog pages becoming attack vectors is a real but bounded risk; the prose
is rendered to terminal, not executed. Mentor-voice operator hints could
trick an operator into running a bad command, but the trick is visible in
the rendered text. Bounded.

## F2 — `cmd_explain` reflects argv into stderr — ANSI escape injection surface (Attacker)

**Classification:** accepted-with-remediation
**Routing:** Red Team + Security pair; small.

`cmd_explain` not-found path:

```rust
eprintln!(
    "error[MDATRON-E0080]: no explain page found for {code}\n   \
     = note: ..."
);
```

If `code` contains ANSI escape sequences (e.g., `\x1b[2J` to clear screen),
they're written verbatim to stderr. A user piping mdatron's stderr to a
terminal sees the escape executed.

Plausible attack:
- A CI log harvester captures mdatron stderr from `mdatron explain "$ATTACKER_INPUT"`
- Attacker stuffs control chars into `ATTACKER_INPUT`
- Log viewer renders the escapes; can hide log lines, scroll history, etc.

Most terminals strip non-printable chars from CI log uploads; the attack
is theoretical for v0.1.0. Belt-and-suspenders: sanitize `code` to
[A-Za-z0-9-] before reflecting; reject anything else with a generic "code
must match ^[A-Za-z0-9-]+$" message. ~5 LOC. v0.1.x.

## F3 — README `git clone https://github.com/magnificentlycursed/mdatron` — typosquat target (Attacker)

**Classification:** accepted-with-remediation
**Routing:** Red Team + PE pair (overlaps with PE F1).

Same finding as PE F1 from the attacker angle. If `github.com/magnificentlycursed/mdatron`
is NOT the operator's canonical hosting (the monorepo is `magnificentlycursed/magnificentlycursed`),
an attacker who registers `github.com/magnificentlycursed/mdatron` (assuming
the org is unclaimed; if the operator owns the org, attacker would need
account compromise) could ship a malicious repo at that path.

The mitigation overlaps with PE F1's corrective pattern: clarify the
canonical repo path. Also, register `magnificentlycursed/mdatron` even if
it's a redirect-only entry, to prevent squatting.

## F4 — `extract_fence` helper would be unsafe in a delegate context (Attacker)

**Classification:** dismissed (with note)
**Routing:** Red Team-internal.

`cli_integration.rs:extract_fence` parses a markdown string for fenced code
blocks. Today it parses the project's own README — operator-controlled
content. If reused in a delegate (per DESIGN-MDATRON.md § Delegated checks)
to extract patterns from PR-modifiable markdown, the helper would parse
untrusted input.

Two issues if reused:
1. No length cap; massive input causes O(n) string allocation
2. Nested fences confuse the parser (per QE F7)

For v0.1.0, the helper stays in test scope. Note for future: if extract_
fence migrates to production code, add length cap + use a proper
markdown parser (pulldown-cmark already a transitive dep via markdown
processing — wait, it's not; the project doesn't use a markdown parser
for body parsing today, only YAML frontmatter via serde_yaml). v0.1.x
not blocking.

## F5 — `cargo install --path` accepts arbitrary local paths (Attacker)

**Classification:** dismissed
**Routing:** Red Team-internal.

README's bootstrap install instruction: `cargo install --path
mdatron/mdatron-cli --locked`. If an attacker substitutes `--path
/tmp/malicious-crate`, cargo installs that. Standard cargo discipline; not
mdatron-specific. The mitigation is "only run `cargo install --path`
against paths the operator verified." Mentioned in standard cargo docs;
README doesn't need to repeat.

## F6 — `mdatron explain` could be abused for catalog enumeration / fingerprinting (Attacker)

**Classification:** dismissed
**Routing:** Red Team-internal.

An attacker with shell access could iterate `mdatron explain MDATRON-E####`
across all 4-digit codes to enumerate the catalog. Why care: catalog
fingerprinting reveals the mdatron version. The version is publicly
discoverable via `mdatron --version`; enumeration adds no new info. Dismiss.

## F7 — Pre-commit hook is a privileged execution surface; format_tty's stderr now larger (Edge-cases)

**Classification:** dismissed (with note)
**Routing:** Red Team-internal.

The pre-commit hook executes `mdatron verify` automatically on commit. The
hook's output goes to the operator's terminal. After Phase 2c, format_tty
adds a `= note:` and `= explain:` line per finding. For a 50-finding
violation set, hook output grows from ~150 lines to ~300 lines.

Not security-relevant per se; the hook's existing terminal output is
operator-controlled. Note: terminal scroll history could blow past the
useful diagnostics under high-finding-count runs. The summary line
("mdatron verify: N error(s)...") still prints; operators can grep for it.

## F8 — The 5 explain pages don't include a "trust this prose" signal — but `mdatron --version` does (Attacker)

**Classification:** dismissed
**Routing:** Red Team-internal observation.

An operator following an explain page's "How to fix" advice should know
the prose came from the binary they installed (not an attacker-crafted
fork or a tampered build). `mdatron --version` prints the version + (with
appropriate flags) the build commit. The standard cargo build chain
provides reproducibility verification.

For v0.1.0 this is sufficient. Post-Phase 6 (crates.io publish), cargo-
audit + cosign signatures land per the binary-first plan § Always-alongside.
Pre-Phase 6, the operator's trust is anchored at their local checkout.

# Summary

8 findings: 0 resolved-pending, 2 accepted-with-remediation (F2 ANSI
sanitization, F3 typosquat clarity), 6 dismissed (F1 PR-review mitigates,
F4 test-scope, F5 cargo-standard, F6 fingerprinting irrelevant, F7
hook-output-size, F8 trust-anchor via --version). No hallucinations.
Attack surfaces are bounded by the existing PR-review + supply-chain
disciplines; the two new mitigations are cheap belt-and-suspenders.
