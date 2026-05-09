//! File caching with fingerprint-based invalidation.
//!
//! # Caching Strategy
//!
//! ```text
//! SharedFileCache
//! └── FxHashMap<FileId, Arc<Mutex<FileSlot>>>
//!     └── FileSlot
//!         ├── source: SlotCell<Source>  ─┐
//!         └── file: SlotCell<Bytes>     ─┼── Fingerprint-based invalidation
//! ```

use std::sync::Arc;
use std::mem;
use std::path::Path;

use parking_lot::{Mutex, RwLock};
use rustc_hash::FxHashMap;
use typst::diag::FileResult;
use typst::foundations::Bytes;
use typst::syntax::{FileId, Source};

use super::access::{current_generation, record_file_access};
use super::read::decode_utf8;
use super::FileResolver;

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
        typst::comemo::evict(0);
    }

    /// Read and cache source text using an explicit file resolver.
    pub fn source_with_files(
        &self,
        id: FileId,
        root: &Path,
        files: &FileResolver,
    ) -> FileResult<Source> {
        self.slot(id).lock().source_with_files(root, files)
    }

    /// Read and cache raw bytes using an explicit file resolver.
    pub fn file_with_files(
        &self,
        id: FileId,
        root: &Path,
        files: &FileResolver,
    ) -> FileResult<Bytes> {
        self.slot(id).lock().file_with_files(root, files)
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

    /// Retrieve parsed source using an explicit file resolver.
    pub fn source_with_files(
        &mut self,
        project_root: &Path,
        files: &FileResolver,
    ) -> FileResult<Source> {
        record_file_access(self.id);
        self.source.get_or_init(
            || files.read(self.id, project_root),
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

    /// Retrieve raw bytes using an explicit file resolver.
    pub fn file_with_files(
        &mut self,
        project_root: &Path,
        files: &FileResolver,
    ) -> FileResult<Bytes> {
        record_file_access(self.id);
        self.file
            .get_or_init(|| files.read(self.id, project_root), |data, _| Ok(Bytes::new(data)))
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

        let files = FileResolver::new();
        let result1 = slot.file_with_files(dir.path(), &files);
        let result2 = slot.file_with_files(dir.path(), &files);

        assert!(result1.is_ok());
        assert_eq!(result1.unwrap(), result2.unwrap());
    }
}
