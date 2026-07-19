# mdatron v1 Ship Criteria

**Status:** transitional coordination document (operator ruling 2026-07-19: standing documents are README and DESIGN; coordination artifacts retire when superseded by design or converted to tracked work). Originally decided 2026-06-01 (Step 0.5); re-cut 2026-07-19 under the ratified respec (vsdd-cli `.design/agent-first-vsdd-toolkit.md` @ a39c3eb). Retires at the design increment's ratification: the acceptance criteria become a crosslink v1.0 milestone with tracked issues, the waiver record migrates to the falsifiability report, and the decision history stays in git.

## Decision

v1.0 ships mdatron as a CLI with agent-loop integration. The primary consumer is an agent receiving diagnostics at action time (hooks) and boundary time (gates); the human TTY surface is secondary. LSP and MCP surfaces are reserved namespaces, deferred past v1: MCP servers are evidence-provisioned per the respec's efficiency-advisory rule, and LSP waits on adopter evidence.

| Area | v1.0 includes | Deferred past v1.0 |
|---|---|---|
| Engine | frontmatter parsing; JSON Schema dispatch; DSL parser + evaluator scoped to cross-file and registry validation; path-confined `key()` index | body-content extraction (gated on a re-run falsifiability test, below) |
| Conformance checks | the design-increment families: state-artifact schema and consistency checks, route-table validation (closed-world), content-hash pin checks, registry-driven vocabulary checks, citation-vs-HEAD verification | — |
| CLI | `verify` (TTY + JSON + compact output), `explain` (TTY + JSON + compact), `init` (deploys `.mdatron/` self-validation skeleton; pre-flight check) | SARIF output; registry subcommands beyond what the conformance checks require; `skill-preamble` (superseded — the methodology layer generates agent context) |
| Agent integration | hook-invocable invocation contract (PostToolUse and gate usage documented for adopters); compact output sized for agent context | MCP tools (evidence-provisioned); LSP (adopter evidence) |
| Error catalog | catalog grows one entry per emitted code, each with an explain page and fixture pair; no seeded-count target | — |
| Release | `cargo install mdatron` from crates.io on Linux and macOS (x64/arm64); `Cargo.lock` committed | cosign/SLSA signing pipeline; per-platform binary releases |
| Documentation | README matching the implemented surface; DSL reference for the implemented operators; adopter onboarding for the init-and-hook path; the vsdd adoption as the worked example | second (non-VSDD) worked example — on adopter evidence |

## v1.0 acceptance criteria

v1.0 is ready when all of the following hold:

1. Engine and CLI: `verify`, `explain`, and `init` functional; all unit and CLI integration tests pass; compact output implemented for `verify`.
2. Conformance-check families shipped per the design increment's ratified contract, with its acceptance fixtures passing.
3. Self-validation: `mdatron verify` runs clean on the mdatron repo itself using its own `.mdatron/` config — this is also the bootstrap-period exit gate (see below).
4. Falsifiability: the cold-context DSL falsifiability test re-run against the narrowed cross-file/registry specification, meeting the 80% bar. See the waiver record below.
5. Publish: `cargo install mdatron` succeeds from crates.io on the four platform targets.
6. Documentation per the table above, verified by operator read and one external cold reader (per the respec's review-budget rule for prose).

## Falsifiability waiver record

Waiver (recorded 2026-06-01, re-baselined 2026-07-19): the original falsifiability run scored 43% (3/7) against the 80% bar over the *full* DSL specification including body-content functions, and was dispositioned revise-without-retest — a waiver of a ship criterion. The ratified respec narrows the DSL's specified scope to cross-file and registry validation; the criterion is re-baselined to that scope and the test must be re-run against it (criterion 4 above). Body-content extraction remains out of scope until a falsifiability run over a body-content specification meets the same bar. This paragraph is the greppable waiver record; it migrates to the canonical waiver marker when the methodology layer ships it.

## Bootstrap-period exit

The bootstrap period (BOOTSTRAP-MITIGATION.md) exits when acceptance criterion 3 holds: `mdatron verify` clean on this repo via its own config. At that point `bootstrap-tooling/bootstrap-validator.py` is deleted per its deletion criterion, and the exit is recorded per Mitigation 3. The coinage-log obligation (Mitigation 2) is discharged into the methodology layer's vocabulary registry per the respec's maturity lifecycle; the prescribed `coinage-log.md` file was never created, and that gap is recorded here rather than backfilled.

## v1.1+ triggers

- LSP: two or more adopters requesting real-time editor diagnostics the hook surface cannot provide.
- MCP: the cost ledger shows repeated structured-query demand the CLI cannot ergonomically serve (evidence-provisioned per the respec).
- Body-content DSL: recurrence evidence for body-content validation needs, plus the falsifiability gate above.
- Operator directive at any time, with rationale.

## Coordination

Per BOUNDARY-PREAMBLE § 8, the methodology layer's design (the ratified respec) is the co-evolution counterpart of this cut. mdatron remains methodology-agnostic: the conformance-check families are generic engines; schemas, patterns, route tables, and registries are supplied by the adopter.
