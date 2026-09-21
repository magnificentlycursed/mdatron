# FAQ

Comparison questions first — the standing credits they draw on are collected in
[Influences & acknowledgments](#influences--acknowledgments) at the end.

## How is mdatron different from Schematron?

It mostly isn't different in *posture*, and that is deliberate. Assertion-based
validation of documents — rules with a context, evaluated over a tree, reporting
what failed *and what was checked* — is a posture Schematron (ISO/IEC 19757-3)
has proven against XML for over two decades. mdatron brings that posture to
typed markdown, for a consumer Schematron never had to serve: coding agents that
read the diagnostics as structured input. The differences follow from that
consumer — a closed, SemVer-versioned JSON envelope; byte-deterministic output;
trust-marked quoted regions; and a rule language deliberately narrower than
XPath. Schematron's SVRL reporting — saying what was *checked*, not only what
failed — is the direct ancestor of the envelope's per-family activity tri-state.

## Is the JSON envelope SARIF? Why not just emit SARIF?

No, and we audited SARIF (OASIS 2.1.0) before designing rather than after.
Several envelope decisions adopt SARIF postures on purpose: findings are kept
separate from tool-condition reporting (SARIF's results vs. notifications
split), per-finding `fingerprint` follows the `partialFingerprints` idea of a
line-churn-stable identity (with a documented divergence: platform-variant
engine prose is excluded so one defect fingerprints identically across
operating systems), and the required `envelope_schema` field is SARIF's
`$schema` pin. We did not adopt SARIF wholesale because the envelope carries
contracts SARIF does not: family activity tri-states with reasons, quoted-region
trust marking for adopter bytes, and a byte-determinism guarantee for the
default envelope. Those are load-bearing for agent consumers and would be
conventions, not contracts, inside SARIF.

## What did Ruff contribute?

The registry discipline: a diagnostic-code registry as the *generator* of
derived artifacts, so drift is impossible by construction rather than caught by
review. mdatron's golden code catalog — generated from the explain pages — is
the full application of that idiom; the tripwire that forces `docs/limits.md`
to match shipped data is the weaker cousin (drift caught at test time rather
than made impossible), and its promotion to a generated table is tracked.

## Why not a policy language like Cedar or OPA?

We audited both for the rule-language design. What carried over is their
*validation* posture more than their languages: validate rules before deploy
(mdatron parses every rule expression at load, not first use), and refuse what
you cannot faithfully evaluate rather than reinterpreting it (the same doctrine
behind refusing non-2020-12 schema dialects loudly instead of silently
revalidating them under different semantics). The DSL itself stays deliberately
small — a conformance rule language, not a general policy language.

## Why do the errors look like rustc's?

Because rustc's diagnostics are the most effective teaching errors in any
mainstream toolchain, and imitating a proven pedagogy beats inventing one.
The shape is borrowed knowingly: coded errors with location and help text, and
`mdatron explain <code>` as the `rustc --explain` analog — every code ships an
explain page, enforced in CI.

## What shaped the run-telemetry parts of the envelope?

*Observability Engineering*, 2nd edition (O'Reilly), which we evaluated as a
design input and partly rejected. Adopted: input lineage digests (attest which
configuration produced a verdict), flag-gated run timings, and stable finding
identities for cross-run trending. Rejected: the wide-event doctrine of adding
context freely — a closed SemVer envelope buys fields one at a time, and the
default envelope's byte-determinism outranks telemetry width. The evaluation,
including the rejections, is recorded in the project's review history.

## How do I build a tool on top of mdatron?

Compose it as a subprocess, never as a library: spawn `mdatron verify --json`
and parse the single JSON object on stdout against the published envelope
schema (`mdatron schema` prints it; pin the `envelope_schema` field's `$id`).
The envelope carries everything a wrapper needs — pipeline status with an
in-band failure reason, per-family activity so "checked nothing" is never
mistaken for "clean", trust-marked quoted regions, and line-churn-stable
fingerprints — and its versioning is SemVer'd independently of the crate, so
a wrapper pins the contract it was built against. Keep diagnostic namespaces
strictly separate: mdatron emits `MDATRON-*` codes; your tool emits its own
prefix (declare it in a code catalog and mdatron will keep your corpus
honest about it). Ship your schemas and patterns as files your installer
drops into the adopter's `.mdatron/` — the methodology lives in data, the
engine stays generic. This is the pattern mdatron's first downstream consumer
uses, and every design decision in the envelope was made with a wrapper in
mind.

## What are crosslink and vsdd?

Development infrastructure and method, not parts of mdatron. mdatron is
developed under the VSDD methodology, authored by Dollspace
([whitepaper](https://gist.github.com/dollspace-gay/d8d3bc3ecf4188df049d7a4726bb2a00)):
specification before implementation, independent verification, and cold-context
adversarial review before anything merges.
[vsdd-cli](https://github.com/magnificentlycursed/vsdd-cli), a sibling project
by mdatron's owner, is the methodology harness implementing that whitepaper.
Work is tracked with
[crosslink](https://github.com/Corvidae-Coding-Projects/crosslink) (MIT), a
local-first issue tracker and knowledge base built for AI-assisted development,
also by Dollspace; we use it daily and file issues upstream. None of these ship
in this crate — the package payload is allowlisted and CI fails if internal
apparatus leaks in.

## Is mdatron affiliated with the tools and standards it validates or cites?

No. mdatron is an independent project. Names such as GitHub Copilot, Claude
Code, Anthropic, OASIS, ISO, and O'Reilly appear only to describe compatibility,
lineage, or citation, and imply no affiliation or endorsement.

## Influences & acknowledgments

mdatron's design practice is to audit a mature reference before designing each
subsystem, and to record what was adopted *and what was rejected, with
reasons* — the rejections are what make it curation rather than imitation. The
audit records live in the project's design history (`DESIGN.md` citations and
the review log). With gratitude:

- **[Schematron](https://github.com/Schematron/schematron)** (ISO/IEC 19757-3,
  Rick Jelliffe and the DSDL community) — the assertion-over-documents posture
  and SVRL's what-was-checked reporting.
- **[SARIF 2.1.0](https://github.com/oasis-tcs/sarif-spec)** (OASIS) — results
  vs. notifications, `partialFingerprints`, the `$schema` contract pin.
- **[Ruff](https://github.com/astral-sh/ruff)** (Astral) —
  registry-as-generator; drift impossible by construction.
- **[Cedar](https://github.com/cedar-policy/cedar)** (AWS) and
  **[Open Policy Agent](https://github.com/open-policy-agent/opa)** —
  validate-before-deploy and refusal-over-reinterpretation.
- **[lychee](https://github.com/lycheeverse/lychee)** — reference for the
  link-check family's scope decisions.
- **[rustc](https://github.com/rust-lang/rust)** (the Rust project) — the
  diagnostic format and the `--explain` pedagogy.
- ***Observability Engineering*, 2nd ed.**, by Charity Majors, Liz Fong-Jones,
  George Miranda, and Austin Parker (O'Reilly, ISBN 9781098179922) — the
  telemetry design inputs above, evaluated against this project's determinism
  constraints. See [the book's page](https://www.honeycomb.io/observability-engineering-oreilly-book);
  buy it from [your local bookstore](https://bookshop.org/book/9781098179922)
  or [Amazon](https://www.amazon.com/dp/1098179927).
- **[crosslink](https://github.com/Corvidae-Coding-Projects/crosslink)** (MIT,
  by [Dollspace](https://github.com/dollspace-gay)) — the issue tracking and
  knowledge infrastructure this project is developed with, and an upstream we
  contribute issues to.
- **The VSDD methodology**
  ([whitepaper](https://gist.github.com/dollspace-gay/d8d3bc3ecf4188df049d7a4726bb2a00),
  by [Dollspace](https://github.com/dollspace-gay)) — the development
  discipline itself.
  [vsdd-cli](https://github.com/magnificentlycursed/vsdd-cli), by mdatron's
  owner, is the methodology harness implementing the whitepaper; mdatron is the
  discipline's first self-hosted proving ground.
