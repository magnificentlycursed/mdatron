//! Pin family (#84; `DESIGN.md` § Nine check families): governing documents
//! pin a sha256 content hash over the files they govern.
//!
//! `.mdatron/pins.yaml` is an engine-defined interface parsed strictly. With
//! pin data supplied: a governed-file change with a stale pin **fails**
//! (`MDATRON-E0061`); a pin whose target cannot be opened **fails**
//! (`MDATRON-E0062`); recomputation is a single command (`mdatron pin
//! --update`). Absent pin data leaves the family inactive.
//!
//! Removals persist (`DESIGN.md` § Validation is data-driven, governance data
//! is governed): un-pinning a
//! file is a governance weakening, recorded as a standing `unpinned:` entry
//! with its justification (reason + owner). Each justified entry emits the
//! informational lint `MDATRON-L0001` on every whole-tree run — the weakening
//! stays loud in the tool's own channel for as long as it stands; an entry
//! missing its justification is flagged `MDATRON-W0042`. `pins.yaml` cannot
//! pin itself (the same fixed point as the init manifest); its integrity
//! anchor is commit review.
//!
//! This family is the mechanization of the attention loop behind the
//! 2026-07-27 staleness incident (tracker #84/#87): the governed file changes,
//! the stale pin blocks, and the re-pin commit is the moment the governing
//! document gets re-read.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::confine::{confine_lexically, open_confined, LexicalViolation, OpenViolation};
use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};
use crate::init::sha256_hex;
use crate::Error;

/// File name of the pin record under `.mdatron/`.
pub const PINS_NAME: &str = "pins.yaml";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPins {
    /// Input-format version (DEF5, #131). Optional on read (absent = v1 legacy
    /// baseline); `pin --update` stamps it going forward (manifest-style
    /// migration), so a tool-rewritten pins.yaml is born versioned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mdatron_format_version: Option<u32>,
    #[serde(default)]
    pins: Vec<RawPin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    unpinned: Vec<RawUnpinned>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPin {
    /// The governing document attesting this pin (the route table's `governed_by`
    /// relation, one spelling). `governing` is the retired 0.6.0 key, accepted
    /// as an alias; `pin --update` rewrites it (`docs/field-rename-ledger.md`).
    #[serde(alias = "governing")]
    governed_by: String,
    file: String,
    /// Optional heading (e.g. `"## Requirements"`) scoping the pin to
    /// that section's span rather than the whole file (#146). Absent = whole-file
    /// (unchanged); omitted from serialization so existing whole-file records
    /// stay byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    section: Option<String>,
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUnpinned {
    file: String,
    #[serde(alias = "governing")]
    governed_by: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    owner: String,
}

/// A validated, active pin entry.
pub struct Pin {
    pub governed_by: String,
    pub file: String,
    /// The section this pin scopes to (a heading spec), or `None` for whole-file.
    pub section: Option<String>,
    pub sha256: String,
}

/// The loaded pin record: active pins plus findings produced during load
/// (confinement refusals fail-closed; standing weakening annotations).
pub struct LoadedPins {
    pub pins: Vec<Pin>,
    pub findings: Vec<Finding>,
    /// sha256 (lowercase hex) of the exact `pins.yaml` bytes `load` read
    /// (#176, the envelope's input lineage).
    pub digest: String,
}

/// Load `.mdatron/pins.yaml`. `Ok(None)` when absent (family inactive); `Err`
/// when unreadable or structurally malformed (loud); otherwise the active pins
/// plus load-time findings: confinement-violating entries are dropped
/// fail-closed under `E0010`/`E0011`, every justified `unpinned:` tombstone
/// emits its standing `L0001` lint, and an unjustified one is flagged `W0042`.
pub fn load(project_root: &Path) -> Result<Option<LoadedPins>, Error> {
    let path = project_root.join(".mdatron").join(PINS_NAME);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(Error::Config(format!(
                "cannot read '{}': {e}",
                path.display()
            )))
        }
    };
    crate::format_version::check_input_format_version(&content, PINS_NAME, false)?;
    let raw: RawPins = serde_yaml_ng::from_str(&content)
        .map_err(|e| Error::Config(format!("cannot parse '{}': {e}", path.display())))?;

    let mut pins = Vec::new();
    let mut findings = Vec::new();

    for entry in raw.pins {
        let mut confined_ok = true;
        for (field, value) in [("governed_by", &entry.governed_by), ("file", &entry.file)] {
            if let Err(v) = confine_lexically(Path::new(value)) {
                findings.push(confinement_finding(&path, field, value, &v));
                confined_ok = false;
            }
        }
        if !confined_ok {
            continue; // dropped: fail-closed
        }
        pins.push(Pin {
            governed_by: entry.governed_by,
            file: entry.file,
            section: entry.section,
            sha256: entry.sha256,
        });
    }

    for t in &raw.unpinned {
        if t.reason.trim().is_empty() || t.owner.trim().is_empty() {
            findings.push(Finding {
                code: "MDATRON-W0042".into(),
                severity: Severity::Warning,
                summary: "governance-weakening-unjustified".into(),
                message: "an unpinned entry carries no justification (reason and \
                          owner are required); a weakening that cannot say why it \
                          stands is not a tombstone, it is an erasure"
                    .into(),
                help: Some("add reason and owner to the unpinned entry".into()),
                location: Location::whole_file(&path),
                explain_ref: Some("MDATRON-W0042".into()),
                quoted: vec![QuotedRegion {
                    platform_variant: false,
                    label: "file".into(),
                    content: t.file.clone(),
                }],
            });
        } else {
            // The standing informational lint: the weakening stays loud in the
            // tool's own channel for as long as the tombstone stands.
            findings.push(Finding {
                code: "MDATRON-L0001".into(),
                severity: Severity::Lint,
                summary: "governance-weakening-standing".into(),
                message: "a file was un-pinned from its governing document; the \
                          tombstone below is the standing record of that weakening"
                    .into(),
                help: None,
                location: Location::whole_file(&path),
                explain_ref: Some("MDATRON-L0001".into()),
                quoted: vec![
                    QuotedRegion {
                        platform_variant: false,
                        label: "file".into(),
                        content: t.file.clone(),
                    },
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

    Ok(Some(LoadedPins {
        pins,
        findings,
        digest: crate::init::sha256_hex(content.as_bytes()),
    }))
}

/// Verify every active pin against the captured snapshot (#103): the pinned
/// file's capture-time bytes — the same bytes every other check validated —
/// are sha256-compared to the record. Stale → `E0061`; target unopenable →
/// `E0062`; symlinked target refused → `E0012`. The pin certifies what the run
/// verified; a post-seam mutation is the NEXT run's finding.
pub fn check(
    project_root: &Path,
    pins: &[Pin],
    snapshot: &crate::snapshot::Snapshot,
    findings: &mut Vec<Finding>,
) {
    use crate::snapshot::Captured;
    let pins_path = project_root.join(".mdatron").join(PINS_NAME);
    for pin in pins {
        // Confinement was established at load; re-derive for the lookup (a
        // lexically-escaping pin path was never captured, and never read).
        let Ok(confined) = confine_lexically(Path::new(&pin.file)) else {
            continue;
        };
        // Discovery captured every relevant pin target before the seam; a
        // miss is an engine defect and reports as one (the None arm), never a
        // filesystem fallback.
        match snapshot.get(confined.as_path()) {
            Some(Captured::Content(c)) => {
                let bytes = c.bytes();
                let actual = match &pin.section {
                    None => sha256_hex(bytes),
                    // Section pin (#146): hash only the heading-delimited span(s).
                    // A non-UTF8 file or a missing heading cannot be located → E0063.
                    Some(section) => match c.text().and_then(|s| pin_section_bytes(s, section)) {
                        Some(span_bytes) => sha256_hex(&span_bytes),
                        None => {
                            findings.push(section_not_found(&pins_path, pin, section));
                            continue;
                        }
                    },
                };
                if actual != pin.sha256 {
                    findings.push(Finding {
                        code: "MDATRON-E0061".into(),
                        severity: Severity::Error,
                        summary: "pin-stale".into(),
                        // #165: the recorded sha is adopter-authored (pins.yaml,
                        // not validated as hex) — it rides in a quoted region,
                        // not inline in the engine-authored message.
                        message: "the governed file changed after its pin was \
                                  recorded; re-read the governing document, then \
                                  re-pin with `mdatron pin --update`"
                            .into(),
                        help: Some(
                            "the stale pin is the attention loop working: the \
                             governing relationship must be reviewed, not just the \
                             hash refreshed"
                                .into(),
                        ),
                        location: Location::whole_file(&pins_path),
                        explain_ref: Some("MDATRON-E0061".into()),
                        quoted: vec![
                            QuotedRegion {
                                platform_variant: false,
                                label: "file".into(),
                                content: pin.file.clone(),
                            },
                            QuotedRegion {
                                platform_variant: false,
                                label: "governed_by".into(),
                                content: pin.governed_by.clone(),
                            },
                            QuotedRegion {
                                platform_variant: false,
                                label: "recorded".into(),
                                content: crate::init::short(&pin.sha256).to_string(),
                            },
                            QuotedRegion {
                                platform_variant: false,
                                label: "found".into(),
                                content: crate::init::short(&actual).to_string(),
                            },
                        ],
                    });
                }
            }
            Some(Captured::SymlinkRefused { tag, .. }) => {
                let reparse = crate::confine::describe_reparse(tag.as_ref().copied());
                findings.push(Finding {
                    code: "MDATRON-E0012".into(),
                    severity: Severity::Error,
                    summary: "symlinked-component-refused".into(),
                    message: format!(
                        "a pinned file resolves through {}; no-follow resolution \
                         refuses it",
                        reparse.what
                    ),
                    help: Some(
                        reparse
                            .help
                            .unwrap_or_else(|| "pin the real file inside the governed tree".into()),
                    ),
                    location: Location::whole_file(&pins_path),
                    explain_ref: Some("MDATRON-E0012".into()),
                    quoted: vec![QuotedRegion {
                        platform_variant: false,
                        label: "file".into(),
                        content: pin.file.clone(),
                    }],
                });
            }
            // Unreadable, open-refused, or (defensively) over the size cap —
            // config-scoped discovery escalates TooLarge before the seam, but
            // if one reaches here E0062 keeps it loud, not silent.
            Some(Captured::OpenedUnreadable { .. })
            | Some(Captured::OpenIo { .. })
            | Some(Captured::TooLarge { .. }) => findings.push(target_unopenable(&pins_path, pin)),
            // Never captured: an ENGINE defect in target discovery — a pin
            // verdict against a healthy file would be a false attestation.
            None => {
                findings.push(Finding {
                    code: "MDATRON-E0081".into(),
                    severity: Severity::Error,
                    summary: "reference-target-not-captured".into(),
                    message: "this pin's target was never captured into the run \
                              snapshot — an engine defect in target discovery, \
                              not a defect in the pin record; please report it \
                              upstream"
                        .into(),
                    help: None,
                    location: Location::whole_file(&pins_path),
                    explain_ref: Some("MDATRON-E0081".into()),
                    quoted: vec![QuotedRegion {
                        platform_variant: false,
                        label: "file".into(),
                        content: pin.file.clone(),
                    }],
                });
            }
        }
    }
}

/// Recompute every active pin's sha256 from current content and rewrite
/// `pins.yaml`, preserving `unpinned:` tombstones. Returns
/// `(file, old_sha256, new_sha256)` for each entry that changed. Entries whose
/// target is absent, a symlink, or not a regular file are left untouched and
/// reported by the caller's next verify (`E0062` / `E0012`); any OTHER open
/// failure is a LOUD error (#64 cold-review W2) — recompute never invents a
/// hash for an unreadable file, and never skips one silently.
/// A target exceeding the declared per-file input bound
/// ([`crate::verify::MAX_FILE_BYTES`]) is a LOUD error naming the bound —
/// verify aborts on such a file (`bound_exceeded`), so re-pinning it would
/// mint a pin verify refuses to check (GH #48 finding 4). With `dry_run`, the
/// diffs are computed and returned but nothing is written.
pub fn update(project_root: &Path, dry_run: bool) -> Result<Vec<(String, String, String)>, Error> {
    let path = project_root.join(".mdatron").join(PINS_NAME);
    let content = std::fs::read_to_string(&path)
        .map_err(|e| Error::Config(format!("cannot read '{}': {e}", path.display())))?;
    // DEF5 (#131): the update path runs the same input-format probe the load
    // path does (GH #48 finding 4 closed the asymmetry) — an unsupported
    // future-format pins.yaml is a legible refusal here too, never a silent
    // rewrite under a format this mdatron does not understand.
    crate::format_version::check_input_format_version(&content, PINS_NAME, false)?;
    let mut raw: RawPins = serde_yaml_ng::from_str(&content)
        .map_err(|e| Error::Config(format!("cannot parse '{}': {e}", path.display())))?;

    let mut changed = Vec::new();
    for entry in &mut raw.pins {
        let confined = confine_lexically(Path::new(&entry.file)).map_err(|v| {
            // Adopter path escaped before it rides an error the CLI prints raw
            // to stderr (#165 marking discipline; lanes-B-E review F2).
            let escaped = crate::diagnostic::escape_path_text(&entry.file);
            Error::Config(format!("pin file '{escaped}' escapes: {v:?}"))
        })?;
        let handle = match open_confined(project_root, &confined) {
            Ok(h) => h,
            // Absent, routed through a file, an invalid name, or refused as a
            // symlink / non-regular file: an adopter-authored pins.yaml defect
            // that the next verify reports (E0062 / E0012) — leave the record
            // (#64 review N4: one partition for all "the pin cannot resolve"
            // shapes, so a typo'd path and a file-as-intermediate behave alike).
            Err(OpenViolation::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::NotADirectory
                        | std::io::ErrorKind::InvalidFilename
                ) =>
            {
                continue
            }
            Err(OpenViolation::Symlink { .. }) | Err(OpenViolation::NotRegular) => continue,
            // Any OTHER open failure (permission, an unresolvable root, an I/O
            // fault) is LOUD: a pin that cannot be recomputed is a reported
            // state, not a silent skip (no-silent-degradation; #64 W2).
            Err(OpenViolation::Io(e)) => {
                let escaped = crate::diagnostic::escape_path_text(&entry.file);
                return Err(Error::Config(format!("cannot re-pin '{escaped}': {e}")));
            }
        };
        use std::io::Read;
        // Bounded read, aligned with verify's declared per-file cap (GH #48
        // finding 4): the check path aborts an oversized pinned file
        // (`bound_exceeded`, max-input-size-per-file), so re-pinning one here
        // would mint a pin the bounded verify path refuses to check — an
        // uncheckable attestation. The over-limit file fails LOUDLY instead.
        let limit = crate::verify::MAX_FILE_BYTES;
        let mut bytes = Vec::new();
        handle
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| {
                let escaped = crate::diagnostic::escape_path_text(&entry.file);
                Error::Config(format!("cannot read '{escaped}': {e}"))
            })?;
        if bytes.len() > limit {
            let escaped = crate::diagnostic::escape_path_text(&entry.file);
            return Err(Error::Config(format!(
                "cannot re-pin '{escaped}': the file exceeds the {limit}-byte \
                 per-file limit (max-input-size-per-file) — verify refuses to \
                 check a pin over an out-of-bounds file, so recording one would \
                 mint an uncheckable pin"
            )));
        }
        let actual = match &entry.section {
            None => sha256_hex(&bytes),
            // Section pin (#146): recompute over the span(s) only — the SAME
            // extraction the check runs (`pin_section_bytes`). A missing heading
            // is left untouched (verify reports E0063) — recompute never invents
            // a hash for a section it cannot locate.
            Some(section) => match std::str::from_utf8(&bytes)
                .ok()
                .and_then(|s| pin_section_bytes(s, section))
            {
                Some(span_bytes) => sha256_hex(&span_bytes),
                None => continue,
            },
        };
        if actual != entry.sha256 {
            changed.push((entry.file.clone(), entry.sha256.clone(), actual.clone()));
            entry.sha256 = actual;
        }
    }

    if dry_run {
        return Ok(changed);
    }
    // DEF5 (#131): stamp the current input-format version so a tool-rewritten
    // pins.yaml is born versioned (manifest-style migration).
    raw.mdatron_format_version = Some(crate::format_version::SUPPORTED_INPUT_FORMAT_VERSION);
    let yaml = serde_yaml_ng::to_string(&raw)
        .map_err(|e| Error::Config(format!("cannot serialize pins: {e}")))?;
    let body = format!(
        "# mdatron pin record — governing documents pin sha256 over governed files (#84).\n\
         # Recompute with `mdatron pin --update`. This file cannot pin itself; its\n\
         # integrity anchor is commit review (DESIGN § Validation is data-driven, governance data is governed).\n{yaml}"
    );
    // Atomic write (#126 DEF8): pins.yaml is rewritten in place on every
    // `pin --update`; a torn write would corrupt the pin set.
    crate::atomic::write(&path, body.as_bytes())
        .map_err(|e| Error::Config(format!("cannot write pins: {e}")))?;
    Ok(changed)
}

/// The bytes a section pin hashes (GH #48 lane G): FRONTMATTER IS STRIPPED
/// first — a `# x` comment line inside frontmatter YAML is not a heading and
/// must not capture the span (the marker family's strip, mirrored) — then
/// EVERY span matching the heading spec (level + exact text, `markup::
/// section_spans`) is concatenated in document order, so content under a
/// duplicate same-level/same-text heading cannot evade the pin (the evasion
/// lane A closed for section rules). `None` when no heading matches (E0063).
/// Both the check and `pin --update` hash through this ONE extraction, so they
/// cannot diverge.
fn pin_section_bytes(content: &str, section: &str) -> Option<Vec<u8>> {
    let body = match crate::frontmatter::parse(content) {
        Ok(Some((_, b))) => b,
        _ => content,
    };
    let spans = crate::markup::section_spans(body, section);
    if spans.is_empty() {
        return None;
    }
    let mut bytes = Vec::with_capacity(spans.iter().map(|s| s.len()).sum());
    for span in &spans {
        bytes.extend_from_slice(span.as_bytes());
    }
    Some(bytes)
}

fn target_unopenable(pins_path: &Path, pin: &Pin) -> Finding {
    Finding {
        code: "MDATRON-E0062".into(),
        severity: Severity::Error,
        summary: "pin-target-missing".into(),
        message: "a pinned file cannot be opened in the governed tree; a pin over \
                  nothing is a dangling governance edge"
            .into(),
        help: Some(
            "restore the file, correct the pin's path, or un-pin it with a \
             justified tombstone (unpinned: entry with reason and owner)"
                .into(),
        ),
        location: Location::whole_file(pins_path),
        explain_ref: Some("MDATRON-E0062".into()),
        quoted: vec![
            QuotedRegion {
                platform_variant: false,
                label: "file".into(),
                content: pin.file.clone(),
            },
            QuotedRegion {
                platform_variant: false,
                label: "governed_by".into(),
                content: pin.governed_by.clone(),
            },
        ],
    }
}

/// A section pin whose named heading could not be located in the target (missing
/// heading, or a non-text file) — `MDATRON-E0063` (#146).
fn section_not_found(pins_path: &Path, pin: &Pin, section: &str) -> Finding {
    Finding {
        code: "MDATRON-E0063".into(),
        severity: Severity::Error,
        summary: "pin-section-not-found".into(),
        message: "a section pin names a heading that is not present in its target \
                  file (a section pinned over nothing cannot be verified); the \
                  heading is matched by level and text, and a `#` inside a code \
                  fence is not a heading"
            .into(),
        help: Some(
            "correct the pin's `section` heading to match one in the file, or \
             remove `section` to pin the whole file"
                .into(),
        ),
        location: Location::whole_file(pins_path),
        explain_ref: Some("MDATRON-E0063".into()),
        quoted: vec![
            QuotedRegion {
                platform_variant: false,
                label: "file".into(),
                content: pin.file.clone(),
            },
            QuotedRegion {
                platform_variant: false,
                label: "section".into(),
                content: section.to_string(),
            },
        ],
    }
}

fn confinement_finding(
    pins_path: &Path,
    field: &str,
    value: &str,
    violation: &LexicalViolation,
) -> Finding {
    let (code, summary) = match violation {
        LexicalViolation::Absolute => ("MDATRON-E0010", "absolute-path-refused"),
        LexicalViolation::ParentSegment => ("MDATRON-E0011", "parent-segment-refused"),
    };
    Finding {
        code: code.into(),
        severity: Severity::Error,
        summary: summary.into(),
        message: format!(
            "a pin's {field} escapes the governed tree; the entry is dropped \
             (fail-closed)"
        ),
        help: Some(
            "pin paths are relative to the project root and may not carry \
                    parent segments or absolute prefixes"
                .into(),
        ),
        location: Location::whole_file(pins_path),
        explain_ref: Some(code.to_string()),
        quoted: vec![QuotedRegion {
            platform_variant: false,
            label: field.to_string(),
            content: value.to_string(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temp project with `.mdatron/pins.yaml` pinning `governed.md` at its
    /// current hash, removed on drop. Self-contained (no verify-pipeline
    /// scaffolding): these tests exercise `pin::update` alone.
    struct TempPinProject(std::path::PathBuf);

    impl TempPinProject {
        fn new(label: &str, content: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("mdatron-pin-{label}-{nanos}"));
            std::fs::create_dir_all(root.join(".mdatron")).unwrap();
            std::fs::write(root.join("GOVERNING.md"), "# gov\n").unwrap();
            std::fs::write(root.join("governed.md"), content).unwrap();
            let sha = sha256_hex(content.as_bytes());
            std::fs::write(
                root.join(".mdatron").join(PINS_NAME),
                format!(
                    "pins:\n- governed_by: GOVERNING.md\n  file: governed.md\n  sha256: \"{sha}\"\n"
                ),
            )
            .unwrap();
            Self(root)
        }

        fn record_path(&self) -> std::path::PathBuf {
            self.0.join(".mdatron").join(PINS_NAME)
        }
    }

    impl Drop for TempPinProject {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    // RED GATE (GH #48 finding 4, lane C): `pin --update` refuses a pinned file
    // exceeding the declared per-file bound, LOUDLY, naming the bound — pre-fix
    // it read unbounded and happily hashed what verify then refused
    // (`bound_exceeded`), minting an uncheckable pin.
    #[test]
    fn update_refuses_an_over_limit_target() {
        let limit = crate::verify::MAX_FILE_BYTES;
        let proj = TempPinProject::new("overlimit", "small\n");
        let mut big = String::with_capacity(limit + 64);
        while big.len() <= limit {
            big.push_str("0123456789abcdef\n");
        }
        std::fs::write(proj.0.join("governed.md"), &big).unwrap();
        // Dry-run and real update both refuse.
        let err = update(&proj.0, true).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("max-input-size-per-file"),
            "the refusal names the bound: {msg}"
        );
        let before = std::fs::read_to_string(proj.record_path()).unwrap();
        update(&proj.0, false).unwrap_err();
        let after = std::fs::read_to_string(proj.record_path()).unwrap();
        assert_eq!(
            before, after,
            "an over-limit target never rewrites the record"
        );
    }

    // Lanes-B-E review F2: the update path's OWN error strings ride raw to the
    // CLI's stderr, so an adopter-authored `file:` carrying a control byte must
    // be escaped in them too (the lane-C sweep originally missed these two
    // neighbors of the escaped over-limit message).
    #[test]
    fn update_confinement_error_escapes_adopter_path() {
        let proj = TempPinProject::new("escape-err", "content\n");
        // YAML forbids a RAW control byte even double-quoted, so the hostile
        // record uses YAML's `\x1b` escape — the parser DECODES it to a raw ESC
        // in the parsed `file` string (exactly how an adopter record smuggles
        // a control byte past the YAML layer).
        std::fs::write(
            proj.0.join(".mdatron").join("pins.yaml"),
            "pins:\n- file: \"../esc\\x1b[31mRED.md\"\n  governed_by: GOVERNING.md\n  sha256: abc\n",
        )
        .unwrap();
        let err = update(&proj.0, true).unwrap_err();
        let msg = format!("{err}");
        assert!(
            !msg.contains('\u{1b}'),
            "no raw ESC in the update error: {msg:?}"
        );
        assert!(
            msg.contains("\\x1B") && msg.contains("escapes"),
            "the escaped path still names the refusal: {msg:?}"
        );
    }

    // GH #48 finding 4 (lane C): the update path runs the same DEF5
    // input-format probe the load path does — a future-format pins.yaml is a
    // legible refusal, never silently rewritten under an unsupported format.
    #[test]
    fn update_refuses_a_future_format_record() {
        let proj = TempPinProject::new("future-format", "content\n");
        let record = std::fs::read_to_string(proj.record_path()).unwrap();
        std::fs::write(
            proj.record_path(),
            format!("mdatron_format_version: 99\n{record}"),
        )
        .unwrap();
        let err = update(&proj.0, true).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("supports up to"), "legible refusal: {msg}");
    }
}
