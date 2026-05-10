//! Font loading and caching.
//!
//! Fonts are expensive to load, so callers should share a `FontStore` across
//! compilations that use the same font configuration.
//!
//! # Font Sources
//!
//! Fonts are searched in order:
//! 1. Custom paths provided at initialization (e.g., project fonts)
//! 2. System fonts (if enabled)
//! 3. Embedded fonts (if enabled via `embed-fonts` feature)

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use typst::text::FontBook;
use typst::utils::LazyHash;
use typst_kit::fonts::Fonts;

// =============================================================================
// Font Configuration
// =============================================================================

/// Options for font initialization.
///
/// Use this to customize font loading behavior for a [`FontStore`].
///
/// # Example
///
/// ```ignore
/// use typst_batch::{FontOptions, FontStore};
/// use std::path::Path;
///
/// let options = FontOptions::new()
///     .with_system_fonts(true)
///     .with_embedded_fonts(true)
///     .with_custom_paths(&[
///         Path::new("assets/fonts"),
///         Path::new("content/fonts"),
///     ]);
///
/// let fonts = FontStore::with_options(options);
/// ```
#[derive(Debug, Clone, Default)]
pub struct FontOptions {
    /// Whether to include system fonts.
    pub include_system_fonts: bool,
    /// Whether to include embedded fonts (New Computer Modern, etc.).
    /// Only effective when `embed-fonts` feature is enabled.
    pub include_embedded_fonts: bool,
    /// Custom font directories to search.
    pub custom_paths: Vec<PathBuf>,
}

impl FontOptions {
    /// Create new font options with default settings.
    ///
    /// Default:
    /// - System fonts: enabled
    /// - Embedded fonts: enabled (if `embed-fonts` feature is enabled)
    /// - Custom paths: empty
    pub fn new() -> Self {
        Self {
            include_system_fonts: true,
            include_embedded_fonts: true,
            custom_paths: Vec::new(),
        }
    }

    /// Set whether to include system fonts.
    ///
    /// Disabling system fonts can speed up initialization in controlled
    /// environments where only specific fonts are needed.
    pub fn with_system_fonts(mut self, include: bool) -> Self {
        self.include_system_fonts = include;
        self
    }

    /// Set whether to include embedded fonts.
    ///
    /// Embedded fonts include New Computer Modern Math and other default fonts.
    /// Only effective when `embed-fonts` feature is enabled.
    pub fn with_embedded_fonts(mut self, include: bool) -> Self {
        self.include_embedded_fonts = include;
        self
    }

    /// Set custom font paths to search.
    ///
    /// These directories are searched for `.ttf`, `.otf`, and other font files.
    pub fn with_custom_paths(mut self, paths: &[&Path]) -> Self {
        self.custom_paths = paths.iter().map(|p| p.to_path_buf()).collect();
        self
    }

    /// Add a single custom font path.
    pub fn add_path(mut self, path: impl AsRef<Path>) -> Self {
        self.custom_paths.push(path.as_ref().to_path_buf());
        self
    }
}

/// Sorting key for deterministic font ordering (for debugging).
///
/// Used by `sort_fonts_deterministically` to help diagnose font ordering issues.
/// The actual font non-determinism problem was solved by ensuring fonts are
/// initialized early with correct paths in serve mode.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FontSortKey {
    path: Option<PathBuf>,
    index: u32,
}

// =============================================================================
// Debug Utilities
// =============================================================================

/// **DEBUG ONLY**: Write font list to `/tmp/typst_batch_fonts_debug.txt` for debugging.
///
/// This function is used to diagnose font loading issues, particularly:
/// - Non-deterministic font ordering across runs
/// - Duplicate fonts from different directories (e.g., `assets/` vs `public/`)
/// - Missing or unexpected fonts
///
/// # Output Format
///
/// ```text
/// === Font Debug Output (PID: 12345) ===
/// Total fonts: 977
///
///    0: Maple Mono | /path/to/font.otf | idx=0 | Normal-700-FontStretch(1000)
///    1: SF Pro | /System/Library/Fonts/SF-Pro.otf | idx=0 | Normal-400-FontStretch(1000)
/// ...
/// === End of Debug Output ===
/// ```
pub fn debug_dump_fonts(fonts: &Fonts) {
    use std::io::Write;
    let debug_path = std::path::Path::new("/tmp/typst_batch_fonts_debug.txt");
    if let Ok(mut file) = std::fs::File::create(debug_path) {
        let _ = writeln!(
            file,
            "=== Font Debug Output (PID: {}) ===",
            std::process::id()
        );
        let _ = writeln!(file, "Total fonts: {}", fonts.fonts.len());
        let _ = writeln!(file);
        for (i, slot) in fonts.fonts.iter().enumerate() {
            let path = slot
                .path()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "embedded".to_string());
            let info = fonts.book.info(i);
            let family = info.map(|i| i.family.as_str()).unwrap_or("?");
            let variant = info
                .map(|i| format!("{:?}", i.variant))
                .unwrap_or_else(|| "?".to_string());
            let _ = writeln!(
                file,
                "{:4}: {} | {} | idx={} | {}",
                i,
                family,
                path,
                slot.index(),
                variant
            );
        }
        let _ = writeln!(file);
        let _ = writeln!(file, "=== End of Debug Output ===");
        eprintln!(
            "[FONT DEBUG] Wrote {} fonts to {:?}",
            fonts.fonts.len(),
            debug_path
        );
    }
}

// =============================================================================
// Font Initialization
// =============================================================================

/// Initialize fonts with detailed options.
fn init_fonts_impl(options: &FontOptions) -> (Fonts, LazyHash<FontBook>) {
    let mut searcher = Fonts::searcher();
    // Include system fonts if enabled
    searcher.include_system_fonts(options.include_system_fonts);
    // Include embedded fonts if enabled (New Computer Modern Math, etc.)
    #[cfg(feature = "embed-fonts")]
    searcher.include_embedded_fonts(options.include_embedded_fonts);

    // Convert PathBuf to &Path for the API
    let paths: Vec<&Path> = options.custom_paths.iter().map(|p| p.as_path()).collect();

    // Search custom paths and optionally system/embedded fonts
    let fonts = searcher.search_with(&paths);

    // DEBUG: Dump font list for debugging
    // debug_dump_fonts(&fonts);

    // Wrap font book in LazyHash for comemo caching
    let book = LazyHash::new(fonts.book.clone());
    (fonts, book)
}

/// Lazily loaded fonts for one explicit font configuration.
pub struct FontStore {
    options: FontOptions,
    fonts: OnceLock<(Fonts, LazyHash<FontBook>)>,
}

impl FontStore {
    /// Create a font store with default options.
    pub fn new() -> Self {
        Self::with_options(FontOptions::new())
    }

    /// Create a font store with custom options.
    pub fn with_options(options: FontOptions) -> Self {
        Self {
            options,
            fonts: OnceLock::new(),
        }
    }

    /// Create a font store with custom font directories.
    pub fn with_paths(paths: &[&Path]) -> Self {
        Self::with_options(FontOptions::new().with_custom_paths(paths))
    }

    /// Load fonts now and return the store.
    pub fn preload(self) -> Self {
        self.ensure_loaded();
        self
    }

    /// Load fonts now if they have not been loaded yet.
    pub fn ensure_loaded(&self) {
        let _ = self.get();
    }

    /// Get the loaded fonts, initializing them on first use.
    pub fn get(&self) -> &(Fonts, LazyHash<FontBook>) {
        self.fonts.get_or_init(|| init_fonts_impl(&self.options))
    }

    /// Check if this store has loaded its fonts.
    pub fn is_loaded(&self) -> bool {
        self.fonts.get().is_some()
    }

    /// Get the number of loaded fonts.
    pub fn font_count(&self) -> Option<usize> {
        self.fonts.get().map(|(fonts, _)| fonts.fonts.len())
    }

    /// Get the number of font families.
    pub fn family_count(&self) -> Option<usize> {
        self.fonts.get().map(|(_, book)| book.families().count())
    }
}

impl Default for FontStore {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Font Sorting (For Debugging)
// =============================================================================

/// Sort fonts by (path, index) for debugging font ordering issues.
///
/// # Background
///
/// `fontdb` uses `std::fs::read_dir()` which doesn't guarantee order.
/// This function can be used to debug font-related issues by ensuring
/// consistent ordering.
///
/// # Note
///
/// The actual font non-determinism problem in serve mode was solved by
/// ensuring `warmup_fonts()` is called even when using vdom-cache,
/// not by sorting fonts.
#[allow(dead_code)]
fn sort_fonts_deterministically(fonts: Fonts) -> Fonts {
    let n = fonts.fonts.len();
    if n == 0 {
        return fonts;
    }

    // Create (original_index, sort_key) pairs
    let mut indices: Vec<(usize, FontSortKey)> = fonts
        .fonts
        .iter()
        .enumerate()
        .map(|(i, slot)| {
            (
                i,
                FontSortKey {
                    path: slot.path().map(|p| p.to_path_buf()),
                    index: slot.index(),
                },
            )
        })
        .collect();

    // Sort by (path, index)
    indices.sort_by(|a, b| a.1.cmp(&b.1));

    // Collect FontInfo in sorted order
    let sorted_infos: Vec<_> = indices
        .iter()
        .filter_map(|(old_idx, _)| fonts.book.info(*old_idx).cloned())
        .collect();

    // Rebuild FontBook from sorted infos
    let new_book = FontBook::from_infos(sorted_infos);

    // Reorder fonts Vec to match
    // We need to move FontSlots, but they're not Clone.
    // Use a permutation approach with Option<FontSlot>
    let mut old_fonts: Vec<Option<_>> = fonts.fonts.into_iter().map(Some).collect();
    let mut new_fonts = Vec::with_capacity(n);
    for (old_idx, _) in indices {
        if let Some(slot) = old_fonts[old_idx].take() {
            new_fonts.push(slot);
        }
    }

    Fonts {
        book: new_book,
        fonts: new_fonts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preload_marks_store_loaded() {
        let paths: [&Path; 0] = [];
        let fonts = FontStore::with_paths(&paths).preload();

        assert!(fonts.is_loaded());
    }

    #[test]
    fn font_store_loads_fonts() {
        let store = FontStore::new();
        let fonts = store.get();
        // Should find at least some system fonts on most systems
        // Note: This test may fail in minimal container environments
        assert!(!fonts.0.fonts.is_empty(), "Should find system fonts");
    }

    #[test]
    fn font_store_has_font_book() {
        let store = FontStore::new();
        let fonts = store.get();
        // FontBook should have indexed the fonts
        assert!(
            fonts.1.families().count() > 0,
            "Font book should have families"
        );
    }

    #[test]
    fn font_store_reuses_loaded_fonts() {
        let store = FontStore::new();
        let fonts1 = store.get();
        let fonts2 = store.get();
        assert!(std::ptr::eq(fonts1, fonts2), "Fonts should be shared");
    }

    #[test]
    fn stores_keep_independent_configurations() {
        let first = FontStore::with_paths(&[]);
        let second = FontStore::with_paths(&[Path::new("/nonexistent")]);

        let first_fonts = first.get();
        let second_fonts = second.get();

        assert!(!std::ptr::eq(first_fonts, second_fonts));
    }
}
