//! Integration tests for the MINK lexer.
//!
//! Tests verify both token kinds and exact source spans (byte ranges), plus
//! error recovery and safety invariants over malformed inputs.

use mink::lexer::{LexError, LexErrorKind, Token, TokenKind, lex};
use mink::source::{SourceId, SourceMap};

/// Lexes `src` as a virtual `test.mink` file and returns the token stream.
fn lexed(src: &str) -> (Vec<Token>, Vec<LexError>) {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("added file is present");
    let result = lex(file);
    (result.tokens().to_vec(), result.errors().to_vec())
}

/// Lexes `src` and returns only the token kinds.
fn kinds(src: &str) -> Vec<TokenKind> {
    lexed(src).0.into_iter().map(|t| t.kind()).collect()
}

/// Lexes `src` and returns `(kind, byte_range)` pairs for every token.
fn kinds_and_spans(src: &str) -> Vec<(TokenKind, std::ops::Range<u32>)> {
    lexed(src)
        .0
        .into_iter()
        .map(|t| (t.kind(), t.span().range()))
        .collect()
}

/// Lexes `src` and returns the error kinds.
fn error_kinds(src: &str) -> Vec<LexErrorKind> {
    lexed(src).1.into_iter().map(|e| e.kind()).collect()
}

/// Lexes `src` in a fresh map and returns the `SourceId` plus the token and
/// error spans, for span-validity checks.
fn lex_with_id(src: &str) -> (SourceId, Vec<Token>, Vec<LexError>) {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("added file is present");
    let result = lex(file);
    (id, result.tokens().to_vec(), result.errors().to_vec())
}

// ---------------------------------------------------------------------------
// Empty input and EOF
// ---------------------------------------------------------------------------

#[test]
fn empty_source_yields_only_eof() {
    assert_eq!(kinds(""), vec![TokenKind::Eof]);
    assert_eq!(kinds_and_spans(""), vec![(TokenKind::Eof, 0..0)]);
    assert!(lexed("").1.is_empty());
}

#[test]
fn eof_span_is_at_end_of_input() {
    assert_eq!(
        kinds_and_spans("abc"),
        vec![(TokenKind::Ident, 0..3), (TokenKind::Eof, 3..3),]
    );
    assert_eq!(
        kinds_and_spans("ab\n"),
        vec![(TokenKind::Ident, 0..2), (TokenKind::Eof, 3..3),]
    );
}

#[test]
fn whitespace_only_yields_eof() {
    assert_eq!(kinds("  \t\r\n\n "), vec![TokenKind::Eof]);
    assert_eq!(kinds_and_spans("   "), vec![(TokenKind::Eof, 3..3)]);
}

// ---------------------------------------------------------------------------
// Identifiers and keywords
// ---------------------------------------------------------------------------

#[test]
fn simple_identifiers() {
    assert_eq!(
        kinds("foo bar _baz qux_ x1 _"),
        vec![
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn identifiers_with_unicode_are_rejected() {
    // Non-ASCII identifier characters are not yet supported; each is an
    // unexpected character error (documented decision).
    assert_eq!(
        error_kinds("héllo"),
        vec![LexErrorKind::UnexpectedCharacter]
    );
    assert_eq!(
        error_kinds("abc\u{00E9}"),
        vec![LexErrorKind::UnexpectedCharacter]
    );
}

#[test]
fn every_keyword_is_recognized() {
    let cases: &[(&str, TokenKind)] = &[
        ("async", TokenKind::Async),
        ("await", TokenKind::Await),
        ("break", TokenKind::Break),
        ("const", TokenKind::Const),
        ("continue", TokenKind::Continue),
        ("else", TokenKind::Else),
        ("enum", TokenKind::Enum),
        ("false", TokenKind::False),
        ("fn", TokenKind::Fn),
        ("for", TokenKind::For),
        ("if", TokenKind::If),
        ("impl", TokenKind::Impl),
        ("in", TokenKind::In),
        ("let", TokenKind::Let),
        ("loop", TokenKind::Loop),
        ("match", TokenKind::Match),
        ("mod", TokenKind::Mod),
        ("mut", TokenKind::Mut),
        ("null", TokenKind::Null),
        ("pub", TokenKind::Pub),
        ("return", TokenKind::Return),
        ("struct", TokenKind::Struct),
        ("trait", TokenKind::Trait),
        ("true", TokenKind::True),
        ("type", TokenKind::Type),
        ("unsafe", TokenKind::Unsafe),
        ("use", TokenKind::Use),
        ("while", TokenKind::While),
    ];
    for (text, expected) in cases {
        assert_eq!(
            kinds_and_spans(text),
            vec![
                (*expected, 0..text.len() as u32),
                (TokenKind::Eof, text.len() as u32..text.len() as u32)
            ],
            "keyword '{text}' should lex as {expected:?}"
        );
        assert!(
            expected.is_keyword()
                || matches!(
                    expected,
                    TokenKind::True | TokenKind::False | TokenKind::Null
                )
        );
        assert!(expected.is_literal() || expected.is_keyword());
    }
}

#[test]
fn keyword_boundaries_keep_identifiers_as_identifiers() {
    // Keywords only match exactly; prefixes, suffixes, and case variants are
    // ordinary identifiers.
    for text in [
        "fnn",
        "f",
        "ifx",
        "iffy",
        "let_",
        "_let",
        "mut_",
        "_mut",
        "mutt",
        "elseif",
        "structs",
        "Struct",
        "FN",
        "trueish",
        "false_",
        "nullx",
        "in_",
        "awaitable",
    ] {
        assert_eq!(
            kinds(text),
            vec![TokenKind::Ident, TokenKind::Eof],
            "'{text}' must lex as a plain identifier"
        );
    }
}

#[test]
fn identifiers_containing_keyword_text_stay_identifiers() {
    assert_eq!(
        kinds("my_fn not_else true_value"),
        vec![
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Eof,
        ]
    );
}

// ---------------------------------------------------------------------------
// Integers
// ---------------------------------------------------------------------------

#[test]
fn decimal_integers() {
    for src in ["0", "1", "42", "007", "1000000"] {
        let (tokens, errors) = lexed(src);
        assert!(errors.is_empty(), "no errors for '{src}'");
        assert_eq!(tokens[0].kind(), TokenKind::Int);
        assert_eq!(tokens[0].span().range(), 0..src.len() as u32);
        assert_eq!(tokens[1].kind(), TokenKind::Eof);
    }
}

#[test]
fn integer_separators() {
    assert_eq!(kinds("1_000"), vec![TokenKind::Int, TokenKind::Eof]);
    assert_eq!(kinds("1_2_3_4"), vec![TokenKind::Int, TokenKind::Eof]);
    // Separators are accepted between digits only.
    assert_eq!(error_kinds("1__2"), vec![LexErrorKind::MalformedNumber]);
    assert_eq!(error_kinds("1_"), vec![LexErrorKind::MalformedNumber]);
}

#[test]
fn radix_integers() {
    for src in [
        "0x1F",
        "0Xff",
        "0o17",
        "0b1010",
        "0xDEAD_BEEF",
        "0b1_0",
        "0o0",
    ] {
        let (tokens, errors) = lexed(src);
        assert!(errors.is_empty(), "no errors for '{src}'");
        assert_eq!(tokens[0].kind(), TokenKind::Int);
        assert_eq!(tokens[0].span().range(), 0..src.len() as u32);
        assert_eq!(tokens[1].kind(), TokenKind::Eof);
    }
}

#[test]
fn malformed_radix_numbers() {
    // Missing digits after the prefix.
    assert_eq!(error_kinds("0x"), vec![LexErrorKind::MalformedNumber]);
    assert_eq!(error_kinds("0b"), vec![LexErrorKind::MalformedNumber]);
    assert_eq!(error_kinds("0o"), vec![LexErrorKind::MalformedNumber]);
    // Digit not valid for the base.
    assert_eq!(error_kinds("0b12"), vec![LexErrorKind::MalformedNumber]);
    assert_eq!(error_kinds("0o8"), vec![LexErrorKind::MalformedNumber]);
    assert_eq!(error_kinds("0x1G"), vec![LexErrorKind::MalformedNumber]);
}

#[test]
fn number_followed_by_identifier_char_is_malformed() {
    assert_eq!(error_kinds("123abc"), vec![LexErrorKind::MalformedNumber]);
    assert_eq!(error_kinds("42x"), vec![LexErrorKind::MalformedNumber]);
}

// ---------------------------------------------------------------------------
// Floats
// ---------------------------------------------------------------------------

#[test]
fn decimal_floats() {
    for src in [
        "1.5", "0.5", "42.0", "1.5e3", "1E5", "1e-2", "1.5e+3", "0.0",
    ] {
        let (tokens, errors) = lexed(src);
        assert!(errors.is_empty(), "no errors for '{src}'");
        assert_eq!(tokens[0].kind(), TokenKind::Float);
        assert_eq!(tokens[0].span().range(), 0..src.len() as u32);
        assert_eq!(tokens[1].kind(), TokenKind::Eof);
    }
}

#[test]
fn float_followed_by_dot_is_not_consumed() {
    // `1.` lexes as Int followed by a member-access dot (documented).
    assert_eq!(
        kinds_and_spans("1."),
        vec![
            (TokenKind::Int, 0..1),
            (TokenKind::Dot, 1..2),
            (TokenKind::Eof, 2..2),
        ]
    );
}

#[test]
fn dot_followed_by_non_digit_is_range_or_member_access() {
    assert_eq!(
        kinds("1..5"),
        vec![
            TokenKind::Int,
            TokenKind::DotDot,
            TokenKind::Int,
            TokenKind::Eof
        ]
    );
    assert_eq!(
        kinds("1..=5"),
        vec![
            TokenKind::Int,
            TokenKind::DotDotEq,
            TokenKind::Int,
            TokenKind::Eof
        ]
    );
    assert_eq!(
        kinds("a.b"),
        vec![
            TokenKind::Ident,
            TokenKind::Dot,
            TokenKind::Ident,
            TokenKind::Eof
        ]
    );
}

#[test]
fn leading_dot_is_not_a_float() {
    assert_eq!(
        kinds_and_spans(".5"),
        vec![
            (TokenKind::Dot, 0..1),
            (TokenKind::Int, 1..2),
            (TokenKind::Eof, 2..2),
        ]
    );
}

// ---------------------------------------------------------------------------
// Strings
// ---------------------------------------------------------------------------

#[test]
fn string_literals() {
    for (src, span_end) in [("\"\"", 2), ("\"hello\"", 7), ("\"a b c\"", 7)] {
        let (tokens, errors) = lexed(src);
        assert!(errors.is_empty(), "no errors for '{src}'");
        assert_eq!(tokens[0].kind(), TokenKind::Str);
        assert_eq!(tokens[0].span().range(), 0..span_end);
        assert_eq!(tokens[1].kind(), TokenKind::Eof);
    }
}

#[test]
fn string_escapes() {
    for src in [
        r#""\n""#,
        r#""\t\r\0\\\"\'""#,
        r#""\x41""#,
        r#""\x00""#,
        r#""\u{1F600}""#,
        r#""\u{41}""#,
        r#""\u{10FFFF}""#,
    ] {
        let (tokens, errors) = lexed(src);
        assert!(errors.is_empty(), "no errors for '{src}'");
        assert_eq!(tokens[0].kind(), TokenKind::Str);
        assert_eq!(tokens[1].kind(), TokenKind::Eof);
    }
}

#[test]
fn string_with_unicode_content() {
    assert_eq!(
        kinds("\"héllo 世界\""),
        vec![TokenKind::Str, TokenKind::Eof]
    );
    let (tokens, _) = lexed("\"é\"");
    // 'é' is two bytes, so the literal spans 0..4.
    assert_eq!(tokens[0].span().range(), 0..4);
}

#[test]
fn unterminated_string_at_eof() {
    assert_eq!(error_kinds("\"abc"), vec![LexErrorKind::UnterminatedString]);
    assert_eq!(error_kinds("\""), vec![LexErrorKind::UnterminatedString]);
    // The partial token still covers the consumed text.
    let (tokens, _) = lexed("\"abc");
    assert_eq!(tokens[0].kind(), TokenKind::Str);
    assert_eq!(tokens[0].span().range(), 0..4);
}

#[test]
fn newline_in_string_is_an_error() {
    // The first string ends at the newline; the `"` after `def` opens a
    // second, also-unterminated string, so two errors are reported.
    assert_eq!(
        error_kinds("\"abc\ndef\""),
        vec![
            LexErrorKind::UnterminatedString,
            LexErrorKind::UnterminatedString,
        ]
    );
}

#[test]
fn invalid_string_escapes() {
    assert_eq!(error_kinds(r#""\q""#), vec![LexErrorKind::InvalidEscape]);
    assert_eq!(error_kinds(r#""\x4""#), vec![LexErrorKind::InvalidEscape]);
    assert_eq!(error_kinds(r#""\x""#), vec![LexErrorKind::InvalidEscape]);
    assert_eq!(
        error_kinds(r#""\u{}""#),
        vec![LexErrorKind::InvalidUnicodeEscape]
    );
    assert_eq!(
        error_kinds(r#""\u{110000}""#),
        vec![LexErrorKind::InvalidUnicodeEscape]
    );
    assert_eq!(
        error_kinds(r#""\u{D800}""#),
        vec![LexErrorKind::InvalidUnicodeEscape]
    );
    // Six hex digits with a value above the valid scalar range.
    assert_eq!(
        error_kinds(r#""\u{123456}""#),
        vec![LexErrorKind::InvalidUnicodeEscape]
    );
    // A trailing backslash before EOF is an invalid escape and the string is
    // also unterminated.
    assert_eq!(
        error_kinds("\"abc\\"),
        vec![
            LexErrorKind::InvalidEscape,
            LexErrorKind::UnterminatedString
        ]
    );
}

#[test]
fn empty_string_is_valid() {
    assert_eq!(kinds("\"\""), vec![TokenKind::Str, TokenKind::Eof]);
    assert!(lexed("\"\"").1.is_empty());
}

// ---------------------------------------------------------------------------
// Characters
// ---------------------------------------------------------------------------

#[test]
fn character_literals() {
    for src in [
        "'a'",
        "'Z'",
        "'0'",
        "'\\n'",
        "'\\''",
        "'\\\\'",
        "'\\x41'",
        "'\\u{1F600}'",
        "'é'",
    ] {
        let (tokens, errors) = lexed(src);
        assert!(errors.is_empty(), "no errors for '{src}'");
        assert_eq!(tokens[0].kind(), TokenKind::Char);
        assert_eq!(tokens[0].span().range(), 0..src.len() as u32);
        assert_eq!(tokens[1].kind(), TokenKind::Eof);
    }
}

#[test]
fn empty_char_literal_is_an_error() {
    assert_eq!(error_kinds("''"), vec![LexErrorKind::InvalidCharLiteral]);
}

#[test]
fn multi_char_literal_is_an_error() {
    assert_eq!(error_kinds("'ab'"), vec![LexErrorKind::InvalidCharLiteral]);
    assert_eq!(error_kinds("'abc'"), vec![LexErrorKind::InvalidCharLiteral]);
}

#[test]
fn unterminated_char_literal() {
    assert_eq!(error_kinds("'a"), vec![LexErrorKind::UnterminatedChar]);
    assert_eq!(error_kinds("'a\n"), vec![LexErrorKind::UnterminatedChar]);
    assert_eq!(error_kinds("'"), vec![LexErrorKind::UnterminatedChar]);
}

#[test]
fn invalid_char_escape() {
    assert_eq!(error_kinds("'\\q'"), vec![LexErrorKind::InvalidEscape]);
}

// ---------------------------------------------------------------------------
// Operators and punctuation
// ---------------------------------------------------------------------------

#[test]
fn all_operators_lex_to_their_kind() {
    let cases: &[(&str, TokenKind)] = &[
        ("(", TokenKind::LParen),
        (")", TokenKind::RParen),
        ("{", TokenKind::LBrace),
        ("}", TokenKind::RBrace),
        ("[", TokenKind::LBracket),
        ("]", TokenKind::RBracket),
        (",", TokenKind::Comma),
        (";", TokenKind::Semi),
        (":", TokenKind::Colon),
        (".", TokenKind::Dot),
        ("::", TokenKind::ColonColon),
        ("->", TokenKind::Arrow),
        ("=>", TokenKind::FatArrow),
        ("+", TokenKind::Plus),
        ("-", TokenKind::Minus),
        ("*", TokenKind::Star),
        ("/", TokenKind::Slash),
        ("%", TokenKind::Percent),
        ("=", TokenKind::Eq),
        ("+=", TokenKind::PlusEq),
        ("-=", TokenKind::MinusEq),
        ("*=", TokenKind::StarEq),
        ("/=", TokenKind::SlashEq),
        ("%=", TokenKind::PercentEq),
        ("==", TokenKind::EqEq),
        ("!=", TokenKind::NotEq),
        ("<", TokenKind::Lt),
        ("<=", TokenKind::Le),
        (">", TokenKind::Gt),
        (">=", TokenKind::Ge),
        ("&", TokenKind::Amp),
        ("|", TokenKind::Pipe),
        ("^", TokenKind::Caret),
        ("~", TokenKind::Tilde),
        ("<<", TokenKind::Shl),
        (">>", TokenKind::Shr),
        ("&&", TokenKind::AmpAmp),
        ("||", TokenKind::PipePipe),
        ("!", TokenKind::Bang),
        ("?", TokenKind::Question),
        ("..", TokenKind::DotDot),
        ("..=", TokenKind::DotDotEq),
    ];
    for (src, expected) in cases {
        assert_eq!(
            kinds_and_spans(src),
            vec![
                (*expected, 0..src.len() as u32),
                (TokenKind::Eof, src.len() as u32..src.len() as u32)
            ],
            "operator '{src}'"
        );
    }
}

#[test]
fn operators_do_not_overlap() {
    // Two-character operators are lexed greedily; `=` alone is only `Eq`.
    assert_eq!(
        kinds("= == =="),
        vec![
            TokenKind::Eq,
            TokenKind::EqEq,
            TokenKind::EqEq,
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        kinds("< <= <<"),
        vec![TokenKind::Lt, TokenKind::Le, TokenKind::Shl, TokenKind::Eof]
    );
}

// ---------------------------------------------------------------------------
// Comments and trivia
// ---------------------------------------------------------------------------

#[test]
fn line_comments_are_skipped() {
    assert_eq!(kinds("// hello\nx"), vec![TokenKind::Ident, TokenKind::Eof]);
    assert_eq!(
        kinds("x // trailing comment"),
        vec![TokenKind::Ident, TokenKind::Eof]
    );
    assert_eq!(kinds("// only a comment"), vec![TokenKind::Eof]);
    assert_eq!(kinds("// comment at eof"), vec![TokenKind::Eof]);
}

#[test]
fn block_comments_are_skipped() {
    assert_eq!(kinds("/* c */x"), vec![TokenKind::Ident, TokenKind::Eof]);
    assert_eq!(
        kinds("x /* a\nb */ y"),
        vec![TokenKind::Ident, TokenKind::Ident, TokenKind::Eof]
    );
    assert_eq!(kinds("/**/"), vec![TokenKind::Eof]);
    assert_eq!(kinds("/* */ /* */"), vec![TokenKind::Eof]);
}

#[test]
fn unterminated_block_comment_is_an_error() {
    assert_eq!(
        error_kinds("/* never closed"),
        vec![LexErrorKind::UnterminatedBlockComment]
    );
    assert_eq!(kinds("/* never closed"), vec![TokenKind::Eof]);
}

#[test]
fn comment_like_operator_sequences() {
    // `/` followed by `=` is an operator, not a comment.
    assert_eq!(
        kinds("a /= b"),
        vec![
            TokenKind::Ident,
            TokenKind::SlashEq,
            TokenKind::Ident,
            TokenKind::Eof
        ]
    );
    // `/` followed by space is a plain slash.
    assert_eq!(
        kinds("a / b"),
        vec![
            TokenKind::Ident,
            TokenKind::Slash,
            TokenKind::Ident,
            TokenKind::Eof
        ]
    );
    // `/*` inside a line comment is just text.
    assert_eq!(
        kinds("// /* not a block\nx"),
        vec![TokenKind::Ident, TokenKind::Eof]
    );
}

#[test]
fn comments_preserve_positions_of_following_tokens() {
    let (tokens, _) = lexed("ab /* skip */ cd");
    assert_eq!(tokens[0].span().range(), 0..2); // ab
    assert_eq!(tokens[1].span().range(), 14..16); // cd
    assert_eq!(tokens[2].span().range(), 16..16); // eof
}

#[test]
fn crlf_and_tabs_are_whitespace() {
    assert_eq!(
        kinds("a\r\nb\tc"),
        vec![
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Eof,
        ]
    );
}

// ---------------------------------------------------------------------------
// Multiline and exact spans
// ---------------------------------------------------------------------------

#[test]
fn multiline_source_spans_are_accurate() {
    let src = "fn main() {\n    return 0;\n}\n";
    let (tokens, errors) = lexed(src);
    assert!(errors.is_empty());
    let expected: &[(TokenKind, std::ops::Range<u32>)] = &[
        (TokenKind::Fn, 0..2),
        (TokenKind::Ident, 3..7),
        (TokenKind::LParen, 7..8),
        (TokenKind::RParen, 8..9),
        (TokenKind::LBrace, 10..11),
        (TokenKind::Return, 16..22),
        (TokenKind::Int, 23..24),
        (TokenKind::Semi, 24..25),
        (TokenKind::RBrace, 26..27),
        (TokenKind::Eof, 28..28),
    ];
    let actual: Vec<_> = tokens
        .iter()
        .map(|t| (t.kind(), t.span().range()))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn unicode_byte_offsets_are_respected() {
    // Identifiers are ASCII-only: `héllo` lexes as `h` (0..1) and then an
    // unexpected-character error for `é` (two bytes, span 1..3).
    let (tokens, errors) = lexed("héllo");
    assert_eq!(tokens[0].kind(), TokenKind::Ident);
    assert_eq!(tokens[0].span().range(), 0..1);
    assert_eq!(errors[0].span().range(), 1..3);
}

#[test]
fn very_small_inputs() {
    for src in ["a", "1", "'a'", "\"x\"", "+", "//", "/* */"] {
        let (tokens, _) = lexed(src);
        assert_eq!(
            tokens.last().map(Token::kind),
            Some(TokenKind::Eof),
            "for {src:?}"
        );
    }
}

#[test]
fn long_input_lexes_without_error() {
    let digits = "1".repeat(100_000);
    let (tokens, errors) = lexed(&digits);
    assert!(errors.is_empty());
    assert_eq!(tokens[0].kind(), TokenKind::Int);
    assert_eq!(tokens[1].kind(), TokenKind::Eof);

    let long_string = format!("\"{}\"", "a".repeat(100_000));
    let (tokens, errors) = lexed(&long_string);
    assert!(errors.is_empty());
    assert_eq!(tokens[0].kind(), TokenKind::Str);
}

// ---------------------------------------------------------------------------
// Unexpected characters and recovery
// ---------------------------------------------------------------------------

#[test]
fn unexpected_characters_produce_errors_and_continue() {
    assert_eq!(error_kinds("@"), vec![LexErrorKind::UnexpectedCharacter]);
    assert_eq!(error_kinds("#"), vec![LexErrorKind::UnexpectedCharacter]);
    assert_eq!(error_kinds("`"), vec![LexErrorKind::UnexpectedCharacter]);
    assert_eq!(error_kinds("$"), vec![LexErrorKind::UnexpectedCharacter]);
    // Lexing continues after each unexpected character.
    assert_eq!(kinds("@ x"), vec![TokenKind::Ident, TokenKind::Eof]);
}

#[test]
fn multiple_lexical_errors_in_one_run() {
    let src = "let x = \"unterminated\nlet y = 'ab'\n@";
    assert_eq!(
        error_kinds(src),
        vec![
            LexErrorKind::UnterminatedString,
            LexErrorKind::InvalidCharLiteral,
            LexErrorKind::UnexpectedCharacter,
        ]
    );
}

#[test]
fn malformed_input_never_panics_and_spans_stay_in_bounds() {
    // Deterministic pseudo-random byte corpus (valid UTF-8 via lossy decode,
    // mirroring how files are loaded), exercising every code path.
    let mut state = 0x853c49e6748fea9bu64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..2_000 {
        let len = (next() % 64) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| next() as u8).collect();
        let src = String::from_utf8_lossy(&bytes);
        check_invariants(&src);
    }
}

#[test]
fn targeted_malformed_corpus() {
    for src in [
        "\\",
        "'",
        "\"",
        "/*",
        "//",
        "0x",
        "0b",
        "0o",
        "1.",
        "..",
        "..=",
        "->",
        "=>",
        "\"\\",
        "'\\",
        "'a",
        "\"a",
        "\x00",
        "\u{0000}",
        "\u{FFFD}",
        "0x_",
        "1__2",
        "'\u{1F600}'",
        "\"\\u{}\"",
        "\"\\x\"",
        "\"\\u{110000}\"",
        "\"\\u{D800}\"",
        "@@@",
        "$$$",
        "```",
        "!!!",
        "&&&",
        "|||",
        "===",
        ">>>",
        "<<<",
        ":::",
        "...",
        "a\nb\nc",
        "\r\n\r\n",
        "'\\n'x",
        "\"a\"\"b\"",
        "0b12_",
        "0x1G_",
        "\"\\u{1234567}\"",
        "'\\u{}'",
        "@#$",
    ] {
        check_invariants(src);
    }
}

/// Asserts the token-stream invariants for `src`: EOF is last, every span is
/// within bounds and non-inverted, and error spans are within bounds.
fn check_invariants(src: &str) {
    let (_id, tokens, errors) = lex_with_id(src);
    let text_len = src.len() as u32;
    assert_eq!(
        tokens.last().map(Token::kind),
        Some(TokenKind::Eof),
        "EOF must be the last token for {src:?}"
    );
    for token in &tokens {
        let span = token.span();
        assert!(span.start() <= span.end(), "inverted span for {src:?}");
        assert!(span.end() <= text_len, "span out of bounds for {src:?}");
    }
    for error in &errors {
        let span = error.span();
        assert!(
            span.start() <= span.end(),
            "inverted error span for {src:?}"
        );
        assert!(
            span.end() <= text_len,
            "error span out of bounds for {src:?}"
        );
    }
    // Spans never split a UTF-8 character.
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("added file is present");
    for token in &tokens {
        assert!(
            file.span_text(token.span()).is_some(),
            "span splits UTF-8 for {src:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// API shape
// ---------------------------------------------------------------------------

#[test]
fn lexed_exposes_tokens_errors_and_validity() {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", "fn main() {}");
    let file = map.get(id).unwrap();
    let result = lex(file);
    assert!(result.is_valid());
    // fn main ( ) { } + Eof = 7 tokens.
    assert_eq!(result.tokens().len(), 7);
    assert!(result.errors().is_empty());

    let bad_id = map.add("bad.mink", "@");
    let bad = lex(map.get(bad_id).unwrap());
    assert!(!bad.is_valid());
    assert_eq!(bad.errors().len(), 1);
}

#[test]
fn lexer_pull_api_matches_one_shot_api() {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", "fn main() { return 0; }");
    let file = map.get(id).unwrap();
    let one_shot = lex(file);

    let mut lexer = mink::lexer::Lexer::new(file);
    let mut pulled = Vec::new();
    while let Some(token) = lexer.next_token() {
        pulled.push(token);
    }
    assert_eq!(pulled, one_shot.tokens());
    assert_eq!(lexer.errors(), one_shot.errors());
}

#[test]
fn span_is_exposed_on_tokens_and_errors() {
    let (_id, tokens, errors) = lex_with_id("\"abc");
    assert_eq!(tokens[0].span().start(), 0);
    assert_eq!(tokens[0].span().end(), 4);
    assert_eq!(errors[0].span().start(), 0);
    assert_eq!(errors[0].span().end(), 4);
}

#[test]
fn token_kind_classifications() {
    assert!(TokenKind::Fn.is_keyword());
    assert!(!TokenKind::Ident.is_keyword());
    assert!(TokenKind::Int.is_literal());
    assert!(TokenKind::True.is_literal());
    assert!(!TokenKind::Plus.is_literal());
    assert!(!TokenKind::Eof.is_keyword());
}

#[test]
fn lexed_into_parts_splits_tokens_and_errors() {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", "let x = 1;");
    let file = map.get(id).unwrap();
    let (tokens, errors) = lex(file).into_parts();
    assert_eq!(tokens.len(), 6); // let x = 1 ; EOF
    assert!(errors.is_empty());
}

// ---------------------------------------------------------------------------
// CLI-facing behavior of the driver check path is covered in tests/cli.rs
// ---------------------------------------------------------------------------
