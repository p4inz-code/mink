//! Converting between byte offsets and the line/character positions the language
//! server protocol uses.
//!
//! The protocol counts a `character` in UTF-16 code units, and a line ends at `\n`.
//! MINK source is UTF-8 and the compiler counts bytes, so every position that
//! crosses the protocol boundary goes through here.

/// A position in a document: a zero-based line and a zero-based UTF-16 column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// The zero-based line.
    pub line: u32,
    /// The zero-based column, in UTF-16 code units.
    pub character: u32,
}

/// The start offsets of every line of a text.
#[derive(Debug, Clone)]
pub struct Lines<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    /// Indexes the lines of `text`.
    pub fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(index + 1);
            }
        }
        Self { text, starts }
    }

    /// The number of lines (a final empty line after a trailing newline counts).
    pub fn count(&self) -> usize {
        self.starts.len()
    }

    /// The position of a byte offset. An offset past the end maps to the end; one inside a
    /// character maps to the character's start.
    pub fn position(&self, offset: usize) -> Position {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = match self.starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next) => next - 1,
        };
        let start = self.starts[line];
        let character = self.text[start..offset].encode_utf16().count();
        Position {
            line: line as u32,
            character: character as u32,
        }
    }

    /// The byte offset of a position. A column past the end of its line maps to the line's end
    /// (before the newline), and a line past the end maps to the end of the text.
    pub fn offset(&self, position: Position) -> usize {
        let line = position.line as usize;
        if line >= self.starts.len() {
            return self.text.len();
        }
        let start = self.starts[line];
        let end = self
            .starts
            .get(line + 1)
            .map_or(self.text.len(), |next| next - 1)
            .max(start);
        let line_text = &self.text[start..end];
        let line_text = line_text.strip_suffix('\r').unwrap_or(line_text);
        let mut units = 0u32;
        for (index, c) in line_text.char_indices() {
            if units >= position.character {
                return start + index;
            }
            let next = units + c.len_utf16() as u32;
            if next > position.character {
                // A column inside a surrogate pair lands on the character's start.
                return start + index;
            }
            units = next;
        }
        start + line_text.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_round_trip_on_ascii() {
        let text = "fn main() {\n    let x = 1;\n}\n";
        let lines = Lines::new(text);
        assert_eq!(lines.count(), 4);
        let at = text.find("let").unwrap();
        assert_eq!(lines.position(at), Position { line: 1, character: 4 });
        assert_eq!(lines.offset(Position { line: 1, character: 4 }), at);
        assert_eq!(lines.position(0), Position { line: 0, character: 0 });
        assert_eq!(lines.position(text.len()), Position { line: 3, character: 0 });
    }

    #[test]
    fn columns_count_utf16_units_not_bytes() {
        // "é" is 2 bytes and 1 unit; "😀" is 4 bytes and 2 units.
        let text = "a\u{e9}\u{1f600}b\n";
        let lines = Lines::new(text);
        let b = text.find('b').unwrap();
        assert_eq!(lines.position(b), Position { line: 0, character: 4 });
        assert_eq!(lines.offset(Position { line: 0, character: 4 }), b);
        // A column in the middle of the surrogate pair lands on the character's start.
        assert_eq!(lines.offset(Position { line: 0, character: 3 }), text.find('\u{1f600}').unwrap());
    }

    #[test]
    fn out_of_range_positions_are_clamped() {
        let text = "ab\ncd";
        let lines = Lines::new(text);
        assert_eq!(lines.offset(Position { line: 0, character: 99 }), 2, "end of the first line");
        assert_eq!(lines.offset(Position { line: 9, character: 0 }), text.len());
        assert_eq!(lines.position(999), Position { line: 1, character: 2 });
    }

    #[test]
    fn carriage_returns_do_not_count_as_columns() {
        let text = "ab\r\ncd\r\n";
        let lines = Lines::new(text);
        assert_eq!(lines.offset(Position { line: 0, character: 99 }), 2);
        assert_eq!(lines.position(4), Position { line: 1, character: 0 });
    }

    #[test]
    fn an_offset_inside_a_character_maps_to_its_start() {
        let text = "\u{e9}x";
        let lines = Lines::new(text);
        assert_eq!(lines.position(1), Position { line: 0, character: 0 });
    }
}
