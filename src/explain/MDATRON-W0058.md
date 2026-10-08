# MDATRON-W0058 — rule-context-matches-nothing

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.8.0

## What this means

A pattern rule checked nothing on this run, for one of two reasons the
message names:

- **Its `context` matched no walked file** — the `schema_class` it names (or a
  route binds with `schema:`), or its path glob. A mistyped class or glob
  **fails open**: the files the rule was written for are never evaluated, and
  the run reads as if every one of them conformed.
- **Its `context` selected walked files, but none could be evaluated.** A rule
  runs only on a file with frontmatter (or one a route binds with `schema:`,
  evaluated as an empty mapping without it); a selected file with no
  frontmatter, unparseable frontmatter (`MDATRON-E0001`), or unreadable bytes
  (`MDATRON-E0003`) is skipped. The context is right; the files are not
  evaluable.

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
reason still counts the rules that matched a file in its scope, and an ad-hoc
`--files` run's reason counts them too, without the warning.

## How to fix

If the message says the context **selected** files: give them frontmatter, or
bind them with a route's `schema:`, or fix the errors already reported on them.

Otherwise, correct the `context` to the `schema_class` the files declare (or a route
binds with `schema:`), or to a root-relative path glob that matches them;
`*` stays within one path segment, `**` crosses any depth. Remove the rule if
the files it was written for are gone. A rule kept for files that do not exist
yet should be added when they do.

## Related codes

- MDATRON-W0054 — a route whose `files` glob claims no walked file
- MDATRON-W0045 — a file declaring a `schema_class` that no schema and no rule
  context validates (the converse: a file nothing checks)
- MDATRON-W0046 — a `file_globs` entry that matches no file
- MDATRON-W0043 / W0051 / W0055 / W0056 — the other scopes that reach nothing
  (vocabulary and coinage globs, `require_frontmatter` globs, code-catalog
  globs, a link register no link-checked file consults)
