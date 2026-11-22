//! `mink fmt`: one canonical layout for MINK source.
//!
//! The formatter has one rule above all the others: **it never changes what the
//! program says.** It reads the token stream, decides only the white space between
//! tokens (indentation, spacing, blank lines, where a line breaks), and writes the
//! same tokens and the same comments back. It never adds, removes or reorders a
//! token, a comment or a character inside a string. Before it gives any result it
//! lexes its own output and compares it, token by token and comment by comment,
//! with the input; if anything differs it refuses and the file is left alone.
//!
//! The layout is fixed and has no options:
//!
//! * four spaces per level, no tabs, no trailing white space, one newline at the end;
//! * `{` stays on the line of its header, `} else {` stays together;
//! * spaces around binary operators, after commas and colons, none inside
//!   parentheses and brackets, and none around `.`, `::`, `?` and ranges;
//! * the author's line breaks are kept, so the formatter never reflows an expression;
//!   continuation lines are indented one level, and lines inside an unclosed
//!   parenthesis or bracket hang one level from the line that opened it;
//! * at most one blank line anywhere, none after `{` or before `}`, and one between
//!   top-level items;
//! * a comment keeps its text and stays on its line; a comment after code is one space away.
//!
//! Where the layout cannot be told apart from the tokens alone (`<` and `>` as
//! comparison or as generics, `|` as bit-or or as a closure bar) the formatter keeps
//! the spacing the author wrote.

use std::fmt;

use crate::lexer::{lex, TokenKind};
use crate::source::{SourceFile, SourceId};

mod command;
mod diff;
mod files;

pub use command::{main, parse_args, FmtArgs, USAGE};
pub use diff::unified_diff;
pub use files::{collect_files, format_file, write_atomically, FileOutcome};

/// Why a file could not be formatted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    /// The source has a lexical error, so its tokens cannot be told apart reliably.
    Lex {
        /// The 1-based line of the first error.
        line: u32,
        /// What is wrong.
        message: String,
    },
    /// The formatter's own output did not match the input. This is a bug in the formatter,
    /// and it is reported instead of writing a file that might mean something else.
    Mismatch(String),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::Lex { line, message } => {
                write!(f, "line {line}: {message}; fix it first, then format")
            }
            FormatError::Mismatch(what) => write!(
                f,
                "the formatter could not keep the program unchanged ({what}); the file was not touched. \
                 This is a bug in mink fmt, please report it with the file"
            ),
        }
    }
}

impl std::error::Error for FormatError {}

const INDENT: &str = "    ";

/// One token with its text and position.
#[derive(Debug, Clone, Copy)]
struct Tok<'a> {
    kind: TokenKind,
    text: &'a str,
    start: usize,
    end: usize,
}

/// One comment found between tokens.
#[derive(Debug, Clone)]
struct Comment {
    /// The comment text, without trailing white space.
    text: String,
    /// Newlines between the previous token or comment and this one; 0 means the same line.
    lines_before: usize,
    /// Whether it is a `/* */` comment.
    block: bool,
}

/// The white space and comments before one token.
#[derive(Debug, Clone, Default)]
struct Gap {
    comments: Vec<Comment>,
    /// Newlines between the last comment (or the previous token) and the next token.
    lines_after: usize,
    /// Whether the gap held a space or tab, outside any comment.
    had_space: bool,
}

fn lex_tokens(src: &str) -> Result<Vec<Tok<'_>>, FormatError> {
    let file = SourceFile::new(SourceId::new(0), "fmt", src.to_string());
    let lexed = lex(&file);
    if let Some(error) = lexed.errors().first() {
        let line = file.line_col(error.span().start()).line;
        return Err(FormatError::Lex {
            line,
            message: error.kind().to_string(),
        });
    }
    Ok(lexed
        .tokens()
        .iter()
        .map(|token| Tok {
            kind: token.kind(),
            text: &src[token.span().start() as usize..token.span().end() as usize],
            start: token.span().start() as usize,
            end: token.span().end() as usize,
        })
        .collect())
}

/// Splits the text between two tokens into comments and line counts.
fn read_gap(text: &str) -> Gap {
    let mut gap = Gap::default();
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut newlines = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            b'\n' => {
                newlines += 1;
                at += 1;
            }
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                let end = text[at..].find('\n').map(|n| at + n).unwrap_or(text.len());
                gap.comments.push(Comment {
                    text: text[at..end].trim_end().to_string(),
                    lines_before: newlines,
                    block: false,
                });
                newlines = 0;
                at = end;
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                let end = text[at + 2..].find("*/").map(|n| at + 2 + n + 2).unwrap_or(text.len());
                // Only the ends of lines are cleaned, so no spaces are left before a newline.
                let cleaned = text[at..end].split('\n').map(str::trim_end).collect::<Vec<_>>().join("\n");
                gap.comments.push(Comment {
                    text: cleaned,
                    lines_before: newlines,
                    block: true,
                });
                newlines = 0;
                at = end;
            }
            b' ' | b'\t' => {
                gap.had_space = true;
                at += 1;
            }
            _ => at += 1,
        }
    }
    gap.lines_after = newlines;
    gap
}

/// Everything the layout needs to know about the tokens around a gap.
fn is_word(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        kind,
        Ident | Int | Float | Str | Char | True | False | Null | Fn | Let | Mut | Const | Type | Struct
            | Enum | Trait | Impl | Mod | Use | Pub | If | Else | Match | Loop | While | For | In | Return
            | Break | Continue | Defer | Move | Async | Await | Unsafe
    )
}

fn is_keyword(kind: TokenKind) -> bool {
    kind.is_keyword() || matches!(kind, TokenKind::Defer | TokenKind::Move)
}

/// Whether a token ends an operand, so a following `-`, `*` or `&` is binary.
fn ends_operand(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        kind,
        Ident | Int | Float | Str | Char | True | False | Null | RParen | RBracket | Question | RBrace
    )
}

fn is_binary_operator(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        kind,
        Plus | Minus | Star | Slash | Percent | Eq | PlusEq | MinusEq | StarEq | SlashEq | PercentEq | EqEq
            | NotEq | Le | Ge | Amp | Caret | Shl | Shr | AmpAmp | PipePipe | Arrow | FatArrow
    )
}

/// Whether a token can only be a binary operator at the start of a continued line.
fn starts_continuation(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        kind,
        Dot | Question | Plus | Slash | Percent | EqEq | NotEq | Le | Ge | Caret | Shl | Shr | AmpAmp
            | PipePipe | DotDot | DotDotEq | StarEq | PlusEq | MinusEq | SlashEq | PercentEq
    )
}

/// Whether a token ends a statement or a block, so the next line starts something new.
fn ends_statement(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Semi | TokenKind::LBrace | TokenKind::RBrace)
}

/// Whether the previous token leaves its statement unfinished, so the next line continues it.
fn leaves_open(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        kind,
        Plus | Minus | Star | Slash | Percent | Eq | PlusEq | MinusEq | StarEq | SlashEq | PercentEq | EqEq
            | NotEq | Lt | Le | Ge | Amp | Caret | Shl | Shr | AmpAmp | PipePipe | Arrow | FatArrow | Dot
            | ColonColon | Colon | DotDot | DotDotEq | Pipe
    )
}

/// Whether no space may be put between two tokens' texts because they would lex differently.
fn would_merge(a: &str, b: &str) -> bool {
    let joined = format!("{a}{b}");
    let file = SourceFile::new(SourceId::new(0), "fmt", joined);
    let lexed = lex(&file);
    let tokens = lexed.tokens();
    if !lexed.errors().is_empty() || tokens.len() != 3 {
        return true;
    }
    let first = tokens[0].span();
    first.end() as usize != a.len()
}

/// Whether to put a space between `prev` and `next` on one line. `None` keeps the author's choice.
fn space_between(prev: &Tok<'_>, next: &Tok<'_>, before_prev: Option<TokenKind>) -> Option<bool> {
    use TokenKind::*;
    let (p, n) = (prev.kind, next.kind);

    // Nothing is put before these.
    if matches!(n, Comma | Semi | RParen | RBracket | Dot | ColonColon | Question | Colon | DotDot | DotDotEq) {
        return Some(false);
    }
    // Nothing is put after these.
    if matches!(p, LParen | LBracket | Dot | ColonColon | Bang | Tilde | DotDot | DotDotEq) {
        return Some(false);
    }
    // After a comma, colon or semicolon: one space.
    if matches!(p, Comma | Semi | Colon) {
        return Some(true);
    }
    // Braces.
    if p == LBrace {
        return Some(n != RBrace);
    }
    if n == RBrace {
        return Some(p != LBrace);
    }
    if n == LBrace {
        return Some(!matches!(p, LParen | LBracket));
    }
    if p == RBrace {
        return Some(!matches!(n, RParen | RBracket | Comma | Semi | Dot | Question));
    }
    // Calls and indexing: no space between an operand and its opening bracket.
    if n == LParen {
        return match p {
            Ident | RParen | RBracket | Question => Some(false),
            Gt => None,
            // `fn(x)` and `move (x)` read either way: keep it.
            Fn | Move => None,
            _ if is_keyword(p) => Some(true),
            _ if is_binary_operator(p) || matches!(p, Pipe | Lt) => Some(true),
            _ => None,
        };
    }
    if n == LBracket {
        return match p {
            Ident | RParen | RBracket | Question => Some(false),
            _ if is_keyword(p) => Some(true),
            _ if is_binary_operator(p) => Some(true),
            _ => None,
        };
    }
    // Spacing that is only known from the author's text.
    if matches!(p, Lt | Gt | Pipe) || matches!(n, Lt | Gt | Pipe) {
        return None;
    }
    // Unary operators bind to what follows, binary ones get a space on both sides.
    if matches!(p, Minus | Star | Amp | Plus) && !before_prev.is_some_and(ends_operand) {
        // `-x`, `*p`, `&x`: unary, nothing after. `&mut x` keeps `mut` attached to the `&`.
        return Some(false);
    }
    if matches!(n, Bang | Tilde) {
        return Some(true);
    }
    if is_binary_operator(p) || is_binary_operator(n) {
        return Some(true);
    }
    if is_word(p) && is_word(n) {
        return Some(true);
    }
    if p == Question && is_word(n) {
        return Some(true);
    }
    // `)` or `]` then a word, `>` then a word, and anything else unclear: keep the author's space.
    None
}

/// An open bracket or brace and the indent of the line it was opened on.
#[derive(Debug, Clone, Copy)]
struct Opener {
    kind: TokenKind,
    line_indent: usize,
    /// The index of the opening token, to look up whether its block holds statements.
    index: usize,
}

struct Writer {
    out: String,
    stack: Vec<Opener>,
    /// The indent of the line being written, without the extra level of a continuation line.
    base_indent: usize,
    /// Whether anything but the indent has been written on the current line.
    line_started: bool,
}

impl Writer {
    fn indent_for_next_token(&self, kind: TokenKind, continuation: bool) -> usize {
        match self.stack.last() {
            Some(top) => {
                let closes = matches!(kind, TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket);
                if closes {
                    top.line_indent
                } else {
                    top.line_indent + 1 + usize::from(continuation && top.kind == TokenKind::LBrace)
                }
            }
            None => usize::from(continuation),
        }
    }

    fn start_line(&mut self, indent: usize, base: usize) {
        self.out.push_str(&INDENT.repeat(indent));
        self.base_indent = base;
        self.line_started = true;
    }

    fn newline(&mut self) {
        // No trailing white space.
        while self.out.ends_with(' ') {
            self.out.pop();
        }
        self.out.push('\n');
        self.line_started = false;
    }

    fn blank_line(&mut self) {
        // At most one blank line: only add one when the previous line is not already blank.
        if !self.out.ends_with("\n\n") && !self.out.is_empty() {
            self.out.push('\n');
        }
    }
}

fn starts_item(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(kind, Fn | Struct | Enum | Impl | Trait | Pub | Async | Unsafe | Type | Mod | Use | Const | Let)
}

/// Blocks written on one line with a single short statement keep their line: guard clauses such as
/// `if n < 0 { return 0; }` stay compact. Anything bigger is written one statement per line.
const COMPACT_LINE_LIMIT: usize = 100;

/// For each `{`, whether the block is written over several lines: it holds more than one statement,
/// a single statement that does not fit on its line, a comment, or a block that is itself
/// multi-line. A block of fields, or of one expression with no `;`, stays as the author wrote it.
fn statement_blocks(src: &str, tokens: &[Tok<'_>], gaps: &[Gap]) -> (Vec<bool>, Vec<bool>) {
    struct Frame {
        index: usize,
        kind: TokenKind,
        semis: usize,
        nested_brace: bool,
        multi: bool,
    }
    let mut flags = vec![false; tokens.len()];
    // For each `}`: whether the block it closes spans more than one line once formatted.
    let mut closes_multiline = vec![false; tokens.len()];
    let mut open: Vec<Frame> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => open.push(Frame {
                index,
                kind: token.kind,
                semis: 0,
                nested_brace: false,
                multi: false,
            }),
            TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => {
                let wanted = match token.kind {
                    TokenKind::RBrace => TokenKind::LBrace,
                    TokenKind::RParen => TokenKind::LParen,
                    _ => TokenKind::LBracket,
                };
                let Some(position) = open.iter().rposition(|f| f.kind == wanted) else {
                    continue;
                };
                let frame = open.remove(position);
                open.truncate(position);
                if token.kind != TokenKind::RBrace {
                    continue;
                }
                let opener = &tokens[frame.index];
                let one_line = !src[opener.start..token.end].contains('\n');
                let has_comment = gaps[frame.index + 1..=index].iter().any(|g| !g.comments.is_empty());
                let line_start = src[..opener.start].rfind('\n').map_or(0, |n| n + 1);
                let line_end = src[token.end..].find('\n').map_or(src.len(), |n| token.end + n);
                let fits = src[line_start..line_end].chars().count() <= COMPACT_LINE_LIMIT;
                let compact = frame.semis == 1
                    && !frame.nested_brace
                    && !has_comment
                    && one_line
                    && fits
                    && tokens[index - 1].kind == TokenKind::Semi;
                let multi = frame.multi || (frame.semis > 0 && !compact);
                flags[frame.index] = multi;
                closes_multiline[index] = multi || !one_line;
                // A block followed by another statement means its parent is a sequence of statements.
                let followed_by_statement = tokens
                    .get(index + 1)
                    .is_some_and(|t| is_word(t.kind) && !matches!(t.kind, TokenKind::Else | TokenKind::In));
                if let Some(parent) = open.last_mut() {
                    parent.nested_brace = true;
                    // A block that holds a multi-line block, or several statements, is itself multi-line.
                    if parent.kind == TokenKind::LBrace && (multi || followed_by_statement) {
                        parent.multi = true;
                    }
                }
            }
            TokenKind::Semi => {
                if let Some(frame) = open.last_mut() {
                    frame.semis += 1;
                }
            }
            _ => {}
        }
    }
    (flags, closes_multiline)
}

/// Lays out `src` (with `\n` line endings) as canonical source text.
fn layout(src: &str, tokens: &[Tok<'_>]) -> String {
    let mut gaps = Vec::with_capacity(tokens.len());
    let mut previous_end = 0usize;
    for token in tokens {
        gaps.push(read_gap(&src[previous_end..token.start]));
        previous_end = token.end;
    }
    let (statement_block, closes_multiline) = statement_blocks(src, tokens, &gaps);

    let mut w = Writer {
        out: String::with_capacity(src.len() + src.len() / 8),
        stack: Vec::new(),
        base_indent: 0,
        line_started: false,
    };

    for (index, token) in tokens.iter().enumerate() {
        let gap = &gaps[index];
        let prev = index.checked_sub(1).map(|i| &tokens[i]);
        let before_prev = index.checked_sub(2).map(|i| tokens[i].kind);
        let top_level = w.stack.is_empty();

        // The comments in front of this token.
        let mut wrote_comment = false;
        let mut after_line_comment = false;
        for comment in &gap.comments {
            let trailing = comment.lines_before == 0 && w.line_started && prev.is_some();
            if trailing {
                // A comment after code sits one space away from it.
                w.out.push(' ');
                w.out.push_str(&comment.text);
            } else {
                if w.line_started {
                    w.newline();
                }
                let after_open_brace = !wrote_comment && prev.is_some_and(|p| p.kind == TokenKind::LBrace);
                let separates_items = !wrote_comment
                    && top_level
                    && prev.is_some_and(|p| p.kind == TokenKind::RBrace)
                    && starts_item(token.kind);
                if (comment.lines_before >= 2 || separates_items) && !after_open_brace {
                    w.blank_line();
                }
                let indent = if prev.is_none() {
                    0
                } else {
                    w.indent_for_next_token(TokenKind::Ident, false)
                };
                w.start_line(indent, indent);
                w.out.push_str(&comment.text);
            }
            wrote_comment = true;
            after_line_comment = !comment.block;
            if !comment.block {
                // A `//` comment runs to the end of its line.
                w.newline();
            }
        }

        if token.kind == TokenKind::Eof {
            break;
        }

        let mut breaks = gap.lines_after;
        if after_line_comment {
            breaks = breaks.max(1);
        }

        let Some(prev) = prev else {
            // The first token of the file starts at the left edge, after any header comments.
            if w.line_started {
                w.newline();
            }
            if wrote_comment && breaks >= 2 {
                w.blank_line();
            }
            w.start_line(0, 0);
            w.out.push_str(token.text);
            push_open(&mut w, token, index);
            continue;
        };

        // One statement per line: a block that holds statements opens a new line after its `{`,
        // and a statement ends its line.
        let after_statement_end = prev.kind == TokenKind::Semi
            && w.stack.last().is_some_and(|o| o.kind == TokenKind::LBrace && statement_block[o.index]);
        let after_statement_open = prev.kind == TokenKind::LBrace && statement_block[index - 1];
        let after_block_end = prev.kind == TokenKind::RBrace
            && w.stack.last().is_none_or(|o| o.kind == TokenKind::LBrace)
            && is_word(token.kind)
            && !matches!(token.kind, TokenKind::Else | TokenKind::In);
        // The `}` of a block that holds statements starts its own line.
        let closes_statement_block = token.kind == TokenKind::RBrace
            && prev.kind != TokenKind::LBrace
            && w.stack.last().is_some_and(|o| o.kind == TokenKind::LBrace && statement_block[o.index]);
        if breaks == 0 && (after_statement_end || after_statement_open || after_block_end || closes_statement_block) {
            breaks = 1;
        }

        // A brace stays with its header, and `else` stays with the `}` before it.
        if breaks > 0 && !wrote_comment {
            let joins_brace = token.kind == TokenKind::LBrace
                && !ends_statement(prev.kind)
                && prev.kind != TokenKind::Comma;
            // `} else` only after a block written over several lines; a compact `{ ... }` keeps its line.
            let joins_else =
                token.kind == TokenKind::Else && prev.kind == TokenKind::RBrace && closes_multiline[index - 1];
            if joins_brace || joins_else {
                breaks = 0;
            }
        }

        if breaks > 0 {
            if w.line_started {
                w.newline();
            }
            // Keep one blank line, but never right after `{` or right before `}`.
            let mut blank = breaks >= 2;
            if prev.kind == TokenKind::LBrace && !wrote_comment || token.kind == TokenKind::RBrace {
                blank = false;
            }
            // One blank line between top-level items (a comment in front already got its own).
            if top_level && prev.kind == TokenKind::RBrace && starts_item(token.kind) && !wrote_comment {
                blank = true;
            }
            if blank {
                w.blank_line();
            }
            let continuation = !wrote_comment && is_continuation(prev.kind, token.kind);
            let base = w.indent_for_next_token(token.kind, false);
            let indent = w.indent_for_next_token(token.kind, continuation);
            w.start_line(indent, base);
        } else {
            // On the same line: a space, none, or what the author wrote.
            let mut space = space_between(prev, token, before_prev).unwrap_or(gap.had_space);
            if wrote_comment {
                space = true;
            }
            if !space && would_merge(prev.text, token.text) {
                space = true;
            }
            if space {
                w.out.push(' ');
            }
        }

        w.out.push_str(token.text);
        push_open(&mut w, token, index);
    }

    // Tidy the end: no blank lines at the end, one final newline.
    while w.out.ends_with('\n') || w.out.ends_with(' ') {
        w.out.pop();
    }
    if !w.out.is_empty() {
        w.out.push('\n');
    }
    w.out
}

/// Whether the line starting with `next` continues a statement that `prev` left unfinished.
fn is_continuation(prev: TokenKind, next: TokenKind) -> bool {
    if ends_statement(prev) {
        return false;
    }
    // `)` or `]` followed by a binary operator or `.` on the next line continues the expression;
    // a block's final expression followed by `}` does not.
    if starts_continuation(next) {
        return true;
    }
    leaves_open(prev) && !matches!(next, TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket)
}

fn push_open(w: &mut Writer, token: &Tok<'_>, index: usize) {
    use TokenKind::*;
    match token.kind {
        LBrace | LParen | LBracket => w.stack.push(Opener {
            kind: token.kind,
            line_indent: w.base_indent,
            index,
        }),
        RBrace | RParen | RBracket => {
            // A closer matches the nearest opener of its own kind; stray closers are ignored.
            let wanted = match token.kind {
                RBrace => LBrace,
                RParen => LParen,
                _ => LBracket,
            };
            if let Some(position) = w.stack.iter().rposition(|o| o.kind == wanted) {
                w.stack.truncate(position);
            }
        }
        _ => {}
    }
}

/// The text and kind of every token plus every comment, for comparing two sources.
fn fingerprint(src: &str) -> Result<(Vec<(TokenKind, String)>, Vec<String>), FormatError> {
    let tokens = lex_tokens(src)?;
    let mut seen = Vec::new();
    let mut comments = Vec::new();
    let mut previous_end = 0usize;
    for token in &tokens {
        for comment in read_gap(&src[previous_end..token.start]).comments {
            comments.push(normalize_comment(&comment.text));
        }
        previous_end = token.end;
        if token.kind != TokenKind::Eof {
            seen.push((token.kind, token.text.to_string()));
        }
    }
    Ok((seen, comments))
}

fn normalize_comment(text: &str) -> String {
    text.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
}

/// Formats MINK source. The result keeps every token and comment; only white space changes.
pub fn format_source(text: &str) -> Result<String, FormatError> {
    let bom = text.starts_with('\u{feff}');
    let body = text.trim_start_matches('\u{feff}');
    let crlf = body.matches("\r\n").count() * 2 > body.matches('\n').count();
    let normalized = body.replace("\r\n", "\n");

    let tokens = lex_tokens(&normalized)?;
    let formatted = layout(&normalized, &tokens);

    // The promise: same tokens, same comments.
    let (before_tokens, before_comments) = fingerprint(&normalized)?;
    let (after_tokens, after_comments) = fingerprint(&formatted)
        .map_err(|e| FormatError::Mismatch(format!("the result no longer lexes: {e}")))?;
    if before_tokens != after_tokens {
        let at = before_tokens
            .iter()
            .zip(after_tokens.iter())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| before_tokens.len().min(after_tokens.len()));
        return Err(FormatError::Mismatch(format!("tokens differ near token {}", at + 1)));
    }
    if before_comments != after_comments {
        return Err(FormatError::Mismatch("comments differ".to_string()));
    }

    let mut result = if crlf { formatted.replace('\n', "\r\n") } else { formatted };
    if bom {
        result.insert(0, '\u{feff}');
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
