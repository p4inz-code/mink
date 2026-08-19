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
    /// The predeclared runtime intrinsics referenced by this program: each
    /// intrinsic's symbol and its stable id in the runtime intrinsic
    /// table. Intrinsics are not items; the MIR layer uses this list to
    /// lower intrinsic references to module-item-style operands, and the
    /// backend uses it to lower calls to runtime services.
    pub intrinsic_symbols: Vec<(crate::semantics::SymbolId, crate::runtime::IntrinsicId)>,
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
    /// A `struct` declaration: a type, not a value. The struct's name and
    /// its fields live in the program's type table (the struct's type
    /// carries its [`StructId`](crate::typecheck::StructId)); the item
    /// records the declaration so the HIR faithfully represents the
    /// program. It produces no code.
    Struct(HirStruct),
    /// An `enum` declaration (session 17): a type, not a value. The enum's
    /// name and its variants live in the program's type table (the enum's
    /// type carries its [`EnumId`](crate::typecheck::EnumId)); the item
    /// records the declaration so the HIR faithfully represents the
    /// program. It produces no code.
    Enum(HirEnum),
    /// A `let` binding.
    Let(HirLet),
    /// A `const` binding.
    Const(HirConst),
}

/// A lowered `struct` declaration: the declared name and span. The
/// struct's fields are resolved into the program's type table by type
/// analysis; the MIR layer skips struct items (they produce no code).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirStruct {
    /// The struct's name (a type name, not a symbol).
    pub name: HirName,
    /// Span covering the whole `struct` item.
    pub span: Span,
}

/// A lowered `enum` declaration (session 17): the declared name and span.
/// The enum's variants are resolved into the program's type table by type
/// analysis; the MIR layer skips enum items (they produce no code).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirEnum {
    /// The enum's name (a type name, not a symbol).
    pub name: HirName,
    /// Span covering the whole `enum` item.
    pub span: Span,
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
    /// A `match` statement (session 18): a `match scrutinee { pattern =>
    /// block, ... }` dispatching on the scalar value of `scrutinee`.
    Match(HirMatch),
    /// An expression evaluated for its side effects.
    Expr(HirExpr),
}

/// A lowered `match` statement: the typed scrutinee plus its arms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirMatch {
    /// The value being matched, evaluated exactly once.
    pub scrutinee: HirExpr,
    /// The arms, in source order.
    pub arms: Vec<HirMatchArm>,
    /// Span covering the whole `match` statement.
    pub span: Span,
}

/// One lowered `match` arm: a pattern, an optional guard (session 27),
/// and its body block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirMatchArm {
    /// The pattern this arm matches.
    pub pattern: HirPattern,
    /// The guard condition (session 27): when present, the arm runs only
    /// if the pattern matches **and** the guard evaluates to `true`.
    pub guard: Option<HirExpr>,
    /// The block run when the pattern matches (and the guard passes).
    pub body: HirBlock,
    /// Span covering the whole arm.
    pub span: Span,
}

/// A lowered match pattern. Mirrors the AST [`Pattern`](crate::ast::Pattern):
/// `_` and `name` match anything (a binding carries its resolved symbol),
/// `true`/`false` match booleans, integer literals (optionally negated)
/// match `Int` values, and `EnumName::Variant` matches by discriminant.
/// Literal values stay undecoded: MIR lowering builds its comparison
/// constants from the literal's span, matching the IR convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirPattern {
    /// `_`: matches any value and binds nothing.
    Wildcard {
        /// Span of the `_` token.
        span: Span,
    },
    /// `name`: matches any value and binds it in the arm's scope.
    Binding(HirIdent),
    /// `EnumName::Variant`: matches the enum value with that discriminant.
    /// For a data-carrying variant (session 19) the pattern is
    /// `EnumName::Variant(pattern)` and `payload` holds the inner pattern,
    /// which binds the variant's payload in the arm's scope.
    EnumVariant {
        /// The enum type name.
        name: Box<HirName>,
        /// The variant name.
        variant: Box<HirName>,
        /// The payload pattern, for a data-carrying variant pattern.
        payload: Option<Box<HirPattern>>,
        /// Span of the whole pattern.
        span: Span,
    },
    /// `true` or `false`.
    Bool {
        /// The literal value.
        value: bool,
        /// Span of the literal token.
        span: Span,
    },
    /// An integer literal pattern: `5` or `-5`.
    Int {
        /// Whether the literal is negated.
        negative: bool,
        /// Span of the integer-literal token (whose text is decoded by the
        /// backend).
        literal_span: Span,
        /// Span of the whole pattern (including any `-`).
        span: Span,
    },
    /// An integer range pattern (session 27): `lo..=hi` or `lo..hi`. The
    /// endpoints are integer-literal tokens (possibly negated), decoded by
    /// the backend; `inclusive` records `..=` vs `..`.
    Range {
        /// Whether the lower endpoint is negated (`-5..`).
        lo_negative: bool,
        /// Span of the lower endpoint's literal token.
        lo_span: Span,
        /// Whether the upper endpoint is negated (`..-5`).
        hi_negative: bool,
        /// Span of the upper endpoint's literal token.
        hi_span: Span,
        /// Whether the upper endpoint is included (`..=` vs `..`).
        inclusive: bool,
        /// Span of the whole range pattern.
        span: Span,
    },
    /// An or-pattern (session 27): `p1 | p2 | …`. The alternatives share
    /// one binding per name; MIR lowering tests them in order and every
    /// matching alternative leads to the same arm body.
    Or {
        /// The alternatives, in source order.
        alternatives: Vec<HirPattern>,
        /// Span of the whole or-pattern.
        span: Span,
    },
    /// A tuple pattern (session 29): `(pat, pat, ...)` or `()`.
    Tuple {
        /// The element patterns, in source order.
        elements: Vec<HirPattern>,
        /// Span of the whole tuple pattern.
        span: Span,
    },
}

impl HirPattern {
    /// The span covered by the whole pattern as written.
    pub fn span(&self) -> Span {
        match self {
            Self::Wildcard { span } => *span,
            Self::Binding(ident) => ident.span,
            Self::EnumVariant { span, .. } => *span,
            Self::Bool { span, .. } => *span,
            Self::Int { span, .. } => *span,
            Self::Range { span, .. } => *span,
            Self::Or { span, .. } => *span,
            Self::Tuple { span, .. } => *span,
        }
    }
}

/// A lowered `{ ... }` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirBlock {
    /// The statements, in source order.
    pub stmts: Vec<HirStmt>,
    /// The optional trailing expression (session 28). When `Some`, this
    /// expression is the block's value.
    pub result: Option<HirExpr>,
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
    /// An `else if` expression chain (session 28).
    IfExpr(Box<HirIf>),
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
    /// A reference/borrow expression (session 16): `&place` (shared,
    /// `mutable: false`) or `&mut place` (exclusive, `mutable: true`).
    Borrow {
        /// Whether the borrow is mutable (`&mut`).
        mutable: bool,
        /// The borrowed place.
        operand: Box<HirExpr>,
    },
    /// A dereference expression (session 16): `*r` reads (or, as an
    /// assignment target, writes) the referent of reference `r`.
    Deref {
        /// The reference expression.
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
    /// A struct literal: `Name { field: value, ... }`. The struct's type is
    /// the literal's type; the field values are paired with their names.
    StructLit {
        /// The struct type name (a type name, not a symbol).
        name: HirName,
        /// The field initializers, in source order.
        fields: Vec<(HirName, HirExpr)>,
    },
    /// An array literal: `[elem, ...]`.
    ArrayLit(Vec<HirExpr>),
    /// An enum variant reference or construction: `EnumName::Variant`. The
    /// expression's type is the enum type. For a unit variant the value is
    /// its discriminant, resolved by MIR lowering from the type table; for
    /// a data-carrying variant (session 19) `payload` holds the
    /// construction's payload expression. The names are boxed so the node
    /// stays as small as the existing aggregate nodes (mirroring the
    /// AST).
    EnumVariant {
        /// The enum type name (a type name, not a symbol).
        name: Box<HirName>,
        /// The variant name.
        variant: Box<HirName>,
        /// The payload expression, for a data-carrying construction.
        payload: Option<Box<HirExpr>>,
    },
    /// An `if` expression (session 28): evaluates `cond`, then evaluates
    /// and returns the value of the `then_block` or `else_branch`.
    IfExpr {
        /// The condition.
        cond: Box<HirExpr>,
        /// The then block (must have a trailing expression).
        then_block: Box<HirBlock>,
        /// The else branch: another `if` expression or a block.
        else_branch: Box<HirElseBranch>,
        /// Span.
        span: Span,
    },
    /// A block expression (session 28): `{ stmts; expr }`.
    Block(Box<HirBlock>),
    /// A tuple expression (session 29): `(expr, expr, ...)` or `()`.
    Tuple(Vec<HirExpr>),
    /// A tuple field access (session 29): `base.index` where `index` is a
    /// non-negative integer literal.
    TupleFieldAccess {
        /// The tuple expression.
        base: Box<HirExpr>,
        /// The field index (decoded from the literal's source text).
        index: u32,
        /// Span of the index literal token.
        index_span: Span,
    },
}
