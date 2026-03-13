//! Cache types for file and source storage.

use std::sync::RwLock;

use rustc_hash::FxHashMap;
use typst::foundations::Bytes;
use typst::syntax::{FileId, Source};

// ============================================================================
// Local Cache
// ============================================================================

/// Task-local file cache storage.
pub(crate) struct LocalFileCache {
    pub(crate) sources: RwLock<FxHashMap<FileId, Source>>,
    pub(crate) files: RwLock<FxHashMap<FileId, Bytes>>,
}

impl LocalFileCache {
    /// Creates a new empty local cache.
    pub fn new() -> Self {
        Self {
            sources: RwLock::new(FxHashMap::default()),
            files: RwLock::new(FxHashMap::default()),
        }
    }
}

impl Default for LocalFileCache {
    fn default() -> Self {
        Self::new()
    }
}
