# MDATRON-E0111 — dead-anchor

**Severity:** error
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A markdown link's `#fragment` matches no heading in its target. The fragment is
resolved with the GitHub heading-slug algorithm: a heading is lowercased, every
character that is not a letter, digit, `_`, or `-` is dropped, and spaces become
hyphens — so `## Five Check Families` is reachable as `#five-check-families`. The
anchor set is built from a CommonMark parse, so both **ATX** (`## Heading`) and
**setext** (underlined with `===`/`---`) headings count, **duplicate** headings
carry GitHub's `-N` disambiguation (a second `## Notes` is reachable as
`#notes-1`, a third as `#notes-2`), and explicit **HTML anchors**
(`<a name="x">`, `<a id="x">`) are valid targets. A same-document link
(`[x](#section)`) resolves against this file's own headings; a cross-file link
(`[x](other.md#section)`) resolves against the target markdown file's headings,
with any frontmatter stripped. Headings inside fenced code blocks are not
anchors. Link checking is per-route opt-in (`links: true` in
`.mdatron/routes.yaml`).

## How to fix

- **The heading was renamed.** Update the fragment to the heading's current slug.
- **The heading repeats.** The first occurrence keeps the bare slug; the second
  and later gain `-1`, `-2`, … — link to the one you mean (`#notes` vs `#notes-1`).
- **The slug is wrong.** Derive it by lowercasing the heading text, dropping
  punctuation, and replacing spaces with hyphens.
- **The heading lives in another file.** Point the link at that file and
  fragment (`other.md#the-heading`).
- **The target is not markdown.** Fragments resolve only against markdown
  headings; drop the fragment, or link to a document that has the anchor.
