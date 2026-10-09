# MDATRON-E0126 — paragraph-over-byte-budget

**Severity:** error
**Status:** accepted
**Introduced in:** 0.8.0

## What this means

A section rule on the route claiming this file sets `max_bytes` with
`per: paragraph`, and this paragraph is larger. A paragraph is a run of
adjacent non-blank lines that are not headings, outside fenced code: a blank
line, a heading or a fence ends it. Its size runs from the start of its first
line to the end of its last, inner line breaks included. The rule covers its
`section`, or the whole document body when it names none. The size and the
budget are in the message; the finding is located at the paragraph's first
line, which is quoted.

Bytes are counted as checked out, CRLF line endings included.

## How to fix

- **Split the paragraph.** A long paragraph usually holds more than one point;
  give each its own.
- **The budget really is larger.** Raise the rule's `max_bytes`.

## Related codes

- MDATRON-E0125 — a whole section over its budget
- MDATRON-E0036 — a whole file over the route's `max_bytes`
