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
        let fd = unsafe { libc::openat(dirfd.as_raw_fd(), c_name.as_ptr(), flags) };
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
}
