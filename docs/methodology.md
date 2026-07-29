# mdatron development methodology

mdatron develops itself under the vsdd methodology — the discipline, not the
tool (vsdd-cli is not yet at alpha). Operator directive 2026-07-28, standing
until vsdd-cli ships. Two halves: conduct (below) and mechanized enforcement
(`docs/methodology-enforcement.md` and the bound seams).

This document is itself governed: changes to it re-enter the phase flow under a
declared composition and cold review, with operator ratification as the exit
(see § 3).

## Conduct

### 1. Phase discipline

Each layer of work runs the phases, which are crosslink milestones; a phase
exit is a boundary commit plus a typed result comment on the issue:

- **1a design** — the increment's contract, with the internal spec-review loop.
- **1b verification architecture** — how it will be proven (the seams/gates).
- **1c decomposition** — the child lanes.
- **2a red gate** — the failing test demonstrated against the defect/gap first.
- **2b implement to green** — the minimal implementation.
- **2c exit gate** — refactor while green; the exit boundary commit.
- **3 adversarial cold review** — independent, multi-lens, no prior context.
- **4 route findings** — see § 2.
- **5 harden** — close the routed findings.
- **6 exit** — the milestone closes with evidence.

### 2. Route findings before fixing

The load-bearing lesson. Every review finding routes to the phase that would
have prevented it *before* any fix is written. A fix-close with no prior
routing plan is malformed. Never grind a fix in place: file the finding, route
it to its owning phase, then fix.

### 3. Owned, reviewed spec changes

The governing DESIGN is never hand-edited solo. A spec change re-enters phase
1a under a declared composition (which domains/lenses review it) and a cold
multi-lens review. Operator ratification is the **exit** of that process — not
a substitute for it. A DESIGN commit cites the ratified review that authorized
it.

### 4. Crosslink discipline

- Issue before code (no code-touching work without an active, linked issue).
- Typed comments (`plan` / `result` / `decision` / `observation` / `note`).
- Findings carry an owner and a validator.
- Closures carry evidence or an explicit disposition.

### 5. Naming and register

No coined labels, letter-clusters, or all-caps tokens in prose or identifiers.
Plain names. A durable term is a registration act (it enters a registry), not a
self-serve coinage. mdatron's own vocabulary family enforces this class for
governed artifacts; the same discipline applies to mdatron's development.

## Honest boundary

The parts that need vsdd-cli — phase-answer derivation, process-integrity
status queries, the mechanized gate commands — are not shippable yet. Those
stay conduct plus cold review until vsdd-cli ships. Everything else is
mechanized now (enforcement doc). Do not wait on the tool for the rest.

## Enforcement, in one line

Conduct alone drifts. The disciplines above are bound — by mdatron's own engine
over the design and review-log estate, a git pre-commit hook, and a
session-stop hook — so that a deviation produces compiler-shaped feedback at the
act, not a latent record. The escape corpus (`docs/methodology-enforcement.md`)
is the regression-seed set: if the seams do not catch each escape, they are
insufficient.
