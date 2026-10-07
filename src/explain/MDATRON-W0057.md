# MDATRON-W0057 — link-register-entry-unused

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.8.0

## What this means

An entry in `.mdatron/links.yaml` matched no absolute URL in any link-checked
file this run: no link's page equals its `url`, or, for a `prefix: true`
entry, starts with it. The entry is quoted beneath the diagnostic. An unused
entry declares nothing the corpus says, and it costs something: the register
is the list a liveness tool checks on a schedule, so a stale entry is a URL
fetched for no reader, and an entry that was once used is often the trace of
a link that went away while the register did not. It is the dead-glob posture
(`MDATRON-W0046`, `W0054`) applied to the register, reported once per entry,
only on a whole-tree run whose jurisdiction came from `config.yaml` — an
incremental (`--changed`) run sees part of the tree, and an ad-hoc `--files`
run replaces the jurisdiction from the command line, so an entry outside
that glob is not stale.

An entry is used by the most specific match: a link whose page equals an
exact entry's `url` uses that entry even when a prefix entry also covers it,
so a prefix entry whose every page is also declared exactly is unused.

## How to fix

Remove the entry, or restore the link it declared. If a prefix entry is kept
for links that will come, expect this warning until they do (fail it in CI
with `--deny-warnings` only once the register is settled).

## Related codes

- MDATRON-E0115 — an absolute URL the register does not declare
- MDATRON-W0056 — a register no link-checked file consults
- MDATRON-W0054 — a route whose `files` glob claims no walked file
