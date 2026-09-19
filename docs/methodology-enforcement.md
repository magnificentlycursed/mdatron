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
markdown); git hooks under `.githooks/` (friction); and CI, with a precision
the first draft of this document overclaimed (#170 methodology-docs review finding 1):
**only the self-validation job is a required status check on `main`**. The two
grade-3 legs therefore both live inside that job — `mdatron verify
--deny-warnings` (the register) and the amendment-citation range scan (below).
The guardrail seeds (`tests/methodology_seams.rs`) run in the `Test` job, which
is **not** required — at the merge boundary they are grade-1 detection, not a
block, until the operator adds `Test` to the required checks (tracked, #170).
Calling them a block before that would be the exact overclaim this grade ladder
exists to prevent.

## Guardrails

Each guardrail is named for the discipline it binds — no coined labels
(methodology §5).

| Guardrail | Discipline | Grade / mechanism | Status |
|---|---|---|---|
| amendment citation | a DESIGN amendment cites its ratified review (§3) | friction — `.githooks/commit-msg` — **plus a CI-backed block**: the required self-validation job re-derives the two citation checks over every DESIGN-touching commit in the pushed range (the seeds additionally pin the hook script itself, in the unrequired `Test` job) | bound |
| governed-estate register | the naming register holds over the methodology docs (scope: `docs/methodology*.md` per `vocabulary_globs`; the review-log estate is deliberately unscanned — frozen evidence — and `DESIGN.md` is outside `file_globs`) | CI-backed block — `mdatron verify --deny-warnings` self-validate | bound — **letter-plus-number clusters only** (`E0091`): §5's coined-label and all-caps prohibitions are mechanized by nothing yet and hold as conduct |
| phase discipline | each layer runs the phases; an exit is a boundary commit plus a typed result comment (§1) | honest boundary — phase-answer derivation and gate commands need vsdd-cli; the recording vehicle (issue lanes, boundary commits, typed comments) is conduct with no detection path | deferred (conduct) |
| routing before fix-close | a fix-close finding carries a prior routing plan (§2) | planned — crosslink-querying check (vsdd-cli boundary, tracked #170) | planned |
| round-result parity | a phase-exit result comment cites the child-issue handles it closes over (§1's result-comment discipline; term defined here) | planned — crosslink-querying check (vsdd-cli boundary, tracked #170) | planned |
| finding ownership | findings carry an owner and a validator (§4) | planned — crosslink-querying check (vsdd-cli boundary, tracked #170); no detection path today | planned |

**Binding is per-clone (#96).** The `.githooks/commit-msg` friction is active only
where `git config core.hooksPath .githooks` is set — a LOCAL git config a fresh
clone does not inherit (it is untracked, so a full re-clone drops it silently).
Re-run it after cloning; a durable auto-bind is requested upstream
(crosslink#99). Friction is the *weaker* half of the seam by design. What an
unbound clone loses and keeps (#170 review finding 2 corrected the first draft here):
it loses the commit-time nudge AND can write uncited amendments into its local
history — the seam seeds never look at commits, only at the checker script. The
merge-boundary guarantee is held by the required self-validation job's
amendment-citation range scan, which re-derives the citations over the pushed
commits themselves: an uncited amendment can exist locally but cannot merge.

## Escape corpus (regression seeds)

Each escape is a dodge the guardrails must catch; the escapes are the negative
fixtures. A guardrail set that misses any escape is insufficient. Each escape
names the grade that catches it — an advisory with no binding grade is itself
the first escape.

| The dodge | Caught by |
|---|---|
| advisory only — a detection with no binding grade is treated as sufficient | this table itself: every row names its binding grade **or its open gap** — a row claiming coverage it lacks is this escape in disguise (#170 review finding 4 caught three doing exactly that) |
| fix in place — a fix written with no prior routing | **no durable catch today** — routing-before-fix-close is planned (vsdd-cli boundary, #170); the work-check hook reminds but does not refuse (see Notes); held by conduct |
| disposition relabel — a fix-close relabelled to dodge routing | **no durable catch today** — same planned guardrail (#170); a result or decision closing a fix-lane must still cite a routing plan, but nothing checks it yet |
| ownerless fix — a fix with no owning finding filed | partial — the work-check hook's issue-before-code refusal is its one binding half (grade 2); the finding-ownership half is planned (#170) |
| forged record — a self-authored gate or composition record | amendment citation, with its residual named (#170 review finding 5): the friction and the CI range scan match citation **keywords** — a self-authored message that types "ratified #N" passes both; verifying the cited review exists and is external is deferred to the corroboration oracle (vsdd-cli #815), the same honest boundary as the row below |
| checker edit — the checker itself is edited to pass | CI-backed block: the guardrail seeds (`tests/methodology_seams.rs`) re-derive integrity over the checker's own wiring, so a committed checker-edit surfaces as CI-red independently of the edited local checker. Friction (the commit-time hook) records the honest-path deviation; the locally-skippable residual is owned-bypass (`--no-verify`, the deviation is yours to record). The tamper-proof residual — an edit that also fools CI — is named to the corroboration oracle (vsdd-cli #815), the honest boundary; no in-repo guardrail reaches it. |

## Owned-bypass stance

Following vsdd-cli: a friction guardrail does not pretend a bypass is impossible
(for in-repo code it never is). It makes the honest path frictionless, the
bypass explicit, and the deviation self-recorded — `git commit --no-verify` is
available for a genuine emergency, and the deviation is yours to record. The
durable verification lives in CI, not in the local hook.

## Notes

- Guardrails are additive to the existing binds, stated at their real strength
  (#170 review finding 3): the PreToolUse work-check hook **refuses** code edits
  without an active issue (its one binding half, grade-2 friction) and
  **reminds** about the plan/result comment discipline — the live config runs
  `comment_discipline: encouraged`, and agent contexts run relaxed with the
  discipline off, so it is a nudge, not a catch (the record shows lanes closed
  without result comments under it). Promoting it to `required` — and deciding
  the agent-override posture deliberately — is an open operator decision
  (#170).
- The escape corpus is a living set: a real deviation that slips through is a
  new seed and a guardrail gap to close.
