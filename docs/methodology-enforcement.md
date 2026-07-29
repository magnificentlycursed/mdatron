# mdatron methodology enforcement

The bound guardrails that make `docs/methodology.md`'s disciplines non-optional,
so a deviation produces compiler-shaped feedback at the act rather than a latent
record. Operator directive 2026-07-28 (#96): "bind, don't just document —
conduct alone drifts."

## Enforcement grades

A guardrail is only as strong as its distance from the reach of the agent it
polices. Grades, weakest to strongest (vsdd-cli #819, ratified; relayed to #96):

- **detection** — reads and reports; cannot block. A finding, a lint.
- **friction** — a bypassable local check that records the deviation. A git
  hook: it runs at the act, but the agent controls local state, so it is
  skippable (`--no-verify`) — the point is not to make the deviation impossible
  but to make it explicit and owned.
- **CI-backed block** — server-side, unskippable, over the range CI can
  independently verify. CI re-derives the check over server-synced state and
  over the checker's own wiring, so it holds even when the local checker was
  edited: CI is a control plane the policed agent does not own.
- **tamper-proof** — holds against an agent that controls all local state,
  including the checker and the tests. This needs an independent corroboration
  oracle the agent cannot author (vsdd-cli #815), or capability restriction (a
  reviewer that literally lacks the mutation tools). Neither is shippable here
  yet — it is the honest boundary.

The load-bearing correction (#99): a **runtime-harness** stop hook (Claude Code)
is grade-2 *friction* at best — it is in-repo code whose own script and wiring
are editable, so it cannot self-protect. Shipping it as a block would assert
enforcement it cannot deliver; vsdd-cli saw exactly this fail live (two
edit-time gates silently de-wired by an init settings clobber, uncaught for
weeks). So mdatron ships **no session-stop block**. The durable third leg is
**CI-tested integrity**, not a session-stop hook.

Mechanisms in hand: mdatron's own engine (patterns and rules over the governed
markdown); git hooks under `.githooks/` (friction); and CI (`cargo test
--workspace` runs the guardrail seeds — the CI-backed block).

## Guardrails

Each guardrail is named for the discipline it binds — no coined labels
(methodology §5).

| Guardrail | Discipline | Grade / mechanism | Status |
|---|---|---|---|
| amendment citation | a DESIGN amendment cites its ratified review (§3) | friction — `.githooks/commit-msg`, backed by CI (the seeds drive the hook) | bound |
| governed-estate register | the naming register holds over the methodology docs (review-entry citations and DESIGN follow on) | CI-backed block — `mdatron verify` self-validate, scoped by `vocabulary_globs` | bound (naming register) |
| routing before fix-close | a fix-close finding carries a prior routing plan (§2) | planned — crosslink-querying check (near the vsdd-cli boundary) | planned |
| round-result parity | a round-result comment cites its child-issue handle (§4) | planned — crosslink-querying check | planned |

## Escape corpus (regression seeds)

Each escape is a dodge the guardrails must catch; the escapes are the negative
fixtures. A guardrail set that misses any escape is insufficient. Each escape
names the grade that catches it — an advisory with no binding grade is itself
the first escape.

| The dodge | Caught by |
|---|---|
| advisory only — a detection with no binding grade is treated as sufficient | this table itself: every advisory names its binding grade |
| fix in place — a fix written with no prior routing | routing-before-fix-close (planned) plus the work-check hook |
| disposition relabel — a fix-close relabelled to dodge routing | routing-before-fix-close (a result or decision closing a fix-lane must still cite a routing plan) |
| ownerless fix — a fix with no owning finding filed | routing-before-fix-close plus the work-check hook (issue-before-code) |
| forged record — a self-authored gate or composition record | amendment citation (friction: cite an external ratified review, not a self-assertion), backed by CI |
| checker edit — the checker itself is edited to pass | CI-backed block: the guardrail seeds (`tests/methodology_seams.rs`) re-derive integrity over the checker's own wiring, so a committed checker-edit surfaces as CI-red independently of the edited local checker. Friction (the commit-time hook) records the honest-path deviation; the locally-skippable residual is owned-bypass (`--no-verify`, the deviation is yours to record). The tamper-proof residual — an edit that also fools CI — is named to the corroboration oracle (vsdd-cli #815), the honest boundary; no in-repo guardrail reaches it. |

## Owned-bypass stance

Following vsdd-cli: a friction guardrail does not pretend a bypass is impossible
(for in-repo code it never is). It makes the honest path frictionless, the
bypass explicit, and the deviation self-recorded — `git commit --no-verify` is
available for a genuine emergency, and the deviation is yours to record. The
durable verification lives in CI, not in the local hook.

## Notes

- Guardrails are additive to the existing binds: the PreToolUse work-check hook
  already refuses code edits without an active issue and enforces the plan and
  result comment discipline on commit and close.
- The escape corpus is a living set: a real deviation that slips through is a
  new seed and a guardrail gap to close.
