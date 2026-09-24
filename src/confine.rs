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
//!    directory's handle without following links — `openat`/`O_NOFOLLOW` on
//!    Unix, `NtCreateFile` with a `RootDirectory` handle and
//!    `FILE_OPEN_REPARSE_POINT` on Windows (#64) — so a symlinked intermediate
//!    component is refused exactly like a symlinked final one, and the handle
//!    returned is the handle the caller reads — confinement is decided on the
//!    handle (`DESIGN.md` § Verification is fast where it is invoked), not on
//!    a path that could be swapped between check and read. The swap-proof
//!    guarantee is universal across the declared targets; the std fallback
//!    below is compiled only for a target nothing declares.

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
    /// `DESIGN.md` § Nine check families). MDATRON-E0011 territory.
    ParentSegment,
}

/// A violation surfaced while opening a lexically-confined path.
#[derive(Debug)]
pub enum OpenViolation {
    /// A component was a symbolic link — or, on Windows, any reparse point.
    /// No-follow resolution refuses it whatever its target — inside or
    /// outside the governed tree. `tag` is the Windows reparse tag so the
    /// finding can name the class ([`describe_reparse`]); `None` on Unix, or
    /// when the tag could not be read. MDATRON-E0012 territory.
    Symlink {
        component: PathBuf,
        tag: Option<u32>,
    },
    /// The leaf exists and opened, but is not a regular file — a FIFO, device,
    /// or directory. Refused before any read: a FIFO with no writer would
    /// otherwise park the process in a blocking `open`/`read` forever (a
    /// one-file denial of verification), and none of these carry content the
    /// engine can verify. The path EXISTS — consumers that only test existence
    /// treat this as present-but-unverifiable. (A unix SOCKET never reaches
    /// this variant: `open(2)` refuses sockets with ENXIO/EOPNOTSUPP before
    /// the fstat, so it surfaces as `Io` and reports as absent.)
    NotRegular,
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
/// use mdatron::confine::open_confined;
/// use std::path::Path;
/// // open_confined requires a &ConfinedPath; a raw &Path is rejected by the
/// // type checker, so an unconfined "../secret.yaml" can never be opened.
/// let _ = open_confined(Path::new("/tmp"), Path::new("../secret.yaml"));
/// ```
///
/// `root` is engine-supplied and trusted: on Unix, symlinks in the root path
/// itself (e.g. macOS `/var` → `/private/var`) are followed; on Windows the
/// root is opened without following reparse points and a root that IS one (a
/// junction-rooted project) is refused — canonicalize the root first, as the
/// CLI does for every subcommand (#64 W2). Every component of `rel` below it
/// is opened relative to its parent directory's handle with no-follow
/// semantics, so a symlink at any depth is refused.
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
        } else {
            // O_NONBLOCK so opening an adopter-named FIFO cannot park the
            // process in a blocking open (a one-file denial of verification).
            // A no-op for the regular files the leaf fstat then requires, so
            // the flag never changes read semantics on accepted handles.
            flags |= libc::O_NONBLOCK;
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
                    tag: None,
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
    // The handle that passed confinement is the handle that is stat'd: refuse
    // anything that is not a regular file (FIFO, socket, device, directory)
    // BEFORE any read — a FIFO read would block until a writer appears, and
    // none of these carry verifiable content.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::fstat(leaf_handle.as_raw_fd(), &mut st) };
    if rc != 0 {
        return Err(OpenViolation::Io(io::Error::last_os_error()));
    }
    if (st.st_mode & libc::S_IFMT) != libc::S_IFREG {
        return Err(OpenViolation::NotRegular);
    }
    Ok(File::from(leaf_handle))
}

/// Windows (#64): the handle-relative no-follow walk in [`win`] — every
/// component opened relative to its parent's handle by `NtCreateFile` with
/// `FILE_OPEN_REPARSE_POINT`, classified through the handle, any reparse
/// point refused regardless of tag.
#[cfg(windows)]
fn open_confined_impl(root: &Path, components: &[&std::ffi::OsStr]) -> Result<File, OpenViolation> {
    win::open_confined_impl(root, components)
}

/// Carve-out for a target nothing declares (neither Unix nor Windows): a
/// per-component symlink check via `symlink_metadata`, then a plain open.
/// Weaker than the handle walks (a component swapped between the check and
/// the open is not caught) — kept compiling so the crate builds on such a
/// target, but no declared target carries it; the swap-proof guarantee holds
/// on every target mdatron supports.
#[cfg(not(any(unix, windows)))]
fn open_confined_impl(root: &Path, components: &[&std::ffi::OsStr]) -> Result<File, OpenViolation> {
    let mut current = root.to_path_buf();
    for name in components {
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(OpenViolation::Symlink {
                    component: PathBuf::from(name),
                    tag: None,
                });
            }
            Ok(_) => {}
            Err(err) => return Err(OpenViolation::Io(err)),
        }
    }
    let handle = File::open(&current).map_err(OpenViolation::Io)?;
    // Mirror the unix leaf fstat: only regular files carry verifiable content.
    match handle.metadata() {
        Ok(meta) if meta.is_file() => Ok(handle),
        Ok(_) => Err(OpenViolation::NotRegular),
        Err(err) => Err(OpenViolation::Io(err)),
    }
}

// ── Closed-world directory enumeration ──────────────────────────────────────────
//
// The engine-owned, no-follow, bounded walk primitive that `dsl::index`'s glob
// resolution and the extras scan build on. `DESIGN.md` § Nine check families:
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
    /// A component of the listed path was a symbolic link — or, on Windows,
    /// any reparse point. Refused no-follow exactly as [`open_confined`]
    /// refuses a symlinked component — the directory is never enumerated
    /// through. `tag` as on [`OpenViolation::Symlink`]. MDATRON-E0012
    /// territory.
    Symlink {
        component: PathBuf,
        tag: Option<u32>,
    },
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

// ── Reparse-point classes (#64 cold-review W1) ─────────────────────────────────
//
// Every reparse point refuses — the Windows walk decides on the attribute,
// never on the tag — but the adopter deserves to know WHAT refused: a symlink
// is fixed by replacing it, a OneDrive Files-On-Demand placeholder by
// hydrating it, a WOF-compressed file by decompressing it. The tag rides the
// violation from the walk (`None` on Unix, where the symlink is the only kind,
// and `None` when the tag could not be read) into the E0012 message and help.

/// Microsoft-assigned reparse tags (ntifs.h). Stable ABI values, spelled here
/// so the classification needs no extra `windows-sys` namespace and renders
/// on every platform.
const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;
const IO_REPARSE_TAG_DEDUP: u32 = 0x8000_0013;
const IO_REPARSE_TAG_WOF: u32 = 0x8000_0017;
const IO_REPARSE_TAG_CLOUD: u32 = 0x9000_001A;
const IO_REPARSE_TAG_PROJFS: u32 = 0x9000_001C;
/// The name-surrogate bit: the reparse point stands for another name (a
/// symlink, a junction, a volume mount point).
const REPARSE_TAG_NAME_SURROGATE: u32 = 0x2000_0000;
/// The cloud-file family: `IO_REPARSE_TAG_CLOUD` plus `CLOUD_1..CLOUD_F`,
/// which differ only in bits 12–15.
const CLOUD_FAMILY_MASK: u32 = 0xFFFF_0FFF;

/// What kind of reparse point a refused component was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReparseClass {
    /// A symbolic link, junction, or volume mount point — a name for another
    /// place. Unix refusals (no tag) are always this class.
    NameSurrogate,
    /// A OneDrive / cloud-provider Files-On-Demand placeholder.
    CloudPlaceholder,
    /// A WOF-compressed file (`compact`, CompactOS).
    WofCompressed,
    /// A Data Deduplication stub.
    Deduplicated,
    /// A projected-filesystem placeholder (ProjFS; VFS for Git).
    ProjectedFile,
    /// A reparse tag mdatron does not name; refused all the same.
    Unknown,
}

/// Classify a refused reparse point by its tag (`None` = a Unix symlink, or a
/// Windows reparse point whose tag could not be read — treated as a link).
pub fn classify_reparse(tag: Option<u32>) -> ReparseClass {
    match tag {
        None => ReparseClass::NameSurrogate,
        Some(t) if t & CLOUD_FAMILY_MASK == IO_REPARSE_TAG_CLOUD => ReparseClass::CloudPlaceholder,
        Some(IO_REPARSE_TAG_WOF) => ReparseClass::WofCompressed,
        Some(IO_REPARSE_TAG_DEDUP) => ReparseClass::Deduplicated,
        Some(IO_REPARSE_TAG_PROJFS) => ReparseClass::ProjectedFile,
        Some(t) if t & REPARSE_TAG_NAME_SURROGATE != 0 => ReparseClass::NameSurrogate,
        Some(_) => ReparseClass::Unknown,
    }
}

/// The adopter-facing description of a refused reparse point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReparseDescription {
    /// A noun phrase for "resolves through …": `a symbolic link`, `a
    /// cloud-file placeholder (reparse tag 0x9000001a)`, …
    pub what: String,
    /// A class-specific remedy, or `None` for the name-surrogate class, where
    /// each finding site keeps its own "replace the symlink" wording.
    pub help: Option<String>,
}

/// Describe a refused reparse point for an E0012 message and help. The
/// refusal itself never varies by class — mdatron follows no reparse point —
/// only the remedy does.
pub fn describe_reparse(tag: Option<u32>) -> ReparseDescription {
    let never = "mdatron never follows reparse points";
    match (classify_reparse(tag), tag) {
        // No tag: on Unix the refusal is always a symbolic link (the only
        // no-follow class there); on Windows a missing tag means the kernel
        // refused traversal before a handle existed (OBJ_DONT_REPARSE) or the
        // tag query failed — say so rather than mislabel it (#64 review N3).
        (_, None) => ReparseDescription {
            what: if cfg!(windows) {
                "a reparse point the kernel refused to traverse (tag unavailable)".into()
            } else {
                "a symbolic link".into()
            },
            help: None,
        },
        (ReparseClass::NameSurrogate, Some(IO_REPARSE_TAG_SYMLINK)) => ReparseDescription {
            what: "a symbolic link".into(),
            help: None,
        },
        (ReparseClass::NameSurrogate, Some(IO_REPARSE_TAG_MOUNT_POINT)) => ReparseDescription {
            what: "a junction or volume mount point (reparse tag 0xa0000003)".into(),
            help: None,
        },
        (ReparseClass::NameSurrogate, Some(t)) => ReparseDescription {
            what: format!("a name-surrogate reparse point (tag 0x{t:08x})"),
            help: None,
        },
        (ReparseClass::CloudPlaceholder, Some(t)) => ReparseDescription {
            what: format!("a cloud-file placeholder (reparse tag 0x{t:08x})"),
            help: Some(format!(
                "hydrate the file (OneDrive: 'Always keep on this device') or move the \
                 project off the synced folder — {never}"
            )),
        },
        (ReparseClass::WofCompressed, Some(t)) => ReparseDescription {
            what: format!("a WOF-compressed file (reparse tag 0x{t:08x}; compact / CompactOS)"),
            help: Some(format!(
                "decompress it (`compact /u <file>`) or copy it out of the compressed \
                 folder — {never}"
            )),
        },
        (ReparseClass::Deduplicated, Some(t)) => ReparseDescription {
            what: format!("a data-deduplicated file (reparse tag 0x{t:08x})"),
            help: Some(format!(
                "exclude the project from Data Deduplication or copy the file to a \
                 non-deduplicated volume — {never}"
            )),
        },
        (ReparseClass::ProjectedFile, Some(t)) => ReparseDescription {
            what: format!(
                "a projected-filesystem placeholder (reparse tag 0x{t:08x}; ProjFS / VFS for Git)"
            ),
            help: Some(format!(
                "hydrate the file through its provider or copy it out of the \
                 virtualized tree — {never}"
            )),
        },
        // N2: the "never follows" sentence lives in `help`, not spliced into
        // the message's "resolves through {what}" clause.
        (_, Some(t)) => ReparseDescription {
            what: format!("a reparse point (tag 0x{t:08x})"),
            help: Some(format!(
                "replace it with a plain file or directory inside the governed tree — {never}"
            )),
        },
    }
}

/// List the immediate entries of `root`/`rel` through validated no-follow
/// handles, returning each entry's name and no-follow file type.
///
/// Every component of `rel` is opened relative to its parent with no-follow
/// semantics before the directory is read, so a symlinked intermediate
/// directory is refused ([`ListViolation::Symlink`]) rather than followed —
/// the closed-world discipline of `DESIGN.md` § Nine check families. `rel` must
/// be a [`confine_lexically`] result (relative, no parent segments); `root` is
/// engine-supplied and trusted — on Unix symlinks in the root path itself are
/// followed, on Windows a reparse point AS the root is refused (canonicalize
/// first; the CLI does, #64 W2). Entries are returned in a deterministic
/// (name-sorted) order.
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
/// Windows walk ([`win`]) does the same through `NtCreateFile` +
/// `FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT`. Only the fallback for an
/// undeclared target stats each component and returns `()`, leaving the
/// caller to read by path (weaker: a swap between check and read is not
/// caught — the same carve-out `open_confined_impl` documents).
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
                    tag: None,
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

#[cfg(not(any(unix, windows)))]
fn validate_dir_chain(root: &Path, components: &[&OsStr]) -> Result<(), ListViolation> {
    let mut current = root.to_path_buf();
    for name in components {
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(ListViolation::Symlink {
                    component: PathBuf::from(name),
                    tag: None,
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

/// Windows (#64): validate the chain through handles and enumerate THROUGH the
/// final directory handle (`GetFileInformationByHandleEx`,
/// `FileIdBothDirectoryInfo`) — never a path-based `read_dir` on a
/// re-resolved path — reporting each entry's reparse flag as a `Symlink` entry.
#[cfg(windows)]
fn list_dir_impl(
    root: &Path,
    rel: &Path,
    components: &[&OsStr],
) -> Result<Vec<DirEntryInfo>, ListViolation> {
    win::list_dir_impl(root, rel, components)
}

/// Undeclared-target carve-out: validate the chain by path (weaker — see
/// `validate_dir_chain`), then read the directory by path. The check-to-read
/// window `open_confined_impl`'s fallback documents applies to enumeration
/// here too; no declared target compiles it.
#[cfg(not(any(unix, windows)))]
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

// ── Windows: the handle-relative no-follow walk (#64) ────────────────────────
//
// Windows has no `openat`. The one handle-relative open the platform offers is
// `NtCreateFile` with `OBJECT_ATTRIBUTES.RootDirectory` set to the parent
// directory's handle and `ObjectName` a SINGLE component — which the standard
// library does not expose, hence the `windows-sys` binding
// (docs/dependencies/windows-sys.md). Every component opens relative to the
// handle that passed confinement, with `FILE_OPEN_REPARSE_POINT` so a reparse
// point opens AS ITSELF and `OBJ_DONT_REPARSE` so the kernel refuses outright
// (`STATUS_REPARSE_POINT_ENCOUNTERED`) should any filter still try to
// traverse one — the same pairing std's `remove_dir_all` uses; the object is
// then classified through the handle it returned (`GetFileInformationByHandle`,
// and `FileAttributeTagInfo` for the tag once the reparse bit is set), and
// `FILE_ATTRIBUTE_REPARSE_POINT` refuses REGARDLESS of reparse tag: symlinks,
// junctions, volume mount points, cloud placeholders, compressed and
// deduplicated stubs are all reparse points and all refuse alike — there is
// no allowlist of "safe" kinds; the tag rides along only so the finding can
// name the class. The walk is iterated blind (no local Windows;
// `windows-latest` CI is the only executor), so it is FAIL-CLOSED by
// construction: any NTSTATUS other than `STATUS_SUCCESS` denies, any
// unexpected object type denies, any ambiguity denies. An untested bug can
// over-refuse (a loud E0012), never grant. No error carries a path: the caller
// names the component.
#[cfg(windows)]
mod win {
    use super::{DirEntryInfo, EntryType, ListViolation, OpenViolation};
    use std::ffi::{c_void, OsStr, OsString};
    use std::fs::File;
    use std::io;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::path::{Path, PathBuf};
    use std::ptr;

    use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
    use windows_sys::Wdk::Storage::FileSystem::{
        NtCreateFile, FILE_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT,
        FILE_SYNCHRONOUS_IO_NONALERT,
    };
    use windows_sys::Win32::Foundation::{
        RtlNtStatusToDosError, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE, NTSTATUS,
        OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE, STATUS_NOT_A_DIRECTORY,
        STATUS_REPARSE_POINT_ENCOUNTERED, STATUS_SUCCESS, UNICODE_STRING,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FileAttributeTagInfo, FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo,
        GetFileInformationByHandle, GetFileInformationByHandleEx, BY_HANDLE_FILE_INFORMATION,
        FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_ID_BOTH_DIR_INFO, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_READ_DATA,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, SYNCHRONIZE,
    };
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

    /// Access for a directory the walk descends through or enumerates: list +
    /// SYNCHRONIZE (what std's `remove_dir_all` opens its `RootDirectory`
    /// parents with) + attributes for the through-the-handle classification.
    /// No `FILE_TRAVERSE`: that is a right the kernel checks on the directory
    /// being traversed against the caller's token (and every ordinary token
    /// bypasses it via SeChangeNotifyPrivilege), not something the parent
    /// handle carries.
    const DIR_ACCESS: u32 = FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE;
    /// Access for the leaf the caller reads.
    const LEAF_ACCESS: u32 = FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE;
    /// Access for a classification-only re-open (see [`open_child_dir`]).
    const PROBE_ACCESS: u32 = FILE_READ_ATTRIBUTES | SYNCHRONIZE;
    /// std's own read-open share mode: never block another reader/writer.
    const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

    /// Why a component refused, before it is mapped onto the caller's
    /// violation type ([`OpenViolation`] / [`ListViolation`]).
    enum Refusal {
        /// The component is a reparse point of ANY tag (symlink, junction,
        /// mount point, cloud placeholder, …): opened as itself — or refused
        /// by the kernel outright — never followed. `tag` names the class for
        /// the finding; `None` when the kernel refused before a handle
        /// existed or the tag query failed (the refusal stands either way).
        Reparse { tag: Option<u32> },
        /// The open or the through-the-handle query failed; the error is the
        /// Win32 mapping of the NTSTATUS — never a path.
        Io(io::Error),
    }

    impl Refusal {
        fn into_open(self, component: &OsStr) -> OpenViolation {
            match self {
                Refusal::Reparse { tag } => OpenViolation::Symlink {
                    component: PathBuf::from(component),
                    tag,
                },
                Refusal::Io(e) => OpenViolation::Io(e),
            }
        }

        fn into_list(self, component: &OsStr) -> ListViolation {
            match self {
                Refusal::Reparse { tag } => ListViolation::Symlink {
                    component: PathBuf::from(component),
                    tag,
                },
                Refusal::Io(e) if e.kind() == io::ErrorKind::NotFound => ListViolation::NotFound,
                Refusal::Io(e) => ListViolation::Io(e),
            }
        }
    }

    /// How an `NtCreateFile` attempt failed.
    enum NtFailure {
        /// The kernel refused with this status (anything but `STATUS_SUCCESS`).
        Status(NTSTATUS),
        /// The component never reached the kernel (or the kernel's answer was
        /// malformed) — refused before/without a syscall.
        Other(io::Error),
    }

    impl NtFailure {
        fn into_io(self) -> io::Error {
            match self {
                NtFailure::Status(status) => status_error(status),
                NtFailure::Other(e) => e,
            }
        }
    }

    /// What a handle turned out to be, decided through the handle itself.
    struct Object {
        attributes: u32,
        /// The reparse tag, queried only when the reparse bit is set; `None`
        /// otherwise, or when the tag query failed (the bit alone refuses).
        tag: Option<u32>,
    }

    impl Object {
        fn is_reparse(&self) -> bool {
            self.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        }

        fn is_directory(&self) -> bool {
            self.attributes & FILE_ATTRIBUTE_DIRECTORY != 0
        }

        fn is_device(&self) -> bool {
            self.attributes & FILE_ATTRIBUTE_DEVICE != 0
        }
    }

    fn invalid_input(msg: &'static str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidInput, msg)
    }

    /// Map an NTSTATUS onto the `io::Error` std would have produced for the
    /// equivalent Win32 failure (`RtlNtStatusToDosError`), so the engine's
    /// kind checks (`NotFound` = absent target) hold unchanged.
    fn status_error(status: NTSTATUS) -> io::Error {
        // SAFETY: a pure table lookup over an integer — no pointers, no state.
        let code = unsafe { RtlNtStatusToDosError(status) };
        io::Error::from_raw_os_error(code as i32)
    }

    fn raw(handle: &OwnedHandle) -> HANDLE {
        handle.as_raw_handle()
    }

    /// A single path component as the NUL-free UTF-16 buffer an
    /// `OBJECT_ATTRIBUTES.ObjectName` names. RELATIVE by construction: a
    /// separator, a NUL, a `:` (an NT relative open reads `name:stream` as an
    /// alternate data stream; no legitimate NTFS name contains one), an empty
    /// name, or `.`/`..` refuses here. The [`super::ConfinedPath`] invariant
    /// already excludes most of these; this is the belt-and-braces the FFI
    /// boundary keeps for itself — one component can never name more than one
    /// object, and never a stream of one.
    fn component_utf16(name: &OsStr) -> io::Result<Vec<u16>> {
        if name.is_empty() || name == "." || name == ".." {
            return Err(invalid_input("path component is empty or a dot segment"));
        }
        let wide: Vec<u16> = name.encode_wide().collect();
        if wide.iter().any(|&u| {
            u == 0 || u == u16::from(b'\\') || u == u16::from(b'/') || u == u16::from(b':')
        }) {
            return Err(invalid_input(
                "path component contains a separator, a stream delimiter, or NUL",
            ));
        }
        // UNICODE_STRING.Length is a BYTE count in a u16.
        if wide.len() * 2 > usize::from(u16::MAX) {
            return Err(invalid_input("path component is too long"));
        }
        Ok(wide)
    }

    /// Open the single component `name` relative to `parent` — a reparse point
    /// opens AS ITSELF (`FILE_OPEN_REPARSE_POINT`) and is never traversed
    /// (`OBJ_DONT_REPARSE`) — and return the handle. Any NTSTATUS other than
    /// `STATUS_SUCCESS` is a refusal; a handle the kernel hands back alongside
    /// a non-success status is closed, never used.
    fn nt_open_relative(
        parent: &OwnedHandle,
        name: &OsStr,
        access: u32,
        options: u32,
    ) -> Result<OwnedHandle, NtFailure> {
        let mut wide = component_utf16(name).map_err(NtFailure::Other)?;
        // Fits: component_utf16 bounds the byte length to u16::MAX.
        let bytes = (wide.len() * 2) as u16;
        let object_name = UNICODE_STRING {
            Length: bytes,
            MaximumLength: bytes,
            Buffer: wide.as_mut_ptr(),
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: raw(parent),
            ObjectName: &object_name,
            Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
            SecurityDescriptor: ptr::null(),
            SecurityQualityOfService: ptr::null(),
        };
        let mut handle: HANDLE = ptr::null_mut();
        let mut iosb = IO_STATUS_BLOCK::default();
        // SAFETY:
        // - `parent` is a live, owned directory handle, borrowed for the whole
        //   call, so `RootDirectory` is valid throughout;
        // - `object_name.Buffer` points into `wide`, a NUL-free UTF-16 buffer
        //   that outlives the call (it is dropped after `attributes`), and
        //   Length/MaximumLength are its exact byte size;
        // - `attributes` (with its exact `Length`) and `object_name` are live
        //   locals for the duration of the call, and the kernel only reads
        //   them; the security pointers are null, which the API permits;
        // - `handle` and `iosb` are live, writable locals the kernel fills;
        // - FILE_OPEN never creates; AllocationSize/EaBuffer are null with
        //   zero lengths as the API requires for an open.
        let status = unsafe {
            NtCreateFile(
                &mut handle,
                access,
                &attributes,
                &mut iosb,
                ptr::null(),
                0,
                SHARE_ALL,
                FILE_OPEN,
                options | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
                ptr::null(),
                0,
            )
        };
        if status != STATUS_SUCCESS {
            if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
                // An informational/warning status can still hand back a
                // handle: it is never used — closed here, and the open refused.
                // SAFETY: the kernel wrote a handle we own exclusively; the
                // OwnedHandle closes it exactly once on drop.
                drop(unsafe { OwnedHandle::from_raw_handle(handle) });
            }
            return Err(NtFailure::Status(status));
        }
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(NtFailure::Other(io::Error::other(
                "NtCreateFile reported success without a handle",
            )));
        }
        // SAFETY: STATUS_SUCCESS — `handle` is a fresh, valid handle this
        // OwnedHandle now owns exclusively.
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }

    /// The attributes of the object BEHIND `handle`, queried through the
    /// handle itself — never by path — via the basic `GetFileInformationByHandle`
    /// every filesystem answers (the attribute-tag class below is NTFS-shaped:
    /// a FAT32/exFAT volume or an exotic redirector may not serve it, and must
    /// not refuse everything, root included, for that).
    fn attributes_of(handle: &OwnedHandle) -> io::Result<u32> {
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: `handle` is live for the call; `info` is a live, correctly
        // sized BY_HANDLE_FILE_INFORMATION the kernel fills in place.
        let ok = unsafe { GetFileInformationByHandle(raw(handle), &mut info) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(info.dwFileAttributes)
    }

    /// The reparse tag of the object behind `handle` — asked only once the
    /// reparse bit is known to be set. A failure yields `None`: the bit alone
    /// already refuses; the tag only names the class.
    fn reparse_tag_of(handle: &OwnedHandle) -> Option<u32> {
        let mut info = FILE_ATTRIBUTE_TAG_INFO {
            FileAttributes: 0,
            ReparseTag: 0,
        };
        // SAFETY: `handle` is live for the call; `info` is a live
        // FILE_ATTRIBUTE_TAG_INFO whose exact size is passed, and
        // FileAttributeTagInfo is the class that struct is defined for, so the
        // kernel writes exactly that many bytes into it.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                raw(handle),
                FileAttributeTagInfo,
                ptr::from_mut(&mut info).cast::<c_void>(),
                std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
            )
        };
        (ok != 0).then_some(info.ReparseTag)
    }

    /// Classify the object behind `handle`: attributes always, the reparse
    /// tag only when the reparse bit is set.
    fn inspect(handle: &OwnedHandle) -> io::Result<Object> {
        let attributes = attributes_of(handle)?;
        let tag = if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            reparse_tag_of(handle)
        } else {
            None
        };
        Ok(Object { attributes, tag })
    }

    /// Open the trusted, engine-supplied root as a directory handle — the ONE
    /// path-based open of the walk (the root is an absolute path the operator
    /// supplied; the danger is the components under it). Reparse points are
    /// not followed even here (`FILE_FLAG_OPEN_REPARSE_POINT`), and the handle
    /// is classified: a root that is itself a reparse point, or not a
    /// directory, refuses. The CLI canonicalizes every subcommand's root
    /// (`GetFinalPathNameByHandle`-resolved) before any walk, so a
    /// junction-rooted project has already been resolved to its real directory
    /// before it gets here (#64 cold-review W2).
    fn open_root(root: &Path) -> io::Result<OwnedHandle> {
        let file = std::fs::OpenOptions::new()
            .access_mode(DIR_ACCESS)
            .share_mode(SHARE_ALL)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(root)?;
        let handle = OwnedHandle::from(file);
        let object = inspect(&handle)?;
        if object.is_reparse() {
            return Err(invalid_input(
                "governed root is a reparse point (canonicalize the root first)",
            ));
        }
        if !object.is_directory() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                "governed root is not a directory",
            ));
        }
        Ok(handle)
    }

    /// Descend one component: open it relative to `parent` as a DIRECTORY
    /// (`FILE_DIRECTORY_FILE`, kernel-enforced), never following a reparse
    /// point, then classify the handle — a reparse point of any tag refuses.
    /// `STATUS_REPARSE_POINT_ENCOUNTERED` (the kernel honoured
    /// `OBJ_DONT_REPARSE` before a handle existed) refuses with no tag.
    /// `STATUS_NOT_A_DIRECTORY` is disambiguated by a classification-only
    /// re-open (no directory constraint, attributes access only) so a FILE
    /// symlink squatting an intermediate reports as the reparse point it is
    /// rather than a bare not-a-directory — the unix walk's `fstatat`
    /// disambiguation, handle-relative. Every branch refuses: the re-open can
    /// never grant.
    fn open_child_dir(parent: &OwnedHandle, name: &OsStr) -> Result<OwnedHandle, Refusal> {
        match nt_open_relative(parent, name, DIR_ACCESS, FILE_DIRECTORY_FILE) {
            Ok(handle) => {
                let object = inspect(&handle).map_err(Refusal::Io)?;
                if object.is_reparse() {
                    return Err(Refusal::Reparse { tag: object.tag });
                }
                if !object.is_directory() {
                    // The kernel enforced FILE_DIRECTORY_FILE; anything else
                    // here is an unexpected object type — refused.
                    return Err(Refusal::Io(io::Error::new(
                        io::ErrorKind::NotADirectory,
                        "path component is not a directory",
                    )));
                }
                Ok(handle)
            }
            Err(NtFailure::Status(STATUS_REPARSE_POINT_ENCOUNTERED)) => {
                Err(Refusal::Reparse { tag: None })
            }
            Err(NtFailure::Status(STATUS_NOT_A_DIRECTORY)) => {
                let probe = nt_open_relative(parent, name, PROBE_ACCESS, 0)
                    .ok()
                    .and_then(|probe| inspect(&probe).ok());
                Err(match probe {
                    Some(object) if object.is_reparse() => Refusal::Reparse { tag: object.tag },
                    _ => Refusal::Io(status_error(STATUS_NOT_A_DIRECTORY)),
                })
            }
            Err(failure) => Err(Refusal::Io(failure.into_io())),
        }
    }

    pub(super) fn open_confined_impl(
        root: &Path,
        components: &[&OsStr],
    ) -> Result<File, OpenViolation> {
        let Some((leaf, intermediates)) = components.split_last() else {
            return Err(OpenViolation::Io(invalid_input("empty source path")));
        };
        let mut dir = open_root(root).map_err(OpenViolation::Io)?;
        for name in intermediates {
            dir = open_child_dir(&dir, name).map_err(|r| r.into_open(name))?;
        }
        let handle = match nt_open_relative(&dir, leaf, LEAF_ACCESS, 0) {
            Ok(handle) => handle,
            // The kernel honoured OBJ_DONT_REPARSE before a handle existed:
            // a reparse point, refused — its tag unknowable from here.
            Err(NtFailure::Status(STATUS_REPARSE_POINT_ENCOUNTERED)) => {
                return Err(OpenViolation::Symlink {
                    component: PathBuf::from(leaf),
                    tag: None,
                })
            }
            Err(failure) => return Err(OpenViolation::Io(failure.into_io())),
        };
        // The handle that passed confinement is the handle that is classified:
        // a reparse point of any tag refuses (E0012); a directory or a device
        // — anything that is not a plain file — is NotRegular, BEFORE any read.
        let object = inspect(&handle).map_err(OpenViolation::Io)?;
        if object.is_reparse() {
            return Err(OpenViolation::Symlink {
                component: PathBuf::from(leaf),
                tag: object.tag,
            });
        }
        if object.is_directory() || object.is_device() {
            return Err(OpenViolation::NotRegular);
        }
        Ok(File::from(handle))
    }

    /// Thread the handle down the chain (see the unix `validate_dir_chain`):
    /// each component opened relative to its parent's handle, the final
    /// validated directory handle returned for handle-based enumeration.
    fn validate_dir_chain(
        root: &Path,
        components: &[&OsStr],
    ) -> Result<OwnedHandle, ListViolation> {
        let root_handle = match open_root(root) {
            Ok(h) => h,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(ListViolation::NotFound),
            Err(e) => return Err(ListViolation::Io(e)),
        };
        components.iter().try_fold(root_handle, |dir, name| {
            open_child_dir(&dir, name).map_err(|r| r.into_list(name))
        })
    }

    pub(super) fn list_dir_impl(
        root: &Path,
        _rel: &Path,
        components: &[&OsStr],
    ) -> Result<Vec<DirEntryInfo>, ListViolation> {
        let dir = validate_dir_chain(root, components)?;
        enumerate(&dir).map_err(ListViolation::Io)
    }

    fn overrun() -> io::Error {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "directory enumeration record overruns its buffer",
        )
    }

    /// Enumerate the validated directory THROUGH its handle
    /// (`FileIdBothDirectoryInfo`) — never by re-resolving a path — reporting
    /// each entry's no-follow type from the attributes the enumeration itself
    /// carries: a reparse point of any tag is a `Symlink` entry, never its
    /// target's type. Every record read is bounds-checked against the buffer
    /// the kernel filled; a malformed chain refuses the listing (no silent
    /// truncation).
    fn enumerate(dir: &OwnedHandle) -> io::Result<Vec<DirEntryInfo>> {
        const HEADER: usize = std::mem::size_of::<FILE_ID_BOTH_DIR_INFO>();
        const NAME_OFFSET: usize = std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
        // 64 KiB, u64-backed so the records' 8-byte fields land aligned (the
        // reads below tolerate any alignment regardless).
        let mut buf: Vec<u64> = vec![0; 8 * 1024];
        let buf_bytes = buf.len() * std::mem::size_of::<u64>();
        let mut class = FileIdBothDirectoryRestartInfo;
        let mut out = Vec::new();
        loop {
            // SAFETY: `dir` is live for the call; `buf` is a live, writable
            // buffer whose exact byte length is passed; the class names a
            // directory-enumeration record type the buffer receives.
            let ok = unsafe {
                GetFileInformationByHandleEx(
                    raw(dir),
                    class,
                    buf.as_mut_ptr().cast::<c_void>(),
                    buf_bytes as u32,
                )
            };
            if ok == 0 {
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                    break;
                }
                return Err(err);
            }
            class = FileIdBothDirectoryInfo;
            let base = buf.as_ptr().cast::<u8>();
            let mut offset = 0usize;
            loop {
                if offset + HEADER > buf_bytes {
                    return Err(overrun());
                }
                // SAFETY: `offset + HEADER <= buf_bytes` (checked above), so the
                // read stays inside `buf`, which the kernel filled with a chain
                // of FILE_ID_BOTH_DIR_INFO records; `read_unaligned` needs no
                // alignment; the struct is `Copy`.
                let entry: FILE_ID_BOTH_DIR_INFO = unsafe {
                    ptr::read_unaligned(base.add(offset).cast::<FILE_ID_BOTH_DIR_INFO>())
                };
                let name_bytes = entry.FileNameLength as usize;
                let name_start = offset + NAME_OFFSET;
                if name_bytes % 2 != 0 || name_start + name_bytes > buf_bytes {
                    return Err(overrun());
                }
                let name: Vec<u16> = (0..name_bytes / 2)
                    .map(|i| {
                        // SAFETY: `name_start + name_bytes <= buf_bytes` (checked
                        // above): every unit read lies inside `buf`; unaligned.
                        unsafe { ptr::read_unaligned(base.add(name_start + 2 * i).cast::<u16>()) }
                    })
                    .collect();
                let dot = u16::from(b'.');
                if name != [dot] && name != [dot, dot] {
                    let object = Object {
                        attributes: entry.FileAttributes,
                        tag: None,
                    };
                    let file_type = if object.is_reparse() {
                        EntryType::Symlink
                    } else if object.is_directory() {
                        EntryType::Dir
                    } else if object.is_device() {
                        EntryType::Other
                    } else {
                        EntryType::File
                    };
                    out.push(DirEntryInfo {
                        name: OsString::from_wide(&name),
                        file_type,
                    });
                }
                if entry.NextEntryOffset == 0 {
                    break;
                }
                // A record chain that wraps (a 32-bit offset overflowing the
                // usize walk) is malformed — refused, never re-read.
                offset = offset
                    .checked_add(entry.NextEntryOffset as usize)
                    .ok_or_else(overrun)?;
            }
        }
        Ok(out)
    }
}

/// Test-only per-platform symlink fixtures (#64): the confinement guarantee is
/// universal, so the symlink red-gates run on Unix AND Windows through one
/// helper. A creation failure FAILS the test loudly, never skips it: on
/// Windows the runner needs `SeCreateSymbolicLinkPrivilege` or Developer Mode,
/// which the GitHub `windows-latest` image grants.
#[cfg(all(test, any(unix, windows)))]
pub(crate) mod test_symlink {
    use std::path::Path;

    fn must(result: std::io::Result<()>, what: &str, link: &Path) {
        if let Err(e) = result {
            panic!(
                "{what} fixture creation must succeed on this runner (Windows: \
                 SeCreateSymbolicLinkPrivilege or Developer Mode) at {}: {e}",
                link.display()
            );
        }
    }

    /// A symlink to a FILE (`symlink_file` on Windows).
    pub(crate) fn file(target: impl AsRef<Path>, link: impl AsRef<Path>) {
        #[cfg(unix)]
        let result = std::os::unix::fs::symlink(target.as_ref(), link.as_ref());
        #[cfg(windows)]
        let result = std::os::windows::fs::symlink_file(target.as_ref(), link.as_ref());
        must(result, "file symlink", link.as_ref());
    }

    /// A symlink to a DIRECTORY (`symlink_dir` on Windows).
    pub(crate) fn dir(target: impl AsRef<Path>, link: impl AsRef<Path>) {
        #[cfg(unix)]
        let result = std::os::unix::fs::symlink(target.as_ref(), link.as_ref());
        #[cfg(windows)]
        let result = std::os::windows::fs::symlink_dir(target.as_ref(), link.as_ref());
        must(result, "directory symlink", link.as_ref());
    }

    /// An NTFS junction (`mklink /J`): a reparse point carrying the
    /// mount-point tag — the tag a volume mount point carries too — so the
    /// junction gates pin the whole mount-point class. Needs no privilege.
    #[cfg(windows)]
    pub(crate) fn junction(target: &Path, link: &Path) {
        use std::os::windows::process::CommandExt;
        // cmd.exe parses its own command line: hand it ONE raw, quoted line so
        // a space or `&` in a temp path can neither split nor inject it.
        let output = std::process::Command::new("cmd")
            .raw_arg(format!(
                "/C mklink /J \"{}\" \"{}\"",
                link.display(),
                target.display()
            ))
            .output()
            .unwrap_or_else(|e| panic!("cmd /C mklink /J must run on this runner: {e}"));
        assert!(
            output.status.success(),
            "mklink /J {} {} must succeed: {}",
            link.display(),
            target.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        let is_reparse = std::fs::symlink_metadata(link)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        assert!(is_reparse, "the junction must exist as a reparse point");
    }
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

    // SE-F7, revised by the #103 phase-3 S-1 fix: a non-regular leaf
    // (directory, FIFO, socket, device) is refused at open with `NotRegular` —
    // decided on the fstat of the handle that passed confinement, BEFORE any
    // read. A FIFO would otherwise park the process in a blocking read (a
    // one-file denial of verification).
    #[cfg(any(unix, windows))]
    #[test]
    fn directory_as_leaf_is_refused_not_regular() {
        let root = temp_root("dir-leaf");
        std::fs::create_dir_all(root.join("adir")).unwrap();
        let err = open_confined(&root, &confined("adir")).unwrap_err();
        assert!(matches!(err, OpenViolation::NotRegular), "got {err:?}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    // #103 phase-3 S-1 (the reproduced hang): a FIFO leaf must be REFUSED,
    // promptly — not opened (a blocking open/read would hang the run forever).
    #[cfg(unix)]
    #[test]
    fn fifo_as_leaf_is_refused_not_regular_without_blocking() {
        use std::os::unix::ffi::OsStrExt;
        let root = temp_root("fifo-leaf");
        let fifo = root.join("pipe.md");
        let c_path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) }, 0);

        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _ = tx.send(matches!(
                open_confined(&root, &confined("pipe.md")),
                Err(OpenViolation::NotRegular)
            ));
        });
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(refused) => assert!(refused, "a FIFO leaf must be NotRegular-refused"),
            Err(_) => panic!("opening a FIFO blocked (the S-1 hang): O_NONBLOCK missing"),
        }
        let _ = worker.join();
        let _ = std::fs::remove_file(std::path::PathBuf::from(std::ffi::OsStr::from_bytes(
            c_path.as_bytes(),
        )));
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

    /// Number of confine ops the leak test drives. A genuine per-op descriptor
    /// leak grows the process fd count by ~this many; the leak signal is
    /// therefore an order of magnitude above [`FD_LEAK_TOLERANCE`].
    #[cfg(unix)]
    const FD_LEAK_OPS: usize = 300;

    /// How much process-global fd growth the leak test tolerates. `/proc/self/fd`
    /// is shared across threads and cargo runs tests in parallel, so a bounded
    /// number of descriptors held by unrelated concurrent tests is expected
    /// noise. This ceiling sits far below the per-op leak signal ([`FD_LEAK_OPS`])
    /// yet well above realistic parallel churn — it separates leak from noise
    /// rather than measuring an exact count.
    #[cfg(unix)]
    const FD_LEAK_TOLERANCE: usize = 32;

    /// Process-global open-fd count, filtered for transient parallel noise by
    /// taking the minimum over several rapid reads: a descriptor another thread
    /// holds only momentarily is unlikely to be present in every sample, while a
    /// real leak persists and survives the min.
    #[cfg(unix)]
    fn quiescent_fd_count() -> usize {
        (0..8).map(|_| open_fd_count()).min().unwrap_or(0)
    }

    // RED GATE (#75): the old assertion took a single process-global fd-count
    // snapshot and allowed only +2. /proc/self/fd is shared across all threads,
    // and cargo runs these tests in parallel — so a handful of descriptors held
    // by *unrelated* concurrent tests (zero leak in confine) perturbs the delta
    // past +2 and fails the build intermittently. This test reproduces that race
    // deterministically: holding 8 unrelated fds open across the window moves the
    // single-snapshot delta to +8, which trips the old `before + 2` bound. The
    // fixed invariant (min-sampling + a tolerance far below the per-op leak
    // signal) survives it.
    #[cfg(unix)]
    #[test]
    fn red_gate_fd_count_race_from_unrelated_open_fds() {
        let root = temp_root("fd-race");
        std::fs::write(root.join("f.yaml"), "k: v\n").unwrap();

        let before = open_fd_count();
        // Simulate unrelated concurrent tests holding descriptors during the
        // measurement window — no confine op leaks anything here.
        let mut held = Vec::new();
        for _ in 0..8 {
            held.push(std::fs::File::open(root.join("f.yaml")).unwrap());
        }
        let after = open_fd_count();

        // The old, too-tight bound would have failed despite zero leak:
        assert!(
            after > before + 2,
            "expected the unrelated fds to breach the old +2 bound: {before} -> {after}"
        );
        // The fixed bound tolerates bounded parallel churn while staying far
        // below a real per-op leak (+OPS):
        assert!(
            after <= before + FD_LEAK_TOLERANCE,
            "held fds should be within the leak tolerance: {before} -> {after}"
        );

        drop(held);
        std::fs::remove_dir_all(&root).unwrap();
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
        let before = quiescent_fd_count();
        for _ in 0..FD_LEAK_OPS {
            exercise(&root);
        }
        let after = quiescent_fd_count();
        // A genuine leak grows ~1 fd per op (→ +FD_LEAK_OPS); the tolerance
        // absorbs bounded process-global fd churn from parallel tests while
        // staying an order of magnitude below any real leak. See #75.
        assert!(
            after <= before + FD_LEAK_TOLERANCE,
            "descriptor leak across {FD_LEAK_OPS} confine ops: {before} -> {after} (invariant I7)"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn symlink_leaf_is_refused_even_when_target_is_inside_root() {
        let root = temp_root("leaf-link");
        std::fs::write(root.join("real.yaml"), "k: v\n").unwrap();
        test_symlink::file(root.join("real.yaml"), root.join("alias.yaml"));

        let err = open_confined(&root, &confined("alias.yaml")).unwrap_err();
        match err {
            OpenViolation::Symlink { component, .. } => {
                assert_eq!(component, PathBuf::from("alias.yaml"));
            }
            other => panic!("expected Symlink, got {other:?}"),
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn symlinked_intermediate_component_is_refused() {
        let root = temp_root("mid-link");
        let outside = temp_root("mid-link-outside");
        std::fs::write(outside.join("data.yaml"), "k: v\n").unwrap();
        test_symlink::dir(&outside, root.join("sub"));

        let err = open_confined(&root, &confined("sub/data.yaml")).unwrap_err();
        match err {
            OpenViolation::Symlink { component, .. } => {
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

    #[cfg(any(unix, windows))]
    #[test]
    fn list_dir_through_symlinked_component_is_refused_without_disclosure() {
        // The closed-world guarantee: a symlinked intermediate directory is
        // refused before any entry of its target is read, so an out-of-tree
        // directory's filenames are never enumerated.
        let root = temp_root("list-symlink");
        let outside = temp_root("list-symlink-outside");
        std::fs::write(outside.join("secret.yaml"), "k: v\n").unwrap();
        test_symlink::dir(&outside, root.join("sub"));

        let err = list_dir(&root, Path::new("sub")).unwrap_err();
        match err {
            ListViolation::Symlink { component, .. } => assert_eq!(component, PathBuf::from("sub")),
            other => panic!("expected Symlink refusal, got {other:?}"),
        }
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn list_dir_reports_symlink_entry_as_symlink_not_its_target() {
        let root = temp_root("list-symlink-entry");
        std::fs::write(root.join("real.yaml"), "k: v\n").unwrap();
        test_symlink::file(root.join("real.yaml"), root.join("alias.yaml"));

        let entries = list_dir(&root, Path::new("")).unwrap();
        let alias = entries.iter().find(|e| e.name == "alias.yaml").unwrap();
        assert_eq!(alias.file_type, EntryType::Symlink);
        std::fs::remove_dir_all(&root).unwrap();
    }

    // ── #64: the guarantee is universal — the Windows walk's own red-gates ──
    //
    // Windows reparse points come in several tags — symlink, junction (the
    // mount-point tag), and volume mount point (the same mount-point tag on a
    // volume GUID) — and the walk refuses on the FILE_ATTRIBUTE_REPARSE_POINT
    // attribute alone, tag-agnostic. A volume mount point cannot be created on
    // a CI runner (it needs a spare volume), so it is not fixtured: it carries
    // the junction's tag and the same attribute, so it falls in exactly the
    // refused class the junction gates pin. Symlink creation on the runner is
    // asserted by the fixture helper, never skipped. Plain files and plain
    // directories still opening is pinned by the ungated tests above
    // (`opens_nested_file_through_handles`, `opens_deeply_nested_file`,
    // `list_dir_returns_sorted_entries_with_types`), which run on every target.

    /// A directory symlink as the LEAF is a symlink first (E0012), not a
    /// not-regular directory: the handle's reparse attribute is classified
    /// before anything else.
    #[cfg(any(unix, windows))]
    #[test]
    fn symlinked_directory_leaf_is_refused_as_symlink() {
        let root = temp_root("dir-leaf-link");
        std::fs::create_dir_all(root.join("real")).unwrap();
        test_symlink::dir(root.join("real"), root.join("alias"));
        let err = open_confined(&root, &confined("alias")).unwrap_err();
        match err {
            OpenViolation::Symlink { component, .. } => {
                assert_eq!(component, PathBuf::from("alias"))
            }
            other => panic!("expected Symlink, got {other:?}"),
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// An intermediate that is a plain FILE refuses as IO (not-a-directory) —
    /// it never opens through, for reads and for listing alike.
    #[cfg(any(unix, windows))]
    #[test]
    fn file_as_intermediate_is_refused_as_io() {
        let root = temp_root("file-mid");
        std::fs::write(root.join("notadir"), "k: v\n").unwrap();
        let err = open_confined(&root, &confined("notadir/x.yaml")).unwrap_err();
        assert!(matches!(err, OpenViolation::Io(_)), "got {err:?}");
        let err = list_dir(&root, Path::new("notadir")).unwrap_err();
        assert!(matches!(err, ListViolation::Io(_)), "got {err:?}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn junction_as_intermediate_is_refused_without_disclosure() {
        let root = temp_root("junction-mid");
        let outside = temp_root("junction-mid-outside");
        std::fs::write(outside.join("SECRET-OUTSIDE.yaml"), "k: v\n").unwrap();
        test_symlink::junction(&outside, &root.join("sub"));

        let err = open_confined(&root, &confined("sub/SECRET-OUTSIDE.yaml")).unwrap_err();
        match err {
            OpenViolation::Symlink { component, .. } => assert_eq!(component, PathBuf::from("sub")),
            other => panic!("a junction is a reparse point and refuses as one; got {other:?}"),
        }
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn junction_as_leaf_is_refused_as_symlink() {
        let root = temp_root("junction-leaf");
        std::fs::create_dir_all(root.join("real")).unwrap();
        test_symlink::junction(&root.join("real"), &root.join("alias"));

        let err = open_confined(&root, &confined("alias")).unwrap_err();
        match err {
            OpenViolation::Symlink { component, .. } => {
                assert_eq!(component, PathBuf::from("alias"))
            }
            other => panic!("a junction leaf refuses as a symlink; got {other:?}"),
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn list_dir_through_junctioned_component_is_refused_without_disclosure() {
        let root = temp_root("list-junction");
        let outside = temp_root("list-junction-outside");
        std::fs::write(outside.join("SECRET-OUTSIDE.yaml"), "k: v\n").unwrap();
        test_symlink::junction(&outside, &root.join("sub"));

        let err = list_dir(&root, Path::new("sub")).unwrap_err();
        match &err {
            ListViolation::Symlink { component, .. } => {
                assert_eq!(component, &PathBuf::from("sub"))
            }
            other => panic!("enumeration through a junction is refused; got {other:?}"),
        }
        assert!(
            !format!("{err:?}").contains("SECRET-OUTSIDE"),
            "the refusal must not disclose the junction target's contents"
        );
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn list_dir_reports_junction_entry_as_symlink_not_its_target() {
        let root = temp_root("list-junction-entry");
        std::fs::create_dir_all(root.join("real")).unwrap();
        test_symlink::junction(&root.join("real"), &root.join("j"));

        let entries = list_dir(&root, Path::new("")).unwrap();
        let j = entries.iter().find(|e| e.name == "j").unwrap();
        assert_eq!(j.file_type, EntryType::Symlink);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The root itself is opened without following reparse points and is
    /// refused when it is one: fail-closed even for the trusted root (the
    /// pipeline always passes the canonical, fully-resolved root).
    #[cfg(windows)]
    #[test]
    fn reparse_point_root_is_refused_fail_closed() {
        let parent = temp_root("junction-root");
        std::fs::create_dir_all(parent.join("real")).unwrap();
        std::fs::write(parent.join("real/x.yaml"), "k: v\n").unwrap();
        test_symlink::junction(&parent.join("real"), &parent.join("j"));

        let err = open_confined(&parent.join("j"), &confined("x.yaml")).unwrap_err();
        assert!(matches!(err, OpenViolation::Io(_)), "got {err:?}");
        let err = list_dir(&parent.join("j"), Path::new("")).unwrap_err();
        assert!(matches!(err, ListViolation::Io(_)), "got {err:?}");
        std::fs::remove_dir_all(&parent).unwrap();
    }

    /// #64 cold-review W7: a FILE symlink squatting an intermediate component
    /// is refused AS A SYMLINK — on Linux `openat(O_DIRECTORY|O_NOFOLLOW)`
    /// says ELOOP, on macOS ENOTDIR is disambiguated by `fstatat`, on Windows
    /// `STATUS_NOT_A_DIRECTORY` is disambiguated by the classification probe —
    /// for reads and for listing alike.
    #[cfg(any(unix, windows))]
    #[test]
    fn file_symlink_as_intermediate_is_refused_as_symlink() {
        let root = temp_root("file-link-mid");
        let outside = temp_root("file-link-mid-outside");
        std::fs::write(outside.join("file.yaml"), "k: v\n").unwrap();
        test_symlink::file(outside.join("file.yaml"), root.join("sub"));

        let err = open_confined(&root, &confined("sub/x.yaml")).unwrap_err();
        match err {
            OpenViolation::Symlink { component, .. } => assert_eq!(component, PathBuf::from("sub")),
            other => {
                panic!("a file symlink as an intermediate is a Symlink refusal; got {other:?}")
            }
        }
        let err = list_dir(&root, Path::new("sub")).unwrap_err();
        match err {
            ListViolation::Symlink { component, .. } => assert_eq!(component, PathBuf::from("sub")),
            other => panic!("listing through a file symlink is a Symlink refusal; got {other:?}"),
        }
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    /// #64 cold-review W8: ~1000 entries — more than one 64 KiB enumeration
    /// buffer on Windows, so the restart → continuation refill branch runs
    /// (on Unix, a long `readdir` stream) — listed completely, each once.
    #[test]
    fn list_dir_enumerates_a_thousand_entries_completely() {
        let root = temp_root("list-thousand");
        for i in 0..1000 {
            std::fs::write(root.join(format!("entry-{i:04}.yaml")), "k: v\n").unwrap();
        }
        let entries = list_dir(&root, Path::new("")).unwrap();
        assert_eq!(entries.len(), 1000, "every entry listed exactly once");
        assert!(entries.iter().all(|e| e.file_type == EntryType::File));
        assert_eq!(entries[0].name, "entry-0000.yaml");
        assert_eq!(entries[999].name, "entry-0999.yaml");
        let distinct: std::collections::BTreeSet<&OsStr> =
            entries.iter().map(|e| e.name.as_os_str()).collect();
        assert_eq!(
            distinct.len(),
            1000,
            "no entry is repeated across buffer refills"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// #64 cold-review W1: a refused reparse point is described as what it is
    /// — the refusal never varies by class, only the remedy does.
    #[test]
    fn reparse_classes_are_named_with_their_own_remedy() {
        use ReparseClass::*;
        assert_eq!(classify_reparse(None), NameSurrogate);
        assert_eq!(classify_reparse(Some(0xA000_000C)), NameSurrogate); // symlink
        assert_eq!(classify_reparse(Some(0xA000_0003)), NameSurrogate); // junction / mount point
        assert_eq!(classify_reparse(Some(0x9000_001A)), CloudPlaceholder);
        assert_eq!(classify_reparse(Some(0x9000_301A)), CloudPlaceholder); // CLOUD_3
        assert_eq!(classify_reparse(Some(0x8000_0017)), WofCompressed);
        assert_eq!(classify_reparse(Some(0x8000_0013)), Deduplicated);
        assert_eq!(classify_reparse(Some(0x9000_001C)), ProjectedFile);
        assert_eq!(classify_reparse(Some(0x8000_0099)), Unknown);

        // N3: a missing tag is the symlink only where symlinks are the only
        // no-follow class (Unix); on Windows it is a kernel-refused reparse
        // point whose tag never became readable, and it says so.
        let link = describe_reparse(None);
        if cfg!(windows) {
            assert!(link.what.contains("tag unavailable"), "{}", link.what);
        } else {
            assert_eq!(link.what, "a symbolic link");
        }
        assert!(
            link.help.is_none(),
            "the tagless class keeps each site's own remedy"
        );
        let symlink = describe_reparse(Some(0xA000_000C));
        assert_eq!(symlink.what, "a symbolic link");
        assert!(symlink.help.is_none());
        assert!(describe_reparse(Some(0xA000_0003))
            .what
            .contains("junction"));
        let cloud = describe_reparse(Some(0x9000_001A));
        assert!(cloud.what.contains("cloud-file placeholder") && cloud.what.contains("0x9000001a"));
        assert!(cloud.help.as_deref().is_some_and(|h| h.contains("hydrate")));
        assert!(describe_reparse(Some(0x8000_0017))
            .help
            .as_deref()
            .is_some_and(|h| h.contains("compact /u")));
        // N2: the message names the thing; the "never follows" sentence is
        // help, so the "resolves through {what}" clause reads as one sentence.
        let unknown = describe_reparse(Some(0x8000_0099));
        assert!(unknown.what.contains("0x80000099") && !unknown.what.contains("never follows"));
        assert!(unknown
            .help
            .as_deref()
            .is_some_and(|h| h.contains("plain file") && h.contains("never follows")));
    }
}
