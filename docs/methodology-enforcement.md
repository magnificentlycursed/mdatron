# mdatron methodology enforcement

The bound seams that make `docs/methodology.md`'s disciplines non-optional, so a
deviation produces compiler-shaped feedback at the act rather than a latent
record. Operator directive 2026-07-28 (#96): "bind, don't just document —
conduct alone drifts."

Seams span three mechanisms already in hand:

- **mdatron's own engine** — patterns/rules over the design and review-log
  estate (mdatron governs its own governed markdown).
- **git hooks** — `commit-msg` / `pre-commit` under `.githooks/` (repo-tracked),
  refusing malformed commits.
- **the session hook estate** — `.claude/hooks/` (a session-stop hook binding
  the disciplines at the end of a work session; the PreToolUse `work-check`
  already binds issue-before-code + comment discipline).

Some derivations need vsdd-cli and are out of scope until it ships (the honest
boundary): phase-answer derivation, process-integrity status queries, the
mechanized gate commands.

## Seams

| # | Discipline | Mechanism | Status |
|---|---|---|---|
| S1 | DESIGN amendment cites its ratified review (§3) | `commit-msg` hook | building |
| S2 | A fix-close finding carries a prior routing plan (§2) | crosslink-querying check (session-stop / pre-commit) | planned |
| S3 | A round-result comment cites its child-issue handle (parity) (§4) | crosslink-querying check | planned |
| S4 | Session-stop binds the disciplines; no un-cited exit | `.claude/hooks` Stop hook | planned |
| S5 | Governed-estate rules (naming register, review-entry citations) | mdatron patterns over review-log/DESIGN | planned |

## Escape corpus (regression seeds)

Each escape must be caught by at least one seam. A seam set that misses any
escape is insufficient. The escapes are the negative fixtures; each seam lane
lands with the seeds it must catch.

| Escape | The dodge | Caught by |
|---|---|---|
| E-a | detection-without-a-seam is treated as sufficient (advisory only) | this table itself: every advisory finding must name its binding seam |
| E-b | fix-in-place (fix without routing) | S2 |
| E-c | relabel a fix-close as a `disposition` to dodge routing | S2 (a `result`/`decision` closing a fix-lane must still cite a routing plan) |
| E-d | fix with no owning finding filed | S2 + `work-check` (issue-before-code) |
| E-e | forge a self-authored gate/composition record | S1 (DESIGN change must cite an EXTERNAL ratified review, not self-assert) + S3 |
| E-f | edit the checker itself to pass | S4 (a commit touching the seam scripts is flagged for review; the seam's own tests must still pass) |

## Notes

- Seams are additive to the existing binds: the PreToolUse `work-check` hook
  already refuses code edits without an active issue and enforces the
  `plan`/`result` comment discipline on commit/close.
- The escape corpus is a living set: a real deviation that slips through is a
  new seed and a seam gap to close.
