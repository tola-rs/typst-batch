//! Immutable file snapshot for lock-free parallel compilation.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use typst::diag::FileResult;
use typst::syntax::{FileId, Source, VirtualPath};

use super::path::normalize_path;
use super::source::{read_source, read_source_with_injection};
use crate::resource::file::file_id_from_path;

/// Error when building a file snapshot.
#[derive(Debug)]
pub struct SnapshotError {
    /// The file path that failed to load.
    pub path: PathBuf,
    /// The underlying IO error.
    pub source: std::io::Error,
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "failed to read {}: {}", self.path.display(), self.source)
    }
}

impl std::error::Error for SnapshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Configuration for building a snapshot with prelude/postlude injection.
#[derive(Default, Clone)]
pub struct SnapshotConfig {
    /// Code to inject at the beginning of each main file.
    pub prelude: Option<String>,
    /// Code to inject at the end of each main file.
    pub postlude: Option<String>,
}

/// Immutable source snapshot for lock-free parallel access.
///
/// Built once before parallel compilation, then shared across all threads.
#[derive(Clone)]
pub struct SourceSnapshot {
    sources: Arc<FxHashMap<FileId, Source>>,
}

impl SourceSnapshot {
    /// Build a snapshot by pre-scanning all content files and their imports.
    ///
    /// Returns an error if any content file fails to load.
    pub fn build(content_files: &[PathBuf], root: &Path) -> Result<Self, SnapshotError> {
        Self::build_with_config(content_files, root, &SnapshotConfig::default(), |_| {})
    }

    /// Build a snapshot with callback for each file loaded.
    ///
    /// Returns an error if any content file fails to load.
    pub fn build_each(
        content_files: &[PathBuf],
        root: &Path,
        on_load: impl Fn(&Path) + Sync,
    ) -> Result<Self, SnapshotError> {
        Self::build_with_config(content_files, root, &SnapshotConfig::default(), on_load)
    }

    /// Build a snapshot with prelude/postlude injection.
    ///
    /// The prelude is injected at the beginning of each main file, and its imports
    /// are also included in the snapshot. This ensures all dependencies are available
    /// during compilation.
    pub fn build_with_config(
        content_files: &[PathBuf],
        root: &Path,
        config: &SnapshotConfig,
        on_load: impl Fn(&Path) + Sync,
    ) -> Result<Self, SnapshotError> {
        let root = normalize_path(root);

        // Collect main file IDs for prelude injection
        let main_ids: FxHashSet<FileId> = content_files
            .iter()
            .filter_map(|p| file_id_from_path(p, &root))
            .collect();

        let sources = load_sources_with_imports(content_files, &root, config, &main_ids, on_load)?;

        Ok(Self {
            sources: Arc::new(sources),
        })
    }

    /// Gets a cached source by file ID.
    #[inline]
    pub fn get_source(&self, id: FileId) -> Option<Source> {
        self.sources.get(&id).cloned()
    }

    /// Returns the number of cached sources.
    #[inline]
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }
}

// ============================================================================
// Source Loading
// ============================================================================

fn load_sources_with_imports(
    content_files: &[PathBuf],
    root: &Path,
    config: &SnapshotConfig,
    main_ids: &FxHashSet<FileId>,
    on_load: impl Fn(&Path) + Sync,
) -> Result<FxHashMap<FileId, Source>, SnapshotError> {
    use rayon::prelude::*;
    use std::sync::Mutex;

    let sources = Mutex::new(FxHashMap::default());
    let first_error: Mutex<Option<SnapshotError>> = Mutex::new(None);

    // Load initial files in parallel (with prelude/postlude injection for main files)
    let initial: Vec<_> = content_files
        .par_iter()
        .filter_map(|path| {
            // Skip if we already have an error
            if first_error.lock().unwrap().is_some() {
                return None;
            }

            let id = match file_id_from_path(path, root) {
                Some(id) => id,
                None => return None, // Path outside root, skip
            };

            match load_source_with_injection(id, root, config, main_ids) {
                Ok(source) => {
                    on_load(path);
                    Some((id, source))
                }
                Err(_) => {
                    // Record the first error
                    let mut err = first_error.lock().unwrap();
                    if err.is_none() {
                        *err = Some(SnapshotError {
                            path: path.clone(),
                            source: std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                format!("failed to load source: {}", path.display()),
                            ),
                        });
                    }
                    None
                }
            }
        })
        .collect();

    // Check for errors
    if let Some(err) = first_error.into_inner().unwrap() {
        return Err(err);
    }

    // Collect imports from initial files (prelude imports are included since prelude was injected)
    let mut pending: Vec<FileId> = Vec::new();
    let mut queued: FxHashSet<FileId> = FxHashSet::default();
    for (id, source) in initial {
        for import_id in parse_imports(&source) {
            if queued.insert(import_id) {
                pending.push(import_id);
            }
        }
        sources.lock().unwrap().insert(id, source);
    }

    // BFS to load all imports (imports are optional, skip failures)
    while !pending.is_empty() {
        let batch: Vec<_> = {
            let sources = sources.lock().unwrap();
            pending
                .drain(..)
                .filter(|id| !sources.contains_key(id))
                .collect()
        };

        if batch.is_empty() {
            break;
        }

        // For imports, we silently skip failures (they might be package imports
        // or optional files that will be handled at compile time)
        let results: Vec<_> = batch
            .par_iter()
            .filter_map(|&id| read_source(id, root).ok().map(|s| (id, s)))
            .collect();

        let mut sources = sources.lock().unwrap();
        for (id, source) in results {
            if sources.contains_key(&id) {
                continue;
            }
            for import_id in parse_imports(&source) {
                if !sources.contains_key(&import_id) && queued.insert(import_id) {
                    pending.push(import_id);
                }
            }
            sources.insert(id, source);
        }
    }

    Ok(sources.into_inner().unwrap())
}

/// Load source with prelude/postlude injection for main files.
fn load_source_with_injection(
    id: FileId,
    root: &Path,
    config: &SnapshotConfig,
    main_ids: &FxHashSet<FileId>,
) -> FileResult<Source> {
    read_source_with_injection(
        id,
        root,
        main_ids.contains(&id),
        config.prelude.as_deref(),
        config.postlude.as_deref(),
    )
}

// ============================================================================
// Import Parsing
// ============================================================================

fn parse_imports(source: &Source) -> Vec<FileId> {
    use typst::syntax::{ast, SyntaxKind};

    let mut imports = Vec::new();
    let mut stack = vec![source.root().clone()];
    let current = source.id();

    while let Some(node) = stack.pop() {
        match node.kind() {
            SyntaxKind::ModuleImport => {
                if let Some(import) = node.cast::<ast::ModuleImport>()
                    && let Some(id) = resolve_import_path(&import.source(), current) {
                        imports.push(id);
                    }
            }
            SyntaxKind::ModuleInclude => {
                if let Some(include) = node.cast::<ast::ModuleInclude>()
                    && let Some(id) = resolve_import_path(&include.source(), current) {
                        imports.push(id);
                    }
            }
            _ => stack.extend(node.children().cloned()),
        }
    }

    imports
}

fn resolve_import_path(expr: &typst::syntax::ast::Expr, current: FileId) -> Option<FileId> {
    use typst::syntax::ast;

    let path_str = match expr {
        ast::Expr::Str(s) => s.get(),
        _ => return None,
    };

    // Skip package imports
    if path_str.starts_with('@') {
        return None;
    }

    let resolved = if path_str.starts_with('/') {
        VirtualPath::new(&*path_str)
    } else {
        current.vpath().join(&*path_str)
    };

    Some(FileId::new(None, resolved))
}
