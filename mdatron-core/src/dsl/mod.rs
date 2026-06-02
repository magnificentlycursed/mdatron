//! Schematron-derived DSL: pattern files parsed into typed AST.
//!
//! v0.1.x ships the parser + AST types. Expression evaluation, the standard library,
//! and cross-file index land in subsequent iterations. Expressions are stored as
//! `String` for now; the parser does NOT validate their internal syntax.
//!
//! See DESIGN-MDATRON.md § DSL specification for the full surface.

pub mod parser;
pub mod types;

pub use parser::parse_pattern_file;
pub use types::{
    ContextSelector, KeyDecl, LocationSpec, Pattern, PatternFile, Rule,
};
