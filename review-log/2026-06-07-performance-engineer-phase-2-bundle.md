---
schema_class: review-entry
schema_version: 1.0.0
review_number: 13
date: 2026-06-07
phase: phase-3
scope: >-
  vsdd-cli crosslink #13 Phase 3 cold-session PerfE review — Phase 2
  runtime + space cost: include_str! catalog binary bloat, format_tty
  allocation pattern, eprintln-per-finding syscall cost, integration-test
  wall-time, README round-trip cost.
lens: >-
  Performance Engineer (latency + allocations + binary size + syscall
  cost + complexity class). Five-lens weighting: Maintainability 3,
  Edge-cases 3, Consistency 2, Attacker 1, Usability 1.
source: director-raised
session_note: >-
  Cold-session reviewer mode. PerfE-domain stance: cost matters at the
  margin; what's the limiting factor under realistic adopter scenarios?
  Sycophancy compensation: small projects look cheap; checking the
  larger-corpus scale.
model: claude-opus-4-7
execution_method: claude-code-cli-task-tool-cold-session
sycophancy_compensation: >-
  Looking for "this is fine at 10 findings but bad at 10,000." Honest
  about whether Phase 2's surfaces have meaningful perf cost or whether
  the perf concerns are theoretical.
---

# Findings

## F1 — `include_str!` catalog adds ~15KB to binary; trivial (Edge-cases)

**Classification:** dismissed
**Routing:** PerfE-internal verification.

Five explain pages × ~3KB each = ~15KB embedded into the binary. Today's
mdatron binary is ~5-10MB (Rust + clap + serde_json + serde_yaml +
mdatron-core). 15KB is rounding error.

Growth model: if the catalog reaches 100 codes × ~3KB each = 300KB. Still
not material for a CLI. Pass.

## F2 — `format_tty` allocates per finding (Maintainability)

**Classification:** accepted-with-remediation
**Routing:** PerfE-side; v0.1.x.

`format_tty` allocates a `String` for each finding's rendered output via
`format!` macro. For a verify run with 10000 findings, that's 10000
allocations + 10000 eprintln syscalls. At ~250 bytes per finding, ~2.5MB
of String allocation, all freed within the same loop.

Phase 2's surface is unchanged from pre-Phase 2 (the open-coded
`print_finding` also allocated per finding via `eprintln!` arg
formatting). Net wash.

The deeper question: should `Finding` rendering be allocation-free
(write to a shared buffer)? At 10k findings the allocation cost is
~1-2ms total — submerged by the actual verify work (file I/O, schema
validation). Not a bottleneck for v0.1.x.

## F3 — `eprintln!` per finding triggers per-line syscalls (Edge-cases)

**Classification:** accepted-with-remediation
**Routing:** PerfE-side; v0.1.x.

`print_finding` invokes `eprintln!` once per call. Each invocation
syscalls (write to stderr fd). For 10k findings, 10k syscalls.

Stderr is line-buffered by default in cargo-built binaries but unbuffered
in some terminal contexts (when stderr is a TTY). 10k syscalls @ ~10μs
each = ~100ms of syscall overhead.

Mitigation: wrap stderr in a `BufWriter` for batched flushing. ~5 LOC.
For an interactive operator who watches diagnostics scroll by, batching
loses the per-finding visual cadence; for CI runs where stderr is
captured to a file, batching is pure win.

Phase 2c didn't tackle this. v0.1.x candidate; not blocking.

## F4 — `mdatron explain CODE` is constant-time after catalog lookup (Edge-cases)

**Classification:** dismissed
**Routing:** PerfE-internal verification.

`cmd_explain` does a 5-arm match + a print. O(1) regardless of catalog
size growth. Even at 100 codes, the lookup compiles to a jump table or
similar. No perf concern.

## F5 — Integration tests' subprocess-spawn cost scales linearly with test count (Maintainability)

**Classification:** dismissed (with note)
**Routing:** PerfE-internal.

18 tests × ~50ms per subprocess spawn = ~900ms of subprocess overhead per
test run. cargo test parallelizes by default; on a 4-core CI runner the
wall-time is ~225ms. Acceptable.

If the test count grows past ~50, the subprocess-spawn cost becomes the
dominant cost. Refactor candidate: use mdatron-core's library API
directly (test the verify pipeline in-process) for the non-CLI-surface
assertions; reserve subprocess spawning for genuine CLI tests (flag
parsing, exit codes, stream contract). v0.1.x.

## F6 — README round-trip test extracts + spawns mdatron once (Edge-cases)

**Classification:** dismissed
**Routing:** PerfE-internal.

`readme_pattern_example_round_trips_against_mdatron_verify` reads the
README + extracts three fences + spawns one mdatron subprocess. ~50ms
total. Trivial cost.

## F7 — Catalog files load into binary at compile time, not runtime (Edge-cases)

**Classification:** dismissed
**Routing:** PerfE-internal verification.

`include_str!` resolves at compile time; the embedded `&'static str` is in
the binary's rodata section. Zero runtime load cost; the cmd_explain
match is dispatching among `&'static str` pointers. Fastest possible
path for the use case. Pass.

## F8 — Honest scope: Phase 2 surfaces don't have perf-critical paths (Consistency)

**Classification:** dismissed
**Routing:** PerfE-internal observation.

The Phase 2 work is operator-interactive: explain subcommand (single
invocation per call), README (read once), TTY diagnostic line (per finding
during a verify run). The verify pipeline itself (Phase 1's territory) is
the perf-load-bearing path; Phase 2 doesn't touch it.

Honest finding-class boundary: Phase 2 has minor perf surfaces but no
hot path. F2 + F3 + F5 are v0.1.x polish; none blocks v0.1.0.

# Summary

8 findings: 0 resolved-pending, 2 accepted-with-remediation (F2
format_tty allocation, F3 eprintln batching), 6 dismissed (F1 binary
size trivial, F4 explain O(1), F5 test scale fine, F6 round-trip
cheap, F7 include_str! compile-time, F8 no hot path). Phase 2 is not
perf-critical; v0.1.x polish opportunities exist if a heavy-corpus
adopter surfaces concrete need. No hallucinations.
