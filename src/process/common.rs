//! Common utilities shared between compile and scan.

use std::path::{Path, PathBuf};

use rustc_hash::FxHashSet;

use crate::resource::file::{FileResolver, get_accessed_files, reset_access_flags};
use crate::resource::package::PackageId;

/// Reset file access tracking before compilation/scanning.
#[inline]
pub fn reset_tracking() {
    reset_access_flags();
}

/// Collect files accessed during compilation/scanning.
///
/// Returns paths relative to root, including virtual paths.
/// Note: Package files are excluded; use `collect_accessed_packages()` for those.
pub fn collect_accessed_files(root: &Path, files: &FileResolver) -> Vec<PathBuf> {
    get_accessed_files()
        .into_iter()
        .filter(|id| id.package().is_none())
        .filter_map(|id| {
            id.vpath().resolve(root).or_else(|| {
                let vpath = id.vpath().as_rooted_path();
                if files.is_virtual_path(vpath) {
                    Some(vpath.to_path_buf())
                } else {
                    None
                }
            })
        })
        .collect()
}

/// Collect packages accessed during compilation/scanning.
///
/// Returns unique package IDs that were imported during compilation.
/// Useful for detecting virtual package usage (e.g., `@myapp/data`).
pub fn collect_accessed_packages() -> Vec<PackageId> {
    let mut seen: FxHashSet<PackageId> = FxHashSet::default();
    get_accessed_files()
        .into_iter()
        .filter_map(|id| id.package().map(PackageId::from_spec))
        .filter(|pkg| seen.insert(pkg.clone()))
        .collect()
}
