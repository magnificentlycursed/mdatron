//! Input-format versioning (DEF5, #131 SO ruling; executed by the
//! scoping-consistency pass, #34). Each adopter input file — `routes.yaml`,
//! `vocabulary.yaml`, `pins.yaml`, `code-catalogs.yaml` — may declare
//! `mdatron_format_version`: a per-file version on the **input** contract, its
//! own SemVer axis, independent of the DSL's `mdatron_dsl_version` and the JSON
//! output's `mdatron_output_version` (three axes, one release).
//!
//! The version turns an unknown-future-format file into a **legible break**
//! ("declares format v2, engine supports v1") instead of an opaque parse error.
//! Because the input structs are `deny_unknown_fields`, the version is read by a
//! LENIENT probe ([`FormatProbe`]) BEFORE the strict parse: a strict parse fails
//! atomically on any unknown sibling field and would never surface the version
//! (the Platform Engineer review's F4). The version field is also declared on
//! each strict struct so `deny_unknown_fields` accepts it.
//!
//! Files **new in 0.6.0** (`code-catalogs.yaml`) are born versioned — the field
//! is required. Existing hand-authored files (`routes.yaml`, `vocabulary.yaml`,
//! `pins.yaml`) take it **optional, absent = v1** (the legacy baseline), so a
//! 0.5.0-authored file still parses. The lenient `config.yaml` (#80 D1) does not
//! gate on a version at all — a hard gate there would revoke its forward-compat.

use serde::Deserialize;

use crate::Error;

/// The highest input-format version this engine understands.
pub(crate) const SUPPORTED_INPUT_FORMAT_VERSION: u32 = 1;

/// A lenient probe reading ONLY `mdatron_format_version` — every other field is
/// ignored, so it runs against a `deny_unknown_fields` file before the strict
/// parse without that parse's atomic-failure-on-unknown-sibling.
#[derive(Debug, Deserialize)]
struct FormatProbe {
    #[serde(default)]
    mdatron_format_version: Option<u32>,
}

/// Probe `content` for its declared `mdatron_format_version` and check it against
/// the engine's supported range — a **legible break** run ahead of the strict
/// parse. `file` names the file for the message; `required` is true for files
/// born versioned in this release (`code-catalogs.yaml`).
pub(crate) fn check_input_format_version(
    content: &str,
    file: &str,
    required: bool,
) -> Result<(), Error> {
    let probe: FormatProbe = serde_yaml_ng::from_str(content).map_err(|e| {
        Error::Config(format!(
            "cannot read mdatron_format_version from '{file}': {e}"
        ))
    })?;
    match probe.mdatron_format_version {
        None if required => Err(Error::Config(format!(
            "'{file}' must declare `mdatron_format_version: {SUPPORTED_INPUT_FORMAT_VERSION}` \
             (this file format is versioned)"
        ))),
        // Optional file with no version: the v1 legacy baseline (0.5.0-authored).
        None => Ok(()),
        Some(0) => Err(Error::Config(format!(
            "'{file}' declares mdatron_format_version 0; input-format versions start at 1"
        ))),
        Some(v) if v > SUPPORTED_INPUT_FORMAT_VERSION => Err(Error::Config(format!(
            "'{file}' declares mdatron_format_version {v}, but this mdatron supports up to \
             {SUPPORTED_INPUT_FORMAT_VERSION} — upgrade mdatron, or use a file written for \
             v{SUPPORTED_INPUT_FORMAT_VERSION}"
        ))),
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_is_ok_when_optional_but_errs_when_required() {
        assert!(check_input_format_version("routes: []\n", "routes.yaml", false).is_ok());
        let err =
            check_input_format_version("codes: []\n", "code-catalogs.yaml", true).unwrap_err();
        assert!(format!("{err}").contains("must declare"));
    }

    #[test]
    fn future_version_is_a_legible_break() {
        let err = check_input_format_version(
            "mdatron_format_version: 2\nroutes: []\n",
            "routes.yaml",
            false,
        )
        .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("supports up to 1") && msg.contains('2'),
            "got {msg}"
        );
    }

    #[test]
    fn version_read_through_unknown_sibling_fields() {
        // The probe is lenient: it reads the version even when a sibling field
        // would trip the strict parse — that is the whole point (PE F4).
        let err = check_input_format_version(
            "mdatron_format_version: 9\nsome_future_field: true\nroutes: []\n",
            "routes.yaml",
            false,
        )
        .unwrap_err();
        assert!(format!("{err}").contains("supports up to 1"));
    }

    #[test]
    fn version_one_and_zero() {
        assert!(check_input_format_version("mdatron_format_version: 1\n", "f", true).is_ok());
        assert!(
            check_input_format_version("mdatron_format_version: 0\n", "f", false)
                .unwrap_err()
                .to_string()
                .contains("start at 1")
        );
    }
}
