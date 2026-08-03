# MDATRON-W0048 — reference-target-unverified

**Severity:** warning
**Status:** accepted
**Introduced in:** 0.6.0

## What this means

A citation's line range, or a link's `#fragment`, was NOT verified because its
target exceeds the input size budget (the per-file cap, or the run's aggregate
capture budget). Only the target's existence was verified.

This warning exists so budget-driven degradation is never silent: the target
*could* have been checked (unlike a non-UTF8 or non-regular-file target), the
engine skipped it by budget, and a fabricated or out-of-range reference into
an oversized file must not silently satisfy a gate. The reference families
deliberately degrade rather than abort here — a prose line naming a large file
must not be able to deny verification of the whole tree — and this finding is
the observable trace of that choice.

## How to fix

1. **The reference matters and must be verified.** Shrink or split the target
   file below the input size budget so the range/fragment check can run.
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
