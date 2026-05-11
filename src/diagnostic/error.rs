//! Compilation error type.

use thiserror::Error;
use typst::World;
use typst::diag::SourceDiagnostic;

use super::info::Diagnostics;
use crate::world::SnapshotError;

/// Error type for Typst compilation failures.
///
/// This provides structured access to compilation errors for programmatic handling,
/// while also implementing `Display` for human-readable output.
///
/// # Example
///
/// ```ignore
/// match compile_html(path, root) {
///     Ok(result) => { /* success */ }
///     Err(CompileError::Compilation { diagnostics, .. }) => {
///         // Access individual errors
///         for diag in diagnostics.errors() {
///             eprintln!("Error: {}", diag.message);
///         }
///     }
///     Err(CompileError::HtmlExport { message }) => {
///         eprintln!("HTML export failed: {message}");
///     }
///     Err(e) => eprintln!("{e}"),
/// }
/// ```
#[derive(Debug, Error)]
pub enum CompileError {
    /// Typst compilation failed with diagnostics.
    #[error("{diagnostics}")]
    Compilation {
        /// The resolved diagnostics.
        diagnostics: Diagnostics,
    },

    /// HTML export failed.
    #[error("HTML export failed: {message}")]
    HtmlExport {
        /// Error message from typst_html.
        message: String,
    },

    /// File I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Snapshot build error.
    #[error("snapshot error: {0}")]
    Snapshot(#[from] SnapshotError),
}

impl CompileError {
    /// Create a compilation error from raw diagnostics.
    pub fn compilation<W: World>(world: &W, raw_diagnostics: Vec<SourceDiagnostic>) -> Self {
        Self::compilation_with_offset(world, raw_diagnostics, 0)
    }

    /// Create a compilation error with line offset for main file.
    pub fn compilation_with_offset<W: World>(
        world: &W,
        raw_diagnostics: Vec<SourceDiagnostic>,
        main_line_offset: usize,
    ) -> Self {
        let diagnostics =
            Diagnostics::resolve_with_offset(world, &raw_diagnostics, main_line_offset);
        Self::Compilation { diagnostics }
    }

    /// Create an HTML export error.
    pub fn html_export(message: impl Into<String>) -> Self {
        Self::HtmlExport {
            message: message.into(),
        }
    }

    /// Check if this error contains any fatal errors (vs just warnings).
    pub fn has_fatal_errors(&self) -> bool {
        match self {
            Self::Compilation { diagnostics } => diagnostics.has_errors(),
            _ => true,
        }
    }

    /// Get the diagnostics if this is a compilation error.
    pub fn diagnostics(&self) -> Option<&Diagnostics> {
        match self {
            Self::Compilation { diagnostics } => Some(diagnostics),
            _ => None,
        }
    }
}
