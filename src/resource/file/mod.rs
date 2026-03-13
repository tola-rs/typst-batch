//! File system abstraction with virtual file and package support.
//!
//! This module provides a layered file system for Typst compilation:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    File Access Flow                         │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                             │
//! │  FileId ──► read_file(id, root)                             │
//! │                    │                                        │
//! │                    ├─► Special IDs (EMPTY, STDIN)           │
//! │                    │                                        │
//! │                    ├─► Virtual Package (@myapp/data:0.0.0)  │
//! │                    │   └─► VirtualFileSystem::read_package  │
//! │                    │                                        │
//! │                    ├─► Virtual Path (/_data/*.json)         │
//! │                    │   └─► VirtualFileSystem::read          │
//! │                    │                                        │
//! │                    └─► Physical File                        │
//! │                        └─► resolve_path() + read_disk()     │
//! │                                                             │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Virtual File System
//!
//! The [`VirtualFileSystem`] trait allows injecting virtual content:
//!
//! - **Virtual paths**: `/_data/*.json` for site metadata
//! - **Virtual packages**: `@myapp/data:0.0.0` for typed data access
//!
//! # Caching
//!
//! Files are cached globally with fingerprint-based invalidation.
//! See [`cache`] module for details.

mod access;
mod cache;
mod read;
mod vfs;

pub use access::{get_accessed_files, record_file_access, reset_access_flags};
pub use cache::{clear_file_cache, FileSlot, SharedFileCache, SlotCell, GLOBAL_FILE_CACHE};
pub use read::{
    decode_utf8, file_id, file_id_from_path, read_file, read_with_global_virtual, virtual_file_id,
    EMPTY_ID, STDIN_ID,
};
pub use vfs::{is_virtual_path, set_virtual_fs, MapVirtualFS, NoVirtualFS, PackageId, PackageVersion, VirtualFileSystem};
