# MDATRON-W0058 — rule-context-matches-nothing

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.8.0

## What this means

A pattern rule's `context` — the `schema_class` it names, or its path glob —
matched none of the walked files, so the rule checked nothing. A mistyped
class or glob therefore **fails open**: the files the rule was written for are
never evaluated, and the run reads as if every one of them conformed.

Through 0.7.0 this was visible only when *every* rule was dead (the rule-DSL
lane then reports `inert`). One live rule anywhere kept the lane `active` with
a healthy reason while a dead rule beside it went unannounced. Now each dead
rule is reported once, at the rule in its pattern file, quoting the `pattern`,
the `rule` and the `context`, and the lane's reason counts the rules that
matched (`2 of 3 rules matched a walked file`).

It is reported only on a whole-tree run whose jurisdiction came from
`config.yaml`: an incremental (`--changed`) run sees just part of the tree, and
an ad-hoc `--files` run replaces the jurisdiction from the command line, so a
rule whose files are outside that scope is not dead. An incremental run's
reason still counts the rules that matched a file in its scope.

## How to fix

Correct the `context` to the `schema_class` the files declare (or a route
binds with `schema:`), or to a root-relative path glob that matches them;
`*` stays within one path segment, `**` crosses any depth. Remove the rule if
the files it was written for are gone. A rule kept for files that do not exist
yet should be added when they do.

## Related codes

- MDATRON-W0054 — a route whose `files` glob claims no walked file
- MDATRON-W0045 — a file declaring a `schema_class` that no schema and no rule
  context validates (the converse: a file nothing checks)
- MDATRON-W0046 — a `file_globs` entry that matches no file
