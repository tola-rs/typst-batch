//! Error types for codegen operations.

use thiserror::Error;

/// Error during JSON to Typst conversion.
#[derive(Debug, Error)]
pub enum ConvertError {
    /// Missing required "func" field in JSON object.
    #[error("missing 'func' field in JSON object")]
    MissingFunc,

    /// JSON is not an object.
    #[error("expected JSON object, got {0}")]
    NotObject(&'static str),

    /// Content that cannot be reconstructed from Typst query JSON.
    #[error("unsupported content at {path}: {func}")]
    Unsupported {
        /// Slash-separated path where the content was found.
        path: String,
        /// Typst content function name.
        func: String,
    },

    /// Missing required field.
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    /// Function call failed.
    #[error("function call failed for '{func}': {reason}")]
    CallFailed {
        /// Function name.
        func: String,
        /// Error reason.
        reason: String,
    },

    /// Value conversion failed.
    #[error("failed to convert value")]
    ValueConversion,

    /// Invalid literal value for given type.
    #[error("invalid {type_name} literal: '{value}'")]
    InvalidLiteral {
        /// Expected type name.
        type_name: &'static str,
        /// The invalid value string.
        value: String,
    },

    /// Unknown type tag in `_typst_type` field.
    #[error("unknown type tag: '{0}'")]
    UnknownTypeTag(String),

    /// Generic error with message.
    #[error("{0}")]
    Other(String),
}
