# MDATRON-E0064 — generated-region-stale

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

This region's bytes differ from its source, so the document no longer shows
what the generator produced. The source path is quoted and both sizes are in
the message. The comparison is byte for byte, line endings included.

## How to fix

- **Regenerate the region**: replace everything between the markers with the
  source file's content.
- **The source is wrong**: rerun the generator that writes it, then copy it in.

## Related codes

- MDATRON-E0065 — the region's source does not exist
- MDATRON-E0066 — a malformed marker
- MDATRON-E0061 — a pinned file changed after its pin (the same freshness idea,
  by hash)
