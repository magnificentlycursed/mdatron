# MDATRON-E0061 — pin-stale

**Severity:** error
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

A file changed after its governing document pinned it: the sha256 recorded in
`.mdatron/pins.yaml` no longer matches the file's content. The stale pin is the
attention loop working — the point is not the hash, it is that the governing
relationship must be re-read when the governed content moves. The motivating
incident: an engine code-range table changed under a design document's open
question and nothing signaled; this family exists so that class of drift
blocks instead.

## How to fix

1. **Re-read the governing document** named in the diagnostic; amend it if the
   change altered what it governs.
2. **Re-pin**: `mdatron pin --update` recomputes every pin in one command
   (`--dry-run` previews). Commit the re-pin together with any governing-doc
   amendment — the commit is the review record.
