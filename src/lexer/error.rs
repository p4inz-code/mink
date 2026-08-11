//! Lexical error model.
//!
//! Lexical errors are produced by the lexer when source text is malformed.
//! Each error carries a category ([`LexErrorKind`]) and the precise source
//! [`Span`](crate::source::Span) it applies to. The lexer records errors and
//! keeps going so that several problems in one file can be reported in a
//! single run; it never panics on malformed input.
//!
//! This is the lexer's lightweight, self-contained error model. When the full
//! structured diagnostic engine lands (see
//! `docs/implementation/LEXER_IMPLEMENTATION.md`), these kinds will feed into
//! it rather than being rendered ad hoc.

use std::fmt;

use crate::source::Span;

/// The category of a lexical error.
///
/// Every category has a stable machine-readable code ([`LexErrorKind::code`])
/// and a human-readable message ([`fmt::Display`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LexErrorKind {
    /// A character that starts no token and is not part of trivia.
    UnexpectedCharacter,
    /// A string literal that ran into a newline or end of input before its
    /// closing quote.
    UnterminatedString,
    /// A character literal that ran into a newline or end of input before its
    /// closing quote.
    UnterminatedChar,
    /// A block comment that reached end of input before its closing `*/`.
    UnterminatedBlockComment,
    /// An escape sequence that is not a recognized MINK escape.
    InvalidEscape,
    /// A `\u{...}` escape that is malformed or encodes an invalid scalar value.
    InvalidUnicodeEscape,
    /// A character literal containing zero or more than one character.
    InvalidCharLiteral,
    /// A numeric literal with an invalid shape (bad base digits, missing
    /// digits, or malformed separators).
    MalformedNumber,
}

impl LexErrorKind {
    /// Stable machine-readable code for this error category.
    ///
    /// The codes are provisional until the full diagnostic engine defines the
    /// final error-code namespace.
    pub fn code(self) -> &'static str {
        match self {
            Self::UnexpectedCharacter => "E-L01",
            Self::UnterminatedString => "E-L02",
            Self::UnterminatedChar => "E-L03",
            Self::UnterminatedBlockComment => "E-L04",
            Self::InvalidEscape => "E-L05",
            Self::InvalidUnicodeEscape => "E-L06",
            Self::InvalidCharLiteral => "E-L07",
            Self::MalformedNumber => "E-L08",
        }
    }
}

impl fmt::Display for LexErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UnexpectedCharacter => "unexpected character",
            Self::UnterminatedString => "unterminated string literal",
            Self::UnterminatedChar => "unterminated character literal",
            Self::UnterminatedBlockComment => "unterminated block comment",
            Self::InvalidEscape => "invalid escape sequence",
            Self::InvalidUnicodeEscape => "invalid unicode escape sequence",
            Self::InvalidCharLiteral => "character literal must contain exactly one character",
            Self::MalformedNumber => "malformed numeric literal",
        };
        f.write_str(message)
    }
}

/// A single lexical error: a category plus the source span it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LexError {
    kind: LexErrorKind,
    span: Span,
}

impl LexError {
    /// Creates a lexical error of `kind` over `span`.
    pub fn new(kind: LexErrorKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// The category of this error.
    pub fn kind(&self) -> LexErrorKind {
        self.kind
    }

    /// The source span this error applies to.
    pub fn span(&self) -> Span {
        self.span
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}
