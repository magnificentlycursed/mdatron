# MDATRON-E0118 — link-policy-violation

**Severity:** error
**Status:** accepted
**Introduced in:** 0.8.0

## What this means

A link in a file claimed by a route with a `link_policy` has an absolute
destination the policy forbids. The message names the clause: its scheme is
not one of the route's `schemes` (a protocol-relative `//host` link has no
scheme and violates any `schemes` list); its host is not one of the route's
`hosts` (a host matches an entry exactly, or is any subdomain of a
`*.example.com` entry — never the apex); or one of its query parameters has
a name matching a `forbid_query` pattern (`^utm_` for the tracking
parameters; names are percent-decoded first, so `utm%5Fsource` is
`utm_source`). Schemes and hosts compare without regard to ASCII case; a
host compares as written otherwise, so `acme.dev.` (a trailing dot) is not
`acme.dev`, and a bracketed IPv6 literal (`[::1]`) must be listed as one. A
destination with no host (`mailto:…`, `ftp://…` has one but is not web) is
judged by `schemes` alone: a `hosts` list on its own says nothing about a
link whose scheme is not `http` or `https`, so pair it with `schemes` to
close that door. A link that violates two clauses is reported twice, once
per clause, and under `forbid_query` once per forbidden parameter, each
quoting the clause it broke.

The policy is per route, so a corpus can be `https`-only on its published
pages and looser on an internal one. It needs no register: `link_policy`
and `.mdatron/links.yaml` are independent, and a link may be declared in the
register and still violate the policy. mdatron never fetches the URL; this is
a statement about what the link says, not whether it answers.

## How to fix

- **Change the link.** `http://` to `https://`, a mirror host to the
  canonical one, a tracking-decorated URL to its clean form.
- **Widen the policy.** Add the scheme or host to the route's list, or
  narrow the `forbid_query` pattern, if the link is right and the policy was
  too strict.
- **The link is not the route's business.** Move the file to a route whose
  policy fits it; a route's policy applies to every file it claims.
- **A relative path mistaken for a URL.** A destination such as
  `docs:intro.md` reads as the scheme `docs` and fails a `schemes` clause;
  write the relative path (`docs/intro.md`) so the link family resolves it
  in the tree.

## Related codes

- MDATRON-E0115 — an absolute URL the register does not declare
- MDATRON-E0117 — a destination that is not a well-formed URL
- MDATRON-E0030 — unrouted-file: the route family's closed-world check
