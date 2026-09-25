# MDATRON-L0001 — governance-weakening-standing

**Severity:** lint
**Status:** accepted
**Introduced in:** 0.2.0

## What this means

A justified tombstone stands in the project's governance data: a file was
deliberately removed from governance, with its reason and owner recorded. The
tombstone has two carriers, and this finding fires from both:

- an `unpinned:` entry in `.mdatron/pins.yaml` — a file un-pinned from its
  governing document (`file` is project-root-relative);
- a `demoted:` entry in `.mdatron/manifest.yaml` — a file demoted from the
  engine-managed partition to adopter-owned (`path` is relative to `.mdatron/`;
  the finding quotes it as `.mdatron/<path>`). Through 0.6.0 this carrier
  emitted nothing, although the design had always said it should.

This informational finding fires from the standing annotation itself on every
whole-tree verify — the weakening stays loud in the tool's own channel for as
long as it stands (`DESIGN.md` § Validation is data-driven, governance data is governed). It is a record,
not a defect. A tombstone missing its reason or owner is `MDATRON-W0042`
instead.

## How to fix

Nothing is broken. If the weakening should end, restore the governance — re-pin
the file (`mdatron pin --update` after listing it under `pins:`), or move the
file back into the managed partition — and delete the tombstone in the same
reviewed commit. If the tombstone is wrong, correct it; commit review is the
anchor.

## Related codes

- MDATRON-W0042 — a tombstone without its justification (reason and owner)
- MDATRON-E0060 — drift in a managed file; a demotion without a tombstone
  trips it
