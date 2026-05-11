//! Structured diagnostic information for custom rendering.

use std::fmt;

use typst::World;
use typst::diag::{Severity, SourceDiagnostic};

use super::filter::DiagnosticFilter;
use super::format::{DiagnosticOptions, SpanLocation, format_info};

// ============================================================================
// DiagnosticSummary
// ============================================================================

/// Summary of diagnostic counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiagnosticSummary {
    /// Number of errors.
    pub errors: usize,
    /// Number of warnings.
    pub warnings: usize,
}

impl DiagnosticSummary {
    /// Create summary from diagnostics.
    pub fn from_diagnostics(diagnostics: &[SourceDiagnostic]) -> Self {
        let (errors, warnings) = count_diagnostics(diagnostics);
        Self { errors, warnings }
    }

    /// Total number of diagnostics.
    pub fn total(&self) -> usize {
        self.errors + self.warnings
    }

    /// Whether there are any errors.
    pub fn has_errors(&self) -> bool {
        self.errors > 0
    }

    /// Whether there are any diagnostics at all.
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

impl fmt::Display for DiagnosticSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.errors, self.warnings) {
            (0, 0) => write!(f, "no diagnostics"),
            (e, 0) => write!(f, "{e} error{}", if e == 1 { "" } else { "s" }),
            (0, w) => write!(f, "{w} warning{}", if w == 1 { "" } else { "s" }),
            (e, w) => write!(
                f,
                "{e} error{}, {w} warning{}",
                if e == 1 { "" } else { "s" },
                if w == 1 { "" } else { "s" }
            ),
        }
    }
}

// ============================================================================
// Diagnostics (Collection)
// ============================================================================

/// A collection of resolved diagnostic information.
///
/// All diagnostics are pre-resolved with source locations,
/// so no `World` is needed for formatting.
///
/// # Example
///
/// ```ignore
/// let result = compile_document(path, root)?;
///
/// // Check for warnings
/// if !result.diagnostics.is_empty() {
///     // Format with default options
///     eprintln!("{}", result.diagnostics);
///
///     // Or iterate for custom handling
///     for diag in result.diagnostics.iter() {
///         println!("{}: {}", diag.severity_str(), diag.message);
///     }
/// }
/// ```
#[derive(Debug, Clone, Default)]
pub struct Diagnostics {
    /// Resolved diagnostic info for display
    items: Vec<DiagnosticInfo>,
    /// Original diagnostics for filtering (preserves span info)
    raw: Vec<SourceDiagnostic>,
}

impl Diagnostics {
    /// Create an empty diagnostics collection.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            raw: Vec::new(),
        }
    }

    /// Create from a vector of diagnostic info (without raw diagnostics).
    pub fn from_vec(items: Vec<DiagnosticInfo>) -> Self {
        Self {
            items,
            raw: Vec::new(),
        }
    }

    /// Resolve diagnostics from raw `SourceDiagnostic` using a World.
    ///
    /// This is called internally during compilation while the World is still available.
    pub fn resolve<W: World>(world: &W, diagnostics: &[SourceDiagnostic]) -> Self {
        Self::resolve_with_offset(world, diagnostics, 0)
    }

    /// Resolve diagnostics with line offset for main file.
    ///
    /// The `main_line_offset` is subtracted from line numbers when the span
    /// belongs to the main file. This is used to correct line numbers when
    /// a prelude has been injected at the beginning of the main file.
    pub fn resolve_with_offset<W: World>(
        world: &W,
        diagnostics: &[SourceDiagnostic],
        main_line_offset: usize,
    ) -> Self {
        let items = diagnostics
            .iter()
            .map(|d| resolve_diagnostic_with_offset(world, d, main_line_offset))
            .collect();
        Self {
            items,
            raw: diagnostics.to_vec(),
        }
    }

    /// Check if there are no diagnostics.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get the number of diagnostics.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }

    /// Check if there are any warnings.
    pub fn has_warnings(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Warning)
    }

    /// Count errors.
    pub fn error_count(&self) -> usize {
        self.items
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    /// Count warnings.
    pub fn warning_count(&self) -> usize {
        self.items
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }

    /// Get a summary of diagnostic counts.
    pub fn summary(&self) -> DiagnosticSummary {
        DiagnosticSummary {
            errors: self.error_count(),
            warnings: self.warning_count(),
        }
    }

    /// Iterate over all diagnostics.
    pub fn iter(&self) -> impl Iterator<Item = &DiagnosticInfo> {
        self.items.iter()
    }

    /// Iterate over errors only.
    pub fn errors(&self) -> impl Iterator<Item = &DiagnosticInfo> {
        self.items.iter().filter(|d| d.severity == Severity::Error)
    }

    /// Iterate over warnings only.
    pub fn warnings(&self) -> impl Iterator<Item = &DiagnosticInfo> {
        self.items
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }

    /// Format with custom options.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use typst_batch::{Diagnostics, DiagnosticOptions};
    ///
    /// // Default formatting
    /// println!("{}", diagnostics);
    ///
    /// // Custom options
    /// let options = DiagnosticOptions::plain();
    /// println!("{}", diagnostics.with_options(&options));
    /// ```
    pub fn with_options<'a>(&'a self, options: DiagnosticOptions) -> DiagnosticsDisplay<'a> {
        DiagnosticsDisplay {
            diagnostics: self,
            options,
            max_errors: None,
        }
    }

    /// Format with a limit on the number of errors displayed.
    ///
    /// This is useful when a single syntax error causes many cascading errors.
    /// Only the first `max` errors will be shown; warnings are not affected.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Show only the first error (useful for syntax errors that cascade)
    /// println!("{}", diagnostics.with_max_errors(1));
    /// ```
    pub fn with_max_errors(&self, max: usize) -> DiagnosticsDisplay<'_> {
        DiagnosticsDisplay {
            diagnostics: self,
            options: DiagnosticOptions::default(),
            max_errors: Some(max),
        }
    }

    /// Convert to a vector of diagnostic info.
    pub fn into_vec(self) -> Vec<DiagnosticInfo> {
        self.items
    }

    /// Get a slice of all diagnostics.
    pub fn as_slice(&self) -> &[DiagnosticInfo] {
        &self.items
    }

    /// Filter diagnostics, keeping only those that pass the predicate.
    pub fn filter<F>(&self, predicate: F) -> Self
    where
        F: Fn(&DiagnosticInfo) -> bool,
    {
        let keep_indices: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, d)| predicate(d))
            .map(|(i, _)| i)
            .collect();

        Self {
            items: keep_indices
                .iter()
                .map(|&i| self.items[i].clone())
                .collect(),
            raw: keep_indices
                .iter()
                .filter_map(|&i| self.raw.get(i).cloned())
                .collect(),
        }
    }

    /// Filter out diagnostics matching any of the given filters.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use typst_batch::{Diagnostics, DiagnosticFilter};
    ///
    /// let filtered = diagnostics.filter_out(&[
    ///     DiagnosticFilter::HtmlExport,
    ///     DiagnosticFilter::ExternalPackages,
    /// ]);
    /// ```
    pub fn filter_out(&self, filters: &[DiagnosticFilter]) -> Self {
        // Use raw diagnostics for filtering (preserves span/package info)
        let keep_indices: Vec<usize> = self
            .raw
            .iter()
            .enumerate()
            .filter(|(_, d)| !filters.iter().any(|f| f.matches(d)))
            .map(|(i, _)| i)
            .collect();

        Self {
            items: keep_indices
                .iter()
                .map(|&i| self.items[i].clone())
                .collect(),
            raw: keep_indices.iter().map(|&i| self.raw[i].clone()).collect(),
        }
    }

    /// Filter out diagnostics from external packages.
    ///
    /// Keeps only diagnostics from user code (paths not starting with `@`).
    pub fn filter_external_packages(&self) -> Self {
        self.filter(|d| {
            // Keep if no path, or path doesn't start with @
            d.path.as_ref().map(|p| !p.starts_with('@')).unwrap_or(true)
        })
    }
}

impl IntoIterator for Diagnostics {
    type Item = DiagnosticInfo;
    type IntoIter = std::vec::IntoIter<DiagnosticInfo>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a> IntoIterator for &'a Diagnostics {
    type Item = &'a DiagnosticInfo;
    type IntoIter = std::slice::Iter<'a, DiagnosticInfo>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl fmt::Display for Diagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        DiagnosticsDisplay {
            diagnostics: self,
            options: DiagnosticOptions::default(),
            max_errors: None,
        }
        .fmt(f)
    }
}

/// Display wrapper for formatting diagnostics with custom options.
pub struct DiagnosticsDisplay<'a> {
    diagnostics: &'a Diagnostics,
    options: DiagnosticOptions,
    max_errors: Option<usize>,
}

impl fmt::Display for DiagnosticsDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Sort: errors first, then warnings
        let mut sorted: Vec<_> = self.diagnostics.items.iter().collect();
        sorted.sort_by_key(|d| match d.severity {
            Severity::Error => 0,
            Severity::Warning => 1,
        });

        // Apply max_errors limit if set
        let display_items: Vec<_> = if let Some(max) = self.max_errors {
            let mut error_count = 0;
            sorted
                .into_iter()
                .filter(|d| {
                    if d.severity == Severity::Error {
                        error_count += 1;
                        error_count <= max
                    } else {
                        true // Keep all warnings
                    }
                })
                .collect()
        } else {
            sorted
        };

        for (i, diag) in display_items.iter().enumerate() {
            let mut output = String::new();
            format_info(&mut output, diag, self.options);
            f.write_str(&output)?;
            if i < display_items.len() - 1 {
                f.write_str("\n")?;
            }
        }

        Ok(())
    }
}

// ============================================================================
// DiagnosticInfo
// ============================================================================

/// Structured diagnostic information for custom rendering.
///
/// This struct contains all the information needed to render a diagnostic
/// in any format (terminal, HTML, JSON, etc.).
///
/// # Example
///
/// ```ignore
/// use typst_batch::diagnostic::{DiagnosticInfo, resolve_diagnostic};
///
/// let info = resolve_diagnostic(&world, &diag);
///
/// // Now you have full control over formatting
/// println!("Error at {}:{}", info.path.unwrap_or("?"), info.line.unwrap_or(0));
/// for line in &info.source_lines {
///     println!("{}: {}", line.line_num, line.text);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct DiagnosticInfo {
    /// Error severity (error or warning).
    pub severity: Severity,
    /// The error message.
    pub message: String,
    /// File path (if available).
    pub path: Option<String>,
    /// Line number (1-indexed, if available).
    pub line: Option<usize>,
    /// Column number (1-indexed, if available).
    pub column: Option<usize>,
    /// Source code lines with highlighting info.
    pub source_lines: Vec<SourceLine>,
    /// Hint messages.
    pub hints: Vec<String>,
    /// Stack trace entries.
    pub traces: Vec<TraceInfo>,
}

impl DiagnosticInfo {
    /// Format with custom options.
    pub fn with_options<'a>(&'a self, options: DiagnosticOptions) -> DiagnosticInfoDisplay<'a> {
        DiagnosticInfoDisplay {
            info: self,
            options,
        }
    }
}

/// Display wrapper for formatting a single diagnostic with custom options.
pub struct DiagnosticInfoDisplay<'a> {
    info: &'a DiagnosticInfo,
    options: DiagnosticOptions,
}

impl fmt::Display for DiagnosticInfoDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut output = String::new();
        format_info(&mut output, self.info, self.options);
        f.write_str(&output)
    }
}

impl fmt::Display for DiagnosticInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.with_options(DiagnosticOptions::default()).fmt(f)
    }
}

/// A source code line with optional highlighting.
#[derive(Debug, Clone)]
pub struct SourceLine {
    /// Line number (1-indexed).
    pub line_num: usize,
    /// The source text.
    pub text: String,
    /// Highlight range (start_col, end_col), 0-indexed.
    pub highlight: Option<(usize, usize)>,
}

/// Stack trace entry.
#[derive(Debug, Clone)]
pub struct TraceInfo {
    /// Description of the trace point (from Typst's Tracepoint::Display).
    pub message: String,
    /// File path (if available).
    pub path: Option<String>,
    /// Line number (1-indexed, if available).
    pub line: Option<usize>,
    /// Column number (1-indexed, if available).
    pub column: Option<usize>,
    /// Source code lines at this trace point.
    pub source_lines: Vec<SourceLine>,
}

// ============================================================================
// Resolution Functions
// ============================================================================

/// Resolve a diagnostic to structured info for custom rendering.
pub fn resolve_diagnostic<W: World>(world: &W, diag: &SourceDiagnostic) -> DiagnosticInfo {
    resolve_diagnostic_with_offset(world, diag, 0)
}

/// Resolve a diagnostic with line offset for main file.
///
/// The `main_line_offset` is subtracted from line numbers when the span
/// belongs to the main file. This is used to correct line numbers when
/// a prelude has been injected at the beginning of the main file.
pub fn resolve_diagnostic_with_offset<W: World>(
    world: &W,
    diag: &SourceDiagnostic,
    main_line_offset: usize,
) -> DiagnosticInfo {
    let location = SpanLocation::from_span_with_offset(world, diag.span, main_line_offset);

    let (path, line, column, source_lines) = match &location {
        Some(loc) => (
            Some(loc.path.clone()),
            Some(loc.start_line),
            Some(loc.start_col + 1), // 1-indexed for display
            loc.to_source_lines(),
        ),
        None => (None, None, None, vec![]),
    };

    let hints = diag.hints.iter().map(|h| h.to_string()).collect();

    let traces = diag
        .trace
        .iter()
        .filter_map(|t| {
            use typst::diag::Tracepoint;

            // Skip import traces - they just show content importing template
            if matches!(t.v, Tracepoint::Import) {
                return None;
            }

            // Use Tracepoint's Display impl for consistent messages
            let message = t.v.to_string();

            let loc = SpanLocation::from_span_with_offset(world, t.span, main_line_offset);
            Some(TraceInfo {
                message,
                path: loc.as_ref().map(|l| l.path.clone()),
                line: loc.as_ref().map(|l| l.start_line),
                column: loc.as_ref().map(|l| l.start_col + 1),
                source_lines: loc
                    .as_ref()
                    .map(|l| l.to_source_lines())
                    .unwrap_or_default(),
            })
        })
        .collect();

    DiagnosticInfo {
        severity: diag.severity,
        message: diag.message.to_string(),
        path,
        line,
        column,
        source_lines,
        hints,
        traces,
    }
}

/// Resolve all diagnostics to structured info.
#[allow(dead_code)]
pub fn resolve_diagnostics<W: World>(
    world: &W,
    diagnostics: &[SourceDiagnostic],
) -> Vec<DiagnosticInfo> {
    diagnostics
        .iter()
        .map(|d| resolve_diagnostic(world, d))
        .collect()
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Count errors and warnings in a diagnostic list.
pub fn count_diagnostics(diagnostics: &[SourceDiagnostic]) -> (usize, usize) {
    diagnostics
        .iter()
        .fold((0, 0), |(errors, warnings), d| match d.severity {
            Severity::Error => (errors + 1, warnings),
            Severity::Warning => (errors, warnings + 1),
        })
}

/// Check if there are any errors in the diagnostics.
pub fn has_errors(diagnostics: &[SourceDiagnostic]) -> bool {
    diagnostics.iter().any(|d| d.severity == Severity::Error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::syntax::Span;

    #[test]
    fn test_count_diagnostics() {
        let diags = vec![
            SourceDiagnostic::error(Span::detached(), "error 1"),
            SourceDiagnostic::error(Span::detached(), "error 2"),
            SourceDiagnostic::warning(Span::detached(), "warning 1"),
            SourceDiagnostic::warning(Span::detached(), "warning 2"),
        ];

        let (errors, warnings) = count_diagnostics(&diags);
        assert_eq!(errors, 2);
        assert_eq!(warnings, 2);
    }

    #[test]
    fn test_has_errors() {
        let warnings_only = vec![
            SourceDiagnostic::warning(Span::detached(), "warning 1"),
            SourceDiagnostic::warning(Span::detached(), "warning 2"),
        ];
        assert!(!has_errors(&warnings_only));

        let with_errors = vec![
            SourceDiagnostic::warning(Span::detached(), "warning 1"),
            SourceDiagnostic::error(Span::detached(), "error 1"),
        ];
        assert!(has_errors(&with_errors));

        let empty: Vec<SourceDiagnostic> = vec![];
        assert!(!has_errors(&empty));
    }

    #[test]
    fn test_diagnostic_summary_display() {
        assert_eq!(
            DiagnosticSummary {
                errors: 0,
                warnings: 0
            }
            .to_string(),
            "no diagnostics"
        );
        assert_eq!(
            DiagnosticSummary {
                errors: 1,
                warnings: 0
            }
            .to_string(),
            "1 error"
        );
        assert_eq!(
            DiagnosticSummary {
                errors: 2,
                warnings: 0
            }
            .to_string(),
            "2 errors"
        );
        assert_eq!(
            DiagnosticSummary {
                errors: 0,
                warnings: 1
            }
            .to_string(),
            "1 warning"
        );
        assert_eq!(
            DiagnosticSummary {
                errors: 1,
                warnings: 2
            }
            .to_string(),
            "1 error, 2 warnings"
        );
    }
}
