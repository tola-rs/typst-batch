//! Internal execution modes for `TypstWorld`.

use std::sync::Arc;

use typst::utils::LazyHash;
use typst::Library;

use super::cache::LocalFileCache;
use super::snapshot::SourceSnapshot;
use crate::resource::file::SharedFileCache;
use crate::resource::font::FontStore;
use crate::resource::library::create_library_with_inputs;

/// File access mode.
pub(crate) enum FileCacheMode {
    /// Task-local cache, no sharing between tasks.
    Local(LocalFileCache),
    /// Shared cache owned by the caller.
    Shared(Arc<SharedFileCache>),
    /// Pre-built source snapshot plus a shared fallback cache.
    Snapshot {
        /// Immutable snapshot containing preloaded project sources.
        snapshot: Arc<SourceSnapshot>,
        /// Shared cache used for files that are not covered by the snapshot.
        fallback: Arc<SharedFileCache>,
    },
}

impl FileCacheMode {
    /// Creates a local cache mode with a fresh cache.
    pub fn local() -> Self {
        Self::Local(LocalFileCache::new())
    }

    /// Creates a shared cache mode.
    pub fn shared(cache: Arc<SharedFileCache>) -> Self {
        Self::Shared(cache)
    }

    /// Creates a snapshot cache mode from a pre-built source snapshot.
    pub fn snapshot(snapshot: Arc<SourceSnapshot>) -> Self {
        Self::Snapshot {
            snapshot,
            fallback: Arc::new(SharedFileCache::new()),
        }
    }

    /// Creates a snapshot cache mode with an existing shared fallback cache.
    pub(crate) fn snapshot_with_fallback(
        snapshot: Arc<SourceSnapshot>,
        fallback: Arc<SharedFileCache>,
    ) -> Self {
        Self::Snapshot {
            snapshot,
            fallback,
        }
    }
}

/// Font loading mode.
#[derive(Clone)]
pub(crate) enum FontMode {
    /// No fonts loaded (for scan/query).
    None,
    /// Shared fonts owned by the caller.
    Shared(Arc<FontStore>),
}

/// Library mode for `sys.inputs`.
#[derive(Clone)]
pub(crate) enum LibraryMode {
    /// Use global library (no sys.inputs).
    Global,
    /// Custom library with sys.inputs.
    Custom(LazyHash<Library>),
}

impl LibraryMode {
    /// Creates a custom library strategy with the given sys.inputs.
    pub(crate) fn custom(inputs: typst::foundations::Dict) -> Self {
        Self::Custom(create_library_with_inputs(inputs))
    }
}
