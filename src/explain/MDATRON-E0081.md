# MDATRON-E0081 — reference-target-not-captured

**Severity:** error
**Status:** accepted
**Introduced in:** 0.7.0

## What this means

A pin, a citation, a body link, or a marker rule named a target that the run
never captured into its snapshot — so the reference could not be checked at
all. This is an **engine defect in target discovery**, not a defect in the pin
record or in the document carrying the reference: every reference target is
captured before any check runs (the capture-complete seam), and a verdict
against a file that was never captured would be a false attestation — a
"stale" pin against a healthy file, a "dead" link to a target that exists.
Rather than guess, the engine reports the reference as unverifiable at error
severity — loud, never silent — and, for a marker rule, disables that rule for
the run instead of mass-flagging healthy references.

Through 0.6.0 this finding borrowed `MDATRON-E0080`'s code and headline, so a
consumer could not tell "the pipeline did not run" (exit `2`, no findings) from
"the pipeline ran, and this one reference was unverifiable" (exit `1`, a
finding). The two are now distinct: `E0080` is only ever the pipeline failure.

## How to fix

Report it upstream with the finding's quoted region (the target path) and a
`mdatron verify --json` envelope of the run; nothing in the project needs to
change. Until the fix lands, check the reference by hand — open the target and
confirm it exists and, for a section-scoped pin, that the heading is present.

## Related codes

- MDATRON-E0080 — the pipeline did not run to completion (exit `2`; no findings)
- MDATRON-E0061 / E0062 — a pin whose target WAS captured and is stale or missing
- MDATRON-E0100 / E0110 / E0112 — a citation, link, or marker reference whose
  target WAS captured and does not resolve
