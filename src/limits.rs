//! The declared-limits catalog (#92 sub-lane D; `DESIGN.md` § Verification is
//! fast where it is invoked: "Hook-time cost is bounded by declared limits
//! shipped as data").
//!
//! ONE data structure declares every input/enumeration bound the engine
//! enforces, with the bound-name strings that surface in `bound_exceeded`
//! diagnostics — so the catalog, the enforcement sites, and the operator-facing
//! documentation cannot drift apart. Enforcement sites take their values from
//! here; the historical `pub const` names re-export the shipped values for
//! compatibility; and `docs/limits.md` is RENDERED from here ([`render_page`]),
//! not hand-synced and checked (#189: the registry-as-generator idiom, reference
//! `ruff-registry-audit` — drift made impossible rather than caught). The
//! committed page must equal its own rendering (a shipped test enforces it and
//! regenerates it under `MDATRON_UPDATE_DOCS=1`), and `mdatron docs limits`
//! renders the running binary's catalog, so the printed page cannot disagree
//! with the enforced values.
//!
//! Two bound classes are deliberately NOT in this catalog: the YAML alias
//! (`repetition limit exceeded`) and recursion (`recursion limit exceeded`)
//! guards ride the parser (`serde_yaml_ng`) and surface as parse diagnostics —
//! pinned by fixture, not re-implemented; and there is no global wall-clock
//! budget (DESIGN's enforcement-status note records that honestly).

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
    /// Maximum bytes of one `--compact` finding block — an OUTPUT contract
    /// limit rather than an input bound (`DESIGN.md` § Agents are the first
    /// consumer; ratified 2026-07-25, #80 D4), shipped as data with the rest
    /// (#189). A block is cut to fit; it never exceeds this.
    pub compact_finding_bytes: usize,
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
    compact_finding_bytes: 512,
};

/// One row of the operator-facing table in `docs/limits.md` (#189): the
/// catalog is the source and the page is its rendering. The surface and
/// on-exceedance prose lives HERE, beside the value it describes, so a bound
/// cannot change without its documentation changing in the same edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitRow {
    /// The bound as diagnostics and DESIGN spell it.
    pub limit: &'static str,
    /// The shipped value rendered for humans (`8 MiB`, `100 000`, `512 B`).
    pub shipped: String,
    /// What the bound measures.
    pub surface: &'static str,
    /// What exceeding it produces.
    pub on_exceedance: &'static str,
}

impl Limits {
    /// The catalog as table rows, in the order of DESIGN's bounds sentence,
    /// with the compact output limit last.
    pub fn rows(&self) -> Vec<LimitRow> {
        vec![
            LimitRow {
                limit: "`max-input-size-per-file`",
                shipped: fmt_bytes(self.per_file_bytes),
                surface:
                    "every captured input (bodies, index sources, pin/cite/link/marker targets)",
                on_exceedance: "config-scoped: `bound_exceeded`; prose-scoped: `W0048` degrade",
            },
            LimitRow {
                limit: "`aggregate-snapshot-size`",
                shipped: fmt_bytes(self.aggregate_bytes),
                surface: "total bytes stored in one run's snapshot",
                on_exceedance: "config-scoped: `bound_exceeded`; prose-scoped: `W0048` degrade",
            },
            LimitRow {
                limit: "`structural-nesting-depth`",
                shipped: fmt_count(self.structural_nesting),
                surface: "flow-collection nesting in any YAML mdatron parses — frontmatter, `.yaml` index sources, `.mdatron/` files — counted before parsing (bracket bytes, quoted or not)",
                on_exceedance: "governed file: `bound_exceeded`; index source: `index_build`; `.mdatron/` file: its load error; link, marker or pin target: read as having no frontmatter",
            },
            LimitRow {
                limit: "DSL expression depth",
                shipped: fmt_count(self.expr_depth),
                surface: "adopter `assert:` expression nesting",
                on_exceedance: "expression `ParseError` at pattern load",
            },
            LimitRow {
                limit: "walk `depth`",
                shipped: fmt_count(self.walk_depth),
                surface: "engine-owned no-follow glob walk (index sources)",
                on_exceedance: "`WalkBounded` index error",
            },
            LimitRow {
                limit: "walk `entries`",
                shipped: fmt_count(self.walk_entries),
                surface: "directory entries listed across one glob walk",
                on_exceedance: "`WalkBounded` index error",
            },
            LimitRow {
                limit: "`concurrent-invocation-count`",
                shipped: fmt_count(self.concurrent_invocations),
                surface: "simultaneous `verify` runs per user per project root",
                on_exceedance: "`bound_exceeded`",
            },
            LimitRow {
                limit: "compact per-finding size",
                shipped: fmt_bytes(self.compact_finding_bytes),
                surface: "one finding block of `--compact` output",
                on_exceedance: "the block is cut to fit (`…`); never exceeded",
            },
        ]
    }

    /// The markdown table `docs/limits.md` carries between its generated-block
    /// markers: a header row and one row per [`LimitRow`].
    pub fn render_table(&self) -> String {
        let mut out = String::from(
            "| Limit | Shipped value | Surface | On exceedance |\n|---|---|---|---|\n",
        );
        for r in self.rows() {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                r.limit, r.shipped, r.surface, r.on_exceedance
            ));
        }
        out
    }
}

/// Opens the generated block in `docs/limits.md`; everything between it and
/// [`GENERATED_END`] is replaced by [`Limits::render_table`] on render.
pub const GENERATED_START: &str =
    "<!-- mdatron-generated: rendered from src/limits.rs (SHIPPED) — do not edit by hand -->";
/// Closes the generated block in `docs/limits.md`.
pub const GENERATED_END: &str = "<!-- mdatron-generated: end -->";

/// Render the limits page: `template` (the committed `docs/limits.md`) with
/// the block between the generated markers replaced by `limits`' table. Errs,
/// naming the defect, when the template lacks either marker or has them out of
/// order — a page that cannot carry the rendering must not be printed as if
/// it did. Line endings are normalized to LF first, so a CRLF checkout (a
/// Windows `autocrlf` clone) renders the same bytes as an LF one and the
/// regenerated page is always LF (L3 cold review MINOR-8).
pub fn render_page(limits: &Limits, template: &str) -> Result<String, String> {
    let template = &template.replace("\r\n", "\n");
    let start = template.find(GENERATED_START).ok_or_else(|| {
        format!("the limits page lacks its generated-block start marker `{GENERATED_START}`")
    })?;
    let after_start = start + GENERATED_START.len();
    let end = template[after_start..]
        .find(GENERATED_END)
        .map(|rel| after_start + rel)
        .ok_or_else(|| {
            format!("the limits page lacks its generated-block end marker `{GENERATED_END}` after the start marker")
        })?;
    let mut out = String::with_capacity(template.len() + 512);
    out.push_str(&template[..after_start]);
    out.push('\n');
    out.push_str(&limits.render_table());
    out.push_str(&template[end..]);
    Ok(out)
}

/// Bytes for humans: whole MiB or KiB when exact, else bytes.
fn fmt_bytes(n: usize) -> String {
    const MIB: usize = 1024 * 1024;
    if n != 0 && n % MIB == 0 {
        format!("{} MiB", n / MIB)
    } else if n != 0 && n % 1024 == 0 {
        format!("{} KiB", n / 1024)
    } else {
        format!("{n} B")
    }
}

/// Counts for humans: thousands separated by a space (`100 000`).
fn fmt_count(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

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
/// permissive mode this call. `pub(crate)` so the verify-layer test can learn
/// the slot path to reproduce the repaired-busy scenario (R4-1).
#[cfg(unix)]
pub(crate) fn slot_dir(project_root: &Path) -> std::io::Result<(std::path::PathBuf, bool)> {
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
    // per-user temp dirs) — see the module note. (Since #64 the confine walk
    // itself carries no check-to-open window on any declared target; this
    // slot-file residual is the one that remains.)
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
pub(crate) fn slot_dir(project_root: &Path) -> std::io::Result<(std::path::PathBuf, bool)> {
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
    // SAFETY: as_raw_fd() yields a valid fd owned by `file` for the duration of
    // the call; flock touches only kernel lock state and cannot violate memory
    // safety.
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

/// Maximum flow-collection nesting depth (`[`/`{`) in a YAML text (#124,
/// roast SHO1 depth-bomb). O(n) pre-scan: a compact deeply-nested collection is
/// the cheapest way to drive quadratic YAML-parse blowup, and 256 is far beyond
/// any legitimate frontmatter.
pub fn max_flow_nesting(s: &str) -> usize {
    let mut depth = 0usize;
    let mut max = 0usize;
    for b in s.bytes() {
        match b {
            b'[' | b'{' => {
                depth += 1;
                if depth > max {
                    max = depth;
                }
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    max
}

/// Refuse YAML whose flow collections nest past [`SHIPPED`]'s
/// `structural_nesting` BEFORE it is parsed (#244): the parser's own recursion
/// guard fires only after a quadratic scan, so every YAML parse of committed
/// content runs this first. `Err` carries the depth found.
pub fn check_flow_nesting(yaml: &str) -> Result<(), usize> {
    match max_flow_nesting(yaml) {
        depth if depth > SHIPPED.structural_nesting => Err(depth),
        _ => Ok(()),
    }
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

    // The inversion (#189; reference: ruff-registry-audit — a registry as the
    // GENERATOR of its derived artifacts): docs/limits.md is the rendering of
    // SHIPPED, so the page must EQUAL render_page(&SHIPPED, page). Under
    // MDATRON_UPDATE_DOCS=1 the test rewrites the page from the catalog instead
    // of failing (expect-test style); the regenerated page is LF (the
    // .gitattributes pin keeps the checkout LF too), and a CRLF checkout is
    // compared after normalization so Windows CI reads the same bytes.
    #[test]
    fn docs_limits_page_is_rendered_from_shipped() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/limits.md");
        let page = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\r\n", "\n");
        let rendered = render_page(&SHIPPED, &page)
            .unwrap_or_else(|e| panic!("docs/limits.md cannot carry the rendering: {e}"));
        if rendered == page {
            return;
        }
        if std::env::var_os("MDATRON_UPDATE_DOCS").is_some() {
            std::fs::write(&path, &rendered).unwrap();
            return;
        }
        panic!(
            "docs/limits.md is not the rendering of limits::SHIPPED — regenerate it with \
             `MDATRON_UPDATE_DOCS=1 cargo test docs_limits_page` (or paste the block below \
             between the generated markers):\n{}",
            SHIPPED.render_table()
        );
    }

    // The shipped values, pinned literally (a bump edits SHIPPED, this test,
    // and — via MDATRON_UPDATE_DOCS — the page): each lands in its own
    // pipe-delimited cell, so a bare "8 MiB" substring cannot stay green under
    // a bump to "128 MiB" (R2-1).
    #[test]
    fn shipped_values_are_pinned() {
        let table = SHIPPED.render_table();
        let row = |needle: &str| {
            table
                .lines()
                .find(|l| l.starts_with('|') && l.contains(needle))
                .unwrap_or_else(|| panic!("no rendered row for {needle}"))
        };
        assert!(row("max-input-size-per-file").contains("| 8 MiB |"));
        assert!(row("aggregate-snapshot-size").contains("| 64 MiB |"));
        assert!(row("structural-nesting-depth").contains("| 256 |"));
        assert!(row("DSL expression depth").contains("| 256 |"));
        assert!(row("walk `depth`").contains("| 64 |"));
        assert!(row("walk `entries`").contains("| 100 000 |"));
        assert!(row("concurrent-invocation-count").contains("| 8 |"));
        assert!(row("compact per-finding size").contains("| 512 B |"));
        assert_eq!(table.lines().count(), 2 + SHIPPED.rows().len());
    }

    // The render is a function of the catalog, not a constant: a different
    // catalog renders a different table.
    #[test]
    fn render_table_is_a_function_of_the_catalog() {
        let mut other = SHIPPED;
        other.per_file_bytes = 128 * 1024 * 1024;
        other.walk_entries = 1_234_567;
        let table = other.render_table();
        assert!(table.contains("| 128 MiB |"), "{table}");
        assert!(table.contains("| 1 234 567 |"), "{table}");
        assert!(!table.contains("| 8 MiB |"));
    }

    #[test]
    fn render_page_replaces_only_the_generated_block_and_refuses_marker_less_pages() {
        let template = format!(
            "# Title\n\nprose before\n\n{GENERATED_START}\n| stale | table |\n{GENERATED_END}\n\nprose after\n"
        );
        let page = render_page(&SHIPPED, &template).unwrap();
        assert!(page.starts_with("# Title\n\nprose before\n\n"));
        assert!(page.ends_with(&format!("{GENERATED_END}\n\nprose after\n")));
        assert!(!page.contains("stale"));
        assert!(page.contains(&format!("{GENERATED_START}\n| Limit |")));
        // Idempotent: rendering a rendered page is the same page.
        assert_eq!(render_page(&SHIPPED, &page).unwrap(), page);

        assert!(render_page(&SHIPPED, "no markers at all").is_err());
        assert!(render_page(&SHIPPED, &format!("{GENERATED_START}\nopen only")).is_err());
        assert!(render_page(&SHIPPED, &format!("{GENERATED_END}\n{GENERATED_START}\n")).is_err());

        // CRLF in, LF out — the same bytes as the LF template (MINOR-8).
        let crlf = template.replace('\n', "\r\n");
        assert_eq!(render_page(&SHIPPED, &crlf).unwrap(), page);
        assert!(!page.contains('\r'));
    }

    #[test]
    fn human_formatting_is_exact_units_or_bytes() {
        assert_eq!(fmt_bytes(8 * 1024 * 1024), "8 MiB");
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(fmt_bytes(3 * 1024), "3 KiB");
        assert_eq!(fmt_bytes(1000), "1000 B");
        assert_eq!(fmt_bytes(0), "0 B");
        assert_eq!(fmt_count(8), "8");
        assert_eq!(fmt_count(256), "256");
        assert_eq!(fmt_count(1000), "1 000");
        assert_eq!(fmt_count(100_000), "100 000");
        assert_eq!(fmt_count(0), "0");
    }
}
