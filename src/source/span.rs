use std::ops::Range;

use super::SourceId;

/// A half-open byte range `[start, end)` into a single source file.
///
/// This is the currency of every compiler stage: the lexer attaches spans to
/// tokens, the parser to AST nodes, and diagnostics to problem locations.
/// Positions are byte offsets into UTF-8 text; an empty span marks a point
/// such as the start or end of a construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    file: SourceId,
    start: u32,
    end: u32,
}

impl Span {
    /// Creates a span over `range` in `file`.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if `range` is inverted (`start > end`).
    pub fn new(file: SourceId, range: Range<u32>) -> Self {
        debug_assert!(
            range.start <= range.end,
            "span start must not exceed its end"
        );
        Self {
            file,
            start: range.start,
            end: range.end,
        }
    }

    /// The file this span refers to.
    pub fn file(self) -> SourceId {
        self.file
    }

    /// Byte offset of the first byte in the span.
    pub fn start(self) -> u32 {
        self.start
    }

    /// Byte offset one past the last byte in the span.
    pub fn end(self) -> u32 {
        self.end
    }

    /// Length of the span in bytes.
    pub fn len(self) -> u32 {
        self.end - self.start
    }

    /// Whether the span covers no bytes (a point location).
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Whether `offset` falls inside the half-open range `[start, end)`.
    pub fn contains(self, offset: u32) -> bool {
        (self.start..self.end).contains(&offset)
    }

    /// The span as a `Range<u32>`.
    pub fn range(self) -> Range<u32> {
        self.start..self.end
    }

    /// Joins two spans into one covering both.
    ///
    /// Returns `None` if the spans belong to different files. The result is
    /// the range from the earlier start to the later end, which also covers
    /// any text between the two spans.
    pub fn join(self, other: Span) -> Option<Span> {
        (self.file == other.file).then(|| Span {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        })
    }
}
