//! Unified `TypstWorld` implementation.
//!
//! A single World implementation with configurable strategies for:
//! - **Files**: Local cache, shared cache, or source snapshot + fallback cache
//! - **Fonts**: None (scan/query), Shared (build/serve)
//! - **Library**: Global or Custom (with sys.inputs)
//!
//! # Usage
//!
//! ## Builder Pattern (explicit configuration)
//!
//! ```ignore
//! // Scan: no fonts, local cache
//! let world = TypstWorld::builder(path, root)
//!     .with_local_cache()
//!     .no_fonts()
//!     .build();
//!
//! // Build: with fonts, snapshot cache
//! let world = TypstWorld::builder(path, root)
//!     .with_snapshot(snapshot)
//!     .with_fonts()
//!     .build();
//!
//! // Serve: with fonts, shared cache
//! let world = TypstWorld::builder(path, root)
//!     .with_shared_cache()
//!     .with_fonts()
//!     .build();
//!
//! // With sys.inputs
//! let world = TypstWorld::builder(path, root)
//!     .with_local_cache()
//!     .no_fonts()
//!     .with_inputs([("key", "value")])
//!     .build();
//! ```

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use chrono::{DateTime, Datelike, FixedOffset, Local, Utc};
use typst::diag::FileResult;
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};

use super::builder::WorldBuilder;
use super::path::normalize_path;
use super::source::read_source_with_injection;
use super::strategy::{FileCacheMode, FontMode, LibraryMode};
use crate::resource::file::{file_id_from_path, record_file_access, FileResolver};
use crate::resource::library::GLOBAL_LIBRARY;

// =============================================================================
// Empty FontBook (for scan/query)
// =============================================================================

static EMPTY_FONTBOOK: OnceLock<LazyHash<FontBook>> = OnceLock::new();

fn empty_fontbook() -> &'static LazyHash<FontBook> {
    EMPTY_FONTBOOK.get_or_init(|| LazyHash::new(FontBook::new()))
}

// =============================================================================
// TypstWorld
// =============================================================================

/// Fixed timestamp for reproducible builds.
///
/// If set, `datetime.today()` returns this fixed time.
/// If not set, `datetime.today()` returns `None`.
pub type Timestamp = DateTime<Utc>;

/// Unified Typst World with configurable strategies.
///
/// Use `TypstWorld::builder()` for explicit configuration,
/// or convenience methods like `for_scan()`, `for_build()`, `for_serve()`.
pub struct TypstWorld {
    root: PathBuf,
    main: FileId,
    files: Arc<FileResolver>,
    cache: FileCacheMode,
    fonts: FontMode,
    library: LibraryMode,
    prelude: Option<String>,
    postlude: Option<String>,
    timestamp: Option<Timestamp>,
}

impl TypstWorld {
    /// Create a builder for explicit configuration.
    pub fn builder(main_path: &Path, root: &Path) -> WorldBuilder {
        WorldBuilder::new(main_path, root)
    }

    // =========================================================================
    // Internal Constructor
    // =========================================================================

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        main_path: &Path,
        root: &Path,
        files: Arc<FileResolver>,
        cache: FileCacheMode,
        fonts: FontMode,
        library: LibraryMode,
        prelude: Option<String>,
        postlude: Option<String>,
        timestamp: Option<Timestamp>,
    ) -> Self {
        let root = normalize_path(root);
        let main_abs = normalize_path(main_path);
        let main = file_id_from_path(&main_abs, &root).unwrap_or_else(|| {
            // Fallback: use filename only if path is outside root
            let filename = main_path.file_name().unwrap_or_default();
            FileId::new(None, VirtualPath::new(filename))
        });

        Self {
            root,
            main,
            files,
            cache,
            fonts,
            library,
            prelude,
            postlude,
            timestamp,
        }
    }

    /// Get the project root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the file resolver used by this world.
    pub(crate) fn files(&self) -> &FileResolver {
        &self.files
    }

    /// Get the number of lines in the prelude (for diagnostic line offset).
    ///
    /// Returns 0 if no prelude is set. The returned count includes the
    /// trailing newline that is added after the prelude during injection.
    pub fn prelude_line_count(&self) -> usize {
        self.prelude
            .as_ref()
            .map(|p| p.matches('\n').count() + 1) // +1 for the trailing newline added during injection
            .unwrap_or(0)
    }

    // =========================================================================
    // Cache Operations
    // =========================================================================

    fn get_source(&self, id: FileId) -> FileResult<Source> {
        match &self.cache {
            FileCacheMode::Local(local) => {
                if let Some(source) = local.sources.read().unwrap().get(&id) {
                    return Ok(source.clone());
                }
                let source = self.load_source(id)?;
                local.sources.write().unwrap().insert(id, source.clone());
                Ok(source)
            }
            FileCacheMode::Shared(shared) => {
                // For main file with prelude/postlude, use load_source to inject them
                // (shared cache doesn't know about per-world prelude settings)
                if id == self.main && (self.prelude.is_some() || self.postlude.is_some()) {
                    return self.load_source(id);
                }
                shared.source_with_files(id, &self.root, &self.files)
            }
            FileCacheMode::Snapshot { snapshot, fallback } => {
                if let Some(source) = snapshot.get_source(id) {
                    record_file_access(id);
                    return Ok(source);
                }
                if id == self.main && (self.prelude.is_some() || self.postlude.is_some()) {
                    return self.load_source(id);
                }
                fallback.source_with_files(id, &self.root, &self.files)
            }
        }
    }

    fn get_file(&self, id: FileId) -> FileResult<Bytes> {
        match &self.cache {
            FileCacheMode::Local(local) => {
                if let Some(bytes) = local.files.read().unwrap().get(&id) {
                    return Ok(bytes.clone());
                }
                let bytes = self.load_file(id)?;
                local.files.write().unwrap().insert(id, bytes.clone());
                Ok(bytes)
            }
            FileCacheMode::Shared(shared) => shared.file_with_files(id, &self.root, &self.files),
            FileCacheMode::Snapshot { fallback, .. } => {
                fallback.file_with_files(id, &self.root, &self.files)
            }
        }
    }

    fn load_source(&self, id: FileId) -> FileResult<Source> {
        record_file_access(id);
        read_source_with_injection(
            id,
            &self.root,
            &self.files,
            id == self.main,
            self.prelude.as_deref(),
            self.postlude.as_deref(),
        )
    }

    fn load_file(&self, id: FileId) -> FileResult<Bytes> {
        record_file_access(id);
        let data = self.files.read(id, &self.root)?;
        Ok(Bytes::new(data))
    }
}

// =============================================================================
// World Trait Implementation
// =============================================================================

impl World for TypstWorld {
    fn library(&self) -> &LazyHash<Library> {
        match &self.library {
            LibraryMode::Global => &GLOBAL_LIBRARY,
            LibraryMode::Custom(lib) => lib,
        }
    }

    fn book(&self) -> &LazyHash<FontBook> {
        match &self.fonts {
            FontMode::None => empty_fontbook(),
            FontMode::Shared(fonts) => &fonts.get().1,
        }
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.get_source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.get_file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        match &self.fonts {
            FontMode::None => None,
            FontMode::Shared(fonts) => fonts.get().0.fonts.get(index)?.get(),
        }
    }

    fn today(&self, offset: Option<i64>) -> Option<Datetime> {
        // Return None if no timestamp is set (for reproducible builds)
        let now = self.timestamp.as_ref()?;

        let with_offset = match offset {
            None => now.with_timezone(&Local).fixed_offset(),
            Some(hours) => {
                let seconds = i32::try_from(hours).ok()?.checked_mul(3600)?;
                now.with_timezone(&FixedOffset::east_opt(seconds)?)
            }
        };

        Datetime::from_ymd(
            with_offset.year(),
            with_offset.month().try_into().ok()?,
            with_offset.day().try_into().ok()?,
        )
    }
}
