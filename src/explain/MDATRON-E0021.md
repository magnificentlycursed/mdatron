# MDATRON-E0021 — undeclared-field-reference

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A DSL rule references a `$self.<field>` path that names an **undeclared property
under a closed schema**. The rule's context binds `$self` to a known
`schema_class`; that schema is a **closed object** (`additionalProperties:
false`); and a segment of the path is not among the schema's declared
`properties`. The reference would read as *absent* at evaluation — so the
assertion mis-fires or passes vacuously against every governed document, and the
rule silently protects nothing.

This is a **load-time, project-level** check (adopting Cedar's
validate-before-deploy posture): it runs once before any document is walked and
hard-gates the run (exit 1), rather than letting a typo'd field reach production
as a quiet no-op.

The check is **deliberately conservative** — it flags only what it can prove is a
typo. It examines a rule only when its context resolves to a *loaded*
schema_class, walks only `$self`-rooted field chains (never let-bindings,
`$file`, `$project`, or quantifier variables), and flags a path only when a
segment is provably absent from a **closed** object's `properties`. Any
undecidable shape — an open object (the JSON-Schema default), an array, a
`$ref`, a combinator (`allOf`/`anyOf`/…), or a level with no `properties` — is
left unflagged. A rule over a path-glob context (whose `$self` schema is not
statically known) is not checked at all.

## How to fix

- **A typo'd field name.** Correct the segment named in the `reference` region
  (e.g. `$self.ownr` → `$self.owner`) so it matches a declared property.
- **A field that should exist.** Add the property to the schema — either declare
  it under `properties`, or, if the object is meant to carry open-ended keys,
  relax `additionalProperties` from `false` (which also removes it from this
  check's scope, since only closed objects are validated).
- **A reference into a nested closed object.** Verify each segment of the dotted
  path resolves through the schema's `properties` tree; a wrong intermediate
  segment fails at the first undeclared closed level.
