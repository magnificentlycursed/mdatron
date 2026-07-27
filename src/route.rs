//! Route family (#83; `DESIGN.md` § Five check families): the route table is
//! an **allowlist over the governed tree**.
//!
//! `.mdatron/routes.yaml` is an engine-defined interface parsed strictly
//! (unknown fields refused — the engine-shipped-schema discipline of
//! `DESIGN.md` § Validation is data-driven). With route data supplied: a walked
//! file no route claims **blocks** (`MDATRON-E0030`); a route citing an absent
//! governing document **blocks** (`MDATRON-E0031`); two routes claiming one
//! file is an **error** (`MDATRON-E0032`, the ratified conflict outcome); a
//! filename underivable from the route's naming grammar is **flagged**
//! (`MDATRON-W0041`). Absent route data leaves the family inactive.
//!
//! Family-wide disciplines: every adopter-supplied path — the `files` glob and
//! `governed_by` — is held to the confinement contract; escapes are rejected
//! under the path-confinement codes (`E0010` absolute, `E0011` parent segment,
//! `E0012` symlink), existent and non-existent targets alike. Naming grammars
//! compile on a linear-time engine (`regex_lite`; `DESIGN.md` L17 requires
//! linear-time matching for adopter-supplied patterns).

use std::path::Path;

use serde::Deserialize;

use crate::confine::{confine_lexically, open_confined, LexicalViolation, OpenViolation};
use crate::diagnostic::{Finding, Location, QuotedRegion, Severity};
use crate::Error;

/// File name of the route table under `.mdatron/`.
pub const ROUTES_NAME: &str = "routes.yaml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTable {
    routes: Vec<RawEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    /// Glob (relative to the project root) of the files this route claims.
    files: String,
    /// The governing document for the claimed files (relative, confined).
    governed_by: String,
    /// Optional naming grammar: a regex the claimed FILENAME must match.
    #[serde(default)]
    naming: Option<String>,
}

/// The loaded route table: the active entries plus the per-entry findings
/// produced during load (confinement refusals, absent governing documents).
pub struct LoadedRoutes {
    pub routes: Vec<Route>,
    pub findings: Vec<Finding>,
}

/// A compiled, active route entry.
pub struct Route {
    pub files: glob::Pattern,
    pub governed_by: String,
    pub naming: Option<regex_lite::Regex>,
}

/// Load, validate, and compile `.mdatron/routes.yaml`.
///
/// - `Ok(None)`: file absent — the family is inactive.
/// - `Err`: unreadable or structurally malformed (strict parse) — loud, a
///   governance file never degrades silently.
/// - `Ok(Some((routes, findings)))`: the active entries plus per-entry
///   findings. A confinement-violating entry yields its `E0010`/`E0011`
///   finding and is DROPPED from the active set (fail-closed: files only it
///   claimed become unrouted). An entry whose governing document is absent or
///   symlinked yields `E0031`/`E0012` but STAYS active for matching, so the
///   defect reports once rather than cascading into spurious `E0030`s.
pub fn load(project_root: &Path) -> Result<Option<LoadedRoutes>, Error> {
    let path = project_root.join(".mdatron").join(ROUTES_NAME);
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
    let raw: RawTable = serde_yaml_ng::from_str(&content)
        .map_err(|e| Error::Config(format!("cannot parse '{}': {e}", path.display())))?;

    let mut routes = Vec::new();
    let mut findings = Vec::new();
    for entry in raw.routes {
        // Confinement of the files GLOB, decided on the pattern text alone
        // (parent segments are rejected in glob patterns too — BOUNDARY-
        // PREAMBLE § 7 carried per DESIGN § Five check families).
        if let Err(v) = confine_lexically(Path::new(&entry.files)) {
            findings.push(confinement_finding(&path, "files", &entry.files, &v));
            continue; // dropped: fail-closed
        }
        let files = match glob::Pattern::new(&entry.files) {
            Ok(p) => p,
            Err(e) => {
                return Err(Error::Config(format!(
                    "route files glob '{}' does not compile: {e}",
                    entry.files
                )))
            }
        };

        // Confinement + no-follow existence of the governing document.
        match confine_lexically(Path::new(&entry.governed_by)) {
            Err(v) => {
                findings.push(confinement_finding(
                    &path,
                    "governed_by",
                    &entry.governed_by,
                    &v,
                ));
                continue; // dropped: fail-closed
            }
            Ok(confined) => match open_confined(project_root, &confined) {
                Ok(_handle) => {} // exists; handle dropped immediately
                Err(OpenViolation::Symlink { .. }) => {
                    findings.push(Finding {
                        code: "MDATRON-E0012".into(),
                        severity: Severity::Error,
                        summary: "symlinked-component-refused".into(),
                        message: "a route's governing document resolves through a \
                                  symbolic link; no-follow resolution refuses it"
                            .into(),
                        help: Some(
                            "point governed_by at the real file inside the \
                                    governed tree"
                                .into(),
                        ),
                        location: Location::whole_file(&path),
                        explain_ref: Some("MDATRON-E0012".into()),
                        quoted: vec![QuotedRegion {
                            label: "governed_by".into(),
                            content: entry.governed_by.clone(),
                        }],
                    });
                }
                Err(OpenViolation::Io(_)) => {
                    findings.push(Finding {
                        code: "MDATRON-E0031".into(),
                        severity: Severity::Error,
                        summary: "governing-document-absent".into(),
                        message: "a route cites a governing document that cannot be \
                                  opened in the governed tree; the files it claims \
                                  are governed by nothing"
                            .into(),
                        help: Some(
                            "create the governing document, or correct the route's \
                             governed_by path"
                                .into(),
                        ),
                        location: Location::whole_file(&path),
                        explain_ref: Some("MDATRON-E0031".into()),
                        quoted: vec![QuotedRegion {
                            label: "governed_by".into(),
                            content: entry.governed_by.clone(),
                        }],
                    });
                }
            },
        }

        let naming = match &entry.naming {
            None => None,
            Some(pattern) => match regex_lite::Regex::new(pattern) {
                Ok(r) => Some(r),
                Err(e) => {
                    return Err(Error::Config(format!(
                        "route naming grammar '{pattern}' does not compile: {e}"
                    )))
                }
            },
        };

        routes.push(Route {
            files,
            governed_by: entry.governed_by,
            naming,
        });
    }
    Ok(Some(LoadedRoutes { routes, findings }))
}

/// Route checks for one walked file (root-relative path). Emits `E0030` when
/// no route claims it, `E0032` when more than one does, and `W0041` when the
/// filename fails the claiming route's naming grammar.
pub fn check_file(routes: &[Route], rel: &Path, abs: &Path, findings: &mut Vec<Finding>) {
    let claims: Vec<&Route> = routes
        .iter()
        .filter(|r| r.files.matches_path(rel))
        .collect();
    match claims.len() {
        0 => findings.push(Finding {
            code: "MDATRON-E0030".into(),
            severity: Severity::Error,
            summary: "unrouted-file".into(),
            message: "this file is inside the walked jurisdiction but no route \
                      claims it; the route table is a closed-world allowlist"
                .into(),
            help: Some(
                "add a route whose files glob claims it, or narrow file_globs if \
                 it should not be walked at all"
                    .into(),
            ),
            location: Location::whole_file(abs),
            explain_ref: Some("MDATRON-E0030".into()),
            quoted: Vec::new(),
        }),
        1 => {
            let route = claims[0];
            if let Some(naming) = &route.naming {
                let name = rel
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if !naming.is_match(&name) {
                    findings.push(Finding {
                        code: "MDATRON-W0041".into(),
                        severity: Severity::Warning,
                        summary: "name-underivable".into(),
                        message: format!(
                            "this file's name is not derivable from its route's \
                             naming grammar /{}/",
                            naming.as_str()
                        ),
                        help: Some(
                            "rename the file to match the grammar, or amend the \
                             route's naming field"
                                .into(),
                        ),
                        location: Location::whole_file(abs),
                        explain_ref: Some("MDATRON-W0041".into()),
                        quoted: vec![QuotedRegion {
                            label: "name".into(),
                            content: name,
                        }],
                    });
                }
            }
        }
        _ => findings.push(Finding {
            code: "MDATRON-E0032".into(),
            severity: Severity::Error,
            summary: "route-conflict".into(),
            message: format!(
                "{} routes claim this file; ownership must be unambiguous \
                 (conflicting adopter data is an error by contract)",
                claims.len()
            ),
            help: Some("disjoint the files globs so exactly one route claims it".into()),
            location: Location::whole_file(abs),
            explain_ref: Some("MDATRON-E0032".into()),
            quoted: claims
                .iter()
                .map(|r| QuotedRegion {
                    label: "route".into(),
                    content: r.files.as_str().to_string(),
                })
                .collect(),
        }),
    }
}

fn confinement_finding(
    routes_path: &Path,
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
            "a route's {field} escapes the governed tree; the entry is dropped \
             and the files it claimed are unrouted (fail-closed)"
        ),
        help: Some(
            "route paths and globs are relative to the project root and \
                    may not carry parent segments or absolute prefixes"
                .into(),
        ),
        location: Location::whole_file(routes_path),
        explain_ref: Some(code.to_string()),
        quoted: vec![QuotedRegion {
            label: field.to_string(),
            content: value.to_string(),
        }],
    }
}
