//! mdatron engine internals — **not a public API**.
//!
//! mdatron is consumed **as a binary** (#81, operator ruling 2026-07-22
//! executing the 2026-06-02 binary-first directive): the machine interface is
//! `mdatron verify --json` / `mdatron explain --json`, with version discipline
//! on the JSON envelope (DESIGN.md § Machine output is a public interface).
//! This lib target exists only so unit tests, the integration suites under
//! `tests/`, and the load-bearing `compile_fail` doctests (confine, #53) can
//! link the engine; it carries **no API-stability promise** and is not a
//! supported consumption surface. Shell out to the binary instead.
//!
//! Two-layer architecture per DESIGN.md § Summary: JSON Schema for structural validation
//! (Layer 1); a Schematron-derived DSL for cross-field, cross-file, and cross-document
//! semantic rules (Layer 2).
//!
//! mdatron is descended from Schematron (ISO/IEC 19757-3). It is **not** related to the
//! TRON blockchain. The `-tron` suffix evokes Schematron, the same way `jsontron` did for JSON.
//!
//! # Implementation state (v0.1.0)
//!
//! The verify pipeline is implemented end to end: frontmatter parsing, JSON Schema
//! dispatch (Layer 1), DSL evaluation with the cross-file `key()` index (Layer 2),
//! and rustc-shaped + JSON output. See CHANGELOG.md for the surface shipped per release.

pub mod codes;
pub mod config;
pub mod confine;
pub mod diagnostic;
pub mod dsl;
pub mod error;
pub mod frontmatter;
pub mod init;
pub mod output;
pub mod schema;
pub mod verify;

pub use diagnostic::{Finding, Location, Severity};
pub use dsl::{parse_pattern_file, PatternFile};
pub use error::Error;
pub use schema::{Schema, ValidationError};
pub use verify::{verify, VerifyConfig, VerifyError};
