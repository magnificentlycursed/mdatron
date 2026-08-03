//! The one immutable snapshot (#103; `DESIGN.md` § Verification is fast where
//! it is invoked): each invocation reads its inputs ONCE, through validated
//! no-follow handles, into this structure — and every check runs against these
//! bytes. Capture is idempotent per path, so a file that is simultaneously a
//! governed body, an index source, and a citation target is read exactly once
//! and every consumer sees the same bytes; after the capture-complete seam the
//! run touches no filesystem.
//!
//! Capture-window semantics: the snapshot is filled over a window (bodies,
//! index sources, then discovered cross-file targets), not at an instant. The
//! read-once guarantee is per input; the seam marks the moment the window
//! closes. Mutations after capture never affect the in-flight run.
//!
//! Every capture is bounded (#124 discipline extended): a per-file byte cap and
//! a running aggregate cap apply to EVERY captured input — governed bodies,
//! index sources, and cross-file targets alike — so no input class is an
//! unbounded-read memory lever.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::confine::{self, ConfinedPath, OpenViolation};
use crate::verify::VerifyError;

/// Successfully captured bytes, classified once at capture.
#[derive(Debug)]
pub enum Content {
    /// Valid UTF-8 — the shape every text consumer (schema, rules, cite ranges,
    /// link anchors, marker members) requires.
    Utf8(String),
    /// A valid capture that is not UTF-8. Byte consumers (pin hashing) use it;
    /// text consumers treat the file as present-but-unreadable.
    Raw(Vec<u8>),
}

impl Content {
    pub fn bytes(&self) -> &[u8] {
        match self {
            Content::Utf8(s) => s.as_bytes(),
            Content::Raw(b) => b,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Content::Utf8(s) => Some(s),
            Content::Raw(_) => None,
        }
    }
}

/// The capture-time state of one input path. Checks map these states onto
/// their own finding taxonomy — the state is recorded once, neutrally, here.
#[derive(Debug)]
pub enum Captured {
    Content(Content),
    /// The file exceeded the per-file byte budget. Stored (so capture stays
    /// read-once even for refusals) but its bytes are dropped and NOT counted
    /// in the aggregate. Policy lives with the consumer: config-scoped input
    /// classes (governed bodies, index sources, pin and marker targets)
    /// escalate this to the whole-run bound error; prose-scoped classes
    /// (citation and link targets) treat it as present-but-unverifiable —
    /// a prose line must not be able to abort the run (#103 phase-3 A-1).
    TooLarge {
        limit: usize,
    },
    /// The confined open succeeded (the path exists) but the content is not
    /// verifiable — a read failure past the open, or a FIFO/socket/device/
    /// directory (refused before any read; a FIFO would block forever).
    /// Consumers that only test existence treat this as present.
    OpenedUnreadable {
        error: String,
    },
    /// No-follow resolution refused a symlinked component.
    SymlinkRefused {
        component: PathBuf,
    },
    /// The confined open itself failed — absent target or I/O refusal.
    OpenIo {
        error: String,
    },
}

/// The immutable snapshot: capture-once storage plus the capture budget.
#[derive(Debug)]
pub struct Snapshot {
    files: BTreeMap<PathBuf, Captured>,
    aggregate: usize,
    max_file_bytes: usize,
    max_aggregate_bytes: usize,
    sealed: bool,
}

impl Snapshot {
    pub fn new(max_file_bytes: usize, max_aggregate_bytes: usize) -> Self {
        Self {
            files: BTreeMap::new(),
            aggregate: 0,
            max_file_bytes,
            max_aggregate_bytes,
            sealed: false,
        }
    }

    /// Seal the snapshot at the capture-complete seam: any later `capture` is
    /// an engine defect and fails as one instead of silently reopening the
    /// filesystem window the seal exists to close.
    pub fn seal(&mut self) {
        self.sealed = true;
    }

    /// Capture `rel` under `root` if it is not already captured, and return its
    /// state. Idempotent: a second capture of the same path returns the stored
    /// state without touching the filesystem — read-once across roles, for
    /// refusals (`TooLarge`) as much as for content.
    ///
    /// Only the AGGREGATE budget is a hard error here (a global condition, not
    /// a per-file state; nothing is stored or counted when it trips). The
    /// per-file budget records a `TooLarge` state and lets the consumer choose
    /// the posture — see [`Captured::TooLarge`].
    pub fn capture(&mut self, root: &Path, rel: &ConfinedPath) -> Result<&Captured, VerifyError> {
        debug_assert!(!self.sealed, "capture after seal (engine defect)");
        if self.sealed {
            return Err(VerifyError::Io {
                path: crate::diagnostic::escape_path_text(&rel.as_path().to_string_lossy()),
                error: "capture after the seam sealed the snapshot (engine defect)".into(),
            });
        }
        let max_file_bytes = self.max_file_bytes;
        match self.files.entry(rel.as_path().to_path_buf()) {
            std::collections::btree_map::Entry::Occupied(entry) => Ok(&*entry.into_mut()),
            std::collections::btree_map::Entry::Vacant(slot) => {
                let state = match confine::open_confined(root, rel) {
                    Ok(handle) => {
                        let mut bytes = Vec::new();
                        // Read at most cap + 1 so an oversized file trips the
                        // bound instead of loading unbounded memory.
                        match handle
                            .take(max_file_bytes as u64 + 1)
                            .read_to_end(&mut bytes)
                        {
                            Err(e) => Captured::OpenedUnreadable {
                                error: e.to_string(),
                            },
                            Ok(_) if bytes.len() > max_file_bytes => Captured::TooLarge {
                                limit: max_file_bytes,
                            },
                            Ok(_) => {
                                // Commit-then-count: the aggregate reflects
                                // exactly the bytes the snapshot stores.
                                if self.aggregate + bytes.len() > self.max_aggregate_bytes {
                                    return Err(VerifyError::BoundExceeded {
                                        bound: "aggregate-snapshot-size".into(),
                                        detail: format!(
                                            "the captured inputs exceed the {}-byte aggregate limit",
                                            self.max_aggregate_bytes
                                        ),
                                    });
                                }
                                self.aggregate += bytes.len();
                                match String::from_utf8(bytes) {
                                    Ok(s) => Captured::Content(Content::Utf8(s)),
                                    Err(e) => Captured::Content(Content::Raw(e.into_bytes())),
                                }
                            }
                        }
                    }
                    Err(OpenViolation::Symlink { component }) => {
                        Captured::SymlinkRefused { component }
                    }
                    // A FIFO/socket/device/directory: it EXISTS but carries no
                    // verifiable content (and a FIFO read would block forever)
                    // — the same consumer posture as opened-but-unreadable.
                    Err(OpenViolation::NotRegular) => Captured::OpenedUnreadable {
                        error: "not a regular file".into(),
                    },
                    Err(OpenViolation::Io(e)) => Captured::OpenIo {
                        error: e.to_string(),
                    },
                };
                Ok(&*slot.insert(state))
            }
        }
    }

    /// The whole-run bound error a config-scoped consumer raises on a
    /// [`Captured::TooLarge`] state — one message shape for governed bodies,
    /// index sources, and pin/marker targets, with the adopter path escaped
    /// under the marking discipline.
    pub fn too_large_error(rel: &Path, limit: usize) -> VerifyError {
        VerifyError::BoundExceeded {
            bound: "max-input-size-per-file".into(),
            detail: format!(
                "'{}' exceeds the {limit}-byte per-file limit",
                crate::diagnostic::escape_path_text(&rel.to_string_lossy())
            ),
        }
    }

    /// The captured state of `rel`, if it was captured. Post-seam consumers use
    /// this exclusively — a `None` here means the path was never discovered
    /// during capture, and the caller fails loud, never falls back to the
    /// filesystem.
    pub fn get(&self, rel: &Path) -> Option<&Captured> {
        self.files.get(rel)
    }

    /// Convenience: the UTF-8 text of a captured file, when its state is
    /// `Content(Utf8)`.
    pub fn text(&self, rel: &Path) -> Option<&str> {
        match self.files.get(rel) {
            Some(Captured::Content(c)) => c.text(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confine::confine_lexically;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("mdatron-snapshot-{label}-{nanos}"));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn confined(rel: &str) -> ConfinedPath {
        confine_lexically(Path::new(rel)).unwrap()
    }

    // Read-once: after the first capture, the filesystem is never consulted
    // again for that path — deleting the file on disk does not change what a
    // second capture request returns.
    #[test]
    fn capture_is_idempotent_and_never_rereads() {
        let temp = TempDir::new("once");
        std::fs::write(temp.0.join("a.md"), "captured bytes").unwrap();
        let mut snap = Snapshot::new(1024, 4096);
        let rel = confined("a.md");
        match snap.capture(&temp.0, &rel).unwrap() {
            Captured::Content(c) => assert_eq!(c.text(), Some("captured bytes")),
            other => panic!("expected content; got {other:?}"),
        }
        std::fs::remove_file(temp.0.join("a.md")).unwrap();
        match snap.capture(&temp.0, &rel).unwrap() {
            Captured::Content(c) => assert_eq!(c.text(), Some("captured bytes")),
            other => panic!("a second capture must serve the stored state; got {other:?}"),
        }
    }

    // The per-file budget records a stored, memoized `TooLarge` state (the
    // consumer chooses abort vs degrade), its bytes are dropped, NOT counted
    // in the aggregate, and the refusal is read-once like any capture.
    #[test]
    fn per_file_budget_is_a_stored_memoized_state() {
        let temp = TempDir::new("per-file");
        std::fs::write(temp.0.join("big.md"), "0123456789").unwrap();
        std::fs::write(temp.0.join("ok.md"), "abc").unwrap();
        let mut snap = Snapshot::new(9, 12);
        match snap.capture(&temp.0, &confined("big.md")).unwrap() {
            Captured::TooLarge { limit } => assert_eq!(*limit, 9),
            other => panic!("expected TooLarge; got {other:?}"),
        }
        // Not counted toward the aggregate: 3 more bytes still fit a 12-byte
        // budget that 10 dropped bytes would have blown.
        match snap.capture(&temp.0, &confined("ok.md")).unwrap() {
            Captured::Content(c) => assert_eq!(c.text(), Some("abc")),
            other => panic!("expected content; got {other:?}"),
        }
        // Memoized: deleting the file changes nothing on a second request.
        std::fs::remove_file(temp.0.join("big.md")).unwrap();
        assert!(matches!(
            snap.capture(&temp.0, &confined("big.md")).unwrap(),
            Captured::TooLarge { .. }
        ));
    }

    // Sealing makes any later capture an engine error, never a silent
    // filesystem reopen.
    #[test]
    fn capture_after_seal_is_an_engine_error() {
        let temp = TempDir::new("sealed");
        std::fs::write(temp.0.join("a.md"), "x").unwrap();
        let mut snap = Snapshot::new(64, 4096);
        snap.capture(&temp.0, &confined("a.md")).unwrap();
        snap.seal();
        // Already-captured paths stay readable through `get`/`text`…
        assert_eq!(snap.text(Path::new("a.md")), Some("x"));
        // …but a NEW capture after the seam must fail as an engine defect.
        // (Guarded by debug_assert in debug builds; exercise the release-mode
        // contract via the returned error when assertions are disabled — here
        // we assert the debug panic instead.)
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            snap.capture(&temp.0, &confined("b.md")).is_err()
        }));
        match attempt {
            // Debug build: the assert fires — the defect is loud.
            Err(_) => {}
            // Release build (debug_assertions off): the Err path is the seal.
            Ok(errored) => assert!(errored, "post-seal capture must error"),
        }
    }

    // The aggregate budget counts EVERY captured input, whatever its role.
    #[test]
    fn aggregate_budget_counts_all_captures() {
        let temp = TempDir::new("aggregate");
        std::fs::write(temp.0.join("a.md"), "aaaaaa").unwrap();
        std::fs::write(temp.0.join("b.md"), "bbbbbb").unwrap();
        let mut snap = Snapshot::new(64, 10);
        snap.capture(&temp.0, &confined("a.md")).unwrap();
        let err = snap.capture(&temp.0, &confined("b.md")).unwrap_err();
        match err {
            VerifyError::BoundExceeded { ref bound, .. } => {
                assert_eq!(bound, "aggregate-snapshot-size")
            }
            other => panic!("expected the aggregate bound; got {other:?}"),
        }
    }

    // A non-UTF8 file is a valid capture with byte-level content: pin hashing
    // consumes the bytes; text consumers get None.
    #[test]
    fn non_utf8_capture_is_raw_content() {
        let temp = TempDir::new("raw");
        std::fs::write(temp.0.join("bin.md"), [0xFF, 0xFE, b'x']).unwrap();
        let mut snap = Snapshot::new(64, 4096);
        match snap.capture(&temp.0, &confined("bin.md")).unwrap() {
            Captured::Content(c) => {
                assert_eq!(c.text(), None);
                assert_eq!(c.bytes(), &[0xFF, 0xFE, b'x']);
            }
            other => panic!("expected raw content; got {other:?}"),
        }
    }

    // An absent target is a recorded state, not an error — the consuming check
    // maps it to its own finding taxonomy.
    #[test]
    fn absent_target_is_open_io_state() {
        let temp = TempDir::new("absent");
        let mut snap = Snapshot::new(64, 4096);
        match snap.capture(&temp.0, &confined("missing.md")).unwrap() {
            Captured::OpenIo { .. } => {}
            other => panic!("expected OpenIo; got {other:?}"),
        }
    }
}
