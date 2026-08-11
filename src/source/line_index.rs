use std::ops::Range;

/// A 1-based line and byte column position within a source file.
///
/// Lines are 1-based and counted by `\n`. Columns are 1-based and measured
/// in bytes from the start of the line, so a multi-byte UTF-8 character
/// occupies multiple columns. Character-based columns can be layered onto
/// [`LineIndex`] when the diagnostic system is implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    /// 1-based line number.
    pub line: u32,
    /// 1-based byte column within the line.
    pub column: u32,
}

/// Maps byte offsets in a source file to lines.
///
/// A line is a run of text terminated by `\n` (or the end of the file).
/// `"a\nb"` therefore has two lines and `"a\nb\n"` has three, the last being
/// empty. Line starts are recorded as byte offsets, so lookup is a binary
/// search over an immutable vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIndex {
    /// Byte offset of the start of each line, including the implicit first
    /// line at offset 0. Strictly increasing.
    line_starts: Vec<u32>,
    /// Total length of the indexed text in bytes.
    text_len: u32,
}

impl LineIndex {
    /// Builds a line index for `text`.
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            text.bytes()
                .enumerate()
                .filter_map(|(i, b)| (b == b'\n').then_some(i as u32 + 1)),
        );
        let text_len = text.len() as u32;
        Self {
            line_starts,
            text_len,
        }
    }

    /// Number of lines in the indexed text.
    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    /// 0-based index of the line containing `offset`.
    ///
    /// Offsets beyond the end of the text are clamped to the final line, so
    /// this never panics for a well-formed offset.
    pub fn line_of(&self, offset: u32) -> u32 {
        let offset = offset.min(self.text_len);
        match self.line_starts.binary_search(&offset) {
            Ok(line) => line as u32,
            Err(next) => (next - 1) as u32,
        }
    }

    /// 1-based line and column for `offset`.
    ///
    /// Offsets beyond the end of the text are clamped to the final byte, so
    /// the reported column is consistent with the reported line.
    pub fn line_col(&self, offset: u32) -> LineCol {
        let offset = offset.min(self.text_len);
        let line = self.line_of(offset);
        let column = offset - self.line_starts[line as usize] + 1;
        LineCol {
            line: line + 1,
            column,
        }
    }

    /// Half-open byte range covered by the 0-based `line`.
    ///
    /// The range includes the line's terminating `\n` if present.
    pub fn line_range(&self, line: u32) -> Range<u32> {
        let start = self.line_starts[line as usize];
        let end = self
            .line_starts
            .get(line as usize + 1)
            .copied()
            .unwrap_or(self.text_len);
        start..end
    }
}
