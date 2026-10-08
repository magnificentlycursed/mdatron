# MDATRON-E0023 — rule-evaluation-failed

**Severity:** error
**Status:** accepted
**Introduced in:** 0.8.0

## What this means

A pattern rule's expression raised an evaluation error against this file's
data. Where it sits decides what else is reported:

- **In the `assert` or a `let:` binding** — the rule reached no verdict on this
  file: its own code is not reported here, neither as a pass nor as a failure.
- **In a `{{expr}}` of the `message`** — the rule had already failed, and its
  own finding is still reported, with that value shown as `[unrenderable]`.
  This finding sits beside it.

Either way every other rule and file in the run is still checked. The quoted
regions name the `pattern`, the `rule`, where the expression sits (`in`:
`assert`, `let.<name>`, or `message`), and the evaluator's `error`.

An unknown function, the wrong number of arguments, or a binding no `let:` or
quantifier defines — in the `assert`, a `let:`, or a message `{{expr}}` — is
refused once when the patterns load, as `MDATRON-E0080` (`kind` `expr_parse`),
whether or not the rule selects any file. Type errors are left to evaluation and
reported here, including one between literals that fails on every file alike
(`count("x")`, `not "a"`).

The usual causes:

- **A possibly-absent field used where a value is required.** A missing field
  reads as `Null`, and none of these accept `Null`: `x in $self.tags` (the right
  side must be an array); `count`, `union`, `intersect`, `difference`, `join`
  (arrays); `concat` (strings); `len` (a string or an array); and `and`, `or`,
  `not`, an `every`/`some`/`filter` predicate (booleans) — so an absent boolean
  flag used as a condition is an error. A file that lacks the field, or has no
  frontmatter at all, evaluates the rule against `Null`. (`every`, `some` and
  `filter` accept a `Null` *collection* as empty, `defined()` accepts anything,
  and `key()` treats a `Null` key as a lookup miss.)
- **A field holding the wrong type** — a string where the rule iterates an
  array, or a non-boolean value where a condition needs `true` or `false`.

Through 0.7.0 any evaluation error aborted the whole run as `MDATRON-E0080`
(`pipeline_error.kind` `eval`, exit `2`, no findings), so one file could deny
verification of every other. Since 0.8.0 it is this finding on the file it
occurred on (exit `1`), and the run completes.

## How to fix

1. **Guard the field.** `defined($self.tags) and "x" in $self.tags`, or
   `not defined($self.tags) or …` — `and`/`or` short-circuit, so the guarded
   side is not evaluated when the guard decides.
2. **Fix the data.** If the field is required, give the file a value of the
   type the rule expects — and declare the field `required` in the file's
   schema so its absence is reported as such.

## Related codes

- MDATRON-E0080 — the pipeline did not run to completion; a rule expression
  that does not parse, or is wrong for every file, is reported there
  (`kind` `expr_parse`)
- MDATRON-E0021 — a `$self` field reference that names an undeclared property,
  caught at load before any file is evaluated
