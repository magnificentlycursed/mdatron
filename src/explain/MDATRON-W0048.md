# MDATRON-W0048 — reference-target-unverified

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A citation's line range, a link's `#fragment`, or a marker reference was NOT
verified because its target — though **present** — could not be checked. Two
cases emit this finding:

- the target exceeds the input size budget (the per-file cap, or the run's
  aggregate capture budget), so the check was skipped by budget;
- the target is **unverifiable** (its bytes are not valid UTF-8 text, or the
  open succeeded but the content could not be read — GH #48): a citation's
  line range, a fragment-bearing link into a markdown target, or a marker
  reference cannot be resolved against bytes the engine cannot read as text.

In both cases only the target's existence was verified, and the finding fires
**per reference** — never deduped, so every skipped check is visible.

This warning exists so degradation is never silent: a fabricated or
out-of-range reference into such a target must not silently satisfy a gate,
and an unverifiable target must never be misreported as a *dead* reference
(that would blame a healthy document). The reference families deliberately
degrade rather than abort here — a prose line naming such a file must not be
able to deny verification of the whole tree — and this finding is the
observable trace of that choice.

## How to fix

1. **The reference matters and must be verified.** For an oversized target,
   shrink or split it below the input size budget; for an unverifiable target,
   fix its encoding (make it valid UTF-8 text) or point the reference at a
   text file.
2. **The degradation is acceptable.** Leave it — the warning documents that
   the reference is existence-checked only. A gate that must not tolerate
   unverified references can escalate warnings with `--deny-warnings`.

## See also

- the mdatron design reference, § Verification is fast where it is invoked (in
  the project repository) — the snapshot capture and its declared input bounds

## Related codes

- MDATRON-E0100 / E0101 — dead citation / citation range out of bounds (the
  checks this warning reports as skipped)
- MDATRON-E0110 / E0111 — dead link target / dead anchor (likewise)
- MDATRON-E0112 — dead marker reference (likewise; an ABSENT target still
  reports dead references — this warning covers only a PRESENT target the
  engine cannot read as text)
