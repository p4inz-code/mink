//! Keyword recognition.
//!
//! Keywords are recognized deterministically from the raw identifier text
//! produced by the lexer. The table is deliberately small and sorted so that
//! lookup is a binary search over a static slice (no allocation, no string
//! interning required at this stage).

use crate::lexer::token::TokenKind;

/// Sorted keyword table: identifier text mapped to its token kind.
///
/// Keep this table sorted for the binary search in [`keyword_kind`].
const KEYWORDS: &[(&str, TokenKind)] = &[
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

/// Returns the token kind for a reserved keyword, or `None` if `word` is an
/// ordinary identifier.
///
/// Matching is exact and case-sensitive, so `fn` is a keyword while `Fn` and
/// `fn_` are identifiers.
pub(super) fn keyword_kind(word: &str) -> Option<TokenKind> {
    KEYWORDS
        .binary_search_by(|(w, _)| (*w).cmp(word))
        .ok()
        .map(|index| KEYWORDS[index].1)
}
