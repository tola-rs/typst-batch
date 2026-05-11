//! Diagnostic formatting for Typst compilation errors and warnings.

mod error;
mod filter;
mod format;
mod info;

// Re-export all public types
pub use error::CompileError;
pub use filter::{DiagnosticFilter, FilterType, PackageKind, filter_html_warnings};
pub use format::{
    DiagnosticOptions, DisplayStyle, format_diagnostics, format_diagnostics_with_options,
};
pub use info::{
    DiagnosticInfo, DiagnosticInfoDisplay, DiagnosticSummary, Diagnostics, DiagnosticsDisplay,
    SourceLine, TraceInfo, count_diagnostics, has_errors, resolve_diagnostic,
    resolve_diagnostic_with_offset, resolve_diagnostics,
};

// Re-export from typst for user convenience
pub use typst::diag::{Severity as DiagnosticSeverity, SourceDiagnostic};
