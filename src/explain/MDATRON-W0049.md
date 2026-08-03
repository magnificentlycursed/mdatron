# MDATRON-W0049 — index-source-unreadable

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A `keys:` declaration in a pattern file names a `source:` that could not be
read — it is missing, its open was refused, or its bytes are not valid UTF-8 —
so it contributed no entries to its cross-file index. The index is **inert for
that source**: it is built from whatever other sources resolved (a glob may
match several), but this one added nothing.

This is a warning, not a whole-run abort, and not a silent skip. One unreadable
source must not deny verification of the rest of the tree, and an under-
populated index must never look identical to a complete one — so the missing
coverage is reported here. If a rule *requires* an entry this source would have
provided (`defined(key(...))`), that rule surfaces its **own** finding at its
configured severity: this warning is the availability signal, the rule is the
conformance gate.

Two neighbouring conditions are NOT this warning — they are hard errors:

- a `source:` that resolves through a **symlink** is refused (`MDATRON-E0012`,
  a confinement decision, not an availability one);
- a `source:` that exceeds the input size budget is the declared-bounds abort
  (`bound_exceeded`).

## How to fix

1. **The source should exist and be readable.** Restore the file, fix its
   permissions, or re-encode it as UTF-8.
2. **The source is a glob and some matches are unreadable.** Narrow the glob,
   or repair the offending files.
3. **The index is no longer needed.** Remove the `keys:` declaration.

## See also

- the mdatron design reference, § the DSL and its cross-file `key()` index (in
  the project repository)

## Related codes

- MDATRON-E0012 — a `source:` resolving through a symlink (refused, not degraded)
- MDATRON-W0043 / W0046 / W0047 — the sibling "family is inert / configured
  scope matched nothing" availability warnings
