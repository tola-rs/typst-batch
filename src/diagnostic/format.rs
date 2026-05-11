//! Diagnostic formatting utilities.

use std::fmt::Write;

use typst::World;
use typst::diag::{Severity, SourceDiagnostic};
use typst::syntax::Span;

use super::info::{DiagnosticInfo, SourceLine, TraceInfo};

// ============================================================================
// Options
// ============================================================================

/// Display style for diagnostic output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayStyle {
    /// Rich output with source snippets and highlighting.
    #[default]
    Rich,
    /// Short output with just file:line:col and message.
    Short,
}

/// Options for controlling diagnostic formatting.
///
/// # Example
///
/// ```ignore
/// use typst_batch::diagnostic::{DiagnosticOptions, DisplayStyle};
///
/// // Default: colored rich output
/// let opts = DiagnosticOptions::default();
///
/// // Plain text (no ANSI colors) for logging
/// let opts = DiagnosticOptions::plain();
///
/// // Short format for CI/IDE integration
/// let opts = DiagnosticOptions::short();
///
/// // Custom configuration
/// let opts = DiagnosticOptions::default()
///     .with_color(true)
///     .with_style(DisplayStyle::Rich)
///     .with_snippets(true);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct DiagnosticOptions {
    /// Whether to use ANSI colors in output.
    pub colored: bool,
    /// Display style (rich with snippets or short).
    pub style: DisplayStyle,
    /// Whether to include source code snippets.
    pub snippets: bool,
    /// Whether to include hints.
    pub hints: bool,
    /// Whether to include trace information.
    pub traces: bool,
    /// Tab width for display (default: 2).
    pub tab_width: usize,
}

impl Default for DiagnosticOptions {
    fn default() -> Self {
        Self {
            colored: true,
            style: DisplayStyle::Rich,
            snippets: true,
            hints: true,
            traces: true,
            tab_width: 2,
        }
    }
}

impl DiagnosticOptions {
    /// Create options for colored terminal output.
    pub fn colored() -> Self {
        Self::default()
    }

    /// Create options for plain text output (no ANSI colors).
    pub fn plain() -> Self {
        Self {
            colored: false,
            ..Self::default()
        }
    }

    /// Create options for short format (file:line:col: message).
    pub fn short() -> Self {
        Self {
            style: DisplayStyle::Short,
            snippets: false,
            traces: false,
            ..Self::default()
        }
    }

    /// Set whether to use colors.
    pub fn with_colored(mut self, colored: bool) -> Self {
        self.colored = colored;
        self
    }

    /// Set display style.
    pub fn with_style(mut self, style: DisplayStyle) -> Self {
        self.style = style;
        self
    }

    /// Set whether to include source snippets.
    pub fn with_snippets(mut self, snippets: bool) -> Self {
        self.snippets = snippets;
        self
    }

    /// Set whether to include hints.
    pub fn with_hints(mut self, hints: bool) -> Self {
        self.hints = hints;
        self
    }

    /// Set whether to include traces.
    pub fn with_traces(mut self, traces: bool) -> Self {
        self.traces = traces;
        self
    }

    /// Set tab width for display.
    pub fn with_tab_width(mut self, width: usize) -> Self {
        self.tab_width = width;
        self
    }
}

// ============================================================================
// Gutter Characters
// ============================================================================

/// Box-drawing characters for source code display.
mod gutter {
    pub const HEADER: &str = "┌─";
    pub const BAR: &str = "│";
    pub const SPAN_START: &str = "╭";
    pub const SPAN_END: &str = "╰";
    pub const DASH: &str = "─";
    pub const MARKER: &str = "^";
}

// ============================================================================
// Coloring
// ============================================================================

/// Apply color to text based on severity.
#[cfg(feature = "colored-diagnostics")]
fn colorize(text: &str, severity: Severity) -> String {
    use owo_colors::OwoColorize;
    match severity {
        Severity::Error => text.red().to_string(),
        Severity::Warning => text.yellow().to_string(),
    }
}

#[cfg(feature = "colored-diagnostics")]
fn colorize_help(text: &str) -> String {
    use owo_colors::OwoColorize;
    text.cyan().to_string()
}

#[cfg(not(feature = "colored-diagnostics"))]
fn colorize(text: &str, _severity: Severity) -> String {
    text.to_owned()
}

#[cfg(not(feature = "colored-diagnostics"))]
fn colorize_help(text: &str) -> String {
    text.to_owned()
}

/// Get paint function based on options.
fn get_paint_fn(options: &DiagnosticOptions, severity: Severity) -> Box<dyn Fn(&str) -> String> {
    if options.colored {
        Box::new(move |s| colorize(s, severity))
    } else {
        Box::new(|s: &str| s.to_owned())
    }
}

fn get_help_paint_fn(options: &DiagnosticOptions) -> Box<dyn Fn(&str) -> String> {
    if options.colored {
        Box::new(colorize_help)
    } else {
        Box::new(|s: &str| s.to_owned())
    }
}

// ============================================================================
// Span Location
// ============================================================================

/// Resolved source location information for a diagnostic span.
///
/// Contains all information needed to display a source code snippet
/// with proper highlighting.
pub(crate) struct SpanLocation {
    /// File path (relative to project root)
    pub path: String,
    /// Starting line number (1-indexed)
    pub start_line: usize,
    /// Starting column (0-indexed)
    pub start_col: usize,
    /// Source lines covered by the span
    lines: Vec<String>,
    /// Column where highlighting starts in first line (0-indexed)
    highlight_start_col: usize,
    /// Column where highlighting ends in last line (0-indexed, exclusive)
    highlight_end_col: usize,
}

impl SpanLocation {
    /// Resolve a span to its source location.
    pub fn from_span<W: World>(world: &W, span: Span) -> Option<Self> {
        Self::from_span_with_offset(world, span, 0)
    }

    /// Resolve a span to its source location, with line offset for main file.
    ///
    /// The `main_line_offset` is subtracted from line numbers when the span
    /// belongs to the main file. This is used to correct line numbers when
    /// a prelude has been injected at the beginning of the main file.
    pub fn from_span_with_offset<W: World>(
        world: &W,
        span: Span,
        main_line_offset: usize,
    ) -> Option<Self> {
        let id = span.id()?;
        let source = world.source(id).ok()?;
        let range = source.range(span)?;
        let text = source.text();

        // Calculate line boundaries
        let start_line_start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
        let end_line_end = text[range.end..]
            .find('\n')
            .map_or(text.len(), |i| range.end + i);
        let end_line_start = text[..range.end].rfind('\n').map_or(0, |i| i + 1);

        // Calculate positions
        // Column numbers are 0-indexed to match typst-cli output
        let raw_start_line = text[..range.start].matches('\n').count() + 1;
        let start_col = text[start_line_start..range.start].chars().count();
        let end_col = text[end_line_start..range.end].chars().count();

        // Apply line offset for main file (when prelude is injected)
        let is_main = id == world.main();
        let start_line = if is_main && main_line_offset > 0 {
            // If the diagnostic is from the prelude itself, skip it
            // (user doesn't need to see warnings from injected code)
            if raw_start_line <= main_line_offset {
                return None;
            }
            raw_start_line - main_line_offset
        } else {
            raw_start_line
        };

        // Extract source lines
        let lines = text[start_line_start..end_line_end]
            .lines()
            .map(String::from)
            .collect();

        // Build path - include package prefix if from a package
        let file_path = id.vpath().as_rootless_path().to_string_lossy();
        let path = if let Some(pkg) = id.package() {
            format!("@{}/{}/{}", pkg.namespace, pkg.name, file_path)
        } else {
            file_path.into_owned()
        };

        Some(Self {
            path,
            start_line,
            start_col,
            lines,
            highlight_start_col: start_col,
            highlight_end_col: end_col,
        })
    }

    /// Check if this span covers multiple lines.
    #[inline]
    const fn is_multiline(&self) -> bool {
        self.lines.len() > 1
    }

    /// Get the last line number covered by this span.
    #[inline]
    const fn end_line(&self) -> usize {
        self.start_line + self.lines.len() - 1
    }

    /// Calculate the width needed to display line numbers.
    #[inline]
    fn line_num_width(&self) -> usize {
        self.end_line().to_string().len().max(1)
    }

    /// Convert to structured source lines for DiagnosticInfo.
    pub fn to_source_lines(&self) -> Vec<SourceLine> {
        let is_multiline = self.lines.len() > 1;
        let last_idx = self.lines.len().saturating_sub(1);

        self.lines
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let line_num = self.start_line + i;
                let line_len = text.chars().count();

                let highlight = if is_multiline {
                    // Multi-line span: first line from start_col to end,
                    // middle lines entire line, last line from start to end_col
                    if i == 0 {
                        Some((self.highlight_start_col, line_len))
                    } else if i == last_idx {
                        Some((0, self.highlight_end_col))
                    } else {
                        Some((0, line_len))
                    }
                } else {
                    // Single-line span
                    Some((self.highlight_start_col, self.highlight_end_col))
                };

                SourceLine {
                    line_num,
                    text: text.clone(),
                    highlight,
                }
            })
            .collect()
    }
}

// ============================================================================
// Snippet Writer
// ============================================================================

/// Helper for writing formatted source snippets.
///
/// Encapsulates the logic for formatting source code with proper
/// alignment, gutter characters, and highlighting.
struct SnippetWriter<'a, F>
where
    F: Fn(&str) -> String,
{
    output: &'a mut String,
    paint: F,
    line_num_width: usize,
}

impl<'a, F> SnippetWriter<'a, F>
where
    F: Fn(&str) -> String,
{
    fn new(output: &'a mut String, paint: F, line_num_width: usize) -> Self {
        Self {
            output,
            paint,
            line_num_width,
        }
    }

    /// Write the location header: "  ┌─ path:line:col"
    fn write_header(&mut self, path: &str, line: usize, col: usize) {
        _ = writeln!(
            self.output,
            "{:>width$} {} {}:{}:{}",
            "",
            (self.paint)(gutter::HEADER),
            path,
            line,
            col,
            width = self.line_num_width
        );
    }

    /// Write an empty gutter line: "  │"
    fn write_empty_gutter(&mut self) {
        _ = writeln!(
            self.output,
            "{:>width$} {}",
            "",
            (self.paint)(gutter::BAR),
            width = self.line_num_width
        );
    }

    /// Write a source line with optional box character and highlighting.
    fn write_source_line(
        &mut self,
        line_num: usize,
        line_text: &str,
        box_char: Option<&str>,
        highlight_range: Option<(usize, usize)>,
    ) {
        let line_num_str = format!("{:>width$}", line_num, width = self.line_num_width);

        let formatted_line = match (box_char, highlight_range) {
            (Some(bc), Some((start, end))) => {
                let (before, highlighted, after) = split_line(line_text, start, end);
                format!(
                    "{} {} {} {}{}{}",
                    (self.paint)(&line_num_str),
                    (self.paint)(gutter::BAR),
                    (self.paint)(bc),
                    before,
                    (self.paint)(&highlighted),
                    after
                )
            }
            (None, Some((start, end))) => {
                let (before, highlighted, after) = split_line(line_text, start, end);
                format!(
                    "{} {} {}{}{}",
                    (self.paint)(&line_num_str),
                    (self.paint)(gutter::BAR),
                    before,
                    (self.paint)(&highlighted),
                    after
                )
            }
            _ => {
                format!(
                    "{} {} {}",
                    (self.paint)(&line_num_str),
                    (self.paint)(gutter::BAR),
                    line_text
                )
            }
        };

        _ = writeln!(self.output, "{formatted_line}");
    }

    /// Write marker line for single-line spans: "  │   ^^^^"
    fn write_single_line_marker(&mut self, start_col: usize, span_len: usize) {
        let spaces = " ".repeat(start_col);
        let markers = gutter::MARKER.repeat(span_len.max(1));
        _ = writeln!(
            self.output,
            "{:>width$} {} {}{}",
            "",
            (self.paint)(gutter::BAR),
            spaces,
            (self.paint)(&markers),
            width = self.line_num_width
        );
    }

    /// Write marker line for multi-line spans: "  │ ╰────^"
    fn write_multiline_end_marker(&mut self, end_col: usize) {
        let dashes = gutter::DASH.repeat(end_col);
        _ = writeln!(
            self.output,
            "{:>width$} {} {}{}{}",
            "",
            (self.paint)(gutter::BAR),
            (self.paint)(gutter::SPAN_END),
            (self.paint)(&dashes),
            (self.paint)(gutter::MARKER),
            width = self.line_num_width
        );
    }
}

/// Split a line into (before, highlighted, after) based on column range.
/// Both `start_col` and `end_col` are 0-indexed.
fn split_line(line: &str, start_col: usize, end_col: usize) -> (String, String, String) {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    // Clamp indices to valid range and ensure start <= end
    let start_idx = start_col.min(len);
    let end_idx = end_col.min(len).max(start_idx);

    let before: String = chars[..start_idx].iter().collect();
    let highlighted: String = chars[start_idx..end_idx].iter().collect();
    let after: String = chars[end_idx..].iter().collect();

    (before, highlighted, after)
}

// ============================================================================
// Public Formatting API
// ============================================================================

/// Format compilation diagnostics into a human-readable string.
pub fn format_diagnostics<W: World>(world: &W, diagnostics: &[SourceDiagnostic]) -> String {
    format_diagnostics_with_options(world, diagnostics, &DiagnosticOptions::default())
}

/// Format compilation diagnostics with custom options.
pub fn format_diagnostics_with_options<W: World>(
    world: &W,
    diagnostics: &[SourceDiagnostic],
    options: &DiagnosticOptions,
) -> String {
    let mut output = String::new();

    // Partition and sort: errors first, then warnings
    let (errors, warnings): (Vec<_>, Vec<_>) = diagnostics
        .iter()
        .partition(|d| d.severity == Severity::Error);

    let all_diags: Vec<_> = errors.iter().chain(warnings.iter()).collect();
    for (i, diag) in all_diags.iter().enumerate() {
        format_diagnostic_internal(&mut output, world, diag, options);
        // Add blank line between diagnostics (but not after the last one)
        if i < all_diags.len() - 1 {
            output.push('\n');
        }
    }

    output
}

/// Disable colored output globally (for tests).
#[cfg(all(test, feature = "colored-diagnostics"))]
pub fn disable_colors() {
    owo_colors::set_override(false);
}

// ============================================================================
// DiagnosticInfo Formatting (World-independent)
// ============================================================================

/// Format a single `DiagnosticInfo` into the output string.
///
/// This function works with pre-resolved diagnostic info and does not
/// require a World reference.
pub fn format_info(output: &mut String, info: &DiagnosticInfo, options: DiagnosticOptions) {
    let label = match info.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let paint = get_paint_fn(&options, info.severity);

    match options.style {
        DisplayStyle::Short => {
            format_info_short(output, info, label, &paint);
        }
        DisplayStyle::Rich => {
            format_info_rich(output, info, label, &paint, &options);
        }
    }
}

/// Format DiagnosticInfo in short style.
fn format_info_short(
    output: &mut String,
    info: &DiagnosticInfo,
    label: &str,
    paint: &dyn Fn(&str) -> String,
) {
    if let (Some(path), Some(line), Some(col)) = (&info.path, info.line, info.column) {
        _ = writeln!(
            output,
            "{}:{}:{}: {}: {}",
            path,
            line,
            col,
            paint(label),
            info.message
        );
    } else {
        _ = writeln!(output, "{}: {}", paint(label), info.message);
    }
}

/// Format DiagnosticInfo in rich style.
fn format_info_rich(
    output: &mut String,
    info: &DiagnosticInfo,
    label: &str,
    paint: &dyn Fn(&str) -> String,
    options: &DiagnosticOptions,
) {
    // Header: "error: message"
    _ = writeln!(output, "{}: {}", paint(label), info.message);

    // Source snippet (if enabled and available)
    if options.snippets
        && !info.source_lines.is_empty()
        && let (Some(path), Some(line), Some(col)) = (&info.path, info.line, info.column)
    {
        write_snippet_from_lines(output, path, line, col, &info.source_lines, paint);
    }

    // Trace information (if enabled)
    if options.traces {
        let help_paint = get_help_paint_fn(options);
        for trace in &info.traces {
            write_trace_info(output, trace, &help_paint);
        }
    }

    // Hints (if enabled)
    if options.hints {
        let help_paint = get_help_paint_fn(options);
        for hint in &info.hints {
            _ = writeln!(output, "  {} hint: {}", help_paint("="), hint);
        }
    }
}

/// Write a source snippet from pre-resolved SourceLines.
fn write_snippet_from_lines(
    output: &mut String,
    path: &str,
    start_line: usize,
    start_col: usize,
    lines: &[SourceLine],
    paint: &dyn Fn(&str) -> String,
) {
    let end_line = lines.last().map(|l| l.line_num).unwrap_or(start_line);
    let line_num_width = end_line.to_string().len().max(1);

    // Header
    _ = writeln!(
        output,
        "{:>width$} {} {}:{}:{}",
        "",
        paint(gutter::HEADER),
        path,
        start_line,
        start_col,
        width = line_num_width
    );

    // Empty gutter
    _ = writeln!(
        output,
        "{:>width$} {}",
        "",
        paint(gutter::BAR),
        width = line_num_width
    );

    let is_multiline = lines.len() > 1;

    for (i, source_line) in lines.iter().enumerate() {
        let line_num_str = format!("{:>width$}", source_line.line_num, width = line_num_width);

        if is_multiline {
            // Multi-line: use box drawing
            let box_char = if i == 0 {
                gutter::SPAN_START
            } else {
                gutter::BAR
            };

            if let Some((start, end)) = source_line.highlight {
                let (before, highlighted, after) = split_line(&source_line.text, start, end);
                _ = writeln!(
                    output,
                    "{} {} {} {}{}{}",
                    paint(&line_num_str),
                    paint(gutter::BAR),
                    paint(box_char),
                    before,
                    paint(&highlighted),
                    after
                );
            } else {
                _ = writeln!(
                    output,
                    "{} {} {} {}",
                    paint(&line_num_str),
                    paint(gutter::BAR),
                    paint(box_char),
                    source_line.text
                );
            }
        } else {
            // Single line
            if let Some((start, end)) = source_line.highlight {
                let (before, highlighted, after) = split_line(&source_line.text, start, end);
                _ = writeln!(
                    output,
                    "{} {} {}{}{}",
                    paint(&line_num_str),
                    paint(gutter::BAR),
                    before,
                    paint(&highlighted),
                    after
                );

                // Marker line
                let span_len = end.saturating_sub(start).max(1);
                let spaces = " ".repeat(start);
                let markers = gutter::MARKER.repeat(span_len);
                _ = writeln!(
                    output,
                    "{:>width$} {} {}{}",
                    "",
                    paint(gutter::BAR),
                    spaces,
                    paint(&markers),
                    width = line_num_width
                );
            } else {
                _ = writeln!(
                    output,
                    "{} {} {}",
                    paint(&line_num_str),
                    paint(gutter::BAR),
                    source_line.text
                );
            }
        }
    }

    // Multi-line end marker
    if is_multiline && let Some(last) = lines.last() {
        let end_col = last.highlight.map(|(_, e)| e).unwrap_or(0);
        let dashes = gutter::DASH.repeat(end_col);
        _ = writeln!(
            output,
            "{:>width$} {} {}{}{}",
            "",
            paint(gutter::BAR),
            paint(gutter::SPAN_END),
            paint(&dashes),
            paint(gutter::MARKER),
            width = line_num_width
        );
    }
}

/// Write trace info from pre-resolved TraceInfo.
fn write_trace_info(output: &mut String, trace: &TraceInfo, help_paint: &dyn Fn(&str) -> String) {
    _ = writeln!(output, "{}: {}", help_paint("help"), trace.message);

    if !trace.source_lines.is_empty()
        && let (Some(path), Some(line), Some(col)) = (&trace.path, trace.line, trace.column)
    {
        write_snippet_from_lines(output, path, line, col, &trace.source_lines, help_paint);
    }
}

// ============================================================================
// Internal Formatting
// ============================================================================

/// Format a single diagnostic with its source snippet.
fn format_diagnostic_internal<W: World>(
    output: &mut String,
    world: &W,
    diag: &SourceDiagnostic,
    options: &DiagnosticOptions,
) {
    let label = match diag.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let paint = get_paint_fn(options, diag.severity);

    match options.style {
        DisplayStyle::Short => {
            format_diagnostic_short(output, world, diag, label, &paint);
        }
        DisplayStyle::Rich => {
            format_diagnostic_rich(output, world, diag, label, &paint, options);
        }
    }
}

/// Format diagnostic in short style: "file:line:col: severity: message"
fn format_diagnostic_short<W: World>(
    output: &mut String,
    world: &W,
    diag: &SourceDiagnostic,
    label: &str,
    paint: &dyn Fn(&str) -> String,
) {
    if let Some(loc) = SpanLocation::from_span(world, diag.span) {
        _ = writeln!(
            output,
            "{}:{}:{}: {}: {}",
            loc.path,
            loc.start_line,
            loc.start_col + 1, // 1-indexed for display
            paint(label),
            diag.message
        );
    } else {
        _ = writeln!(output, "{}: {}", paint(label), diag.message);
    }
}

/// Format diagnostic in rich style with source snippets.
fn format_diagnostic_rich<W: World>(
    output: &mut String,
    world: &W,
    diag: &SourceDiagnostic,
    label: &str,
    paint: &dyn Fn(&str) -> String,
    options: &DiagnosticOptions,
) {
    // Header: "error: message"
    _ = writeln!(output, "{}: {}", paint(label), diag.message);

    // Source snippet (if enabled)
    if options.snippets
        && let Some(location) = SpanLocation::from_span(world, diag.span)
    {
        write_snippet(output, &location, paint);
    }

    // Trace information (call stack) - if enabled
    if options.traces {
        let help_paint = get_help_paint_fn(options);
        for trace in &diag.trace {
            write_trace(output, world, &trace.v, trace.span, &help_paint);
        }
    }

    // Hints - if enabled
    if options.hints {
        let help_paint = get_help_paint_fn(options);
        for hint in &diag.hints {
            _ = writeln!(output, "  {} hint: {}", help_paint("="), hint);
        }
    }
}

/// Write a source code snippet with highlighting.
fn write_snippet(output: &mut String, location: &SpanLocation, paint: &dyn Fn(&str) -> String) {
    let mut writer = SnippetWriter::new(output, |s| paint(s), location.line_num_width());

    writer.write_header(&location.path, location.start_line, location.start_col + 1);
    writer.write_empty_gutter();

    if location.is_multiline() {
        write_multiline_snippet(&mut writer, location);
    } else {
        write_singleline_snippet(&mut writer, location);
    }
}

/// Write a single-line source snippet.
fn write_singleline_snippet<F>(writer: &mut SnippetWriter<F>, location: &SpanLocation)
where
    F: Fn(&str) -> String,
{
    let line_text = location.lines.first().map_or("", String::as_str);

    let span_len = location
        .highlight_end_col
        .saturating_sub(location.highlight_start_col)
        .max(1);

    writer.write_source_line(
        location.start_line,
        line_text,
        None,
        Some((location.highlight_start_col, location.highlight_end_col)),
    );
    writer.write_single_line_marker(location.highlight_start_col, span_len);
}

/// Write a multi-line source snippet with box drawing.
fn write_multiline_snippet<F>(writer: &mut SnippetWriter<F>, location: &SpanLocation)
where
    F: Fn(&str) -> String,
{
    for (i, line_text) in location.lines.iter().enumerate() {
        let line_num = location.start_line + i;
        let is_first = i == 0;
        let line_len = line_text.chars().count();

        let (box_char, highlight_range) = if is_first {
            (
                gutter::SPAN_START,
                (location.highlight_start_col, line_len + 1),
            )
        } else {
            (gutter::BAR, (1, line_len + 1))
        };

        writer.write_source_line(line_num, line_text, Some(box_char), Some(highlight_range));
    }

    writer.write_multiline_end_marker(location.highlight_end_col);
}

/// Write trace information with help theme.
///
/// Skips Import traces since they just show which content file imported
/// the failing template, which is usually not helpful context.
fn write_trace<W: World>(
    output: &mut String,
    world: &W,
    tracepoint: &typst::diag::Tracepoint,
    span: Span,
    help_paint: &dyn Fn(&str) -> String,
) {
    use typst::diag::Tracepoint;

    // Skip import traces - they just show content importing template
    if matches!(tracepoint, Tracepoint::Import) {
        return;
    }

    let message = match tracepoint {
        Tracepoint::Call(Some(name)) => {
            format!("error occurred in this call of function `{name}`")
        }
        Tracepoint::Call(None) => "error occurred in this function call".into(),
        Tracepoint::Show(name) => format!("error occurred in this show rule for `{name}`"),
        Tracepoint::Import => unreachable!(), // Handled above
    };

    _ = writeln!(output, "{}: {}", help_paint("help"), message);

    if let Some(location) = SpanLocation::from_span(world, span) {
        write_snippet(output, &location, help_paint);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_line_helper() {
        // Test normal case: "hello world", cols 6-11 -> "hello " + "world" + ""
        let (before, highlighted, after) = split_line("hello world", 6, 11);
        assert_eq!(before, "hello ");
        assert_eq!(highlighted, "world");
        assert_eq!(after, "");

        // Test start at beginning: "abc", cols 0-1 -> "" + "a" + "bc"
        let (before, highlighted, after) = split_line("abc", 0, 1);
        assert_eq!(before, "");
        assert_eq!(highlighted, "a");
        assert_eq!(after, "bc");

        // Test full line: "test", cols 0-4 -> "" + "test" + ""
        let (before, highlighted, after) = split_line("test", 0, 4);
        assert_eq!(before, "");
        assert_eq!(highlighted, "test");
        assert_eq!(after, "");

        // Test with Unicode: "你好世界" (Hello World), cols 0-2 -> "" + "你好" + "世界"
        let (before, highlighted, after) = split_line("你好世界", 0, 2);
        assert_eq!(before, "");
        assert_eq!(highlighted, "你好");
        assert_eq!(after, "世界");
    }

    #[test]
    fn test_span_location_methods() {
        let location = SpanLocation {
            path: "test.typ".to_string(),
            start_line: 10,
            start_col: 0, // 0-indexed
            lines: vec!["line1".into(), "line2".into(), "line3".into()],
            highlight_start_col: 0,
            highlight_end_col: 5,
        };

        assert!(location.is_multiline());
        assert_eq!(location.end_line(), 12);
        assert_eq!(location.line_num_width(), 2);

        let single_line = SpanLocation {
            path: "test.typ".to_string(),
            start_line: 5,
            start_col: 0, // 0-indexed
            lines: vec!["single".into()],
            highlight_start_col: 0,
            highlight_end_col: 6,
        };

        assert!(!single_line.is_multiline());
        assert_eq!(single_line.end_line(), 5);
        assert_eq!(single_line.line_num_width(), 1);
    }
}
