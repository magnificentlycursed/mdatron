# MDATRON-E0101 — citation-line-out-of-range

**Severity:** error
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

A citation's target exists, but the cited line (or the end of the cited
range) lies past the target's last line — or the range is malformed (start
after end, or line 0; lines are 1-based). The claim points at content the
file does not hold, which is the same failure as a dead citation at finer
grain.

## How to fix

Re-derive the line number from the current target (the diagnostic reports the
target's actual line count) and update the citation, or cite the file without
a line if the claim is about the file as a whole.
