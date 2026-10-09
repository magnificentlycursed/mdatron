# MDATRON-E0066 — generated-region-malformed

**Severity:** error
**Status:** accepted
**Introduced in:** 0.8.0

## What this means

A route claiming this file sets `generated: true`, so each generated region
in it is checked. A generated region is the bytes between a marker pair:

```markdown
<!-- mdatron:generated from="docs/generated/table.md" -->
…exactly the bytes of docs/generated/table.md…
<!-- /mdatron:generated -->
```

`from` is root-relative and confined to the governed tree. The region is
everything after the opening marker's line and before the closing marker's
line, so a source that ends in a newline is pasted with its newline. A marker
inside fenced code is an example, not a marker. mdatron only compares: the
project's own generator writes the source file, and the region must match it.

A marker here is malformed, so there is no region to check. The message says
which case:

- an opening marker that is not exactly
  `<!-- mdatron:generated from="<path>" -->` with a non-empty path;
- a region that opens inside another region (regions do not nest);
- a closing marker with no opening marker before it;
- an opening marker that is never closed.

## How to fix

Write the pair exactly as above, one region at a time, each opening marker
followed by its closing marker.

## Related codes

- MDATRON-E0064 — the region differs from its source
- MDATRON-E0065 — the region's source does not exist
