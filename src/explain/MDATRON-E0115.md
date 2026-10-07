# MDATRON-E0115 — external-link-undeclared

**Severity:** error
**Status:** accepted
**Introduced in:** 0.8.0

## What this means

A markdown link in a link-checked file points at an absolute URL (one with a
scheme, such as `https://…`, or a protocol-relative `//host/…`) that no entry
in `.mdatron/links.yaml` declares. The register is the closed set of URLs the
corpus may point at: an entry matches a link whose page (the URL before any
`#`) equals its `url` exactly, or, when the entry sets `prefix: true`, any
link whose page starts with its `url` (a prefix for a web URL runs past its
host — `https://acme.dev/`, never `https://acme.dev` — so it cannot be
extended into another host). The comparison is on the destination as the
CommonMark parser reads it (`&amp;` is `&`, `\_` is `_`) and nothing more:
no case folding, no normalising of a trailing slash, no percent-decoding, so
`https://Acme.dev/` is not `https://acme.dev/` and `https://acme.dev` is not
`https://acme.dev/`. Every markdown link kind is subject to the register:
inline, reference-style, image and autolink destinations alike, including
`<x@y.z>` (a `mailto:` link) and other non-web schemes. A bare URL in prose
and a raw HTML `<a href>` are not markdown links: they are neither checked
nor exported.

mdatron does not fetch the URL, so this is not a liveness verdict: the link
may be perfectly alive. What the finding says is that the corpus points
somewhere the register does not list. A liveness tool (lychee or its class)
checks the register's URLs on a schedule; `mdatron links --external` exports
every URL the corpus links to, for the same tool. The register exists only
when the file does; without it, absolute URLs are checked for well-formedness
(`MDATRON-E0117`) and against any route `link_policy` (`MDATRON-E0118`), and
nothing else.

## How to fix

- **The link is right.** Add its URL to `links.yaml` — an exact entry, or a
  `prefix: true` entry for the site or section it lives under. Confirm the
  page exists before you declare it; the register is what a liveness tool
  will check.
- **The link is wrong.** Correct it to a URL the register declares; a typo
  in the host or path is the common case.
- **The register was meant to cover it.** Compare the entry and the link as
  the link reads once CommonMark has decoded it: scheme, host case, trailing
  slash, `www.`, and whether a prefix entry really runs past its host.
- **A relative path mistaken for a URL.** A destination such as
  `docs:intro.md` reads as the scheme `docs` and is a well-formed URL to
  mdatron, so it reaches the register (or passes silently without one);
  write the relative path (`docs/intro.md`) so the link family resolves it
  in the tree.

## Related codes

- MDATRON-E0116 — a declared page, but a `#fragment` the entry does not list
- MDATRON-E0117 — a destination that is not a well-formed URL
- MDATRON-E0118 — a URL outside the claiming route's `link_policy`
- MDATRON-E0110 — a relative link whose in-tree target is missing
- MDATRON-W0057 — a register entry no link uses
