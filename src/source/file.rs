use std::path::{Path, PathBuf};

use super::{LineCol, LineIndex, SourceId, Span};

/// A single source file registered in a [`SourceMap`](crate::source::SourceMap).
///
/// Owns the source text, a display name (typically the path it was loaded
/// from), and a precomputed [`LineIndex`] so that byte offsets can be turned
/// into line/column positions cheaply.
#[derive(Debug, Clone)]
pub struct SourceFile {
    id: SourceId,
    name: PathBuf,
    text: String,
    line_index: LineIndex,
}

impl SourceFile {
    /// Creates a source file with the given identity, display name, and text.
    ///
    /// The line index is computed eagerly; constructing a file is `O(n)` in
    /// the text length.
    pub fn new(id: SourceId, name: impl Into<PathBuf>, text: impl Into<String>) -> Self {
        let text = text.into();
        let line_index = LineIndex::new(&text);
        Self {
            id,
            name: name.into(),
            text,
            line_index,
        }
    }

    /// The identity of this file within its `SourceMap`.
    pub fn id(&self) -> SourceId {
        self.id
    }

    /// The display name of this file, usually the path it was loaded from.
    pub fn name(&self) -> &Path {
        &self.name
    }

    /// The full source text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Length of the source text in bytes.
    pub fn len(&self) -> u32 {
        self.text.len() as u32
    }

    /// Whether the source text is empty.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Number of lines in the source text.
    pub fn line_count(&self) -> u32 {
        self.line_index.line_count()
    }

    /// 1-based line and column for a byte offset.
    pub fn line_col(&self, offset: u32) -> LineCol {
        self.line_index.line_col(offset)
    }

    /// Returns the source text covered by `span`.
    ///
    /// Returns `None` if the span belongs to a different file or its range
    /// falls outside the text or splits a multi-byte UTF-8 character.
    pub fn span_text(&self, span: Span) -> Option<&str> {
        if span.file() != self.id {
            return None;
        }
        let start = span.start() as usize;
        let end = span.end() as usize;
        let text = &self.text;
        // The inverted-range guard (start > end) protects against malformed
        // spans in release builds, where `Span::new`'s debug assertion does
        // not run; slicing would otherwise panic.
        if start > end
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            return None;
        }
        Some(&text[start..end])
    }
}
