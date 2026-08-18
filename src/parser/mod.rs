//! Parsing: turns a MINK token stream into a syntax tree.
//!
//! The parser consumes the pull-based [`Lexer`] API ([`Lexer::new`] +
//! [`Lexer::next_token`]) and produces the [`Ast`](crate::ast::Ast) described
//! by the frozen grammar in `docs/language/CORE_GRAMMAR.md`. It preserves
//! exact source spans on every node, parses expressions with conventional
//! operator precedence, and recovers from syntax errors so that several
//! independent problems in one file are reported in a single run.
//!
//! Design notes:
//!
//! - **Architecture:** a hand-written recursive-descent parser. Expressions
//!   use precedence climbing through one helper ([`Parser::parse_binary_level`])
//!   over a fixed operator table, so precedence and associativity live in one
//!   place per level and are easy to audit against the grammar.
//! - **Tokens:** `Token` is `Copy`, so the parser keeps a single-token
//!   lookahead window; tokens are consumed exactly once and never reparsed.
//! - **Recovery:** panic-mode recovery skips to the nearest synchronization
//!   point (`;`, `}`, or the next declaration keyword) after a syntax error.
//!   A stack of open delimiters reports unclosed `(`/`{`/`[` at end of input.
//! - **Never panics:** malformed input produces [`ParseError`]s; the parser
//!   itself has no input-dependent failure modes.
//!
//! See `docs/implementation/PARSER_IMPLEMENTATION.md` for the full design
//! record.

mod error;

use crate::ast::{
    AssignOp, Ast, BinaryOp, Block, ConstItem, ElseBranch, EnumItem, EnumVariant, Expr, ExprKind,
    FnItem, Ident, IfStmt, Item, ItemKind, LetItem, MatchArm, MatchStmt, Param, Pattern, Stmt,
    StmtKind, StructField, StructFieldInit, StructItem, Ty, TyKind, UnaryOp,
};
use crate::lexer::{LexError, Lexer, Token, TokenKind};
use crate::source::{SourceFile, Span};

pub use error::{ParseError, ParseErrorKind};

/// The complete result of parsing one source file.
///
/// Carries the [`Ast`] plus every lexical error (recorded by the underlying
/// lexer while the token stream was produced) and every syntax error recorded
/// by the parser, so the caller can report all problems in one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseOutput {
    ast: Ast,
    token_count: usize,
    lex_errors: Vec<LexError>,
    parse_errors: Vec<ParseError>,
}

impl ParseOutput {
    /// The parsed syntax tree.
    pub fn ast(&self) -> &Ast {
        &self.ast
    }

    /// Number of tokens consumed, excluding the final `Eof` token.
    pub fn token_count(&self) -> usize {
        self.token_count
    }

    /// Lexical errors recorded while the token stream was produced.
    pub fn lex_errors(&self) -> &[LexError] {
        &self.lex_errors
    }

    /// Syntax errors recorded by the parser, in the order they were found.
    pub fn parse_errors(&self) -> &[ParseError] {
        &self.parse_errors
    }

    /// Whether the source is both lexically and syntactically valid.
    pub fn is_valid(&self) -> bool {
        self.lex_errors.is_empty() && self.parse_errors.is_empty()
    }

    /// Consumes this result, returning its tree and errors separately.
    pub fn into_parts(self) -> (Ast, Vec<LexError>, Vec<ParseError>) {
        (self.ast, self.lex_errors, self.parse_errors)
    }
}

/// Parses an entire source file into a syntax tree.
///
/// This is the one-shot entry point used by the driver. The parser pulls
/// tokens from a fresh [`Lexer`] over `file`, so the caller never lexes the
/// source twice.
pub fn parse(file: &SourceFile) -> ParseOutput {
    Parser::new(file).parse_all()
}

/// A recursive-descent parser over a single source file.
struct Parser<'a> {
    file: &'a SourceFile,
    lexer: Lexer<'a>,
    /// Single-token lookahead window.
    peeked: Option<Token>,
    /// A synthetic `Eof` token reused once the real one has been consumed.
    eof: Token,
    /// Non-`Eof` tokens consumed so far.
    token_count: usize,
    /// Syntax errors recorded so far, in the order they were found.
    errors: Vec<ParseError>,
    /// Stack of open `(`/`{`/`[` delimiters with their opener spans, used to
    /// report unclosed delimiters at end of input.
    open_delims: Vec<(TokenKind, Span)>,
    /// Set once an unclosed delimiter has been reported at end of input, so
    /// that outer, still-open delimiters do not cascade into further errors
    /// for the same root cause.
    eof_unclosed_reported: bool,
    /// Whether the expression being parsed sits directly before a `{ ... }`
    /// block in the grammar (`if`/`while` conditions, `for` iterables). In
    /// that position an `Ident {` is the block, not a struct literal, so
    /// struct literals are disabled at the top level of such expressions
    /// (parenthesized groups re-enable them). See
    /// `docs/implementation/AGGREGATE_TYPES_IMPLEMENTATION.md`.
    in_block_context: bool,
}

impl<'a> Parser<'a> {
    fn new(file: &'a SourceFile) -> Self {
        let eof = Token::new(TokenKind::Eof, Span::new(file.id(), file.len()..file.len()));
        Self {
            file,
            lexer: Lexer::new(file),
            peeked: None,
            eof,
            token_count: 0,
            errors: Vec::new(),
            open_delims: Vec::new(),
            eof_unclosed_reported: false,
            in_block_context: false,
        }
    }

    fn parse_all(mut self) -> ParseOutput {
        let ast = self.parse_program();
        ParseOutput {
            ast,
            token_count: self.token_count,
            lex_errors: self.lexer.errors().to_vec(),
            parse_errors: self.errors,
        }
    }

    // ------------------------------------------------------------------
    // Token plumbing
    // ------------------------------------------------------------------

    /// The current token, pulling from the lexer on first use. Returns the
    /// synthetic `Eof` once the real `Eof` has been consumed.
    fn current(&mut self) -> Token {
        if self.peeked.is_none() {
            self.peeked = Some(self.lexer.next_token().unwrap_or(self.eof));
        }
        match self.peeked {
            Some(token) => token,
            None => self.eof,
        }
    }

    /// The kind of the current token.
    fn current_kind(&mut self) -> TokenKind {
        self.current().kind()
    }

    /// Consumes and returns the current token, refilling the lookahead
    /// window. Non-`Eof` tokens are counted exactly once.
    fn bump(&mut self) -> Token {
        let token = self.current();
        if token.kind() != TokenKind::Eof {
            self.token_count += 1;
        }
        self.peeked = None;
        token
    }

    /// Consumes the current token if its kind matches `kind`.
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.current_kind() == kind {
            let _ = self.bump();
            true
        } else {
            false
        }
    }

    /// The source text covered by `span`. Token spans are always valid.
    fn text(&self, span: Span) -> String {
        self.file
            .span_text(span)
            .expect("token spans are always in bounds")
            .to_string()
    }

    /// The smallest span covering both `a` and `b` within this file.
    fn join(&self, a: Span, b: Span) -> Span {
        Span::new(
            self.file.id(),
            a.start().min(b.start())..a.end().max(b.end()),
        )
    }

    /// An empty span at the current position, used for placeholder blocks
    /// synthesized after an unrecoverable missing block.
    fn point_span(&mut self) -> Span {
        let pos = self.current().span().start();
        Span::new(self.file.id(), pos..pos)
    }

    /// Records a syntax error of `kind` over `span`.
    fn record_error(&mut self, kind: ParseErrorKind, span: Span) {
        self.errors.push(ParseError::new(kind, span));
    }

    /// Records that `open_kind` was never closed before end of input, pointing
    /// at the delimiter's opener span. Only the innermost unclosed delimiter
    /// is reported: once one has been reported, outer delimiters left open by
    /// the same failure are not reported again. (The early return also
    /// deliberately skips popping the stale delimiter — after end of input
    /// nothing inspects the stack again, so keeping it is harmless.)
    fn report_unclosed(&mut self, open_kind: TokenKind, error_kind: ParseErrorKind) {
        if self.eof_unclosed_reported {
            return;
        }
        self.eof_unclosed_reported = true;
        let span = match self
            .open_delims
            .iter()
            .rposition(|(kind, _)| *kind == open_kind)
        {
            Some(index) => {
                let span = self.open_delims[index].1;
                self.open_delims.remove(index);
                span
            }
            None => self.current().span(),
        };
        self.record_error(error_kind, span);
    }

    // ------------------------------------------------------------------
    // Recovery
    // ------------------------------------------------------------------

    /// Skips tokens up to (but not consuming) `;`, `}`, or `Eof`, consuming
    /// a `;` if one is reached. A `{ ... }` group encountered on the way is
    /// skipped as a unit so that a malformed statement cannot swallow a
    /// following block. Used after a statement-level error.
    fn recover_statement(&mut self) {
        while !matches!(
            self.current_kind(),
            TokenKind::Semi | TokenKind::RBrace | TokenKind::Eof
        ) {
            if self.current_kind() == TokenKind::LBrace {
                self.skip_balanced_brace_group();
            } else {
                let _ = self.bump();
            }
        }
        if self.current_kind() == TokenKind::Semi {
            let _ = self.bump();
        }
    }

    /// Skips tokens up to (but not consuming) the next top-level declaration
    /// keyword or `Eof`, skipping `{ ... }` groups as units. Used after an
    /// item-level error.
    fn recover_item(&mut self) {
        while !matches!(
            self.current_kind(),
            TokenKind::Fn
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Let
                | TokenKind::Const
                | TokenKind::Eof
        ) {
            if self.current_kind() == TokenKind::LBrace {
                self.skip_balanced_brace_group();
            } else {
                let _ = self.bump();
            }
        }
    }

    /// Consumes one balanced `{ ... }` group, assuming the current token is
    /// `{`. If the group is never closed, consumes to end of input.
    fn skip_balanced_brace_group(&mut self) {
        let _ = self.bump(); // '{'
        let mut depth = 1u32;
        while depth > 0 {
            match self.current_kind() {
                TokenKind::LBrace => {
                    depth += 1;
                    let _ = self.bump();
                }
                TokenKind::RBrace => {
                    depth -= 1;
                    let _ = self.bump();
                }
                TokenKind::Eof => break,
                _ => {
                    let _ = self.bump();
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Program and items
    // ------------------------------------------------------------------

    fn parse_program(&mut self) -> Ast {
        let mut items = Vec::new();
        loop {
            match self.current_kind() {
                TokenKind::Eof => break,
                TokenKind::Semi => {
                    // Empty statement at module scope: consume silently.
                    let _ = self.bump();
                }
                TokenKind::Fn
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Let
                | TokenKind::Const => match self.parse_item() {
                    Ok(item) => items.push(item),
                    Err(()) => self.recover_item(),
                },
                _ => {
                    let token = self.current();
                    self.record_error(ParseErrorKind::ExpectedItem, token.span());
                    self.recover_item();
                }
            }
        }
        Ast::new(items)
    }

    fn parse_item(&mut self) -> Result<Item, ()> {
        match self.current_kind() {
            TokenKind::Fn => {
                let (func, span) = self.parse_fn()?;
                Ok(Item {
                    kind: ItemKind::Fn(func),
                    span,
                })
            }
            TokenKind::Struct => {
                let (struct_item, span) = self.parse_struct()?;
                Ok(Item {
                    kind: ItemKind::Struct(struct_item),
                    span,
                })
            }
            TokenKind::Enum => {
                let (enum_item, span) = self.parse_enum()?;
                Ok(Item {
                    kind: ItemKind::Enum(enum_item),
                    span,
                })
            }
            TokenKind::Let => {
                let (binding, span) = self.parse_let()?;
                Ok(Item {
                    kind: ItemKind::Let(binding),
                    span,
                })
            }
            TokenKind::Const => {
                let (binding, span) = self.parse_const()?;
                Ok(Item {
                    kind: ItemKind::Const(binding),
                    span,
                })
            }
            _ => Err(()),
        }
    }

    /// Parses an `enum Name { Variant, ... }` declaration, including a
    /// trailing comma. A variant is either a bare identifier (a unit
    /// variant) or an identifier followed by a parenthesized payload type
    /// (a data-carrying variant, session 19): `Variant(Type)`. Match
    /// statements (session 18) reference variants through `E::V` patterns
    /// in `parse_pattern`, not through the declaration.
    fn parse_enum(&mut self) -> Result<(EnumItem, Span), ()> {
        let start = self.bump().span(); // 'enum'
        let name = self.expect_ident()?;
        if self.current_kind() != TokenKind::LBrace {
            let token = self.current();
            self.record_error(ParseErrorKind::ExpectedBlock, token.span());
            return Err(());
        }
        let open = self.bump().span();
        self.open_delims.push((TokenKind::LBrace, open));
        let mut variants = Vec::new();
        loop {
            match self.current_kind() {
                TokenKind::RBrace => {
                    let close = self.bump().span();
                    self.open_delims.pop();
                    let span = self.join(start, close);
                    return Ok((
                        EnumItem {
                            name,
                            variants,
                            span,
                        },
                        span,
                    ));
                }
                TokenKind::Eof => {
                    self.report_unclosed(TokenKind::LBrace, ParseErrorKind::UnclosedBrace);
                    return Err(());
                }
                _ => {
                    let token = self.current();
                    if token.kind() != TokenKind::Ident {
                        self.record_error(ParseErrorKind::ExpectedIdentifier, token.span());
                        self.skip_to_variant_boundary();
                    } else {
                        let _ = self.bump();
                        let name = Ident {
                            name: self.text(token.span()),
                            span: token.span(),
                        };
                        let mut span = token.span();
                        let mut payload = None;
                        if self.current_kind() == TokenKind::LParen {
                            match self.parse_variant_payload_type() {
                                Ok((ty, close)) => {
                                    span = self.join(token.span(), close);
                                    payload = Some(ty);
                                }
                                Err(()) => {
                                    // An error is already recorded; the
                                    // cursor has been recovered to a variant
                                    // boundary (`,`, `}`, `Eof`) or past the
                                    // offending `)`, so the boundary handling
                                    // below proceeds normally.
                                }
                            }
                        }
                        // An explicit discriminant (session 20): `V = 5`.
                        // The literal's wrapping 64-bit value is decoded by
                        // type analysis; the span grows to cover `= literal`.
                        let mut discriminant = None;
                        if self.current_kind() == TokenKind::Eq {
                            match self.parse_variant_discriminant() {
                                Ok((literal, end)) => {
                                    span = self.join(span, end);
                                    discriminant = Some(literal);
                                }
                                Err(()) => {
                                    // An error is already recorded; the
                                    // cursor has been recovered to a variant
                                    // boundary, so the boundary handling
                                    // below proceeds normally.
                                }
                            }
                        }
                        variants.push(EnumVariant {
                            name,
                            payload,
                            discriminant,
                            span,
                        });
                    }
                    match self.current_kind() {
                        TokenKind::Comma => {
                            let _ = self.bump();
                            if self.current_kind() == TokenKind::RBrace {
                                break;
                            }
                        }
                        TokenKind::RBrace => break,
                        TokenKind::Eof => {
                            self.report_unclosed(TokenKind::LBrace, ParseErrorKind::UnclosedBrace);
                            return Err(());
                        }
                        _ => {
                            let token = self.current();
                            self.record_error(ParseErrorKind::ExpectedComma, token.span());
                            self.skip_to_variant_boundary();
                            if self.current_kind() == TokenKind::Comma {
                                let _ = self.bump();
                            }
                        }
                    }
                }
            }
        }
        let close = self.bump().span();
        self.open_delims.pop();
        let span = self.join(start, close);
        Ok((
            EnumItem {
                name,
                variants,
                span,
            },
            span,
        ))
    }

    /// Parses the parenthesized payload type of a data-carrying variant
    /// declaration, after the variant name. The `(` has not yet been
    /// consumed. Returns the parsed type and the closing `)` span, or `Err`
    /// after recording an error and recovering the cursor to a variant
    /// boundary (`,`, `}`, `Eof`) or past the offending `)`.
    fn parse_variant_payload_type(&mut self) -> Result<(Ty, Span), ()> {
        let open = self.bump().span(); // '('
        self.open_delims.push((TokenKind::LParen, open));
        // `Variant()` is not a data-carrying variant: a payload is
        // required (E-P25).
        if self.current_kind() == TokenKind::RParen {
            let bad = self.current();
            self.record_error(ParseErrorKind::EmptyPayload, bad.span());
            let _ = self.bump();
            self.open_delims.pop();
            return Err(());
        }
        let ty = match self.parse_type() {
            Ok(ty) => ty,
            Err(()) => {
                // The type failed to parse (an error is already recorded);
                // recover to `)` or a variant boundary.
                self.recover_variant_payload();
                return Err(());
            }
        };
        if self.current_kind() != TokenKind::RParen {
            let bad = self.current();
            self.record_error(ParseErrorKind::ExpectedRParen, bad.span());
            self.recover_variant_payload();
            return Err(());
        }
        let close = self.bump().span();
        self.open_delims.pop();
        Ok((ty, close))
    }

    /// Parses an explicit variant discriminant after the `=` of
    /// `Variant = IntLit` (session 20): an integer literal, optionally
    /// negated (`A = 5`, `A = -1`, `A = 0x10`, `A = 1_000`). Returns the
    /// literal expression and the span covering `= literal`. A missing,
    /// non-integer, or float value is `E-P19` (expected an integer
    /// literal); the cursor is recovered to a variant boundary so the
    /// variant-list loop continues cleanly.
    fn parse_variant_discriminant(&mut self) -> Result<(Expr, Span), ()> {
        let eq = self.bump().span(); // '='
        let token = self.current();
        let literal = match token.kind() {
            TokenKind::Int => {
                let _ = self.bump();
                Expr {
                    kind: ExprKind::Int,
                    span: token.span(),
                }
            }
            TokenKind::Minus => {
                let minus = self.bump().span(); // '-'
                let token = self.current();
                if token.kind() != TokenKind::Int {
                    self.record_error(ParseErrorKind::ExpectedIntegerLiteral, token.span());
                    self.skip_to_variant_boundary();
                    return Err(());
                }
                let _ = self.bump();
                let literal = Expr {
                    kind: ExprKind::Int,
                    span: token.span(),
                };
                let span = self.join(minus, token.span());
                Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::Neg,
                        operand: Box::new(literal),
                    },
                    span,
                }
            }
            _ => {
                self.record_error(ParseErrorKind::ExpectedIntegerLiteral, token.span());
                self.skip_to_variant_boundary();
                return Err(());
            }
        };
        let span = self.join(eq, literal.span);
        Ok((literal, span))
    }

    /// Recovers from a malformed variant payload: skips to the closing `)`
    /// (consuming it) or, failing that, to a variant boundary. A `,` is not
    /// a boundary here — it sits inside the payload's parentheses (an extra
    /// payload argument such as `V(Int, Int)`), so it is skipped with the
    /// rest of the malformed payload rather than being mistaken for the
    /// variant separator. Also pops the payload's `(` from the
    /// open-delimiter stack.
    fn recover_variant_payload(&mut self) {
        while !matches!(
            self.current_kind(),
            TokenKind::RParen | TokenKind::RBrace | TokenKind::Eof
        ) {
            let _ = self.bump();
        }
        if self.current_kind() == TokenKind::RParen {
            let _ = self.bump();
        }
        self.open_delims.pop();
    }

    /// Skips tokens up to (but not consuming) the next variant boundary:
    /// `,`, `}`, or `Eof`. Used to recover from a malformed enum variant.
    fn skip_to_variant_boundary(&mut self) {
        while !matches!(
            self.current_kind(),
            TokenKind::Comma | TokenKind::RBrace | TokenKind::Eof
        ) {
            let _ = self.bump();
        }
    }

    /// Parses a `struct Name { field: Type, ... }` declaration.
    fn parse_struct(&mut self) -> Result<(StructItem, Span), ()> {
        let start = self.bump().span(); // 'struct'
        let name = self.expect_ident()?;
        if self.current_kind() != TokenKind::LBrace {
            let token = self.current();
            self.record_error(ParseErrorKind::ExpectedBlock, token.span());
            return Err(());
        }
        let open = self.bump().span();
        self.open_delims.push((TokenKind::LBrace, open));
        let mut fields = Vec::new();
        loop {
            match self.current_kind() {
                TokenKind::RBrace => {
                    let close = self.bump().span();
                    self.open_delims.pop();
                    let span = self.join(start, close);
                    return Ok((StructItem { name, fields, span }, span));
                }
                TokenKind::Eof => {
                    self.report_unclosed(TokenKind::LBrace, ParseErrorKind::UnclosedBrace);
                    return Err(());
                }
                _ => {
                    match self.parse_struct_field() {
                        Ok(field) => fields.push(field),
                        Err(()) => self.skip_to_field_boundary(),
                    }
                    match self.current_kind() {
                        TokenKind::Comma => {
                            let _ = self.bump();
                            if self.current_kind() == TokenKind::RBrace {
                                break;
                            }
                        }
                        TokenKind::RBrace => break,
                        TokenKind::Eof => {
                            self.report_unclosed(TokenKind::LBrace, ParseErrorKind::UnclosedBrace);
                            return Err(());
                        }
                        _ => {
                            let token = self.current();
                            self.record_error(ParseErrorKind::ExpectedComma, token.span());
                            self.skip_to_field_boundary();
                            if self.current_kind() == TokenKind::Comma {
                                let _ = self.bump();
                            }
                        }
                    }
                }
            }
        }
        let close = self.bump().span();
        self.open_delims.pop();
        let span = self.join(start, close);
        Ok((StructItem { name, fields, span }, span))
    }

    /// Parses one `name: Type` struct field declaration.
    fn parse_struct_field(&mut self) -> Result<StructField, ()> {
        let name = self.expect_ident()?;
        if self.current_kind() != TokenKind::Colon {
            let token = self.current();
            self.record_error(ParseErrorKind::ExpectedColon, token.span());
            return Err(());
        }
        let _ = self.bump(); // ':'
        let ty = self.parse_type()?;
        let span = self.join(name.span, ty.span);
        Ok(StructField { name, ty, span })
    }

    /// Parses a type: a named type (`Int`, a struct name), `Ptr<T>`, or a
    /// fixed-length array type `[T; N]`.
    fn parse_type(&mut self) -> Result<Ty, ()> {
        let token = self.current();
        match token.kind() {
            TokenKind::Amp => {
                // A reference type (session 16): `&T` (shared) or
                // `&mut T` (exclusive).
                let start = self.bump().span();
                let mutable = if self.current_kind() == TokenKind::Mut {
                    let _ = self.bump();
                    true
                } else {
                    false
                };
                let inner = self.parse_type()?;
                let span = self.join(start, inner.span);
                Ok(Ty {
                    kind: TyKind::Ref {
                        mutable,
                        inner: Box::new(inner),
                    },
                    span,
                })
            }
            TokenKind::Ident => {
                let _ = self.bump();
                let name = self.text(token.span());
                let ident = Ident {
                    name,
                    span: token.span(),
                };
                if self.current_kind() == TokenKind::Lt {
                    if ident.name != "Ptr" {
                        // Only `Ptr<T>` has the generic form; anything else
                        // is rejected rather than silently misparsed.
                        let lt = self.current();
                        self.record_error(ParseErrorKind::ExpectedGT, lt.span());
                        while !matches!(
                            self.current_kind(),
                            TokenKind::Gt | TokenKind::Comma | TokenKind::RBrace | TokenKind::Eof
                        ) {
                            let _ = self.bump();
                        }
                        if self.current_kind() == TokenKind::Gt {
                            let _ = self.bump();
                        }
                        return Err(());
                    }
                    let _ = self.bump(); // '<'
                    let inner = self.parse_type()?;
                    if self.current_kind() != TokenKind::Gt {
                        let bad = self.current();
                        self.record_error(ParseErrorKind::ExpectedGT, bad.span());
                        return Err(());
                    }
                    let gt = self.bump().span();
                    let span = self.join(token.span(), gt);
                    Ok(Ty {
                        kind: TyKind::Ptr(Box::new(inner)),
                        span,
                    })
                } else {
                    Ok(Ty {
                        kind: TyKind::Named(ident),
                        span: token.span(),
                    })
                }
            }
            TokenKind::LBracket => {
                let open = self.bump().span();
                self.open_delims.push((TokenKind::LBracket, open));
                let elem = self.parse_type()?;
                if self.current_kind() != TokenKind::Semi {
                    let bad = self.current();
                    self.record_error(ParseErrorKind::ExpectedSemicolon, bad.span());
                    return Err(());
                }
                let _ = self.bump(); // ';'
                let len = self.parse_array_len()?;
                let close = self.expect_bracket_close()?;
                let span = self.join(open, close);
                Ok(Ty {
                    kind: TyKind::Array {
                        elem: Box::new(elem),
                        len,
                    },
                    span,
                })
            }
            _ => {
                self.record_error(ParseErrorKind::ExpectedType, token.span());
                Err(())
            }
        }
    }

    /// Parses the length of an array type: a plain integer literal.
    fn parse_array_len(&mut self) -> Result<Expr, ()> {
        let token = self.current();
        if token.kind() == TokenKind::Int {
            let _ = self.bump();
            Ok(Expr {
                kind: ExprKind::Int,
                span: token.span(),
            })
        } else {
            self.record_error(ParseErrorKind::ExpectedIntegerLiteral, token.span());
            Err(())
        }
    }

    /// Skips tokens up to (but not consuming) the next field boundary: `,`,
    /// `}`, or `Eof`. Used to recover from a malformed struct field.
    fn skip_to_field_boundary(&mut self) {
        while !matches!(
            self.current_kind(),
            TokenKind::Comma | TokenKind::RBrace | TokenKind::Eof
        ) {
            let _ = self.bump();
        }
    }

    fn parse_fn(&mut self) -> Result<(FnItem, Span), ()> {
        let start = self.bump().span(); // 'fn'
        let name = self.expect_ident()?;
        self.expect_lparen()?;
        let params = self.parse_params()?;
        // Optional return-type annotation: `-> Type`.
        let return_ty = if self.current_kind() == TokenKind::Arrow {
            let _ = self.bump(); // '->'
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = match self.parse_block_body() {
            Ok(block) => block,
            Err(()) => {
                // The missing block is the error (already recorded); skip to
                // the next declaration so parsing can continue.
                self.recover_item();
                Block {
                    stmts: Vec::new(),
                    span: self.point_span(),
                }
            }
        };
        let span = self.join(start, body.span);
        Ok((
            FnItem {
                name,
                params,
                return_ty,
                body,
            },
            span,
        ))
    }

    fn parse_let(&mut self) -> Result<(LetItem, Span), ()> {
        let start = self.bump().span(); // 'let'
        let mutable = self.eat(TokenKind::Mut);
        self.parse_binding_tail(start, mutable)
    }

    fn parse_const(&mut self) -> Result<(ConstItem, Span), ()> {
        let start = self.bump().span(); // 'const'
        let (binding, span) = self.parse_binding_tail(start, false)?;
        Ok((
            ConstItem {
                name: binding.name,
                ty: binding.ty,
                init: binding.init,
            },
            span,
        ))
    }

    /// Parses `name [: Type] = expr ;` shared by `let` and `const` bindings.
    fn parse_binding_tail(&mut self, start: Span, mutable: bool) -> Result<(LetItem, Span), ()> {
        let name = self.expect_ident()?;
        // Optional type annotation: `name: Type`.
        let ty = if self.current_kind() == TokenKind::Colon {
            let _ = self.bump(); // ':'
            Some(self.parse_type()?)
        } else {
            None
        };
        if self.current_kind() != TokenKind::Eq {
            let token = self.current();
            self.record_error(ParseErrorKind::ExpectedEqual, token.span());
            return Err(());
        }
        let _ = self.bump();
        let init = self.parse_expression()?;
        let semi = self.expect_semi()?;
        let span = self.join(start, semi);
        Ok((
            LetItem {
                name,
                mutable,
                ty,
                init,
            },
            span,
        ))
    }

    /// Parses the parameter list up to and including `)`.
    fn parse_params(&mut self) -> Result<Vec<Param>, ()> {
        let mut params = Vec::new();
        if self.eat(TokenKind::RParen) {
            self.open_delims.pop();
            return Ok(params);
        }
        loop {
            if self.current_kind() == TokenKind::Eof {
                self.report_unclosed(TokenKind::LParen, ParseErrorKind::UnclosedParen);
                return Err(());
            }
            params.push(self.parse_param()?);
            match self.current_kind() {
                TokenKind::Comma => {
                    let _ = self.bump();
                    if self.current_kind() == TokenKind::RParen {
                        break;
                    }
                }
                TokenKind::RParen => break,
                TokenKind::Eof => {
                    self.report_unclosed(TokenKind::LParen, ParseErrorKind::UnclosedParen);
                    return Err(());
                }
                _ => {
                    let token = self.current();
                    self.record_error(ParseErrorKind::ExpectedComma, token.span());
                    // Skip the offending tokens up to the list terminator or a
                    // statement boundary (`;`/`}`), so a stray `;` or `}` does
                    // not swallow the enclosing block.
                    while !matches!(
                        self.current_kind(),
                        TokenKind::Comma
                            | TokenKind::RParen
                            | TokenKind::Semi
                            | TokenKind::RBrace
                            | TokenKind::Eof
                    ) {
                        let _ = self.bump();
                    }
                    match self.current_kind() {
                        TokenKind::Comma => {
                            let _ = self.bump();
                            // A recovered comma before `)` is a trailing comma:
                            // finish the list without a second error.
                            if self.current_kind() == TokenKind::RParen {
                                break;
                            }
                        }
                        TokenKind::RParen => break,
                        TokenKind::Eof => {
                            self.report_unclosed(TokenKind::LParen, ParseErrorKind::UnclosedParen);
                            return Err(());
                        }
                        // Stopped at `;` or `}`: leave the statement-boundary
                        // recovery to the caller.
                        _ => return Err(()),
                    }
                }
            }
        }
        let _ = self.bump(); // ')'
        self.open_delims.pop();
        Ok(params)
    }

    fn parse_param(&mut self) -> Result<Param, ()> {
        let name = self.expect_ident()?;
        // Optional type annotation: `name: Type`.
        let ty = if self.current_kind() == TokenKind::Colon {
            let _ = self.bump(); // ':'
            Some(self.parse_type()?)
        } else {
            None
        };
        let span = match &ty {
            Some(t) => self.join(name.span, t.span),
            None => name.span,
        };
        Ok(Param { name, ty, span })
    }

    // ------------------------------------------------------------------
    // Statements
    // ------------------------------------------------------------------

    fn parse_statement(&mut self) -> Result<Stmt, ()> {
        match self.current_kind() {
            TokenKind::Let => {
                let (binding, span) = self.parse_let()?;
                Ok(Stmt {
                    kind: StmtKind::Let(binding),
                    span,
                })
            }
            TokenKind::Const => {
                let (binding, span) = self.parse_const()?;
                Ok(Stmt {
                    kind: StmtKind::Const(binding),
                    span,
                })
            }
            TokenKind::Return => self.parse_return(),
            TokenKind::Break => self.parse_break_or_continue(StmtKind::Break),
            TokenKind::Continue => self.parse_break_or_continue(StmtKind::Continue),
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::Match => self.parse_match(),
            _ => {
                let expr = self.parse_expression()?;
                let semi = self.expect_semi()?;
                let span = self.join(expr.span, semi);
                Ok(Stmt {
                    kind: StmtKind::Expr(expr),
                    span,
                })
            }
        }
    }

    fn parse_return(&mut self) -> Result<Stmt, ()> {
        let start = self.bump().span(); // 'return'
        if self.current_kind() == TokenKind::Semi {
            let semi = self.bump().span();
            let span = self.join(start, semi);
            return Ok(Stmt {
                kind: StmtKind::Return(None),
                span,
            });
        }
        let value = self.parse_expression()?;
        let semi = self.expect_semi()?;
        let span = self.join(start, semi);
        Ok(Stmt {
            kind: StmtKind::Return(Some(value)),
            span,
        })
    }

    fn parse_break_or_continue(&mut self, kind: StmtKind) -> Result<Stmt, ()> {
        let start = self.bump().span();
        let semi = self.expect_semi()?;
        let span = self.join(start, semi);
        Ok(Stmt { kind, span })
    }

    fn parse_if(&mut self) -> Result<Stmt, ()> {
        let if_stmt = self.parse_if_stmt()?;
        let span = if_stmt.span;
        Ok(Stmt {
            kind: StmtKind::If(if_stmt),
            span,
        })
    }

    fn parse_if_stmt(&mut self) -> Result<IfStmt, ()> {
        let start = self.bump().span(); // 'if'
        // The condition is followed by a `{ ... }` block in the grammar, so
        // an `Ident {` in it is the block, never a struct literal.
        let saved = self.in_block_context;
        self.in_block_context = true;
        let cond = self.parse_expression()?;
        self.in_block_context = saved;
        let then_block = self.parse_body_or_recover();
        let else_branch = if self.current_kind() == TokenKind::Else {
            let _ = self.bump();
            match self.current_kind() {
                TokenKind::If => {
                    let nested = self.parse_if_stmt()?;
                    Some(ElseBranch::If(Box::new(nested)))
                }
                TokenKind::LBrace => match self.parse_block_body() {
                    Ok(block) => Some(ElseBranch::Block(block)),
                    Err(()) => {
                        self.recover_statement();
                        None
                    }
                },
                _ => {
                    let token = self.current();
                    self.record_error(ParseErrorKind::ExpectedBlock, token.span());
                    self.recover_statement();
                    None
                }
            }
        } else {
            None
        };
        let end = match &else_branch {
            Some(ElseBranch::If(nested)) => nested.span,
            Some(ElseBranch::Block(block)) => block.span,
            None => then_block.span,
        };
        let span = self.join(start, end);
        Ok(IfStmt {
            cond,
            then_block,
            else_branch,
            span,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, ()> {
        let start = self.bump().span(); // 'while'
        let saved = self.in_block_context;
        self.in_block_context = true;
        let cond = self.parse_expression()?;
        self.in_block_context = saved;
        let body = self.parse_body_or_recover();
        let span = self.join(start, body.span);
        Ok(Stmt {
            kind: StmtKind::While { cond, body },
            span,
        })
    }

    fn parse_for(&mut self) -> Result<Stmt, ()> {
        let start = self.bump().span(); // 'for'
        let name = self.expect_ident()?;
        if self.current_kind() != TokenKind::In {
            let token = self.current();
            self.record_error(ParseErrorKind::ExpectedIn, token.span());
            return Err(());
        }
        let _ = self.bump(); // 'in'
        let saved = self.in_block_context;
        self.in_block_context = true;
        let iterable = self.parse_expression()?;
        self.in_block_context = saved;
        let body = self.parse_body_or_recover();
        let span = self.join(start, body.span);
        Ok(Stmt {
            kind: StmtKind::For {
                name,
                iterable,
                body,
            },
            span,
        })
    }

    fn parse_loop(&mut self) -> Result<Stmt, ()> {
        let start = self.bump().span(); // 'loop'
        let body = self.parse_body_or_recover();
        let span = self.join(start, body.span);
        Ok(Stmt {
            kind: StmtKind::Loop(body),
            span,
        })
    }

    /// Parses a `match scrutinee { pattern => block, ... }` statement
    /// (session 18). Arms are `pattern => block` pairs separated by commas
    /// (a trailing comma is allowed); a block terminates its arm
    /// unambiguously, so the separator is required only between arms.
    fn parse_match(&mut self) -> Result<Stmt, ()> {
        let start = self.bump().span(); // 'match'
        // The scrutinee is followed by a `{ ... }` block in the grammar,
        // so an `Ident {` in it is the block, never a struct literal.
        let saved = self.in_block_context;
        self.in_block_context = true;
        let scrutinee = self.parse_expression()?;
        self.in_block_context = saved;
        if self.current_kind() != TokenKind::LBrace {
            let token = self.current();
            self.record_error(ParseErrorKind::ExpectedBlock, token.span());
            return Err(());
        }
        let open = self.bump().span();
        self.open_delims.push((TokenKind::LBrace, open));
        let mut arms = Vec::new();
        loop {
            match self.current_kind() {
                TokenKind::RBrace => {
                    let close = self.bump().span();
                    self.open_delims.pop();
                    let span = self.join(start, close);
                    return Ok(Stmt {
                        kind: StmtKind::Match(MatchStmt {
                            scrutinee,
                            arms,
                            span,
                        }),
                        span,
                    });
                }
                TokenKind::Eof => {
                    self.report_unclosed(TokenKind::LBrace, ParseErrorKind::UnclosedBrace);
                    return Err(());
                }
                _ => match self.parse_match_arm() {
                    Ok(arm) => arms.push(arm),
                    Err(()) => self.skip_to_arm_boundary(),
                },
            }
            // The separator after an arm: a comma (trailing commas are
            // allowed, mirroring struct fields and enum variants) or the
            // closing brace. A missing comma is recorded and recovered
            // from; an arm's block ends unambiguously, so parsing can
            // continue at the next arm.
            match self.current_kind() {
                TokenKind::Comma => {
                    let _ = self.bump();
                    if self.current_kind() == TokenKind::RBrace {
                        break;
                    }
                }
                TokenKind::RBrace => break,
                TokenKind::Eof => {
                    self.report_unclosed(TokenKind::LBrace, ParseErrorKind::UnclosedBrace);
                    return Err(());
                }
                _ => {
                    let token = self.current();
                    self.record_error(ParseErrorKind::ExpectedComma, token.span());
                    self.skip_to_arm_boundary();
                    if self.current_kind() == TokenKind::Comma {
                        let _ = self.bump();
                    }
                }
            }
        }
        // The arm list ended at the closing brace (the separator `break`).
        let close = self.bump().span();
        self.open_delims.pop();
        let span = self.join(start, close);
        Ok(Stmt {
            kind: StmtKind::Match(MatchStmt {
                scrutinee,
                arms,
                span,
            }),
            span,
        })
    }

    /// Parses one match arm: `pattern ('if' Expr)? => block` (the guard
    /// arrived in session 27).
    fn parse_match_arm(&mut self) -> Result<MatchArm, ()> {
        let pattern = self.parse_pattern()?;
        // A guard (session 27): `pattern if expr =>`. The guard is parsed
        // in ordinary expression context; `=>` is not an expression token,
        // so it terminates the guard expression naturally. The `|`
        // ambiguity is resolved by position: `|` before `if` continues the
        // or-pattern, `|` after `if` is part of the guard expression.
        let mut guard = None;
        if self.current_kind() == TokenKind::If {
            let _ = self.bump(); // 'if'
            if let Ok(expr) = self.parse_expression() {
                guard = Some(expr);
            }
        }
        if self.current_kind() != TokenKind::FatArrow {
            let token = self.current();
            self.record_error(ParseErrorKind::ExpectedFatArrow, token.span());
            return Err(());
        }
        let _ = self.bump(); // '=>'
        let body = match self.parse_block_body() {
            Ok(block) => block,
            Err(()) => {
                self.recover_statement();
                Block {
                    stmts: Vec::new(),
                    span: self.point_span(),
                }
            }
        };
        let span = self.join(pattern.span(), body.span);
        Ok(MatchArm {
            pattern,
            guard,
            body,
            span,
        })
    }

    /// Parses a match pattern (sessions 18–27): a single pattern, an
    /// integer range (`1..=5`, `1..5`), or an or-pattern (`1 | 2 | 3`,
    /// `E::A(x) | E::B(x)`). Alternatives are separated by `|`; each
    /// alternative may itself be a range (`1 | 2..=5`).
    fn parse_pattern(&mut self) -> Result<Pattern, ()> {
        let first = self.parse_pattern_atom()?;
        // The `|` token cannot follow a pattern in any other position, so
        // it unambiguously starts another or-pattern alternative.
        if self.current_kind() != TokenKind::Pipe {
            return Ok(first);
        }
        let start = first.span();
        let mut alternatives = vec![first];
        while self.current_kind() == TokenKind::Pipe {
            let _ = self.bump(); // '|'
            match self.parse_pattern_atom() {
                Ok(alternative) => alternatives.push(alternative),
                Err(()) => return Err(()),
            }
        }
        let last = alternatives.last().expect("at least the first alternative");
        let span = self.join(start, last.span());
        Ok(Pattern::Or { alternatives, span })
    }

    /// Parses one or-pattern alternative: a single pattern, optionally
    /// followed by a range continuation (`lo..=hi` / `lo..hi`).
    fn parse_pattern_atom(&mut self) -> Result<Pattern, ()> {
        let pattern = self.parse_pattern_base()?;
        if matches!(self.current_kind(), TokenKind::DotDot | TokenKind::DotDotEq) {
            return self.parse_range_pattern(pattern);
        }
        Ok(pattern)
    }

    /// Parses an integer range pattern from a parsed `lo` endpoint: the
    /// `..`/`..=` operator, then the `hi` endpoint. Both endpoints must be
    /// integer literal patterns (`E-P19` otherwise); a range endpoint is a
    /// single literal, never another range (`1..2..3` is `E-P19`).
    fn parse_range_pattern(&mut self, lo: Pattern) -> Result<Pattern, ()> {
        if !matches!(lo, Pattern::Int { .. }) {
            let token = self.current();
            self.record_error(ParseErrorKind::ExpectedIntegerLiteral, token.span());
            return Err(());
        }
        let inclusive = self.current_kind() == TokenKind::DotDotEq;
        let _ = self.bump(); // '..' or '..='
        let hi = self.parse_range_endpoint()?;
        if matches!(self.current_kind(), TokenKind::DotDot | TokenKind::DotDotEq) {
            let token = self.current();
            self.record_error(ParseErrorKind::ExpectedIntegerLiteral, token.span());
            return Err(());
        }
        let span = self.join(lo.span(), hi.span());
        Ok(Pattern::Range {
            lo: Box::new(lo),
            hi: Box::new(hi),
            inclusive,
            span,
        })
    }

    /// Parses one range-pattern endpoint: an integer literal, optionally
    /// negated (`5`, `-5`). A non-literal endpoint is `E-P19` (expected an
    /// integer literal).
    fn parse_range_endpoint(&mut self) -> Result<Pattern, ()> {
        let token = self.current();
        match token.kind() {
            TokenKind::Int => {
                let _ = self.bump();
                Ok(Pattern::Int {
                    negative: false,
                    literal: Expr {
                        kind: ExprKind::Int,
                        span: token.span(),
                    },
                    span: token.span(),
                })
            }
            TokenKind::Minus => {
                let start = self.bump().span(); // '-'
                let token = self.current();
                if token.kind() != TokenKind::Int {
                    self.record_error(ParseErrorKind::ExpectedIntegerLiteral, token.span());
                    return Err(());
                }
                let _ = self.bump();
                let span = self.join(start, token.span());
                Ok(Pattern::Int {
                    negative: true,
                    literal: Expr {
                        kind: ExprKind::Int,
                        span: token.span(),
                    },
                    span,
                })
            }
            _ => {
                self.record_error(ParseErrorKind::ExpectedIntegerLiteral, token.span());
                Err(())
            }
        }
    }

    /// Parses a single (non-or, non-range) match pattern (session 18):
    /// `_`, a name (binding), an enum variant path `E::V` (with an
    /// optional payload pattern), a boolean literal, or an (optionally
    /// negated) integer literal.
    fn parse_pattern_base(&mut self) -> Result<Pattern, ()> {
        let token = self.current();
        match token.kind() {
            // `_` is a reserved-word-like wildcard; the lexer produces it
            // as an identifier, so it is recognized by spelling.
            TokenKind::Ident if self.text(token.span()) == "_" => {
                let _ = self.bump();
                Ok(Pattern::Wildcard { span: token.span() })
            }
            TokenKind::Ident => {
                let _ = self.bump();
                let name = self.text(token.span());
                let ident = Ident {
                    name,
                    span: token.span(),
                };
                if self.current_kind() == TokenKind::ColonColon {
                    // `E::V`: an enum variant pattern. A missing variant
                    // name is the same `E-P22` the expression form reports.
                    let _ = self.bump(); // '::'
                    if self.current_kind() != TokenKind::Ident {
                        let token = self.current();
                        self.record_error(ParseErrorKind::ExpectedVariant, token.span());
                        return Err(());
                    }
                    let token = self.bump();
                    let variant = Ident {
                        name: self.text(token.span()),
                        span: token.span(),
                    };
                    // A following `(` makes the pattern a data-carrying
                    // variant pattern (session 19): `E::V(pattern)`. An
                    // empty payload `E::V()` is `E-P25`.
                    let mut payload = None;
                    if self.current_kind() == TokenKind::LParen {
                        let open = self.bump().span();
                        self.open_delims.push((TokenKind::LParen, open));
                        if self.current_kind() == TokenKind::RParen {
                            let bad = self.current();
                            self.record_error(ParseErrorKind::EmptyPayload, bad.span());
                            let _ = self.bump();
                            self.open_delims.pop();
                        } else {
                            match self.parse_pattern() {
                                Ok(inner) => {
                                    if self.current_kind() != TokenKind::RParen {
                                        let bad = self.current();
                                        self.record_error(
                                            ParseErrorKind::ExpectedRParen,
                                            bad.span(),
                                        );
                                        self.recover_pattern_payload();
                                    } else {
                                        let _ = self.bump();
                                        self.open_delims.pop();
                                        payload = Some(Box::new(inner));
                                    }
                                }
                                Err(()) => self.recover_pattern_payload(),
                            }
                        }
                    }
                    Ok(Pattern::EnumVariant {
                        name: ident,
                        variant,
                        payload,
                    })
                } else {
                    Ok(Pattern::Binding(ident))
                }
            }
            TokenKind::True | TokenKind::False => {
                let _ = self.bump();
                Ok(Pattern::Bool {
                    value: token.kind() == TokenKind::True,
                    span: token.span(),
                })
            }
            TokenKind::Int => {
                let _ = self.bump();
                Ok(Pattern::Int {
                    negative: false,
                    literal: Expr {
                        kind: ExprKind::Int,
                        span: token.span(),
                    },
                    span: token.span(),
                })
            }
            TokenKind::Minus => {
                let start = self.bump().span(); // '-'
                let token = self.current();
                if token.kind() != TokenKind::Int {
                    self.record_error(ParseErrorKind::ExpectedIntegerLiteral, token.span());
                    return Err(());
                }
                let _ = self.bump();
                let span = self.join(start, token.span());
                Ok(Pattern::Int {
                    negative: true,
                    literal: Expr {
                        kind: ExprKind::Int,
                        span: token.span(),
                    },
                    span,
                })
            }
            _ => {
                self.record_error(ParseErrorKind::ExpectedPattern, token.span());
                Err(())
            }
        }
    }

    /// Recovers from a malformed pattern payload: skips to the closing `)`
    /// (consuming it) or, failing that, to a match-arm boundary. Also pops
    /// the payload's `(` from the open-delimiter stack.
    fn recover_pattern_payload(&mut self) {
        while !matches!(
            self.current_kind(),
            TokenKind::RParen | TokenKind::Comma | TokenKind::RBrace | TokenKind::Eof
        ) {
            let _ = self.bump();
        }
        if self.current_kind() == TokenKind::RParen {
            let _ = self.bump();
        }
        self.open_delims.pop();
    }

    /// Skips tokens up to (but not consuming) the next match-arm boundary:
    /// `,`, `}`, or `Eof`. Used to recover from a malformed match arm.
    fn skip_to_arm_boundary(&mut self) {
        while !matches!(
            self.current_kind(),
            TokenKind::Comma | TokenKind::RBrace | TokenKind::Eof
        ) {
            if self.current_kind() == TokenKind::LBrace {
                self.skip_balanced_brace_group();
            } else {
                let _ = self.bump();
            }
        }
    }

    /// Parses a `{ ... }` block body; on a missing `{` records
    /// [`ParseErrorKind::ExpectedBlock`] and returns `Err`.
    fn parse_block_body(&mut self) -> Result<Block, ()> {
        if self.current_kind() != TokenKind::LBrace {
            let token = self.current();
            self.record_error(ParseErrorKind::ExpectedBlock, token.span());
            return Err(());
        }
        let open = self.bump().span();
        self.open_delims.push((TokenKind::LBrace, open));
        let mut stmts = Vec::new();
        loop {
            match self.current_kind() {
                TokenKind::RBrace => {
                    let close = self.bump().span();
                    self.open_delims.pop();
                    let span = self.join(open, close);
                    return Ok(Block { stmts, span });
                }
                TokenKind::Eof => {
                    self.report_unclosed(TokenKind::LBrace, ParseErrorKind::UnclosedBrace);
                    let span = Span::new(self.file.id(), open.start()..self.file.len());
                    return Ok(Block { stmts, span });
                }
                TokenKind::Semi => {
                    // Empty statement: consume without producing a node.
                    let _ = self.bump();
                }
                _ => match self.parse_statement() {
                    Ok(stmt) => stmts.push(stmt),
                    Err(()) => self.recover_statement(),
                },
            }
        }
    }

    /// Parses a block body or, when the block is missing, recovers and
    /// returns an empty placeholder block.
    fn parse_body_or_recover(&mut self) -> Block {
        match self.parse_block_body() {
            Ok(block) => block,
            Err(()) => {
                self.recover_statement();
                Block {
                    stmts: Vec::new(),
                    span: self.point_span(),
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    fn parse_expression(&mut self) -> Result<Expr, ()> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, ()> {
        let lhs = self.parse_range()?;
        let Some(op) = assign_op(self.current_kind()) else {
            return Ok(lhs);
        };
        let _ = self.bump();
        let value = self.parse_assignment()?;
        if !is_place(&lhs) {
            self.record_error(ParseErrorKind::ExpectedAssignmentTarget, lhs.span);
        }
        let span = self.join(lhs.span, value.span);
        Ok(Expr {
            kind: ExprKind::Assign {
                op,
                target: Box::new(lhs),
                value: Box::new(value),
            },
            span,
        })
    }

    fn parse_range(&mut self) -> Result<Expr, ()> {
        let start = self.parse_logical_or()?;
        let inclusive = match self.current_kind() {
            TokenKind::DotDot => false,
            TokenKind::DotDotEq => true,
            _ => return Ok(start),
        };
        let _ = self.bump();
        let end = self.parse_range()?;
        let span = self.join(start.span, end.span);
        Ok(Expr {
            kind: ExprKind::Range {
                inclusive,
                start: Box::new(start),
                end: Box::new(end),
            },
            span,
        })
    }

    /// Parses a left-associative binary level: one or more `next` operands
    /// joined by any of the operators in `ops`.
    fn parse_binary_level(
        &mut self,
        mut next: impl FnMut(&mut Parser<'a>) -> Result<Expr, ()>,
        ops: &[(TokenKind, BinaryOp)],
    ) -> Result<Expr, ()> {
        let mut lhs = next(self)?;
        while let Some((_, op)) = ops.iter().find(|(kind, _)| *kind == self.current_kind()) {
            let _ = self.bump();
            let rhs = next(self)?;
            let span = self.join(lhs.span, rhs.span);
            lhs = Expr {
                kind: ExprKind::Binary {
                    op: *op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_logical_or(&mut self) -> Result<Expr, ()> {
        self.parse_binary_level(
            |p| p.parse_logical_and(),
            &[(TokenKind::PipePipe, BinaryOp::Or)],
        )
    }

    fn parse_logical_and(&mut self) -> Result<Expr, ()> {
        self.parse_binary_level(|p| p.parse_bit_or(), &[(TokenKind::AmpAmp, BinaryOp::And)])
    }

    fn parse_bit_or(&mut self) -> Result<Expr, ()> {
        self.parse_binary_level(|p| p.parse_bit_xor(), &[(TokenKind::Pipe, BinaryOp::BitOr)])
    }

    fn parse_bit_xor(&mut self) -> Result<Expr, ()> {
        self.parse_binary_level(
            |p| p.parse_bit_and(),
            &[(TokenKind::Caret, BinaryOp::BitXor)],
        )
    }

    fn parse_bit_and(&mut self) -> Result<Expr, ()> {
        self.parse_binary_level(
            |p| p.parse_equality(),
            &[(TokenKind::Amp, BinaryOp::BitAnd)],
        )
    }

    fn parse_equality(&mut self) -> Result<Expr, ()> {
        self.parse_binary_level(
            |p| p.parse_relational(),
            &[
                (TokenKind::EqEq, BinaryOp::Eq),
                (TokenKind::NotEq, BinaryOp::Ne),
            ],
        )
    }

    fn parse_relational(&mut self) -> Result<Expr, ()> {
        self.parse_binary_level(
            |p| p.parse_shift(),
            &[
                (TokenKind::Lt, BinaryOp::Lt),
                (TokenKind::Le, BinaryOp::Le),
                (TokenKind::Gt, BinaryOp::Gt),
                (TokenKind::Ge, BinaryOp::Ge),
            ],
        )
    }

    fn parse_shift(&mut self) -> Result<Expr, ()> {
        self.parse_binary_level(
            |p| p.parse_additive(),
            &[
                (TokenKind::Shl, BinaryOp::Shl),
                (TokenKind::Shr, BinaryOp::Shr),
            ],
        )
    }

    fn parse_additive(&mut self) -> Result<Expr, ()> {
        self.parse_binary_level(
            |p| p.parse_multiplicative(),
            &[
                (TokenKind::Plus, BinaryOp::Add),
                (TokenKind::Minus, BinaryOp::Sub),
            ],
        )
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ()> {
        self.parse_binary_level(
            |p| p.parse_unary(),
            &[
                (TokenKind::Star, BinaryOp::Mul),
                (TokenKind::Slash, BinaryOp::Div),
                (TokenKind::Percent, BinaryOp::Rem),
            ],
        )
    }

    fn parse_unary(&mut self) -> Result<Expr, ()> {
        match self.current_kind() {
            TokenKind::Minus => {
                let start = self.bump().span();
                let operand = self.parse_unary()?;
                let span = self.join(start, operand.span);
                Ok(Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::Neg,
                        operand: Box::new(operand),
                    },
                    span,
                })
            }
            TokenKind::Bang => {
                let start = self.bump().span();
                let operand = self.parse_unary()?;
                let span = self.join(start, operand.span);
                Ok(Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::Not,
                        operand: Box::new(operand),
                    },
                    span,
                })
            }
            TokenKind::Tilde => {
                let start = self.bump().span();
                let operand = self.parse_unary()?;
                let span = self.join(start, operand.span);
                Ok(Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::BitNot,
                        operand: Box::new(operand),
                    },
                    span,
                })
            }
            TokenKind::Amp => {
                // `&expr` (shared borrow) or `&mut expr` (exclusive
                // borrow); `&` in binary position remains bitwise-and.
                let start = self.bump().span();
                let mutable = if self.current_kind() == TokenKind::Mut {
                    let _ = self.bump();
                    true
                } else {
                    false
                };
                let operand = self.parse_unary()?;
                let span = self.join(start, operand.span);
                Ok(Expr {
                    kind: ExprKind::Borrow {
                        mutable,
                        operand: Box::new(operand),
                    },
                    span,
                })
            }
            TokenKind::Star => {
                // `*expr`: dereference a reference (session 16); `*` in
                // binary position remains multiplication.
                let start = self.bump().span();
                let operand = self.parse_unary()?;
                let span = self.join(start, operand.span);
                Ok(Expr {
                    kind: ExprKind::Deref {
                        operand: Box::new(operand),
                    },
                    span,
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ()> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.current_kind() {
                TokenKind::LParen => {
                    let callee = expr;
                    let (args, close) = self.parse_args()?;
                    let span = self.join(callee.span, close);
                    expr = Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(callee),
                            args,
                        },
                        span,
                    };
                }
                TokenKind::Dot => {
                    let _ = self.bump();
                    let base = expr;
                    let member = self.expect_ident()?;
                    let span = self.join(base.span, member.span);
                    expr = Expr {
                        kind: ExprKind::Member {
                            base: Box::new(base),
                            member,
                        },
                        span,
                    };
                }
                TokenKind::LBracket => {
                    let open = self.bump().span();
                    self.open_delims.push((TokenKind::LBracket, open));
                    let base = expr;
                    let index = self.parse_expression()?;
                    let close = self.expect_bracket_close()?;
                    let span = self.join(base.span, close);
                    expr = Expr {
                        kind: ExprKind::Index {
                            base: Box::new(base),
                            index: Box::new(index),
                        },
                        span,
                    };
                }
                _ => return Ok(expr),
            }
        }
    }

    /// Parses a call argument list `( ... )`, including a trailing comma.
    fn parse_args(&mut self) -> Result<(Vec<Expr>, Span), ()> {
        let open = self.bump().span(); // '('
        self.open_delims.push((TokenKind::LParen, open));
        let mut args = Vec::new();
        if self.current_kind() == TokenKind::RParen {
            let close = self.bump().span();
            self.open_delims.pop();
            return Ok((args, close));
        }
        loop {
            // End of input after (or instead of) an argument means the
            // argument list itself is unclosed — mirroring the parameter-list
            // check, this reports `UnclosedParen` rather than a generic
            // `UnexpectedEof` on the next expression.
            if self.current_kind() == TokenKind::Eof {
                self.report_unclosed(TokenKind::LParen, ParseErrorKind::UnclosedParen);
                return Err(());
            }
            args.push(self.parse_expression()?);
            match self.current_kind() {
                TokenKind::Comma => {
                    let _ = self.bump();
                    if self.current_kind() == TokenKind::RParen {
                        break;
                    }
                }
                TokenKind::RParen => break,
                TokenKind::Eof => {
                    self.report_unclosed(TokenKind::LParen, ParseErrorKind::UnclosedParen);
                    return Err(());
                }
                _ => {
                    let token = self.current();
                    self.record_error(ParseErrorKind::ExpectedComma, token.span());
                    // Skip the offending tokens up to the list terminator or a
                    // statement boundary (`;`/`}`), so a stray `;` or `}` does
                    // not swallow the enclosing block.
                    while !matches!(
                        self.current_kind(),
                        TokenKind::Comma
                            | TokenKind::RParen
                            | TokenKind::Semi
                            | TokenKind::RBrace
                            | TokenKind::Eof
                    ) {
                        let _ = self.bump();
                    }
                    match self.current_kind() {
                        TokenKind::Comma => {
                            let _ = self.bump();
                            // A recovered comma before `)` is a trailing comma:
                            // finish the list without a second error.
                            if self.current_kind() == TokenKind::RParen {
                                break;
                            }
                        }
                        TokenKind::RParen => break,
                        TokenKind::Eof => {
                            self.report_unclosed(TokenKind::LParen, ParseErrorKind::UnclosedParen);
                            return Err(());
                        }
                        // Stopped at `;` or `}`: leave the statement-boundary
                        // recovery to the caller.
                        _ => return Err(()),
                    }
                }
            }
        }
        let close = self.bump().span(); // ')'
        self.open_delims.pop();
        Ok((args, close))
    }

    /// Consumes the closing `)` of a parenthesized group, recovering on
    /// mismatch and reporting [`ParseErrorKind::UnclosedParen`] at `Eof`.
    fn expect_paren_close(&mut self) -> Result<Span, ()> {
        if self.current_kind() == TokenKind::RParen {
            let close = self.bump().span();
            self.open_delims.pop();
            return Ok(close);
        }
        if self.current_kind() == TokenKind::Eof {
            self.report_unclosed(TokenKind::LParen, ParseErrorKind::UnclosedParen);
            return Err(());
        }
        let token = self.current();
        self.record_error(ParseErrorKind::ExpectedRParen, token.span());
        while !matches!(
            self.current_kind(),
            TokenKind::RParen | TokenKind::Semi | TokenKind::RBrace | TokenKind::Eof
        ) {
            let _ = self.bump();
        }
        if self.current_kind() == TokenKind::RParen {
            let close = self.bump().span();
            self.open_delims.pop();
            return Ok(close);
        }
        Err(())
    }

    /// Consumes the closing `]` of an index expression, mirroring
    /// [`Parser::expect_paren_close`] with bracket-specific errors.
    fn expect_bracket_close(&mut self) -> Result<Span, ()> {
        if self.current_kind() == TokenKind::RBracket {
            let close = self.bump().span();
            self.open_delims.pop();
            return Ok(close);
        }
        if self.current_kind() == TokenKind::Eof {
            self.report_unclosed(TokenKind::LBracket, ParseErrorKind::UnclosedBracket);
            return Err(());
        }
        let token = self.current();
        self.record_error(ParseErrorKind::ExpectedRBracket, token.span());
        while !matches!(
            self.current_kind(),
            TokenKind::RBracket | TokenKind::Semi | TokenKind::RBrace | TokenKind::Eof
        ) {
            let _ = self.bump();
        }
        if self.current_kind() == TokenKind::RBracket {
            let close = self.bump().span();
            self.open_delims.pop();
            return Ok(close);
        }
        Err(())
    }

    fn parse_primary(&mut self) -> Result<Expr, ()> {
        let token = self.current();
        let span = token.span();
        match token.kind() {
            TokenKind::Int => {
                let _ = self.bump();
                Ok(Expr {
                    kind: ExprKind::Int,
                    span,
                })
            }
            TokenKind::Float => {
                let _ = self.bump();
                Ok(Expr {
                    kind: ExprKind::Float,
                    span,
                })
            }
            TokenKind::Str => {
                let _ = self.bump();
                Ok(Expr {
                    kind: ExprKind::Str,
                    span,
                })
            }
            TokenKind::Char => {
                let _ = self.bump();
                Ok(Expr {
                    kind: ExprKind::Char,
                    span,
                })
            }
            TokenKind::True => {
                let _ = self.bump();
                Ok(Expr {
                    kind: ExprKind::Bool(true),
                    span,
                })
            }
            TokenKind::False => {
                let _ = self.bump();
                Ok(Expr {
                    kind: ExprKind::Bool(false),
                    span,
                })
            }
            TokenKind::Null => {
                let _ = self.bump();
                Ok(Expr {
                    kind: ExprKind::Null,
                    span,
                })
            }
            TokenKind::Ident => {
                let _ = self.bump();
                let name = self.text(span);
                let ident = Ident { name, span };
                // `Name::Variant` is an enum variant reference (session 17):
                // a path whose first segment names an enum type and whose
                // second names one of its variants. `::` was previously
                // lexed but rejected; it now forms variant paths only —
                // module paths (`mod::item`) remain a later milestone.
                if self.current_kind() == TokenKind::ColonColon {
                    self.parse_enum_variant(ident)
                } else if !self.in_block_context && self.current_kind() == TokenKind::LBrace {
                    // `Name { ... }` is a struct literal unless the
                    // expression sits directly before a block in the
                    // grammar (`if`/`while` conditions, `for` iterables),
                    // where the `{` opens the block instead.
                    self.parse_struct_literal(ident)
                } else {
                    Ok(Expr {
                        kind: ExprKind::Ident(ident),
                        span,
                    })
                }
            }
            TokenKind::LBracket => self.parse_array_literal(),
            TokenKind::LParen => {
                let open = self.bump().span();
                self.open_delims.push((TokenKind::LParen, open));
                // A parenthesized group is a self-contained expression:
                // struct literals inside it are enabled even in a block
                // context (`if (Point { x: 1 }) { }`).
                let saved = self.in_block_context;
                self.in_block_context = false;
                let inner = self.parse_expression()?;
                self.in_block_context = saved;
                let close = self.expect_paren_close()?;
                let span = self.join(open, close);
                Ok(Expr {
                    kind: ExprKind::Group(Box::new(inner)),
                    span,
                })
            }
            TokenKind::Eof => {
                self.record_error(ParseErrorKind::UnexpectedEof, span);
                Err(())
            }
            _ => {
                self.record_error(ParseErrorKind::ExpectedExpression, span);
                Err(())
            }
        }
    }

    /// Parses an enum variant reference or construction
    /// `Name::Variant` after the enum name token. The variant must be a
    /// bare identifier; a missing variant is `E-P22`. A following `(` makes
    /// the expression a data-carrying construction (session 19):
    /// `Name::Variant(payload)`; the payload is parsed here (not as a call)
    /// because `E::V` is unambiguously a variant path, so `E::V(x)` can
    /// never be a function call. An empty payload `Variant()` is `E-P25`.
    fn parse_enum_variant(&mut self, name: Ident) -> Result<Expr, ()> {
        let _ = self.bump(); // '::'
        if self.current_kind() != TokenKind::Ident {
            let token = self.current();
            self.record_error(ParseErrorKind::ExpectedVariant, token.span());
            return Err(());
        }
        let token = self.bump();
        let variant = Ident {
            name: self.text(token.span()),
            span: token.span(),
        };
        let mut span = self.join(name.span, variant.span);
        let mut payload = None;
        if self.current_kind() == TokenKind::LParen {
            let open = self.bump().span();
            self.open_delims.push((TokenKind::LParen, open));
            // `E::V()` carries no payload (E-P25).
            if self.current_kind() == TokenKind::RParen {
                let bad = self.current();
                self.record_error(ParseErrorKind::EmptyPayload, bad.span());
                let _ = self.bump();
                self.open_delims.pop();
            } else {
                let expr = match self.parse_expression() {
                    Ok(expr) => expr,
                    Err(()) => {
                        // An error is already recorded; recover to `)` or a
                        // statement boundary so a stray `;`/`}` does not
                        // swallow the enclosing block.
                        self.recover_construction_payload();
                        return Ok(Expr {
                            kind: ExprKind::EnumVariant {
                                name: Box::new(name),
                                variant: Box::new(variant),
                                payload,
                            },
                            span,
                        });
                    }
                };
                if self.current_kind() != TokenKind::RParen {
                    let bad = self.current();
                    self.record_error(ParseErrorKind::ExpectedRParen, bad.span());
                    self.recover_construction_payload();
                } else {
                    let close = self.bump().span();
                    self.open_delims.pop();
                    span = self.join(name.span, close);
                    payload = Some(Box::new(expr));
                }
            }
        }
        Ok(Expr {
            kind: ExprKind::EnumVariant {
                name: Box::new(name),
                variant: Box::new(variant),
                payload,
            },
            span,
        })
    }

    /// Recovers from a malformed construction payload: skips to the closing
    /// `)` (consuming it) or, failing that, to a statement boundary. Also
    /// pops the payload's `(` from the open-delimiter stack.
    fn recover_construction_payload(&mut self) {
        while !matches!(
            self.current_kind(),
            TokenKind::RParen | TokenKind::Semi | TokenKind::RBrace | TokenKind::Eof
        ) {
            let _ = self.bump();
        }
        if self.current_kind() == TokenKind::RParen {
            let _ = self.bump();
        }
        self.open_delims.pop();
    }

    /// Parses a struct literal `Name { field: value, ... }` after the name
    /// token, including a trailing comma.
    fn parse_struct_literal(&mut self, name: Ident) -> Result<Expr, ()> {
        let open = self.bump().span(); // '{'
        self.open_delims.push((TokenKind::LBrace, open));
        let mut fields = Vec::new();
        if self.current_kind() == TokenKind::RBrace {
            let close = self.bump().span();
            self.open_delims.pop();
            let span = self.join(name.span, close);
            return Ok(Expr {
                kind: ExprKind::StructLit { name, fields },
                span,
            });
        }
        loop {
            if self.current_kind() == TokenKind::Eof {
                self.report_unclosed(TokenKind::LBrace, ParseErrorKind::UnclosedBrace);
                return Err(());
            }
            match self.parse_struct_field_init() {
                Ok(field) => fields.push(field),
                Err(()) => self.skip_to_field_boundary(),
            }
            match self.current_kind() {
                TokenKind::Comma => {
                    let _ = self.bump();
                    if self.current_kind() == TokenKind::RBrace {
                        break;
                    }
                }
                TokenKind::RBrace => break,
                TokenKind::Eof => {
                    self.report_unclosed(TokenKind::LBrace, ParseErrorKind::UnclosedBrace);
                    return Err(());
                }
                _ => {
                    let token = self.current();
                    self.record_error(ParseErrorKind::ExpectedComma, token.span());
                    self.skip_to_field_boundary();
                    if self.current_kind() == TokenKind::Comma {
                        let _ = self.bump();
                    }
                }
            }
        }
        let close = self.bump().span();
        self.open_delims.pop();
        let span = self.join(name.span, close);
        Ok(Expr {
            kind: ExprKind::StructLit { name, fields },
            span,
        })
    }

    /// Parses one `name: value` struct-literal initializer.
    fn parse_struct_field_init(&mut self) -> Result<StructFieldInit, ()> {
        let name = self.expect_ident()?;
        if self.current_kind() != TokenKind::Colon {
            let token = self.current();
            self.record_error(ParseErrorKind::ExpectedColon, token.span());
            return Err(());
        }
        let _ = self.bump(); // ':'
        let value = self.parse_expression()?;
        let span = self.join(name.span, value.span);
        Ok(StructFieldInit { name, value, span })
    }

    /// Parses an array literal `[elem, ...]`, including a trailing comma.
    fn parse_array_literal(&mut self) -> Result<Expr, ()> {
        let open = self.bump().span(); // '['
        self.open_delims.push((TokenKind::LBracket, open));
        let mut elems = Vec::new();
        if self.current_kind() == TokenKind::RBracket {
            let close = self.bump().span();
            self.open_delims.pop();
            let span = self.join(open, close);
            return Ok(Expr {
                kind: ExprKind::ArrayLit(elems),
                span,
            });
        }
        loop {
            if self.current_kind() == TokenKind::Eof {
                self.report_unclosed(TokenKind::LBracket, ParseErrorKind::UnclosedBracket);
                return Err(());
            }
            elems.push(self.parse_expression()?);
            match self.current_kind() {
                TokenKind::Comma => {
                    let _ = self.bump();
                    if self.current_kind() == TokenKind::RBracket {
                        break;
                    }
                }
                TokenKind::RBracket => break,
                TokenKind::Eof => {
                    self.report_unclosed(TokenKind::LBracket, ParseErrorKind::UnclosedBracket);
                    return Err(());
                }
                _ => {
                    let token = self.current();
                    self.record_error(ParseErrorKind::ExpectedComma, token.span());
                    while !matches!(
                        self.current_kind(),
                        TokenKind::Comma | TokenKind::RBracket | TokenKind::Eof
                    ) {
                        let _ = self.bump();
                    }
                    if self.current_kind() == TokenKind::Comma {
                        let _ = self.bump();
                    }
                }
            }
        }
        let close = self.bump().span();
        self.open_delims.pop();
        let span = self.join(open, close);
        Ok(Expr {
            kind: ExprKind::ArrayLit(elems),
            span,
        })
    }

    /// Consumes an identifier, recording [`ParseErrorKind::ExpectedIdentifier`]
    /// and returning `Err` otherwise.
    fn expect_ident(&mut self) -> Result<Ident, ()> {
        let token = self.current();
        if token.kind() == TokenKind::Ident {
            let _ = self.bump();
            let name = self.text(token.span());
            return Ok(Ident {
                name,
                span: token.span(),
            });
        }
        self.record_error(ParseErrorKind::ExpectedIdentifier, token.span());
        Err(())
    }

    /// Consumes `(`, recording [`ParseErrorKind::ExpectedLParen`] and
    /// returning `Err` otherwise.
    fn expect_lparen(&mut self) -> Result<Span, ()> {
        if self.current_kind() == TokenKind::LParen {
            let span = self.bump().span();
            self.open_delims.push((TokenKind::LParen, span));
            return Ok(span);
        }
        let token = self.current();
        self.record_error(ParseErrorKind::ExpectedLParen, token.span());
        Err(())
    }

    /// Consumes `;`, recovering on mismatch and reporting
    /// [`ParseErrorKind::ExpectedSemicolon`].
    fn expect_semi(&mut self) -> Result<Span, ()> {
        if self.current_kind() == TokenKind::Semi {
            let span = self.bump().span();
            return Ok(span);
        }
        let token = self.current();
        self.record_error(ParseErrorKind::ExpectedSemicolon, token.span());
        while !matches!(
            self.current_kind(),
            TokenKind::Semi | TokenKind::RBrace | TokenKind::Eof
        ) {
            let _ = self.bump();
        }
        if self.current_kind() == TokenKind::Semi {
            return Ok(self.bump().span());
        }
        Err(())
    }
}

/// Maps an assignment operator token to its [`AssignOp`].
fn assign_op(kind: TokenKind) -> Option<AssignOp> {
    match kind {
        TokenKind::Eq => Some(AssignOp::Assign),
        TokenKind::PlusEq => Some(AssignOp::AddAssign),
        TokenKind::MinusEq => Some(AssignOp::SubAssign),
        TokenKind::StarEq => Some(AssignOp::MulAssign),
        TokenKind::SlashEq => Some(AssignOp::DivAssign),
        TokenKind::PercentEq => Some(AssignOp::RemAssign),
        _ => None,
    }
}

/// Whether `expr` is a valid assignment target (a place expression).
/// `*r` (session 16) is a place: the storage addressed by reference `r`.
fn is_place(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::Ident(_)
            | ExprKind::Member { .. }
            | ExprKind::Index { .. }
            | ExprKind::Deref { .. }
    )
}
