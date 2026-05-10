//! Package storage configuration.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use typst::diag::{FileError, FileResult};
use typst::syntax::package::{PackageSpec, PackageVersion as TypstPackageVersion};
use typst_kit::download::{Downloader, ProgressSink};
pub use typst_kit::package::PackageStorage;

const PACKAGE_PATH_ENV: &str = "TYPST_PACKAGE_PATH";
const PACKAGE_CACHE_PATH_ENV: &str = "TYPST_PACKAGE_CACHE_PATH";

/// A semantic version number for Typst packages.
///
/// Typst package versions use three numeric components: major, minor, patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PackageVersion {
    /// Major version number.
    pub major: u32,
    /// Minor version number.
    pub minor: u32,
    /// Patch version number.
    pub patch: u32,
}

impl PackageVersion {
    /// Create a version.
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub(crate) const fn to_typst(self) -> TypstPackageVersion {
        TypstPackageVersion {
            major: self.major,
            minor: self.minor,
            patch: self.patch,
        }
    }
}

impl fmt::Display for PackageVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Error returned when parsing a Typst package specifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageIdParseError {
    message: String,
}

impl fmt::Display for PackageIdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PackageIdParseError {}

/// Identifies a Typst package by namespace, name, and version.
///
/// This is typst-batch's own type, so users do not need to depend on Typst's
/// internal package representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageId {
    namespace: String,
    name: String,
    version: PackageVersion,
}

impl PackageId {
    /// Create a package identifier.
    pub fn new(
        namespace: impl Into<String>,
        name: impl Into<String>,
        version: PackageVersion,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            name: name.into(),
            version,
        }
    }

    /// Parse a Typst package specifier, such as `@preview/example:1.2.3`.
    pub fn parse(source: impl AsRef<str>) -> Result<Self, PackageIdParseError> {
        source.as_ref().parse()
    }

    /// Get the package namespace, such as `"preview"`.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Get the package name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the package version.
    pub fn version(&self) -> PackageVersion {
        self.version
    }

    /// Check whether this package has the given namespace, name, and version.
    pub fn matches(&self, namespace: &str, name: &str, version: PackageVersion) -> bool {
        self.namespace == namespace && self.name == name && self.version == version
    }

    pub(crate) fn from_spec(spec: &PackageSpec) -> Self {
        Self {
            namespace: spec.namespace.as_str().to_string(),
            name: spec.name.as_str().to_string(),
            version: PackageVersion {
                major: spec.version.major,
                minor: spec.version.minor,
                patch: spec.version.patch,
            },
        }
    }

    pub(crate) fn to_spec(&self) -> PackageSpec {
        PackageSpec {
            namespace: self.namespace.as_str().into(),
            name: self.name.as_str().into(),
            version: self.version.to_typst(),
        }
    }
}

impl FromStr for PackageId {
    type Err = PackageIdParseError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        let spec = source
            .parse::<PackageSpec>()
            .map_err(|err| PackageIdParseError {
                message: err.to_string(),
            })?;
        Ok(Self::from_spec(&spec))
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}/{}:{}", self.namespace, self.name, self.version)
    }
}

/// Options for package storage initialization.
#[derive(Debug, Clone, Default)]
pub struct PackageOptions {
    /// User-Agent string for package downloads from the Typst registry.
    ///
    /// Default: "typst-batch/{version}"
    pub user_agent: Option<String>,

    /// Local Typst package directory.
    pub package_path: Option<PathBuf>,

    /// Typst package cache directory.
    pub package_cache_path: Option<PathBuf>,
}

impl PackageOptions {
    /// Create new options with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the User-Agent string.
    pub fn with_user_agent(mut self, agent: impl Into<String>) -> Self {
        self.user_agent = Some(agent.into());
        self
    }

    /// Set the local Typst package directory.
    pub fn with_package_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.package_path = Some(path.into());
        self
    }

    /// Set the Typst package cache directory.
    pub fn with_package_cache_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.package_cache_path = Some(path.into());
        self
    }

    fn user_agent_or_default(&self) -> String {
        self.user_agent
            .clone()
            .unwrap_or_else(|| concat!("typst-batch/", env!("CARGO_PKG_VERSION")).to_string())
    }

    fn storage(&self) -> PackageStorage {
        self.storage_with_env(|name| std::env::var_os(name))
    }

    fn storage_with_env(&self, env: impl Fn(&str) -> Option<OsString>) -> PackageStorage {
        let package_path = self
            .package_path
            .clone()
            .or_else(|| env(PACKAGE_PATH_ENV).map(PathBuf::from));
        let package_cache_path = self
            .package_cache_path
            .clone()
            .or_else(|| env(PACKAGE_CACHE_PATH_ENV).map(PathBuf::from));

        PackageStorage::new(
            package_cache_path,
            package_path,
            Downloader::new(self.user_agent_or_default()),
        )
    }
}

/// Package storage owned by an explicit file resolver.
#[derive(Clone)]
pub struct PackageStore {
    options: PackageOptions,
    storage: Arc<PackageStorage>,
}

impl PackageStore {
    /// Create a package store from explicit options.
    pub fn new(options: PackageOptions) -> Self {
        Self {
            options: options.clone(),
            storage: Arc::new(options.storage()),
        }
    }

    /// Return the options used to create this store.
    pub fn options(&self) -> &PackageOptions {
        &self.options
    }

    /// Make a package available and return its directory.
    pub fn prepare(&self, package: &PackageId) -> FileResult<PathBuf> {
        self.prepare_spec(&package.to_spec())
    }

    /// Parse, prepare, and return a package directory.
    pub fn prepare_package(&self, package: impl AsRef<str>) -> FileResult<PathBuf> {
        let package = package
            .as_ref()
            .parse()
            .map_err(|err: PackageIdParseError| FileError::Other(Some(err.to_string().into())))?;
        self.prepare(&package)
    }

    pub(crate) fn prepare_spec(&self, spec: &PackageSpec) -> FileResult<PathBuf> {
        Ok(self.storage.prepare_package(spec, &mut ProgressSink)?)
    }
}

impl Default for PackageStore {
    fn default() -> Self {
        Self::new(PackageOptions::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;
    use typst::syntax::package::PackageSpec;
    use typst_kit::download::{DownloadState, Progress};

    struct NoProgress;

    impl Progress for NoProgress {
        fn print_start(&mut self) {}
        fn print_progress(&mut self, _: &DownloadState) {}
        fn print_finish(&mut self, _: &DownloadState) {}
    }

    #[test]
    fn test_options_default() {
        let opts = PackageOptions::default();
        assert!(opts.user_agent.is_none());
        assert!(opts.user_agent_or_default().starts_with("typst-batch/"));
    }

    #[test]
    fn test_package_id_can_be_constructed_directly() {
        let pkg = PackageId::new("preview", "demo", PackageVersion::new(1, 2, 3));

        assert_eq!(pkg.namespace(), "preview");
        assert_eq!(pkg.name(), "demo");
        assert_eq!(pkg.version(), PackageVersion::new(1, 2, 3));
        assert_eq!(pkg.to_string(), "@preview/demo:1.2.3");
    }

    #[test]
    fn test_package_id_can_be_parsed_from_typst_syntax() {
        let pkg = PackageId::parse("@preview/demo:1.2.3").unwrap();

        assert!(pkg.matches("preview", "demo", PackageVersion::new(1, 2, 3)));
    }

    #[test]
    fn test_options_with_user_agent() {
        let opts = PackageOptions::new().with_user_agent("test/1.0");
        assert_eq!(opts.user_agent, Some("test/1.0".to_string()));
        assert_eq!(opts.user_agent_or_default(), "test/1.0");
    }

    #[test]
    fn test_options_accept_explicit_package_paths() {
        let opts = PackageOptions::new()
            .with_package_path("explicit/packages")
            .with_package_cache_path("explicit/cache");

        assert_eq!(opts.package_path, Some(PathBuf::from("explicit/packages")));
        assert_eq!(
            opts.package_cache_path,
            Some(PathBuf::from("explicit/cache"))
        );
    }

    #[test]
    fn test_options_read_typst_package_paths_from_env() {
        let storage = PackageOptions::new().storage_with_env(|name| match name {
            "TYPST_PACKAGE_PATH" => Some(OsString::from("data/typst/packages")),
            "TYPST_PACKAGE_CACHE_PATH" => Some(OsString::from("cache/typst/packages")),
            _ => None,
        });

        assert_eq!(
            storage.package_path(),
            Some(Path::new("data/typst/packages"))
        );
        assert_eq!(
            storage.package_cache_path(),
            Some(Path::new("cache/typst/packages"))
        );
    }

    #[test]
    fn test_explicit_package_paths_override_typst_env() {
        let storage = PackageOptions::new()
            .with_package_path("explicit/packages")
            .with_package_cache_path("explicit/cache")
            .storage_with_env(|name| match name {
                "TYPST_PACKAGE_PATH" => Some(OsString::from("env/packages")),
                "TYPST_PACKAGE_CACHE_PATH" => Some(OsString::from("env/cache")),
                _ => None,
            });

        assert_eq!(storage.package_path(), Some(Path::new("explicit/packages")));
        assert_eq!(
            storage.package_cache_path(),
            Some(Path::new("explicit/cache"))
        );
    }

    #[test]
    fn test_explicit_package_path_prepares_local_typst_package() {
        let dir = TempDir::new().unwrap();
        let package_root = dir.path().join("typst-packages");
        let package_dir = package_root.join("preview").join("demo").join("0.1.0");
        std::fs::create_dir_all(&package_dir).unwrap();

        let storage = PackageOptions::new()
            .with_package_path(&package_root)
            .storage_with_env(|_| None);
        let spec: PackageSpec = "@preview/demo:0.1.0".parse().unwrap();

        let prepared = storage
            .prepare_package(&spec, &mut NoProgress)
            .expect("local package should be prepared from explicit package path");

        assert_eq!(prepared, package_dir);
    }

    #[test]
    fn test_store_prepares_from_options() {
        let dir = TempDir::new().unwrap();
        let package_root = dir.path().join("typst-packages");
        let package_dir = package_root.join("preview").join("demo").join("0.1.0");
        std::fs::create_dir_all(&package_dir).unwrap();

        let store = PackageStore::new(PackageOptions::new().with_package_path(&package_root));
        let package = PackageId::new("preview", "demo", PackageVersion::new(0, 1, 0));

        assert_eq!(store.prepare(&package).unwrap(), package_dir);
    }

    #[test]
    fn test_store_prepares_from_package_string() {
        let dir = TempDir::new().unwrap();
        let package_root = dir.path().join("typst-packages");
        let package_dir = package_root.join("preview").join("demo").join("0.1.0");
        std::fs::create_dir_all(&package_dir).unwrap();

        let store = PackageStore::new(PackageOptions::new().with_package_path(&package_root));

        assert_eq!(
            store.prepare_package("@preview/demo:0.1.0").unwrap(),
            package_dir
        );
    }
}
