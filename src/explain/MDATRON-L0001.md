# MDATRON-L0001 — governance-weakening-standing

**Severity:** lint
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

A justified `unpinned:` tombstone stands in `.mdatron/pins.yaml`: a file was
deliberately removed from pin governance, with its reason and owner recorded.
This informational finding fires from the standing annotation itself on every
whole-tree verify — the weakening stays loud in the tool's own channel for as
long as it stands (`DESIGN.md` § Governance data is governed). It is a record,
not a defect.

## How to fix

Nothing is broken. If the weakening should end, re-pin the file (add it back
to `pins:` via `mdatron pin --update` after listing it) and delete the
tombstone in the same reviewed commit. If the tombstone is wrong, correct it —
commit review is the anchor.
