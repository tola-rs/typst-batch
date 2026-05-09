//! Explicit file resolution for Typst worlds.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use typst::diag::{FileError, FileResult};
use typst::syntax::FileId;

use super::read::{EMPTY_ID, STDIN_ID, read_disk};
use super::vfs::{NoVirtualFS, PackageId, VirtualFileSystem};
use crate::resource::package;
use crate::resource::file::record_file_access;

/// Resolves Typst file IDs against virtual files, packages, and disk.
#[derive(Clone)]
pub struct FileResolver {
    vfs: Arc<dyn VirtualFileSystem>,
    package_options: package::Options,
    packages: package::Store,
}

impl FileResolver {
    /// Create a resolver with no virtual files and package paths from environment.
    pub fn new() -> Self {
        Self::from_parts(Arc::new(NoVirtualFS), package::Options::new())
    }

    /// Use a virtual filesystem for virtual paths and packages.
    pub fn with_virtual_fs<V>(self, vfs: V) -> Self
    where
        V: VirtualFileSystem + 'static,
    {
        Self::from_parts(Arc::new(vfs), self.package_options)
    }

    /// Set the local Typst package directory.
    pub fn with_package_path(self, path: impl Into<PathBuf>) -> Self {
        let options = self.package_options.with_package_path(path);
        Self::from_parts(self.vfs, options)
    }

    /// Set the Typst package cache directory.
    pub fn with_package_cache_path(self, path: impl Into<PathBuf>) -> Self {
        let options = self.package_options.with_package_cache_path(path);
        Self::from_parts(self.vfs, options)
    }

    /// Read a file ID using this resolver.
    pub fn read(&self, id: FileId, root: &Path) -> FileResult<Vec<u8>> {
        if id == *EMPTY_ID {
            return Ok(Vec::new());
        }
        if id == *STDIN_ID {
            return read_stdin();
        }

        if let Some(spec) = id.package() {
            let pkg = PackageId::from_spec(spec);
            let path = id.vpath().as_rooted_path().to_string_lossy();
            if let Some(content) = self.vfs.read_package(&pkg, &path) {
                record_file_access(id);
                return Ok(content);
            }
        }

        let vpath = id.vpath().as_rooted_path();
        if let Some(content) = self.vfs.read(vpath) {
            record_file_access(id);
            return Ok(content);
        }

        let path = self.resolve_path(root, id)?;
        read_disk(&path)
    }

    /// Check whether this resolver provides virtual content for a path.
    pub fn is_virtual_path(&self, path: &Path) -> bool {
        self.vfs.read(path).is_some()
    }

    fn from_parts(vfs: Arc<dyn VirtualFileSystem>, options: package::Options) -> Self {
        let packages = package::Store::new(options.clone());
        Self {
            vfs,
            package_options: options,
            packages,
        }
    }

    fn resolve_path(&self, root: &Path, id: FileId) -> FileResult<PathBuf> {
        let root = id
            .package()
            .map(|spec| self.packages.prepare(spec))
            .transpose()?
            .unwrap_or_else(|| root.to_path_buf());

        id.vpath().resolve(&root).ok_or(FileError::AccessDenied)
    }
}

impl Default for FileResolver {
    fn default() -> Self {
        Self::new()
    }
}

fn read_stdin() -> FileResult<Vec<u8>> {
    let mut buf = Vec::new();
    io::stdin()
        .read_to_end(&mut buf)
        .or_else(|e| {
            if e.kind() == io::ErrorKind::BrokenPipe {
                Ok(0)
            } else {
                Err(FileError::from_io(e, Path::new("<stdin>")))
            }
        })?;
    Ok(buf)
}
