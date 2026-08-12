//! HIR: the typed, lowered high-level intermediate representation.
//!
//! HIR is the first compiler IR layer, produced by lowering the [`Ast`] with
//! the session-05 [`SemanticResult`] and the session-06/07 [`TypeResult`]
//! (see [`lower`]). It is the durable, owned representation later compiler
//! stages (MIR lowering, code generation, tooling) consume:
//!
//! - every node preserves its exact source [`Span`](crate::source::Span);
//! - identifiers are **resolved**: references and declaration names carry
//!   their [`SymbolId`] from the semantic result — lowering never re-runs
//!   name resolution;
//! - every expression and binding carries its **canonical** type as a
//!   [`TypeId`] from the type result — lowering never re-runs type
//!   analysis, and inference variables are resolved to the type they
//!   denote;
//! - syntax-only `Group` nodes are eliminated (a parenthesized expression
//!   lowers to its inner node, keeping the parentheses' span);
//! - the HIR owns all of its data (names are copied strings; no references
//!   into the AST or the results) and carries its own cloned [`TypeTable`],
//!   so it is fully self-contained once produced.
//!
//! Control flow is represented explicitly — nested blocks, if/else
//! branches, while/for/loop, break/continue, and returns — in the shape
//! MIR lowering (session 09) consumes (see
//! `docs/implementation/MIR_IMPLEMENTATION.md`).
//!
//! Lowering is deterministic: nodes and errors are produced in source
//! order. For a valid front end (clean semantic and type analysis)
//! lowering always succeeds; failures are reported as structured
//! [`HirError`]s (`E-H01`…`E-H03`) instead of panicking (see
//! `docs/implementation/HIR_IMPLEMENTATION.md`).
//!
//! The pipeline continues from type analysis:
//!
//! ```text
//! AST → semantic analysis → type analysis → HIR lowering → MIR lowering
//! ```

mod error;
mod lower;

use crate::ast::{AssignOp, BinaryOp, UnaryOp};
use crate::semantics::SymbolId;
use crate::source::Span;
use crate::typecheck::{TypeId, TypeTable};

pub use error::{HirError, HirErrorKind};
pub use lower::lower;

/// A lowered MINK program: typed, symbol-resolved items in source order,
/// plus the type table its [`TypeId`]s refer to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirProgram {
    /// The top-level items, in source order.
    pub items: Vec<HirItem>,
    /// The type table backing every [`TypeId`] in this program. It is a
    /// clone of the type-analysis table, so the HIR is self-contained:
    /// types can be canonicalized, compared, and rendered without the
    /// original [`TypeResult`](crate::typecheck::TypeResult).
    pub types: TypeTable,
}

/// A top-level declaration: a function, a `let` binding, or a `const`
/// binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirItem {
    /// The kind of declaration.
    pub kind: HirItemKind,
    /// Span covering the whole item.
    pub span: Span,
}

/// The kind of a top-level [`HirItem`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirItemKind {
    /// A `fn` function declaration.
    Fn(HirFn),
    /// A `let` binding.
    Let(HirLet),
    /// A `const` binding.
    Const(HirConst),
}

/// A lowered function declaration, resolved to its symbol and typed with
/// its `Fn` type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirFn {
    /// The function's name, resolved to its symbol.
    pub name: HirIdent,
    /// The parameters, in declaration order, each resolved to its symbol.
    pub params: Vec<HirParam>,
    /// The function body.
    pub body: HirBlock,
    /// Span covering the whole `fn` item.
    pub span: Span,
    /// The function's `Fn` type.
    pub ty: TypeId,
}

/// A lowered function parameter, resolved to its symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirParam {
    /// The parameter's name and symbol.
    pub name: HirIdent,
    /// Span of the parameter identifier.
    pub span: Span,
    /// The parameter's type.
    pub ty: TypeId,
}

/// A lowered `let` binding with its initializer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirLet {
    /// The bound name and symbol.
    pub name: HirIdent,
    /// Whether the binding is mutable (`let mut`).
    pub mutable: bool,
    /// The initializer expression.
    pub init: Box<HirExpr>,
    /// Span of the whole `let` statement/item.
    pub span: Span,
    /// The binding's type.
    pub ty: TypeId,
}

/// A lowered `const` binding with its initializer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirConst {
    /// The bound name and symbol.
    pub name: HirIdent,
    /// The initializer expression.
    pub init: Box<HirExpr>,
    /// Span of the whole `const` statement/item.
    pub span: Span,
    /// The binding's type.
    pub ty: TypeId,
}

/// A resolved identifier: its source spelling, exact span, the symbol it
/// refers to, and the symbol's (canonical) type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirIdent {
    /// The identifier's exact source spelling.
    pub name: String,
    /// Span of the identifier token.
    pub span: Span,
    /// The symbol this identifier refers to.
    pub symbol: SymbolId,
    /// The symbol's canonical type.
    pub ty: TypeId,
}

/// A plain name that is not a symbol reference (for example a member name;
/// member symbols arrive with user-defined types in a later milestone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirName {
    /// The name's exact source spelling.
    pub name: String,
    /// Span of the name token.
    pub span: Span,
}

/// A lowered statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirStmt {
    /// The kind of statement.
    pub kind: HirStmtKind,
    /// Span covering the whole statement.
    pub span: Span,
}

/// The kind of a [`HirStmt`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirStmtKind {
    /// A `let` binding.
    Let(HirLet),
    /// A `const` binding.
    Const(HirConst),
    /// `return;` or `return expr;`.
    Return(Option<HirExpr>),
    /// `break;`.
    Break,
    /// `continue;`.
    Continue,
    /// An `if` / `else if` / `else` statement.
    If(HirIf),
    /// A `while` loop.
    While {
        /// The loop condition.
        cond: HirExpr,
        /// The loop body.
        body: HirBlock,
    },
    /// A `for` loop.
    For {
        /// The loop variable, resolved to its symbol.
        var: HirIdent,
        /// The iterated expression.
        iterable: HirExpr,
        /// The loop body.
        body: HirBlock,
    },
    /// An unconditional `loop { body }`.
    Loop(HirBlock),
    /// An expression evaluated for its side effects.
    Expr(HirExpr),
}

/// A lowered `{ ... }` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirBlock {
    /// The statements, in source order.
    pub stmts: Vec<HirStmt>,
    /// Span covering the whole block including its braces.
    pub span: Span,
}

/// A lowered `if` statement with an optional `else` branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirIf {
    /// The condition expression.
    pub cond: HirExpr,
    /// The block executed when the condition is true.
    pub then_block: HirBlock,
    /// The optional `else` branch.
    pub else_branch: Option<HirElseBranch>,
    /// Span covering the whole `if` statement including any `else` branch.
    pub span: Span,
}

/// The `else` branch of a [`HirIf`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirElseBranch {
    /// An `else if` chain: the nested statement's own `else` is inside it.
    If(Box<HirIf>),
    /// A plain `else { ... }` block.
    Block(HirBlock),
}

/// A lowered expression: a [`HirExprKind`] plus its exact source span and
/// canonical type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirExpr {
    /// The kind of expression.
    pub kind: HirExprKind,
    /// Span covering the whole expression as written (including any
    /// surrounding parentheses).
    pub span: Span,
    /// The expression's canonical type.
    pub ty: TypeId,
}

/// The kind of a [`HirExpr`].
///
/// Literal expressions (`Int`, `Float`, `Str`, `Char`) carry no decoded
/// value, matching the AST: the raw source text is recovered from the
/// expression's span. Syntax-only grouping is already eliminated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirExprKind {
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
    /// A resolved name reference.
    Var(HirIdent),
    /// A prefix unary operation: `-x`, `!x`, `~x`.
    Unary {
        /// The operator.
        op: UnaryOp,
        /// The operand.
        operand: Box<HirExpr>,
    },
    /// An infix binary operation.
    Binary {
        /// The operator.
        op: BinaryOp,
        /// The left operand.
        lhs: Box<HirExpr>,
        /// The right operand.
        rhs: Box<HirExpr>,
    },
    /// An assignment: `target = value`, `target += value`, etc.
    Assign {
        /// The assignment operator.
        op: AssignOp,
        /// The assignment target (a place expression).
        target: Box<HirExpr>,
        /// The assigned value.
        value: Box<HirExpr>,
    },
    /// A range construction: `start..end` or `start..=end`.
    Range {
        /// Whether the range is inclusive (`..=`).
        inclusive: bool,
        /// The range start.
        start: Box<HirExpr>,
        /// The range end.
        end: Box<HirExpr>,
    },
    /// A function call: `callee(args)`.
    Call {
        /// The called expression.
        callee: Box<HirExpr>,
        /// The arguments, in source order.
        args: Vec<HirExpr>,
    },
    /// A member access: `base.member`.
    Member {
        /// The accessed base expression.
        base: Box<HirExpr>,
        /// The member name (not a symbol at this stage).
        member: HirName,
    },
    /// An index expression: `base[index]`.
    Index {
        /// The indexed base expression.
        base: Box<HirExpr>,
        /// The index expression.
        index: Box<HirExpr>,
    },
}
