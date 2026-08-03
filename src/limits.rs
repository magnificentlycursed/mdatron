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

/// A held invocation slot: releasing (dropping) it frees the slot. The OS
/// releases it on process death too — the lock is advisory `flock` on unix and
/// a `share_mode(0)` exclusive open on windows, both of which evaporate with
/// the owning process, so a crashed run can never wedge the count (no stale
/// lockfile class).
#[derive(Debug)]
pub struct InvocationSlot {
    // Held only for its OS-level lock; never read. Dropping closes and
    // releases.
    _file: std::fs::File,
}

/// Acquire one of the `limit` per-root invocation slots, or report that every
/// slot is busy (the caller maps `None` to the `concurrent-invocation-count`
/// bound diagnostic). Slot files live under the system temp directory, keyed
/// by a digest of the canonicalized root — never inside the repository, so no
/// VCS noise and no interaction with the `.mdatron/` managed partition.
///
/// Platforms without either lock primitive (neither unix nor windows) run
/// unbounded — the same documented, platform-scoped carve-out posture as the
/// confine fallback.
pub fn acquire_invocation_slot(
    project_root: &Path,
    limit: usize,
) -> std::io::Result<Option<InvocationSlot>> {
    use sha2::{Digest, Sha256};
    let canonical = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    let dir = {
        let mut short = format!("{digest:x}");
        short.truncate(16);
        std::env::temp_dir().join(format!("mdatron-slots-{short}"))
    };
    std::fs::create_dir_all(&dir)?;
    for i in 0..limit {
        let path = dir.join(format!("slot-{i}"));
        if let Some(slot) = try_lock_slot(&path)? {
            return Ok(Some(slot));
        }
    }
    Ok(None)
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
}
