//! Token model: [`TokenKind`] and [`Token`].
//!
//! Tokens deliberately carry no source text: the exact text a token covers is
//! recovered from the owning [`SourceFile`](crate::source::SourceFile) through
//! the token's [`Span`]. This keeps tokens small, `Copy`, and free of heap
//! allocations, and it preserves the raw source spelling for diagnostics,
//! formatting, and LSP tooling.

use crate::source::Span;

/// The kind of a lexical token.
///
/// Keyword recognition is deterministic and happens while lexing: an
/// identifier whose text matches a reserved word becomes the keyword's token
/// kind, never [`TokenKind::Ident`]. The keyword set is deliberately small
/// and was frozen with the grammar in session 03 (see
/// `docs/language/CORE_GRAMMAR.md` §10 and
/// `docs/implementation/LEXER_IMPLEMENTATION.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// An identifier: ASCII `[A-Za-z_][A-Za-z0-9_]*`.
    Ident,
    /// An integer literal: decimal, or `0x`/`0o`/`0b` prefixed, with optional
    /// `_` digit separators.
    Int,
    /// A floating-point literal with a fraction and/or an exponent.
    Float,
    /// A string literal, including the surrounding quotes.
    Str,
    /// A character literal, including the surrounding quotes.
    Char,
    /// The boolean literal `true`.
    True,
    /// The boolean literal `false`.
    False,
    /// The null/absence literal `null`.
    Null,

    /// The keyword `fn`.
    Fn,
    /// The keyword `let`.
    Let,
    /// The keyword `mut` (mutable binding modifier).
    Mut,
    /// The keyword `const`.
    Const,
    /// The keyword `type`.
    Type,
    /// The keyword `struct`.
    Struct,
    /// The keyword `enum`.
    Enum,
    /// The keyword `trait`.
    Trait,
    /// The keyword `impl`.
    Impl,
    /// The keyword `mod`.
    Mod,
    /// The keyword `use`.
    Use,
    /// The keyword `pub`.
    Pub,
    /// The keyword `if`.
    If,
    /// The keyword `else`.
    Else,
    /// The keyword `match`.
    Match,
    /// The keyword `loop`.
    Loop,
    /// The keyword `while`.
    While,
    /// The keyword `for`.
    For,
    /// The keyword `in`.
    In,
    /// The keyword `return`.
    Return,
    /// The keyword `break`.
    Break,
    /// The keyword `continue`.
    Continue,
    /// The keyword `async`.
    Async,
    /// The keyword `await`.
    Await,
    /// The keyword `unsafe`.
    Unsafe,

    /// The delimiter `(`.
    LParen,
    /// The delimiter `)`.
    RParen,
    /// The delimiter `{`.
    LBrace,
    /// The delimiter `}`.
    RBrace,
    /// The delimiter `[`.
    LBracket,
    /// The delimiter `]`.
    RBracket,
    /// The punctuation `,`.
    Comma,
    /// The punctuation `;`.
    Semi,
    /// The punctuation `:`.
    Colon,
    /// The punctuation `.` (member access).
    Dot,
    /// The path separator `::`.
    ColonColon,
    /// The return-type arrow `->`.
    Arrow,
    /// The match-arm arrow `=>`.
    FatArrow,

    /// The operator `+`.
    Plus,
    /// The operator `-`.
    Minus,
    /// The operator `*`.
    Star,
    /// The operator `/`.
    Slash,
    /// The operator `%`.
    Percent,
    /// The assignment operator `=`.
    Eq,
    /// The operator `+=`.
    PlusEq,
    /// The operator `-=`.
    MinusEq,
    /// The operator `*=`.
    StarEq,
    /// The operator `/=`.
    SlashEq,
    /// The operator `%=`.
    PercentEq,
    /// The operator `==`.
    EqEq,
    /// The operator `!=`.
    NotEq,
    /// The operator `<`.
    Lt,
    /// The operator `<=`.
    Le,
    /// The operator `>`.
    Gt,
    /// The operator `>=`.
    Ge,
    /// The operator `&`.
    Amp,
    /// The operator `|`.
    Pipe,
    /// The operator `^`.
    Caret,
    /// The operator `~`.
    Tilde,
    /// The operator `<<`.
    Shl,
    /// The operator `>>`.
    Shr,
    /// The operator `&&`.
    AmpAmp,
    /// The operator `||`.
    PipePipe,
    /// The operator `!`.
    Bang,
    /// The optional-handling operator `?`.
    Question,
    /// The range operator `..`.
    DotDot,
    /// The inclusive range operator `..=`.
    DotDotEq,

    /// End of input. Every token stream ends with exactly one `Eof` token
    /// whose span is empty and located at the end of the source text.
    Eof,
}

impl TokenKind {
    /// Whether this kind is a reserved keyword.
    ///
    /// The boolean and null literals are reserved words too but are not
    /// counted as keywords; they are literals (see [`TokenKind::is_literal`]).
    pub fn is_keyword(self) -> bool {
        matches!(
            self,
            Self::Fn
                | Self::Let
                | Self::Mut
                | Self::Const
                | Self::Type
                | Self::Struct
                | Self::Enum
                | Self::Trait
                | Self::Impl
                | Self::Mod
                | Self::Use
                | Self::Pub
                | Self::If
                | Self::Else
                | Self::Match
                | Self::Loop
                | Self::While
                | Self::For
                | Self::In
                | Self::Return
                | Self::Break
                | Self::Continue
                | Self::Async
                | Self::Await
                | Self::Unsafe
        )
    }

    /// Whether this kind is a literal (number, string, char, boolean, or
    /// null).
    pub fn is_literal(self) -> bool {
        matches!(
            self,
            Self::Int
                | Self::Float
                | Self::Str
                | Self::Char
                | Self::True
                | Self::False
                | Self::Null
        )
    }
}

/// A single lexical token: a [`TokenKind`] plus the source [`Span`] it covers.
///
/// `Token` is `Copy` and allocation-free, so token buffers are cheap to build
/// and parsers can hold an arbitrary lookahead window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Token {
    kind: TokenKind,
    span: Span,
}

impl Token {
    /// Creates a token of `kind` covering `span`.
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// The kind of this token.
    pub fn kind(&self) -> TokenKind {
        self.kind
    }

    /// The source span this token covers.
    pub fn span(&self) -> Span {
        self.span
    }
}
