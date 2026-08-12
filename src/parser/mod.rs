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
    AssignOp, Ast, BinaryOp, Block, ConstItem, ElseBranch, Expr, ExprKind, FnItem, Ident, IfStmt,
    Item, ItemKind, LetItem, Param, Stmt, StmtKind, UnaryOp,
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
            TokenKind::Fn | TokenKind::Let | TokenKind::Const | TokenKind::Eof
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
                TokenKind::Fn | TokenKind::Let | TokenKind::Const => match self.parse_item() {
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

    fn parse_fn(&mut self) -> Result<(FnItem, Span), ()> {
        let start = self.bump().span(); // 'fn'
        let name = self.expect_ident()?;
        self.expect_lparen()?;
        let params = self.parse_params()?;
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
        Ok((FnItem { name, params, body }, span))
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
                init: binding.init,
            },
            span,
        ))
    }

    /// Parses `name = expr ;` shared by `let` and `const` bindings.
    fn parse_binding_tail(&mut self, start: Span, mutable: bool) -> Result<(LetItem, Span), ()> {
        let name = self.expect_ident()?;
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
        let span = name.span;
        Ok(Param { name, span })
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
        let cond = self.parse_expression()?;
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
        let cond = self.parse_expression()?;
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
        let iterable = self.parse_expression()?;
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
        let (op, start) = match self.current_kind() {
            TokenKind::Minus => (UnaryOp::Neg, self.bump().span()),
            TokenKind::Bang => (UnaryOp::Not, self.bump().span()),
            TokenKind::Tilde => (UnaryOp::BitNot, self.bump().span()),
            _ => return self.parse_postfix(),
        };
        let operand = self.parse_unary()?;
        let span = self.join(start, operand.span);
        Ok(Expr {
            kind: ExprKind::Unary {
                op,
                operand: Box::new(operand),
            },
            span,
        })
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
                Ok(Expr {
                    kind: ExprKind::Ident(Ident { name, span }),
                    span,
                })
            }
            TokenKind::LParen => {
                let open = self.bump().span();
                self.open_delims.push((TokenKind::LParen, open));
                let inner = self.parse_expression()?;
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
fn is_place(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::Ident(_) | ExprKind::Member { .. } | ExprKind::Index { .. }
    )
}
