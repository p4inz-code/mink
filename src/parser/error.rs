//! Parser (syntax) error model.
//!
//! Syntax errors are produced by the parser when source text does not match
//! the frozen grammar (`docs/language/CORE_GRAMMAR.md`). Each error carries a
//! category ([`ParseErrorKind`]) and the precise source
//! [`Span`](crate::source::Span) it applies to. The parser records errors and
//! recovers (see `docs/implementation/PARSER_IMPLEMENTATION.md`) so that
//! several independent problems in one file can be reported in a single run;
//! it never panics on malformed input.
//!
//! This mirrors the lexer's lightweight error model
//! ([`LexError`](crate::lexer::LexError)). Both feed into the driver's
//! `check` report; the full structured diagnostic engine (severity,
//! explanations, related spans, machine-readable output) remains a later
//! milestone per `docs/language/ERROR_SYSTEM.md`.

use std::fmt;

use crate::source::Span;

/// The category of a syntax error.
///
/// Every category has a stable machine-readable code ([`ParseErrorKind::code`])
/// and a human-readable message ([`fmt::Display`]). Codes are provisional
/// until the full diagnostic engine defines the final error-code namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParseErrorKind {
    /// A token that cannot begin any top-level declaration.
    ExpectedItem,
    /// A name was required but another token was found.
    ExpectedIdentifier,
    /// An expression was required but another token (or end of input) was
    /// found.
    ExpectedExpression,
    /// The left side of an assignment is not a valid assignment target
    /// (a variable, member, or index).
    ExpectedAssignmentTarget,
    /// `=` was required in a `let` or `const` declaration.
    ExpectedEqual,
    /// `;` was required to terminate a statement.
    ExpectedSemicolon,
    /// `,` was required between parameters or arguments.
    ExpectedComma,
    /// `(` was required (e.g. after a function name).
    ExpectedLParen,
    /// `)` was required (e.g. to close a parameter or argument list).
    ExpectedRParen,
    /// `]` was required to close an index expression.
    ExpectedRBracket,
    /// A `{ ... }` block was required (function body, `if`/`else`/loop body).
    ExpectedBlock,
    /// `in` was required in a `for` loop header.
    ExpectedIn,
    /// End of input was reached while a `(` was still open.
    UnclosedParen,
    /// End of input was reached while a `{` was still open.
    UnclosedBrace,
    /// End of input was reached while a `[` was still open.
    UnclosedBracket,
    /// End of input where more source was required.
    UnexpectedEof,
    /// `}` was required to close a struct declaration or literal.
    ExpectedRBrace,
    /// `:` was required between a field name and its type or initializer.
    ExpectedColon,
    /// An integer literal was required (the length of an array type).
    ExpectedIntegerLiteral,
    /// A type was required (a struct field's declared type).
    ExpectedType,
    /// `>` was required to close `Ptr<T>` (or a non-`Ptr` name was used
    /// with the generic form, which only `Ptr` supports).
    ExpectedGT,
    /// A variant name was required after `::` in an enum variant
    /// reference (`Name::`).
    ExpectedVariant,
}

impl ParseErrorKind {
    /// Stable machine-readable code for this error category.
    ///
    /// Codes are provisional until the full diagnostic engine defines the
    /// final error-code namespace.
    pub fn code(self) -> &'static str {
        match self {
            Self::ExpectedItem => "E-P01",
            Self::ExpectedIdentifier => "E-P02",
            Self::ExpectedExpression => "E-P03",
            Self::ExpectedAssignmentTarget => "E-P04",
            Self::ExpectedEqual => "E-P05",
            Self::ExpectedSemicolon => "E-P06",
            Self::ExpectedComma => "E-P07",
            Self::ExpectedLParen => "E-P08",
            Self::ExpectedRParen => "E-P09",
            Self::ExpectedRBracket => "E-P10",
            Self::ExpectedBlock => "E-P11",
            Self::ExpectedIn => "E-P12",
            Self::UnclosedParen => "E-P13",
            Self::UnclosedBrace => "E-P14",
            Self::UnclosedBracket => "E-P15",
            Self::UnexpectedEof => "E-P16",
            Self::ExpectedRBrace => "E-P17",
            Self::ExpectedColon => "E-P18",
            Self::ExpectedIntegerLiteral => "E-P19",
            Self::ExpectedType => "E-P20",
            Self::ExpectedGT => "E-P21",
            Self::ExpectedVariant => "E-P22",
        }
    }
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ExpectedItem => "expected a top-level declaration (`fn`, `let`, or `const`)",
            Self::ExpectedIdentifier => "expected an identifier",
            Self::ExpectedExpression => "expected an expression",
            Self::ExpectedAssignmentTarget => {
                "the target of an assignment must be a variable, member, or index"
            }
            Self::ExpectedEqual => "expected '=' in a declaration",
            Self::ExpectedSemicolon => "expected ';'",
            Self::ExpectedComma => "expected ','",
            Self::ExpectedLParen => "expected '('",
            Self::ExpectedRParen => "expected ')'",
            Self::ExpectedRBracket => "expected ']'",
            Self::ExpectedBlock => "expected a block `{ ... }`",
            Self::ExpectedIn => "expected 'in' in a for loop",
            Self::UnclosedParen => "unclosed '(' — expected ')' before end of input",
            Self::UnclosedBrace => "unclosed '{' — expected '}' before end of input",
            Self::UnclosedBracket => "unclosed '[' — expected ']' before end of input",
            Self::UnexpectedEof => "unexpected end of input",
            Self::ExpectedRBrace => "expected '}'",
            Self::ExpectedColon => "expected ':'",
            Self::ExpectedIntegerLiteral => "expected an integer literal",
            Self::ExpectedType => "expected a type",
            Self::ExpectedGT => "expected '>'",
            Self::ExpectedVariant => "expected a variant name after '::'",
        };
        f.write_str(message)
    }
}

/// A single syntax error: a category plus the source span it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParseError {
    kind: ParseErrorKind,
    span: Span,
}

impl ParseError {
    /// Creates a syntax error of `kind` over `span`.
    pub fn new(kind: ParseErrorKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// The category of this error.
    pub fn kind(&self) -> ParseErrorKind {
        self.kind
    }

    /// The source span this error applies to.
    pub fn span(&self) -> Span {
        self.span
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}
