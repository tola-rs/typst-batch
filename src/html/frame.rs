//! HTML frame wrapper.

#[cfg(feature = "svg")]
use super::HtmlDocument;

/// A frame that should be rendered as SVG.
///
/// Frames contain typst-rendered content (math, images, plots, etc.)
/// that needs to be embedded as inline SVG in HTML output.
#[derive(Debug, Clone, Copy)]
pub struct HtmlFrame<'a>(pub(crate) &'a typst_html::HtmlFrame);

impl<'a> HtmlFrame<'a> {
    /// Get the frame's ID, if any.
    #[inline]
    pub fn id(&self) -> Option<&str> {
        self.0.id.as_deref()
    }

    // =========================================================================
    // Measurement API - for inline SVG vertical alignment
    // =========================================================================

    /// Get the frame's size in points (width, height).
    ///
    /// Useful for calculating CSS dimensions or vertical alignment.
    #[inline]
    pub fn size(&self) -> (f64, f64) {
        let s = self.0.inner.size();
        (s.x.to_pt(), s.y.to_pt())
    }

    /// Get the frame's width in points.
    #[inline]
    pub fn width(&self) -> f64 {
        self.0.inner.width().to_pt()
    }

    /// Get the frame's height in points.
    #[inline]
    pub fn height(&self) -> f64 {
        self.0.inner.height().to_pt()
    }

    /// Get the frame's baseline offset from top in points.
    ///
    /// For inline math, you can use this to calculate `vertical-align`:
    /// ```ignore
    /// let shift = frame.height() - frame.baseline();
    /// let css = format!("vertical-align: -{}pt", shift);
    /// ```
    #[inline]
    pub fn baseline(&self) -> f64 {
        self.0.inner.baseline().to_pt()
    }

    /// Get the text size (in points) where the frame was defined.
    ///
    /// Useful for converting pt to em units:
    /// ```ignore
    /// let shift_em = shift_pt / frame.text_size();
    /// ```
    #[inline]
    pub fn text_size(&self) -> f64 {
        self.0.text_size.to_pt()
    }

    /// Calculate the vertical-align offset in em units for inline display.
    ///
    /// Returns a negative value suitable for CSS `vertical-align`.
    /// For baseline-aligned inline content like math formulas.
    #[inline]
    pub fn vertical_align_em(&self) -> f64 {
        let shift = self.height() - self.baseline();
        -shift / self.text_size()
    }

    // =========================================================================
    // Rendering
    // =========================================================================

    /// Render this frame to an SVG string.
    ///
    /// The returned string is a complete `<svg>...</svg>` element
    /// suitable for embedding in HTML.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let svg = frame.to_svg(&doc);
    /// // svg contains: <svg class="typst-frame" style="..." viewBox="...">...</svg>
    /// ```
    ///
    /// # Note
    ///
    /// This method requires the `svg` feature to be enabled.
    #[cfg(feature = "svg")]
    pub fn to_svg(&self, doc: &HtmlDocument) -> String {
        doc.render_frame_svg(self)
    }

    /// Render this frame to an inline SVG string with vertical-align style.
    ///
    /// Convenient wrapper that applies the calculated `vertical-align` offset.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let html = frame.to_inline_svg(&doc);
    /// // Returns: <span style="vertical-align: -0.5em"><svg ...></svg></span>
    /// ```
    #[cfg(feature = "svg")]
    pub fn to_inline_svg(&self, doc: &HtmlDocument) -> String {
        let svg = self.to_svg(doc);
        let align = self.vertical_align_em();
        format!(
            r#"<span style="vertical-align: {:.4}em">{}</span>"#,
            align, svg
        )
    }
}
