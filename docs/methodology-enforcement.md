# mdatron methodology enforcement

The bound seams that make `docs/methodology.md`'s disciplines non-optional, so a
deviation produces compiler-shaped feedback at the act rather than a latent
record. Operator directive 2026-07-28 (#96): "bind, don't just document —
conduct alone drifts."

Seams span three mechanisms already in hand:

- mdatron's own engine — patterns and rules over the design and review-log
  estate (mdatron governs its own governed markdown).
- git hooks — the `commit-msg` and `pre-commit` hooks under `.githooks/`
  (repo-tracked), refusing malformed commits.
- the session hook estate — `.claude/hooks/` (a session-stop hook binding the
  disciplines at the end of a work session; the PreToolUse work-check hook
  already binds issue-before-code and comment discipline).

Some derivations need vsdd-cli and are out of scope until it ships (the honest
boundary): phase-answer derivation, process-integrity status queries, the
mechanized gate commands.

## Seams

Each seam is named for the discipline it binds — no coined labels (methodology
§5).

| Seam | Discipline | Mechanism | Status |
|---|---|---|---|
| amendment citation | a DESIGN amendment cites its ratified review (§3) | `commit-msg` hook | bound |
| routing before fix-close | a fix-close finding carries a prior routing plan (§2) | crosslink-querying check (session-stop / pre-commit) | planned |
| round-result parity | a round-result comment cites its child-issue handle (§4) | crosslink-querying check | planned |
| session-stop binding | the disciplines are bound at session exit; no un-cited exit | `.claude/hooks` stop hook | planned |
| governed-estate register | the naming register holds over the methodology docs (review-entry citations and DESIGN follow on) | mdatron's engine over the methodology docs, scoped by `vocabulary_globs` | bound (naming register) |

## Escape corpus (regression seeds)

Each escape is a dodge the seams must catch; the escapes are the negative
fixtures. A seam set that misses any escape is insufficient — each seam lane
lands with the seeds it must catch.

| The dodge | Caught by |
|---|---|
| advisory only — a detection with no binding seam is treated as sufficient | this table itself: every advisory finding must name its binding seam |
| fix in place — a fix written with no prior routing | the routing-before-fix-close seam |
| disposition relabel — a fix-close relabelled as a disposition to dodge routing | the routing-before-fix-close seam (a result or decision closing a fix-lane must still cite a routing plan) |
| ownerless fix — a fix with no owning finding filed | the routing-before-fix-close seam plus the work-check hook (issue-before-code) |
| forged record — a self-authored gate or composition record | the amendment-citation seam (a DESIGN change cites an external ratified review, not a self-assertion) plus the round-result-parity seam |
| checker edit — the checker itself is edited to pass | the session-stop-binding seam (a commit touching the seam scripts is flagged for review; the seam's own tests must still pass) |

## Notes

- Seams are additive to the existing binds: the PreToolUse work-check hook
  already refuses code edits without an active issue and enforces the plan and
  result comment discipline on commit and close.
- The escape corpus is a living set: a real deviation that slips through is a
  new seed and a seam gap to close.
