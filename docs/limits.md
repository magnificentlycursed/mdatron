# Declared limits

The bounds catalog (`DESIGN.md` § Verification is fast where it is invoked:
"Hook-time cost is bounded by declared limits shipped as data"). The single
source of truth is `src/limits.rs::SHIPPED`; the table below is **rendered
from it** (#189). `mdatron docs limits` renders the running binary's own
catalog, and this committed page must equal that rendering — a shipped test
enforces it and regenerates the page under `MDATRON_UPDATE_DOCS=1` — so the
values enforced and the values documented cannot drift apart.

Exceeding any input bound is a diagnostic, never silent degradation.
Config-scoped inputs (governed bodies, index sources, pinned files, marker
`target_doc`) surface a bound breach as a whole-run `bound_exceeded` pipeline
error; prose-scoped targets (citation and link targets) degrade to
existence-verified with a `MDATRON-W0048` warning — a prose line must not be
able to abort the run (#103). The one output limit, the compact per-finding
size, is enforced by cutting the block to fit rather than by a diagnostic.

<!-- mdatron-generated: rendered from src/limits.rs (SHIPPED) — do not edit by hand -->
| Limit | Shipped value | Surface | On exceedance |
|---|---|---|---|
| `max-input-size-per-file` | 8 MiB | every captured input (bodies, index sources, pin/cite/link/marker targets) | config-scoped: `bound_exceeded`; prose-scoped: `W0048` degrade |
| `aggregate-snapshot-size` | 64 MiB | total bytes stored in one run's snapshot | config-scoped: `bound_exceeded`; prose-scoped: `W0048` degrade |
| `structural-nesting-depth` | 256 | flow-collection nesting in any YAML mdatron parses — frontmatter, `.yaml` index sources, `.mdatron/` files — counted before parsing (bracket bytes, quoted or not) | governed file: `bound_exceeded`; index source: `index_build`; `.mdatron/` file: its load error; link, marker or pin target: read as having no frontmatter |
| DSL expression depth | 256 | adopter `assert:` expression nesting | expression `ParseError` at pattern load |
| walk `depth` | 64 | engine-owned no-follow glob walk (index sources) | `WalkBounded` index error |
| walk `entries` | 100 000 | directory entries listed across one glob walk | `WalkBounded` index error |
| `concurrent-invocation-count` | 8 | simultaneous `verify` runs per user per project root | `bound_exceeded` |
| compact per-finding size | 512 B | one finding block of `--compact` output | the block is cut to fit (`…`); never exceeded |
<!-- mdatron-generated: end -->

## Notes

- **Concurrent invocations** are counted with per-user, per-root slot files
  under the system temp directory (never inside the repository; the directory
  is uid-keyed, mode `0700`, ownership-verified — a foreign or symlinked
  directory at the expected path is refused with a named diagnostic), locked
  with `flock` on unix and an exclusive-share open on windows. The LOCKS
  evaporate with the owning process, so a crashed run can never wedge the
  count; the 0-byte slot files are deliberately never unlinked (unlinking a
  held slot would let a fresh acquirer double-admit past the limit on a new
  inode). The count bounds each user's runs, not the machine total; the
  standalone `pin` command sits outside it. Platforms with neither lock
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

## Compact per-finding sizes (measured)

The compact per-finding size is a **contract limit**, not a band derived from
actuals (ratified 2026-07-25, tracker #80, decision 4; `DESIGN.md` § Agents are
the first consumer). This table is its usage record — the governing contract's
cost ledger, kept here beside the limit it measures against (#189).

| Date | Build | Finding shape | Bytes | Limit headroom |
|---|---|---|---|---|
| 2026-07-25 | #44 lane (0.1.0+) | E0050 enum violation, absolute path, allowed-options list, one quoted value | 315 | 197 |
| 2026-07-25 | #44 lane (0.1.0+) | E0050 additionalProperties, absolute path, one quoted key | 246 | 266 |
| 2026-09-25 | #189 lane (0.7.0-dev, envelope 3.1.0) | E0050 enum violation, root-relative path, allowed-options list, one quoted value | 181 | 331 |
| 2026-09-25 | #189 lane (0.7.0-dev, envelope 3.1.0) | E0050 additionalProperties, root-relative path, one quoted key | 138 | 374 |

Method (the 2026-09-25 rows; the 2026-07-25 rows used the since-archived
review-log corpus): a fixture project whose `.mdatron/config.yaml` walks
`posts/*.md`; a `.mdatron/schemas/blog.json` requiring `schema_class`
(`const: blog`), `title` and `status` (enum `draft`, `published`, `retired`)
with `additionalProperties: false`; and two posts — `posts/enum.md` carrying
`status: archived` (the enum row) and `posts/extra.md` carrying a stray
`extra: nope` field (the additionalProperties row). Run
`mdatron verify --project-root <fixture> --compact -q` and byte-count each
blank-line-separated block; the path in each block is root-relative, so the
file names above are part of the measurement. Contract tests additionally
assert the limit on typical, hostile, and over-limit-quoted shapes
(`src/diagnostic.rs` compact tests) and on the CLI surface
(`tests/cli_integration.rs`).
