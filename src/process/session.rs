//! Compile session for tracking file access.
//!
//! Encapsulates the side effects of `reset_tracking()` and `collect_accessed_*()`.

use std::path::{Path, PathBuf};

use crate::resource::file::FileResolver;
use crate::resource::package::PackageId;

use super::common::{collect_accessed_files, collect_accessed_packages, reset_tracking};

/// Tracks file and package access during compilation/scanning.
///
/// Create a session before compilation, then call `finish()` to collect results.
///
/// # Example
///
/// ```ignore
/// let session = CompileSession::start();
/// let result = typst::compile(world);
/// let deps = session.finish(world.root(), world.files());
/// // deps.files and deps.packages now available
/// ```
pub struct CompileSession {
    _private: (),
}

impl CompileSession {
    /// Start a new compile session, resetting access tracking.
    #[inline]
    pub fn start() -> Self {
        reset_tracking();
        Self { _private: () }
    }

    /// Finish the session and collect accessed files/packages.
    #[inline]
    pub fn finish(self, root: &Path, files: &FileResolver) -> AccessedDeps {
        AccessedDeps {
            files: collect_accessed_files(root, files),
            packages: collect_accessed_packages(),
        }
    }
}

/// Files and packages accessed during compilation/scanning.
#[derive(Debug, Clone, Default)]
pub struct AccessedDeps {
    /// Files accessed during compilation (relative to root).
    pub files: Vec<PathBuf>,
    /// Packages accessed during compilation.
    pub packages: Vec<PackageId>,
}
