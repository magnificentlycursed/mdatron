# MDATRON-E0022 — comparison-type-mismatch

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A rule's `==` or `!=` compares two operands whose types the frontmatter schema
declares to be incompatible — for example `$self.count == "yes"` where `count`
is declared `integer`, or `$self.title == 5` where `title` is `string`. An
integer value is never equal to a string literal, so the comparison is
**constant** for every document, and the operator decides how the rule breaks:

- with `==`, the comparison is always **false**, so the rule's assertion
  always fires — it flags every document;
- with `!=`, the comparison is always **true**, so the rule's assertion never
  fires — a **silent no-op** that looks like a check but enforces nothing.

Either way the rule does not do what its author intended. This is a type
error, hard-gated regardless of the operator (Cedar's validator errors on type
mismatches irrespective of how the operands are compared).

This extends the `MDATRON-E0021` validate-before-deploy posture (adopting
Cedar's policy validator) from field *existence* to comparison *type
compatibility*: the mismatch is caught once, at rule load, before any document
is walked — not as a runtime surprise.

Like E0021 this is a **hard gate** (error, exit 1) and therefore
**conservative by construction** — it fires only when BOTH operands are
statically decidable: a scalar literal, or a `$self.<field>` whose schema
declares a single concrete type under a closed object (`additionalProperties:
false`). Every undecidable shape passes unflagged — an open object, a
multi-type/nullable field (`["string","null"]`), a field with no declared
`type`, an array/object level, a `$ref` or combinator, or a non-`$self`
operand. `integer` and `number` are treated as compatible (an integer is a
valid number).

## How to fix

1. **The literal is wrong.** Use a literal of the field's declared type.
2. **The field is wrong.** Reference the field whose type you meant to compare.
3. **The schema is wrong.** Widen the property's declared `type` if the
   comparison is genuinely intended across types.

## See also

- the mdatron design reference, § the DSL and its narrowed surface (in the
  project repository)

## Related codes

- MDATRON-E0021 — a `$self` field reference that names an undeclared property
- MDATRON-W0050 — a well-typed comparison whose literal is outside a declared
  `enum` (a dead clause, warned not gated)
