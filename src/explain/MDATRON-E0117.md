# MDATRON-E0117 — malformed-url

**Severity:** error
**Status:** accepted
**Introduced in:** 0.8.0

## What this means

A link in a link-checked file has an absolute destination (a scheme, or a
protocol-relative `//host`) that is not a well-formed URL, so it is dead
everywhere, before any liveness check could run. The reason is in the
message. What is checked is the structural shape of RFC 3986 and nothing
stylistic: the destination holds whitespace or a control character (a
`<https://a b>` angle-bracket destination, say), or more than one `#`; an
`http`, `https` or `//host` destination has no host, a host with an empty
label (`https://.acme.dev`, `https://a..b`), a host holding a character a
host name cannot (`_`, `/` in the wrong place), an empty or non-numeric port,
or an unclosed IPv6 literal; any other scheme has nothing after its `:`
(`mailto:`). Internationalised host names are accepted as written.

This check needs no data: every route with `links: true` runs it. It is the
first of the offline checks on absolute URLs; the register
(`.mdatron/links.yaml`, `MDATRON-E0115`/`MDATRON-E0116`) and a route's
`link_policy` (`MDATRON-E0118`) are not consulted for a destination that is
not a URL.

## How to fix

- **A space or line break crept in.** Percent-encode it (`%20`) or remove it;
  a URL wrapped onto a second line in the source is this finding.
- **Two `#`.** A URL has one fragment; drop the second `#` or encode it.
- **No host.** `https://` must be followed by the host (`https://acme.dev/…`);
  `https:docs` and `https:///path` are not URLs.
- **A relative path mistaken for a URL.** A destination such as
  `docs:intro.md` reads as the scheme `docs`; write the relative path
  (`docs/intro.md`) so the link family resolves it in the tree.

## Related codes

- MDATRON-E0115 — an absolute URL the register does not declare
- MDATRON-E0118 — a URL outside the claiming route's `link_policy`
- MDATRON-E0110 — a relative link whose in-tree target is missing
