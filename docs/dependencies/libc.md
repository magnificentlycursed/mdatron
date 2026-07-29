# libc

**Status:** Approved.

**Pinned version:** `^0.2` (Unix targets only)

## Why this dependency

The standard library exposes no handle-relative no-follow open, which the
confinement model requires: `confine::open_confined` walks each path component
via `openat` with `O_NOFOLLOW | O_CLOEXEC | O_DIRECTORY`, so a symlinked
intermediate component is refused exactly like a symlinked leaf (DESIGN.md
§ Verification is fast where it is invoked; the swap-proof guarantee). Only the
Unix implementation links libc; the non-unix fallback (documented weaker
carve-out, #56/#64) uses std alone.

**Alternatives considered:**

- `std::fs` alone: rejected — no `openat`/`O_NOFOLLOW`, so no handle-relative
  no-follow walk; confinement would decide on a re-walked path (the
  check-then-read gap this design closes).
- `rustix` / `nix`: heavier safe wrappers over the same syscalls; the direct
  libc surface used here is small (`openat`, a few flags, error codes) and
  keeps the unsafe block auditable and minimal.

## PE supply-chain notes

- **Version pin:** `libc = "0.2"` under `[target.'cfg(unix)'.dependencies]`.
- **Maintainer trust:** rust-lang; the canonical FFI bindings crate.
- **`cargo audit`:** clean at pin time.

## Security notes

- **License:** MIT OR Apache-2.0; compatible.
- **Threat model:** used exclusively to strengthen path confinement (a security
  boundary). The `unsafe` FFI calls carry SAFETY comments; misclassification
  stays fail-closed (a failed open denies access, never grants it).

## SO approval

- **Scope justification:** the openat surface is what makes confinement
  handle-decided rather than path-decided — a core security property; the dep
  is Unix-scoped and minimal.

## Co-authorship attribution

Per VSDD-E0100 discipline:

```
Co-authored-by: Solution Owner <so@vsdd-domains>
Co-authored-by: Platform Engineer <pe@vsdd-domains>
Co-authored-by: Security <security@vsdd-domains>
```
