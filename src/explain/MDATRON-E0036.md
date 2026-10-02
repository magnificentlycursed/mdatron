# MDATRON-E0036 — file-over-byte-budget

**Severity:** error
**Status:** accepted
**Introduced in:** 0.7.0

## What this means

The route claiming this file sets `max_bytes`, and the file is larger. The
bound exists for consumers that read only so much: Codex, for one, stops
adding `AGENTS.md` files once they total 32 KiB, and whatever is past the
budget is cut off without a warning. The file's size and the bound are in
the diagnostic's message. The size is the file's whole content in bytes as
checked out: frontmatter included, and CRLF line endings counted, so a
Windows checkout of the same file can be larger. Keep bounded files on LF
line endings (`eol=lf` in `.gitattributes`).

The bound is per file. Where a consumer's budget covers several files read
together, split it across their routes so that the largest combination still
fits.

## How to fix

- **Shorten the file.** Move detail into a document the file links to, and
  keep what the consumer must always read.
- **The budget really is larger.** Raise the route's `max_bytes` to the
  consumer's actual limit.

## Related codes

- MDATRON-E0030 — unrouted-file: the route family's closed-world check
- MDATRON-E0120 — section-count-violation: require content, not only bound it
