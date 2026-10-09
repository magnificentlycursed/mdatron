//! `mdatron init`: deploy the `.mdatron/` skeleton and its init manifest (the
//! record of the engine-managed partition), idempotently, refusing drifted
//! managed files.
//!
//! Per `DESIGN.md` § Requirements (Init): the skeleton is the schema and pattern directories,
//! a seeded engine-default config, and the init manifest defining the managed
//! partition. Managed files — listed in the manifest with sha256 content
//! hashes — are drift-refused; the manifest is the authority on WHAT is
//! managed (its entries are honored as data). Seeded files (config.yaml, #77)
//! are written once and adopter-owned from then on — never hashed, never
//! overwritten; their demotion from the managed partition persists as
//! tombstones in trees that had them managed. Adopter-authored data (schemas
//! and patterns) lives outside the manifest and is never touched. The manifest
//! cannot hash itself (a fixed point, `DESIGN.md` § Validation is data-driven, governance data is
//! governed); its own integrity is anchored by commit review.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::confine::confine_lexically;
use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};

/// The engine-default config, **seeded** at init and adopter-owned from then
/// on. Its `file_globs` are the consumer-authored jurisdiction the verify
/// pipeline honors (#77) — which is exactly why it cannot be drift-guarded:
/// the original guard (placed "until config consumption lands") would make
/// customization impossible. Its demotion from the managed partition is
/// recorded as a tombstone in trees that had it managed (DESIGN § Validation is data-driven,
/// governance data is governed).
const DEFAULT_CONFIG: &str = "\
# mdatron configuration — seeded by `mdatron init`; adopter-owned.
# file_globs declare what is in mdatron's jurisdiction; files outside them
# are not walked. Adopter schemas live in .mdatron/schemas/, patterns in
# .mdatron/patterns/.
file_globs:
  - \"**/*.md\"
";

/// Files the engine seeds at init, relative to `.mdatron/`: written when
/// absent, never overwritten, never hashed — adopter-owned once deployed.
const SEED_FILES: &[(&str, &str)] = &[("config.yaml", DEFAULT_CONFIG)];

/// Inert templates the engine deploys at init as MANAGED files (#203 F4, GH #56
/// finding 4): one per family input file, fully commented, each header stating
/// the activation rule, scope, keys, and codes. The `.example` extension is
/// what keeps a family from activating by accident — the loaders read the real
/// names only. Managed (hashed, drift-refused) because they are engine prose to
/// copy from, not to edit; the copy is the adopter's.
pub const TEMPLATE_FILES: &[(&str, &str)] = &[
    ("routes.yaml.example", ROUTES_TEMPLATE),
    ("pins.yaml.example", PINS_TEMPLATE),
    ("vocabulary.yaml.example", VOCABULARY_TEMPLATE),
    ("code-catalogs.yaml.example", CODE_CATALOGS_TEMPLATE),
    ("links.yaml.example", LINKS_TEMPLATE),
];

/// The sha256 of every template content an earlier mdatron shipped (#231),
/// labelled with the release that shipped it, or `unreleased` for content that
/// only ever lived on main. A refresh moves a template forward only from one of
/// these: a recorded hash this binary does not know was written by a NEWER (or
/// an unreleased) mdatron, or re-hashed by hand, and is left as recorded — so
/// developers on mixed versions no longer ping-pong the committed bytes. A
/// released version's bytes are kept in `tests/fixtures/init-templates/<version>/`
/// and a test holds its hashes to them. When a template changes, ALWAYS add the
/// hash it replaces (the `template_hashes_are_pinned` test says so); listing an
/// unreleased hash is harmless. A template must never change back to content
/// listed here, or the two versions would refresh each other again.
const RELEASED_TEMPLATE_HASHES: &[(&str, &str, &str)] = &[
    (
        "0.7.0",
        "routes.yaml.example",
        "3b9112768d61b61b358ce49ea279581691a663488826c359f67ca3dbf64dcdba",
    ),
    (
        "0.7.0",
        "pins.yaml.example",
        "d896b9c74b219041bea8670303654ad67db8b29e4736ffbde82d7bd25a3a5c6f",
    ),
    (
        "0.7.0",
        "vocabulary.yaml.example",
        "c7d0a0d3f13018322f8b8b585c3b123c1f87f85fd3f1f0d26a2ef93571f93b25",
    ),
    (
        "0.7.0",
        "code-catalogs.yaml.example",
        "b4083b0aa957019cf7397747032deb38292aab8384e68923ecf6d043bab5be6f",
    ),
    // The routes template as main carried it from #215 to #234 (the every-rule
    // vacuity note), never in a release.
    (
        "unreleased",
        "routes.yaml.example",
        "e4d23d6eaa46e7a9b13766ba8e1dcedf925926bfe0ccece0f3721b468b617d4e",
    ),
];

/// Whether `sha256` is a released, superseded version of template `path`.
fn is_released_template(path: &str, sha256: &str) -> bool {
    RELEASED_TEMPLATE_HASHES
        .iter()
        .any(|(_, p, h)| *p == path && *h == sha256)
}

/// The line that separates a template's header from its (commented) example
/// body; a test strips the comment marker from the lines after it and runs the
/// body through the real loader, so the documented shape is executable. Spelled
/// without a `---` run (round-2 N1): an adopter who blanket-uncomments the
/// block must not manufacture a YAML document separator.
pub const TEMPLATE_BODY_MARKER: &str = "# (example: uncomment the lines below)";

const ROUTES_TEMPLATE: &str = r####"# routes.yaml.example — the route family. Copy to routes.yaml to activate.
#
# ACTIVATION  the family is supplied the moment .mdatron/routes.yaml EXISTS,
#             even with `routes: []`; from then on every walked file must be
#             claimed by exactly one route (E0030 unclaimed, E0032 claimed
#             twice, W0053 empty table, W0054 a route claiming nothing).
#             Delete the file to deactivate.
# SCOPE       every walked file (config.yaml file_globs); a route's `files`
#             glob is root-relative; `*` stays within one path segment,
#             `**` crosses any depth.
# GATEWAY     citations, links, marker_rules, and section_rules exist only
#             on a route.
# KEYS        per route — required: files, governed_by
#                         optional: naming, citations, links, link_root,
#                                   marker_rules, section_rules, schema,
#                                   name_equals_dir, max_bytes, imports,
#                                   requires_sibling, link_policy
#             file-level  — optional: mdatron_format_version (absent = 1)
# CODES       E0030 E0031 E0032 W0041 W0053 W0054; per opt-in E0100 E0101
#             W0048 E0081 (citations), E0110 E0111 E0115 E0116 E0117 W0048
#             E0081 (links), E0118 (link_policy), E0112 E0114 W0048 E0081
#             (markers), E0120 E0121 E0122 E0123 E0124 (section rules),
#             E0033 E0034 (schema), E0035 (name_equals_dir), E0036
#             (max_bytes), E0037 (requires_sibling);
#             E0010 E0011 E0012 on paths.
# The body below is the minimal activating shape: one route claiming every
# walked file, answering to a document that must exist (point governed_by at
# yours). The optional additions each opt a family in — and section_rules
# assert structure on EVERY file the route claims, so add them to a route
# whose files all carry that structure.
# (example: uncomment the lines below)
# mdatron_format_version: 1
# routes:
# - files: "**/*.md"
#   governed_by: README.md
# (optional additions: uncomment what you need)
#   schema: skill          # bind claimed files to .mdatron/schemas/skill.json
#   name_equals_dir: name  # frontmatter `name` must equal the parent directory
#   max_bytes: 32768       # the most bytes a claimed file may hold
#   requires_sibling: CLAUDE.md  # must exist beside every claimed file
#   naming: "^[a-z0-9-]+\\.md$"
#   citations: true
#   links: true
#   link_root: false
#   imports: true          # resolve @path imports like relative links
#   link_policy:           # what an absolute URL may be (needs links)
#     schemes: [https]
#     hosts: [docs.example.com, "*.example.com"]
#     forbid_query: ["^utm_"]
#   marker_rules:
#   - pattern: "^Provenance: (.+)$"
#     element: list-item-bold-name
#     target_doc: DESIGN.md
#     target_section: "## Decisions"
#   section_rules:
#   - section: "## Requirements"
#     element: h3
#     match: "^### "
#     count: ">= 1"
#   - section: "## Requirements"
#     element: h3
#     match: "^Slice [0-9]+"
#     match_on: name   # test the heading TEXT, not the line (default: line)
#     count: ">= 1"
#   - element: line    # no `section`: the whole document (here: not empty)
#     match: "."
#     count: ">= 1"
#   - section: "## Links"
#     element: list-item
#     every: "^- \\[[^\\]]+\\]\\([^)]+\\)"   # every item is a link
#                      # (a section with NO list item passes: pair it with a
#                      # count rule when at least one item must exist)
#   - section: "## Requirements"
#     order:           # elements matching an earlier item come first
#     - element: blockquote
#       match: "."
#     - element: h3
#       match: "."
#   - disjoint:
#     - section: "## Requirements"
#       element: h3
#       id_pattern: 'Slice (\d+)'
#     - section: "## Decisions"
#       element: list-item-bold-name
#       id_pattern: 'Slice (\d+)'
"####;

const PINS_TEMPLATE: &str = r####"# pins.yaml.example — the pin family. Copy to pins.yaml to activate.
#
# ACTIVATION  the file exists. Each pin attests a governed file's sha256 (or
#             one heading's span) on behalf of a governing document; a stale
#             hash blocks (E0061) until `mdatron pin --update` re-pins after
#             the governing document is re-read. `unpinned:` tombstones are
#             the standing record of a removed pin (L0001 every whole-tree
#             run; W0042 when reason or owner is missing).
# SCOPE       the pinned files themselves — any file inside the project
#             root, walked or not.
# KEYS        per pin      — required: governed_by, file, sha256
#                            optional: section
#             per unpinned — required: file, governed_by, reason, owner
#             file-level   — optional: mdatron_format_version (absent = 1;
#                            `pin --update` stamps it)
# CODES       E0061 E0062 E0063 E0081 L0001 W0042; E0010 E0011 E0012 on paths.
# (example: uncomment the lines below)
# mdatron_format_version: 1
# pins:
# - governed_by: DESIGN.md
#   file: src/lib.rs
#   sha256: "0000000000000000000000000000000000000000000000000000000000000000"
# - governed_by: DESIGN.md
#   file: docs/plan.md
#   section: "## Decomposition"
#   sha256: "0000000000000000000000000000000000000000000000000000000000000000"
# unpinned:
# - file: docs/old-plan.md
#   governed_by: DESIGN.md
#   reason: "superseded by docs/plan.md"
#   owner: operator
"####;

const VOCABULARY_TEMPLATE: &str = r####"# vocabulary.yaml.example — the vocabulary family (the naming register).
# Copy to vocabulary.yaml to activate.
#
# ACTIVATION  the file exists.
# SCOPE       every walked file, or config.yaml vocabulary_globs when set
#             (W0043 if that scope matches nothing); the bold-means-coinage
#             check (E0090) is further narrowed by coinage_globs when set.
# KEYS        all optional — terms[] {term, status, sense} with status one of
#             registered | draft | reserved; coinage_globs[]; label_schemes
#             {allow[]}; anti_patterns[] {pattern, guidance}; numeric_claims[]
#             {field}; mdatron_format_version (absent = 1).
# CODES       E0090 E0091 E0092 E0093 E0094 W0043 W0044.
# The body below activates cleanly on any tree; the optional addition scopes
# the coinage check to files that must exist in YOUR tree (a scope matching
# nothing is W0043).
# (example: uncomment the lines below)
# mdatron_format_version: 1
# terms:
# - term: "jurisdiction"
#   status: registered
#   sense: "the file set the declared file_globs walk"
# - term: "spend shape"
#   status: draft
#   sense: "how a review round's agent budget is declared"
# label_schemes:
#   allow:
#   - "^ADR-[0-9]+$"
# anti_patterns:
# - pattern: "very unique"
#   guidance: "say 'unique' — uniqueness does not grade"
# numeric_claims:
# - field: items
# (optional additions: uncomment what you need)
# coinage_globs:
# - "docs/spec/**/*.md"
"####;

const CODE_CATALOGS_TEMPLATE: &str = r####"# code-catalogs.yaml.example — the code-catalog family. Copy to
# code-catalogs.yaml to activate.
#
# ACTIVATION  the file exists.
# SCOPE       every walked file, or config.yaml code_catalog_globs when set
#             (W0055 if that scope matches nothing). Inline code spans ARE
#             scanned (a backticked code is a citation); fenced blocks are
#             examples and are not.
# KEYS        per catalog — required: namespace, codes[]
#                           optional: comprehensive (default false; only a
#                                     comprehensive catalog can orphan a
#                                     token)
#             file-level  — required: mdatron_format_version
# CODES       E0113 W0055.
# (example: uncomment the lines below)
# mdatron_format_version: 1
# catalogs:
# - namespace: "ADOPTER-"
#   comprehensive: true
#   codes: ["E0001", "W0100"]
"####;

const LINKS_TEMPLATE: &str = r####"# links.yaml.example — the external-link register. Copy to links.yaml to
# activate.
#
# ACTIVATION  the file exists; it applies to the files a route opts in with
#             links: true (W0056 if none is walked, on a whole-tree run
#             from config.yaml).
# SCOPE       every markdown link (inline, reference, image, autolink) to an
#             absolute URL (a scheme, or //host) in a link-checked file must
#             match an entry — exactly, or by prefix when the entry sets
#             prefix: true (a scheme:// or //host prefix runs past its host)
#             — else E0115; its #fragment must be one the entry lists under
#             fragments when the entry has a fragments key (an empty list
#             accepts none; E0116). An entry no link uses is W0057 (whole-tree
#             runs from config.yaml only). A bare
#             URL in prose or a raw HTML <a href> is not a link. mdatron
#             never fetches a URL: the
#             register is the list a liveness tool checks on a schedule, and
#             `mdatron links --external` exports what the corpus links to.
# KEYS        per entry — required: url
#                         optional: prefix (default false), fragments[]
#             file-level  — required: mdatron_format_version
# CODES       E0115 E0116 W0056 W0057.
# (example: uncomment the lines below)
# mdatron_format_version: 1
# links:
# - url: https://docs.example.com/
#   prefix: true
# - url: https://example.com/reference
#   fragments: [installation, usage]
"####;

/// Engine-known content for manifest-listed managed paths, used to repair a
/// missing managed file in trees whose manifest still lists it (v1 trees
/// predating the config.yaml demotion). The manifest is the authority on WHAT
/// is managed; this is only the authority on what the engine can redeploy.
fn engine_content(path: &str) -> Option<&'static str> {
    match path {
        "config.yaml" => Some(DEFAULT_CONFIG),
        other => TEMPLATE_FILES
            .iter()
            .find(|(name, _)| *name == other)
            .map(|(_, content)| *content),
    }
}

/// The shipped content of a managed TEMPLATE (`*.example`), if `path` is one —
/// the only managed files a refresh may rewrite (#207).
fn template_content(path: &str) -> Option<&'static str> {
    TEMPLATE_FILES
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, content)| *content)
}

/// Directories the skeleton creates under `.mdatron/`.
const SKELETON_DIRS: &[&str] = &["schemas", "patterns"];

const MANIFEST_NAME: &str = "manifest.yaml";
/// Manifest shape version. v2 adds demotion tombstones and empties the default
/// managed set (config.yaml demoted to a seed, #77); v1 manifests — which may
/// still list config.yaml as managed — parse and are honored as data.
const MANIFEST_VERSION: u32 = 2;

/// Outcome of a successful init run.
#[derive(Debug, PartialEq, Eq)]
pub enum InitOutcome {
    /// First run, a repair of missing managed files, a template refresh, or a
    /// template kept as a newer version recorded it: paths created, templates
    /// rewritten to this version's content (a managed template still
    /// byte-identical to what was recorded, whose shipped content changed), and
    /// templates left alone because this version does not know their content.
    Deployed {
        created: Vec<String>,
        refreshed: Vec<String>,
        /// Unedited templates recorded with content this version does not
        /// know — a newer (or unreleased) mdatron's — left as recorded, never
        /// moved backwards (#231).
        newer: Vec<String>,
    },
    /// Re-run on an intact, unmodified tree: nothing to do.
    AlreadyInitialized,
}

/// A managed-file drift: a manifest-listed file whose content no longer matches
/// its recorded hash. Surfaced to the CLI as an `MDATRON-E0060` diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    pub file: String,
    pub expected_sha256: String,
    pub actual_sha256: String,
}

/// Errors from an init run. `Drift` is a governance refusal (a managed file was
/// hand-modified); the others are IO/parse failures.
#[derive(Debug)]
pub enum InitError {
    Io {
        path: String,
        error: String,
    },
    ManifestParse {
        path: String,
        error: String,
    },
    Drift(Vec<Drift>),
    /// A template path already exists with content the engine did not write
    /// (#203 F4, round-2 m11): refused rather than hashed as the engine's own
    /// (which would refuse the NEXT init as drift the adopter never caused) or
    /// overwritten.
    TemplateCollision {
        path: String,
    },
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InitError::Io { path, error } => write!(f, "init IO at '{path}': {error}"),
            InitError::ManifestParse { path, error } => {
                write!(f, "init manifest parse at '{path}': {error}")
            }
            InitError::Drift(drifts) => {
                write!(
                    f,
                    "{} managed file(s) drifted from the init manifest",
                    drifts.len()
                )
            }
            InitError::TemplateCollision { path } => write!(
                f,
                "'{path}' already exists with content the engine did not write; move it \
                 aside (the engine deploys its own template there), then re-run init"
            ),
        }
    }
}

impl std::error::Error for InitError {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    /// Engine-deployed files (path relative to `.mdatron/`) with their sha256.
    /// The manifest never lists itself (fixed point). The manifest DEFINES the
    /// managed partition (DESIGN § Validation is data-driven, governance data is governed): drift checks
    /// run over these entries as data, not over an engine-side list.
    managed: Vec<ManagedEntry>,
    /// Demotion tombstones: standing records of entries removed from the
    /// managed partition, with their justification (DESIGN: removals persist
    /// as tombstones; a naive demotion trips drift, a tombstoned one stays
    /// loud through this record; the terminal anchor is commit review).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    demoted: Vec<DemotionTombstone>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagedEntry {
    path: String,
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DemotionTombstone {
    /// The demoted file, relative to `.mdatron/` (the manifest's own base —
    /// unlike a pins.yaml tombstone's root-relative `file`).
    path: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    owner: String,
}

/// The init manifest as `verify` reads it: its managed-file drift (`E0060`,
/// #230) and its standing demotion tombstones rendered as findings, plus the
/// digest of the bytes read (input lineage).
pub struct LoadedManifest {
    pub findings: Vec<Finding>,
    pub digest: String,
}

/// Render the manifest's demotion tombstones as the standing governance-
/// weakening findings — the second carrier of `MDATRON-L0001` beside
/// `pins.yaml`'s `unpinned[]` (DESIGN § Validation is data-driven, governance data is governed: "a
/// tombstoned demotion stays loud through its standing annotation"; #204 R3 —
/// through 0.6.0 only the pins carrier emitted the lint, so the DESIGN
/// criterion was unmet). A tombstone without its justification is
/// `MDATRON-W0042`, exactly as for an unpinned entry. An absent manifest is
/// `None` (a tree that never ran `init`); a manifest that does not parse is a
/// config error — the file is engine-managed, so a parse failure is a defect,
/// never something to skip silently.
///
/// Drift (#230, DESIGN § Validation is data-driven, governance data is
/// governed: "drift in them is refused"): every managed file whose bytes no
/// longer hash to the recorded sha256 is `MDATRON-E0060`, decided by the same
/// [`entry_state`] `init` refuses on — a snapshot shows it, so the gate
/// reports it rather than leaving it to an `init`-then-diff recipe. A missing
/// managed file is not drift (`init` repairs it).
pub fn load_manifest(project_root: &Path) -> Result<Option<LoadedManifest>, crate::Error> {
    let path = project_root.join(".mdatron").join(MANIFEST_NAME);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(crate::Error::Config(format!(
                "cannot read '{}': {e}",
                path.display()
            )))
        }
    };
    let manifest: Manifest = crate::yaml::from_str(&content)
        .map_err(|e| crate::Error::Config(format!("cannot parse '{}': {e}", path.display())))?;
    let dir = project_root.join(".mdatron");
    let mut drifts = Vec::new();
    let mut unverifiable = Vec::new();
    for entry in &manifest.managed {
        match entry_state(&dir, &path, entry) {
            Ok(EntryState::Drifted(d)) => drifts.push(d),
            Ok(EntryState::Intact | EntryState::Missing) => {}
            // One managed file that cannot be checked must not deny the rest
            // of the run (#103 posture): a per-file finding at that file.
            Ok(EntryState::Unverifiable(why)) => {
                unverifiable.push(unverifiable_managed_finding(&dir, &entry.path, &why))
            }
            Err(e) => return Err(crate::Error::Config(e.to_string())),
        }
    }
    let mut findings = drift_findings(project_root, &drifts);
    findings.append(&mut unverifiable);
    for t in &manifest.demoted {
        // The same confinement the managed[] half of this file gets (round-2
        // m13): a demoted path escaping .mdatron/ is a manifest-integrity
        // failure, refused — never rendered as if it named a governed file.
        if let Err(v) = confine_lexically(Path::new(&t.path)) {
            let why = match v {
                crate::confine::LexicalViolation::Absolute => "is an absolute path",
                crate::confine::LexicalViolation::ParentSegment => {
                    "climbs above .mdatron/ with `..`"
                }
            };
            return Err(crate::Error::Config(format!(
                "cannot use '{}': the demoted path '{}' {why}; a tombstone names a \
                 file inside .mdatron/",
                path.display(),
                t.path
            )));
        }
        let file = QuotedRegion {
            platform_variant: false,
            label: "file".into(),
            content: format!(".mdatron/{}", t.path),
        };
        if t.reason.trim().is_empty() || t.owner.trim().is_empty() {
            findings.push(Finding {
                code: "MDATRON-W0042".into(),
                severity: Severity::Warning,
                summary: "governance-weakening-unjustified".into(),
                message: "a demotion tombstone carries no justification (reason and \
                          owner are required); a weakening that cannot say why it \
                          stands is not a tombstone, it is an erasure"
                    .into(),
                help: Some("add reason and owner to the demoted entry".into()),
                location: Location::whole_file(&path),
                explain_ref: Some("MDATRON-W0042".into()),
                quoted: vec![file],
            });
        } else {
            findings.push(Finding {
                code: "MDATRON-L0001".into(),
                severity: Severity::Lint,
                summary: "governance-weakening-standing".into(),
                message: "a file was demoted from the managed partition; the \
                          tombstone below is the standing record of that weakening"
                    .into(),
                help: None,
                location: Location::whole_file(&path),
                explain_ref: Some("MDATRON-L0001".into()),
                quoted: vec![
                    file,
                    QuotedRegion {
                        platform_variant: false,
                        label: "reason".into(),
                        content: t.reason.clone(),
                    },
                    QuotedRegion {
                        platform_variant: false,
                        label: "owner".into(),
                        content: t.owner.clone(),
                    },
                ],
            });
        }
    }
    Ok(Some(LoadedManifest {
        findings,
        digest: sha256_hex(content.as_bytes()),
    }))
}

/// Lowercase hex sha256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Run `mdatron init` at `project_root`.
///
/// - No manifest present → first run: deploy the skeleton, seed files, and a
///   fresh (v2, empty-managed) manifest.
/// - Manifest present, every manifest-listed managed file intact and every
///   seed present → no-op ([`InitOutcome::AlreadyInitialized`]).
/// - A manifest-listed managed file hand-modified (hash mismatch) → refuse
///   ([`InitError::Drift`]); nothing is written. Seeds are adopter-owned:
///   an edited seed is neither refused nor overwritten.
/// - A manifest-listed managed file missing → repaired from engine-known
///   content; the manifest is rewritten preserving entries and tombstones.
///   A missing seed is re-seeded.
/// - A managed TEMPLATE still byte-identical to its recorded hash (nobody
///   edited it) whose shipped content changed in this version → refreshed to
///   this version's content and re-hashed, so an upgrade never leaves an
///   adopter reading a stale example (#207). An edited template is drift and
///   refused as above — the refresh never overwrites a hand change. Only the
///   `*.example` templates refresh: a v1 manifest's managed `config.yaml` is
///   adopter configuration in substance and is never rewritten.
pub fn init(project_root: &Path) -> Result<InitOutcome, InitError> {
    let dir = project_root.join(".mdatron");
    let manifest_path = dir.join(MANIFEST_NAME);

    if manifest_path.exists() {
        let mut manifest = read_manifest(&manifest_path)?;
        let mut drifts = Vec::new();
        let mut missing: Vec<usize> = Vec::new();
        let mut stale: Vec<usize> = Vec::new();
        let mut newer: Vec<String> = Vec::new();
        for (i, entry) in manifest.managed.iter().enumerate() {
            match entry_state(&dir, &manifest_path, entry)? {
                EntryState::Drifted(d) => drifts.push(d),
                EntryState::Missing => missing.push(i),
                EntryState::Unverifiable(why) => {
                    return Err(InitError::Io {
                        path: format!(".mdatron/{}", entry.path),
                        error: format!(
                            "the managed file cannot be checked ({}); init does not \
                             write through or over it",
                            why.describe()
                        ),
                    });
                }
                EntryState::Intact => {
                    // Untouched since recorded, but this version ships
                    // different content: refresh below (never on drift) —
                    // forward only (#231): from a released older version,
                    // never over content this binary does not know.
                    if let Some(content) = template_content(&entry.path) {
                        if sha256_hex(content.as_bytes()) != entry.sha256 {
                            if is_released_template(&entry.path, &entry.sha256) {
                                stale.push(i);
                            } else {
                                newer.push(format!(".mdatron/{}", entry.path));
                            }
                        }
                    }
                }
            }
        }
        // Drift is a governance refusal — never write over a hand-modified
        // managed file. Missing managed files are repaired below.
        if !drifts.is_empty() {
            return Err(InitError::Drift(drifts));
        }

        let mut created = Vec::new();
        let mut refreshed = Vec::new();

        // Refresh untouched templates whose shipped content changed (#207).
        // Reached only when nothing drifted, so no hand change is overwritten.
        if !stale.is_empty() {
            for &i in &stale {
                let entry = &mut manifest.managed[i];
                let Some(content) = template_content(&entry.path) else {
                    continue;
                };
                let p = dir.join(&entry.path);
                crate::atomic::write(&p, content.as_bytes()).map_err(|e| io_err(&p, &e))?;
                entry.sha256 = sha256_hex(content.as_bytes());
                refreshed.push(format!(".mdatron/{}", entry.path));
            }
            write_manifest(&dir, &manifest)?;
        }

        // Repair missing MANAGED files from engine-known content. The manifest
        // is the authority on what is managed (its entries are data — v1 trees
        // still listing config.yaml are honored); the engine only supplies the
        // bytes. The manifest is rewritten preserving its entries and
        // tombstones, with repaired entries re-hashed from the written content.
        if !missing.is_empty() {
            for &i in &missing {
                let entry = &mut manifest.managed[i];
                let Some(content) = engine_content(&entry.path) else {
                    return Err(InitError::Io {
                        path: entry.path.clone(),
                        error: "missing managed file has no engine-known content to redeploy"
                            .into(),
                    });
                };
                let p = dir.join(&entry.path);
                crate::atomic::write(&p, content.as_bytes()).map_err(|e| io_err(&p, &e))?;
                entry.sha256 = sha256_hex(content.as_bytes());
                created.push(format!(".mdatron/{}", entry.path));
            }
            write_manifest(&dir, &manifest)?;
        }

        // Skeleton dirs and SEED files are re-created when absent — idempotent
        // repair without touching anything that exists (seeds are
        // adopter-owned; an edited seed is never overwritten, never refused).
        ensure_dirs(&dir)?;
        for (name, content) in SEED_FILES {
            let p = dir.join(name);
            if !p.exists() {
                crate::atomic::write(&p, content.as_bytes()).map_err(|e| io_err(&p, &e))?;
                created.push(format!(".mdatron/{name}"));
            }
        }

        // Templates (#203 F4, round-2 M5): a manifest that predates them gains
        // them here — the same idempotent repair as a missing managed file, so
        // a tree initialized by 0.6.0 receives them on its next init.
        if deploy_templates(&dir, &mut manifest.managed, &mut created)? {
            write_manifest(&dir, &manifest)?;
        }

        return if created.is_empty() && refreshed.is_empty() && newer.is_empty() {
            Ok(InitOutcome::AlreadyInitialized)
        } else {
            Ok(InitOutcome::Deployed {
                created,
                refreshed,
                newer,
            })
        };
    }

    // First run: deploy the skeleton, seeds, and a fresh (v2) manifest.
    let created = deploy(&dir)?;
    Ok(InitOutcome::Deployed {
        created,
        refreshed: Vec::new(),
        newer: Vec::new(),
    })
}

/// Deploy every template not yet in the manifest: write it when absent and
/// list it with its hash. A template that already exists with the engine's
/// own content is simply adopted; one that exists with FOREIGN content is a
/// collision, refused by name. Returns whether the manifest changed.
fn deploy_templates(
    dir: &Path,
    managed: &mut Vec<ManagedEntry>,
    created: &mut Vec<String>,
) -> Result<bool, InitError> {
    let mut changed = false;
    for (name, content) in TEMPLATE_FILES {
        let p = dir.join(name);
        let listed = managed.iter().any(|e| e.path == *name);
        match std::fs::read(&p) {
            Ok(bytes) if !listed && bytes != content.as_bytes() => {
                return Err(InitError::TemplateCollision {
                    path: format!(".mdatron/{name}"),
                });
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                crate::atomic::write(&p, content.as_bytes()).map_err(|e| io_err(&p, &e))?;
                created.push(format!(".mdatron/{name}"));
            }
            Err(e) => return Err(io_err(&p, &e)),
        }
        if !listed {
            managed.push(ManagedEntry {
                path: (*name).to_string(),
                sha256: sha256_hex(content.as_bytes()),
            });
            changed = true;
        }
    }
    Ok(changed)
}

fn deploy(dir: &Path) -> Result<Vec<String>, InitError> {
    // A template collision is refused BEFORE anything is written (round-2 N2):
    // a refusal must not leave a half-scaffolded tree with no manifest.
    for (name, content) in TEMPLATE_FILES {
        let p = dir.join(name);
        match std::fs::read(&p) {
            Ok(bytes) if bytes != content.as_bytes() => {
                return Err(InitError::TemplateCollision {
                    path: format!(".mdatron/{name}"),
                });
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(&p, &e)),
        }
    }
    let mut created = Vec::new();

    if !dir.exists() {
        std::fs::create_dir_all(dir).map_err(|e| io_err(dir, &e))?;
        created.push(".mdatron/".to_string());
    }
    for sub in SKELETON_DIRS {
        let p = dir.join(sub);
        if !p.exists() {
            std::fs::create_dir_all(&p).map_err(|e| io_err(&p, &e))?;
            created.push(format!(".mdatron/{sub}/"));
        }
    }

    // Seeds: written on first run; adopter-owned from then on (never hashed,
    // never overwritten by later runs).
    for (name, content) in SEED_FILES {
        let p = dir.join(name);
        if !p.exists() {
            crate::atomic::write(&p, content.as_bytes()).map_err(|e| io_err(&p, &e))?;
            created.push(format!(".mdatron/{name}"));
        }
    }

    // Templates (#203 F4): deployed as managed files and listed with their
    // hashes — config.yaml stays a seed (#77), and the manifest still DEFINES
    // the partition (v1 trees' existing entries are honored as data).
    let mut managed = Vec::new();
    deploy_templates(dir, &mut managed, &mut created)?;
    let manifest = Manifest {
        version: MANIFEST_VERSION,
        managed,
        demoted: Vec::new(),
    };
    let existed = dir.join(MANIFEST_NAME).exists();
    write_manifest(dir, &manifest)?;
    if !existed {
        created.push(format!(".mdatron/{MANIFEST_NAME}"));
    }

    Ok(created)
}

/// Serialize + write the manifest with its do-not-edit header, preserving
/// whatever entries and tombstones the given manifest carries.
fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<(), InitError> {
    let manifest_path = dir.join(MANIFEST_NAME);
    let yaml = serde_yaml_ng::to_string(manifest).map_err(|e| InitError::Io {
        path: manifest_path.to_string_lossy().into_owned(),
        error: e.to_string(),
    })?;
    let body = format!(
        "# mdatron init manifest (the engine-managed partition) — do not edit by hand.\n{yaml}"
    );
    // Atomic write (#126 DEF8): the manifest is rewritten in place on repair
    // (tombstones, re-hashed redeploys); a torn write would corrupt the
    // managed-partition record.
    crate::atomic::write(&manifest_path, body.as_bytes()).map_err(|e| io_err(&manifest_path, &e))
}

fn ensure_dirs(dir: &Path) -> Result<(), InitError> {
    for sub in SKELETON_DIRS {
        let p = dir.join(sub);
        if !p.exists() {
            std::fs::create_dir_all(&p).map_err(|e| io_err(&p, &e))?;
        }
    }
    Ok(())
}

/// One managed entry against the tree, decided once for `init` and `verify`
/// alike (#230): the same bytes against the same recorded hash, so the two
/// commands can never disagree about what drifted.
enum EntryState {
    /// The file's bytes hash to the recorded sha256.
    Intact,
    /// The file exists with other content: a hand change (`E0060`).
    Drifted(Drift),
    /// The file is absent (`init` repairs it; not drift).
    Missing,
    /// The file could not be checked (#230 review): a symlinked component,
    /// not a regular file (a FIFO would block the read), over the per-file
    /// input bound, or unreadable. Read no-follow, like every governed file.
    Unverifiable(Unverifiable),
}

/// Why a managed file could not be checked against its recorded hash.
#[derive(Debug)]
enum Unverifiable {
    Symlink(std::path::PathBuf),
    NotRegular,
    TooLarge,
    Io(String),
}

impl Unverifiable {
    fn describe(&self) -> String {
        match self {
            Unverifiable::Symlink(c) => format!("'{}' is a symlink", c.display()),
            Unverifiable::NotRegular => "not a regular file".into(),
            Unverifiable::TooLarge => "over the per-file input limit".into(),
            Unverifiable::Io(e) => e.clone(),
        }
    }
}

/// Decide `entry`'s state under `dir` (`.mdatron/`). The manifest is read from
/// the tree and cannot hash itself (fixed point), so a hand-edit to it is not
/// drift-caught; its managed paths are held to the same confinement contract
/// as any governed path (DESIGN.md § Nine check families): a path escaping
/// `.mdatron/` is a manifest-integrity failure — refused, nothing outside the
/// partition read.
fn entry_state(
    dir: &Path,
    manifest_path: &Path,
    entry: &ManagedEntry,
) -> Result<EntryState, InitError> {
    let confined =
        confine_lexically(Path::new(&entry.path)).map_err(|v| InitError::ManifestParse {
            path: manifest_path.to_string_lossy().into_owned(),
            error: format!(
                "managed path '{}' escapes the .mdatron/ partition ({v:?})",
                entry.path
            ),
        })?;
    // No-follow, regular-file-only, bounded — the confined open every governed
    // read uses (#230 review: a plain read followed a committed symlink out of
    // the partition, and a FIFO hung the run).
    let cap = crate::limits::SHIPPED.per_file_bytes;
    let handle = match crate::confine::open_confined(dir, &confined) {
        Ok(h) => h,
        Err(crate::confine::OpenViolation::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(EntryState::Missing)
        }
        Err(crate::confine::OpenViolation::Symlink { component, .. }) => {
            return Ok(EntryState::Unverifiable(Unverifiable::Symlink(component)))
        }
        Err(crate::confine::OpenViolation::NotRegular) => {
            return Ok(EntryState::Unverifiable(Unverifiable::NotRegular))
        }
        Err(crate::confine::OpenViolation::Io(e)) => {
            return Ok(EntryState::Unverifiable(Unverifiable::Io(e.to_string())))
        }
    };
    let mut bytes = Vec::new();
    if let Err(e) =
        std::io::Read::read_to_end(&mut std::io::Read::take(handle, cap as u64 + 1), &mut bytes)
    {
        return Ok(EntryState::Unverifiable(Unverifiable::Io(e.to_string())));
    }
    if bytes.len() > cap {
        return Ok(EntryState::Unverifiable(Unverifiable::TooLarge));
    }
    let actual = sha256_hex(&bytes);
    Ok(if actual == entry.sha256 {
        EntryState::Intact
    } else {
        EntryState::Drifted(Drift {
            file: entry.path.clone(),
            expected_sha256: entry.sha256.clone(),
            actual_sha256: actual,
        })
    })
}

fn read_manifest(path: &Path) -> Result<Manifest, InitError> {
    let content = std::fs::read_to_string(path).map_err(|e| io_err(path, &e))?;
    crate::yaml::from_str(&content).map_err(|e| InitError::ManifestParse {
        path: path.to_string_lossy().into_owned(),
        error: e.to_string(),
    })
}

fn io_err(path: &Path, e: &std::io::Error) -> InitError {
    InitError::Io {
        path: path.to_string_lossy().into_owned(),
        error: e.to_string(),
    }
}

/// A managed file `verify` could not check (#230 review): a symlinked
/// component is the confinement refusal `MDATRON-E0012`; anything else (not a
/// regular file, over the per-file bound, unreadable) is `MDATRON-E0003`.
fn unverifiable_managed_finding(dir: &Path, managed: &str, why: &Unverifiable) -> Finding {
    let (code, summary, message, help) = match why {
        Unverifiable::Symlink(_) => (
            "MDATRON-E0012",
            "symlinked-component-refused",
            "a managed file resolves through a symlink; no-follow resolution \
             refuses it, so its recorded hash cannot be checked",
            "replace the symlink with the file init deployed (delete it and \
             re-run init)",
        ),
        _ => (
            "MDATRON-E0003",
            "governed-file-unreadable",
            "this managed file's content cannot be read, so its recorded hash \
             cannot be checked",
            "restore the file init deployed (delete it and re-run init)",
        ),
    };
    Finding {
        code: code.into(),
        severity: Severity::Error,
        summary: summary.into(),
        message: message.into(),
        help: Some(help.into()),
        location: Location::whole_file(dir.join(managed)),
        explain_ref: Some(code.into()),
        quoted: vec![QuotedRegion {
            platform_variant: true,
            label: "cause".into(),
            content: why.describe(),
        }],
    }
}

/// Render a drift set as `MDATRON-E0060` findings (pin family, #50) for the CLI
/// diagnostic surface. One finding per drifted managed file.
pub fn drift_findings(project_root: &Path, drifts: &[Drift]) -> Vec<Finding> {
    drifts
        .iter()
        .map(|d| Finding {
            code: "MDATRON-E0060".into(),
            severity: Severity::Error,
            summary: "managed-manifest-drift".into(),
            // #165: the managed file path + recorded sha are adopter-influenced —
            // they ride in quoted regions, not inline in the engine message.
            message: "a managed file was modified after init; restore it or \
                      re-init in a clean tree"
                .into(),
            help: Some(
                "managed files are engine-owned; adopter data belongs in \
                 .mdatron/schemas/ or .mdatron/patterns/, outside the manifest"
                    .into(),
            ),
            location: Location {
                file: project_root.join(".mdatron").join(&d.file),
                line: 1,
                column: 0,
            },
            explain_ref: Some("MDATRON-E0060".into()),
            quoted: vec![
                QuotedRegion {
                    platform_variant: false,
                    label: "file".into(),
                    content: d.file.clone(),
                },
                QuotedRegion {
                    platform_variant: false,
                    label: "recorded".into(),
                    content: short(&d.expected_sha256).to_string(),
                },
                QuotedRegion {
                    platform_variant: false,
                    label: "found".into(),
                    content: short(&d.actual_sha256).to_string(),
                },
            ],
        })
        .collect()
}

/// Truncate a hash for display to at most 12 CHARS, char-boundary-safe (#165
/// review). Shared by init (E0060), pin (E0061), and the BIN crate's `pin`
/// subcommand renderer (GH #48 finding 4 — `pub`, not `pub(crate)`, precisely
/// so the binary's old/new hash lines cannot regress to a naive byte slice):
/// an adopter/manifest `sha256` field is not validated as hex, so a multibyte
/// char could straddle byte 12 and panic a naive byte slice. One tested copy
/// (`tests::short_truncates_char_boundary_safe`).
pub fn short(hash: &str) -> &str {
    match hash.char_indices().nth(12) {
        Some((idx, _)) => &hash[..idx],
        None => hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // #165 review: `short` truncates to at most 12 CHARS, char-boundary-safe — an
    // adopter/manifest sha field is not validated as hex, so a multibyte char
    // straddling byte 12 must not panic a naive byte slice.
    #[test]
    fn short_truncates_char_boundary_safe() {
        assert_eq!(short("0123456789abcdef"), "0123456789ab"); // 12 of 16 ascii
        assert_eq!(short("abc"), "abc"); // shorter than 12
                                         // 'é' is two bytes, straddling byte 12 → a naive &s[..12] would panic.
        assert_eq!(short("0123456789aé0"), "0123456789aé");
    }

    fn temp_root(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("mdatron-init-{label}-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    // DESIGN L142: first run deploys the skeleton and the init manifest.
    #[test]
    fn first_run_deploys_skeleton() {
        let root = temp_root("deploy");
        let outcome = init(&root).unwrap();
        assert!(matches!(outcome, InitOutcome::Deployed { .. }));
        assert!(root.join(".mdatron/schemas").is_dir());
        assert!(root.join(".mdatron/patterns").is_dir());
        assert!(root.join(".mdatron/config.yaml").is_file());
        assert!(root.join(".mdatron/manifest.yaml").is_file());
        std::fs::remove_dir_all(&root).unwrap();
    }

    // DESIGN L142: a second run on the unmodified tree is a no-op.
    #[test]
    fn second_run_is_noop() {
        let root = temp_root("noop");
        init(&root).unwrap();
        assert_eq!(init(&root).unwrap(), InitOutcome::AlreadyInitialized);
        std::fs::remove_dir_all(&root).unwrap();
    }

    // Security: a hand-edited manifest that lists a managed path escaping the
    // .mdatron/ partition is refused. The manifest can't hash itself, so drift
    // detection won't catch such an edit — the confinement guard must.
    #[test]
    fn manifest_path_escaping_partition_is_refused() {
        let root = temp_root("escape");
        init(&root).unwrap();
        std::fs::write(
            root.join(".mdatron/manifest.yaml"),
            "version: 1\nmanaged:\n  - path: \"../../escape.yaml\"\n    sha256: \"00\"\n",
        )
        .unwrap();
        match init(&root) {
            Err(InitError::ManifestParse { .. }) => {}
            other => panic!("expected ManifestParse refusal for escaping path, got {other:?}"),
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Hand-author a v1-style manifest (config.yaml still managed) — the shape
    /// existing adopter trees carry from before the #77 demotion.
    fn write_v1_manifest(root: &std::path::Path, sha: &str) {
        std::fs::write(
            root.join(".mdatron/manifest.yaml"),
            format!("version: 1\nmanaged:\n- path: config.yaml\n  sha256: \"{sha}\"\n"),
        )
        .unwrap();
    }

    // RED GATE FLIP (#77): config.yaml is a SEED — adopter-owned. Editing its
    // file_globs (the point of consumer-authored jurisdiction) must be neither
    // refused nor overwritten. Pre-fix this exact sequence was refused as
    // MDATRON-E0060 drift, making scoping impossible.
    #[test]
    fn edited_seed_config_is_not_refused() {
        let root = temp_root("seed-edit");
        init(&root).unwrap();
        std::fs::write(
            root.join(".mdatron/config.yaml"),
            "file_globs:\n  - \"docs/**/*.md\"\n",
        )
        .unwrap();
        assert_eq!(init(&root).unwrap(), InitOutcome::AlreadyInitialized);
        assert!(std::fs::read_to_string(root.join(".mdatron/config.yaml"))
            .unwrap()
            .contains("docs/**/*.md"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    // The drift machinery is manifest-driven data, not gone: a tree whose
    // manifest lists config.yaml as managed (v1 shape) still refuses a
    // hand-modified copy — the managed-partition contract holds wherever the
    // manifest declares it.
    #[test]
    fn manifest_listed_managed_file_drift_is_refused() {
        let root = temp_root("v1-drift");
        init(&root).unwrap();
        write_v1_manifest(&root, &sha256_hex(DEFAULT_CONFIG.as_bytes()));
        // Intact: the v1 tree is a clean no-op.
        assert_eq!(init(&root).unwrap(), InitOutcome::AlreadyInitialized);
        // Tampered: refused, not overwritten.
        std::fs::write(
            root.join(".mdatron/config.yaml"),
            "file_globs: [tampered]\n",
        )
        .unwrap();
        match init(&root) {
            Err(InitError::Drift(drifts)) => {
                assert_eq!(drifts.len(), 1);
                assert_eq!(drifts[0].file, "config.yaml");
                assert_ne!(drifts[0].expected_sha256, drifts[0].actual_sha256);
            }
            other => panic!("expected Drift refusal, got {other:?}"),
        }
        assert!(std::fs::read_to_string(root.join(".mdatron/config.yaml"))
            .unwrap()
            .contains("tampered"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    // A v1 tree's missing managed file is repaired from engine-known content,
    // preserving the manifest's own entries (the manifest defines the
    // partition; the engine only supplies bytes).
    #[test]
    fn v1_managed_missing_file_is_repaired() {
        let root = temp_root("v1-repair");
        init(&root).unwrap();
        write_v1_manifest(&root, &sha256_hex(DEFAULT_CONFIG.as_bytes()));
        std::fs::remove_file(root.join(".mdatron/config.yaml")).unwrap();
        assert!(matches!(init(&root).unwrap(), InitOutcome::Deployed { .. }));
        assert!(root.join(".mdatron/config.yaml").is_file());
        let manifest = std::fs::read_to_string(root.join(".mdatron/manifest.yaml")).unwrap();
        assert!(
            manifest.contains("config.yaml"),
            "repair must preserve the manifest's managed entry: {manifest}"
        );
        assert_eq!(init(&root).unwrap(), InitOutcome::AlreadyInitialized);
        std::fs::remove_dir_all(&root).unwrap();
    }

    // Demotion tombstones (DESIGN: removals persist) parse and survive a
    // manifest rewrite triggered by managed-file repair.
    #[test]
    fn tombstones_survive_repair() {
        let root = temp_root("tombstone");
        init(&root).unwrap();
        std::fs::write(
            root.join(".mdatron/manifest.yaml"),
            format!(
                "version: 2\nmanaged:\n- path: config.yaml\n  sha256: \"{}\"\ndemoted:\n- path: old.yaml\n  reason: \"retired at v2\"\n  owner: operator\n",
                sha256_hex(DEFAULT_CONFIG.as_bytes())
            ),
        )
        .unwrap();
        std::fs::remove_file(root.join(".mdatron/config.yaml")).unwrap();
        assert!(matches!(init(&root).unwrap(), InitOutcome::Deployed { .. }));
        let manifest = std::fs::read_to_string(root.join(".mdatron/manifest.yaml")).unwrap();
        assert!(
            manifest.contains("old.yaml") && manifest.contains("retired at v2"),
            "tombstone must survive the rewrite: {manifest}"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    // A missing seed is re-seeded (idempotent), and the tree returns to no-op.
    #[test]
    fn missing_seed_is_reseeded() {
        let root = temp_root("reseed");
        init(&root).unwrap();
        std::fs::remove_file(root.join(".mdatron/config.yaml")).unwrap();
        assert!(matches!(init(&root).unwrap(), InitOutcome::Deployed { .. }));
        assert!(root.join(".mdatron/config.yaml").is_file());
        assert_eq!(init(&root).unwrap(), InitOutcome::AlreadyInitialized);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sha256_hex_is_stable_and_lowercase() {
        // NIST FIPS-180-2 test vector for the empty input.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let h = sha256_hex(b"mdatron");
        assert_eq!(h.len(), 64);
        assert!(h
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        // Deterministic across calls.
        assert_eq!(h, sha256_hex(b"mdatron"));
    }

    /// Overwrite template `name` in an initialized tree with `bytes` and record
    /// their hash, as the version that wrote them would have.
    fn record_template(dir: &Path, name: &str, bytes: &str) {
        std::fs::write(dir.join(name), bytes).unwrap();
        let mut manifest = read_manifest(&dir.join(MANIFEST_NAME)).unwrap();
        manifest
            .managed
            .iter_mut()
            .find(|e| e.path == name)
            .unwrap()
            .sha256 = sha256_hex(bytes.as_bytes());
        write_manifest(dir, &manifest).unwrap();
    }

    fn released_template(version: &str, name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/init-templates")
            .join(version)
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    // #207, #231: an untouched template recorded by a RELEASED older version
    // (0.7.0's actual routes template) is refreshed to this version's content
    // and re-hashed; an edited one is drift, refused, and left alone.
    #[test]
    fn init_refreshes_an_untouched_released_template_and_refuses_an_edited_one() {
        let root = temp_root("refresh");
        init(&root).unwrap();
        let dir = root.join(".mdatron");
        let name = "routes.yaml.example";
        let current = template_content(name).unwrap();
        record_template(&dir, name, &released_template("0.7.0", name));

        match init(&root).unwrap() {
            InitOutcome::Deployed {
                created,
                refreshed,
                newer,
            } => {
                assert!(
                    created.is_empty() && newer.is_empty(),
                    "{created:?} {newer:?}"
                );
                assert_eq!(refreshed, vec![format!(".mdatron/{name}")]);
            }
            other => panic!("expected a refresh, got {other:?}"),
        }
        assert_eq!(std::fs::read_to_string(dir.join(name)).unwrap(), current);
        // Re-hashed: the next run is a no-op, not drift.
        assert_eq!(init(&root).unwrap(), InitOutcome::AlreadyInitialized);

        // An EDITED template is drift: refused, and not overwritten.
        let edited = format!("{current}# my note\n");
        std::fs::write(dir.join(name), &edited).unwrap();
        assert!(matches!(init(&root), Err(InitError::Drift(_))));
        assert_eq!(std::fs::read_to_string(dir.join(name)).unwrap(), edited);
        let _ = std::fs::remove_dir_all(&root);
    }

    // RED GATE (#231, GH #73 defect 4): an untouched template whose recorded
    // content this version does not know — a NEWER mdatron wrote it — is left
    // as recorded and reported, never moved back to this version's content,
    // so developers on mixed versions stop ping-ponging the committed bytes.
    #[test]
    fn init_never_moves_a_newer_template_back() {
        let root = temp_root("refresh-newer");
        init(&root).unwrap();
        let dir = root.join(".mdatron");
        let name = "routes.yaml.example";
        let newer_bytes = "# the routes template a newer mdatron ships\n";
        record_template(&dir, name, newer_bytes);
        let manifest_before = std::fs::read(dir.join(MANIFEST_NAME)).unwrap();

        match init(&root).unwrap() {
            InitOutcome::Deployed {
                created,
                refreshed,
                newer,
            } => {
                assert!(
                    created.is_empty() && refreshed.is_empty(),
                    "{created:?} {refreshed:?}"
                );
                assert_eq!(newer, vec![format!(".mdatron/{name}")]);
            }
            other => panic!("expected the newer template kept and reported, got {other:?}"),
        }
        assert_eq!(
            std::fs::read_to_string(dir.join(name)).unwrap(),
            newer_bytes
        );
        assert_eq!(
            std::fs::read(dir.join(MANIFEST_NAME)).unwrap(),
            manifest_before
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // #231: each recorded released hash is the hash of that release's actual
    // template bytes (the committed fixtures). A template unchanged since that
    // release has the same hash today; harmless, as a refresh needs the
    // recorded hash to differ from the current content first.
    #[test]
    fn released_template_hashes_match_their_fixtures() {
        for (version, name, hash) in RELEASED_TEMPLATE_HASHES {
            if *version == "unreleased" {
                continue;
            }
            assert_eq!(
                sha256_hex(released_template(version, name).as_bytes()),
                *hash,
                "{version} {name}"
            );
        }
    }

    // #231 tripwire: the shipped templates' hashes, pinned. Changing a template
    // fails here: add the hash it replaces to RELEASED_TEMPLATE_HASHES —
    // labelled with the release that shipped it (and its bytes under
    // tests/fixtures/init-templates/<version>/), or `unreleased` — so the next
    // version refreshes it forward, then update this pin.
    #[test]
    fn template_hashes_are_pinned() {
        let pinned = [
            (
                "routes.yaml.example",
                "0e7dc66dae9c98ca285e5e7ffde03d25db727768b7a3a683ab2dd9705891edf3",
            ),
            (
                "pins.yaml.example",
                "d896b9c74b219041bea8670303654ad67db8b29e4736ffbde82d7bd25a3a5c6f",
            ),
            (
                "vocabulary.yaml.example",
                "c7d0a0d3f13018322f8b8b585c3b123c1f87f85fd3f1f0d26a2ef93571f93b25",
            ),
            (
                "code-catalogs.yaml.example",
                "b4083b0aa957019cf7397747032deb38292aab8384e68923ecf6d043bab5be6f",
            ),
            (
                "links.yaml.example",
                "b11549e96b4fe8c2c411dc4b0445e79b9a07c22e722120547f843b578359054f",
            ),
        ];
        assert_eq!(pinned.len(), TEMPLATE_FILES.len());
        for (name, hash) in pinned {
            assert_eq!(
                sha256_hex(template_content(name).unwrap().as_bytes()),
                hash,
                "{name} changed: add the hash it replaces to RELEASED_TEMPLATE_HASHES (see the comment)"
            );
        }
    }
}
