//! Internal errors for mdatron engine operations.
//!
//! These are engine-internal errors (e.g. IO failures, config-load failures). They are
//! distinct from [`crate::diagnostic::Finding`]s, which are validation outcomes the engine
//! reports to operators.
//!
//! Phase 2a Red Gate: variants declared; Display via `thiserror` is sufficient (no stubs
//! required for the error type itself since variants are pure data).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("yaml parse error: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),

    /// The YAML was refused before parsing: its flow collections nest past the
    /// declared structural-nesting bound (#244).
    #[error("yaml refused: flow collections nest {depth} deep (limit {limit})")]
    NestingTooDeep { depth: usize, limit: usize },

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("config error: {0}")]
    Config(String),
}

impl From<crate::yaml::YamlError> for Error {
    fn from(e: crate::yaml::YamlError) -> Self {
        match e {
            crate::yaml::YamlError::TooDeep { depth, limit } => {
                Error::NestingTooDeep { depth, limit }
            }
            crate::yaml::YamlError::Parse(e) => Error::Yaml(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_error_display_includes_message() {
        let err = Error::Config("missing required field".into());
        let displayed = format!("{err}");
        assert!(displayed.contains("missing required field"));
        assert!(displayed.contains("config error"));
    }
}
