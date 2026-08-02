# MDATRON-E0063 — pin-section-not-found

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A pin carries a `section` (a heading naming a span of its target file), but that
heading is not present in the file — so the section it pins over cannot be
located, and the pin cannot be verified. Section pins (#146) let a governing
document pin a sha256 over one heading-delimited span rather than the whole
file: the hash covers the span from the named heading's line through just
before the next heading of the same or higher level. The heading is matched by
**level and text** (`"## Decomposition"` matches a `##` heading, not a `###`
one), and a `#` inside a fenced code block is not a heading. This finding fires
when no such heading is found — the heading was renamed, mistyped, its level
changed, or the target is not a text file.

## How to fix

- **The heading was renamed or its level changed.** Update the pin's `section`
  to the heading's current text and level, then `mdatron pin --update`.
- **Typo in the `section` spec.** It must be the full heading line, e.g.
  `section: "## Decomposition (phase 1c)"`.
- **You meant to pin the whole file.** Remove the `section` field; the pin then
  covers the entire file as before.
