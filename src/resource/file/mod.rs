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
//! Files can be cached with fingerprint-based invalidation.
//! See [`SharedFileCache`] for details.

mod access;
mod cache;
mod read;
mod resolver;
mod vfs;

pub use access::{get_accessed_files, record_file_access, reset_access_flags};
pub use cache::{FileSlot, SharedFileCache, SlotCell};
pub use read::{
    decode_utf8, file_id, file_id_from_path, read_file, virtual_file_id, EMPTY_ID, STDIN_ID,
};
pub use resolver::FileResolver;
pub use vfs::{MapVirtualFS, NoVirtualFS, PackageId, PackageVersion, VirtualFileSystem};
