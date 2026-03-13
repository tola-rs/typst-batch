//! File caching with fingerprint-based invalidation.
//!
//! # Caching Strategy
//!
//! ```text
//! GLOBAL_FILE_CACHE (shared across all compilations)
//! └── SharedFileCache
//!     └── FxHashMap<FileId, Arc<Mutex<FileSlot>>>
//!         └── FileSlot
//!             ├── source: SlotCell<Source>  ─┐
//!             └── file: SlotCell<Bytes>     ─┼── Fingerprint-based invalidation
//! ```

use std::sync::Arc;
use std::mem;
use std::path::Path;
use std::sync::LazyLock;

use parking_lot::{Mutex, RwLock};
use rustc_hash::FxHashMap;
use typst::diag::FileResult;
use typst::foundations::Bytes;
use typst::syntax::{FileId, Source};

use super::access::{current_generation, record_file_access};
use super::read::{decode_utf8, read_with_global_virtual};
use super::vfs::VirtualFileSystem;
use crate::resource::file::read::read_with_virtual;

// =============================================================================
// Global File Cache
// =============================================================================

/// Shared cache keyed by `FileId`.
///
/// Each file gets its own lock, so unrelated files can be processed in parallel
/// without contending on the whole cache map.
#[derive(Default)]
pub struct SharedFileCache {
    slots: RwLock<FxHashMap<FileId, Arc<Mutex<FileSlot>>>>,
}

impl SharedFileCache {
    /// Create an empty shared cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all cached slots.
    pub fn clear(&self) {
        self.slots.write().clear();
    }

    /// Read and cache source text using the global virtual file system.
    pub fn source_with_global_virtual(&self, id: FileId, root: &Path) -> FileResult<Source> {
        self.slot(id).lock().source_with_global_virtual(root)
    }

    /// Read and cache raw bytes using the global virtual file system.
    pub fn file_with_global_virtual(&self, id: FileId, root: &Path) -> FileResult<Bytes> {
        self.slot(id).lock().file_with_global_virtual(root)
    }

    fn slot(&self, id: FileId) -> Arc<Mutex<FileSlot>> {
        if let Some(slot) = self.slots.read().get(&id) {
            return Arc::clone(slot);
        }

        let mut slots = self.slots.write();
        Arc::clone(
            slots
                .entry(id)
                .or_insert_with(|| Arc::new(Mutex::new(FileSlot::new(id)))),
        )
    }
}

/// Global shared file cache - reused across all compilations.
///
/// # Design Notes
///
/// The cache key is `FileId` (which contains only `VirtualPath`, not the root).
/// This is intentional - the cache is designed for reuse within the **same project**.
///
/// For correct cross-project usage:
/// - Call `reset_access_flags()` before each compilation
/// - Call `clear_file_cache()` when switching between different projects
pub static GLOBAL_FILE_CACHE: LazyLock<SharedFileCache> = LazyLock::new(SharedFileCache::new);

/// Clear the global file cache.
///
/// Call when template/dependency files change to ensure fresh data is loaded.
/// This also clears the comemo cache.
pub fn clear_file_cache() {
    GLOBAL_FILE_CACHE.clear();
    typst::comemo::evict(0);
}

// =============================================================================
// SlotCell - Fingerprint-based Caching
// =============================================================================

/// Lazily processes data for a file with fingerprint-based caching.
///
/// Uses a generation counter for efficient access tracking instead of
/// per-slot boolean flags that require O(n) reset.
pub struct SlotCell<T> {
    data: Option<FileResult<T>>,
    fingerprint: u128,
    /// Generation when this cell was last accessed.
    last_access_gen: u64,
}

impl<T: Clone> Default for SlotCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> SlotCell<T> {
    /// Create a new empty slot cell.
    pub const fn new() -> Self {
        Self {
            data: None,
            fingerprint: 0,
            last_access_gen: 0,
        }
    }

    /// Check if this cell was accessed in the current compilation.
    #[inline]
    fn is_accessed(&self) -> bool {
        self.last_access_gen == current_generation()
    }

    /// Mark this cell as accessed in the current compilation.
    #[inline]
    fn mark_accessed(&mut self) {
        self.last_access_gen = current_generation();
    }

    /// Get or initialize cached data using fingerprint-based invalidation.
    pub fn get_or_init(
        &mut self,
        load: impl FnOnce() -> FileResult<Vec<u8>>,
        process: impl FnOnce(Vec<u8>, Option<T>) -> FileResult<T>,
    ) -> FileResult<T> {
        // Fast path: already accessed in this compilation
        let was_accessed = self.is_accessed();
        self.mark_accessed();

        if was_accessed
            && let Some(data) = &self.data
        {
            return data.clone();
        }

        let result = load();
        let fingerprint = typst::utils::hash128(&result);

        // Fingerprint unchanged: reuse previous result
        if mem::replace(&mut self.fingerprint, fingerprint) == fingerprint
            && let Some(data) = &self.data
        {
            return data.clone();
        }

        // Process and cache new data
        let prev = self.data.take().and_then(Result::ok);
        let value = result.and_then(|data| process(data, prev));
        self.data = Some(value.clone());
        value
    }
}

// =============================================================================
// FileSlot - Per-file Caching
// =============================================================================

/// Holds cached data for a file ID.
pub struct FileSlot {
    id: FileId,
    source: SlotCell<Source>,
    file: SlotCell<Bytes>,
}

impl FileSlot {
    /// Create a new file slot for the given ID.
    pub const fn new(id: FileId) -> Self {
        Self {
            id,
            source: SlotCell::new(),
            file: SlotCell::new(),
        }
    }

    /// Retrieve parsed source for this file (no virtual data).
    pub fn source(&mut self, project_root: &Path) -> FileResult<Source> {
        self.source_with_virtual(project_root, &super::vfs::NoVirtualFS)
    }

    /// Retrieve parsed source using the global virtual file system.
    pub fn source_with_global_virtual(&mut self, project_root: &Path) -> FileResult<Source> {
        record_file_access(self.id);
        self.source.get_or_init(
            || read_with_global_virtual(self.id, project_root),
            |data, prev| {
                let text = decode_utf8(&data)?;
                match prev {
                    Some(mut src) => {
                        src.replace(text);
                        Ok(src)
                    }
                    None => Ok(Source::new(self.id, text.into())),
                }
            },
        )
    }

    /// Retrieve parsed source with virtual file system support.
    pub fn source_with_virtual<V: VirtualFileSystem>(
        &mut self,
        project_root: &Path,
        virtual_fs: &V,
    ) -> FileResult<Source> {
        record_file_access(self.id);
        self.source.get_or_init(
            || read_with_virtual(self.id, project_root, virtual_fs),
            |data, prev| {
                let text = decode_utf8(&data)?;
                match prev {
                    Some(mut src) => {
                        src.replace(text);
                        Ok(src)
                    }
                    None => Ok(Source::new(self.id, text.into())),
                }
            },
        )
    }

    /// Retrieve raw bytes for this file (no virtual data).
    pub fn file(&mut self, project_root: &Path) -> FileResult<Bytes> {
        self.file_with_virtual(project_root, &super::vfs::NoVirtualFS)
    }

    /// Retrieve raw bytes using the global virtual file system.
    pub fn file_with_global_virtual(&mut self, project_root: &Path) -> FileResult<Bytes> {
        record_file_access(self.id);
        self.file.get_or_init(
            || read_with_global_virtual(self.id, project_root),
            |data, _| Ok(Bytes::new(data)),
        )
    }

    /// Retrieve raw bytes with virtual file system support.
    pub fn file_with_virtual<V: VirtualFileSystem>(
        &mut self,
        project_root: &Path,
        virtual_fs: &V,
    ) -> FileResult<Bytes> {
        record_file_access(self.id);
        self.file.get_or_init(
            || read_with_virtual(self.id, project_root, virtual_fs),
            |data, _| Ok(Bytes::new(data)),
        )
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::file::access::reset_access_flags;
    use std::fs;
    use tempfile::TempDir;
    use typst::syntax::VirtualPath;

    #[test]
    fn test_slot_cell_fingerprint() {
        reset_access_flags();

        let mut slot: SlotCell<String> = SlotCell::new();

        let result1 = slot.get_or_init(
            || Ok(b"hello".to_vec()),
            |data, _| Ok(String::from_utf8(data).unwrap()),
        );
        assert_eq!(result1.unwrap(), "hello");

        // Same generation, should use cached value
        let result2 = slot.get_or_init(
            || Ok(b"hello".to_vec()),
            |_, _| panic!("Should not reprocess - same generation"),
        );
        assert_eq!(result2.unwrap(), "hello");

        // New generation, but same fingerprint - should still use cached
        reset_access_flags();
        let result3 = slot.get_or_init(
            || Ok(b"hello".to_vec()),
            |_, _| panic!("Should not reprocess - same fingerprint"),
        );
        assert_eq!(result3.unwrap(), "hello");
    }

    #[test]
    fn test_file_slot_caching() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.typ");
        fs::write(&path, "= Hello").unwrap();

        let vpath = VirtualPath::new("test.typ");
        let id = FileId::new(None, vpath);
        let mut slot = FileSlot::new(id);

        let result1 = slot.file(dir.path());
        let result2 = slot.file(dir.path());

        assert!(result1.is_ok());
        assert_eq!(result1.unwrap(), result2.unwrap());
    }
}
