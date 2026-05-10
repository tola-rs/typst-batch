//! Virtual File System trait and implementations.
//!
//! Provides abstraction for injecting virtual content into Typst's file system.

use std::path::Path;

use crate::resource::package::PackageId;
use rustc_hash::FxHashMap;

// =============================================================================
// VirtualFileSystem Trait
// =============================================================================

/// Trait for providing virtual files and packages.
///
/// This is the primary extension point for injecting dynamic content into
/// Typst's file system without physical files.
///
/// # Capabilities
///
/// 1. **Virtual paths**: Provide content for paths like `/_data/pages.json`
/// 2. **Virtual packages**: Provide content for packages like `@myapp/data:0.0.0`
///
/// # Example
///
/// ```ignore
/// use typst_batch::{FileResolver, VirtualFileSystem, PackageId, PackageVersion};
/// use std::path::Path;
///
/// struct MyVFS;
///
/// impl VirtualFileSystem for MyVFS {
///     fn read(&self, path: &Path) -> Option<Vec<u8>> {
///         match path.to_str()? {
///             "/_data/site.json" => Some(b"{}".to_vec()),
///             _ => None,
///         }
///     }
///
///     fn read_package(&self, pkg: &PackageId, path: &str) -> Option<Vec<u8>> {
///         if pkg.matches("myapp", "data", PackageVersion::new(0, 0, 0)) {
///             match path {
///                 "/lib.typ" => Some(b"#let pages = ()".to_vec()),
///                 "/typst.toml" => Some(b"[package]\nname = \"data\"".to_vec()),
///                 _ => None,
///             }
///         } else {
///             None
///         }
///     }
/// }
///
/// let files = FileResolver::new().with_virtual_fs(MyVFS);
/// ```
pub trait VirtualFileSystem: Send + Sync {
    /// Read a virtual file by path.
    ///
    /// Return `Some(bytes)` to provide virtual content, or `None` to fall
    /// back to the real filesystem.
    ///
    /// The path is root-relative (e.g., `/_data/config.json`).
    fn read(&self, path: &Path) -> Option<Vec<u8>>;

    /// Read a file from a virtual package.
    ///
    /// Return `Some(bytes)` to provide virtual package content, or `None`
    /// to fall back to normal package resolution (download from registry).
    ///
    /// # Arguments
    ///
    /// * `pkg` - Package identifier (namespace, name, version)
    /// * `path` - Path within the package (e.g., `"/lib.typ"`)
    fn read_package(&self, _pkg: &PackageId, _path: &str) -> Option<Vec<u8>> {
        None
    }
}

// =============================================================================
// NoVirtualFS - Default Implementation
// =============================================================================

/// No-op virtual file system (all files from real filesystem).
pub struct NoVirtualFS;

impl VirtualFileSystem for NoVirtualFS {
    fn read(&self, _path: &Path) -> Option<Vec<u8>> {
        None
    }
}

// =============================================================================
// MapVirtualFS - Simple Map-based Implementation
// =============================================================================

/// A simple map-based virtual file system.
///
/// Provides a convenient way to inject virtual files without implementing
/// the [`VirtualFileSystem`] trait manually.
///
/// # Example
///
/// ```ignore
/// use typst_batch::{FileResolver, MapVirtualFS};
///
/// let mut vfs = MapVirtualFS::new();
/// vfs.insert("/_data/site.json", r#"{"title":"My Blog"}"#);
/// let files = FileResolver::new().with_virtual_fs(vfs);
/// ```
#[derive(Default, Clone)]
pub struct MapVirtualFS {
    files: FxHashMap<String, Vec<u8>>,
}

impl MapVirtualFS {
    /// Create a new empty virtual file system.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a virtual file with string content.
    pub fn insert(&mut self, path: impl Into<String>, content: impl AsRef<str>) {
        self.files
            .insert(path.into(), content.as_ref().as_bytes().to_vec());
    }

    /// Insert a virtual file with binary content.
    pub fn insert_bytes(&mut self, path: impl Into<String>, content: impl Into<Vec<u8>>) {
        self.files.insert(path.into(), content.into());
    }

    /// Check if a path exists.
    pub fn contains(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// Remove a virtual file.
    pub fn remove(&mut self, path: &str) -> Option<Vec<u8>> {
        self.files.remove(path)
    }

    /// Get the number of virtual files.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Iterate over all virtual file paths.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

impl VirtualFileSystem for MapVirtualFS {
    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        let path_str = path.to_str()?;
        self.files.get(path_str).cloned()
    }
}
