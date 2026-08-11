//! The MINK lexer.
//!
//! Converts MINK source text into a deterministic stream of [`Token`]s with
//! accurate source [`Span`]s, collecting [`LexError`]s for malformed input
//! instead of panicking. The lexer skips trivia (whitespace and comments) and
//! recovers from errors so that several problems in one file are reported in
//! a single run.
//!
//! Design decisions are recorded in
//! `docs/implementation/LEXER_IMPLEMENTATION.md`.

use crate::lexer::error::{LexError, LexErrorKind};
use crate::lexer::keywords::keyword_kind;
use crate::lexer::token::{Token, TokenKind};
use crate::source::{SourceFile, Span};

/// The complete result of lexing one source file: every token (including the
/// final [`TokenKind::Eof`]) plus every lexical error encountered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lexed {
    tokens: Vec<Token>,
    errors: Vec<LexError>,
}

impl Lexed {
    /// All tokens, ending with the final `Eof` token.
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    /// Lexical errors in source order. Empty for lexically valid input.
    pub fn errors(&self) -> &[LexError] {
        &self.errors
    }

    /// Whether the source lexed without any errors.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Consumes this result, returning its tokens and errors separately.
    pub fn into_parts(self) -> (Vec<Token>, Vec<LexError>) {
        (self.tokens, self.errors)
    }

    /// Consumes this result, returning only the errors.
    pub fn into_errors(self) -> Vec<LexError> {
        self.errors
    }
}

/// Lexes an entire source file, returning a [`Lexed`] stream.
///
/// This is the one-shot entry point used by the driver and tests. For
/// incremental, pull-based consumption (a future parser), use [`Lexer`]
/// directly.
pub fn lex(file: &SourceFile) -> Lexed {
    let mut lexer = Lexer::new(file);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next_token() {
        tokens.push(token);
    }
    Lexed {
        tokens,
        errors: lexer.errors,
    }
}

/// A pull-based lexer over a single source file.
///
/// [`Lexer::next_token`] returns the next token, or `None` once the final
/// `Eof` token has been emitted. Trivia is skipped internally. Errors are
/// collected in [`Lexer::errors`] while lexing continues, so a caller can
/// drain the whole stream and then inspect the errors.
///
/// The lexer scans the source text in a single forward pass using byte
/// offsets and never allocates per token: `Token` is a `Copy` value.
pub struct Lexer<'a> {
    file: &'a SourceFile,
    pos: u32,
    eof_emitted: bool,
    errors: Vec<LexError>,
}

impl<'a> Lexer<'a> {
    /// Creates a lexer over `file`.
    pub fn new(file: &'a SourceFile) -> Self {
        Self {
            file,
            pos: 0,
            eof_emitted: false,
            errors: Vec::new(),
        }
    }

    /// Lexical errors recorded so far, in source order.
    pub fn errors(&self) -> &[LexError] {
        &self.errors
    }

    /// The source file being lexed.
    pub fn file(&self) -> &SourceFile {
        self.file
    }

    /// Returns the next token, or `None` after the final `Eof` token.
    ///
    /// Never panics: malformed input produces an error in [`Lexer::errors`]
    /// and lexing continues.
    pub fn next_token(&mut self) -> Option<Token> {
        loop {
            self.skip_trivia();
            if self.pos >= self.text_len() {
                return self.eof_token();
            }
            let start = self.pos;
            let kind = match self.peek_char() {
                Some(c) if c.is_ascii_alphabetic() || c == '_' => self.lex_identifier(),
                Some(c) if c.is_ascii_digit() => self.lex_number(),
                Some('"') => self.lex_string(),
                Some('\'') => self.lex_char(),
                Some(c) => match self.lex_operator() {
                    Some(kind) => kind,
                    None => {
                        self.pos += c.len_utf8() as u32;
                        self.record_error(LexErrorKind::UnexpectedCharacter, start, self.pos);
                        continue;
                    }
                },
                None => return self.eof_token(),
            };
            return Some(Token::new(kind, Span::new(self.file.id(), start..self.pos)));
        }
    }

    /// Emits the final `Eof` token exactly once, then `None`.
    fn eof_token(&mut self) -> Option<Token> {
        if self.eof_emitted {
            return None;
        }
        self.eof_emitted = true;
        let len = self.text_len();
        Some(Token::new(
            TokenKind::Eof,
            Span::new(self.file.id(), len..len),
        ))
    }

    /// Skips whitespace, line comments, and block comments.
    ///
    /// A block comment that reaches end of input without `*/` produces an
    /// [`LexErrorKind::UnterminatedBlockComment`] error.
    fn skip_trivia(&mut self) {
        loop {
            match self.peek_char() {
                Some(c) if c.is_whitespace() => {
                    self.pos += c.len_utf8() as u32;
                }
                Some('/') => match self.peek_byte(1) {
                    Some(b'/') => self.skip_line_comment(),
                    Some(b'*') => self.skip_block_comment(),
                    _ => return,
                },
                _ => return,
            }
        }
    }

    /// Consumes a `//` comment up to (but not including) the newline.
    fn skip_line_comment(&mut self) {
        self.pos += 2;
        while let Some(c) = self.peek_char() {
            if c == '\n' {
                return;
            }
            self.pos += c.len_utf8() as u32;
        }
    }

    /// Consumes a `/* ... */` comment. Comments do not nest.
    fn skip_block_comment(&mut self) {
        let start = self.pos;
        self.pos += 2;
        loop {
            match self.peek_char() {
                Some('*') if self.peek_byte(1) == Some(b'/') => {
                    self.pos += 2;
                    return;
                }
                Some(c) => {
                    self.pos += c.len_utf8() as u32;
                }
                None => {
                    self.record_error(LexErrorKind::UnterminatedBlockComment, start, self.pos);
                    return;
                }
            }
        }
    }

    /// Lexes an identifier or keyword: ASCII `[A-Za-z_][A-Za-z0-9_]*`.
    fn lex_identifier(&mut self) -> TokenKind {
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.pos += c.len_utf8() as u32;
            } else {
                break;
            }
        }
        let text = &self.file.text()[start as usize..self.pos as usize];
        keyword_kind(text).unwrap_or(TokenKind::Ident)
    }

    /// Lexes a numeric literal.
    ///
    /// Supports decimal integers, `0x`/`0o`/`0b` prefixed integers, decimal
    /// floats with fraction and exponent, and `_` digit separators. Malformed
    /// numbers record an error and yield a partial `Int`/`Float` token so the
    /// stream stays dense. See `docs/implementation/LEXER_IMPLEMENTATION.md`.
    fn lex_number(&mut self) -> TokenKind {
        let start = self.pos;
        let mut is_float = false;

        // Radix prefix: 0x / 0o / 0b (case-insensitive).
        let mut radix: Option<u32> = None;
        if self.peek_byte(0) == Some(b'0') {
            radix = match self.peek_byte(1) {
                Some(b'x' | b'X') => Some(16),
                Some(b'o' | b'O') => Some(8),
                Some(b'b' | b'B') => Some(2),
                _ => None,
            };
        }
        if let Some(base) = radix {
            self.pos += 2;
            let digits_start = self.pos;
            self.scan_base_digits(base);
            if self.pos == digits_start {
                // A prefix with no digits at all (e.g. `0x`).
                self.record_error(LexErrorKind::MalformedNumber, start, self.pos);
                return self.finish_number(is_float);
            }
            // Fall through so the trailing-character check below applies to
            // radix literals too (e.g. `0b12`, `0x1G`).
        } else {
            self.scan_decimal_digits();
        }

        // Fraction and exponent only exist for decimal literals.
        if radix.is_none() {
            // Fraction: a '.' followed by a digit (a '.' followed by '.' is a
            // range operator, and a trailing '.' alone is a member-access
            // dot).
            if self.peek_byte(0) == Some(b'.')
                && self.peek_byte(1).is_some_and(|b| b.is_ascii_digit())
            {
                self.pos += 1;
                self.scan_decimal_digits();
                is_float = true;
            }

            // Exponent: 'e'/'E' followed by an optional sign and a digit.
            if let Some(b'e' | b'E') = self.peek_byte(0) {
                let sign_offset = usize::from(matches!(self.peek_byte(1), Some(b'+' | b'-')));
                if self
                    .peek_byte(1 + sign_offset as u32)
                    .is_some_and(|b| b.is_ascii_digit())
                {
                    self.pos += 1;
                    if sign_offset == 1 {
                        self.pos += 1;
                    }
                    self.scan_decimal_digits();
                    is_float = true;
                }
            }
        }

        // A number immediately followed by an identifier character is
        // malformed (e.g. `123abc`, `0b12`, `1_`). Consume the offending run
        // so lexing can continue cleanly.
        if self
            .peek_byte(0)
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            while let Some(b) = self.peek_byte(0) {
                if b.is_ascii_alphanumeric() || b == b'_' {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            self.record_error(LexErrorKind::MalformedNumber, start, self.pos);
        }

        self.finish_number(is_float)
    }

    /// Scans a run of decimal digits with `_` separators.
    fn scan_decimal_digits(&mut self) {
        self.scan_base_digits(10);
    }

    /// Scans digits of `base` with single `_` separators between digits.
    fn scan_base_digits(&mut self, base: u32) {
        loop {
            match self.peek_byte(0) {
                Some(b) if is_digit(b, base) => {
                    self.pos += 1;
                }
                Some(b'_') if self.peek_byte(1).is_some_and(|n| is_digit(n, base)) => {
                    self.pos += 2;
                }
                _ => return,
            }
        }
    }

    /// Returns the numeric token kind: float when a fraction or exponent was
    /// present, otherwise integer.
    fn finish_number(&self, is_float: bool) -> TokenKind {
        if is_float {
            TokenKind::Float
        } else {
            TokenKind::Int
        }
    }

    /// Lexes a string literal starting at the opening quote.
    ///
    /// Unterminated strings (end of input or an unescaped newline) record an
    /// error and yield a partial `Str` token covering the consumed text.
    fn lex_string(&mut self) -> TokenKind {
        let start = self.pos;
        self.pos += 1; // opening quote
        loop {
            match self.peek_char() {
                Some('"') => {
                    self.pos += 1;
                    return TokenKind::Str;
                }
                Some('\\') => self.scan_escape(),
                Some('\n') => {
                    self.record_error(LexErrorKind::UnterminatedString, start, self.pos);
                    return TokenKind::Str;
                }
                Some(c) => {
                    self.pos += c.len_utf8() as u32;
                }
                None => {
                    self.record_error(LexErrorKind::UnterminatedString, start, self.pos);
                    return TokenKind::Str;
                }
            }
        }
    }

    /// Lexes a character literal starting at the opening quote.
    ///
    /// Handles empty (`''`), multi-character (`'ab'`), unterminated, and
    /// escape forms, recording an error for each malformed shape.
    fn lex_char(&mut self) -> TokenKind {
        let start = self.pos;
        self.pos += 1; // opening quote
        let mut count = 0u32;

        // First element: one character or one escape sequence.
        match self.peek_char() {
            Some('\'') => {
                self.pos += 1;
                self.record_error(LexErrorKind::InvalidCharLiteral, start, self.pos);
                return TokenKind::Char;
            }
            Some('\\') => {
                self.scan_escape();
                count += 1;
            }
            Some('\n') | None => {
                self.record_error(LexErrorKind::UnterminatedChar, start, self.pos);
                return TokenKind::Char;
            }
            Some(c) => {
                self.pos += c.len_utf8() as u32;
                count += 1;
            }
        }

        // Closing quote, or more characters.
        loop {
            match self.peek_char() {
                Some('\'') => {
                    self.pos += 1;
                    break;
                }
                Some('\\') => {
                    self.scan_escape();
                    count += 1;
                }
                Some('\n') | None => {
                    self.record_error(LexErrorKind::UnterminatedChar, start, self.pos);
                    return TokenKind::Char;
                }
                Some(c) => {
                    self.pos += c.len_utf8() as u32;
                    count += 1;
                }
            }
        }

        if count != 1 {
            self.record_error(LexErrorKind::InvalidCharLiteral, start, self.pos);
        }
        TokenKind::Char
    }

    /// Scans one escape sequence after the backslash.
    ///
    /// Recognized escapes: `\n \r \t \0 \\ \" \'`, `\xHH` (two hex digits),
    /// and `\u{...}` (one to six hex digits, valid Unicode scalar value).
    /// Malformed escapes record an error and are skipped so the enclosing
    /// literal can still be scanned to its end.
    fn scan_escape(&mut self) {
        let escape_start = self.pos;
        self.pos += 1; // backslash
        match self.peek_char() {
            Some('n' | 'r' | 't' | '0' | '\\' | '"' | '\'') => {
                self.pos += 1;
            }
            Some('x') => {
                self.pos += 1;
                let mut digits = 0;
                while digits < 2 {
                    match self.peek_byte(0) {
                        Some(b) if b.is_ascii_hexdigit() => {
                            self.pos += 1;
                            digits += 1;
                        }
                        _ => break,
                    }
                }
                if digits != 2 {
                    self.record_error(LexErrorKind::InvalidEscape, escape_start, self.pos);
                }
            }
            Some('u') => {
                self.pos += 1;
                self.scan_unicode_escape(escape_start);
            }
            Some('\n') => {
                // A backslash immediately before a newline: the string is
                // unterminated; leave the newline for the caller to handle.
                self.record_error(LexErrorKind::InvalidEscape, escape_start, self.pos);
            }
            Some(c) => {
                self.pos += c.len_utf8() as u32;
                self.record_error(LexErrorKind::InvalidEscape, escape_start, self.pos);
            }
            None => {
                self.record_error(LexErrorKind::InvalidEscape, escape_start, self.pos);
            }
        }
    }

    /// Scans `\u{...}`: one to six hex digits naming a valid scalar value.
    fn scan_unicode_escape(&mut self, escape_start: u32) {
        if self.peek_char() != Some('{') {
            self.record_error(LexErrorKind::InvalidUnicodeEscape, escape_start, self.pos);
            return;
        }
        self.pos += 1; // '{'
        let mut value: u32 = 0;
        let mut digits = 0;
        while digits < 6 {
            match self.peek_byte(0) {
                Some(b) if b.is_ascii_hexdigit() => {
                    value = value.wrapping_mul(16) + hex_value(b);
                    self.pos += 1;
                    digits += 1;
                }
                _ => break,
            }
        }
        let valid = self.peek_char() == Some('}')
            && digits >= 1
            && value <= 0x10_FFFF
            && !(0xD800..=0xDFFF).contains(&value);
        if self.peek_char() == Some('}') {
            self.pos += 1;
        }
        if !valid {
            self.record_error(LexErrorKind::InvalidUnicodeEscape, escape_start, self.pos);
        }
    }

    /// Lexes an operator or punctuation token, advancing `self.pos`.
    ///
    /// Returns `None` if the current character starts no known operator.
    fn lex_operator(&mut self) -> Option<TokenKind> {
        let (kind, len) = match self.peek_byte(0)? {
            b'(' => (TokenKind::LParen, 1),
            b')' => (TokenKind::RParen, 1),
            b'{' => (TokenKind::LBrace, 1),
            b'}' => (TokenKind::RBrace, 1),
            b'[' => (TokenKind::LBracket, 1),
            b']' => (TokenKind::RBracket, 1),
            b',' => (TokenKind::Comma, 1),
            b';' => (TokenKind::Semi, 1),
            b':' => match self.peek_byte(1) {
                Some(b':') => (TokenKind::ColonColon, 2),
                _ => (TokenKind::Colon, 1),
            },
            b'.' => match self.peek_byte(1) {
                Some(b'.') => match self.peek_byte(2) {
                    Some(b'=') => (TokenKind::DotDotEq, 3),
                    _ => (TokenKind::DotDot, 2),
                },
                _ => (TokenKind::Dot, 1),
            },
            b'+' => match self.peek_byte(1) {
                Some(b'=') => (TokenKind::PlusEq, 2),
                _ => (TokenKind::Plus, 1),
            },
            b'-' => match self.peek_byte(1) {
                Some(b'=') => (TokenKind::MinusEq, 2),
                Some(b'>') => (TokenKind::Arrow, 2),
                _ => (TokenKind::Minus, 1),
            },
            b'*' => match self.peek_byte(1) {
                Some(b'=') => (TokenKind::StarEq, 2),
                _ => (TokenKind::Star, 1),
            },
            b'/' => match self.peek_byte(1) {
                Some(b'=') => (TokenKind::SlashEq, 2),
                _ => (TokenKind::Slash, 1),
            },
            b'%' => match self.peek_byte(1) {
                Some(b'=') => (TokenKind::PercentEq, 2),
                _ => (TokenKind::Percent, 1),
            },
            b'=' => match self.peek_byte(1) {
                Some(b'=') => (TokenKind::EqEq, 2),
                Some(b'>') => (TokenKind::FatArrow, 2),
                _ => (TokenKind::Eq, 1),
            },
            b'!' => match self.peek_byte(1) {
                Some(b'=') => (TokenKind::NotEq, 2),
                _ => (TokenKind::Bang, 1),
            },
            b'<' => match self.peek_byte(1) {
                Some(b'<') => (TokenKind::Shl, 2),
                Some(b'=') => (TokenKind::Le, 2),
                _ => (TokenKind::Lt, 1),
            },
            b'>' => match self.peek_byte(1) {
                Some(b'>') => (TokenKind::Shr, 2),
                Some(b'=') => (TokenKind::Ge, 2),
                _ => (TokenKind::Gt, 1),
            },
            b'&' => match self.peek_byte(1) {
                Some(b'&') => (TokenKind::AmpAmp, 2),
                _ => (TokenKind::Amp, 1),
            },
            b'|' => match self.peek_byte(1) {
                Some(b'|') => (TokenKind::PipePipe, 2),
                _ => (TokenKind::Pipe, 1),
            },
            b'^' => (TokenKind::Caret, 1),
            b'~' => (TokenKind::Tilde, 1),
            b'?' => (TokenKind::Question, 1),
            _ => return None,
        };
        self.pos += len;
        Some(kind)
    }

    /// Records a lexical error over `start..end`.
    fn record_error(&mut self, kind: LexErrorKind, start: u32, end: u32) {
        self.errors
            .push(LexError::new(kind, Span::new(self.file.id(), start..end)));
    }

    /// Byte length of the source text.
    fn text_len(&self) -> u32 {
        self.file.len()
    }

    /// The byte at `self.pos + offset`, if within bounds.
    fn peek_byte(&self, offset: u32) -> Option<u8> {
        self.file
            .text()
            .as_bytes()
            .get(self.pos as usize + offset as usize)
            .copied()
    }

    /// The character starting at `self.pos`, if any.
    fn peek_char(&self) -> Option<char> {
        self.file.text()[self.pos as usize..].chars().next()
    }
}

/// Whether `b` is a valid digit in `base` (2, 8, 10, or 16).
fn is_digit(b: u8, base: u32) -> bool {
    match b {
        b'0'..=b'9' => u32::from(b - b'0') < base,
        b'a'..=b'f' => base == 16,
        b'A'..=b'F' => base == 16,
        _ => false,
    }
}

/// The numeric value of an ASCII hex digit.
fn hex_value(b: u8) -> u32 {
    match b {
        b'0'..=b'9' => (b - b'0') as u32,
        b'a'..=b'f' => (b - b'a' + 10) as u32,
        b'A'..=b'F' => (b - b'A' + 10) as u32,
        _ => 0,
    }
}
