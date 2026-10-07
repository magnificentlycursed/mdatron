# MDATRON-E0117 — malformed-url

**Severity:** error
**Status:** accepted
**Introduced in:** 0.8.0

## What this means

A link in a link-checked file has an absolute destination (a scheme, or a
protocol-relative `//host`) that is not a well-formed URL. The reason is in
the message. What is checked is the structural shape of RFC 3986 and nothing
stylistic: the destination holds whitespace, a control character or an
invisible format character such as a zero-width space (a `<https://a b>`
angle-bracket destination, say), or more than one `#`; an `http`, `https` or
`//host` destination has no host, a host with an empty label
(`https://.acme.dev`, `https://a..b`), a host holding a character a host
name cannot (`\`, `|`, `<`; `_`, `~` and percent-encoded labels are legal
and accepted), an empty or non-numeric port, an unclosed or malformed IPv6
literal, or a `.` or `..` path segment, which a client resolves away so the
page a reader lands on is not the text a register would compare; any other
scheme has nothing after its `:` (`mailto:`). Internationalised host names
are accepted as written. No liveness tool could fetch such a destination,
so it is not exported by `mdatron links`.

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
- **A `.` or `..` in the path.** Write the resolved path
  (`https://acme.dev/private/x`, not `https://acme.dev/public/../private/x`).
- **An invisible character.** A zero-width space or a soft hyphen copied in
  with the URL; retype the destination.

## Related codes

- MDATRON-E0115 — an absolute URL the register does not declare
- MDATRON-E0118 — a URL outside the claiming route's `link_policy`
- MDATRON-E0110 — a relative link whose in-tree target is missing
