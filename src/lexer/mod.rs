//! Lexical analysis: converts MINK source text into a token stream.
//!
//! The lexer turns a [`SourceFile`](crate::source::SourceFile) into a
//! deterministic sequence of [`Token`]s with accurate byte-based
//! [`Span`](crate::source::Span)s, reporting lexical problems as
//! [`LexError`]s instead of panicking. It is the first stage of the compiler
//! pipeline (see `docs/compiler/COMPILER_ARCHITECTURE.md` §2–3).
//!
//! Two usage styles are supported:
//!
//! - one-shot: [`lex`] returns a complete [`Lexed`] stream (used by the
//!   driver and tests), and
//! - pull-based: [`Lexer::new`] + [`Lexer::next_token`] for a future parser
//!   that needs incremental consumption and lookahead.
//!
//! Lexical decisions are documented in
//! `docs/implementation/LEXER_IMPLEMENTATION.md`; the frozen grammar (and the
//! keyword set it fixed) is in `docs/language/CORE_GRAMMAR.md`.

mod error;
mod keywords;
mod scanner;
mod token;

pub use error::{LexError, LexErrorKind};
pub use scanner::{Lexed, Lexer, lex};
pub use token::{Token, TokenKind};
