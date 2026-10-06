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
parameters). Schemes and hosts compare case-insensitively; a destination
with no host (`mailto:…`) is judged by `schemes` alone. A link that violates
two clauses is reported twice, once per clause, each quoting the clause it
broke.

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

## Related codes

- MDATRON-E0115 — an absolute URL the register does not declare
- MDATRON-E0117 — a destination that is not a well-formed URL
- MDATRON-E0030 — unrouted-file: the route family's closed-world check
