//! Fast scanning API (skips Layout phase).
//!
//! # Example
//!
//! ```ignore
//! use typst_batch::Scanner;
//!
//! // Simple scan
//! let result = Scanner::new(root).scan(path)?;
//! let links = result.extract(LinkExtractor::new());
//!
//! // With sys.inputs
//! let result = Scanner::new(root)
//!     .with_inputs([("draft", true)])
//!     .scan(path)?;
//!
//! // Multiple extractions in one pass
//! let (links, headings) = result.extract((
//!     LinkExtractor::new(),
//!     HeadingExtractor::new(),
//! ));
//! ```

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value as JsonValue;
use typst::comemo::Track;
use typst::diag::SourceDiagnostic;
use typst::engine::{Route, Sink, Traced};
use typst::foundations::{Content, Dict, Label};
use typst::introspection::MetadataElem;
use typst::loading::DataSource;
use typst::model::{Destination, HeadingElem, LinkElem, LinkTarget};
use typst::utils::PicoStr;
use typst::visualize::ImageElem;
use typst::World;
use typst::ROUTINES;
use typst_html::{HtmlAttr, HtmlElem};

use super::inputs::WithInputs;
use super::session::{AccessedDeps, CompileSession};
use crate::diagnostic::{has_errors, CompileError};
use crate::resource::file::FileResolver;
use crate::resource::package::PackageId;
use crate::world::TypstWorld;

/// Builder for fast Typst scanning (Eval-only, skips Layout).
///
/// Significantly faster than [`Compiler`](super::compile::Compiler) because it skips:
/// - Layout calculation
/// - Frame generation
/// - HTML document creation
pub struct Scanner<'a> {
    root: &'a Path,
    files: Arc<FileResolver>,
    inputs: Option<Dict>,
}

impl<'a> WithInputs for Scanner<'a> {
    fn inputs_mut(&mut self) -> &mut Option<Dict> {
        &mut self.inputs
    }
}

impl<'a> Scanner<'a> {
    /// Create a new scanner with the given root directory.
    pub fn new(root: &'a Path) -> Self {
        Self {
            root,
            files: Arc::new(FileResolver::new()),
            inputs: None,
        }
    }

    /// Use an explicit file resolver for this scan.
    pub fn with_files(mut self, files: FileResolver) -> Self {
        self.files = Arc::new(files);
        self
    }

    /// Execute the scan on a single file.
    pub fn scan<P: AsRef<Path>>(self, path: P) -> Result<ScanResult, CompileError> {
        let path = path.as_ref();
        let world = self.build_world(path);
        scan_impl(&world)
    }

    fn build_world(&self, path: &Path) -> TypstWorld {
        match &self.inputs {
            Some(inputs) => TypstWorld::builder(path, self.root)
                .with_files(Arc::clone(&self.files))
                .with_local_cache()
                .no_fonts()
                .with_inputs_dict(inputs.clone())
                .build(),
            None => TypstWorld::builder(path, self.root)
                .with_files(Arc::clone(&self.files))
                .with_local_cache()
                .no_fonts()
                .build(),
        }
    }
}

/// Result of fast scanning (Eval-only, no Layout).
#[derive(Debug)]
pub struct ScanResult {
    /// The document's Content tree for extraction.
    content: Content,
    /// Files and packages accessed during scanning.
    accessed: AccessedDeps,
    /// Scan diagnostics (warnings only).
    diagnostics: Vec<SourceDiagnostic>,
}

impl ScanResult {
    /// Extract data using an extractor.
    ///
    /// For advanced use cases or custom extractors.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Multiple extractors (tuple)
    /// let (links, headings) = result.extract((
    ///     LinkExtractor::new(),
    ///     HeadingExtractor::new(),
    /// ));
    /// ```
    #[inline]
    pub fn extract<E: Extractor>(&self, extractor: E) -> E::Output {
        extract(&self.content, extractor)
    }

    /// Extract all links from the document.
    #[inline]
    pub fn links(&self) -> Vec<Link> {
        self.extract(LinkExtractor::new())
    }

    /// Extract all headings from the document.
    #[inline]
    pub fn headings(&self) -> Vec<Heading> {
        self.extract(HeadingExtractor::new())
    }

    /// Extract metadata by label.
    #[inline]
    pub fn metadata(&self, label: &str) -> Option<JsonValue> {
        MetadataExtractor::new(label).and_then(|e| self.extract(e))
    }

    /// Get the raw content tree.
    pub fn content(&self) -> &Content {
        &self.content
    }

    /// Get files and packages accessed during scanning.
    pub fn accessed(&self) -> &AccessedDeps {
        &self.accessed
    }

    /// Get files accessed during scanning.
    pub fn accessed_files(&self) -> &[PathBuf] {
        &self.accessed.files
    }

    /// Get packages accessed during scanning.
    ///
    /// Useful for detecting virtual package usage (e.g., `@myapp/data`).
    pub fn accessed_packages(&self) -> &[PackageId] {
        &self.accessed.packages
    }

    /// Get scan diagnostics.
    pub fn diagnostics(&self) -> &[SourceDiagnostic] {
        &self.diagnostics
    }
}

/// Trait for extracting data from Typst Content.
pub trait Extractor: Sized {
    /// The type returned after extraction.
    type Output;

    /// Visit a content element during traversal.
    ///
    /// Return `ControlFlow::Break(())` to stop traversal early.
    fn visit(&mut self, elem: &Content) -> ControlFlow<()>;

    /// Finalize and return the extracted data.
    fn finish(self) -> Self::Output;
}

/// Extract data from Content using an extractor.
pub fn extract<E: Extractor>(content: &Content, mut extractor: E) -> E::Output {
    let _ = content.traverse(&mut |elem: Content| extractor.visit(&elem));
    extractor.finish()
}

macro_rules! impl_extractor_for_tuple {
    ($first:ident $(, $rest:ident)*) => {
        impl<$first: Extractor $(, $rest: Extractor)*> Extractor for ($first, $($rest,)*) {
            type Output = ($first::Output, $($rest::Output,)*);

            #[allow(non_snake_case)]
            fn visit(&mut self, elem: &Content) -> ControlFlow<()> {
                let ($first, $($rest,)*) = self;
                $first.visit(elem)?;
                $($rest.visit(elem)?;)*
                ControlFlow::Continue(())
            }

            #[allow(non_snake_case)]
            fn finish(self) -> Self::Output {
                let ($first, $($rest,)*) = self;
                ($first.finish(), $($rest.finish(),)*)
            }
        }

        impl_extractor_for_tuple!($($rest),*);
    };
    () => {};
}

impl_extractor_for_tuple!(A, B, C, D, E, F, G, H);

/// Extracts all links from the document.
#[derive(Debug, Default)]
pub struct LinkExtractor {
    links: Vec<Link>,
    href_attr: Option<HtmlAttr>,
    src_attr: Option<HtmlAttr>,
}

impl LinkExtractor {
    /// Create a new link extractor.
    pub fn new() -> Self {
        Self {
            links: Vec::new(),
            href_attr: HtmlAttr::intern("href").ok(),
            src_attr: HtmlAttr::intern("src").ok(),
        }
    }
}

impl Extractor for LinkExtractor {
    type Output = Vec<Link>;

    fn visit(&mut self, elem: &Content) -> ControlFlow<()> {
        if let Some(link) = elem.to_packed::<LinkElem>()
            && let LinkTarget::Dest(Destination::Url(url)) = &link.dest
        {
            self.links.push(Link {
                dest: url.as_str().to_string(),
                source: LinkSource::Link,
            });
        }

        if let Some(html_elem) = elem.to_packed::<HtmlElem>() {
            let attrs = html_elem.attrs.get_cloned(Default::default());

            if let Some(href) = self.href_attr
                && let Some(value) = attrs.get(href)
            {
                self.links.push(Link {
                    dest: value.to_string(),
                    source: LinkSource::Href,
                });
            }

            if let Some(src) = self.src_attr
                && let Some(value) = attrs.get(src)
            {
                self.links.push(Link {
                    dest: value.to_string(),
                    source: LinkSource::Src,
                });
            }
        }

        // Extract image source paths
        if let Some(image) = elem.to_packed::<ImageElem>()
            && let DataSource::Path(path) = &image.source.source
        {
            self.links.push(Link {
                dest: path.to_string(),
                source: LinkSource::Image,
            });
        }

        ControlFlow::Continue(())
    }

    fn finish(self) -> Self::Output {
        self.links
    }
}

/// A link extracted from the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The link destination.
    pub dest: String,
    /// Where this link came from.
    pub source: LinkSource,
}

/// The source of a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkSource {
    /// From `#link()` element.
    Link,
    /// From `href` attribute.
    Href,
    /// From `src` attribute.
    Src,
    /// From `#image()` element source path.
    Image,
}

impl Link {
    /// Check if HTTP/HTTPS link.
    #[inline]
    pub fn is_http(&self) -> bool {
        self.dest.starts_with("http://") || self.dest.starts_with("https://")
    }

    /// Check if external link.
    #[inline]
    pub fn is_external(&self) -> bool {
        self.dest.contains("://")
            || self.dest.starts_with("mailto:")
            || self.dest.starts_with("tel:")
    }

    /// Check if site-root link (starts with `/`).
    #[inline]
    pub fn is_site_root(&self) -> bool {
        self.dest.starts_with('/') && !self.dest.starts_with("//")
    }

    /// Check if fragment link (starts with `#`).
    #[inline]
    pub fn is_fragment(&self) -> bool {
        self.dest.starts_with('#')
    }

    /// Check if relative link.
    #[inline]
    pub fn is_relative(&self) -> bool {
        !self.is_external() && !self.is_site_root() && !self.is_fragment()
    }
}

/// Extracts all headings from the document.
#[derive(Debug, Default)]
pub struct HeadingExtractor {
    headings: Vec<Heading>,
}

impl HeadingExtractor {
    /// Create a new heading extractor.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Extractor for HeadingExtractor {
    type Output = Vec<Heading>;

    fn visit(&mut self, elem: &Content) -> ControlFlow<()> {
        if let Some(heading) = elem.to_packed::<HeadingElem>() {
            let level = heading.resolve_level(Default::default()).get() as u8;
            let text = heading.body.plain_text().to_string();
            // Extract supplement if it's custom Content (not Auto or Func)
            let supplement = heading
                .supplement
                .get_cloned(Default::default())
                .custom()
                .flatten()
                .and_then(|s| match s {
                    typst::model::Supplement::Content(c) => Some(c.plain_text().to_string()),
                    typst::model::Supplement::Func(_) => None,
                })
                .filter(|s| s != "Section");
            self.headings.push(Heading { level, text, supplement });
        }
        ControlFlow::Continue(())
    }

    fn finish(self) -> Self::Output {
        self.headings
    }
}

/// A heading extracted from the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// Heading level (1-6).
    pub level: u8,
    /// Heading text content (plain text from body).
    pub text: String,
    /// Heading supplement (e.g., "Section", "Chapter").
    /// Used for custom heading IDs when not default.
    pub supplement: Option<String>,
}

/// Extracts metadata by label.
#[derive(Debug)]
pub struct MetadataExtractor {
    label: Label,
    value: Option<JsonValue>,
}

impl MetadataExtractor {
    /// Create a new metadata extractor for the given label.
    pub fn new(label: &str) -> Option<Self> {
        Some(Self {
            label: Label::new(PicoStr::intern(label))?,
            value: None,
        })
    }
}

impl Extractor for MetadataExtractor {
    type Output = Option<JsonValue>;

    fn visit(&mut self, elem: &Content) -> ControlFlow<()> {
        if self.value.is_some() {
            return ControlFlow::Break(());
        }

        if let Some(meta) = elem.to_packed::<MetadataElem>()
            && meta.label() == Some(self.label)
        {
            self.value = serde_json::to_value(&meta.value).ok();
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }

    fn finish(self) -> Self::Output {
        self.value
    }
}

/// Internal scan implementation, exposed for BatchCompiler reuse.
pub(crate) fn scan_impl(world: &TypstWorld) -> Result<ScanResult, CompileError> {
    let session = CompileSession::start();
    let line_offset = world.prelude_line_count();

    let traced = Traced::default();
    let mut sink = Sink::new();

    let source = world
        .source(world.main())
        .map_err(|e| CompileError::html_export(format!("Failed to read source: {e:?}")))?;

    let world_ref: &dyn World = world;
    let result = typst_eval::eval(
        &ROUTINES,
        world_ref.track(),
        traced.track(),
        sink.track_mut(),
        Route::default().track(),
        &source,
    );

    let warnings = sink.warnings();

    let module = result.map_err(|errors| {
        let all_diags: Vec<_> = errors.iter().chain(&warnings).cloned().collect();
        CompileError::compilation_with_offset(world, all_diags, line_offset)
    })?;

    if has_errors(&warnings) {
        return Err(CompileError::compilation_with_offset(world, warnings.to_vec(), line_offset));
    }

    let accessed = session.finish(world.root(), world.files());

    Ok(ScanResult {
        content: module.content(),
        accessed,
        diagnostics: warnings.to_vec(),
    })
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::file::{FileResolver, VirtualFileSystem};
    use crate::resource::package::PackageId;
    use std::path::Path;
    use std::fs;
    use tempfile::TempDir;

    struct ProbeVfs(&'static str);

    impl VirtualFileSystem for ProbeVfs {
        fn read(&self, path: &Path) -> Option<Vec<u8>> {
            (path == Path::new("/probe.txt")).then(|| self.0.as_bytes().to_vec())
        }

        fn read_package(&self, _pkg: &PackageId, _path: &str) -> Option<Vec<u8>> {
            None
        }
    }

    #[test]
    fn test_scanner_basic() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.typ");
        fs::write(&file, "= Hello\nWorld").unwrap();

        let result = Scanner::new(dir.path()).scan(&file);
        assert!(result.is_ok());
        assert!(!result.unwrap().content().is_empty());
    }

    #[test]
    fn test_scanner_with_inputs() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.typ");
        fs::write(
            &file,
            r#"#let x = sys.inputs.at("key", default: "none")
= #x"#,
        )
        .unwrap();

        let result = Scanner::new(dir.path())
            .with_inputs([("key", "value")])
            .scan(&file);
        assert!(result.is_ok());
    }

    #[test]
    fn scanner_uses_its_own_file_resolver() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.typ");
        fs::write(&file, r#"#metadata(read("/probe.txt")) <probe>"#).unwrap();

        let first_files = FileResolver::new().with_virtual_fs(ProbeVfs("first"));
        let second_files = FileResolver::new().with_virtual_fs(ProbeVfs("second"));

        let first = Scanner::new(dir.path())
            .with_files(first_files)
            .scan(&file)
            .unwrap();
        let second = Scanner::new(dir.path())
            .with_files(second_files)
            .scan(&file)
            .unwrap();

        assert_eq!(first.metadata("probe"), Some("first".into()));
        assert_eq!(second.metadata("probe"), Some("second".into()));
    }

    #[test]
    fn test_extract_links() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.typ");
        fs::write(
            &file,
            r#"
#link("https://example.com")[External]
#link("/local")[Local]
"#,
        )
        .unwrap();

        let result = Scanner::new(dir.path()).scan(&file).unwrap();
        let links = result.extract(LinkExtractor::new());

        assert_eq!(links.len(), 2);
        assert!(links.iter().any(|l| l.is_http()));
        assert!(links.iter().any(|l| l.is_site_root()));
    }

    #[test]
    fn test_extract_image_links() {
        let dir = TempDir::new().unwrap();

        // Create a dummy image file
        let img_path = dir.path().join("test.png");
        fs::write(&img_path, &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]).unwrap();

        let file = dir.path().join("test.typ");
        fs::write(
            &file,
            r#"#image("test.png")"#,
        )
        .unwrap();

        let result = Scanner::new(dir.path()).scan(&file).unwrap();
        let links = result.extract(LinkExtractor::new());

        eprintln!("links: {:?}", links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].source, LinkSource::Image);
        assert!(links[0].dest.contains("test.png"));
    }

    #[test]
    fn test_extract_headings() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.typ");
        fs::write(
            &file,
            r#"
= Level 1
== Level 2
=== Level 3
"#,
        )
        .unwrap();

        let result = Scanner::new(dir.path()).scan(&file).unwrap();
        let headings = result.extract(HeadingExtractor::new());

        assert_eq!(headings.len(), 3);
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[1].level, 2);
        assert_eq!(headings[2].level, 3);
    }

    #[test]
    fn test_extract_metadata() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.typ");
        fs::write(
            &file,
            r#"#metadata((title: "Test")) <meta>"#,
        )
        .unwrap();

        let result = Scanner::new(dir.path()).scan(&file).unwrap();
        let meta = result.extract(MetadataExtractor::new("meta").unwrap());

        assert!(meta.is_some());
    }

    #[test]
    fn test_extract_tuple() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.typ");
        fs::write(
            &file,
            r#"
= Heading
#link("https://example.com")[Link]
"#,
        )
        .unwrap();

        let result = Scanner::new(dir.path()).scan(&file).unwrap();
        let (links, headings) = result.extract((
            LinkExtractor::new(),
            HeadingExtractor::new(),
        ));

        assert_eq!(links.len(), 1);
        assert_eq!(headings.len(), 1);
    }

    #[test]
    fn test_link_classification() {
        let http = Link { dest: "http://x.com".into(), source: LinkSource::Link };
        let https = Link { dest: "https://x.com".into(), source: LinkSource::Link };
        let mailto = Link { dest: "mailto:a@b.com".into(), source: LinkSource::Href };
        let root = Link { dest: "/about".into(), source: LinkSource::Link };
        let fragment = Link { dest: "#section".into(), source: LinkSource::Link };
        let relative = Link { dest: "./img.png".into(), source: LinkSource::Src };

        assert!(http.is_http() && http.is_external());
        assert!(https.is_http() && https.is_external());
        assert!(!mailto.is_http() && mailto.is_external());
        assert!(root.is_site_root() && !root.is_external());
        assert!(fragment.is_fragment());
        assert!(relative.is_relative());
    }
}
