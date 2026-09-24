# windows-sys

**Status:** Approved (ratified by the operator 2026-09-24, #64 — the record
landed before the dependency, per the 2026-09-19 ruling).

**Pinned version:** `^0.61` (resolves to 0.61.2), Windows targets only, with
exactly the namespace features the handle walk needs: `Wdk_Foundation`,
`Wdk_Storage_FileSystem`, `Win32_Foundation`, `Win32_Storage_FileSystem`.

## Why this dependency

Windows is a declared target since 0.6.0 (drive-path resolution, CRLF-stable
scanners, a green `windows-latest` matrix that caught three real defects that
cycle). On Windows the confinement walk still uses the documented weaker
fallback: `symlink_metadata` on each component, then a *following* open. That
is a check-then-open window — a component swapped for a reparse point between
the check and the open is followed — so the swap-proof guarantee in DESIGN
§ Verification is fast where it is invoked is qualified Unix-only, and the
`MDATRON-E0012` explain page carries the same hedge. Windows has no `openat`;
the only handle-relative open is `NtCreateFile` with a `RootDirectory` handle
in `OBJECT_ATTRIBUTES`, which the standard library does not expose. This crate
provides the raw binding for exactly that call plus the reparse-attribute query
needed to refuse every reparse kind after the open.

**Alternatives considered:**

- `std` alone via `OpenOptionsExt::custom_flags(FILE_FLAG_OPEN_REPARSE_POINT |
  FILE_FLAG_BACKUP_SEMANTICS)`: rejected as the *whole* fix — it lets the leaf
  open refuse to follow a reparse point with zero new dependencies, but it is
  path-based (`CreateFileW` re-resolves the full path), so an intermediate
  component swapped between the parent open and the child open is still
  followed. It closes the leaf case only; the intermediate window is the
  guarantee this lane exists to deliver.
- `CreateFileW` with the same flags through this crate: rejected for the same
  reason — path-based, no `RootDirectory`; the crate's value is `NtCreateFile`.
- `windows` (the safe-wrapper sibling): rejected — far larger surface and
  generated wrapper layers for a call site that is one function; mirrors the
  `libc`-over-`rustix`/`nix` choice: keep the `unsafe` block small and
  auditable.

## Supply-chain notes

- **Version pin:** `windows-sys = { version = "0.61", features = [...] }`
  under `[target.'cfg(windows)'.dependencies]`; `Cargo.lock` committed, every
  CI job `--locked`. Feature-scoped so only the four namespaces' bindings
  compile.
- **Maintainer trust:** Microsoft, `microsoft/windows-rs` (Kenny Kerr); the
  canonical Windows bindings crate, ~1.6 billion downloads; MSRV 1.71, well
  under the pinned 1.88.
- **`cargo audit` / `cargo deny`:** to be verified clean at pin time; the
  license is inside the `deny.toml` allowlist; crates.io source.
- **Transitive deps:** none (`windows-sys` is a leaf: generated bindings, no
  runtime dependencies) — verify against `Cargo.lock` at pin time.

## Security notes

- **License:** MIT OR Apache-2.0; compatible.
- **Threat model:** used exclusively to strengthen path confinement (a security
  boundary), replacing a documented weaker posture. **Fail-closed by
  construction** — the operating rule for unsafe FFI that is iterated blind
  (no local Windows; `windows-latest` CI is the only executor): any
  `NTSTATUS` other than success denies; any handle whose
  `FILE_ATTRIBUTE_REPARSE_POINT` is set denies regardless of reparse tag
  (symlink, junction, and mount point are all reparse points and all refused —
  no tag allowlist); any unexpected object type denies; every ambiguity
  denies. An untested bug can therefore over-refuse (a loud `E0012`), never
  grant. The `unsafe` calls (`NtCreateFile`, the `OBJECT_ATTRIBUTES` /
  `UNICODE_STRING` construction) carry SAFETY comments naming the invariants
  (valid handle, NUL-free UTF-16 name, struct lifetimes outliving the call).
- **Refusals pinned in CI** (`windows-latest` red-gates): a symlinked
  intermediate directory, a junction, and a volume mount point each refused
  without disclosure; a plain file and a plain directory still open; the
  Unix-only test gates flip to `cfg(any(unix, windows))` where the guarantee
  becomes universal.
- **Non-Windows, non-Unix targets:** none are declared; the std fallback
  stays compiled under `cfg(not(any(unix, windows)))` with its carve-out
  comment, so the crate never compiles on a target that has no reason to
  carry it.

## Approval

- **Scope justification (Solution Owner):** the operator ruling of 2026-09-19
  declared Windows a target and scheduled the handle walk as the first
  post-0.6.0 item; the dependency is Windows-scoped, feature-minimal, and
  serves one security property.
- **Supply-chain (Platform Engineer):** first-party Microsoft bindings, leaf
  crate, feature-scoped, pinned and locked.
- **Security:** fail-closed FFI with CI-pinned refusals; on close, the
  `E0012` explain hedge and the DESIGN check-then-swap qualification revert to
  universal — the record of the carve-out is this file's history.
