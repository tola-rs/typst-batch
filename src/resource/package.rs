//! Global package storage with caching.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::OnceLock;

use typst_kit::download::Downloader;
pub use typst_kit::package::PackageStorage;

const PACKAGE_PATH_ENV: &str = "TYPST_PACKAGE_PATH";
const PACKAGE_CACHE_PATH_ENV: &str = "TYPST_PACKAGE_CACHE_PATH";

/// Options for package storage initialization.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// User-Agent string for package downloads from the Typst registry.
    ///
    /// Default: "typst-batch/{version}"
    pub user_agent: Option<String>,

    /// Local Typst package directory.
    pub package_path: Option<PathBuf>,

    /// Typst package cache directory.
    pub package_cache_path: Option<PathBuf>,
}

impl Options {
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

/// Global shared package storage.
static STORAGE: OnceLock<PackageStorage> = OnceLock::new();

/// Initialize package storage with default settings.
///
/// This can only be called once. Subsequent calls are ignored.
/// Returns `true` if storage was initialized, `false` if already initialized.
pub fn init() -> bool {
    init_with_options(Options::new())
}

/// Initialize package storage with custom options.
///
/// This can only be called once. Subsequent calls are ignored.
/// Returns `true` if storage was initialized, `false` if already initialized.
///
/// # Example
///
/// ```ignore
/// use typst_batch::resource::package;
///
/// package::init_with_options(
///     package::Options::new()
///         .with_user_agent("my-app/1.0.0")
///         .with_package_path("vendor/typst/packages"),
/// );
/// ```
pub fn init_with_options(options: Options) -> bool {
    STORAGE.set(options.storage()).is_ok()
}

/// Get the global package storage.
///
/// If not explicitly initialized, uses default settings on first access.
pub fn storage() -> &'static PackageStorage {
    STORAGE.get_or_init(|| Options::new().storage())
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
        let opts = Options::default();
        assert!(opts.user_agent.is_none());
        assert!(opts.user_agent_or_default().starts_with("typst-batch/"));
    }

    #[test]
    fn test_options_with_user_agent() {
        let opts = Options::new().with_user_agent("test/1.0");
        assert_eq!(opts.user_agent, Some("test/1.0".to_string()));
        assert_eq!(opts.user_agent_or_default(), "test/1.0");
    }

    #[test]
    fn test_options_accept_explicit_package_paths() {
        let opts = Options::new()
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
        let storage = Options::new().storage_with_env(|name| match name {
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
        let storage = Options::new()
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

        let storage = Options::new()
            .with_package_path(&package_root)
            .storage_with_env(|_| None);
        let spec: PackageSpec = "@preview/demo:0.1.0".parse().unwrap();

        let prepared = storage
            .prepare_package(&spec, &mut NoProgress)
            .expect("local package should be prepared from explicit package path");

        assert_eq!(prepared, package_dir);
    }

    #[test]
    fn test_storage_initialized() {
        let _storage = storage();
    }

    #[test]
    fn test_storage_is_shared() {
        let storage1 = storage();
        let storage2 = storage();
        assert!(std::ptr::eq(storage1, storage2), "Storage should be shared");
    }
}
