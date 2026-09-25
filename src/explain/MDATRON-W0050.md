# MDATRON-W0050 — comparison-dead-clause

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A rule compares a `$self.<field>` against a literal that is **not among the
field's declared `enum`** — for example `$self.status == "archived"` where
`status` is declared `enum: ["draft", "final"]`. No conforming document
can carry `archived` (the schema forbids it), so the comparison has the same
value for every document the rule ever sees: the clause is constant, it never
varies, and it contributes nothing. An `==` is always false; a `!=` always
true.

This is the always-false / dead-clause signal from Cedar's validator, which
warns (rather than errors) on conditions that can never change outcome. It
catches a stale enum value left in a rule after the schema's `enum` was
narrowed, or a typo in the compared literal.

This is a **warning**, not a gate: the rule is well-formed and the types match
(a wrong-*type* literal is `MDATRON-E0022` instead). It is conservative — it
fires only when the field is reachable through declared `properties` at every
level, declares a scalar `enum`, and the same-typed literal is provably
outside it (object openness plays no part — the closed-object gate belongs to
E0021's existence check). mdatron deliberately does not
attempt value-level reasoning across clauses (e.g. `x == "a" and x == "b"`);
only schema-decidable enum membership is checked.

## How to fix

1. **The literal is stale or mistyped.** Use one of the field's declared enum
   values.
2. **The enum is wrong.** Add the value to the schema's `enum` if the document
   should be allowed to carry it.
3. **The clause is genuinely unreachable.** Remove it.

## See also

- the mdatron design reference, § Cross-file semantics stay narrowed (in the
  project repository)

## Related codes

- MDATRON-E0022 — a comparison between type-incompatible operands (gated)
- MDATRON-E0021 — a `$self` field reference that names an undeclared property
