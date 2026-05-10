//! Prelude module for convenient imports.
//!
//! ```ignore
//! use typst_batch::prelude::*;
//! ```

// Compilation (Builder API)
#[cfg(feature = "batch")]
pub use crate::process::batch::Batcher;
pub use crate::process::compile::{CompileResult, Compiler, SingleCompiler, compile_world};
pub use crate::process::{AccessedDeps, CompileSession, WithInputs};
#[cfg(feature = "batch")]
pub use crate::world::SourceSnapshot;

// Fast Scanning (5-20x faster than compile)
#[cfg(feature = "scan")]
pub use crate::process::scan::{
    Extractor, Heading, HeadingExtractor, Link, LinkExtractor, LinkSource, MetadataExtractor,
    ScanResult, Scanner, extract,
};

// Diagnostics
pub use crate::diagnostic::{
    CompileError, DiagnosticFilter, DiagnosticInfo, DiagnosticOptions, DiagnosticSeverity,
    DiagnosticSummary, Diagnostics, DisplayStyle, FilterType, PackageKind, SourceDiagnostic,
    SourceLine, TraceInfo,
};

// VFS
pub use crate::resource::file::{
    FileResolver, MapVirtualFS, NoVirtualFS, SharedFileCache, VirtualFileSystem, file_id,
    file_id_from_path, get_accessed_files, reset_access_flags, virtual_file_id,
};

// Fonts
pub use crate::resource::font::{FontOptions, FontStore};

// Library
pub use crate::resource::library::{GLOBAL_LIBRARY, create_library_with_inputs};

// World
pub use crate::world::{TypstWorld, WorldBuilder, normalize_path};

// Package
pub use crate::resource::package;
pub use crate::resource::package::{
    PackageId, PackageIdParseError, PackageOptions, PackageStore, PackageVersion,
};

// Codegen
pub use crate::codegen::{
    DictBuilder, Inputs, ToTypst, array, array_raw, dict, dict_raw, dict_sparse,
};

// HTML types (stable API)
pub use crate::html::{HtmlDocument, HtmlElement, HtmlFrame, HtmlNode, NodeKind};

/// Unstable re-exports of internal typst crates.
pub mod unstable {
    pub use typst;
    pub use typst_html;
    #[cfg(feature = "svg")]
    pub use typst_svg;
}
