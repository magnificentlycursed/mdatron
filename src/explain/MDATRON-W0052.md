# MDATRON-W0052 — inert-pattern-key

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.7.0

## What this means

A pattern file under `.mdatron/patterns/` carries a key the engine parses but
does not act on. Two keys are in this class:

- a pattern's `phases:` — documented through 0.6.0 as "runtime-selectable
  subsets", but no selector ever existed on the command line or in the
  pipeline; every rule runs on every verify regardless of the list;
- a rule's `location: {field, expression}` — accepted by the strict parser,
  never consumed; a rule finding is always located at the whole artifact.

Both stay accepted under DSL v1 (`mdatron_dsl_version: 1`, the default) so no
existing file breaks, and both are announced here rather than tolerated
silently: the DSL reference promises that the documented surface and the
engine match exactly, and "accepted, no effect" is the honest effect. Both keys
are retired at DSL v2, where a file that carries them is refused as unknown
keys. One warning is reported per occurrence — at the pattern file's first
line for `phases:`, at the rule for `location:` — quoting the pattern id.

## How to fix

Delete the key from the pattern file. Nothing else changes: the rules ran
exactly the same with the key present. If you relied on `phases:` to run a
subset of rules, split the rules into separate pattern files and keep only the
files you want in `.mdatron/patterns/` (the DSL runs every file present).

## Related codes

- MDATRON-E0021 — a rule referencing an undeclared `$self` field (also a
  load-time rule check)
- MDATRON-W0050 — a comparison no conforming document can satisfy (the other
  "this rule cannot do what it says" warning)
