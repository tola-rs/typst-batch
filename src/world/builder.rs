//! Builder pattern for `TypstWorld`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use typst::foundations::Dict;

use super::core::{Timestamp, TypstWorld};
use super::snapshot::SourceSnapshot;
use super::strategy::{FileCacheMode, FontMode, LibraryMode};
use crate::resource::file::{FileResolver, SharedFileCache};
use crate::resource::font::FontStore;

/// Builder for configuring `TypstWorld`.
///
/// Use `TypstWorld::builder()` to create a builder.
pub struct WorldBuilder {
    main_path: PathBuf,
    root: PathBuf,
    files: Arc<FileResolver>,
    cache: Option<FileCacheMode>,
    fonts: Option<FontMode>,
    library: LibraryMode,
    prelude: Option<String>,
    postlude: Option<String>,
    timestamp: Option<Timestamp>,
}

impl WorldBuilder {
    /// Create a new builder.
    pub(crate) fn new(main_path: &Path, root: &Path) -> Self {
        Self {
            main_path: main_path.to_path_buf(),
            root: root.to_path_buf(),
            files: Arc::new(FileResolver::new()),
            cache: None,
            fonts: None,
            library: LibraryMode::Global,
            prelude: None,
            postlude: None,
            timestamp: None,
        }
    }

    // =========================================================================
    // Cache Strategy
    // =========================================================================

    /// Use an explicit file resolver for this world.
    pub fn with_files(mut self, files: Arc<FileResolver>) -> Self {
        self.files = files;
        self
    }

    /// Use task-local cache (no sharing between compilations).
    ///
    /// Best for: isolated compilations, scanning operations.
    pub fn with_local_cache(mut self) -> Self {
        self.cache = Some(FileCacheMode::local());
        self
    }

    /// Use shared cache with lock-based synchronization.
    ///
    /// Best for: hot reload, incremental updates where files change frequently.
    pub fn with_shared_cache(mut self, cache: Arc<SharedFileCache>) -> Self {
        self.cache = Some(FileCacheMode::shared(cache));
        self
    }

    /// Use pre-built immutable snapshot for lock-free parallel access.
    ///
    /// Best for: batch compilation where files are pre-scanned.
    pub fn with_snapshot(mut self, snapshot: Arc<SourceSnapshot>) -> Self {
        self.cache = Some(FileCacheMode::snapshot(snapshot));
        self
    }

    /// Use a pre-built snapshot with an explicit shared fallback cache.
    pub(crate) fn with_snapshot_and_fallback(
        mut self,
        snapshot: Arc<SourceSnapshot>,
        fallback: Arc<SharedFileCache>,
    ) -> Self {
        self.cache = Some(FileCacheMode::snapshot_with_fallback(snapshot, fallback));
        self
    }

    // =========================================================================
    // Font Strategy
    // =========================================================================

    /// Disable font loading.
    ///
    /// Best for: scanning/query operations that don't require layout.
    pub fn no_fonts(mut self) -> Self {
        self.fonts = Some(FontMode::None);
        self
    }

    /// Use shared fonts.
    ///
    /// Best for: compilation operations that require layout/rendering.
    pub fn with_fonts(mut self, fonts: Arc<FontStore>) -> Self {
        self.fonts = Some(FontMode::Shared(fonts));
        self
    }

    /// Configure `sys.inputs` for the compilation.
    pub fn with_inputs<I, K, V>(mut self, inputs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<typst::foundations::Str>,
        V: typst::foundations::IntoValue,
    {
        let dict: Dict = inputs
            .into_iter()
            .map(|(k, v)| (k.into(), v.into_value()))
            .collect();
        self.library = LibraryMode::custom(dict);
        self
    }

    /// Configure `sys.inputs` from a pre-built `Dict`.
    pub fn with_inputs_dict(mut self, inputs: Dict) -> Self {
        self.library = LibraryMode::custom(inputs);
        self
    }

    // =========================================================================
    // Prelude
    // =========================================================================

    /// Set Typst code to prepend to the main file.
    ///
    /// The prelude is injected at the beginning of the main source file
    /// before compilation. Useful for injecting show rules, imports, etc.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let world = TypstWorld::builder(path, root)
    ///     .with_shared_cache()
    ///     .with_fonts()
    ///     .with_prelude(r#"
    ///         #show math.equation: eq => html.frame(eq)
    ///     "#)
    ///     .build();
    /// ```
    pub fn with_prelude(mut self, prelude: impl Into<String>) -> Self {
        self.prelude = Some(prelude.into());
        self
    }

    /// Set Typst code to append to the main file.
    ///
    /// The postlude is injected at the end of the main source file
    /// before compilation. Useful for injecting query operations, cleanup, etc.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let world = TypstWorld::builder(path, root)
    ///     .with_shared_cache()
    ///     .with_fonts()
    ///     .with_postlude(r#"
    ///         #context {
    ///             let eqs = query(math.equation)
    ///             // Process equations...
    ///         }
    ///     "#)
    ///     .build();
    /// ```
    pub fn with_postlude(mut self, postlude: impl Into<String>) -> Self {
        self.postlude = Some(postlude.into());
        self
    }

    // =========================================================================
    // Timestamp
    // =========================================================================

    /// Set a fixed timestamp for `datetime.today()`.
    ///
    /// If not set, `datetime.today()` returns `None` (compile error).
    /// This ensures reproducible builds by default.
    pub fn with_timestamp(mut self, timestamp: Timestamp) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    /// Build the `TypstWorld`.
    ///
    /// # Panics
    ///
    /// Panics if cache or fonts strategy is not set.
    pub fn build(self) -> TypstWorld {
        let cache = self.cache.expect("cache strategy must be set");
        let fonts = self.fonts.expect("fonts strategy must be set");
        TypstWorld::new(
            &self.main_path,
            &self.root,
            self.files,
            cache,
            fonts,
            self.library,
            self.prelude,
            self.postlude,
            self.timestamp,
        )
    }
}
