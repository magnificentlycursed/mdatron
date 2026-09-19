//! Run-level memoization of cross-file reference-target parsing (GH #48
//! finding 8, crosslink #173).
//!
//! Before this module, `marker::resolve_members` re-parsed a rule's
//! `target_doc` (frontmatter strip + member extraction) for EVERY governed file
//! the rule applied to — a shared target referenced from N files cost
//! O(N × target-size) inside the hook-time budget `docs/limits.md` bounds — and
//! re-emitted the same rule-level findings (target_doc confinement
//! `E0010`/`E0011`/`E0012`, `E0114` target-section-not-found, the `E0080`
//! never-captured defect) once per governed file. The link family likewise
//! re-parsed a target's heading-slug set per referring file (its cache was
//! local to one `check_file` call).
//!
//! One [`RefMemo`] is created per `run()` invocation and threaded `&mut`
//! through `verify_file` into `marker::check_file` and `link::check_file`, so:
//!
//! - a marker `target_doc` is parsed ONCE per run per rule key, and the
//!   rule-level findings above are emitted once per run per key — located at
//!   the first governed file the walk encounters for the rule (the walk is
//!   deterministic, so the location is too). Per-LINE findings (`E0112`,
//!   including the captured-nothing shape) are NEVER deduped.
//! - a link target's anchor/slug set is computed once per RUN instead of once
//!   per referring file. Per-reference findings (`E0110`/`E0111`/`W0048`) stay
//!   per-reference; only the parsed slug set is cached.
//!
//! The memo lives and dies with one `run()` call (incremental runs included),
//! so there is no cross-run state; everything it caches derives from the run's
//! immutable snapshot (#103), which cannot change under it.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::route::{ElementClass, MarkerRule};

/// The resolution identity of a marker rule: two rules with the same key
/// resolve the same member set (the pattern plays no part in resolution), so
/// they share one memo entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MarkerKey {
    pub target_doc: String,
    pub target_section: Option<String>,
    pub element: ElementClass,
}

impl MarkerKey {
    /// The memo key of one marker rule.
    pub fn of(rule: &MarkerRule) -> Self {
        Self {
            target_doc: rule.target_doc.clone(),
            target_section: rule.target_section.clone(),
            element: rule.element,
        }
    }
}

/// The resolution outcome of one marker rule key (GH #48 lane G): the member
/// set when the target parsed, or one of two non-set states the per-line scan
/// maps onto its own findings.
pub enum MarkerMembers {
    /// The target parsed; references resolve against this set.
    Resolved(HashSet<String>),
    /// The rule is disabled — its target_doc failed confinement, its
    /// target_section heading is absent, or the target was never captured. A
    /// rule-level finding was emitted; the rule's lines are skipped.
    Disabled,
    /// The target document is PRESENT but unverifiable (non-UTF8 content, or
    /// opened-but-unreadable): the check cannot run, which is not the same as
    /// "the reference resolves to nothing" — each matching line reports
    /// `W0048` (reference-target-unverified), never a false-dead `E0112`.
    Unverifiable,
}

/// The per-run reference-target memo. One instance per `run()` invocation,
/// created before the per-file walk and threaded `&mut` through it.
#[derive(Default)]
pub struct RefMemo {
    /// Marker member states by rule key. Key PRESENCE doubles as the
    /// reported-marker: the rule-level findings are emitted exactly when the
    /// entry is first inserted (a memo miss), so a present key means they were
    /// already reported this run. Per-line findings (E0112, and the W0048 of
    /// the Unverifiable state) are never deduped by the memo.
    pub marker_members: HashMap<MarkerKey, MarkerMembers>,
    /// Link anchor slug sets by resolved root-relative target path. `None` =
    /// the target exists but is not anchor-checkable (non-UTF8), so fragments
    /// into it are not resolved — the same value semantics the per-file cache
    /// had; only its lifetime widened to the run.
    pub link_slugs: HashMap<PathBuf, Option<HashSet<String>>>,
    /// Test probe: how many marker target resolutions were actually performed
    /// (memo misses). Lets a test assert a shared target parses once for two
    /// referring files.
    #[cfg(test)]
    pub marker_resolves: usize,
}
