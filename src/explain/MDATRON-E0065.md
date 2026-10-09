# MDATRON-E0065 — generated-region-source-missing

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
must start at column 0: an indented one (inside a list item, or an indented
code block showing the syntax) and one inside fenced code are text, not
markers. One at column 0 inside a multi-line HTML comment is still read as a
marker, so keep examples of the syntax in fenced code. mdatron only compares: the
project's own generator writes the source file, and the region must match it.

The file named in `from` does not exist in the working tree, or it is not a
readable regular file, so the region cannot be compared. The path is quoted.

## How to fix

- **The path is wrong**: correct `from` (it is root-relative, not relative to
  the document).
- **The source was never committed**: run the generator and commit its output.

## Related codes

- MDATRON-E0064 — the region differs from its source
- MDATRON-E0010 / E0011 / E0012 — a `from` that is absolute, escapes the tree,
  or resolves through a symlink
