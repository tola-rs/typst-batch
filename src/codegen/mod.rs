//! Typst code generation and type conversion.
//!
//! Provides bidirectional conversion between Rust/JSON and Typst types.
//!
//! # Modules
//!
//! - [`serialize`] - Typst → JSON conversion
//! - [`deserialize`] - JSON → Typst conversion
//! - [`lookup`] - Function lookup utilities
//! - [`source`] - Rust → Typst source code generation
//! - [`builder`] - Dict/Array construction helpers
//! - [`inputs`] - sys.inputs builder from JSON
//! - [`error`] - Error types

mod builder;
mod deserialize;
mod error;
mod inputs;
mod literal;
mod lookup;
mod roundtrip;
mod serialize;
mod source;

// Serialization (Typst → JSON)
pub use serialize::{content_to_json, json_to_simple_text, value_to_json};

// Deserialization (JSON → Typst)
pub use deserialize::{json_to_content, json_to_value};

// Literal parsing (string → Typst value)
pub use literal::{parse_angle, parse_color, parse_length, parse_ratio, parse_typst_literal};

// Source generation
pub use source::ToTypst;

// Builders
pub use builder::{DictBuilder, array, array_raw, dict, dict_raw, dict_sparse, format_array};

// Inputs
pub use inputs::{Inputs, json_to_simple_value};

// Errors
pub use error::ConvertError;
