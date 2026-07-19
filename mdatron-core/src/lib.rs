//! mdatron-core — validator engine for the mdatron typed-markdown validation toolkit.
//!
//! Two-layer architecture per DESIGN-MDATRON.md: JSON Schema for structural validation
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
pub mod diagnostic;
pub mod dsl;
pub mod error;
pub mod frontmatter;
pub mod schema;
pub mod verify;
pub mod output;

pub use diagnostic::{Finding, Location, Severity};
pub use dsl::{parse_pattern_file, PatternFile};
pub use error::Error;
pub use schema::{Schema, ValidationError};
pub use verify::{verify, VerifyConfig, VerifyError};
