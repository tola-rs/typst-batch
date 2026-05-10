# typst-batch

Typst → HTML batch compilation library with shared resources.

> **Note**: This library was extracted from [tola-ssg](https://github.com/tola-ssg/tola-ssg). It is designed for Typst → HTML workflows and may not be generic enough for all use cases.

## Features

- **Shared resources**: Fonts loaded once, packages cached
- **Batch compilation**: Parallel processing with shared file snapshot
- **Fast scanning**: Skip Layout phase for metadata extraction
- **Virtual file system**: Inject dynamic content without physical files
- **Structured diagnostics**: Rich error messages with source locations

## Installation

```toml
[dependencies]
typst-batch = "0.1"
```

### Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `colored-diagnostics` | ✓ | ANSI colored output |
| `scan` | ✓ | Fast scanning API (skips Layout) |
| `batch` | ✓ | Parallel batch compilation (rayon) |
| `svg` | | SVG rendering for frames |

## Quick Start

```rust
use typst_batch::prelude::*;
use std::path::Path;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fonts = Arc::new(FontStore::with_paths(&[]).preload());

    // Compile a single file
    let result = Compiler::new(Path::new("."))
        .with_font_store(fonts)
        .with_path(Path::new("doc.typ"))
        .compile()?;

    let html = result.html()?;
    std::fs::write("output.html", &html)?;

    // Print warnings
    if !result.diagnostics().is_empty() {
        eprintln!("{}", result.diagnostics());
    }

    Ok(())
}
```

## API

### Single File Compilation

```rust
use typst_batch::prelude::*;

// Basic compilation
let result = Compiler::new(root)
    .with_path(path)
    .compile()?;

// With sys.inputs
let result = Compiler::new(root)
    .with_inputs([("title", "Hello"), ("author", "Alice")])
    .with_path(path)
    .compile()?;

// Query metadata: #metadata((title: "Hello")) <post-meta>
if let Some(meta) = result.document().query_metadata("post-meta") {
    println!("Title: {}", meta["title"]);
}
```

### Batch Compilation

```rust
use typst_batch::prelude::*;

// Create batcher with shared snapshot
let batcher = Compiler::new(root)
    .into_batch()
    .with_inputs_obj(inputs)
    .with_snapshot_from(&files)?;

// Batch scan (Eval-only, skips Layout)
let scans = batcher.batch_scan(&files)?;

// Filter drafts, then compile
let non_drafts: Vec<_> = files.iter()
    .zip(&scans)
    .filter(|(_, r)| r.as_ref().map(|s| !s.is_draft()).unwrap_or(false))
    .map(|(p, _)| p)
    .collect();

// Batch compile with completion callback
let results = batcher.batch_compile_each(&non_drafts, |path| {
    println!("Compiled: {}", path.display());
})?;

// Batch compile with per-file inputs
let results = batcher.batch_compile_with_inputs(&files, |path| {
    Inputs::from_json(&serde_json::json!({
        "current_file": path.to_string_lossy(),
    })).unwrap()
})?;
```

### Fast Scanning

```rust
use typst_batch::prelude::*;

// Single file scan
let result = Scanner::new(root).scan(path)?;
let links = result.links();
let headings = result.headings();
let meta = result.metadata("post-meta");

// Batch scan (lightweight, no fonts)
let scanner = Batcher::for_scan(root)
    .with_snapshot_from(&files)?;
let scans = scanner.batch_scan(&files)?;
```

### Virtual File System

```rust
use typst_batch::prelude::*;

let mut vfs = MapVirtualFS::new();
vfs.insert("/_data/site.json", r#"{"title":"My Blog"}"#);

let files = FileResolver::new().with_virtual_fs(vfs);
let result = Compiler::new(root)
    .with_files(files)
    .with_path(path)
    .compile()?;
```

In Typst:
```typst
#let site = json("/_data/site.json")
= #site.title
```

### Package Storage

```rust
use typst_batch::prelude::*;

let packages = PackageStore::new(
    PackageOptions::new()
        .with_package_path("vendor/typst-packages")
        .with_package_cache_path(".cache/typst-packages"),
);

let metalogo = PackageId::new("preview", "metalogo", PackageVersion::new(1, 2, 0));
let package_dir = packages.prepare(&metalogo)?;

let files = FileResolver::new().with_package_store(packages);
```

If no explicit paths are provided, package storage reads
`TYPST_PACKAGE_PATH` and `TYPST_PACKAGE_CACHE_PATH`.

### SVG Frame Rendering

```rust
use typst_batch::prelude::*;

let result = Compiler::new(root).with_path(path).compile()?;
let doc = result.document();

// Collect frames from document tree
let frames: Vec<HtmlFrame> = collect_frames(&doc);

// Render frames to SVG (parallel with `batch` feature)
let svgs: Vec<String> = doc.render_frames(&frames);
```

### Diagnostics

```rust
use typst_batch::prelude::*;

let result = Compiler::new(root).with_path(path).compile()?;

// Filter out unwanted diagnostics
let filtered = result.diagnostics().filter_out(&[
    DiagnosticFilter::new(DiagnosticSeverity::Warning, FilterType::HtmlExport),
    DiagnosticFilter::new(DiagnosticSeverity::Warning, FilterType::Package(PackageKind::AllPreview)),
]);

if !filtered.is_empty() {
    eprintln!("{}", filtered);
}
```

## Modules

| Module | Description |
|--------|-------------|
| `process` | Compile and scan APIs (`Compiler`, `Scanner`, `Batcher`) |
| `html` | HTML document types (`HtmlDocument`, `HtmlElement`, `HtmlFrame`) |
| `codegen` | JSON ↔ Typst value conversion |
| `resource` | Shared resources (fonts, packages, file cache) |
| `world` | Typst World implementation |
| `diagnostic` | Error formatting and filtering |

## Typst Access

For advanced use cases:

```rust
use typst_batch::unstable::typst;
use typst_batch::unstable::typst_html;
```

## Requirements

- Typst 0.14.1
- Rust Edition 2024

## License

MIT
