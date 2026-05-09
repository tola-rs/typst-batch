//! Shared helpers for loading Typst source text.

use std::borrow::Cow;
use std::path::Path;

use typst::diag::FileResult;
use typst::syntax::{FileId, Source};

use crate::resource::file::{decode_utf8, FileResolver};

/// Read a source file without injecting prelude/postlude.
pub(crate) fn read_source(id: FileId, root: &Path, files: &FileResolver) -> FileResult<Source> {
    let bytes = files.read(id, root)?;
    let text = decode_utf8(&bytes)?;
    Ok(build_source(id, text, false, None, None))
}

/// Read a source file and inject prelude/postlude when it is a main file.
pub(crate) fn read_source_with_injection(
    id: FileId,
    root: &Path,
    files: &FileResolver,
    is_main: bool,
    prelude: Option<&str>,
    postlude: Option<&str>,
) -> FileResult<Source> {
    let bytes = files.read(id, root)?;
    let text = decode_utf8(&bytes)?;
    Ok(build_source(id, text, is_main, prelude, postlude))
}

/// Build a Typst source from decoded text.
pub(crate) fn build_source(
    id: FileId,
    text: &str,
    is_main: bool,
    prelude: Option<&str>,
    postlude: Option<&str>,
) -> Source {
    let text = inject_main_source(text, is_main, prelude, postlude).into_owned();

    Source::new(id, text)
}

/// Inject prelude/postlude around a main source file.
pub(crate) fn inject_main_source<'a>(
    text: &'a str,
    is_main: bool,
    prelude: Option<&str>,
    postlude: Option<&str>,
) -> Cow<'a, str> {
    if !is_main || (prelude.is_none() && postlude.is_none()) {
        return Cow::Borrowed(text);
    }

    let mut result = String::new();
    if let Some(prelude) = prelude {
        result.push_str(prelude);
        result.push('\n');
    }
    result.push_str(text);
    if let Some(postlude) = postlude {
        result.push('\n');
        result.push_str(postlude);
    }

    Cow::Owned(result)
}

#[cfg(test)]
mod tests {
    use typst::syntax::{FileId, VirtualPath};

    use super::{build_source, inject_main_source};

    #[test]
    fn keeps_non_main_sources_borrowed() {
        let text = inject_main_source("= Hello", false, Some("#set text(red)"), None);
        assert!(matches!(text, std::borrow::Cow::Borrowed("= Hello")));
    }

    #[test]
    fn injects_prelude_and_postlude_for_main_sources() {
        let text = inject_main_source(
            "= Hello",
            true,
            Some("#import \"a.typ\""),
            Some("#context none"),
        );
        assert_eq!(text, "#import \"a.typ\"\n= Hello\n#context none");
    }

    #[test]
    fn builds_source_with_injection() {
        let id = FileId::new(None, VirtualPath::new("main.typ"));
        let source = build_source(id, "= Hello", true, Some("#import \"a.typ\""), None);
        assert!(source.text().starts_with("#import \"a.typ\"\n= Hello"));
    }
}
