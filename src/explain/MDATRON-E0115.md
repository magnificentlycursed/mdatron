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
link whose page starts with its `url`. Matching is on the text as written —
no case folding, no normalising of a trailing slash — so `https://Acme.dev/`
is not `https://acme.dev/` and `https://acme.dev` is not `https://acme.dev/`.
Every link kind the link family sees is subject to the register: inline,
reference-style, image and autolink destinations alike, including
`mailto:` and other non-web schemes.

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
- **The register was meant to cover it.** Compare the entry and the link
  character by character: scheme, host case, trailing slash, `www.`.

## Related codes

- MDATRON-E0116 — a declared page, but a `#fragment` the entry does not list
- MDATRON-E0117 — a destination that is not a well-formed URL
- MDATRON-E0118 — a URL outside the claiming route's `link_policy`
- MDATRON-E0110 — a relative link whose in-tree target is missing
- MDATRON-W0057 — a register entry no link uses
