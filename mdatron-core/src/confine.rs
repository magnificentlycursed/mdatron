//! Path confinement: component-wise resolution and validated no-follow opens
//! under a governed root.
//!
//! Implements the family-wide confinement discipline of `DESIGN.md` § Five
//! check families: adopter-supplied paths resolve inside the governed tree,
//! with parent-directory, absolute-path, and symlink escapes rejected —
//! including paths whose targets do not exist. Two layers:
//!
//! 1. [`confine_lexically`] decides confinement on the path text alone,
//!    component-wise, touching no filesystem — so a non-existent target is
//!    judged on exactly the same basis as an existing one. (The predecessor
//!    helper canonicalized both sides and fell back to the un-normalized
//!    textual path when the target did not exist, which `starts_with` then
//!    compared component-wise — `root/../..` could pass. The path-confinement
//!    defect issue in this tracker records the hole.)
//! 2. [`open_confined`] opens each component relative to its parent
//!    directory's handle with `O_NOFOLLOW`, so a symlinked intermediate
//!    component is refused exactly like a symlinked final one, and the handle
//!    returned is the handle the caller reads — confinement is decided on the
//!    handle (`DESIGN.md` § Verification is fast where it is invoked), not on
//!    a path that could be swapped between check and read.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::path::{Component, Path, PathBuf};

/// A path proven confinement-safe: relative, composed only of normal
/// components (no root/prefix, no `..`). The **sole** constructor is
/// [`confine_lexically`], and [`open_confined`] accepts nothing else — so the
/// lexical check can never be skipped, and a caller cannot hand `open_confined`
/// a `../secret.yaml` that would otherwise be silently sanitized to
/// `root/secret.yaml` (the defect issue in this tracker records the hole).
///
/// The invariant — every component is [`Component::Normal`] — is established at
/// construction and never mutated, so [`open_confined`] needs no re-validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfinedPath(PathBuf);

impl ConfinedPath {
    /// The normalized, root-relative path. Borrowing is read-only: the
    /// confinement invariant cannot be broken through this view.
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// A confinement violation decidable from the path text alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexicalViolation {
    /// Absolute path (a root or prefix component). MDATRON-E0010 territory.
    Absolute,
    /// A `..` component. Parent segments are rejected outright, escaping or
    /// not, and in glob patterns too (BOUNDARY-PREAMBLE § 7, carried per
    /// `DESIGN.md` § Five check families). MDATRON-E0011 territory.
    ParentSegment,
}

/// A violation surfaced while opening a lexically-confined path.
#[derive(Debug)]
pub enum OpenViolation {
    /// A component was a symbolic link. No-follow resolution refuses it
    /// whatever its target — inside or outside the governed tree.
    /// MDATRON-E0012 territory.
    Symlink { component: PathBuf },
    /// Ordinary IO failure (not found, permission, not-a-directory).
    Io(io::Error),
}

/// Decide confinement of an adopter-supplied source path on its text alone.
///
/// Accepts only relative paths made of normal components (`.` is dropped);
/// returns a [`ConfinedPath`] wrapping the normalized root-relative path.
/// Requires no filesystem access, so targets that do not exist are rejected on
/// the same basis as targets that do. This is the only way to obtain a
/// `ConfinedPath`, and hence the only entry to [`open_confined`].
pub fn confine_lexically(source: &Path) -> Result<ConfinedPath, LexicalViolation> {
    let mut rel = PathBuf::new();
    for component in source.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => return Err(LexicalViolation::Absolute),
            Component::ParentDir => return Err(LexicalViolation::ParentSegment),
            Component::CurDir => {}
            Component::Normal(c) => rel.push(c),
        }
    }
    Ok(ConfinedPath(rel))
}

/// Open a [`ConfinedPath`] under `root` through validated no-follow handles
/// and return the handle the caller must read from.
///
/// Taking a `ConfinedPath` (rather than a bare `&Path`) is the fix for the
/// silent-sanitize defect: a path carrying a `..` or root component cannot be
/// constructed as a `ConfinedPath`, so it can never reach this open — the
/// pairing with [`confine_lexically`] is enforced by the type, not by caller
/// discipline. The example below does not compile:
///
/// ```compile_fail
/// use mdatron_core::confine::open_confined;
/// use std::path::Path;
/// // open_confined requires a &ConfinedPath; a raw &Path is rejected by the
/// // type checker, so an unconfined "../secret.yaml" can never be opened.
/// let _ = open_confined(Path::new("/tmp"), Path::new("../secret.yaml"));
/// ```
///
/// `root` is engine-supplied and trusted; symlinks in the root path itself
/// (e.g. macOS `/var` → `/private/var`) are permitted. Every component of
/// `rel` below it is opened relative to its parent directory's handle with
/// no-follow semantics, so a symlink at any depth is refused.
pub fn open_confined(root: &Path, rel: &ConfinedPath) -> Result<File, OpenViolation> {
    // Every component is Normal by the ConfinedPath invariant (established in
    // confine_lexically), so this is a straight projection to the names — no
    // component is silently dropped.
    let components: Vec<&std::ffi::OsStr> = rel
        .0
        .components()
        .map(|c| match c {
            Component::Normal(name) => name,
            other => {
                unreachable!("ConfinedPath invariant violated: non-normal component {other:?}")
            }
        })
        .collect();
    if components.is_empty() {
        return Err(OpenViolation::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "empty source path",
        )));
    }
    open_confined_impl(root, &components)
}

#[cfg(unix)]
fn open_confined_impl(root: &Path, components: &[&std::ffi::OsStr]) -> Result<File, OpenViolation> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;

    fn openat_no_follow(
        dirfd: &OwnedFd,
        name: &std::ffi::OsStr,
        directory: bool,
    ) -> Result<OwnedFd, OpenViolation> {
        let c_name = CString::new(name.as_bytes()).map_err(|_| {
            OpenViolation::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path component contains NUL",
            ))
        })?;
        let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        if directory {
            flags |= libc::O_DIRECTORY;
        }
        // Retry on EINTR: signal delivery mid-syscall must not surface as a
        // spurious transient Io. Misclassification stays fail-closed — the
        // retry only re-attempts the same no-follow open, never grants access.
        let fd = loop {
            let fd = unsafe { libc::openat(dirfd.as_raw_fd(), c_name.as_ptr(), flags) };
            if fd < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break fd;
        };
        if fd < 0 {
            let err = io::Error::last_os_error();
            // O_NOFOLLOW on a symlink: ELOOP on Linux and macOS, EMLINK on
            // FreeBSD-lineage systems. With O_DIRECTORY added, macOS reports
            // a symlink-to-directory as ENOTDIR instead — disambiguate from a
            // genuine file-as-directory with a handle-relative no-follow stat.
            let symlink = match err.raw_os_error() {
                Some(libc::ELOOP) | Some(libc::EMLINK) => true,
                Some(libc::ENOTDIR) if directory => {
                    let mut st: libc::stat = unsafe { std::mem::zeroed() };
                    let rc = unsafe {
                        libc::fstatat(
                            dirfd.as_raw_fd(),
                            c_name.as_ptr(),
                            &mut st,
                            libc::AT_SYMLINK_NOFOLLOW,
                        )
                    };
                    rc == 0 && (st.st_mode & libc::S_IFMT) == libc::S_IFLNK
                }
                _ => false,
            };
            return if symlink {
                Err(OpenViolation::Symlink {
                    component: PathBuf::from(name),
                })
            } else {
                Err(OpenViolation::Io(err))
            };
        }
        // SAFETY: fd is a freshly-opened, owned descriptor.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    let root_handle: OwnedFd = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY)
        .open(root)
        .map_err(OpenViolation::Io)?
        .into();

    let mut dir = root_handle;
    let (leaf, intermediates) = components.split_last().expect("checked non-empty");
    for name in intermediates {
        dir = openat_no_follow(&dir, name, true)?;
    }
    let leaf_handle = openat_no_follow(&dir, leaf, false)?;
    Ok(File::from(leaf_handle))
}

/// Non-unix fallback: per-component symlink check via `symlink_metadata`,
/// then a plain open. Weaker than the unix handle walk (a component swapped
/// between the check and the open is not caught), which the snapshot contract
/// tolerates only where the platform offers no `openat` surface.
#[cfg(not(unix))]
fn open_confined_impl(root: &Path, components: &[&std::ffi::OsStr]) -> Result<File, OpenViolation> {
    let mut current = root.to_path_buf();
    for name in components {
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(OpenViolation::Symlink {
                    component: PathBuf::from(name),
                });
            }
            Ok(_) => {}
            Err(err) => return Err(OpenViolation::Io(err)),
        }
    }
    File::open(&current).map_err(OpenViolation::Io)
}

// ── Closed-world directory enumeration ──────────────────────────────────────────
//
// The engine-owned, no-follow, bounded walk primitive that `dsl::index`'s glob
// resolution and the extras scan build on. `DESIGN.md` § Five check families:
// the engine enumerates rather than discovering the tree — symlinks are not
// followed during enumeration, so symlink cycles cannot extend a walk. Listing
// is decided on validated no-follow handles exactly as `open_confined` decides
// a read: a symlinked intermediate directory is refused, never enumerated
// through, so an out-of-tree directory's contents are never disclosed.

/// No-follow file-type classification of a directory entry. A symbolic link is
/// reported as [`EntryType::Symlink`] whatever its target — never as the
/// target's type — so a caller enforcing closed-world discipline can refuse or
/// skip it without resolving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    Dir,
    File,
    Symlink,
    Other,
}

/// One entry returned by [`list_dir`]: its name and its no-follow file type.
#[derive(Debug, Clone)]
pub struct DirEntryInfo {
    pub name: OsString,
    pub file_type: EntryType,
}

/// A violation surfaced while listing a directory under the governed root for
/// closed-world enumeration.
#[derive(Debug)]
pub enum ListViolation {
    /// A component of the listed path was a symbolic link. Refused no-follow
    /// exactly as [`open_confined`] refuses a symlinked component — the
    /// directory is never enumerated through. MDATRON-E0012 territory.
    Symlink { component: PathBuf },
    /// The directory (or a component of its path) does not exist. Kept
    /// distinct from other IO so glob enumeration can treat a missing
    /// directory as "no matches" (closed-world: enumerate what is present)
    /// rather than a hard error, while a real IO failure is still reported.
    NotFound,
    /// Ordinary IO failure (permission, not-a-directory, …). Reported, never
    /// silently skipped (`DESIGN.md` § Agents are the first consumer: no
    /// silent degradation).
    Io(io::Error),
}

/// List the immediate entries of `root`/`rel` through validated no-follow
/// handles, returning each entry's name and no-follow file type.
///
/// Every component of `rel` is opened relative to its parent with no-follow
/// semantics before the directory is read, so a symlinked intermediate
/// directory is refused ([`ListViolation::Symlink`]) rather than followed —
/// the closed-world discipline of `DESIGN.md` § Five check families. `rel` must
/// be a [`confine_lexically`] result (relative, no parent segments); `root` is
/// engine-supplied and trusted, so symlinks in the root path itself are
/// permitted. Entries are returned in a deterministic (name-sorted) order.
pub fn list_dir(root: &Path, rel: &Path) -> Result<Vec<DirEntryInfo>, ListViolation> {
    let components: Vec<&OsStr> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();

    let mut out = list_dir_impl(root, rel, &components)?;
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Prove every component of `rel` is a no-follow-openable directory under
/// `root`, refusing a symlinked component, and return the final validated
/// directory handle. The unix path opens each component relative to its
/// parent's handle with `O_NOFOLLOW | O_DIRECTORY` and hands the leaf handle
/// back so the caller enumerates through it (no re-resolution by path); the
/// non-unix fallback stats each component and returns `()`, leaving the caller
/// to read by path (weaker: a swap between check and read is not caught — the
/// same tolerance `open_confined` documents).
#[cfg(unix)]
fn validate_dir_chain(
    root: &Path,
    components: &[&OsStr],
) -> Result<std::os::fd::OwnedFd, ListViolation> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;

    fn open_child_dir(dirfd: &OwnedFd, name: &OsStr) -> Result<OwnedFd, ListViolation> {
        let c_name = CString::new(name.as_bytes()).map_err(|_| {
            ListViolation::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path component contains NUL",
            ))
        })?;
        let flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_DIRECTORY;
        // Retry on EINTR (see open_confined_impl): fail-closed, access never granted.
        let fd = loop {
            let fd = unsafe { libc::openat(dirfd.as_raw_fd(), c_name.as_ptr(), flags) };
            if fd < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break fd;
        };
        if fd < 0 {
            let err = io::Error::last_os_error();
            // O_NOFOLLOW on a symlink: ELOOP on Linux/macOS, EMLINK on
            // FreeBSD-lineage. With O_DIRECTORY, macOS reports a
            // symlink-to-directory as ENOTDIR — disambiguate from a genuine
            // non-directory with a handle-relative no-follow stat (mirrors
            // `open_confined`).
            let symlink = match err.raw_os_error() {
                Some(libc::ELOOP) | Some(libc::EMLINK) => true,
                Some(libc::ENOTDIR) => {
                    let mut st: libc::stat = unsafe { std::mem::zeroed() };
                    let rc = unsafe {
                        libc::fstatat(
                            dirfd.as_raw_fd(),
                            c_name.as_ptr(),
                            &mut st,
                            libc::AT_SYMLINK_NOFOLLOW,
                        )
                    };
                    rc == 0 && (st.st_mode & libc::S_IFMT) == libc::S_IFLNK
                }
                _ => false,
            };
            if symlink {
                return Err(ListViolation::Symlink {
                    component: PathBuf::from(name),
                });
            }
            if err.kind() == io::ErrorKind::NotFound {
                return Err(ListViolation::NotFound);
            }
            return Err(ListViolation::Io(err));
        }
        // SAFETY: fd is a freshly-opened, owned descriptor.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    let root_handle: OwnedFd = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY)
        .open(root)
    {
        Ok(f) => f.into(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(ListViolation::NotFound),
        Err(e) => return Err(ListViolation::Io(e)),
    };

    // Thread the handle down the chain: each component is opened relative to
    // its parent's handle, and the final validated directory handle is returned
    // for handle-based enumeration. Fold (rather than a `mut` accumulator) so
    // an empty component list is lint-clean — for an empty chain the root
    // handle itself is the validated directory.
    let final_dir = components
        .iter()
        .try_fold(root_handle, |dir, name| open_child_dir(&dir, name))?;
    Ok(final_dir)
}

/// Enumerate the validated directory through its no-follow handle — never by
/// re-resolving the path — so a component swapped after validation cannot
/// redirect the read (`open_confined`'s check-then-read closure, extended to
/// enumeration). `fdopendir` adopts the handle (`closedir` closes it); per
/// POSIX the raw fd is not used directly afterwards, so `dirfd` recovers it for
/// the per-entry no-follow classification.
#[cfg(unix)]
fn list_dir_impl(
    root: &Path,
    _rel: &Path,
    components: &[&OsStr],
) -> Result<Vec<DirEntryInfo>, ListViolation> {
    use std::os::fd::{IntoRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;

    let dir_fd: OwnedFd = validate_dir_chain(root, components)?;

    let raw = dir_fd.into_raw_fd();
    let dirp = unsafe { libc::fdopendir(raw) };
    if dirp.is_null() {
        let e = io::Error::last_os_error();
        unsafe { libc::close(raw) };
        return Err(ListViolation::Io(e));
    }
    // Guard so `closedir` runs on every exit path (including the `?` returns
    // below), releasing the handle exactly once.
    struct DirGuard(*mut libc::DIR);
    impl Drop for DirGuard {
        fn drop(&mut self) {
            unsafe { libc::closedir(self.0) };
        }
    }
    let _guard = DirGuard(dirp);
    let dfd = unsafe { libc::dirfd(dirp) };

    let mut out = Vec::new();
    loop {
        // `readdir` returns NULL both at end-of-stream and on error; reset errno
        // first so a mid-enumeration error is not mistaken for the end (no
        // silent truncation of the listing).
        unsafe { *errno_location() = 0 };
        let ent = unsafe { libc::readdir(dirp) };
        if ent.is_null() {
            let e = io::Error::last_os_error();
            if e.raw_os_error().unwrap_or(0) != 0 {
                return Err(ListViolation::Io(e));
            }
            break;
        }
        let name_bytes = unsafe { std::ffi::CStr::from_ptr((*ent).d_name.as_ptr()) }.to_bytes();
        if name_bytes == b"." || name_bytes == b".." {
            continue;
        }
        let c_name = std::ffi::CString::new(name_bytes).map_err(|_| {
            ListViolation::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "directory entry name contains NUL",
            ))
        })?;
        // No-follow classification through the validated directory fd, so an
        // entry that is itself a symlink is reported as one, never resolved.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::fstatat(dfd, c_name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) };
        let file_type = if rc != 0 {
            EntryType::Other
        } else {
            match st.st_mode & libc::S_IFMT {
                libc::S_IFLNK => EntryType::Symlink,
                libc::S_IFDIR => EntryType::Dir,
                libc::S_IFREG => EntryType::File,
                _ => EntryType::Other,
            }
        };
        out.push(DirEntryInfo {
            name: OsStr::from_bytes(name_bytes).to_os_string(),
            file_type,
        });
    }
    Ok(out)
}

/// Pointer to the thread-local `errno`, per platform (`libc` exposes different
/// accessors). Used to distinguish `readdir`'s end-of-stream NULL from an error
/// NULL.
#[cfg(unix)]
fn errno_location() -> *mut libc::c_int {
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "emscripten"))]
    {
        unsafe { libc::__errno_location() }
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "emscripten")))]
    {
        unsafe { libc::__error() }
    }
}

#[cfg(not(unix))]
fn validate_dir_chain(root: &Path, components: &[&OsStr]) -> Result<(), ListViolation> {
    let mut current = root.to_path_buf();
    for name in components {
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(ListViolation::Symlink {
                    component: PathBuf::from(name),
                });
            }
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                return Err(ListViolation::Io(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "not a directory",
                )));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(ListViolation::NotFound),
            Err(e) => return Err(ListViolation::Io(e)),
        }
    }
    Ok(())
}

/// Non-unix: validate the chain by path (weaker — see `validate_dir_chain`),
/// then read the directory by path. The check-to-read window `open_confined`'s
/// fallback documents applies to enumeration here too, tolerated only where the
/// platform offers no handle-relative no-follow open.
#[cfg(not(unix))]
fn list_dir_impl(
    root: &Path,
    rel: &Path,
    components: &[&OsStr],
) -> Result<Vec<DirEntryInfo>, ListViolation> {
    validate_dir_chain(root, components)?;

    let dir_path = root.join(rel);
    let read_dir = match std::fs::read_dir(&dir_path) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(ListViolation::NotFound),
        Err(e) => return Err(ListViolation::Io(e)),
    };

    let mut out = Vec::new();
    for entry in read_dir {
        let entry = entry.map_err(ListViolation::Io)?;
        let ft = entry.file_type().map_err(ListViolation::Io)?;
        let file_type = if ft.is_symlink() {
            EntryType::Symlink
        } else if ft.is_dir() {
            EntryType::Dir
        } else if ft.is_file() {
            EntryType::File
        } else {
            EntryType::Other
        };
        out.push(DirEntryInfo {
            name: entry.file_name(),
            file_type,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── confine_lexically: pure, filesystem-independent ─────────────────────

    #[test]
    fn plain_relative_path_is_confined() {
        assert_eq!(
            confine_lexically(Path::new("a/b/c.yaml"))
                .unwrap()
                .as_path(),
            Path::new("a/b/c.yaml")
        );
    }

    #[test]
    fn cur_dir_components_are_dropped() {
        assert_eq!(
            confine_lexically(Path::new("./a/./b.yaml"))
                .unwrap()
                .as_path(),
            Path::new("a/b.yaml")
        );
    }

    #[test]
    fn leading_parent_segment_rejected() {
        assert_eq!(
            confine_lexically(Path::new("../escape.yaml")).unwrap_err(),
            LexicalViolation::ParentSegment
        );
    }

    #[test]
    fn interior_parent_segment_rejected_even_when_non_escaping() {
        // `a/../b.yaml` resolves inside the root, but parent segments are
        // rejected outright (BOUNDARY-PREAMBLE § 7 carried forward).
        assert_eq!(
            confine_lexically(Path::new("a/../b.yaml")).unwrap_err(),
            LexicalViolation::ParentSegment
        );
    }

    #[test]
    fn deep_traversal_rejected_without_filesystem() {
        // The predecessor's fallback compared `root/../..` component-wise
        // with starts_with and passed it; the lexical check cannot.
        assert_eq!(
            confine_lexically(Path::new("../../etc/passwd")).unwrap_err(),
            LexicalViolation::ParentSegment
        );
    }

    #[test]
    fn absolute_path_rejected() {
        assert_eq!(
            confine_lexically(Path::new("/etc/hosts")).unwrap_err(),
            LexicalViolation::Absolute
        );
    }

    #[test]
    fn empty_path_confines_to_empty() {
        assert_eq!(
            confine_lexically(Path::new("")).unwrap().as_path(),
            Path::new("")
        );
    }

    // ── open_confined ───────────────────────────────────────────────────────

    fn temp_root(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("mdatron-confine-{label}-{nanos}"));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// The only way to build the `open_confined` argument: through the lexical
    /// check. A source that fails confinement panics here, mirroring the fact
    /// that it can never reach `open_confined` in production code.
    fn confined(source: &str) -> ConfinedPath {
        confine_lexically(Path::new(source)).expect("test source must be confinable")
    }

    #[test]
    fn opens_nested_file_through_handles() {
        use std::io::Read;
        let root = temp_root("nested");
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/b/data.yaml"), "k: v\n").unwrap();

        let mut file = open_confined(&root, &confined("a/b/data.yaml")).unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        assert_eq!(content, "k: v\n");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_target_is_io_not_symlink() {
        let root = temp_root("missing");
        let err = open_confined(&root, &confined("absent.yaml")).unwrap_err();
        assert!(matches!(err, OpenViolation::Io(_)));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn empty_source_is_io() {
        let root = temp_root("empty");
        let err = open_confined(&root, &confined("")).unwrap_err();
        assert!(matches!(err, OpenViolation::Io(_)));
        std::fs::remove_dir_all(&root).unwrap();
    }

    // RED GATE (#53): a path carrying a `..` component must never reach an open
    // as `root/secret.yaml`. Pre-fix, `open_confined(root, Path::new("../secret.yaml"))`
    // filtered the ParentDir away and opened `root/secret.yaml`; the falsifying
    // call now fails to type-check (see the `compile_fail` doctest on
    // `open_confined`). This runtime test guards the seam that remains: the only
    // way to reach `open_confined` is via `confine_lexically`, which refuses the
    // traversal outright, so the target file is never opened.
    #[test]
    fn red_gate_parent_traversal_cannot_reach_open() {
        let root = temp_root("red-gate-parent");
        std::fs::write(root.join("secret.yaml"), "k: v\n").unwrap();

        // The would-be argument to open_confined cannot even be constructed.
        assert_eq!(
            confine_lexically(Path::new("../secret.yaml")).unwrap_err(),
            LexicalViolation::ParentSegment,
        );
        // And the same file, reached through a properly confined path, still opens
        // — the fix rejects traversal without breaking legitimate access.
        assert!(open_confined(&root, &confined("secret.yaml")).is_ok());

        std::fs::remove_dir_all(&root).unwrap();
    }

    // SE-F7: intermediate-handle threading for a path deeper than the two-level
    // case the other tests cover.
    #[test]
    fn opens_deeply_nested_file() {
        use std::io::Read;
        let root = temp_root("deep");
        std::fs::create_dir_all(root.join("a/b/c/d")).unwrap();
        std::fs::write(root.join("a/b/c/d/deep.yaml"), "k: deep\n").unwrap();

        let mut file = open_confined(&root, &confined("a/b/c/d/deep.yaml")).unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        assert_eq!(content, "k: deep\n");
        std::fs::remove_dir_all(&root).unwrap();
    }

    // SE-F7: a directory as the leaf opens (unix permits O_RDONLY on a
    // directory); the not-a-file nature surfaces as EISDIR when the caller
    // reads — the parse stage's error, not a confinement violation.
    #[cfg(unix)]
    #[test]
    fn directory_as_leaf_opens_but_read_is_eisdir() {
        use std::io::Read;
        let root = temp_root("dir-leaf");
        std::fs::create_dir_all(root.join("adir")).unwrap();

        let mut file = open_confined(&root, &confined("adir")).unwrap();
        let mut buf = Vec::new();
        let err = file.read_to_end(&mut buf).unwrap_err();
        assert_eq!(err.raw_os_error(), Some(libc::EISDIR));
        std::fs::remove_dir_all(&root).unwrap();
    }

    // SEC-F6 / invariant I7: the handle walks (open_confined) and the
    // handle-based enumeration (list_dir, via fdopendir/closedir) must release
    // every descriptor — success, symlink-refusal, and not-found paths alike.
    #[cfg(unix)]
    fn open_fd_count() -> usize {
        let dir = if Path::new("/proc/self/fd").exists() {
            "/proc/self/fd"
        } else {
            "/dev/fd"
        };
        std::fs::read_dir(dir).map(|rd| rd.count()).unwrap_or(0)
    }

    #[cfg(unix)]
    #[test]
    fn confine_ops_do_not_leak_fds() {
        use std::io::Read;
        let root = temp_root("fd-leak");
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/b/x.yaml"), "k: v\n").unwrap();
        std::os::unix::fs::symlink("x.yaml", root.join("a/b/link.yaml")).unwrap();

        let exercise = |root: &Path| {
            if let Ok(mut f) = open_confined(root, &confined("a/b/x.yaml")) {
                let mut s = String::new();
                let _ = f.read_to_string(&mut s);
            }
            let _ = open_confined(root, &confined("a/b/link.yaml")); // refused mid-walk
            let _ = open_confined(root, &confined("a/b/absent.yaml")); // NotFound
            let _ = list_dir(root, Path::new("a/b")); // handle-based enumeration
            let _ = list_dir(root, Path::new("no-such")); // NotFound
        };

        // Warm up (first-call allocations) before measuring.
        for _ in 0..20 {
            exercise(&root);
        }
        let before = open_fd_count();
        for _ in 0..300 {
            exercise(&root);
        }
        let after = open_fd_count();
        assert!(
            after <= before + 2,
            "descriptor leak across 300 confine ops: {before} -> {after} (invariant I7)"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlink_leaf_is_refused_even_when_target_is_inside_root() {
        let root = temp_root("leaf-link");
        std::fs::write(root.join("real.yaml"), "k: v\n").unwrap();
        std::os::unix::fs::symlink(root.join("real.yaml"), root.join("alias.yaml")).unwrap();

        let err = open_confined(&root, &confined("alias.yaml")).unwrap_err();
        match err {
            OpenViolation::Symlink { component } => {
                assert_eq!(component, PathBuf::from("alias.yaml"));
            }
            other => panic!("expected Symlink, got {other:?}"),
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_intermediate_component_is_refused() {
        let root = temp_root("mid-link");
        let outside = temp_root("mid-link-outside");
        std::fs::write(outside.join("data.yaml"), "k: v\n").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("sub")).unwrap();

        let err = open_confined(&root, &confined("sub/data.yaml")).unwrap_err();
        match err {
            OpenViolation::Symlink { component } => {
                assert_eq!(component, PathBuf::from("sub"));
            }
            other => panic!("expected Symlink, got {other:?}"),
        }
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    // ── list_dir: no-follow closed-world enumeration ────────────────────────

    fn names(entries: &[DirEntryInfo]) -> Vec<String> {
        entries
            .iter()
            .map(|e| e.name.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn list_dir_returns_sorted_entries_with_types() {
        let root = temp_root("list-basic");
        std::fs::write(root.join("b.yaml"), "k: v\n").unwrap();
        std::fs::write(root.join("a.yaml"), "k: v\n").unwrap();
        std::fs::create_dir(root.join("d")).unwrap();

        let entries = list_dir(&root, Path::new("")).unwrap();
        assert_eq!(names(&entries), vec!["a.yaml", "b.yaml", "d"]);
        assert_eq!(entries[0].file_type, EntryType::File);
        assert_eq!(entries[2].file_type, EntryType::Dir);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn list_dir_missing_directory_is_not_found() {
        let root = temp_root("list-missing");
        let err = list_dir(&root, Path::new("no-such-dir")).unwrap_err();
        assert!(matches!(err, ListViolation::NotFound));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn list_dir_through_symlinked_component_is_refused_without_disclosure() {
        // The closed-world guarantee: a symlinked intermediate directory is
        // refused before any entry of its target is read, so an out-of-tree
        // directory's filenames are never enumerated.
        let root = temp_root("list-symlink");
        let outside = temp_root("list-symlink-outside");
        std::fs::write(outside.join("secret.yaml"), "k: v\n").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("sub")).unwrap();

        let err = list_dir(&root, Path::new("sub")).unwrap_err();
        match err {
            ListViolation::Symlink { component } => assert_eq!(component, PathBuf::from("sub")),
            other => panic!("expected Symlink refusal, got {other:?}"),
        }
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn list_dir_reports_symlink_entry_as_symlink_not_its_target() {
        let root = temp_root("list-symlink-entry");
        std::fs::write(root.join("real.yaml"), "k: v\n").unwrap();
        std::os::unix::fs::symlink(root.join("real.yaml"), root.join("alias.yaml")).unwrap();

        let entries = list_dir(&root, Path::new("")).unwrap();
        let alias = entries.iter().find(|e| e.name == "alias.yaml").unwrap();
        assert_eq!(alias.file_type, EntryType::Symlink);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
