//! The declared-limits catalog (#92 sub-lane D; `DESIGN.md` § Verification is
//! fast where it is invoked: "Hook-time cost is bounded by declared limits
//! shipped as data").
//!
//! ONE data structure declares every input/enumeration bound the engine
//! enforces, with the bound-name strings that surface in `bound_exceeded`
//! diagnostics — so the catalog, the enforcement sites, and the operator-facing
//! documentation (`docs/limits.md`) cannot drift apart. Enforcement sites take
//! their values from here; the historical `pub const` names re-export the
//! shipped values for compatibility.
//!
//! Two bound classes are deliberately NOT in this catalog: the YAML alias
//! (`repetition limit exceeded`) and recursion (`recursion limit exceeded`)
//! guards ride the parser (`serde_yaml_ng`) and surface as parse diagnostics —
//! pinned by fixture, not re-implemented; and there is no global wall-clock
//! budget (the DESIGN § enforcement-status note records that honestly).

use std::path::Path;

/// The shipped limits, as one declared value. Field order mirrors the DESIGN
/// bounds sentence.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Maximum bytes read for any single captured input (`max-input-size-per-file`).
    pub per_file_bytes: usize,
    /// Maximum total bytes stored in one run's snapshot (`aggregate-snapshot-size`).
    pub aggregate_bytes: usize,
    /// Maximum flow-collection nesting in a governed body's YAML
    /// (`structural-nesting-depth`).
    pub structural_nesting: usize,
    /// Maximum DSL expression nesting depth (enforced at expression parse;
    /// a deeper `assert:` is a `ParseError`).
    pub expr_depth: usize,
    /// Maximum directory depth for the engine-owned no-follow glob walk
    /// (`depth` in `WalkBounded`).
    pub walk_depth: usize,
    /// Maximum directory entries listed across one glob walk (`entries` in
    /// `WalkBounded`).
    pub walk_entries: usize,
    /// Maximum concurrent verify invocations per user per project root
    /// (`concurrent-invocation-count`).
    pub concurrent_invocations: usize,
}

/// The shipped catalog. Generous phase-1 values; every change here is a
/// contract change and lands with its `docs/limits.md` row.
pub const SHIPPED: Limits = Limits {
    per_file_bytes: 8 * 1024 * 1024,
    aggregate_bytes: 64 * 1024 * 1024,
    structural_nesting: 256,
    expr_depth: 256,
    walk_depth: 64,
    walk_entries: 100_000,
    concurrent_invocations: 8,
};

/// A held invocation slot: releasing (dropping) it frees the slot. The LOCK
/// (not the file) dies with the process — advisory `flock` on unix, a
/// `share_mode(0)` exclusive open on windows — so a crashed run can never
/// wedge the count. The 0-byte slot FILES are deliberate durable litter:
/// they are never unlinked, because unlinking a slot another process holds
/// would let a fresh acquirer re-create and lock a NEW inode at the same
/// path — the classic flock-on-unlinked-inode double-admit past the limit.
/// Any future cleanup must respect that invariant (per-root cost is one
/// directory and up to `limit` empty files, reaped by the OS temp cleaner).
#[derive(Debug)]
pub struct InvocationSlot {
    // Held only for its OS-level lock; never read. Dropping closes and
    // releases.
    _file: std::fs::File,
}

/// The result of an acquisition attempt: a held slot, or a busy pool (which
/// the caller maps to the `concurrent-invocation-count` diagnostic). `Busy`
/// carries whether the slot directory was just repaired from a permissive
/// mode — because a lock a foreign process took THROUGH that window survives
/// the repair (chmod revokes no held fd/lock, and never-unlink forbids
/// rotating the slot files out), so a busy-after-repair pool may be
/// foreign-held rather than genuinely N-concurrent (#103 phase-3 R3-1).
#[derive(Debug)]
pub enum SlotOutcome {
    Acquired(InvocationSlot),
    Busy { repaired_permissive_dir: bool },
}

/// Acquire one of the `limit` per-root invocation slots, or report that every
/// slot is busy. Slot files live under the system temp directory — never
/// inside the repository (no VCS noise, no `.mdatron/` managed-partition
/// interaction) — keyed by the effective uid AND a digest of the
/// canonicalized root, in a `0o700` directory verified through a no-follow
/// handle (phase-3 B-1/R3-2): on a shared host, two users verifying the same
/// checkout get DISJOINT per-user pools instead of one user's directory
/// permissions failing the other's runs, and a foreign, symlinked, or
/// non-directory path is refused with a named diagnostic. (Residual accepted:
/// per-user pools mean the count bounds each user's runs, not the machine
/// total — the bound's purpose is runaway hook fan-out, which is per-user in
/// practice.)
///
/// Platforms without either lock primitive (neither unix nor windows) run
/// unbounded — the same documented, platform-scoped carve-out posture as the
/// confine fallback.
pub fn acquire_invocation_slot(project_root: &Path, limit: usize) -> std::io::Result<SlotOutcome> {
    let (dir, repaired_permissive_dir) = slot_dir(project_root)?;
    for i in 0..limit {
        let path = dir.join(format!("slot-{i}"));
        if let Some(slot) = try_lock_slot(&path)? {
            return Ok(SlotOutcome::Acquired(slot));
        }
    }
    Ok(SlotOutcome::Busy {
        repaired_permissive_dir,
    })
}

fn root_digest_short(project_root: &Path) -> String {
    use sha2::{Digest, Sha256};
    let canonical = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    let mut short = format!("{digest:x}");
    short.truncate(16);
    short
}

/// Returns the verified slot directory and whether it was repaired from a
/// permissive mode this call.
#[cfg(unix)]
fn slot_dir(project_root: &Path) -> std::io::Result<(std::path::PathBuf, bool)> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
    // SAFETY: geteuid takes no arguments and cannot fail.
    let uid = unsafe { libc::geteuid() };
    let dir = std::env::temp_dir().join(format!(
        "mdatron-slots-{uid}-{}",
        root_digest_short(project_root)
    ));
    match std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
    {
        Ok(()) => {}
        // A pre-existing file/symlink/dir at the path lands here (EEXIST): the
        // no-follow open below decides its fate.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    // Open the directory through a NO-FOLLOW handle, and do every check AND
    // the repair on THAT HANDLE, never the path (R3-2): a symlink or a
    // non-directory squatting the path fails the open (ELOOP/ENOTDIR) and is
    // named; a swap after the open cannot redirect the fstat or the fchmod.
    // Only the by-path slot-file opens below remain a residual, tolerated on a
    // sticky or per-user temp parent (Linux /tmp, the macOS and Windows
    // per-user temp dirs) exactly as the confine fallback's check-to-open
    // window is — see the module note.
    let handle = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(&dir)
        .map_err(|e| {
            std::io::Error::other(format!(
                "invocation-slot path '{}' is not a real directory ({e}); point \
                 TMPDIR at a private directory, or remove the entry",
                dir.display()
            ))
        })?;
    let meta = handle.metadata()?; // fstat on the handle
    if meta.uid() != uid {
        return Err(std::io::Error::other(format!(
            "invocation-slot directory '{}' is owned by uid {} (expected {uid}); \
             point TMPDIR at a private directory (removing another user's /tmp \
             entry is usually not possible)",
            dir.display(),
            meta.uid()
        )));
    }
    // A same-uid directory pre-created with permissive modes (tooling, archive
    // extraction, an old build) would let other users open and flock the slot
    // files — flock needs no write bit — quietly re-opening the slot-holding
    // denial lever (R2-2). We own it: repair to 0700 via fchmod on the handle
    // (race-free). NOTE (R3-1): chmod revokes no fd or lock a foreign process
    // ALREADY took through the permissive window, and never-unlink forbids
    // rotating the slot files out — so the repair closes FUTURE opens, not
    // locks already held. The caller flags a busy-after-repair pool as
    // possibly foreign-held rather than genuinely N-concurrent.
    let mut repaired = false;
    if meta.mode() & 0o077 != 0 {
        handle.set_permissions(std::fs::Permissions::from_mode(0o700))?; // fchmod
        repaired = true;
    }
    Ok((dir, repaired))
}

#[cfg(not(unix))]
fn slot_dir(project_root: &Path) -> std::io::Result<(std::path::PathBuf, bool)> {
    // Windows: `temp_dir()` is already per-user (%LOCALAPPDATA%\Temp), so the
    // uid keying and ownership check are inherent in the location.
    let dir =
        std::env::temp_dir().join(format!("mdatron-slots-{}", root_digest_short(project_root)));
    std::fs::create_dir_all(&dir)?;
    Ok((dir, false))
}

#[cfg(unix)]
fn try_lock_slot(path: &Path) -> std::io::Result<Option<InvocationSlot>> {
    use std::os::fd::AsRawFd;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    // LOCK_NB: a busy slot is an immediate "try the next one", never a wait.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        Ok(Some(InvocationSlot { _file: file }))
    } else {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
            Ok(None)
        } else {
            Err(err)
        }
    }
}

#[cfg(windows)]
fn try_lock_slot(path: &Path) -> std::io::Result<Option<InvocationSlot>> {
    use std::os::windows::fs::OpenOptionsExt;
    // share_mode(0): exclusive access for the lifetime of the handle; a second
    // open fails with a sharing violation, and the OS releases on process
    // death.
    match std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .share_mode(0)
        .open(path)
    {
        Ok(file) => Ok(Some(InvocationSlot { _file: file })),
        Err(e) if e.raw_os_error() == Some(32) => Ok(None), // ERROR_SHARING_VIOLATION
        Err(e) => Err(e),
    }
}

#[cfg(not(any(unix, windows)))]
fn try_lock_slot(path: &Path) -> std::io::Result<Option<InvocationSlot>> {
    // No portable auto-releasing lock primitive: the bound is unenforced here,
    // the documented platform carve-out (mirrors the confine fallback note).
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    Ok(Some(InvocationSlot { _file: file }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test helper: the held slot, or None when the pool is busy.
    fn acquired(outcome: SlotOutcome) -> Option<InvocationSlot> {
        match outcome {
            SlotOutcome::Acquired(slot) => Some(slot),
            SlotOutcome::Busy { .. } => None,
        }
    }

    // N slots serve N holders; the N+1st acquisition reports busy; dropping a
    // guard frees its slot. flock is per open-file-description, so in-process
    // holders contend exactly like separate processes.
    #[test]
    fn slots_bound_concurrent_holders_and_release_on_drop() {
        let root = std::env::temp_dir().join(format!(
            "mdatron-slot-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let limit = 3;
        let mut held = Vec::new();
        for _ in 0..limit {
            let slot = acquired(acquire_invocation_slot(&root, limit).unwrap())
                .expect("a free slot while under the limit");
            held.push(slot);
        }
        assert!(
            acquired(acquire_invocation_slot(&root, limit).unwrap()).is_none(),
            "the N+1st concurrent invocation must find every slot busy"
        );
        held.pop();
        assert!(
            acquired(acquire_invocation_slot(&root, limit).unwrap()).is_some(),
            "dropping a guard frees its slot"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // Phase-3 B-1: a symlink pre-created at the uid-keyed slot path is an
    // attack (or wreckage) and is refused with a diagnostic naming it —
    // never followed onto foreign state.
    #[cfg(unix)]
    #[test]
    fn symlinked_slot_directory_is_refused() {
        let root = std::env::temp_dir().join(format!(
            "mdatron-slot-sym-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        // Learn the expected path, then replace it with a symlink elsewhere.
        let (dir, _) = slot_dir(&root).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let elsewhere = root.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &dir).unwrap();

        let err = acquire_invocation_slot(&root, 2).unwrap_err();
        assert!(
            err.to_string().contains("not a real directory"),
            "the refusal names the symlink; got: {err}"
        );
        let _ = std::fs::remove_file(&dir);
        let _ = std::fs::remove_dir_all(&root);
    }

    // R2-4: a regular FILE squatting the slot-dir path is refused BY NAME,
    // not as a bare EEXIST.
    #[cfg(unix)]
    #[test]
    fn regular_file_at_slot_dir_path_is_refused_by_name() {
        let root = std::env::temp_dir().join(format!(
            "mdatron-slot-file-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (dir, _) = slot_dir(&root).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::write(&dir, b"squatter").unwrap();
        let err = acquire_invocation_slot(&root, 2).unwrap_err();
        assert!(
            err.to_string().contains("not a real directory"),
            "the refusal names the squatter; got: {err}"
        );
        let _ = std::fs::remove_file(&dir);
        let _ = std::fs::remove_dir_all(&root);
    }

    // R2-2: a same-uid slot dir pre-created with permissive modes is REPAIRED
    // to 0700 (other users could otherwise flock the slots — flock needs no
    // write bit), never accepted as-is.
    #[cfg(unix)]
    #[test]
    fn permissive_same_uid_slot_dir_is_repaired_to_0700() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "mdatron-slot-mode-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (dir, _) = slot_dir(&root).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        // slot_dir reports the repair; acquisition still yields a slot.
        let (_, repaired) = slot_dir(&root).unwrap();
        assert!(repaired, "the permissive mode is reported as repaired");
        let slot = acquired(acquire_invocation_slot(&root, 2).unwrap());
        assert!(
            slot.is_some(),
            "a same-uid permissive dir is repaired, not refused"
        );
        let mode = std::fs::symlink_metadata(&dir)
            .unwrap()
            .permissions()
            .mode()
            & 0o7777;
        assert_eq!(mode, 0o700, "the dir is repaired to the declared posture");
        drop(slot);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Drift tripwire (reference: ruff-registry-audit — hand-synced peer
    // artifacts drift unless a check forces agreement): every SHIPPED value
    // must appear in its docs/limits.md table row, and SHIPPED itself must
    // carry the values those rows render.
    #[test]
    fn docs_limits_table_matches_shipped() {
        let doc =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/limits.md"))
                .unwrap();
        let row = |needle: &str| {
            doc.lines()
                .find(|l| l.starts_with('|') && l.contains(needle))
                .unwrap_or_else(|| panic!("docs/limits.md lacks a table row for {needle}"))
        };
        // Pipe-delimited cells (R2-1): a bare "8 MiB" substring would stay
        // green under a doc-side bump to "128 MiB".
        assert_eq!(SHIPPED.per_file_bytes, 8 * 1024 * 1024);
        assert!(row("max-input-size-per-file").contains("| 8 MiB |"));
        assert_eq!(SHIPPED.aggregate_bytes, 64 * 1024 * 1024);
        assert!(row("aggregate-snapshot-size").contains("| 64 MiB |"));
        assert_eq!(SHIPPED.structural_nesting, 256);
        assert!(row("structural-nesting-depth").contains("| 256 |"));
        assert_eq!(SHIPPED.expr_depth, 256);
        assert!(row("DSL expression depth").contains("| 256 |"));
        assert_eq!(SHIPPED.walk_depth, 64);
        assert!(row("walk `depth`").contains("| 64 |"));
        assert_eq!(SHIPPED.walk_entries, 100_000);
        assert!(row("walk `entries`").contains("| 100 000 |"));
        assert_eq!(SHIPPED.concurrent_invocations, 8);
        assert!(row("concurrent-invocation-count").contains("| 8 |"));
    }
}
