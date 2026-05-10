//! Explicit file resolution for Typst worlds.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use typst::diag::{FileError, FileResult};
use typst::syntax::FileId;

use super::read::{EMPTY_ID, STDIN_ID, read_disk};
use super::vfs::{NoVirtualFS, VirtualFileSystem};
use crate::resource::file::record_file_access;
use crate::resource::package::{PackageId, PackageOptions, PackageStore};

/// Resolves Typst file IDs against virtual files, packages, and disk.
#[derive(Clone)]
pub struct FileResolver {
    vfs: Arc<dyn VirtualFileSystem>,
    packages: PackageStore,
}

impl FileResolver {
    /// Create a resolver with no virtual files and package paths from environment.
    pub fn new() -> Self {
        Self {
            vfs: Arc::new(NoVirtualFS),
            packages: PackageStore::default(),
        }
    }

    /// Use a virtual filesystem for virtual paths and packages.
    pub fn with_virtual_fs<V>(self, vfs: V) -> Self
    where
        V: VirtualFileSystem + 'static,
    {
        Self {
            vfs: Arc::new(vfs),
            packages: self.packages,
        }
    }

    /// Use explicit package storage options.
    pub fn with_package_options(self, options: PackageOptions) -> Self {
        Self {
            vfs: self.vfs,
            packages: PackageStore::new(options),
        }
    }

    /// Use an explicit package store.
    pub fn with_package_store(self, store: PackageStore) -> Self {
        Self {
            vfs: self.vfs,
            packages: store,
        }
    }

    /// Set the local Typst package directory.
    pub fn with_package_path(self, path: impl Into<PathBuf>) -> Self {
        let options = self.packages.options().clone().with_package_path(path);
        self.with_package_options(options)
    }

    /// Set the Typst package cache directory.
    pub fn with_package_cache_path(self, path: impl Into<PathBuf>) -> Self {
        let options = self
            .packages
            .options()
            .clone()
            .with_package_cache_path(path);
        self.with_package_options(options)
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

    fn resolve_path(&self, root: &Path, id: FileId) -> FileResult<PathBuf> {
        let root = id
            .package()
            .map(|spec| self.packages.prepare_spec(spec))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use typst::syntax::{FileId, VirtualPath};

    #[test]
    fn resolver_accepts_preconfigured_package_store() {
        let dir = TempDir::new().unwrap();
        let package_root = dir.path().join("typst-packages");
        let package_dir = package_root.join("preview").join("demo").join("0.1.0");
        fs::create_dir_all(&package_dir).unwrap();
        fs::write(package_dir.join("lib.typ"), "#let value = 1").unwrap();

        let store = PackageStore::new(PackageOptions::new().with_package_path(&package_root));
        let spec = "@preview/demo:0.1.0".parse().unwrap();
        let id = FileId::new(Some(spec), VirtualPath::new("lib.typ"));
        let files = FileResolver::new().with_package_store(store);

        let bytes = files.read(id, dir.path()).unwrap();

        assert_eq!(bytes, b"#let value = 1");
    }
}

fn read_stdin() -> FileResult<Vec<u8>> {
    let mut buf = Vec::new();
    io::stdin().read_to_end(&mut buf).or_else(|e| {
        if e.kind() == io::ErrorKind::BrokenPipe {
            Ok(0)
        } else {
            Err(FileError::from_io(e, Path::new("<stdin>")))
        }
    })?;
    Ok(buf)
}
