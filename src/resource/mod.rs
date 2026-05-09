//! Shared resources for Typst compilation (fonts, packages, file cache).

pub mod file;
pub mod font;
pub mod library;
pub mod package;

use std::path::Path;
use std::sync::{Arc, LazyLock};

/// Initialize immutable shared resources and return a warmed font store.
pub fn warmup(font_dirs: &[&Path]) -> Arc<font::FontStore> {
    // Initialize library
    LazyLock::force(&library::GLOBAL_LIBRARY);

    let fonts = Arc::new(font::FontStore::with_paths(font_dirs));
    fonts.get();
    fonts
}
