# Cost ledger

Usage record for the diagnostic-output cost contract (`DESIGN.md` § Output:
"sizes are measured and recorded in the cost ledger"). The compact per-finding
limit is **512 bytes** (contract limit, ratified 2026-07-25, tracker #80 D4).

## Compact per-finding sizes (measured)

| Date | Build | Finding shape | Bytes | Limit headroom |
|---|---|---|---|---|
| 2026-07-25 | #44 lane (0.1.0+) | E0050 enum violation, absolute path, allowed-options list, one quoted value | 315 | 197 |
| 2026-07-25 | #44 lane (0.1.0+) | E0050 additionalProperties, absolute path, one quoted key | 246 | 266 |

Method: seeded violations in the repo's own review-log corpus, driven via
`mdatron verify --project-root . --compact -q`, byte-measured per blank-line-
separated block. Contract tests additionally assert the limit on typical,
hostile, and over-limit-quoted shapes (`src/diagnostic.rs` compact tests) and
on the CLI surface (`tests/cli_integration.rs`).
