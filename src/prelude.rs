//! Prelude module for convenient imports.
//!
//! ```ignore
//! use typst_batch::prelude::*;
//! ```

// Compilation (Builder API)
pub use crate::process::compile::{CompileResult, Compiler, MainPath, RootPath, SingleCompiler};
pub use crate::process::{AccessedDeps, CompileSession, WithInputs};
#[cfg(feature = "batch")]
pub use crate::process::batch::Batcher;
#[cfg(feature = "batch")]
pub use crate::world::SourceSnapshot;


// Fast Scanning (5-20x faster than compile)
#[cfg(feature = "scan")]
pub use crate::process::scan::{
    extract, Extractor, Heading, HeadingExtractor, Link, LinkExtractor, LinkSource,
    MetadataExtractor, ScanResult, Scanner,
};

// Diagnostics
pub use crate::diagnostic::{
    CompileError, DiagnosticFilter, DiagnosticInfo, DiagnosticOptions, DiagnosticSeverity,
    DiagnosticSummary, Diagnostics, DisplayStyle, FilterType, PackageKind, SourceDiagnostic,
    SourceLine, TraceInfo,
};

// VFS & VPS
pub use crate::resource::file::{
    file_id, file_id_from_path, get_accessed_files, reset_access_flags, virtual_file_id,
    FileResolver, MapVirtualFS, NoVirtualFS, PackageId, PackageVersion, SharedFileCache,
    VirtualFileSystem,
};

// Fonts
pub use crate::resource::font::{FontOptions, FontStore};

// Library
pub use crate::resource::library::{create_library_with_inputs, GLOBAL_LIBRARY};

// World
pub use crate::world::{normalize_path, TypstWorld, WorldBuilder};

// Package
pub use crate::resource::package;

// Resource initialization
pub use crate::resource::warmup;

// Codegen
pub use crate::codegen::{DictBuilder, Inputs, ToTypst, array, array_raw, dict, dict_raw, dict_sparse};

// HTML types (stable API)
pub use crate::html::{HtmlDocument, HtmlElement, HtmlFrame, HtmlNode, NodeKind};



/// Unstable re-exports of internal typst crates.
pub mod unstable {
    pub use typst;
    pub use typst_html;
    #[cfg(feature = "svg")]
    pub use typst_svg;
}
