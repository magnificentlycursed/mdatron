# MDATRON-E0117 — malformed-url

**Severity:** error
**Status:** accepted
**Introduced in:** 0.8.0

## What this means

A link in a link-checked file has an absolute destination (a scheme, or a
protocol-relative `//host`) that is not a well-formed URL. The reason is in
the message. What is checked is the structural shape of RFC 3986 and nothing
stylistic: the destination holds whitespace, a control character or an
invisible format character such as a zero-width space (a `[text](<https://a b>)`
bracketed destination, the one link form that admits a space), or more than one `#`; an `http`, `https` or
`//host` destination has no host, a host with an empty label
(`https://.acme.dev`, `https://a..b`), a host holding a character a host
name cannot (`\`, `|`, `<`; `_`, `~` and percent-encoded labels are legal
and accepted), an empty or non-numeric port (RFC 3986 admits an empty one; it is refused as the typo it almost always is), an unclosed or malformed IPv6
literal, or a `.` or `..` path segment on any `scheme://` form (a `\` too, on a web one), which a client resolves away so the
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

- **A space crept in.** Only two spellings reach this finding: a space
  inside a bracketed destination (`[text](<https://a b>)`) or whitespace a
  character reference decodes to (`&#32;`, `&#10;`). Percent-encode it
  (`%20`) or remove it. A URL wrapped onto a second line is not a link to
  CommonMark at all — nothing reports it, so rejoin it.
- **A `\` in a web path.** A browser reads it as `/`, so `x\..\admin`
  lands somewhere other than the text says; write `/` and the resolved path.
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
