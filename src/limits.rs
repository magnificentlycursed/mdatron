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
    /// Maximum concurrent verify invocations per project root
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

/// Acquire one of the `limit` per-root invocation slots, or report that every
/// slot is busy (the caller maps `None` to the `concurrent-invocation-count`
/// bound diagnostic). Slot files live under the system temp directory — never
/// inside the repository (no VCS noise, no `.mdatron/` managed-partition
/// interaction) — keyed by the effective uid AND a digest of the
/// canonicalized root, in a `0o700` directory whose ownership is verified
/// (phase-3 B-1): on a shared host, two users verifying the same checkout get
/// DISJOINT per-user pools instead of one user's directory permissions
/// failing the other's runs, and a foreign or symlinked directory at the
/// expected path is refused with a diagnostic naming the problem rather than
/// a bare EACCES. (Residual accepted: per-user pools mean the count bounds
/// each user's runs, not the machine total — the bound's purpose is runaway
/// hook fan-out, which is per-user in practice.)
///
/// Platforms without either lock primitive (neither unix nor windows) run
/// unbounded — the same documented, platform-scoped carve-out posture as the
/// confine fallback.
pub fn acquire_invocation_slot(
    project_root: &Path,
    limit: usize,
) -> std::io::Result<Option<InvocationSlot>> {
    let dir = slot_dir(project_root)?;
    for i in 0..limit {
        let path = dir.join(format!("slot-{i}"));
        if let Some(slot) = try_lock_slot(&path)? {
            return Ok(Some(slot));
        }
    }
    Ok(None)
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

#[cfg(unix)]
fn slot_dir(project_root: &Path) -> std::io::Result<std::path::PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
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
        // A regular FILE at the path lands here (EEXIST): fall through so the
        // metadata checks below name what occupies the path (R2-4), instead
        // of a bare "File exists".
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    // Refuse a hostile pre-created path: a symlink, a non-directory, or a
    // directory owned by another uid at OUR uid-keyed name is an attack or a
    // misconfiguration — name it, never proceed onto foreign state.
    //
    // Residual (documented, R2-3): after these checks the slot files are
    // opened BY PATH. On a sticky or per-user temp parent (Linux /tmp, the
    // macOS and Windows per-user temp dirs) the verified directory cannot be
    // swapped by another user; a SHARED, NON-STICKY custom TMPDIR retains a
    // small check-to-open window — a platform-scoped tolerated posture in the
    // same class as the confine fallback carve-out, with a blast radius of a
    // 0-byte create/flock at an attacker-chosen path.
    let meta = std::fs::symlink_metadata(&dir)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(std::io::Error::other(format!(
            "invocation-slot path '{}' exists but is not a real directory \
             (symlink refused); point TMPDIR at a private directory, or remove \
             the entry",
            dir.display()
        )));
    }
    if meta.uid() != uid {
        return Err(std::io::Error::other(format!(
            "invocation-slot directory '{}' is owned by uid {} (expected {uid}); \
             point TMPDIR at a private directory (removing another user's /tmp \
             entry is usually not possible)",
            dir.display(),
            meta.uid()
        )));
    }
    // A same-uid directory pre-created with permissive modes (tooling,
    // archive extraction, an old build) would let other users open and flock
    // the slot files — flock needs no write bit — quietly re-opening the
    // slot-holding denial lever (R2-2). We own it: repair to the declared
    // 0700 posture instead of refusing.
    if meta.mode() & 0o077 != 0 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

#[cfg(not(unix))]
fn slot_dir(project_root: &Path) -> std::io::Result<std::path::PathBuf> {
    // Windows: `temp_dir()` is already per-user (%LOCALAPPDATA%\Temp), so the
    // uid keying and ownership check are inherent in the location.
    let dir =
        std::env::temp_dir().join(format!("mdatron-slots-{}", root_digest_short(project_root)));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
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
            let slot = acquire_invocation_slot(&root, limit)
                .unwrap()
                .expect("a free slot while under the limit");
            held.push(slot);
        }
        assert!(
            acquire_invocation_slot(&root, limit).unwrap().is_none(),
            "the N+1st concurrent invocation must find every slot busy"
        );
        held.pop();
        assert!(
            acquire_invocation_slot(&root, limit).unwrap().is_some(),
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
        let dir = slot_dir(&root).unwrap();
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
        let dir = slot_dir(&root).unwrap();
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
        let dir = slot_dir(&root).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let slot = acquire_invocation_slot(&root, 2).unwrap();
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
