//! The one entry point for YAML parsing of file content (#244).
//!
//! serde_yaml_ng's recursion guard fires only after a scan quadratic in
//! flow-collection nesting depth, so a committed `x: [[[…]]]` 100k deep cost
//! tens of seconds per parse — in a link target, an index source, or any
//! `.mdatron/` governance file. [`from_str`] refuses such input with
//! [`crate::limits::check_flow_nesting`] BEFORE parsing. Every non-test parse
//! goes through it; `clippy.toml` disallows calling `serde_yaml_ng::from_str`
//! directly, so a new call site cannot skip the bound.

use serde::de::DeserializeOwned;

/// A refused or failed YAML parse. `Display` is the bare reason; callers add
/// which file it was, as they did for the parser's own error.
#[derive(Debug, thiserror::Error)]
pub enum YamlError {
    #[error("flow collections nest {depth} deep (limit {limit})")]
    TooDeep { depth: usize, limit: usize },

    #[error(transparent)]
    Parse(#[from] serde_yaml_ng::Error),
}

/// `serde_yaml_ng::from_str`, behind the structural-nesting bound.
pub fn from_str<T: DeserializeOwned>(s: &str) -> Result<T, YamlError> {
    crate::limits::check_flow_nesting(s).map_err(|depth| YamlError::TooDeep {
        depth,
        limit: crate::limits::SHIPPED.structural_nesting,
    })?;
    #[allow(clippy::disallowed_methods)] // the one sanctioned call
    serde_yaml_ng::from_str(s).map_err(YamlError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_past_the_bound_and_parses_at_it() {
        let limit = crate::limits::SHIPPED.structural_nesting;
        let nest = |d: usize| format!("x: {}{}", "[".repeat(d), "]".repeat(d));
        match from_str::<serde_yaml_ng::Value>(&nest(limit + 1)) {
            Err(YamlError::TooDeep { depth, limit: l }) => {
                assert_eq!((depth, l), (limit + 1, limit))
            }
            other => panic!("expected TooDeep; got {other:?}"),
        }
        assert!(!matches!(
            from_str::<serde_yaml_ng::Value>(&nest(limit)),
            Err(YamlError::TooDeep { .. })
        ));
        assert_eq!(
            from_str::<serde_yaml_ng::Value>("a: 1").unwrap()["a"],
            serde_yaml_ng::Value::from(1)
        );
    }
}
