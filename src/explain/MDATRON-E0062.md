# MDATRON-E0062 — pin-target-missing

**Severity:** error
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

A pin in `.mdatron/pins.yaml` names a file that cannot be opened inside the
governed tree. A pin over nothing is a dangling governance edge — the record
claims governance the tree cannot honor. (A symlinked target refuses
separately under `MDATRON-E0012`; `mdatron pin --update` never invents a hash
for an unreadable file.)

## How to fix

- **The file moved or was renamed.** Correct the pin's `file` path.
- **The file was deliberately removed.** Un-pin it with a justified tombstone:
  move the entry to `unpinned:` with `reason` and `owner` — the weakening then
  stays loud as the standing `MDATRON-L0001` lint rather than vanishing.
