//! Schematron-derived DSL: pattern files parsed into a typed AST and evaluated.
//!
//! Implemented: the pattern-file parser (`parser`), the expression parser
//! (`expr_parser`) and evaluator (`expr`) with quantifiers (`every`, `some`, `in`),
//! the value/collection builtins (`defined`, `count`, `len`, `union`, `intersect`,
//! `difference`, `concat`, `join`), arithmetic and comparison, `let:` bindings,
//! `{{expr}}` message interpolation, and the path-confined cross-file `key()` index
//! (`index`). Scope is cross-file and registry validation; body-content extraction
//! is out of scope (see the DSL falsifiability report and V1-SHIP-CRITERIA).
//!
//! See DESIGN-MDATRON.md § DSL specification for the implemented surface.

pub mod expr;
pub mod expr_parser;
pub mod index;
pub mod parser;
pub mod types;

pub use expr::{evaluate, EvalContext, EvalError, Expr, Value, VarRef};
pub use expr_parser::{parse_expression, ParseError as ExprParseError};
pub use index::{Index, IndexError, IndexRegistry};
pub use parser::parse_pattern_file;
pub use types::{
    ContextSelector, KeyDecl, LocationSpec, Pattern, PatternFile, Rule,
};
