//! Diagnostic filtering utilities.

use typst::diag::{Severity, SourceDiagnostic};

/// Specifies which packages to filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageKind {
    /// All preview packages (`@preview/*`).
    AllPreview,
    /// All local packages (`@local/*`).
    AllLocal,
    /// Specific packages by full path.
    ///
    /// - `"@preview/cetz"` matches cetz from preview namespace
    /// - `"@local/mylib"` matches mylib from local namespace
    /// - `"@myapp"` matches all myapp packages
    Specific(Vec<String>),
}

impl PackageKind {
    /// Create a filter for specific packages.
    pub fn specific<I, S>(packages: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::Specific(
            packages
                .into_iter()
                .map(|s| s.as_ref().to_string())
                .collect(),
        )
    }

    /// Check if a diagnostic matches this package filter.
    fn matches(&self, diag: &SourceDiagnostic) -> bool {
        let Some(pkg) = diag.span.id().and_then(|id| id.package()) else {
            return false;
        };
        let full_path = format!("@{}/{}", pkg.namespace, pkg.name);
        match self {
            PackageKind::AllPreview => full_path.starts_with("@preview/"),
            PackageKind::AllLocal => full_path.starts_with("@local/"),
            PackageKind::Specific(patterns) => patterns.iter().any(|p| full_path.starts_with(p)),
        }
    }
}

/// Filter type for matching diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterType {
    /// Match all diagnostics.
    All,
    /// Match HTML export development warning.
    HtmlExport,
    /// Match diagnostics from specific packages.
    Package(PackageKind),
    /// Match diagnostics containing specific text in message.
    MessageContains(String),
}

impl FilterType {
    /// Check if a diagnostic matches this filter type.
    fn matches(&self, diag: &SourceDiagnostic) -> bool {
        match self {
            FilterType::All => true,
            FilterType::HtmlExport => diag
                .message
                .contains("html export is under active development"),
            FilterType::Package(kind) => kind.matches(diag),
            FilterType::MessageContains(text) => diag.message.contains(text.as_str()),
        }
    }
}

/// Filter for excluding diagnostics.
///
/// Combines severity and filter type for precise control.
///
/// # Example
///
/// ```ignore
/// use typst::diag::Severity;
/// use typst_batch::diagnostic::{DiagnosticFilter, FilterType, PackageKind};
///
/// // Filter all warnings
/// let filter = DiagnosticFilter::new(Severity::Warning, FilterType::All);
///
/// // Filter errors from specific packages
/// let filter = DiagnosticFilter::new(
///     Severity::Error,
///     FilterType::Package(PackageKind::specific(["@myapp/pages"])),
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticFilter {
    /// The severity to match (Error or Warning).
    pub severity: Severity,
    /// The filter type to apply.
    pub filter: FilterType,
}

impl DiagnosticFilter {
    /// Create a new diagnostic filter.
    pub fn new(severity: Severity, filter: FilterType) -> Self {
        Self { severity, filter }
    }

    /// Check if a SourceDiagnostic should be filtered out.
    pub(crate) fn matches(&self, diag: &SourceDiagnostic) -> bool {
        diag.severity == self.severity && self.filter.matches(diag)
    }
}

/// Filter out known HTML export development warnings.
///
/// Typst's HTML export is experimental and always produces a warning.
/// This function filters out that warning to reduce noise in error output.
pub fn filter_html_warnings(diagnostics: &[SourceDiagnostic]) -> Vec<SourceDiagnostic> {
    let filter = DiagnosticFilter::new(Severity::Warning, FilterType::HtmlExport);
    diagnostics
        .iter()
        .filter(|d| !filter.matches(d))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::syntax::Span;

    #[test]
    fn test_filter_html_warnings() {
        let diags = vec![
            SourceDiagnostic::error(Span::detached(), "error 1"),
            SourceDiagnostic::warning(Span::detached(), "html export is under active development"),
            SourceDiagnostic::warning(Span::detached(), "other warning"),
        ];

        let filtered = filter_html_warnings(&diags);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|d| d.message == "error 1"));
        assert!(filtered.iter().any(|d| d.message == "other warning"));
    }

    #[test]
    fn test_filter_all_warnings() {
        let diags = vec![
            SourceDiagnostic::error(Span::detached(), "error 1"),
            SourceDiagnostic::warning(Span::detached(), "warning 1"),
            SourceDiagnostic::warning(Span::detached(), "warning 2"),
        ];

        let filter = DiagnosticFilter::new(Severity::Warning, FilterType::All);
        let filtered: Vec<_> = diags.iter().filter(|d| !filter.matches(d)).collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].message, "error 1");
    }

    #[test]
    fn test_filter_message_contains() {
        let diags = vec![
            SourceDiagnostic::error(Span::detached(), "error with keyword"),
            SourceDiagnostic::error(Span::detached(), "other error"),
        ];

        let filter = DiagnosticFilter::new(
            Severity::Error,
            FilterType::MessageContains("keyword".into()),
        );
        let filtered: Vec<_> = diags.iter().filter(|d| !filter.matches(d)).collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].message, "other error");
    }
}
