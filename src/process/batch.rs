//! Batch compilation with shared snapshot.
//!
//! Use `Batcher` for:
//! - **Batch compile**: Parallel compilation with shared file snapshot
//! - **Scan + Compile**: Avoid per-file Scanner overhead; Eval cache from `batch_scan`
//!   is reused by `batch_compile`, so Layout is the only extra cost
//!
//! Use `BatchScanner` for:
//! - **Batch scan only**: Lightweight parallel scanning without font loading
//!   (e.g., query metadata, validate links)
//!
//! # Example
//!
//! ```ignore
//! // Scan + Compile workflow
//! let batcher = Batcher::new(root).with_snapshot_from(&files)?;
//! let scans = batcher.batch_scan(&files)?;
//! let non_drafts = filter_non_drafts(&files, &scans);
//! let results = batcher.batch_compile(&non_drafts)?;
//!
//! // Scan-only workflow (no fonts, faster)
//! let scanner = BatchScanner::new(root).with_snapshot_from(&files)?;
//! let scans = scanner.batch_scan(&files)?;
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use typst::foundations::Dict;

use crate::codegen::Inputs;
use crate::diagnostic::CompileError;
use crate::resource::file::{FileResolver, SharedFileCache};
use crate::resource::font::FontStore;
use crate::world::{SnapshotConfig, SourceSnapshot, TypstWorld};

use super::compile::{CompileResult, compile_with_world};
use super::inputs::WithInputs;
#[cfg(feature = "scan")]
use super::scan::{ScanResult, scan_impl};

/// Batch compiler with shared file snapshot.
///
/// Provides `batch_scan()` and `batch_compile()` for parallel processing.
/// When used together, Eval cache from scan is reused during compile.
pub struct Batcher<'a> {
    root: &'a Path,
    pub(crate) files: Arc<FileResolver>,
    pub(crate) font_store: Arc<FontStore>,
    inputs: Option<Dict>,
    pub(crate) preludes: Vec<String>,
    pub(crate) postludes: Vec<String>,
    snapshot: Option<Arc<SourceSnapshot>>,
    pub(crate) fallback_cache: Arc<SharedFileCache>,
}

impl<'a> WithInputs for Batcher<'a> {
    fn inputs_mut(&mut self) -> &mut Option<Dict> {
        &mut self.inputs
    }
}

impl<'a> Batcher<'a> {
    /// Create a new batcher with the given root directory.
    pub fn new(root: &'a Path) -> Self {
        Self {
            root,
            files: Arc::new(FileResolver::new()),
            font_store: Arc::new(FontStore::new()),
            inputs: None,
            preludes: Vec::new(),
            postludes: Vec::new(),
            snapshot: None,
            fallback_cache: Arc::new(SharedFileCache::new()),
        }
    }

    /// Create a lightweight scanner for scan-only workflows.
    ///
    /// Returns a [`BatchScanner`] which only exposes `batch_scan()` (no `batch_compile()`).
    /// Uses no fonts and is optimized for query/validate scenarios.
    pub fn for_scan(root: &'a Path) -> BatchScanner<'a> {
        BatchScanner::new(root)
    }

    /// Use an explicit file resolver for this batcher.
    pub fn with_files(mut self, files: FileResolver) -> Self {
        self.files = Arc::new(files);
        self
    }

    /// Use an explicit fallback cache for files outside the snapshot.
    pub fn with_file_cache(mut self, cache: Arc<SharedFileCache>) -> Self {
        self.fallback_cache = cache;
        self
    }

    /// Use an explicit font store.
    pub fn with_font_store(mut self, fonts: Arc<FontStore>) -> Self {
        self.font_store = fonts;
        self
    }

    /// Add prelude code to inject at the beginning of each main file.
    pub fn with_prelude(mut self, prelude: impl Into<String>) -> Self {
        self.preludes.push(prelude.into());
        self
    }

    /// Add postlude code to inject at the end of each main file.
    pub fn with_postlude(mut self, postlude: impl Into<String>) -> Self {
        self.postludes.push(postlude.into());
        self
    }

    /// Pre-build a snapshot from files for efficient multi-phase compilation.
    ///
    /// The snapshot caches all files and their imports, enabling lock-free
    /// parallel access. Call `batch_scan()` and `batch_compile()` to reuse it.
    ///
    /// Files not in the snapshot will fall back to a batch-scoped shared cache.
    ///
    /// Note: Prelude/postlude must be set before calling this method for them
    /// to be injected into the snapshot.
    pub fn with_snapshot_from<P: AsRef<Path>>(self, paths: &[P]) -> Result<Self, CompileError> {
        self.with_snapshot_from_each(paths, |_| {})
    }

    /// Pre-build a snapshot from files with a callback for each file loaded.
    ///
    /// Like `with_snapshot_from`, but invokes the callback once per content file
    /// during snapshot construction. Useful for progress tracking.
    ///
    /// Note: Prelude/postlude must be set before calling this method for them
    /// to be injected into the snapshot.
    pub fn with_snapshot_from_each<P, F>(
        mut self,
        paths: &[P],
        on_each: F,
    ) -> Result<Self, CompileError>
    where
        P: AsRef<Path>,
        F: Fn(&Path) + Sync,
    {
        if paths.is_empty() {
            return Ok(self);
        }

        let path_bufs: Vec<PathBuf> = paths.iter().map(|p| p.as_ref().to_path_buf()).collect();

        // Build snapshot with prelude/postlude injection
        let config = SnapshotConfig {
            prelude: self.build_prelude_opt(),
            postlude: self.build_postlude_opt(),
        };

        let snapshot = Arc::new(SourceSnapshot::build_with_config_and_files(
            &path_bufs,
            self.root,
            Arc::clone(&self.files),
            &config,
            on_each,
        )?);
        self.snapshot = Some(snapshot);
        self.fallback_cache = Arc::new(SharedFileCache::new());

        Ok(self)
    }

    /// Use an existing snapshot for compilation.
    ///
    /// This allows sharing a snapshot between multiple `Batcher` instances.
    pub fn with_snapshot(mut self, snapshot: Arc<SourceSnapshot>) -> Self {
        self.snapshot = Some(snapshot);
        self.fallback_cache = Arc::new(SharedFileCache::new());
        self
    }

    /// Get the current snapshot, if any.
    ///
    /// Returns `None` if `with_snapshot_from()` or `with_snapshot()` hasn't been called.
    pub fn snapshot(&self) -> Option<Arc<SourceSnapshot>> {
        self.snapshot.clone()
    }

    /// Scan multiple files in parallel (Eval-only, skips Layout).
    ///
    /// Uses the same snapshot as `batch_compile`, enabling comemo cache reuse.
    /// Call this before `batch_compile` to filter files (e.g., skip drafts)
    /// without paying the Layout cost twice.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let batcher = Batcher::new(root).with_snapshot_from(&files)?;
    ///
    /// // Phase 1: Scan (Eval cached by comemo)
    /// let scans = batcher.batch_scan(&files)?;
    /// let non_drafts: Vec<_> = files.iter()
    ///     .zip(&scans)
    ///     .filter(|(_, r)| !is_draft(r))
    ///     .map(|(p, _)| p)
    ///     .collect();
    ///
    /// // Phase 2: Compile (Eval cache hit, only Layout runs)
    /// let results = batcher.batch_compile(&non_drafts)?;
    /// ```
    #[cfg(feature = "scan")]
    pub fn batch_scan<P: AsRef<Path> + Sync>(
        &self,
        paths: &[P],
    ) -> Result<Vec<Result<ScanResult, CompileError>>, CompileError> {
        use rayon::prelude::*;

        if paths.is_empty() {
            return Ok(vec![]);
        }

        let snapshot = self.get_or_build_snapshot(paths)?;

        // Scan in parallel with lock-free snapshot access
        let results: Vec<_> = paths
            .par_iter()
            .map(|path| {
                let path = path.as_ref();
                let world = self.build_world(path, &snapshot);
                scan_impl(&world)
            })
            .collect();

        Ok(results)
    }

    /// Compile multiple files in parallel.
    ///
    /// If `with_snapshot_from()` was called, reuses the pre-built snapshot.
    /// Otherwise, builds a new snapshot from the provided paths.
    ///
    /// Returns results in the same order as input paths.
    pub fn batch_compile<P: AsRef<Path> + Sync>(
        &self,
        paths: &[P],
    ) -> Result<Vec<Result<CompileResult, CompileError>>, CompileError> {
        self.batch_compile_each(paths, |_| {})
    }

    /// Compile multiple files in parallel with callback for each file.
    ///
    /// Like `batch_compile`, but invokes the callback once per file compiled.
    /// Useful for progress tracking.
    pub fn batch_compile_each<P, F>(
        &self,
        paths: &[P],
        on_each: F,
    ) -> Result<Vec<Result<CompileResult, CompileError>>, CompileError>
    where
        P: AsRef<Path> + Sync,
        F: Fn(&Path) + Sync,
    {
        use rayon::prelude::*;

        if paths.is_empty() {
            return Ok(vec![]);
        }

        let snapshot = self.get_or_build_snapshot(paths)?;

        // Compile in parallel with lock-free snapshot access
        let results: Vec<_> = paths
            .par_iter()
            .map(|path| {
                let path = path.as_ref();
                let world = self.build_world(path, &snapshot);
                let result = compile_with_world(&world);
                on_each(path);
                result
            })
            .collect();

        Ok(results)
    }

    /// Compile multiple files in parallel with per-file inputs.
    ///
    /// Values returned by `inputs_for` are merged over base inputs configured
    /// with `with_inputs_obj()` or `with_inputs_dict()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use typst_batch::Inputs;
    ///
    /// let batcher = Batcher::new(root)
    ///     .with_inputs_obj(global_inputs)
    ///     .with_snapshot_from(&files)?;
    ///
    /// let results = batcher.batch_compile_with_inputs(&files, |path| {
    ///     Inputs::from_json(&data_for(path)).unwrap()
    /// })?;
    /// ```
    pub fn batch_compile_with_inputs<P, F>(
        &self,
        paths: &[P],
        inputs_for: F,
    ) -> Result<Vec<Result<CompileResult, CompileError>>, CompileError>
    where
        P: AsRef<Path> + Sync,
        F: Fn(&Path) -> Inputs + Sync,
    {
        self.batch_compile_with_inputs_each(paths, inputs_for, |_| {})
    }

    /// Compile multiple files in parallel with per-file inputs and completion callback.
    pub fn batch_compile_with_inputs_each<P, F, G>(
        &self,
        paths: &[P],
        inputs_for: F,
        on_each: G,
    ) -> Result<Vec<Result<CompileResult, CompileError>>, CompileError>
    where
        P: AsRef<Path> + Sync,
        F: Fn(&Path) -> Inputs + Sync,
        G: Fn(&Path) + Sync,
    {
        use rayon::prelude::*;

        if paths.is_empty() {
            return Ok(vec![]);
        }

        let snapshot = self.get_or_build_snapshot(paths)?;

        // Compile in parallel with per-file inputs.
        let results: Vec<_> = paths
            .par_iter()
            .map(|path| {
                let path = path.as_ref();
                let world = self.build_world_with_inputs(path, &snapshot, inputs_for(path));
                let result = compile_with_world(&world);
                on_each(path);
                result
            })
            .collect();

        Ok(results)
    }

    fn build_world(&self, path: &Path, snapshot: &Arc<SourceSnapshot>) -> TypstWorld {
        let mut builder = TypstWorld::builder(path, self.root)
            .with_files(Arc::clone(&self.files))
            .with_snapshot_and_fallback(snapshot.clone(), Arc::clone(&self.fallback_cache))
            .with_fonts(Arc::clone(&self.font_store));

        if let Some(inputs) = &self.inputs {
            builder = builder.with_inputs_dict(inputs.clone());
        }

        // Pass prelude for line offset calculation in diagnostics
        // (content is already injected into snapshot, but TypstWorld needs it for prelude_line_count)
        if let Some(prelude) = self.build_prelude_opt() {
            builder = builder.with_prelude(&prelude);
        }

        builder.build()
    }

    fn build_prelude_opt(&self) -> Option<String> {
        if self.preludes.is_empty() {
            None
        } else {
            Some(self.preludes.join("\n"))
        }
    }

    fn build_postlude_opt(&self) -> Option<String> {
        if self.postludes.is_empty() {
            None
        } else {
            Some(self.postludes.join("\n"))
        }
    }

    fn get_or_build_snapshot<P: AsRef<Path>>(
        &self,
        paths: &[P],
    ) -> Result<Arc<SourceSnapshot>, CompileError> {
        match &self.snapshot {
            Some(s) => Ok(s.clone()),
            None => {
                let path_bufs: Vec<PathBuf> =
                    paths.iter().map(|p| p.as_ref().to_path_buf()).collect();
                let config = SnapshotConfig {
                    prelude: self.build_prelude_opt(),
                    postlude: self.build_postlude_opt(),
                };
                Ok(Arc::new(SourceSnapshot::build_with_config_and_files(
                    &path_bufs,
                    self.root,
                    Arc::clone(&self.files),
                    &config,
                    |_| {},
                )?))
            }
        }
    }

    fn build_world_with_inputs(
        &self,
        path: &Path,
        snapshot: &Arc<SourceSnapshot>,
        inputs: Inputs,
    ) -> TypstWorld {
        let mut merged = self.inputs.clone().unwrap_or_default();
        for (key, value) in inputs.into_dict() {
            merged.insert(key, value);
        }

        // Pass prelude for line offset calculation in diagnostics
        let mut builder = TypstWorld::builder(path, self.root)
            .with_files(Arc::clone(&self.files))
            .with_snapshot_and_fallback(snapshot.clone(), Arc::clone(&self.fallback_cache))
            .with_fonts(Arc::clone(&self.font_store))
            .with_inputs_dict(merged);

        if let Some(prelude) = self.build_prelude_opt() {
            builder = builder.with_prelude(&prelude);
        }

        builder.build()
    }
}

/// Lightweight batch scanner without font loading.
///
/// Use this for scan-only workflows (query, validate) where Layout is not needed.
/// Does not expose `batch_compile()` - use [`Batcher`] if you need compilation.
pub struct BatchScanner<'a> {
    root: &'a Path,
    files: Arc<FileResolver>,
    inputs: Option<Dict>,
    snapshot: Option<Arc<SourceSnapshot>>,
    fallback_cache: Arc<SharedFileCache>,
    prelude: Option<String>,
}

impl<'a> WithInputs for BatchScanner<'a> {
    fn inputs_mut(&mut self) -> &mut Option<Dict> {
        &mut self.inputs
    }
}

impl<'a> BatchScanner<'a> {
    /// Create a new batch scanner with the given root directory.
    pub fn new(root: &'a Path) -> Self {
        Self {
            root,
            files: Arc::new(FileResolver::new()),
            inputs: None,
            snapshot: None,
            fallback_cache: Arc::new(SharedFileCache::new()),
            prelude: None,
        }
    }

    /// Add prelude code to inject at the beginning of each main file.
    pub fn with_prelude(mut self, prelude: impl Into<String>) -> Self {
        self.prelude = Some(prelude.into());
        self
    }

    /// Use an explicit file resolver for this batch scanner.
    pub fn with_files(mut self, files: FileResolver) -> Self {
        self.files = Arc::new(files);
        self
    }

    /// Pre-build a snapshot from files for efficient batch scanning.
    pub fn with_snapshot_from<P: AsRef<Path>>(mut self, paths: &[P]) -> Result<Self, CompileError> {
        if paths.is_empty() {
            return Ok(self);
        }

        let path_bufs: Vec<PathBuf> = paths.iter().map(|p| p.as_ref().to_path_buf()).collect();
        let config = SnapshotConfig {
            prelude: self.prelude.clone(),
            postlude: None,
        };
        let snapshot = Arc::new(SourceSnapshot::build_with_config_and_files(
            &path_bufs,
            self.root,
            Arc::clone(&self.files),
            &config,
            |_| {},
        )?);
        self.snapshot = Some(snapshot);
        self.fallback_cache = Arc::new(SharedFileCache::new());

        Ok(self)
    }

    /// Scan multiple files in parallel (Eval-only, no fonts).
    #[cfg(feature = "scan")]
    pub fn batch_scan<P: AsRef<Path> + Sync>(
        &self,
        paths: &[P],
    ) -> Result<Vec<Result<ScanResult, CompileError>>, CompileError> {
        use rayon::prelude::*;

        if paths.is_empty() {
            return Ok(vec![]);
        }

        // Use existing snapshot or build a new one
        let snapshot = match &self.snapshot {
            Some(s) => s.clone(),
            None => {
                let path_bufs: Vec<PathBuf> =
                    paths.iter().map(|p| p.as_ref().to_path_buf()).collect();
                let config = SnapshotConfig {
                    prelude: self.prelude.clone(),
                    postlude: None,
                };
                Arc::new(SourceSnapshot::build_with_config_and_files(
                    &path_bufs,
                    self.root,
                    Arc::clone(&self.files),
                    &config,
                    |_| {},
                )?)
            }
        };

        // Scan in parallel with lightweight world (no fonts)
        let results: Vec<_> = paths
            .par_iter()
            .map(|path| {
                let path = path.as_ref();
                let world = self.build_world(path, &snapshot);
                scan_impl(&world)
            })
            .collect();

        Ok(results)
    }

    fn build_world(&self, path: &Path, snapshot: &Arc<SourceSnapshot>) -> TypstWorld {
        let mut builder = TypstWorld::builder(path, self.root)
            .with_files(Arc::clone(&self.files))
            .with_snapshot_and_fallback(snapshot.clone(), Arc::clone(&self.fallback_cache))
            .no_fonts();

        if let Some(inputs) = &self.inputs {
            builder = builder.with_inputs_dict(inputs.clone());
        }

        // Pass prelude for line offset calculation in diagnostics
        if let Some(prelude) = &self.prelude {
            builder = builder.with_prelude(prelude);
        }

        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::Inputs;
    use serde_json::json;
    use std::fs;
    use std::sync::Mutex;
    use tempfile::TempDir;

    #[test]
    fn batch_compile_with_inputs_uses_per_file_inputs() {
        let dir = TempDir::new().unwrap();
        let first = dir.path().join("first.typ");
        let second = dir.path().join("second.typ");
        let source = r#"#let title = sys.inputs.at("title", default: "missing")
= #title"#;
        fs::write(&first, source).unwrap();
        fs::write(&second, source).unwrap();

        let batcher = Batcher::new(dir.path())
            .with_snapshot_from(&[&first, &second])
            .unwrap();
        let results = batcher
            .batch_compile_with_inputs(&[&first, &second], |path| {
                let title = if path == first.as_path() {
                    "First"
                } else {
                    "Second"
                };
                Inputs::from_json(&json!({ "title": title })).unwrap()
            })
            .unwrap();

        let first_html = results[0].as_ref().unwrap().html().unwrap();
        let second_html = results[1].as_ref().unwrap().html().unwrap();

        assert!(String::from_utf8_lossy(&first_html).contains("First"));
        assert!(String::from_utf8_lossy(&second_html).contains("Second"));
    }

    #[test]
    fn batch_compile_with_inputs_each_reports_finished_files() {
        let dir = TempDir::new().unwrap();
        let first = dir.path().join("first.typ");
        let second = dir.path().join("second.typ");
        fs::write(&first, "= First").unwrap();
        fs::write(&second, "= Second").unwrap();

        let finished = Mutex::new(Vec::new());
        let batcher = Batcher::new(dir.path())
            .with_snapshot_from(&[&first, &second])
            .unwrap();
        let results = batcher
            .batch_compile_with_inputs_each(
                &[&first, &second],
                |_| Inputs::empty(),
                |path| finished.lock().unwrap().push(path.to_path_buf()),
            )
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(Result::is_ok));
        assert_eq!(finished.lock().unwrap().len(), 2);
    }
}
