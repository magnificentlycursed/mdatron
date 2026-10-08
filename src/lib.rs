//! mdatron engine internals — **not a public API**.
//!
//! mdatron is consumed **as a binary** (#81, operator ruling 2026-07-22
//! executing the 2026-06-02 binary-first directive): the machine interface is
//! `mdatron verify --json` / `mdatron explain --json`, with version discipline
//! on the JSON envelope (DESIGN.md § Diagnostics are a versioned contract).
//! This lib target exists only so unit tests, the integration suites under
//! `tests/`, and the load-bearing `compile_fail` doctests (confine, #53) can
//! link the engine; it carries **no API-stability promise** and is not a
//! supported consumption surface. Shell out to the binary instead. To keep
//! that mechanical (#185, GH #52 minor: semver tooling reads every `pub` item
//! as contract): a module nothing outside the crate uses is `pub(crate)`, and
//! every item that IS exported — for the binary and the suites — is
//! `#[doc(hidden)]`, so the rendered documentation is this page alone.
//!
//! Two axes per DESIGN.md § Summary: JSON Schema for structural validation
//! (the schema family); a Schematron-derived DSL for cross-field, cross-file, and cross-document
//! semantic rules (the rule DSL).
//!
//! mdatron is descended from Schematron (ISO/IEC 19757-3); the `-tron` suffix
//! evokes Schematron, the same way `jsontron` did for JSON.
//!
//! # Implementation state
//!
//! The verify pipeline is implemented end to end: frontmatter parsing, JSON Schema
//! dispatch (the schema family), DSL evaluation with the cross-file `key()` index (the rule DSL),
//! and rustc-shaped + JSON output. See CHANGELOG.md for the surface shipped per
//! release (this crate is versioned in `Cargo.toml`, not pinned in this header).

// Test code opts out of the panic-path restriction lints ([lints.clippy] in
// Cargo.toml, #185): in a test an unwrap IS the assertion. Production code
// stays under them.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub(crate) mod atomic;
pub(crate) mod cite;
#[doc(hidden)]
pub mod codecat;
#[doc(hidden)]
pub mod codes;
#[doc(hidden)]
pub mod config;
#[doc(hidden)]
pub mod confine;
pub(crate) mod dep;
#[doc(hidden)]
pub mod diagnostic;
#[doc(hidden)]
pub mod dsl;
pub(crate) mod error;
pub(crate) mod format_version;
#[doc(hidden)]
pub mod frontmatter;
pub(crate) mod globs;
#[doc(hidden)]
pub mod init;
#[doc(hidden)]
pub mod limits;
pub(crate) mod link;
// External-link checks (#215): the offline side of absolute URLs — the
// register, well-formedness, route policy, and the export.
#[doc(hidden)]
pub mod links;
pub(crate) mod marker;
pub(crate) mod markup;
pub(crate) mod memo;
#[doc(hidden)]
pub mod output;
#[doc(hidden)]
pub mod pin;
#[doc(hidden)]
pub mod route;
#[doc(hidden)]
pub mod schema;
pub(crate) mod section;
#[doc(hidden)]
pub mod snapshot;
#[doc(hidden)]
pub mod verify;
#[doc(hidden)]
pub mod vocab;
#[doc(hidden)]
pub mod yaml;

// Never-panic properties over the crate-private body-text scanners (#193):
// test-only, in-crate because the scanners are pub(crate); CI's
// parser-robustness job runs `cargo test --lib robustness::` beside the
// public-API harness in tests/parser_robustness.rs.
#[cfg(test)]
mod robustness;

#[doc(hidden)]
pub use diagnostic::{Finding, Location, Severity};
#[doc(hidden)]
pub use dsl::{parse_pattern_file, PatternFile};
#[doc(hidden)]
pub use error::Error;
#[doc(hidden)]
pub use schema::{Schema, ValidationError};
#[doc(hidden)]
pub use verify::{verify, VerifyConfig, VerifyError};
