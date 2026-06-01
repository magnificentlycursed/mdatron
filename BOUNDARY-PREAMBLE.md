# DESIGN-MDATRON — Boundary Preamble

**Status:** Step 0.5 artifact. Precedes substantive DESIGN-MDATRON authoring (Step 1). Closes the boundary questions raised by SA, DR, PE, and Security in the 2026-06-01 multi-domain review.

**Provenance:** Authored to address SA-F1 through SA-F4, DR-F1, DR-F3, PE-F4, SEC-F1, SEC-F2 from [`mdatron/review-log/2026-06-01-solution-architect.md`](./review-log/2026-06-01-solution-architect.md).

**Reading guidance:** Eight declarations follow. Each closes an open boundary question raised in the review. Disagreements should be raised + revised here BEFORE Step 1 begins; once Step 1 is authoring, these are load-bearing.

---

## 1. Repo-tree position

DESIGN-MDATRON.md and sibling design documents live at the root of the `mdatron-cli` repository, NOT inside `vsdd-cli/`. The mdatron repo is a standalone Rust workspace publishing the `mdatron` crate.

**Rationale (per SA-F2 + SO-F1):** mdatron's design must serve non-VSDD adopters first-class. Living inside vsdd-cli's repo would (a) signal mdatron is vsdd-cli-internal; (b) couple their release cycles; (c) create unavoidable cross-references-as-paths from non-VSDD adopters' perspective.

vsdd-cli's existing DESIGN docs continue to reference mdatron via cross-repo URL citation (see § 4).

## 2. Crate workspace shape

mdatron ships as a 2-crate workspace; the LSP server folds into the CLI binary for v1:

| Crate | Owns | Distribution |
|---|---|---|
| `mdatron-core` | Validator engine; DSL parser + evaluator; JSON Schema dispatch; cross-file indexing; error catalog format; SARIF emission; anchor-ID derivation; bypass-marker parsing; delegate-protocol library | Library; published to crates.io; consumed by other crates including vsdd-cli at Step 5 |
| `mdatron-cli` | Subcommand dispatch (`mdatron verify`, `mdatron verify hook`, `mdatron explain`, `mdatron lsp-serve`, `mdatron schema`); CLI ergonomics; output formatting | Binary; `cargo install mdatron`; published to crates.io |

LSP server lives as `mdatron lsp-serve` subcommand within `mdatron-cli`. Promoted to a separate `mdatron-lsp` crate only if v1+ evidence (IDE-extension complexity; alternative LSP client implementations) justifies splitting.

vsdd-cli at Step 5 depends on **`mdatron-core` as a Rust library**, not on the CLI binary. The CLI binary is consumed by adopters via `cargo install mdatron`; vsdd-cli embeds the library for in-process invocation. This dual-surface pattern matches `clap` / `clap_builder`, `tower` / `tower-http`, etc.

## 3. Cross-DESIGN-doc coordination

DESIGN-MDATRON's frontmatter declares the consume/produce graph:

```yaml
schema_class: design-doc
schema_version: 1.0.0
doc_class: design
version: 0.1.0
consumes_from: []         # mdatron is methodology-agnostic — formal consume graph empty
produces_for:
  - vsdd-cli/DESIGN-SCHEMA          # vsdd-cli's DESIGN-SCHEMA cites mdatron's schema-format conventions
  - vsdd-cli/DESIGN-VERIFICATION    # vsdd-cli's DESIGN-VERIFICATION cites mdatron's hook composition + LSP integration
last_revision_trigger: Step 0.5 boundary preamble
```

The asymmetry is deliberate: mdatron's design must not assume VSDD; vsdd-cli's design DOES assume mdatron (after Step 5).

**Informal upstream influence (acknowledged but not declared as consume):** mdatron's DSL was informed by VSDD's review evidence (the 8 pattern-classes from late-detection review). This influence shaped the DSL's coverage scope but is NOT a formal cross-doc dependency — mdatron's content is decidable from its own first principles, with VSDD as a worked example.

vsdd-cli's existing DESIGN docs add `consumes_from: [mdatron://DESIGN-MDATRON]` entries at Step 2 (scope-adjust).

## 4. Cross-repo citation convention

Citations between repos use one of two forms:

**Stable URL form (v1; mandatory):**

```markdown
[DESIGN-MDATRON § Delegate protocol](https://github.com/<org>/mdatron-cli/blob/v1.0.0/DESIGN-MDATRON.md#delegate-protocol)
```

Version-pinned URLs only. No HEAD-pointing citations — they rot silently when the target doc reorganizes.

**`mdatron://` scheme form (reserved for v1+):**

```markdown
[DESIGN-MDATRON § Delegate protocol](mdatron://DESIGN-MDATRON#delegate-protocol)
```

Resolution: mdatron itself emits a manifest at release time mapping `mdatron://` URIs to versioned GitHub URLs. Consuming tools (vsdd-cli's link-resolver hook; LSP go-to-definition) translate via the manifest.

v1 ships only the URL form. The `mdatron://` scheme is reserved here so the convention is non-conflicting when adopter-tooling emerges.

## 5. Artifact class split

vsdd-cli's 13 artifact classes split into VSDD-specific (stay in vsdd-cli) versus generalizable (ship as mdatron-examples):

| Class | Location | Rationale |
|---|---|---|
| Review entry | vsdd-cli | `lens`, `sycophancy_compensation`, `execution_method` are VSDD-methodology terms |
| Finding | vsdd-cli | Per-domain classification universe, validator-pair semantics |
| Phase primer | vsdd-cli | Phase enum is VSDD's 10-phase taxonomy |
| Domain prompt | vsdd-cli | 18-domain set with VSDD's tier/activation semantics |
| Supplement | vsdd-cli | Tied to VSDD's domain set |
| Methodology event variants | vsdd-cli | 18 VSDD-specific event types |
| `.vsdd/config.yaml` | vsdd-cli | Per-feature axes are VSDD's discipline |
| Exit Signal record | vsdd-cli | 4-dimension convergence is VSDD-canonical |
| **DESIGN doc** | **mdatron-examples + shared-pattern** | Generic "design document with consume/produce graph" — useful template for any architecture-doc repo |
| **Methodology spec section** | **vsdd-cli (entirely)** — mdatron-examples deferred to v1.1 (resolved 2026-06-01) | Per SO-F1 (speculative), avoid pre-building schema-composition machinery (JSON Schema `allOf` extension + anchor-derivation-through-composition + rule-references-to-inherited-fields) for hypothetical non-VSDD adopters; revisit when real adoption evidence surfaces the need. mdatron's reusable value at v1.0 is the `key()` cross-file lookup mechanism, not a curated schema library |
| **Manual-test** | **mdatron-examples** | "Test class + falsifiability check" generalizes |
| **PR template** | **mdatron-examples** | Generic PR-template validation is a common need |
| **CHANGELOG (structural)** | **mdatron-examples** | Keep-a-Changelog validation is widely applicable |

mdatron ships the 4 generalized classes (DESIGN doc, Manual-test, PR template, CHANGELOG) as `mdatron-examples/` reference schemas — not enforced by default, but available for adopters to enable per-project via `.mdatron/config.yaml`.

**Open-for-revision row resolved 2026-06-01:** the "Methodology spec section" row resolved to vsdd-cli (entirely) with mdatron-examples deferred to v1.1. mdatron-examples v1.0 ships the 4 clearly-generic classes (DESIGN doc, Manual-test, PR template, CHANGELOG) only.

## 6. Install-order discipline

```
cargo install crosslink         # crosslink first (substrate)
cargo install mdatron           # mdatron second (validator engine; methodology-agnostic)
cargo install vsdd              # vsdd third (composes against both)

cd <project>
crosslink init
vsdd init                       # vsdd init pre-flight checks for crosslink AND mdatron
```

`vsdd init --check` fails with explicit error if mdatron is not installed:

```
error[VSDD-E0221]: mdatron-not-installed
  vsdd init requires mdatron >= 1.0; not found on PATH.
  install via: cargo install mdatron --locked
```

**Reserved error code:** `VSDD-E0221` (parallel to existing `VSDD-E0220: existing-file-malformed-refuse-to-overwrite`). Reservation declared here; full catalog entry authored at Step 2.

## 7. Path-confinement discipline (delegate registrations + key() sources)

**Delegate registrations** at `.mdatron/delegates/*.yaml` are loaded only when:

- The delegate file is committed to the project's canonical tree (signed by git history)
- The delegate's `command:` field points to a binary either (a) reachable on `$PATH` (operator-trusted) OR (b) under the project root via relative path

PR-modifiable delegate files MAY register new delegates; mdatron emits a warning when first encountered:

```
warning[MDATRON-W0030]: delegate-registered-from-pr-modifiable-path
  delegate `<name>` registered in PR-modifiable file; will not execute until trusted.
  to trust: add the delegate's content-hash to .mdatron/delegates-trusted.yaml (operator-only)
```

Execution requires explicit operator opt-in via signed `.mdatron/delegates-trusted.yaml` listing approved delegate content hashes. PR cannot self-trust its own delegate registrations.

**`key()` sources** in pattern files MUST resolve under the project root:

- Absolute paths rejected at config load: `MDATRON-E0010: key-source-absolute-path`
- Parent-traversing relative paths (`..` segments) rejected: `MDATRON-E0011: key-source-parent-traversal`
- Symlinks not followed; `lstat` checked at load time
- Glob patterns may not contain `..` segments

**Reserved mdatron error codes:** `MDATRON-E0010`, `MDATRON-E0011`, `MDATRON-W0030`. Full catalog entries authored in DESIGN-MDATRON at Step 1.

Closes SEC-F1 + SEC-F2.

## 8. Co-evolution discipline (closes SA-F1)

DESIGN-MDATRON and vsdd-cli's existing DESIGN docs evolve as **siblings**, NOT as upstream/downstream. The co-evolution discipline:

- **Joint-pass commits**: a single Phase 4 → Phase 1a routing commit may touch BOTH repos when boundary-spanning decisions land (cross-doc coordination changes; class-split table revisions; install-order updates).
- **Cross-repo PR pairing**: when a change in mdatron requires a corresponding change in vsdd-cli, both PRs reference each other in description bodies + neither merges before the other passes CI.
- **Version-coordination event**: `MdatronVersionPinned` (new methodology event variant; declared in vsdd-cli's DESIGN-OBSERVABILITY at Step 2) records the mdatron version vsdd-cli pins against.
- **Drift hook extension**: methodology-version-drift hook (`VSDD-W0200`) extended to also check mdatron-version-drift between vsdd-cli's pin and the installed mdatron.

This closes SA-F1's open question. The discipline is now declared; Steps 1 and 2 can interleave under it.

---

## What this preamble does NOT decide

The preamble closes boundary questions only. Substantive design decisions defer to Step 1 (DESIGN-MDATRON authoring):

- DSL syntax details (operators, function signatures, type system depth)
- Cross-file index implementation (cache invalidation; mtime tracking; LSP hot-reload)
- Diagnostic output formats beyond TTY + SARIF + JSON
- Per-built-in-pattern semantics
- Delegate protocol envelope versioning policy
- Schema-source-of-truth language choice (JSON Schema only? Other formats accepted as input?)
- ~~v1 ship-criterion (deferred to SO per SO-F2)~~ — **CLOSED 2026-06-01:** see [`V1-SHIP-CRITERIA.md`](./V1-SHIP-CRITERIA.md). v1.0 cut ships Phases A+B+C+F (CLI + agent integration + onboarding); LSP and MCP held for v1.1.
- LSP feature scope (deferred to PE-F2 + AIE-F1) — partially answered by v1.0 cut (no LSP in v1.0); detailed feature scope still TBD when v1.1 begins

When Step 1 begins, the preamble's 8 declarations are foundational; everything else is open.

---

## Acceptance criteria for closing Step 0.5

Step 0.5 closes when:

1. **SO answers**:
   - ~~SO-F1 (named 2-3 concrete adopter candidates OR accepted the inversion as speculative with sized cost)~~ **CLOSED 2026-06-01:** accepted as speculative. Cost-sizing implication: mdatron-examples library stays small at v1.0 (clearly-generic classes only); design proceeds assuming VSDD may be the sole adopter for 12–18 months.
   - ~~SO-F2 (declared v1 ship-criterion)~~ **CLOSED 2026-06-01 — see [`V1-SHIP-CRITERIA.md`](./V1-SHIP-CRITERIA.md)**.
   - ~~SO-F3 (chosen throwaway-vs-mdatron-shaped for Step 3)~~ **CLOSED 2026-06-01:** build-in-mdatron-shape-from-day-one. Step 3 implements `vsdd init` + `mdatron-core` simultaneously; vsdd-cli depends on `mdatron-core` as a local path dependency from Step 3 onward. Step 5 becomes "switch from local-path-dep to published-crate-dep" rather than a refactor wave.
2. **Cross-DESIGN-doc coordination table mirrored** in vsdd-cli's DESIGN-SCHEMA and DESIGN-VERIFICATION (added by the same Phase 4 commit that lands this preamble's adoption)
3. **Methodology amendment for `MdatronVersionPinned` event variant + `VSDD-W0200` extension is queued** for Step 2 (declared as a deliverable; not implemented yet)
4. **The Bootstrap-Period Mitigation Plan** ([`BOOTSTRAP-MITIGATION.md`](./BOOTSTRAP-MITIGATION.md)) is also drafted and reviewed

Then Step 1 begins.
