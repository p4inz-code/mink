//! Tests for the source infrastructure: file identity, file representation,
//! line/column mapping, and spans.

use mink::source::{LineCol, LineIndex, SourceId, SourceMap, Span};

#[test]
fn source_map_assigns_unique_ids_and_retrieves_files() {
    let mut map = SourceMap::new();
    let a = map.add("a.mink", "first");
    let b = map.add("b.mink", "second");

    assert_ne!(a, b);
    assert_eq!(map.len(), 2);
    assert!(!map.is_empty());
    assert_eq!(map.get(a).unwrap().text(), "first");
    assert_eq!(map.get(b).unwrap().text(), "second");
    assert_eq!(map.get(b).unwrap().name(), std::path::Path::new("b.mink"));
    assert!(map.get(SourceId::new(99)).is_none());
}

#[test]
fn source_file_remembers_name_and_text() {
    let mut map = SourceMap::new();
    let id = map.add("hello.mink", "fn main() {}");
    let file = map.get(id).unwrap();

    assert_eq!(file.id(), id);
    assert_eq!(file.name(), std::path::Path::new("hello.mink"));
    assert_eq!(file.text(), "fn main() {}");
    assert_eq!(file.len(), 12);
    assert!(!file.is_empty());
    assert_eq!(file.line_count(), 1);
}

#[test]
fn line_columns_are_one_based() {
    let mut map = SourceMap::new();
    let id = map.add("t.mink", "fn main() {\n    return 0;\n}");
    let file = map.get(id).unwrap();

    assert_eq!(file.line_count(), 3);
    // Offset 0 is the first byte of line 1.
    assert_eq!(file.line_col(0), LineCol { line: 1, column: 1 });
    // '\n' after "fn main() {" is the 12th byte (0-based 11) of line 1.
    assert_eq!(
        file.line_col(11),
        LineCol {
            line: 1,
            column: 12
        }
    );
    // First byte of line 2.
    assert_eq!(file.line_col(12), LineCol { line: 2, column: 1 });
    // '}' at the start of line 3 (text length is 27 bytes).
    assert_eq!(file.line_col(26), LineCol { line: 3, column: 1 });
    // One past the last byte still maps to the final line.
    assert_eq!(file.line_col(27), LineCol { line: 3, column: 2 });
    // Far out-of-range offsets clamp to the final line consistently.
    assert_eq!(file.line_col(100), LineCol { line: 3, column: 2 });
}

#[test]
fn trailing_newline_starts_a_final_empty_line() {
    let mut map = SourceMap::new();
    let id = map.add("t.mink", "a\nb\n");
    let file = map.get(id).unwrap();

    assert_eq!(file.line_count(), 3);
    assert_eq!(file.line_col(0), LineCol { line: 1, column: 1 });
    assert_eq!(file.line_col(2), LineCol { line: 2, column: 1 });
    // Offset 4 is one past the final '\n'.
    assert_eq!(file.line_col(4), LineCol { line: 3, column: 1 });
}

#[test]
fn columns_count_bytes_for_unicode() {
    let mut map = SourceMap::new();
    // 'é' is two bytes in UTF-8.
    let id = map.add("t.mink", "héllo\n");
    let file = map.get(id).unwrap();

    // Offset 1 is the first byte of 'é'.
    assert_eq!(file.line_col(1), LineCol { line: 1, column: 2 });
    // Offset 6 is the '\n' (byte-based column 7).
    assert_eq!(file.line_col(6), LineCol { line: 1, column: 7 });
    assert_eq!(file.line_col(7), LineCol { line: 2, column: 1 });
}

#[test]
fn line_ranges_cover_each_line_including_trailing_newline() {
    // Two lines, trailing newline on the first only.
    let index = LineIndex::new("ab\ncd");
    assert_eq!(index.line_count(), 2);
    // Line ranges are 0-based and include the terminating '\n'.
    assert_eq!(index.line_range(0), 0..3);
    assert_eq!(index.line_range(1), 3..5);

    let trailing = LineIndex::new("ab\n");
    assert_eq!(trailing.line_count(), 2);
    // The final line after a trailing '\n' is empty.
    assert_eq!(trailing.line_range(1), 3..3);
}

#[test]
fn spans_cover_byte_ranges() {
    let mut map = SourceMap::new();
    let id = map.add("t.mink", "abcdef");

    let whole = Span::new(id, 0..6);
    assert_eq!(whole.start(), 0);
    assert_eq!(whole.end(), 6);
    assert_eq!(whole.len(), 6);
    assert!(!whole.is_empty());
    assert!(whole.contains(0));
    assert!(whole.contains(5));
    assert!(!whole.contains(6));
    assert_eq!(whole.range(), 0..6);

    let point = Span::new(id, 3..3);
    assert!(point.is_empty());
    assert_eq!(point.len(), 0);

    let joined = whole.join(point).unwrap();
    assert_eq!(joined.range(), 0..6);
}

#[test]
fn spans_from_different_files_do_not_join() {
    let mut map = SourceMap::new();
    let a = map.add("a.mink", "aaa");
    let b = map.add("b.mink", "bbb");

    assert!(Span::new(a, 0..1).join(Span::new(b, 0..1)).is_none());
    // Identical spans still join to themselves.
    let span = Span::new(a, 1..2);
    assert_eq!(span.join(span).unwrap().range(), 1..2);
}

#[test]
fn span_text_extracts_source_slices() {
    let mut map = SourceMap::new();
    let id = map.add("t.mink", "fn main() {}");

    let file = map.get(id).unwrap();
    let keyword = Span::new(id, 0..2);
    assert_eq!(file.span_text(keyword), Some("fn"));
    assert_eq!(file.span_text(Span::new(id, 3..7)), Some("main"));
    assert_eq!(file.span_text(Span::new(id, 12..12)), Some(""));
}

#[test]
fn span_text_rejects_spans_from_other_files() {
    let mut map = SourceMap::new();
    let a = map.add("a.mink", "aaa");
    let b = map.add("b.mink", "bbb");

    let file_b = map.get(b).unwrap();
    assert_eq!(file_b.span_text(Span::new(a, 0..1)), None);
}

#[test]
fn span_text_rejects_out_of_bounds_and_char_splitting_spans() {
    let mut map = SourceMap::new();
    // 'é' is U+00E9, encoded as two UTF-8 bytes (0xC3 0xA9) each, so the
    // text occupies bytes 0..2 and 2..4.
    let id = map.add("t.mink", "éé");
    let file = map.get(id).unwrap();

    // Off the end of the text.
    assert_eq!(file.span_text(Span::new(id, 4..6)), None);
    // Splits the first 'é' (byte 1 is a continuation byte).
    assert_eq!(file.span_text(Span::new(id, 0..1)), None);
    // Splits the second 'é'.
    assert_eq!(file.span_text(Span::new(id, 2..3)), None);
    // Whole characters are fine.
    assert_eq!(file.span_text(Span::new(id, 0..2)), Some("é"));
    assert_eq!(file.span_text(Span::new(id, 2..4)), Some("é"));
}

#[test]
fn source_map_loads_a_file_from_disk() {
    let path =
        std::env::temp_dir().join(format!("mink_source_test_{}_load.mink", std::process::id()));
    std::fs::write(&path, "fn main() {}\n").unwrap();

    let mut map = SourceMap::new();
    let id = map.load(&path).unwrap();
    let file = map.get(id).unwrap();

    assert_eq!(file.name(), path.as_path());
    assert_eq!(file.text(), "fn main() {}\n");
    assert_eq!(file.line_count(), 2);

    let _ = std::fs::remove_file(&path);
}
