# mdatron methodology enforcement

The bound guardrails that make `docs/methodology.md`'s disciplines non-optional,
so a deviation produces compiler-shaped feedback at the act rather than a latent
record. Operator directive 2026-07-28 (#96): "bind, don't just document —
conduct alone drifts."

## Enforcement grades

A guardrail is only as strong as its distance from the reach of the agent it
polices. Grades, weakest to strongest — the names follow vsdd-cli's current
design (its 2026-09-18 vocabulary alignment renamed the ladder this document
was first written against; the old names are kept as parenthetical aliases
until the upstream registry lands its deprecation entries; synced 2026-09-19,
#194):

- **detective control** (formerly "detection") — reads and reports; cannot
  block. A finding, a lint.
- **bypassable preventive control** (formerly "friction") — a bypassable local
  check that records the deviation. A git hook: it runs at the act, but the
  agent controls local state, so it is skippable (`--no-verify`) — the point
  is not to make the deviation impossible but to make it explicit and owned.
- **enforced preventive control** (formerly "CI-backed block") — server-side,
  unskippable: a **required status check** over the range CI can independently
  verify. CI re-derives the check over server-synced state and over the
  checker's own wiring, so it holds even when the local checker was edited: CI
  is a control plane the policed agent does not own.

**Tamper-proof** enforcement is beyond all three grades: holding against an
agent that controls all local state, including the checker and the tests,
needs an independent corroboration oracle the agent cannot author (vsdd-cli
#815), or capability restriction (a reviewer that literally lacks the mutation
tools). Neither is shippable here yet — it is the honest boundary.

The load-bearing correction (#99): a **runtime-harness** stop hook (Claude Code)
is bypassable-preventive at best — it is in-repo code whose own script and
wiring are editable, so it cannot self-protect. Shipping it as a block would
assert enforcement it cannot deliver; vsdd-cli saw exactly this fail live (two
edit-time gates silently de-wired by an init settings clobber, uncaught for
weeks). So mdatron ships **no session-stop block**. The durable third leg is
**CI-tested integrity**, not a session-stop hook.

Mechanisms in hand: mdatron's own engine (patterns and rules over the governed
markdown); git hooks under `.githooks/` (bypassable preventive); and CI, with a
precision the first draft of this document overclaimed (#170 methodology-docs
review finding 1): **only the self-validation job is a required status check on
`main`**. The two enforced-preventive legs therefore both live inside that job
— `mdatron verify --deny-warnings` (the register) and the amendment-citation
range scan (below). The guardrail seeds (`tests/methodology_seams.rs`) run in
the `Test` job, which is **not** required — at the merge boundary they are
detective, not a block, until the operator adds `Test` to the required checks
(tracked, #170). Calling them a block before that would be the exact overclaim
this grade ladder exists to prevent.

## Guardrails

Each guardrail is named for the discipline it binds — no coined labels
(methodology §5).

| Guardrail | Discipline | Grade / mechanism | Status |
|---|---|---|---|
| amendment citation | a DESIGN amendment cites its ratified review (§3) | bypassable preventive — `.githooks/commit-msg` — **plus an enforced-preventive leg**: the required self-validation job re-derives the two citation checks over every DESIGN-touching commit in the pushed range (the seeds additionally pin the hook script itself, in the unrequired `Test` job) | bound |
| governed-estate register | the naming register holds over the methodology docs (scope: `docs/methodology*.md` per `vocabulary_globs`; `DESIGN.md` is outside `file_globs`) | enforced preventive — `mdatron verify --deny-warnings` self-validate | bound — **letter-plus-number clusters only** (`E0091`): §5's coined-label and all-caps prohibitions are mechanized by nothing yet and hold as conduct |
| phase discipline | each layer runs the phases; an exit is a boundary commit plus a typed result comment (§1) | honest boundary — phase-answer derivation and the red-green gate commands need vsdd-cli (its status and init queries have since shipped upstream); the recording vehicle (issue lanes, boundary commits, typed comments) is conduct with no detection path | deferred (conduct) |
| review spend shape | a cold-review composition declares its fan-out shape and hard agent-count ceiling before dispatch, and refutation happens across rounds, never as a per-finding verifier fan-out (§1 phase 3; synced from upstream's declaration-completeness gate, #194 — the discipline was founded on this project's own overspend incidents) | convention — declared in the phase-3 plan comment; no check reads it here yet (upstream gates it at ratification) | conduct |
| routing before fix-close | a fix-close finding carries a prior routing plan (§2) | **available upstream for adoption** (#194): vsdd-cli shipped `vsdd gate` and its routing-gate CI workflow (exit 0 pass / 1 blocked / 2 fail-closed) — the check this row waited for exists; adopting it here is an open operator decision. Until adopted: held by conduct | shipped upstream, unadopted |
| round-result reconciliation (upstream's name; formerly "round-result parity") | a phase-exit result comment reconciles against the child issues it closes over (§1's result-comment discipline) | **available upstream for adoption** (#194): the reconciliation query ships in vsdd-cli's status surface. Until adopted: held by conduct | shipped upstream, unadopted |
| finding ownership | findings carry an owner and a validator (§4) | planned — crosslink-querying check (vsdd-cli boundary, tracked #170); no detection path today | planned |

**Binding is per-clone (#96).** The `.githooks/commit-msg` bypassable-preventive leg is active only
where `git config core.hooksPath .githooks` is set — a LOCAL git config a fresh
clone does not inherit (it is untracked, so a full re-clone drops it silently).
Re-run it after cloning; a durable auto-bind is requested upstream
(crosslink#99). The local hook is the *weaker* half of the seam by design. What an
unbound clone loses and keeps (#170 review finding 2 corrected the first draft here):
it loses the commit-time nudge AND can write uncited amendments into its local
history — the seam seeds never look at commits, only at the checker script. The
merge-boundary guarantee is held by the required self-validation job's
amendment-citation range scan, which re-derives the citations over the pushed
commits themselves: an uncited amendment can exist locally but cannot merge.

## Regression corpus (the escape seeds)

(Upstream's current name — its 2026-09-18 vocabulary alignment registered
*regression corpus* for what this document first called the escape corpus;
synced 2026-09-19, #194. At the stopgaps' retirement the corpus splits: the
escape seeds migrate to vsdd-cli's regression corpus, standing deviations to
its exception register — tracked on the retirement checklist.)

Each escape is a dodge the guardrails must catch; the escapes are the negative
fixtures. A guardrail set that misses any escape is insufficient. Each escape
names the grade that catches it — an advisory with no binding grade is itself
the first escape.

| The dodge | Caught by |
|---|---|
| advisory only — a detection with no binding grade is treated as sufficient | this table itself: every row names its binding grade **or its open gap** — a row claiming coverage it lacks is this escape in disguise (#170 review finding 4 caught three doing exactly that) |
| fix in place — a fix written with no prior routing | **the durable catch has shipped upstream** (vsdd gate + routing-gate workflow) and awaits adoption here (#194); until adopted, the work-check hook reminds but does not refuse (see Notes) and the discipline holds as conduct |
| disposition relabel — a fix-close relabelled to dodge routing | same upstream guardrail as the row above, shipped and unadopted (#194); a result or decision closing a fix-lane must still cite a routing plan, and nothing here checks it until adoption |
| ownerless fix — a fix with no owning finding filed | partial — the work-check hook's issue-before-code refusal is its one binding half (grade 2); the finding-ownership half is planned (#170) |
| forged record — a self-authored gate or composition record | amendment citation, with its residual named (#170 review finding 5): the friction and the CI range scan match citation **keywords** — a self-authored message that types "ratified #N" passes both; verifying the cited review exists and is external is deferred to the corroboration oracle (vsdd-cli #815), the same honest boundary as the row below |
| checker edit — the checker itself is edited to pass | CI-backed block: the guardrail seeds (`tests/methodology_seams.rs`) re-derive integrity over the checker's own wiring, so a committed checker-edit surfaces as CI-red independently of the edited local checker. The bypassable-preventive leg (the commit-time hook) records the honest-path deviation; the locally-skippable residual is owned-bypass (`--no-verify`, the deviation is yours to record). The tamper-proof residual — an edit that also fools CI — is named to the corroboration oracle (vsdd-cli #815), the honest boundary; no in-repo guardrail reaches it. |

## Owned-bypass stance

Following vsdd-cli: a bypassable-preventive guardrail does not pretend a bypass is impossible
(for in-repo code it never is). It makes the honest path frictionless, the
bypass explicit, and the deviation self-recorded — `git commit --no-verify` is
available for a genuine emergency, and the deviation is yours to record. The
durable verification lives in CI, not in the local hook.

## Notes

- Guardrails are additive to the existing binds, stated at their real strength
  (#170 review finding 3): the PreToolUse work-check hook **refuses** code edits
  without an active issue (its one binding half, bypassable preventive) and
  **reminds** about the plan/result comment discipline — the live config runs
  `comment_discipline: encouraged`, and agent contexts run relaxed with the
  discipline off, so it is a nudge, not a catch (the record shows lanes closed
  without result comments under it). Promoting it to `required` — and deciding
  the agent-override posture deliberately — is an open operator decision
  (#170).
- The escape corpus is a living set: a real deviation that slips through is a
  new seed and a guardrail gap to close.
