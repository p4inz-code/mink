//! MINK abstract syntax tree (AST).
//!
//! The AST is the typed syntax representation produced by the
//! [`parser`](crate::parser) and consumed by later compiler stages (name
//! resolution, type checking, semantic analysis, HIR lowering) and by
//! tooling (diagnostics, formatter, LSP). Every node carries the exact
//! source [`Span`](crate::source::Span) it was parsed from.
//!
//! Design notes:
//!
//! - Nodes are plain data structures with public fields so that later stages
//!   can pattern-match exhaustively over the grammar.
//! - Identifiers carry their name text (small, and the currency of name
//!   resolution); literal values are **not** decoded into the AST. Literal
//!   tokens are represented by their [`ExprKind`] plus the expression's
//!   span, and the raw source text is recovered through
//!   [`SourceFile::span_text`](crate::source::SourceFile::span_text). This
//!   avoids duplicating (possibly large) source strings inside the tree;
//!   literal decoding belongs to a later milestone.
//! - The AST is `Clone` + `PartialEq` + `Eq` so tests can assert exact tree
//!   structure.
//!
//! The frozen grammar this AST mirrors is documented in
//! `docs/language/CORE_GRAMMAR.md`; parser architecture and decisions are in
//! `docs/implementation/PARSER_IMPLEMENTATION.md`.

use crate::source::Span;

/// A parsed MINK source file: an ordered list of top-level [`Item`]s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ast {
    /// The top-level items, in source order.
    pub items: Vec<Item>,
}

impl Ast {
    /// Creates a program from `items`.
    pub fn new(items: Vec<Item>) -> Self {
        Self { items }
    }

    /// The top-level items, in source order.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Whether the program contains no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// A top-level declaration: a function, a `let` binding, or a `const`
/// binding.
///
/// The grammar currently allows only declarations at module scope; executable
/// statements live inside function bodies (see
/// `docs/language/CORE_GRAMMAR.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The kind of declaration.
    pub kind: ItemKind,
    /// Span covering the whole item, from its leading keyword through its
    /// trailing terminator or closing brace.
    pub span: Span,
}

/// The kind of a top-level [`Item`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    /// A `fn` function declaration.
    Fn(FnItem),
    /// A `let` binding.
    Let(LetItem),
    /// A `const` binding.
    Const(ConstItem),
}

/// A `fn` function declaration: `fn name(params) { body }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnItem {
    /// The function's name.
    pub name: Ident,
    /// The declared parameters.
    pub params: Vec<Param>,
    /// The function body block.
    pub body: Block,
}

/// A single function parameter.
///
/// The frozen grammar has bare identifier parameters; type annotations are
/// deferred to the type-system milestone (see
/// `docs/implementation/PARSER_IMPLEMENTATION.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    /// The parameter's name.
    pub name: Ident,
    /// Span of the parameter (its identifier).
    pub span: Span,
}

/// A `let` binding: `let [mut] name = init;`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetItem {
    /// The bound name.
    pub name: Ident,
    /// Whether the binding is mutable (`let mut`).
    pub mutable: bool,
    /// The initializer expression.
    pub init: Expr,
}

/// A `const` binding: `const name = init;`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstItem {
    /// The bound name.
    pub name: Ident,
    /// The compile-time constant expression.
    pub init: Expr,
}

/// A name: its source spelling plus the span of the identifier token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    /// The identifier's exact source spelling.
    pub name: String,
    /// Span of the identifier token.
    pub span: Span,
}

/// A `{ ... }` block: an ordered list of statements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The statements, in source order.
    pub stmts: Vec<Stmt>,
    /// Span covering the whole block including its braces.
    pub span: Span,
}

/// A single statement inside a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stmt {
    /// The kind of statement.
    pub kind: StmtKind,
    /// Span covering the whole statement.
    pub span: Span,
}

/// The kind of a [`Stmt`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StmtKind {
    /// A `let` binding.
    Let(LetItem),
    /// A `const` binding.
    Const(ConstItem),
    /// `return;` or `return expr;`.
    Return(Option<Expr>),
    /// `break;`.
    Break,
    /// `continue;`.
    Continue,
    /// An `if` / `else if` / `else` statement.
    If(IfStmt),
    /// A `while` loop: `while cond { body }`.
    While {
        /// The loop condition.
        cond: Expr,
        /// The loop body.
        body: Block,
    },
    /// A `for` loop: `for name in iterable { body }`.
    For {
        /// The loop variable.
        name: Ident,
        /// The iterated expression.
        iterable: Expr,
        /// The loop body.
        body: Block,
    },
    /// An unconditional `loop { body }`.
    Loop(Block),
    /// An expression evaluated for its side effects, followed by `;`.
    Expr(Expr),
}

/// An `if` statement with an optional `else` branch.
///
/// The optional `else` branch is either a block or a further `else if`
/// chain, represented by [`ElseBranch`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfStmt {
    /// The condition expression.
    pub cond: Expr,
    /// The block executed when the condition is true.
    pub then_block: Block,
    /// The optional `else` branch.
    pub else_branch: Option<ElseBranch>,
    /// Span covering the whole `if` statement including any `else` branch.
    pub span: Span,
}

/// The `else` branch of an [`IfStmt`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElseBranch {
    /// An `else if` chain: the nested statement's own `else` is inside it.
    If(Box<IfStmt>),
    /// A plain `else { ... }` block.
    Block(Block),
}

/// An expression: a [`ExprKind`] plus the exact source span it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expr {
    /// The kind of expression.
    pub kind: ExprKind,
    /// Span covering the whole expression.
    pub span: Span,
}

/// The kind of an [`Expr`].
///
/// Literal expressions (`Int`, `Float`, `Str`, `Char`) carry no decoded
/// value: the raw source text is recovered from the expression's span (see
/// the module documentation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprKind {
    /// An integer literal.
    Int,
    /// A floating-point literal.
    Float,
    /// A string literal.
    Str,
    /// A character literal.
    Char,
    /// The boolean literal `true` or `false`.
    Bool(bool),
    /// The `null` literal.
    Null,
    /// A name reference.
    Ident(Ident),
    /// A prefix unary operation: `-x`, `!x`, `~x`.
    Unary {
        /// The operator.
        op: UnaryOp,
        /// The operand.
        operand: Box<Expr>,
    },
    /// An infix binary operation.
    Binary {
        /// The operator.
        op: BinaryOp,
        /// The left operand.
        lhs: Box<Expr>,
        /// The right operand.
        rhs: Box<Expr>,
    },
    /// An assignment: `target = value`, `target += value`, etc.
    Assign {
        /// The assignment operator.
        op: AssignOp,
        /// The assignment target (a place expression).
        target: Box<Expr>,
        /// The assigned value.
        value: Box<Expr>,
    },
    /// A range construction: `start..end` or `start..=end`.
    Range {
        /// Whether the range is inclusive (`..=`).
        inclusive: bool,
        /// The range start.
        start: Box<Expr>,
        /// The range end.
        end: Box<Expr>,
    },
    /// A function call: `callee(args)`.
    Call {
        /// The called expression.
        callee: Box<Expr>,
        /// The arguments, in source order.
        args: Vec<Expr>,
    },
    /// A member access: `base.member`.
    Member {
        /// The accessed base expression.
        base: Box<Expr>,
        /// The member name.
        member: Ident,
    },
    /// An index expression: `base[index]`.
    Index {
        /// The indexed base expression.
        base: Box<Expr>,
        /// The index expression.
        index: Box<Expr>,
    },
    /// A parenthesized expression: `(inner)`. Kept as a node so tooling can
    /// distinguish explicit grouping from parser-imposed association.
    Group(Box<Expr>),
}

/// A prefix unary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// Arithmetic negation `-x`.
    Neg,
    /// Logical negation `!x`.
    Not,
    /// Bitwise complement `~x`.
    BitNot,
}

impl UnaryOp {
    /// The source spelling of this operator.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Neg => "-",
            Self::Not => "!",
            Self::BitNot => "~",
        }
    }
}

/// An infix binary operator, ordered by decreasing precedence (see
/// `docs/language/CORE_GRAMMAR.md` for the full table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Rem,
    /// `<<`
    Shl,
    /// `>>`
    Shr,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `&`
    BitAnd,
    /// `^`
    BitXor,
    /// `|`
    BitOr,
    /// `&&`
    And,
    /// `||`
    Or,
}

impl BinaryOp {
    /// The source spelling of this operator.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Rem => "%",
            Self::Shl => "<<",
            Self::Shr => ">>",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::BitAnd => "&",
            Self::BitXor => "^",
            Self::BitOr => "|",
            Self::And => "&&",
            Self::Or => "||",
        }
    }
}

/// An assignment operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssignOp {
    /// `=`
    Assign,
    /// `+=`
    AddAssign,
    /// `-=`
    SubAssign,
    /// `*=`
    MulAssign,
    /// `/=`
    DivAssign,
    /// `%=`
    RemAssign,
}

impl AssignOp {
    /// The source spelling of this operator.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Assign => "=",
            Self::AddAssign => "+=",
            Self::SubAssign => "-=",
            Self::MulAssign => "*=",
            Self::DivAssign => "/=",
            Self::RemAssign => "%=",
        }
    }
}
