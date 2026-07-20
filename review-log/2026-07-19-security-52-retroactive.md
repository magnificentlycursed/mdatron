# Security domain review (rust supplemental lens) — issue #52, confine module (L2 fix at 0b21986)

Reviewed at HEAD (2122a80) with the security primer, the Rust supplemental,
and DESIGN.md § Five check families / § Verification is fast where it is
invoked. Conducted inline by the session. Covers the seven mandated targets
from issue #52.

## Verdict

Deny-by-default holds everywhere examined: every identified race or
misclassification can relabel a refusal but never convert a refusal into
access, the unsafe block is sound with correct fd ownership on all paths, and
the unix walk delivers the handle-is-what-you-read property the design
demands. The two real exposures are (1) the non-unix fallback, whose weaker
guarantee is honest in the module docs but overclaimed by the E0012 explain
page and uncarved in DESIGN.md, and (2) glob expansion, which runs path-based
filesystem discovery before the handle discipline applies.

## Invariants (standing falsification paths for the module)

### I1: No adopter-supplied source may cause bytes outside the governed tree to be read
- falsification_path: decl source `/etc/hosts` (or symlink to it); build must error (E0010/E0012 class) with the target's content absent from every output; violated if any parse/diagnostic carries target bytes.
- currently_tested: yes (absolute_source_rejected_even_when_inside_root, symlink_source_pointing_outside_root_refused)

### I2: Parent segments are rejected lexically, target existence irrelevant
- falsification_path: sources `../x.yaml` with x existing above root, and `../../nope.yaml` absent; both must yield PathTraversal (E0011) before any filesystem access.
- currently_tested: yes (parent_traversal_rejected_when_target_exists, deep_parent_traversal_with_nonexistent_target_is_rejected)

### I3: Interior non-escaping parent segments are rejected outright
- falsification_path: `sub/../matrix.yaml` with matrix.yaml in-root; must be PathTraversal, violated if content is read.
- currently_tested: yes (interior_parent_segment_rejected_even_when_non_escaping)

### I4: A symlink at any component depth is refused without resolving its target
- falsification_path: symlink at leaf (in-root target), symlink at intermediate dir (outside target); both must be SymlinkRefused (E0012) naming the component; violated if refusal depends on where the link points or the target is read.
- currently_tested: yes (symlink_source_refused_even_when_target_is_inside_root, symlinked_intermediate_component_refused)

### I5: The handle that passed confinement is the handle that is read (unix)
- falsification_path: swap a checked component for a symlink between confinement and read (at the capture-complete seam once it exists); the read must come from the originally-opened handle, violated if post-swap target bytes appear.
- currently_tested: no — the seam does not exist yet; this is the check-then-swap seed mandated by the incremental-agreement acceptance criteria (phase 2a work).

### I6: Races can relabel a refusal, never grant access
- falsification_path: race the openat-ENOTDIR / fstatat disambiguation window (swap symlink-to-dir for real dir); outcome may flip Symlink<->Io but must remain Err; violated if any interleaving returns Ok.
- currently_tested: no (holds by construction — both branches are refusals; record as inspection evidence).

### I7: Every acquired descriptor is closed on every path
- falsification_path: count open fds before/after error-path calls (missing target, symlink refusal, deep walk); any growth is a leak.
- currently_tested: no (holds by OwnedFd construction; a leak test would pin it).

### I8: NUL-bearing components are refused safely as Io
- falsification_path: source "a\0b.yaml" (YAML can carry NUL into a Rust String); must be Io InvalidInput, violated by panic or by Symlink classification.
- currently_tested: no.

### I9: Glob expansion may read no file content and grant no match outside the root
- falsification_path: glob-matched symlink (covered); glob pattern whose expansion would traverse a symlinked intermediate directory (uncovered); any out-of-root match must error before open.
- currently_tested: partially (glob_matched_symlink_refused only).

### I10: Enumeration terminates in the presence of symlink cycles
- falsification_path: in-root symlink cycle + recursive glob pattern; expansion must terminate with bounded work.
- currently_tested: no — and the property currently rests on unverified glob-crate internals (see SEC-F2).

### I11 (non-unix): per-component no-follow check precedes the open, and the platform's weaker window is documented wherever the guarantee is claimed
- falsification_path: on a non-unix platform, swap a checked component for a symlink between symlink_metadata and File::open; the read follows the link — this window is the accepted residual and must be stated anywhere the handle guarantee is asserted.
- currently_tested: no (not testable on unix CI; documentation honesty is the enforceable part — see SEC-F1).

## Findings

### SEC-F1: E0012 explain page overclaims the handle guarantee; DESIGN.md carries no platform carve-out (medium)
- severity: medium
- file: mdatron-cli/src/explain/MDATRON-E0012.md:15-17 (also DESIGN.md § Verification is fast where it is invoked)
- defect: the explain page states unqualified "a link swapped after the check cannot redirect the read: the handle that passed confinement is the handle that is read," which is false on the non-unix fallback (symlink_metadata check, then a following File::open); confine.rs:165-168 is honest about this, the adopter-facing page and the design contract are not.
- attack_scenario: local writer on a Windows-hosted governed tree swaps a checked component for a symlink inside the check-to-open window and the read follows it — exactly the check-then-read gap the snapshot model exists to close, silently reopened per-platform while the operator-facing docs assert it closed.
- suggested_fix: qualify the explain page and record the platform carve-out in DESIGN.md via Raise-to-SO (spec-contract change authority), or implement a handle-based Windows walk (FILE_FLAG_OPEN_REPARSE_POINT) and delete the fallback's weakness.
- validator: read the three texts side by side; the inconsistency is textual.

### SEC-F2: glob expansion is path-based discovery before the handle discipline (medium)
- severity: medium
- file: mdatron-core/src/dsl/index.rs:174-197
- defect: expansion delegates directory walking to glob::glob, which uses path-based APIs whose symlink-following and cycle behavior the engine neither pins nor bounds — closed-world walking (DESIGN.md § Five check families) is asserted by the design but not enforced on this path.
- attack_scenario: adopter-supplied pattern plus an in-root symlinked directory: expansion enumerates directory listings outside the governed tree (filename disclosure into diagnostics via match paths) or, with a cycle, extends the walk unboundedly (hook-time DoS — the step budget from the design is not yet implemented on this path).
- suggested_fix: replace or wrap expansion with a no-follow, bounded walk (the same primitive the extras scan needs), and add the I9/I10 fixtures.
- validator: fixtures from I9/I10; observe enumeration through a symlinked dir and cycle termination.

### SEC-F3: out-of-root paths flow into diagnostic text unmarked (low)
- severity: low
- file: mdatron-core/src/dsl/index.rs:191-195
- defect: the strip_prefix failure branch puts the full out-of-root matched path into IndexError::PathTraversal's message — filesystem-derived text (adopter-influenced via the pattern) entering diagnostic output; the marking discipline treats adopter content as a trust surface.
- attack_scenario: hardening/documentation only today (the path is filesystem-derived, not free adopter bytes), but a crafted directory name inside a followed symlink target could put chosen bytes into the message line.
- suggested_fix: render the offending path under the quoted-content marking discipline (or report the root-relative pattern instead of the match).
- validator: fixture from SEC-F2's enumeration case; inspect the rendered finding.

### SEC-F4: errno taxonomy — EINTR unhandled, misclassification is fail-closed (low)
- severity: low
- file: mdatron-core/src/confine.rs:114-136
- defect: EINTR surfaces as Io (spurious transient failure, no retry); ELOOP/EMLINK/ENOTDIR disambiguation is correct on the named platforms; all misclassifications stay within Err.
- attack_scenario: none beyond transient false failures; access is never granted.
- suggested_fix: retry on EINTR; leave the rest.
- validator: inspection (I6 records the fail-closed argument).

### SEC-F5: libc dependency added without the dependency-approval discipline (informational)
- severity: informational
- file: mdatron-core/Cargo.toml:21-22
- defect: the security primer's supply-chain dimension (operator-directive 2026-05-27) requires SO + PE + Security investigation for new dependencies; libc landed with the hotfix, outside that discipline (already recorded as part of the #51 deviation).
- attack_scenario: process only; libc is workspace-pinned, ubiquitous, and cfg(unix)-scoped — retroactive approval is the right disposition, plus cargo-audit in CI if not already present.
- suggested_fix: record retroactive SO+PE+Security sign-off on this issue; verify cargo audit runs in the gauntlet.
- validator: tracker record + CI config inspection.

### SEC-F6: unsafe block soundness — clean (informational)
- severity: informational
- file: mdatron-core/src/confine.rs:93-163
- defect: none — recording the inspection: from_raw_fd consumes a freshly-owned fd (SAFETY comment accurate); mem::zeroed on libc::stat is sound (POD); `dir = openat_no_follow(&dir, ..)?` drops the parent handle at reassignment and every early return closes all handles; CString rejection path is safe.
- attack_scenario: n/a.
- suggested_fix: n/a.
- validator: I7's fd-count test would pin the lifecycle empirically.

## Coverage notes
All seven mandated targets covered: (1) unsafe block — SEC-F6; (2) OwnedFd
lifecycle — SEC-F6/I7; (3) errno taxonomy — SEC-F4/I6; (4) non-unix fallback —
SEC-F1/I11; (5) CString/NUL — I8; (6) glob-before-handle-walk — SEC-F2/F3,
I9/I10; (7) trusted root — verified: root comes from VerifyConfig/engine
callers, never from decl data; the root handle intentionally follows symlinks
in the root path itself (documented at confine.rs:71-74). Gap: glob crate
internals not read — SEC-F2 is filed as unpinned-and-load-bearing rather than
confirmed; the I9/I10 fixtures decide it either way. Validator-pair routing:
these findings go to Red Team for the probe pass per the primer.
