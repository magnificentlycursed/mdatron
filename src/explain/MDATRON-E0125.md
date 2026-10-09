# MDATRON-E0125 — section-over-byte-budget

**Severity:** error
**Status:** accepted
**Introduced in:** 0.8.0

## What this means

A section rule on the route claiming this file sets `max_bytes` for a
`section`, and the section is larger. The section is measured as an injector
reads it: from its heading line through every line before the next heading of
the same or a higher level, subsections and trailing blank lines included. The size and the budget are
in the message; the finding is located at the section's heading, and a
duplicated heading is measured span by span.

The budget exists for readers that take a section or a page only up to a size
— an agent harness that injects at most a few thousand characters, a
specification with a token budget — and drop what is past it without saying
so. Bytes are counted as checked out, CRLF line endings included, like the
route's per-file `max_bytes` (`MDATRON-E0036`).

## How to fix

- **Shorten the section.** Move detail into a document or a later section the
  reader can fetch, and keep what must always be read.
- **The budget really is larger.** Raise the rule's `max_bytes` to the reader's
  actual limit.

## Related codes

- MDATRON-E0126 — a paragraph over a `per: paragraph` budget
- MDATRON-E0036 — a whole file over the route's `max_bytes`
- MDATRON-E0122 — the rule's section is not in the document
