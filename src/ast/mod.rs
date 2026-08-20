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

/// A top-level declaration: a function, a `struct` declaration, a `let`
/// binding, or a `const` binding.
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
    /// A `struct` declaration: a named record of typed fields (session 14).
    Struct(StructItem),
    /// An `enum` declaration: a closed set of named alternatives (session
    /// 17).
    Enum(EnumItem),
    /// A `let` binding.
    Let(LetItem),
    /// A `const` binding.
    Const(ConstItem),
}

/// A `struct` declaration: `struct Name { field: Type, ... }` (session 14).
///
/// Structs are the first user-defined types: a named record whose fields
/// have explicit types. They are top-level declarations only; their layout
/// (field offsets, alignment, size) is deterministic and documented in
/// `docs/implementation/AGGREGATE_TYPES_IMPLEMENTATION.md`. A struct must
/// declare at least one field (enforced by type analysis).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructItem {
    /// The struct's name (a type name, resolved in the type namespace).
    pub name: Ident,
    /// The declared fields, in source order.
    pub fields: Vec<StructField>,
    /// Span covering the whole item from `struct` through the closing brace.
    pub span: Span,
}

/// An `enum` declaration: `enum Name { Variant, ... }` (session 17).
///
/// Enums are the second user-defined type form: a closed set of named
/// alternatives. A variant is either a unit variant (a bare name, value
/// `Name::Variant`) or a data-carrying variant (session 19) carrying a
/// single payload: `Variant(Type)`. The value of a unit variant is its
/// discriminant alone; a data-carrying variant's value additionally holds
/// its payload, and the type's layout is a tagged union — see
/// `docs/implementation/ENUM_TYPES_IMPLEMENTATION.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumItem {
    /// The enum's name (a type name, resolved in the type namespace).
    pub name: Ident,
    /// The declared variants, in source order.
    pub variants: Vec<EnumVariant>,
    /// Span covering the whole item from `enum` through the closing brace.
    pub span: Span,
}

/// One declared variant of an [`EnumItem`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    /// The variant's name.
    pub name: Ident,
    /// The variant's payload type, if it is a data-carrying variant
    /// (`Variant(Type)`); `None` for a unit variant. The declared type is
    /// validated (named, non-array, non-reference) by type analysis.
    pub payload: Option<Ty>,
    /// An explicit discriminant (session 20): `Variant = 5`. The value is
    /// an integer literal expression (possibly negated) whose wrapping
    /// 64-bit value becomes the variant's tag. `None` for a variant whose
    /// discriminant is implicit (the previous variant's value plus one,
    /// starting at 0). The literal is decoded by type analysis.
    pub discriminant: Option<Expr>,
    /// Span covering the variant (its identifier, the parenthesized
    /// payload type for data-carrying variants, and any explicit
    /// discriminant literal).
    pub span: Span,
}

/// One declared field of a [`StructItem`]: `name: Type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructField {
    /// The field's name.
    pub name: Ident,
    /// The field's declared type.
    pub ty: Ty,
    /// Span covering the whole field (name, colon, and type).
    pub span: Span,
}

/// A type as written in source. Types appear in struct field declarations
/// (`name: Type`); the current milestone supports named primitive and struct
/// types, pointer types (`Ptr<T>`), and fixed-length array types
/// (`[T; N]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ty {
    /// The kind of type.
    pub kind: TyKind,
    /// Span covering the whole type.
    pub span: Span,
}

/// The kind of a [`Ty`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TyKind {
    /// A named type: a primitive (`Int`, `Float`, `Bool`, `Char`, `Str`) or
    /// a user-declared struct name.
    Named(Ident),
    /// A pointer type: `Ptr<T>`.
    Ptr(Box<Ty>),
    /// A reference type (session 16): `&T` (immutable) or `&mut T`
    /// (mutable). References are first-class types enforced by the borrow
    /// checker; see `docs/implementation/REFERENCES_BORROWING_IMPLEMENTATION.md`.
    Ref {
        /// Whether the reference is mutable (`&mut T`).
        mutable: bool,
        /// The referent type.
        inner: Box<Ty>,
    },
    /// A fixed-length array type: `[T; N]` where `N` is a non-negative
    /// integer literal (validated by type analysis).
    Array {
        /// The element type.
        elem: Box<Ty>,
        /// The length literal (an integer-literal expression; its value is
        /// decoded from the source text by later stages).
        len: Expr,
    },
    /// A tuple type (session 29): `(T1, T2, ...)`. An empty tuple `()`
    /// is the unit type. A single-element tuple `(T,)` is distinct from
    /// `T`.
    Tuple(Vec<Ty>),
}

/// A `fn` function declaration: `fn name(params) { body }` or
/// `fn name(params) -> Type { body }`.
///
/// Session 25 added optional return-type annotations: a function may
/// declare `-> ReturnType` after its parameter list, which the type
/// checker enforces. When absent (`None`), the return type is inferred
/// from `return` expressions, preserving backward compatibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnItem {
    /// The function's name.
    pub name: Ident,
    /// The declared parameters.
    pub params: Vec<Param>,
    /// An optional return-type annotation (`-> Type`). When `Some`, the
    /// type checker enforces that every `return expr;` in the body
    /// produces this type; when `None`, the return type is inferred.
    pub return_ty: Option<Ty>,
    /// The function body block.
    pub body: Block,
}

/// A single function parameter: `name` or `name: Type`.
///
/// Session 25 added optional type annotations: a parameter may carry a
/// declared type (`name: Type`) that the type checker enforces. When the
/// annotation is absent (`None`), the parameter's type is inferred from
/// usage, preserving backward compatibility with earlier programs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    /// The parameter's name.
    pub name: Ident,
    /// An optional type annotation (`: Type`). When `Some`, the type
    /// checker enforces that the parameter's usage matches the declared
    /// type; when `None`, the type is inferred.
    pub ty: Option<Ty>,
    /// Span of the parameter (its identifier and optional annotation).
    pub span: Span,
}

/// A `let` binding: `let [mut] name [: Type] = init;` or
/// `let [mut] (a, b) [: Type] = init;` (session 31, tuple destructuring).
///
/// Session 26 added optional type annotations: a binding may carry a
/// declared type (`name: Type`) that the type checker enforces. When the
/// annotation is absent (`None`), the type is inferred from the initializer
/// expression, preserving backward compatibility.
///
/// Session 31 adds optional tuple destructuring patterns: when `pattern`
/// is `Some(Pattern::Tuple { .. })`, the binding destructures the
/// initializer into its elements. The `name` field is still set (to the
/// first element's identifier) for backward compatibility with consumers
/// that only handle simple bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetItem {
    /// The bound name (for simple `let x = ...` bindings).
    pub name: Ident,
    /// Whether the binding is mutable (`let mut`).
    pub mutable: bool,
    /// An optional type annotation (`: Type`). When `Some`, the type
    /// checker enforces that the initializer expression has the declared
    /// type; when `None`, the type is inferred.
    pub ty: Option<Ty>,
    /// The initializer expression.
    pub init: Expr,
    /// An optional destructuring pattern (session 31). When `Some`, the
    /// binding destructures the initializer into its pattern elements.
    /// For simple `let x = ...` this is `None`.
    pub pattern: Option<Pattern>,
}

/// A `const` binding: `const name [: Type] = init;`.
///
/// Session 26 added optional type annotations, mirroring let bindings.
/// When the annotation is present, the type checker enforces that the
/// initializer expression matches the declared type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstItem {
    /// The bound name.
    pub name: Ident,
    /// An optional type annotation (`: Type`). When `Some`, the type
    /// checker enforces that the initializer expression has the declared
    /// type; when `None`, the type is inferred.
    pub ty: Option<Ty>,
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

/// A `{ ... }` block: an ordered list of statements, optionally ending
/// with an expression that determines the block's value.
///
/// Session 28 introduced block expressions: when the block's last item
/// is an expression **not** followed by `;`, that expression becomes the
/// block's trailing expression and determines its type. An empty block
/// or a block whose last item ends with `;` has no trailing expression
/// and evaluates to `Unit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The statements, in source order.
    pub stmts: Vec<Stmt>,
    /// The optional trailing expression (session 28). When `Some`, this
    /// expression is the block's value and its type determines the
    /// block's type. When `None`, the block evaluates to `Unit`.
    pub result: Option<Expr>,
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
    /// `break;` or `break expr;` (session 30).
    Break(Option<Expr>),
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
    /// A `match` statement (session 18): `match scrutinee { pattern =>
    /// block, ... }` dispatching on the scalar value of `scrutinee`.
    Match(MatchStmt),
    /// An expression evaluated for its side effects, followed by `;`.
    Expr(Expr),
}

/// A `match` statement (session 18): evaluates `scrutinee` once, then runs
/// the block of the first arm whose pattern matches its value.
///
/// Matching is a statement, not an expression: arms are statement blocks,
/// like `if` bodies, and `match` produces no value. Only scalar values are
/// matchable — `Int` (integer-literal patterns), `Bool` (`true`/`false`),
/// and enums (variant paths `E::V`) — plus the catch-all `_` wildcard and
/// `name` binding patterns. The type checker requires every match to be
/// exhaustive and rejects unreachable arms; see
/// `docs/implementation/PATTERN_MATCHING_IMPLEMENTATION.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchStmt {
    /// The value being matched, evaluated exactly once.
    pub scrutinee: Expr,
    /// The arms, in source order. The first matching arm's block runs.
    pub arms: Vec<MatchArm>,
    /// Span covering the whole `match` statement including its braces.
    pub span: Span,
}

/// One arm of a [`MatchStmt`]: `pattern => block`, optionally guarded
/// (`pattern if expr => block`, session 27).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    /// The pattern this arm matches.
    pub pattern: Pattern,
    /// The guard condition (session 27): when present, the arm runs only
    /// if the pattern matches **and** the guard evaluates to `true`. A
    /// guarded arm does not commit its pattern's coverage, so it never
    /// makes a match exhaustive and never makes later arms unreachable.
    pub guard: Option<Expr>,
    /// The block run when the pattern matches (and the guard passes).
    pub body: Block,
    /// Span covering the whole arm (pattern, guard, `=>`, and block).
    pub span: Span,
}

/// A match pattern (session 18): the left side of a `=>` arm.
///
/// Patterns match scalar values: `_` and `name` match anything (a `name`
/// additionally binds the value in the arm's scope), `true`/`false` match
/// booleans, integer literals (optionally negated) match `Int` values, and
/// `E::V` matches the enum variant `V` of enum `E` by discriminant.
/// Literal values are not decoded into the tree: the raw source text is
/// recovered from the literal's span, matching the expression convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    /// `_`: matches any value and binds nothing.
    Wildcard {
        /// Span of the `_` token.
        span: Span,
    },
    /// `name`: matches any value and binds it (immutably) in the arm's
    /// scope.
    Binding(Ident),
    /// `EnumName::Variant`: matches the enum value whose discriminant is
    /// `Variant`'s position in `EnumName`'s variant list. For a
    /// data-carrying variant (session 19) the pattern is
    /// `EnumName::Variant(pattern)` and `payload` holds the inner pattern,
    /// which binds the variant's payload in the arm's scope.
    EnumVariant {
        /// The enum type name.
        name: Ident,
        /// The variant name.
        variant: Ident,
        /// The payload pattern for a data-carrying variant, if any.
        payload: Option<Box<Pattern>>,
    },
    /// `true` or `false`: matches a boolean value.
    Bool {
        /// The literal value.
        value: bool,
        /// Span of the literal token.
        span: Span,
    },
    /// An integer literal pattern: `5` or `-5`. The literal is an
    /// `ExprKind::Int` node whose span covers the digit token; `negative`
    /// records a leading `-`.
    Int {
        /// Whether the literal is negated (`-5`).
        negative: bool,
        /// The integer-literal expression (its span covers the digits).
        literal: Expr,
        /// Span covering the whole pattern (including any `-`).
        span: Span,
    },
    /// An integer range pattern (session 27): `lo..=hi` (inclusive) or
    /// `lo..hi` (exclusive), where both endpoints are integer literal
    /// patterns (`5`, `-5`, `0x10`, …). The pattern matches an `Int` value
    /// inside the interval.
    Range {
        /// The lower endpoint: an [`Pattern::Int`] (possibly negated).
        lo: Box<Pattern>,
        /// The upper endpoint: an [`Pattern::Int`] (possibly negated).
        hi: Box<Pattern>,
        /// Whether the upper endpoint is included (`..=` vs `..`).
        inclusive: bool,
        /// Span covering the whole range pattern.
        span: Span,
    },
    /// An or-pattern (session 27): `p1 | p2 | …`, matching any value any
    /// alternative matches. Every alternative must bind the same names
    /// (with compatible types); a binding in one alternative is bound by
    /// all of them.
    Or {
        /// The alternatives, in source order.
        alternatives: Vec<Pattern>,
        /// Span covering the whole or-pattern.
        span: Span,
    },
    /// A tuple pattern (session 29): `(pat, pat, ...)` or `()` for unit.
    /// The pattern matches a tuple value element-wise.
    Tuple {
        /// The element patterns, in source order.
        elements: Vec<Pattern>,
        /// Span covering the whole tuple pattern.
        span: Span,
    },
}

impl Pattern {
    /// The span covered by the whole pattern as written.
    pub fn span(&self) -> Span {
        match self {
            Self::Wildcard { span } => *span,
            Self::Binding(ident) => ident.span,
            Self::EnumVariant { name, variant, .. } => {
                let start = name.span.start().min(variant.span.start());
                let end = name.span.end().max(variant.span.end());
                Span::new(name.span.file(), start..end)
            }
            Self::Bool { span, .. } => *span,
            Self::Int { span, .. } => *span,
            Self::Range { span, .. } => *span,
            Self::Or { span, .. } => *span,
            Self::Tuple { span, .. } => *span,
        }
    }
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

/// The `else` branch of an [`IfStmt`] or [`IfExpr`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElseBranch {
    /// An `else if` chain: the nested statement's own `else` is inside it.
    If(Box<IfStmt>),
    /// An `else if` expression chain (session 28).
    IfExpr(Box<IfExpr>),
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
    /// A reference/borrow expression (session 16): `&place` (shared,
    /// `mutable: false`) or `&mut place` (exclusive, `mutable: true`).
    Borrow {
        /// Whether the borrow is mutable (`&mut`).
        mutable: bool,
        /// The borrowed place.
        operand: Box<Expr>,
    },
    /// A dereference expression (session 16): `*r` reads (or, as an
    /// assignment target, writes) the referent of reference `r`.
    Deref {
        /// The reference expression.
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
    /// A struct literal: `Name { field: value, ... }` (session 14). The
    /// struct name resolves in the type namespace.
    StructLit {
        /// The struct type name.
        name: Ident,
        /// The field initializers, in source order.
        fields: Vec<StructFieldInit>,
    },
    /// An array literal: `[elem, ...]` (session 14). The element type is
    /// inferred from the elements; the array's length is the element count.
    ArrayLit(Vec<Expr>),
    /// An enum variant reference or construction (session 17):
    /// `EnumName::Variant`. The enum name resolves in the type namespace;
    /// the variant names one of its declared alternatives. The expression's
    /// type is the enum type. For a data-carrying variant (session 19) the
    /// construction `EnumName::Variant(expr)` carries the payload
    /// expression; `payload` is `None` for unit variants. The names are
    /// boxed so the node stays as small as the existing aggregate nodes (a
    /// nested `Group` chain can be hundreds deep, and every parser frame
    /// holds one `Expr` by value).
    EnumVariant {
        /// The enum type name.
        name: Box<Ident>,
        /// The variant name.
        variant: Box<Ident>,
        /// The payload expression for a data-carrying construction, if any.
        payload: Option<Box<Expr>>,
    },
    /// A parenthesized expression: `(inner)`. Kept as a node so tooling can
    /// distinguish explicit grouping from parser-imposed association.
    Group(Box<Expr>),
    /// An `if` expression (session 28): `if cond { then } else { else }`.
    /// Both branches must be expression blocks that produce a value of the
    /// same type; the `else` branch is required. The expression evaluates
    /// to the value of the taken branch.
    IfExpr(Box<IfExpr>),
    /// A block expression (session 28): `{ stmts; expr }`. The block's
    /// trailing expression determines the value.
    Block(Box<Block>),
    /// A tuple expression (session 29): `(expr, expr, ...)` or `()` for
    /// the unit value. A single-element tuple `(expr,)` has a trailing
    /// comma.
    Tuple(Vec<Expr>),
    /// A tuple field access (session 29): `base.index` where `index` is
    /// a non-negative integer literal. The field index is decoded from
    /// the source text by later stages.
    TupleFieldAccess {
        /// The tuple expression.
        base: Box<Expr>,
        /// The field index (the integer literal's span, for source text
        /// recovery).
        index: Ident,
    },
    /// A `while` expression (session 30): `while cond { body }` used in
    /// expression position. The loop body must contain `break` with a
    /// value; the expression's type is inferred from the break values.
    WhileExpr {
        /// The loop condition.
        cond: Box<Expr>,
        /// The loop body (an expression block with trailing expression).
        body: Box<Block>,
        /// Span covering the whole expression.
        span: Span,
    },
    /// A `loop` expression (session 30): `loop { body }` used in
    /// expression position. The loop body must contain `break` with a
    /// value; the expression's type is inferred from the break values.
    LoopExpr {
        /// The loop body (an expression block with trailing expression).
        body: Box<Block>,
        /// Span covering the whole expression.
        span: Span,
    },
}

/// An `if` expression (session 28): evaluates `cond`, then evaluates
/// and returns the value of the `then_block` when true, or the
/// `else_branch` when false. The `else` is required because both
/// branches must produce a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfExpr {
    /// The condition expression.
    pub cond: Box<Expr>,
    /// The block evaluated when the condition is true; must have a
    /// trailing expression.
    pub then_block: Block,
    /// The `else` branch: either another `if` expression or a block.
    /// Required for if-expressions.
    pub else_branch: ElseBranch,
    /// Span covering the whole expression from `if` through the else branch.
    pub span: Span,
}

/// One field initializer of a struct literal: `name: value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructFieldInit {
    /// The initialized field's name.
    pub name: Ident,
    /// The value expression.
    pub value: Expr,
    /// Span covering the whole initializer.
    pub span: Span,
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
