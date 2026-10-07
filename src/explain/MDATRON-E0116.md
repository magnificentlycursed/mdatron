# MDATRON-E0116 — external-anchor-undeclared

**Severity:** error
**Status:** accepted
**Introduced in:** 0.8.0

## What this means

A link in a link-checked file points at a page `.mdatron/links.yaml` declares,
with a `#fragment` that the matching entry's `fragments` list does not hold.
An entry's `fragments` is the closed set of anchors the corpus may use on
that page (or on every page a `prefix: true` entry covers); an entry with no
`fragments` key accepts any fragment, and one with an empty list accepts
none. A bare `#` (top of page) is always accepted. Fragments are compared
as the destination reads after CommonMark decoding, without percent-decoding or slug derivation: for a page outside the
tree, mdatron has no headings to derive slugs from.

This is the external twin of `MDATRON-E0111`. For a page inside the tree the
engine reads the real headings; for a page outside it, it never fetches, so
the register's list is the only thing it can check against — the list is a
promise the author made after looking at the live page. The export
(`mdatron links --external`) carries the fragment with the URL, so a liveness
tool that checks anchors (lychee with `--include-fragments`) can verify the
promise on the live page.

## How to fix

- **The anchor exists on the live page.** Add it to the entry's `fragments`,
  spelled as the link spells it.
- **The anchor was renamed or removed.** Update the link's fragment to one
  the page has, and the entry's list if it is new.
- **The page's anchors are not worth tracking.** Remove the `fragments` key
  from the entry; it then accepts any fragment.
- **The link hit a prefix entry.** The `fragments` of a `prefix: true` entry
  apply to every page under it; declare the page with an exact entry of its
  own if its anchors differ.

## Related codes

- MDATRON-E0115 — an absolute URL the register does not declare at all
- MDATRON-E0111 — a `#fragment` that matches no heading in an in-tree target
- MDATRON-E0117 — a destination that is not a well-formed URL
