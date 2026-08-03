# Declared limits

The bounds catalog (`DESIGN.md` § Verification is fast where it is invoked:
"Hook-time cost is bounded by declared limits shipped as data"). The single
source of truth is `src/limits.rs::SHIPPED`; this table is its operator-facing
rendering — a change to either lands with the other (#92 sub-lane D).

Exceeding any bound is a diagnostic, never silent degradation. Config-scoped
inputs (governed bodies, index sources, pinned files, marker `target_doc`)
surface a bound breach as a whole-run `bound_exceeded` pipeline error;
prose-scoped targets (citation and link targets) degrade to
existence-verified with a `MDATRON-W0048` warning — a prose line must not be
able to abort the run (#103).

| Limit | Shipped value | Surface | On exceedance |
|---|---|---|---|
| `max-input-size-per-file` | 8 MiB | every captured input (bodies, index sources, pin/cite/link/marker targets) | config-scoped: `bound_exceeded`; prose-scoped: `W0048` degrade |
| `aggregate-snapshot-size` | 64 MiB | total bytes stored in one run's snapshot | config-scoped: `bound_exceeded`; prose-scoped: `W0048` degrade |
| `structural-nesting-depth` | 256 | flow-collection nesting in governed-body YAML | `bound_exceeded` |
| DSL expression depth | 256 | adopter `assert:` expression nesting | expression `ParseError` at pattern load |
| walk `depth` | 64 | engine-owned no-follow glob walk (index sources) | `WalkBounded` index error |
| walk `entries` | 100 000 | directory entries listed across one glob walk | `WalkBounded` index error |
| `concurrent-invocation-count` | 8 | simultaneous `verify` runs per project root | `bound_exceeded` |

## Notes

- **Concurrent invocations** are counted with per-root slot files under the
  system temp directory (never inside the repository), locked with `flock` on
  unix and an exclusive-share open on windows. Both evaporate with the owning
  process, so a crashed run can never wedge the count. Platforms with neither
  primitive run unbounded — a documented carve-out mirroring the confine
  fallback posture.
- **YAML alias and recursion bounds** ride the parser (`serde_yaml_ng`'s
  repetition and recursion guards) and surface as parse diagnostics
  (`MDATRON-E0001` for a governed body; an index build error for a `keys:`
  source). Pinned by fixture; deliberately not re-implemented in the catalog.
- **No global wall-clock budget** is enforced; the DESIGN enforcement-status
  note records this honestly. Directory walking and dependent-closure
  traversal are bounded by the size/count/depth limits above, and pattern
  matching is structurally linear-time (no ReDoS budget to exhaust).
