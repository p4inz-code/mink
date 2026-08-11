//! Source file and position infrastructure shared by all compiler stages.
//!
//! The lexer, parser, AST, and diagnostics all operate on byte offsets into
//! UTF-8 source text. This module provides the file registry ([`SourceMap`]),
//! file representation ([`SourceFile`]), file identity ([`SourceId`]),
//! half-open byte spans ([`Span`]), and line/column mapping ([`LineIndex`]).
//!
//! Design notes are recorded in
//! `docs/implementation/ENGINEERING_FOUNDATION.md` §5.

mod file;
mod id;
mod line_index;
mod map;
mod span;

pub use file::SourceFile;
pub use id::SourceId;
pub use line_index::{LineCol, LineIndex};
pub use map::SourceMap;
pub use span::Span;
