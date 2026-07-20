# Software Engineer domain review — issue #52, confine module (L2 fix at 0b21986)

Reviewed at HEAD (2122a80) with the software-engineer primer, the Rust supplemental,
and DESIGN.md § Five check families / § Verification is fast where it is invoked.
Conducted inline by the session (subagent spawn unavailable during review window).

## Verdict

The core of the fix is sound and correctly minimal: the two-layer contract
(lexical decision, then handle-based open) matches the ratified design, the
lexical layer is exactly the smallest code that enforces the contract, error
paths name what failed with the operator's recovery action on the explain
pages, and fd lifecycle is correct on every path including early returns. The
weak flank is everything the glob crate touches: pattern construction, its
enumeration semantics, and the API's tolerance of unconfined input. None of
the findings reopens the L2 escape; all are periphery.

## Findings

### F1: open_confined silently sanitizes non-Normal components (major)
- severity: major
- file: mdatron-core/src/confine.rs:76-82
- defect: `open_confined` filter_maps away RootDir/Prefix/ParentDir components instead of rejecting them, so a caller that skips `confine_lexically` gets silent wrong-file access rather than an error.
- failure_scenario: future caller passes `Path::new("../secret.yaml")` directly to `open_confined`; the ParentDir component is dropped and `root/secret.yaml` opens successfully — misuse invisible at the call site. Current callers are correctly paired; nothing enforces the pairing.
- suggested_fix: return a ConfinedPath newtype from `confine_lexically` and make `open_confined` accept only it (or error on any non-Normal component).
- validator: unit test calling `open_confined(root, Path::new("../x"))` and asserting an error, run against current code — it opens `root/x` today.

### F2: project_root interpolated unescaped into the glob pattern (major)
- severity: major
- file: mdatron-core/src/dsl/index.rs:178-180
- defect: the glob pattern is `project_root.join(rel).to_string_lossy()`, so glob metacharacters in the ROOT path (`[`, `?`, `*`) are interpreted as pattern syntax.
- failure_scenario: project root `/Users/x/proj[1]` — `[1]` parses as a char class matching `1`, so every glob source enumerates `/Users/x/proj1/...` (a sibling tree) and legitimate in-root globs match nothing; matches from the sibling tree then fail strip_prefix and surface as PathTraversal with the sibling's paths in the message. Deny-by-default holds (no content read), but in-root globs are silently broken for such roots.
- suggested_fix: glob::Pattern::escape the root portion, or enumerate relative to the root (e.g., chdir-free manual walk or pattern matching against strip_prefixed candidates).
- validator: create a root containing `[1]` in a path component, declare source `*.yaml` with a matching in-root file, build the registry — the file is not found.

### F3: glob enumeration semantics are load-bearing and unverified (major, test gap)
- severity: major
- file: mdatron-core/src/dsl/index.rs:174-197
- defect: DESIGN.md's closed-world discipline ("symlinks are not followed during enumeration ... symlink cycles cannot extend a walk") is delegated to glob::glob's internal directory walk, whose follow/no-follow and cycle behavior is neither pinned by the code nor exercised by any test.
- failure_scenario: a symlinked directory inside the root pointing at a large or cyclic tree; if glob's walk follows it, enumeration reads directory listings outside the governed tree (or fails to terminate on a cycle) before the handle-based open ever runs. The existing glob_matched_symlink_refused test covers only a symlink at the LEAF of a match.
- suggested_fix: add fixtures — (a) glob pattern `sub/*.yaml` where `sub` is a symlink to an outside dir, (b) a symlink-cycle fixture under a recursive pattern — and, if glob follows, replace expansion with a no-follow walk; this also feeds the phase-2a fixture-corpus audit (issue #52 item 5).
- validator: the two fixtures above; observe whether expansion enumerates through the link and whether it terminates.

### F4: EINTR from openat surfaces as a spurious Io error (minor)
- severity: minor
- file: mdatron-core/src/confine.rs:114-116
- defect: raw libc::openat is not retried on EINTR (std's open does this internally; the raw call does not).
- failure_scenario: signal delivery during the walk yields a transient MDATRON-mapped Io failure on a valid path — a false negative under the no-silent-degradation discipline (the state is reported, but as the wrong condition).
- suggested_fix: loop on EINTR around the openat call.
- validator: code inspection; deterministic test requires signal injection — reasonable to accept on inspection.

### F5: non-UTF8 path degradation via to_string_lossy in the glob pattern (minor)
- severity: minor
- file: mdatron-core/src/dsl/index.rs:179
- defect: a non-UTF8 root or pattern byte sequence is replaced with U+FFFD before glob parsing, so matching runs against a corrupted pattern.
- failure_scenario: non-UTF8 bytes in the root path on unix; globs silently match nothing.
- suggested_fix: fold into the F2 rework (enumerate relative to the root using OsStr-preserving APIs).
- validator: fixture with a non-UTF8 directory name (unix), glob source inside it.

### F6: build_index trims the source string (note)
- severity: note
- file: mdatron-core/src/dsl/index.rs:136
- defect: `decl.source.trim()` makes a filename with leading/trailing whitespace unreachable, silently.
- failure_scenario: quality only; also mildly helpful (`" /etc/x"` still rejects as absolute after trim).
- suggested_fix: none required; document, or drop the trim and let the Io error name the real string.
- validator: n/a.

### F7: leaf-directory open and deep-nesting untested (note)
- severity: note
- file: mdatron-core/src/confine.rs:161
- defect: the leaf opens without O_DIRECTORY, so a directory at the leaf yields an open handle whose read fails later (EISDIR at parse time) — behavior fine, taxonomy untested; no test exercises >2-deep nesting or a directory-as-source.
- failure_scenario: quality/coverage only.
- suggested_fix: two small unit tests in confine.rs.
- validator: the tests themselves.

## Coordination flags
- F3 → Quality Engineer (fixture-corpus audit, phase 2a) and Platform Engineer (glob crate behavior is a platform/dependency question).
- F1 → Solution Architect only if the newtype is judged an architectural seam change; otherwise sanity-check pair.

## Coverage notes
Reviewed: confine.rs in full, index.rs build/resolve path and confinement test
section in full, codes.rs, Cargo.toml, explain pages E0010-E0012. Not run:
cargo test / clippy (tooling unavailable in the review window — the red-gate
retrofit run covers the executable leg separately). Not reviewed: the glob
crate's source (F3 is filed as unverified-and-load-bearing, not as a
confirmed defect).
