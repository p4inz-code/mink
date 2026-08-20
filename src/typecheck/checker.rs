//! The type checker: a single forward pass over the AST that consumes the
//! session-05 [`SemanticResult`] and produces expression types, symbol
//! types, and type diagnostics.
//!
//! The checker never re-runs name resolution or scope construction:
//! identifier references resolve through [`SemanticResult::resolve`], which
//! maps an identifier span to its [`SymbolId`]. Every declared symbol is
//! given a type slot up front (functions get a real function type whose
//! parameters and result are inference variables; everything else gets an
//! inference variable), so module-scope order independence and mutual
//! recursion work without a second pass.
//!
//! The checker never panics on parser-produced ASTs: every lookup is
//! guarded, and the unknown/error type (`docs/implementation/
//! TYPE_SYSTEM_IMPLEMENTATION.md` §8) absorbs failed sub-expressions so
//! independent errors are reported without cascades.
//!
//! Inference (session 07) is constraint-based and bidirectional where an
//! expected type genuinely determines the answer: conditions pin their
//! expression to `Bool`, `for` iterables pin to `Range<T>`, and the
//! boolean/integer operators pin their unconstrained operands. Inference
//! variables form a union-find structure (see
//! [`TypeTable::unify`](super::ty::TypeTable::unify)), so chains,
//! recursion, and mutually constrained calls all resolve deterministically;
//! see `docs/implementation/TYPE_INFERENCE_IMPLEMENTATION.md`.

use std::collections::HashMap;

use crate::ast::{
    AssignOp, Ast, BinaryOp, Block, ElseBranch, Expr, ExprKind, FnItem, Ident, IfStmt, Item,
    ItemKind, MatchStmt, Pattern, Stmt, StmtKind, StructFieldInit, Ty, TyKind, UnaryOp,
};
use crate::runtime::layout::{self, LayoutError};
use crate::semantics::{SemanticResult, SymbolId, SymbolKind};
use crate::source::{SourceMap, Span};

use super::TypeResult;
use super::error::TypeError;
use super::ty::{EnumId, EnumVariantInfo, StructFieldInfo, StructId, TypeId, TypeKind, TypeTable};

/// A struct whose field types are still unresolved: the struct's id plus
/// each declared field's name, span, and (unresolved) type expression.
/// Used during phase 2 of struct registration, after every declaration is
/// visible.
type PendingStructFields = Vec<(StructId, Vec<(String, Span, Ty)>)>;

/// One enum variant awaiting discriminant/payload resolution: its name,
/// declaration span, (unresolved) payload type expression, and any
/// explicit `= literal` discriminant expression (session 20).
struct PendingVariant {
    name: String,
    span: Span,
    payload: Option<Ty>,
    discriminant: Option<Expr>,
}

/// An enum whose variants are still unresolved: the enum's id and name
/// plus each declared variant. Used during variant resolution, after every
/// enum, struct, and payload declaration is visible.
type PendingEnumVariants = Vec<(EnumId, String, Vec<PendingVariant>)>;

/// Runs type analysis over `ast`, consuming the semantic result and reading
/// literal source text through `sources`.
///
/// The analysis is deterministic: symbol types, expression types, and
/// errors are produced in source order.
pub(crate) fn check_ast(ast: &Ast, semantic: &SemanticResult, sources: &SourceMap) -> TypeResult {
    let mut checker = Checker::new(ast, semantic, sources);
    checker.run();
    checker.finish()
}

/// One finite value class a refutable match pattern covers, used for
/// exhaustiveness and unreachable-arm detection. Patterns of the same key
/// match the same value class, so a repeated key is an unreachable arm.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CoverageKey {
    /// The boolean literal `true` or `false`.
    Bool(bool),
    /// An enum variant, by name.
    Variant(String),
}

/// The coverage of one value class by a refutable pattern: the covered
/// key plus, for a data-carrying variant pattern whose payload pattern is
/// refutable, the nested coverage of the payload type. `sub: None` means
/// the key's value class is fully covered.
#[derive(Debug, Clone, PartialEq, Eq)]
struct KeyCover {
    /// The covered key.
    key: CoverageKey,
    /// Nested payload coverage for a partially covered variant; `None`
    /// when the value class is fully covered.
    sub: Option<Box<Coverage>>,
}

/// The recursive coverage of a scrutinee type by the refutable patterns
/// of a `match` (sessions 18–27). `all` records that a `_`/binding arm
/// covered every value, making every later arm unreachable; `keys` record
/// the covered finite value classes (a data-carrying variant's payload
/// coverage recurses through [`KeyCover::sub`]); `intervals` record the
/// covered `Int` points and ranges (session 27) as a sorted, disjoint,
/// merged list of inclusive `[lo, hi]` intervals.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Coverage {
    /// The covered keys, in arm order.
    keys: Vec<KeyCover>,
    /// Covered integer intervals `(lo, hi)`, sorted by `lo`, disjoint, and
    /// merged (adjacent intervals are merged too, since integer coverage
    /// is contiguous). Points are `[n, n]`.
    intervals: Vec<(i64, i64)>,
    /// Whether a catch-all (`_`/binding) arm covered every value.
    all: bool,
}

/// The operator categories the checker distinguishes; each category has
/// exactly one operand rule (`docs/language/CORE_LANGUAGE.md` §26).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpCategory {
    /// `+ - * / %`: both operands the same numeric type; the result is
    /// that type.
    Arithmetic,
    /// `<< >>`: both operands `Int`; the result is `Int`.
    Shift,
    /// `< <= > >=`: both operands the same numeric type; the result is
    /// `Bool`.
    Comparison,
    /// `== !=`: both operands the same scalar type; the result is `Bool`.
    Equality,
    /// `& ^ |`: both operands `Int`; the result is `Int`.
    Bitwise,
    /// `&& ||`: both operands `Bool`; the result is `Bool`.
    Logical,
}

impl OpCategory {
    fn of(op: BinaryOp) -> Self {
        use BinaryOp::*;
        match op {
            Add | Sub | Mul | Div | Rem => Self::Arithmetic,
            Shl | Shr => Self::Shift,
            Lt | Le | Gt | Ge => Self::Comparison,
            Eq | Ne => Self::Equality,
            BitAnd | BitXor | BitOr => Self::Bitwise,
            And | Or => Self::Logical,
        }
    }
}

/// The type checker traversal.
/// A registered struct declaration in the checker's type namespace.
///
/// Struct names are type names, separate from the value namespace of the
/// semantic symbol table: a struct literal's name resolves here, and a
/// struct name used as a value is an ordinary (unresolved) name. The first
/// declaration of a name wins (duplicates are reported by semantic
/// analysis); fields are resolved after every declaration is registered
/// (module-scope order independence) and live in the [`TypeTable`].
struct StructReg {
    /// The struct's declared name.
    name: String,
    /// Span of the declared name.
    span: Span,
    /// The struct's type id.
    id: TypeId,
    /// The struct's stable id (indexes the table's struct list).
    struct_id: StructId,
}

/// A registered enum declaration in the checker's type namespace.
///
/// Enum names are type names, separate from the value namespace of the
/// semantic symbol table: an enum variant reference's first segment
/// resolves here, and an enum name used as a value is an ordinary
/// (unresolved) name. The first declaration of a name wins (duplicates are
/// reported by semantic analysis); variants are recorded after every
/// declaration is registered (module-scope order independence) and live in
/// the [`TypeTable`].
struct EnumReg {
    /// The enum's declared name.
    name: String,
    /// Span of the declared name.
    span: Span,
    /// The enum's type id.
    id: TypeId,
    /// The enum's stable id (indexes the table's enum list).
    enum_id: EnumId,
}

/// The type checker traversal.
struct Checker<'a> {
    ast: &'a Ast,
    semantic: &'a SemanticResult,
    /// The source map, for reading literal source text (the
    /// null-pointer-constant rule and array-length decoding).
    sources: &'a SourceMap,
    types: TypeTable,
    /// The type of every symbol, indexed by `SymbolId::raw()`.
    symbol_types: Vec<TypeId>,
    /// Declaration name span start → symbol id, for binding lookups.
    decls: HashMap<u32, SymbolId>,
    /// The registered structs (type namespace): name → registration.
    structs: HashMap<String, StructReg>,
    /// The registered enums (type namespace): name → registration.
    enums: HashMap<String, EnumReg>,
    /// Expression types in traversal order: (expression span, type).
    expr_types: Vec<(Span, TypeId)>,
    /// The spans of every member/index expression whose forward-pass type
    /// was a deferred fresh variable (its base was an unresolved inference
    /// variable). These are re-typed by [`Checker::resolve_deferred_members`]
    /// once call sites have resolved the parameters they depend on.
    deferred: Vec<Span>,
    /// Type errors, in the order they were found.
    errors: Vec<TypeError>,
    /// The current function's result type, while inside a function body.
    fn_result: Option<TypeId>,
    /// The current loop's break value type, while inside a loop expression
    /// (session 30). `Some(ty)` means `break expr;` must produce `ty`.
    loop_result: Option<TypeId>,
}

impl<'a> Checker<'a> {
    fn new(ast: &'a Ast, semantic: &'a SemanticResult, sources: &'a SourceMap) -> Self {
        let mut types = TypeTable::new();
        // Placeholder for symbol slots until pre-registration fills them.
        let placeholder = types.push(TypeKind::Error);
        Self {
            ast,
            semantic,
            sources,
            types,
            symbol_types: vec![placeholder; semantic.symbols().len()],
            decls: HashMap::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
            expr_types: Vec::new(),
            deferred: Vec::new(),
            errors: Vec::new(),
            fn_result: None,
            loop_result: None,
        }
    }

    fn run(&mut self) {
        // Type names are registered before any field/variant types are
        // resolved: struct fields and enum payloads may reference structs
        // and enums regardless of declaration order (module-scope order
        // independence). Resolving happens only after both namespaces are
        // fully populated.
        self.register_enum_names();
        self.register_struct_names();
        self.resolve_enum_variants();
        self.resolve_struct_fields();
        self.validate_aggregate_layouts();
        self.pre_register();
        for item in &self.ast.items {
            self.check_item(item);
        }
        // Member/index expressions whose base was an unresolved inference
        // variable during the forward pass are re-typed now that every
        // call site has resolved the function parameters they depend on
        // (a body is checked before the calls that constrain its
        // parameters). See [`Checker::resolve_deferred_members`].
        self.resolve_deferred_members();
    }

    // ------------------------------------------------------------------
    // Deferred member/index resolution
    // ------------------------------------------------------------------

    /// Re-types every member/index expression whose forward-pass type was
    /// a deferred fresh variable (its base was unresolved when its body was
    /// checked — a function parameter is resolved by the call sites, which
    /// are checked after the body).
    ///
    /// The forward pass could not type `p.f` while `p` was still an
    /// inference variable, so it recorded a fresh unconstrained variable
    /// (see `deferred`) and every parent typed against that variable,
    /// which may have adopted a wrong concrete type (an `Int` sum adopting
    /// a `Bool` field's deferred type). By now every call site has unified
    /// its arguments, so this walk re-types each affected subtree
    /// bottom-up against the base's canonical type — updating the recorded
    /// expression types in place (deterministic traversal order) — and
    /// re-checks the operators, assignments, conditions, iterables, and
    /// returns that consumed them, so a previously invisible mismatch is
    /// reported instead of reaching the backend. Diagnostics that the
    /// recomputation reproduces are deduplicated by [`Checker::push_error`].
    fn resolve_deferred_members(&mut self) {
        for item in &self.ast.items {
            match &item.kind {
                ItemKind::Fn(f) => {
                    // Restore the function's result type so `return`
                    // re-checks inside this pass see it.
                    let saved = self.fn_result.take();
                    self.fn_result = self.fn_result_of(f);
                    self.resolve_deferred_block(&f.body);
                    self.fn_result = saved;
                }
                ItemKind::Let(binding) => {
                    let (ty, recomputed) = self.resolve_deferred_expr(&binding.init);
                    if recomputed {
                        if let Some(ann) = &binding.ty {
                            let ann_ty = self.resolve_type(ann);
                            if let Err((expected, actual)) = self.types.unify(ann_ty, ty) {
                                self.push_error(TypeError::mismatch(
                                    binding.init.span,
                                    self.display(expected),
                                    self.display(actual),
                                    Some(binding.name.span),
                                ));
                            }
                        }
                        self.unify_decl(&binding.name, ty, binding.init.span);
                    }
                }
                ItemKind::Const(binding) => {
                    let (ty, recomputed) = self.resolve_deferred_expr(&binding.init);
                    if recomputed {
                        if let Some(ann) = &binding.ty {
                            let ann_ty = self.resolve_type(ann);
                            if let Err((expected, actual)) = self.types.unify(ann_ty, ty) {
                                self.push_error(TypeError::mismatch(
                                    binding.init.span,
                                    self.display(expected),
                                    self.display(actual),
                                    Some(binding.name.span),
                                ));
                            }
                        }
                        self.unify_decl(&binding.name, ty, binding.init.span);
                    }
                }
                ItemKind::Struct(_) | ItemKind::Enum(_) => {}
            }
        }
    }

    /// The result type variable of `f`'s signature, if the function was
    /// pre-registered (mirrors [`Checker::check_fn`]).
    fn fn_result_of(&self, f: &FnItem) -> Option<TypeId> {
        let fn_ty = self
            .decls
            .get(&f.name.span.start())
            .copied()
            .and_then(|symbol| self.symbol_types.get(symbol.raw() as usize).copied())?;
        match self.types.kind(fn_ty).cloned() {
            Some(TypeKind::Fn { result, .. }) => Some(result),
            _ => None,
        }
    }

    fn resolve_deferred_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.resolve_deferred_stmt(stmt);
        }
    }

    fn resolve_deferred_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Let(binding) => {
                let (ty, recomputed) = self.resolve_deferred_expr(&binding.init);
                if recomputed {
                    if let Some(ref pattern) = binding.pattern {
                        self.check_let_destructure(binding, pattern, ty);
                    } else {
                        if let Some(ann) = &binding.ty {
                            let ann_ty = self.resolve_type(ann);
                            if let Err((expected, actual)) = self.types.unify(ann_ty, ty) {
                                self.push_error(TypeError::mismatch(
                                    binding.init.span,
                                    self.display(expected),
                                    self.display(actual),
                                    Some(binding.name.span),
                                ));
                            }
                        }
                        self.unify_decl(&binding.name, ty, binding.init.span);
                    }
                }
            }
            StmtKind::Const(binding) => {
                let (ty, recomputed) = self.resolve_deferred_expr(&binding.init);
                if recomputed {
                    if let Some(ann) = &binding.ty {
                        let ann_ty = self.resolve_type(ann);
                        if let Err((expected, actual)) = self.types.unify(ann_ty, ty) {
                            self.push_error(TypeError::mismatch(
                                binding.init.span,
                                self.display(expected),
                                self.display(actual),
                                Some(binding.name.span),
                            ));
                        }
                    }
                    self.unify_decl(&binding.name, ty, binding.init.span);
                }
            }
            StmtKind::Return(Some(value)) => {
                let (ty, recomputed) = self.resolve_deferred_expr(value);
                if recomputed {
                    self.recheck_return(ty, value.span);
                }
            }
            StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue => {}
            StmtKind::If(stmt) => self.resolve_deferred_if(stmt),
            StmtKind::While { cond, body } => {
                let (ty, recomputed) = self.resolve_deferred_expr(cond);
                if recomputed {
                    self.recheck_condition(ty, cond.span);
                }
                self.resolve_deferred_block(body);
            }
            StmtKind::For {
                name,
                iterable,
                body,
            } => {
                let (iter_ty, recomputed) = self.resolve_deferred_expr(iterable);
                if recomputed {
                    self.check_for_var(name, iter_ty, iterable.span);
                }
                self.resolve_deferred_block(body);
            }
            StmtKind::Loop(body) => self.resolve_deferred_block(body),
            StmtKind::Match(stmt) => self.resolve_deferred_match(stmt),
            StmtKind::Expr(expr) => {
                self.resolve_deferred_expr(expr);
            }
        }
    }

    /// Re-checks a `match` whose scrutinee contained a deferred
    /// member/index: the scrutinee is re-typed, the arms' patterns and
    /// exhaustiveness are re-checked against the resolved type (duplicates
    /// are deduplicated by [`Checker::push_error`]), and the arm bodies are
    /// re-walked. Without this, a pattern that mismatched the *resolved*
    /// scrutinee type would silently reach the backend.
    fn resolve_deferred_match(&mut self, stmt: &MatchStmt) {
        let (scrutinee_ty, recomputed) = self.resolve_deferred_expr(&stmt.scrutinee);
        if recomputed {
            self.check_match_patterns(stmt, scrutinee_ty);
        }
        for arm in &stmt.arms {
            // A guard (session 27) is a boolean condition like an `if`
            // condition: when a deferred member/index in it resolved, it
            // is re-checked against `Bool` so a contradiction is reported
            // instead of silently reaching the backend.
            if let Some(guard) = &arm.guard {
                let (guard_ty, guard_recomputed) = self.resolve_deferred_expr(guard);
                if guard_recomputed {
                    self.recheck_condition(guard_ty, guard.span);
                }
            }
            self.resolve_deferred_block(&arm.body);
        }
    }

    fn resolve_deferred_if(&mut self, stmt: &IfStmt) {
        let (ty, recomputed) = self.resolve_deferred_expr(&stmt.cond);
        if recomputed {
            self.recheck_condition(ty, stmt.cond.span);
        }
        self.resolve_deferred_block(&stmt.then_block);
        match &stmt.else_branch {
            Some(ElseBranch::If(nested)) => self.resolve_deferred_if(nested),
            Some(ElseBranch::IfExpr(inner)) => {
                let (ty, r) = self.resolve_deferred_expr(&inner.cond);
                if r {
                    self.recheck_condition(ty, inner.cond.span);
                }
                self.resolve_deferred_block(&inner.then_block);
                match &inner.else_branch {
                    ElseBranch::IfExpr(e) => {
                        let (ty2, r2) = self.resolve_deferred_expr(&e.cond);
                        if r2 {
                            self.recheck_condition(ty2, e.cond.span);
                        }
                        self.resolve_deferred_block(&e.then_block);
                    }
                    ElseBranch::Block(b) => self.resolve_deferred_block(b),
                    _ => {}
                }
            }
            Some(ElseBranch::Block(block)) => self.resolve_deferred_block(block),
            None => {}
        }
    }

    /// Re-checks a `return` value against the current function's result
    /// type after a deferred member/index in it resolved (mirrors the
    /// forward pass's `Return` arm).
    fn recheck_return(&mut self, value_ty: TypeId, span: Span) {
        let Some(result) = self.fn_result else {
            return;
        };
        if let Err((expected, actual)) = self.types.unify(result, value_ty) {
            self.push_error(TypeError::mismatch(
                span,
                self.display(expected),
                self.display(actual),
                None,
            ));
        }
    }

    /// Re-checks a condition against `Bool` after a deferred member/index
    /// in it resolved (mirrors [`Checker::check_condition`] without
    /// re-recording the expression).
    fn recheck_condition(&mut self, cond_ty: TypeId, span: Span) {
        let expected = self.bool_ty();
        if let Err((expected, actual)) = self.types.unify(cond_ty, expected) {
            self.push_error(TypeError::mismatch(
                span,
                self.display(expected),
                self.display(actual),
                None,
            ));
        }
    }

    /// Re-types `expr` bottom-up, updating recorded types in place, and
    /// returns the expression's (canonical) type plus whether this
    /// expression or any descendant was re-typed (a deferred member/index
    /// whose base resolved after the forward pass).
    fn resolve_deferred_expr(&mut self, expr: &Expr) -> (TypeId, bool) {
        let (computed, recomputed) = match &expr.kind {
            ExprKind::Member { base, member } => {
                let (base_ty, _) = self.resolve_deferred_expr(base);
                let recompute = self.deferred.contains(&expr.span);
                let ty = if recompute {
                    let canon = self.types.canonical(base_ty);
                    if self.types.is_error(canon) {
                        self.types.push(TypeKind::Error)
                    } else {
                        self.check_member(canon, member)
                    }
                } else {
                    self.recorded_ty(expr.span)
                        .unwrap_or_else(|| self.types.push(TypeKind::Error))
                };
                (ty, recompute)
            }
            ExprKind::Index { base, index } => {
                let (base_ty, base_recomputed) = self.resolve_deferred_expr(base);
                let (index_ty, index_recomputed) = self.resolve_deferred_expr(index);
                let recompute =
                    self.deferred.contains(&expr.span) || base_recomputed || index_recomputed;
                let ty = if recompute {
                    let canon = self.types.canonical(base_ty);
                    if self.types.is_error(canon) {
                        self.types.push(TypeKind::Error)
                    } else {
                        self.check_index(canon, index, index_ty, expr.span)
                    }
                } else {
                    self.recorded_ty(expr.span)
                        .unwrap_or_else(|| self.types.push(TypeKind::Error))
                };
                (ty, recompute)
            }
            ExprKind::Unary { op, operand } => {
                let (operand_ty, operand_recomputed) = self.resolve_deferred_expr(operand);
                if operand_recomputed {
                    (self.check_unary(*op, operand_ty, expr.span), true)
                } else {
                    (
                        self.recorded_ty(expr.span)
                            .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                        false,
                    )
                }
            }
            ExprKind::Borrow { mutable, operand } => {
                let (operand_ty, operand_recomputed) = self.resolve_deferred_expr(operand);
                let recompute = self.deferred.contains(&expr.span) || operand_recomputed;
                if recompute {
                    (
                        self.check_borrow(*mutable, operand, operand_ty, expr.span),
                        true,
                    )
                } else {
                    (
                        self.recorded_ty(expr.span)
                            .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                        false,
                    )
                }
            }
            ExprKind::Deref { operand } => {
                let (operand_ty, operand_recomputed) = self.resolve_deferred_expr(operand);
                let recompute = self.deferred.contains(&expr.span) || operand_recomputed;
                if recompute {
                    (self.check_deref(operand_ty, expr.span), true)
                } else {
                    (
                        self.recorded_ty(expr.span)
                            .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                        false,
                    )
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let (lhs_ty, lhs_recomputed) = self.resolve_deferred_expr(lhs);
                let (rhs_ty, rhs_recomputed) = self.resolve_deferred_expr(rhs);
                if lhs_recomputed || rhs_recomputed {
                    (self.check_binary(*op, lhs_ty, rhs_ty, expr.span), true)
                } else {
                    (
                        self.recorded_ty(expr.span)
                            .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                        false,
                    )
                }
            }
            ExprKind::Assign { op, target, value } => {
                let (_, target_recomputed) = self.resolve_deferred_expr(target);
                let (value_ty, value_recomputed) = self.resolve_deferred_expr(value);
                if target_recomputed || value_recomputed {
                    (
                        self.check_assign(*op, target, value_ty, value.span, expr.span),
                        true,
                    )
                } else {
                    (
                        self.recorded_ty(expr.span)
                            .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                        false,
                    )
                }
            }
            ExprKind::Range { start, end, .. } => {
                let (start_ty, start_recomputed) = self.resolve_deferred_expr(start);
                let (end_ty, end_recomputed) = self.resolve_deferred_expr(end);
                if start_recomputed || end_recomputed {
                    (self.check_range(start_ty, end_ty, expr.span), true)
                } else {
                    (
                        self.recorded_ty(expr.span)
                            .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                        false,
                    )
                }
            }
            ExprKind::Call { callee, args } => {
                let (callee_ty, callee_recomputed) = self.resolve_deferred_expr(callee);
                let mut any = callee_recomputed;
                for arg in args {
                    let (_, recomputed) = self.resolve_deferred_expr(arg);
                    any |= recomputed;
                }
                if any {
                    (self.check_call(callee, callee_ty, args, expr.span), true)
                } else {
                    (
                        self.recorded_ty(expr.span)
                            .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                        false,
                    )
                }
            }
            ExprKind::StructLit { name, fields } => {
                let mut any = false;
                for field in fields {
                    let (_, recomputed) = self.resolve_deferred_expr(&field.value);
                    any |= recomputed;
                }
                if any {
                    (self.check_struct_literal(name, fields, expr.span), true)
                } else {
                    (
                        self.recorded_ty(expr.span)
                            .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                        false,
                    )
                }
            }
            ExprKind::ArrayLit(elems) => {
                let mut any = false;
                for elem in elems {
                    let (_, recomputed) = self.resolve_deferred_expr(elem);
                    any |= recomputed;
                }
                if any {
                    (self.check_array_literal(elems, expr.span), true)
                } else {
                    (
                        self.recorded_ty(expr.span)
                            .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                        false,
                    )
                }
            }
            ExprKind::Group(inner) => {
                let (ty, recomputed) = self.resolve_deferred_expr(inner);
                if recomputed {
                    self.update_recorded(expr.span, ty);
                }
                (ty, recomputed)
            }
            ExprKind::EnumVariant {
                name,
                variant,
                payload,
            } => {
                let mut any = false;
                if let Some(payload) = payload {
                    let (_, recomputed) = self.resolve_deferred_expr(payload);
                    any |= recomputed;
                }
                if any {
                    (
                        self.check_enum_variant(name, variant, payload, expr.span),
                        true,
                    )
                } else {
                    (
                        self.recorded_ty(expr.span)
                            .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                        false,
                    )
                }
            }
            ExprKind::IfExpr(inner) => {
                let _ = inner;
                (
                    self.recorded_ty(expr.span)
                        .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                    false,
                )
            }
            ExprKind::Block(_block) => (
                self.recorded_ty(expr.span)
                    .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                false,
            ),
            ExprKind::Int
            | ExprKind::Float
            | ExprKind::Str
            | ExprKind::Char
            | ExprKind::Bool(_)
            | ExprKind::Null
            | ExprKind::Ident(_) => (
                self.recorded_ty(expr.span)
                    .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                false,
            ),
            ExprKind::Tuple(elems) => {
                let mut any = false;
                for elem in elems {
                    let (_, r) = self.resolve_deferred_expr(elem);
                    any |= r;
                }
                if any {
                    let elem_tys: Vec<TypeId> = elems.iter().map(|e| self.expr_type(e)).collect();
                    let ty = self.types.push(TypeKind::Tuple(elem_tys));
                    self.update_recorded(expr.span, ty);
                    (ty, true)
                } else {
                    (
                        self.recorded_ty(expr.span)
                            .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                        false,
                    )
                }
            }
            ExprKind::TupleFieldAccess { base, index } => {
                let (_base_ty, base_recomputed) = self.resolve_deferred_expr(base);
                let recompute = self.deferred.contains(&expr.span) || base_recomputed;
                if recompute {
                    let ty = self.check_tuple_field_access(base, index, expr.span);
                    (ty, true)
                } else {
                    (
                        self.recorded_ty(expr.span)
                            .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                        false,
                    )
                }
            }
            ExprKind::WhileExpr { cond, body, .. } => {
                let (_, cond_recomputed) = self.resolve_deferred_expr(cond);
                let recompute = self.deferred.contains(&expr.span) || cond_recomputed;
                if recompute {
                    self.check_condition(cond);
                    let result_var = self.types.push(TypeKind::Infer(None));
                    let saved = self.loop_result.replace(result_var);
                    self.check_block(body);
                    self.loop_result = saved;
                    (result_var, true)
                } else {
                    (
                        self.recorded_ty(expr.span)
                            .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                        false,
                    )
                }
            }
            ExprKind::LoopExpr { body, .. } => {
                let recompute = self.deferred.contains(&expr.span);
                if recompute {
                    let result_var = self.types.push(TypeKind::Infer(None));
                    let saved = self.loop_result.replace(result_var);
                    self.check_block(body);
                    self.loop_result = saved;
                    (result_var, true)
                } else {
                    (
                        self.recorded_ty(expr.span)
                            .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                        false,
                    )
                }
            }
        };
        if recomputed {
            self.update_recorded(expr.span, computed);
        }
        (self.types.canonical(computed), recomputed)
    }

    /// The canonical type recorded for `span`, if any.
    fn recorded_ty(&self, span: Span) -> Option<TypeId> {
        self.expr_types
            .iter()
            .find(|(recorded, _)| *recorded == span)
            .map(|(_, ty)| self.types.canonical(*ty))
    }

    /// Overwrites the recorded type for `span` in place. The deferred
    /// re-type pass updates the forward pass's entry rather than appending
    /// a duplicate (lookups match the first entry for a span).
    fn update_recorded(&mut self, span: Span, ty: TypeId) {
        if let Some(entry) = self.expr_types.iter_mut().find(|(s, _)| *s == span) {
            entry.1 = ty;
        }
    }

    /// Pushes `error`, skipping an identical (kind, span) diagnostic that
    /// was already reported. The deferred re-type pass recomputes subtrees
    /// whose forward pass may already have reported the same error, so a
    /// plain push would duplicate it.
    fn push_error(&mut self, error: TypeError) {
        if self
            .errors
            .iter()
            .any(|existing| existing.kind() == error.kind() && existing.span() == error.span())
        {
            return;
        }
        self.errors.push(error);
    }

    // ------------------------------------------------------------------
    // Struct registration and type resolution
    // ------------------------------------------------------------------

    /// Registers every struct declaration into the type table (first
    /// declaration of each name wins, mirroring semantic analysis),
    /// resolves every field type (module-scope order independence: a field
    /// type may reference any struct in the module), and validates every
    /// struct's deterministic layout.
    /// Registers every struct declaration's name into the type table
    /// (first declaration of each name wins, mirroring semantic analysis).
    /// Field types are resolved separately by
    /// [`Checker::resolve_struct_fields`], after every type name is
    /// registered.
    fn register_struct_names(&mut self) {
        for item in &self.ast.items {
            let ItemKind::Struct(s) = &item.kind else {
                continue;
            };
            if self.structs.contains_key(&s.name.name) {
                continue;
            }
            let id = self.types.push_struct(s.name.name.clone());
            let struct_id = self
                .types
                .struct_id(id)
                .expect("a pushed struct type always denotes a struct");
            self.structs.insert(
                s.name.name.clone(),
                StructReg {
                    name: s.name.name.clone(),
                    span: s.name.span,
                    id,
                    struct_id,
                },
            );
        }
    }

    /// Resolves every registered struct's field types. The first
    /// declaration of each name is identified by its span (later
    /// duplicates are skipped); every field type resolves against the fully
    /// populated type namespace (module-scope order independence).
    fn resolve_struct_fields(&mut self) {
        let mut pending: PendingStructFields = Vec::new();
        for item in &self.ast.items {
            let ItemKind::Struct(s) = &item.kind else {
                continue;
            };
            let Some(reg) = self.structs.get(&s.name.name) else {
                continue;
            };
            if reg.span != s.name.span {
                continue; // duplicate declaration; the first wins
            }
            pending.push((
                reg.struct_id,
                s.fields
                    .iter()
                    .map(|field| (field.name.name.clone(), field.name.span, field.ty.clone()))
                    .collect(),
            ));
        }
        for (struct_id, fields) in pending {
            let resolved = fields
                .iter()
                .map(|(name, _, ty)| {
                    let resolved_ty = self.resolve_type(ty);
                    StructFieldInfo {
                        name: name.clone(),
                        ty: resolved_ty,
                    }
                })
                .collect::<Vec<_>>();
            self.types.set_struct_fields(struct_id, resolved);
        }
    }

    /// Registers every enum declaration's name into the type table (first
    /// declaration of each name wins, mirroring semantic analysis).
    /// Variant payload types are resolved separately by
    /// [`Checker::resolve_enum_variants`], after every type name is
    /// registered.
    fn register_enum_names(&mut self) {
        for item in &self.ast.items {
            let ItemKind::Enum(e) = &item.kind else {
                continue;
            };
            if self.enums.contains_key(&e.name.name) {
                continue;
            }
            let id = self.types.push_enum(e.name.name.clone());
            let enum_id = self
                .types
                .enum_id(id)
                .expect("a pushed enum type always denotes an enum");
            self.enums.insert(
                e.name.name.clone(),
                EnumReg {
                    name: e.name.name.clone(),
                    span: e.name.span,
                    id,
                    enum_id,
                },
            );
        }
    }

    /// Resolves every enum variant's payload type and records every
    /// variant with its effective discriminant (session 20): an explicit
    /// `V = n` literal's wrapping 64-bit value, or the previous variant's
    /// value plus one (starting at 0, unchanged from sessions 17/19 when no
    /// explicit values are given). A value reused by an earlier variant is
    /// `E-T31` (the tag word could not distinguish the variants); an
    /// implicit continuation past `i64::MAX` is `E-T32`. The first
    /// declaration of each name is identified by its span (later duplicates
    /// are skipped); payload types resolve against the fully populated
    /// type namespace (module-scope order independence), so a payload may
    /// reference any struct or enum.
    fn resolve_enum_variants(&mut self) {
        // Collect the declarations to resolve first, so resolving one
        // payload (which can push errors) never aliases the borrow of the
        // registry or the AST.
        let mut pending: PendingEnumVariants = Vec::new();
        for item in &self.ast.items {
            let ItemKind::Enum(e) = &item.kind else {
                continue;
            };
            let Some(reg) = self.enums.get(&e.name.name) else {
                continue;
            };
            if reg.span != e.name.span {
                continue; // duplicate declaration; the first wins
            }
            pending.push((
                reg.enum_id,
                reg.name.clone(),
                e.variants
                    .iter()
                    .map(|variant| PendingVariant {
                        name: variant.name.name.clone(),
                        span: variant.span,
                        payload: variant.payload.clone(),
                        discriminant: variant.discriminant.clone(),
                    })
                    .collect(),
            ));
        }
        for (enum_id, enum_name, variants) in pending {
            let mut next: i64 = 0; // the value the next implicit variant gets
            let mut next_ok = true; // `next` is a valid implicit value
            let mut used: Vec<(i64, Span)> = Vec::new();
            let mut overflow_reported = false;
            let mut resolved = Vec::with_capacity(variants.len());
            for variant in variants {
                let value = match &variant.discriminant {
                    // An explicit literal uses the same wrapping 64-bit
                    // decode as a constant array index.
                    Some(literal) => Some(self.constant_index(literal).unwrap_or(next)),
                    None if next_ok => Some(next),
                    None => None,
                };
                match value {
                    Some(value) => {
                        if let Some((_, first)) = used.iter().find(|(v, _)| *v == value) {
                            self.push_error(TypeError::duplicate_discriminant(
                                variant.span,
                                &enum_name,
                                &variant.name,
                                value,
                                *first,
                            ));
                        }
                        used.push((value, variant.span));
                        match value.checked_add(1) {
                            Some(v) => {
                                next = v;
                                next_ok = true;
                            }
                            None => next_ok = false,
                        }
                    }
                    // An implicit variant after an explicit `i64::MAX`
                    // cannot continue; reported once, at the first such
                    // variant.
                    None if !overflow_reported => {
                        self.push_error(TypeError::discriminant_overflow(
                            variant.span,
                            &enum_name,
                            &variant.name,
                        ));
                        overflow_reported = true;
                    }
                    None => {}
                }
                resolved.push(EnumVariantInfo {
                    name: variant.name,
                    // On an erroring enum the fallback value is
                    // meaningless; the error blocks code generation.
                    discriminant: value.unwrap_or(next),
                    payload: variant
                        .payload
                        .as_ref()
                        .map(|ty| self.resolve_variant_payload_type(ty, variant.span)),
                });
            }
            self.types.set_enum_variants(enum_id, resolved);
        }
    }

    /// Resolves a data-carrying variant's declared payload type, reporting
    /// invalid payload kinds. A payload must be a value type with a
    /// deterministic layout: `Ptr<T>` and reference types are rejected
    /// because they do not participate in value semantics (mirroring the
    /// restriction on struct fields); array types are rejected because
    /// their layout is not yet supported inside tagged unions (deferred).
    /// A failed payload resolves to the unknown/error type so independent
    /// problems keep being reported.
    fn resolve_variant_payload_type(&mut self, ty: &Ty, _variant_span: Span) -> TypeId {
        let resolved = self.resolve_type(ty);
        if self.types.is_error(resolved) {
            return resolved;
        }
        match self.types.kind(resolved) {
            Some(
                TypeKind::Ptr(_)
                | TypeKind::Ref { .. }
                | TypeKind::Array { .. }
                | TypeKind::Fn { .. },
            ) => {
                self.push_error(TypeError::invalid_variant_payload(
                    ty.span,
                    self.types.display(resolved),
                ));
                self.types.push(TypeKind::Error)
            }
            _ => resolved,
        }
    }

    /// Validates every registered struct's and enum's layout. Recursive or
    /// oversized aggregates are reported once, at the declaration.
    fn validate_aggregate_layouts(&mut self) {
        let mut layout_errors: Vec<TypeError> = Vec::new();
        for reg in self.structs.values() {
            if let Err(error) = layout::struct_layout(reg.struct_id, &self.types) {
                layout_errors.push(TypeError::invalid_aggregate_layout(
                    reg.span,
                    layout_error_message(&error),
                ));
            }
        }
        for reg in self.enums.values() {
            if let Err(error) = layout::enum_layout(reg.enum_id, &self.types) {
                layout_errors.push(TypeError::invalid_aggregate_layout(
                    reg.span,
                    layout_error_message(&error),
                ));
            }
        }
        for error in layout_errors {
            self.push_error(error);
        }
    }

    /// Resolves a written type (`Ty`) to its type id, reporting unknown
    /// names and invalid array lengths. A failed type resolves to the
    /// unknown/error type so independent problems keep being reported.
    fn resolve_type(&mut self, ty: &Ty) -> TypeId {
        match &ty.kind {
            TyKind::Named(ident) => match ident.name.as_str() {
                "Int" => self.types.push(TypeKind::Int),
                "Float" => self.types.push(TypeKind::Float),
                "Bool" => self.types.push(TypeKind::Bool),
                "Char" => self.types.push(TypeKind::Char),
                "Str" => self.types.push(TypeKind::Str),
                "Null" => self.types.push(TypeKind::Null),
                _ => {
                    if let Some(reg) = self.structs.get(&ident.name) {
                        reg.id
                    } else if let Some(reg) = self.enums.get(&ident.name) {
                        reg.id
                    } else {
                        self.errors
                            .push(TypeError::unknown_type(ident.span, &ident.name));
                        self.types.push(TypeKind::Error)
                    }
                }
            },
            TyKind::Ptr(inner) => {
                let elem = self.resolve_type(inner);
                self.types.push(TypeKind::Ptr(elem))
            }
            TyKind::Ref { mutable, inner } => {
                let elem = self.resolve_type(inner);
                if matches!(
                    self.types.kind(elem),
                    Some(TypeKind::Ref { .. }) | Some(TypeKind::Error)
                ) {
                    // No reference-to-reference types: `&&T` and `&mut &T`
                    // are rejected (reborrowing is deferred; see the
                    // references implementation doc).
                    if !self.types.is_error(elem) {
                        self.push_error(TypeError::invalid_borrow_target(
                            inner.span,
                            format!(
                                "cannot form a reference to `{}`; reference-to-reference types are not supported",
                                self.display(elem)
                            ),
                        ));
                    }
                    self.types.push(TypeKind::Error)
                } else {
                    self.types.push(TypeKind::Ref {
                        mutable: *mutable,
                        elem,
                    })
                }
            }
            TyKind::Array { elem, len } => {
                let elem = self.resolve_type(elem);
                self.array_type(elem, len, ty.span)
            }
            TyKind::Tuple(elements) => {
                let resolved: Vec<TypeId> = elements.iter().map(|e| self.resolve_type(e)).collect();
                if resolved.is_empty() {
                    self.types.push(TypeKind::Unit)
                } else {
                    self.types.push(TypeKind::Tuple(resolved))
                }
            }
        }
    }

    /// Constructs the array type `Array<elem, len>`, decoding and validating
    /// the length literal: it must be a positive integer whose layout fits
    /// the runtime memory model. An invalid length is `E-T16`; an array
    /// whose layout does not fit (recursive element, oversized) is `E-T18`;
    /// both produce the unknown/error type.
    fn array_type(&mut self, elem: TypeId, len: &Expr, span: Span) -> TypeId {
        let Some(len_value) = self.decode_array_len(len) else {
            self.push_error(TypeError::invalid_array_length(
                len.span,
                "the length must be a positive integer literal",
            ));
            return self.types.push(TypeKind::Error);
        };
        if len_value == 0 {
            self.push_error(TypeError::invalid_array_length(
                len.span,
                "the length must be positive",
            ));
            return self.types.push(TypeKind::Error);
        }
        let ty = self.types.push(TypeKind::Array {
            elem,
            len: len_value,
        });
        match layout::array_layout(ty, &self.types) {
            Ok(_) => ty,
            // An empty element is either a genuinely empty struct (reported
            // by `validate_aggregate_layouts` at its own declaration) or a
            // struct whose fields are not resolved yet — `resolve_struct_
            // fields` fills declarations in source order, so a field type
            // may reference a *later* struct whose fields are still empty
            // here. Both cases re-validate after every struct's fields are
            // set, so the eager check defers `Empty` instead of rejecting
            // a valid forward reference.
            Err(LayoutError::Empty { .. }) => ty,
            Err(error) => {
                self.push_error(TypeError::invalid_aggregate_layout(
                    span,
                    layout_error_message(&error),
                ));
                self.types.push(TypeKind::Error)
            }
        }
    }

    /// The value of an array-length literal: a non-negative integer decoded
    /// from its source text (decimal, `0x`/`0o`/`0b`, `_` separators), or
    /// `None` when the literal is not an integer or overflows `u64`.
    fn decode_array_len(&self, len: &Expr) -> Option<u64> {
        if !matches!(len.kind, ExprKind::Int) {
            return None;
        }
        let file = self.sources.get(len.span.file())?;
        let text = file.span_text(len.span)?;
        let (radix, digits) =
            if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                (16u64, rest)
            } else if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
                (8u64, rest)
            } else if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
                (2u64, rest)
            } else {
                (10u64, text)
            };
        let mut value: u64 = 0;
        for byte in digits.bytes() {
            if byte == b'_' {
                continue;
            }
            let digit = match byte {
                b'0'..=b'9' => u64::from(byte - b'0'),
                b'a'..=b'f' => u64::from(byte - b'a' + 10),
                b'A'..=b'F' => u64::from(byte - b'A' + 10),
                _ => return None,
            };
            if digit >= radix {
                return None;
            }
            value = value.checked_mul(radix)?.checked_add(digit)?;
        }
        Some(value)
    }

    fn finish(self) -> TypeResult {
        TypeResult::new(self.errors, self.symbol_types, self.expr_types, self.types)
    }

    // ------------------------------------------------------------------
    // Pre-registration
    // ------------------------------------------------------------------

    /// Gives every declared symbol a type slot before any body is analyzed,
    /// so module-scope order independence and mutual recursion work:
    /// function symbols get a real function type whose parameters and
    /// result are inference variables (or declared types when annotations
    /// are present); every other symbol gets a fresh inference variable
    /// that its declaration or usage later resolves.
    fn pre_register(&mut self) {
        // Function name span start → (arity, param type annotations,
        // return type annotation).  Cloned from the AST so the borrow
        // on `self.ast` is released before the mutable loop.
        type FnInfo = (usize, Vec<Option<Ty>>, Option<Ty>);
        let mut fn_info: HashMap<u32, FnInfo> = HashMap::new();
        for item in &self.ast.items {
            if let ItemKind::Fn(f) = &item.kind {
                let param_tys: Vec<Option<Ty>> = f.params.iter().map(|p| p.ty.clone()).collect();
                fn_info.insert(
                    f.name.span.start(),
                    (f.params.len(), param_tys, f.return_ty.clone()),
                );
            }
        }
        for symbol in self.semantic.symbols().iter() {
            let ty = match symbol.kind {
                SymbolKind::Fn => {
                    let (arity, param_tys, return_ty) = fn_info
                        .get(&symbol.span.start())
                        .cloned()
                        .unwrap_or((0, Vec::new(), None));
                    let params: Vec<TypeId> = (0..arity)
                        .map(|i| {
                            if let Some(ann) = param_tys.get(i).and_then(|t| t.as_ref()) {
                                self.resolve_type(ann)
                            } else {
                                self.types.push(TypeKind::Infer(None))
                            }
                        })
                        .collect();
                    let result = if let Some(ann) = &return_ty {
                        self.resolve_type(ann)
                    } else {
                        self.types.push(TypeKind::Infer(None))
                    };
                    self.types.push(TypeKind::Fn { params, result })
                }
                SymbolKind::Intrinsic => self.intrinsic_type(symbol),
                _ => self.types.push(TypeKind::Infer(None)),
            };
            self.symbol_types[symbol.id.raw() as usize] = ty;
            self.decls.insert(symbol.span.start(), symbol.id);
        }
        // Or-pattern binding aliases (session 27): every occurrence of an
        // or-pattern binding after its first resolves to the same symbol,
        // so each alternative's binding occurrence is typed and unified
        // with the one logical binding.
        for (span, symbol) in self.semantic.binding_aliases() {
            self.decls.insert(span.start(), *symbol);
        }
    }

    /// The concrete function type of a runtime intrinsic, from its
    /// declared signature. Intrinsics are typed concretely — not through
    /// inference variables — so calling `rt_free` produces `Unit` (which
    /// cannot be used as a value), calling `rt_alloc` produces `Ptr<Int>`,
    /// and the string intrinsics produce and consume `Str`.
    fn intrinsic_type(&mut self, symbol: &crate::semantics::Symbol) -> TypeId {
        let intrinsic = crate::runtime::intrinsics::by_name(&symbol.name)
            .expect("intrinsic symbols always have a registered signature");
        let params = intrinsic
            .params
            .iter()
            .map(|param| self.intrinsic_kind_type(*param))
            .collect::<Vec<_>>();
        let result = self.intrinsic_kind_type(intrinsic.result);
        self.types.push(TypeKind::Fn { params, result })
    }

    /// The type id of an [`IntrinsicType`] in this checker's table.
    fn intrinsic_kind_type(&mut self, kind: crate::runtime::intrinsics::IntrinsicType) -> TypeId {
        match kind {
            crate::runtime::intrinsics::IntrinsicType::Int => self.types.push(TypeKind::Int),
            crate::runtime::intrinsics::IntrinsicType::Ptr => {
                let elem = self.types.push(TypeKind::Int);
                self.types.push(TypeKind::Ptr(elem))
            }
            crate::runtime::intrinsics::IntrinsicType::Str => self.types.push(TypeKind::Str),
            crate::runtime::intrinsics::IntrinsicType::Float => self.types.push(TypeKind::Float),
            crate::runtime::intrinsics::IntrinsicType::Char => self.types.push(TypeKind::Char),
            crate::runtime::intrinsics::IntrinsicType::Unit => self.types.push(TypeKind::Unit),
        }
    }

    // ------------------------------------------------------------------
    // Items and statements
    // ------------------------------------------------------------------

    fn check_item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Fn(f) => self.check_fn(f),
            // Struct and enum declarations were registered (types, fields,
            // variants, and layout) before any body was analyzed.
            ItemKind::Struct(_) | ItemKind::Enum(_) => {}
            ItemKind::Let(binding) => {
                let ty = self.expr_type(&binding.init);
                // Session 26: when a type annotation is present, unify the
                // initializer's type with the declared type so mismatches
                // are reported as E-T01.
                if let Some(ann) = &binding.ty {
                    let ann_ty = self.resolve_type(ann);
                    if let Err((expected, actual)) = self.types.unify(ann_ty, ty) {
                        self.push_error(TypeError::mismatch(
                            binding.init.span,
                            self.display(expected),
                            self.display(actual),
                            Some(binding.name.span),
                        ));
                    }
                }
                self.unify_decl(&binding.name, ty, binding.init.span);
            }
            ItemKind::Const(binding) => {
                let ty = self.expr_type(&binding.init);
                if let Some(ann) = &binding.ty {
                    let ann_ty = self.resolve_type(ann);
                    if let Err((expected, actual)) = self.types.unify(ann_ty, ty) {
                        self.push_error(TypeError::mismatch(
                            binding.init.span,
                            self.display(expected),
                            self.display(actual),
                            Some(binding.name.span),
                        ));
                    }
                }
                self.unify_decl(&binding.name, ty, binding.init.span);
            }
        }
    }

    /// Checks a function body against the function's pre-registered
    /// signature: each parameter symbol is linked to its slot in the
    /// function type (so body usage and call-site argument checks share one
    /// variable), `return` expressions unify with the result variable, and
    /// parameter usage constrains the parameter variables.
    fn check_fn(&mut self, f: &FnItem) {
        let Some(fn_ty) = self
            .decls
            .get(&f.name.span.start())
            .copied()
            .and_then(|symbol| self.symbol_types.get(symbol.raw() as usize).copied())
        else {
            self.check_block(&f.body);
            return;
        };
        let (params, result) = match self.types.kind(fn_ty).cloned() {
            Some(TypeKind::Fn { params, result }) => (params, result),
            // Defensive: function symbols always receive a `Fn` type during
            // pre-registration.
            _ => (Vec::new(), self.types.push(TypeKind::Error)),
        };
        for (param, param_slot) in f.params.iter().zip(params) {
            if let Some(&symbol) = self.decls.get(&param.name.span.start()) {
                let var = self.symbol_types[symbol.raw() as usize];
                let _ = self.types.unify(var, param_slot);
            }
        }
        let saved = self.fn_result.replace(result);
        self.check_block(&f.body);
        self.fn_result = saved;
    }

    fn check_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.check_stmt(stmt);
        }
    }

    /// Types a block and returns its value type (the trailing expression's
    /// type, or `Unit` if there is no trailing expression).
    fn check_block_return_type(&mut self, block: &Block) -> TypeId {
        self.check_block(block);
        match &block.result {
            Some(result) => self.expr_type(result),
            None => self.types.push(TypeKind::Unit),
        }
    }

    /// Types an else branch and returns its value type.
    fn check_else_return_type(&mut self, branch: &ElseBranch) -> TypeId {
        match branch {
            ElseBranch::Block(block) => self.check_block_return_type(block),
            ElseBranch::IfExpr(inner) => {
                self.check_condition(&inner.cond);
                let then_ty = self.check_block_return_type(&inner.then_block);
                let else_ty = self.check_else_return_type(&inner.else_branch);
                if let Err((a, b)) = self.types.unify(then_ty, else_ty) {
                    self.push_error(TypeError::mismatch(
                        inner.span,
                        self.display(a),
                        self.display(b),
                        None,
                    ));
                }
                then_ty
            }
            ElseBranch::If(stmt) => {
                self.check_if(stmt);
                self.types.push(TypeKind::Unit)
            }
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Let(binding) => {
                let ty = self.expr_type(&binding.init);
                if let Some(ref pattern) = binding.pattern {
                    // Tuple destructuring (session 31): check the initializer
                    // is a tuple with the right arity and element types.
                    self.check_let_destructure(binding, pattern, ty);
                } else {
                    if let Some(ann) = &binding.ty {
                        let ann_ty = self.resolve_type(ann);
                        if let Err((expected, actual)) = self.types.unify(ann_ty, ty) {
                            self.push_error(TypeError::mismatch(
                                binding.init.span,
                                self.display(expected),
                                self.display(actual),
                                Some(binding.name.span),
                            ));
                        }
                    }
                    self.unify_decl(&binding.name, ty, binding.init.span);
                }
            }
            StmtKind::Const(binding) => {
                let ty = self.expr_type(&binding.init);
                if let Some(ann) = &binding.ty {
                    let ann_ty = self.resolve_type(ann);
                    if let Err((expected, actual)) = self.types.unify(ann_ty, ty) {
                        self.push_error(TypeError::mismatch(
                            binding.init.span,
                            self.display(expected),
                            self.display(actual),
                            Some(binding.name.span),
                        ));
                    }
                }
                self.unify_decl(&binding.name, ty, binding.init.span);
            }
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    let ty = self.expr_type(value);
                    if let Some(result) = self.fn_result {
                        if let Err((expected, actual)) = self.types.unify(result, ty) {
                            self.push_error(TypeError::mismatch(
                                value.span,
                                self.display(expected),
                                self.display(actual),
                                None,
                            ));
                        }
                    }
                }
            }
            StmtKind::Break(value) => {
                if let Some(value) = value {
                    let ty = self.expr_type(value);
                    if let Some(result) = self.loop_result {
                        if let Err((expected, actual)) = self.types.unify(result, ty) {
                            self.push_error(TypeError::mismatch(
                                value.span,
                                self.display(expected),
                                self.display(actual),
                                None,
                            ));
                        }
                    }
                } else if self.loop_result.is_some() {
                    // `break;` inside a loop expression: the loop expects
                    // a break value (E-T36).
                    self.push_error(TypeError::break_value_expected(stmt.span));
                }
            }
            StmtKind::Continue => {}
            StmtKind::If(stmt) => self.check_if(stmt),
            StmtKind::While { cond, body } => {
                self.check_condition(cond);
                self.check_block(body);
            }
            StmtKind::For {
                name,
                iterable,
                body,
            } => {
                let iter_ty = self.expr_type(iterable);
                self.check_for_var(name, iter_ty, iterable.span);
                self.check_block(body);
            }
            StmtKind::Loop(body) => self.check_block(body),
            StmtKind::Match(stmt) => self.check_match(stmt),
            StmtKind::Expr(expr) => {
                self.expr_type(expr);
            }
        }
    }

    /// Types a `match` statement: the scrutinee is typed, every arm's
    /// pattern is checked against it, the match must be exhaustive, and
    /// unreachable arms are rejected (see
    /// `docs/implementation/PATTERN_MATCHING_IMPLEMENTATION.md`).
    fn check_match(&mut self, stmt: &MatchStmt) {
        let scrutinee_ty = self.expr_type(&stmt.scrutinee);
        self.check_match_patterns(stmt, scrutinee_ty);
        for arm in &stmt.arms {
            self.check_block(&arm.body);
        }
    }

    /// Checks every arm of a `match` against the scrutinee type and
    /// verifies exhaustiveness. Shared by the forward pass and the
    /// deferred member re-type pass (which re-checks the arms once a
    /// deferred scrutinee resolves); duplicate diagnostics are deduplicated
    /// by [`Checker::push_error`].
    fn check_match_patterns(&mut self, stmt: &MatchStmt, scrutinee_ty: TypeId) {
        let mut canon = self.types.canonical(scrutinee_ty);
        // The unknown/error type (a root cause reported elsewhere) is
        // silent: its match is not checked, so one root error never
        // cascades into a swarm of match diagnostics.
        if self.types.is_error(canon) {
            return;
        }
        // Only `Int`, `Bool`, and enums are matchable in this milestone;
        // anything else is a single E-T26 and the arms are not checked (the
        // root cause is the scrutinee type, not the arms). An unresolved
        // scrutinee is deferred: its patterns pin its type.
        let matchable = matches!(
            self.types.kind(canon),
            Some(TypeKind::Int | TypeKind::Bool | TypeKind::Enum(_))
        );
        if !matchable && !matches!(self.types.kind(canon), Some(TypeKind::Infer(_))) {
            self.push_error(TypeError::invalid_match_scrutinee(
                stmt.scrutinee.span,
                self.display(canon),
            ));
            return;
        }
        let mut coverage = Coverage::default();
        for arm in &stmt.arms {
            // Re-canonicalize per arm: an earlier refutable pattern may
            // have pinned an unresolved scrutinee to its type.
            canon = self.types.canonical(scrutinee_ty);
            if coverage.all {
                // Every value already matches the earlier `_`/binding arm.
                // The dead arm is not checked, so its own errors never
                // cascade.
                self.push_error(TypeError::unreachable_match_arm(
                    arm.pattern.span(),
                    "this arm can never run: an earlier `_` or binding arm already matches every value",
                ));
                continue;
            }
            // A guarded arm (session 27) never commits coverage: it can
            // still fail, so it neither makes the match exhaustive nor
            // makes later arms unreachable. Whether guarded or not, an arm
            // whose pattern an earlier arm already fully covers can never
            // run (E-T25).
            let guarded = arm.guard.is_some();
            match &arm.pattern {
                Pattern::Or { alternatives, span } => {
                    self.check_or_arm(alternatives, *span, canon, guarded, &mut coverage);
                }
                _ => {
                    let arm_coverage =
                        self.check_arm_pattern(&arm.pattern, canon, arm.pattern.span());
                    if self.coverage_contains(&coverage, &arm_coverage) {
                        self.push_error(TypeError::unreachable_match_arm(
                            arm.pattern.span(),
                            "this arm can never run: an earlier arm already matches the same value",
                        ));
                    } else if !guarded {
                        self.merge_arm_coverage(&mut coverage, arm_coverage);
                    }
                }
            }
            // The guard (session 27) is a boolean condition; a non-Bool
            // guard is E-T01. The pattern's bindings were unified above, so
            // the guard can reference them.
            if let Some(guard) = &arm.guard {
                self.check_condition(guard);
            }
        }
        // Exhaustiveness: after every pattern has pinned the scrutinee,
        // the canonical type determines what a catch-all-free match must
        // cover.
        canon = self.types.canonical(scrutinee_ty);
        if self.types.is_error(canon) || matches!(self.types.kind(canon), Some(TypeKind::Infer(_)))
        {
            return;
        }
        if coverage.all {
            return;
        }
        if !self.coverage_exhaustive(&coverage, canon) {
            self.push_error(TypeError::non_exhaustive_match(
                stmt.span,
                self.exhaustiveness_message(&coverage, canon),
            ));
        }
    }

    /// Checks a data-carrying variant pattern's payload pattern against
    /// the payload type: literal and nested-variant patterns must match the
    /// payload type (`E-T01`), and payload bindings unify with it. Returns
    /// the pattern's coverage of the payload type (`None` when the pattern
    /// is irrefutable — `_` or a binding — so the variant is fully
    /// covered).
    fn check_payload_pattern(
        &mut self,
        pattern: &Pattern,
        payload_ty: TypeId,
        _span: Span,
    ) -> Option<Box<Coverage>> {
        match pattern {
            // An irrefutable payload pattern covers every payload value.
            Pattern::Wildcard { .. } => None,
            Pattern::Binding(name) => {
                self.unify_decl(name, payload_ty, name.span);
                None
            }
            Pattern::Bool { value, span: pspan } => {
                let expected = self.bool_ty();
                match self.types.unify(payload_ty, expected) {
                    Ok(_) => Some(Box::new(Coverage {
                        keys: vec![KeyCover {
                            key: CoverageKey::Bool(*value),
                            sub: None,
                        }],
                        ..Default::default()
                    })),
                    Err((expected, actual)) => {
                        self.push_error(TypeError::mismatch(
                            *pspan,
                            self.display(expected),
                            self.display(actual),
                            None,
                        ));
                        None
                    }
                }
            }
            Pattern::Int {
                negative,
                literal,
                span: pspan,
            } => {
                let expected = self.int_ty();
                match self.types.unify(payload_ty, expected) {
                    Ok(_) => {
                        let value = self
                            .decode_int_literal(literal)
                            .map(|value| {
                                if *negative {
                                    value.wrapping_neg()
                                } else {
                                    value
                                }
                            })
                            .unwrap_or(0);
                        let mut coverage = Coverage::default();
                        Self::insert_interval(&mut coverage, value, value);
                        Some(Box::new(coverage))
                    }
                    Err((expected, actual)) => {
                        self.push_error(TypeError::mismatch(
                            *pspan,
                            self.display(expected),
                            self.display(actual),
                            None,
                        ));
                        None
                    }
                }
            }
            Pattern::Range {
                lo,
                hi,
                inclusive,
                span: pspan,
            } => {
                let expected = self.int_ty();
                match self.types.unify(payload_ty, expected) {
                    Ok(_) => {
                        let mut coverage = Coverage::default();
                        if let (Some(lo), Some(hi)) =
                            (self.pattern_int_value(lo), self.pattern_int_value(hi))
                        {
                            // Exclusive `..` excludes the upper endpoint;
                            // an endpoint at `i64::MIN` cannot be excluded
                            // (nothing lies below it), so the range is
                            // empty. A `lo > hi` range is empty too; an
                            // empty range covers nothing.
                            let hi = if *inclusive {
                                hi
                            } else {
                                match hi.checked_sub(1) {
                                    Some(hi) => hi,
                                    None => return Some(Box::new(coverage)),
                                }
                            };
                            if lo <= hi {
                                Self::insert_interval(&mut coverage, lo, hi);
                            }
                        }
                        Some(Box::new(coverage))
                    }
                    Err((expected, actual)) => {
                        self.push_error(TypeError::mismatch(
                            *pspan,
                            self.display(expected),
                            self.display(actual),
                            None,
                        ));
                        None
                    }
                }
            }
            Pattern::EnumVariant {
                name,
                variant,
                payload,
            } => {
                let pattern_span = pattern.span();
                if let Some(enum_ty) = self.enum_variant_type(name, variant, pattern_span) {
                    match self.types.unify(payload_ty, enum_ty) {
                        Ok(_) => {
                            let declared = self.variant_payload(enum_ty, &variant.name);
                            match (payload.as_deref(), declared) {
                                (Some(inner), None) => {
                                    self.push_error(TypeError::variant_payload_arity(
                                        pattern_span,
                                        format!(
                                            "variant `{}::{}` is a unit variant and cannot carry a payload pattern",
                                            name.name, variant.name
                                        ),
                                    ));
                                    self.check_payload_pattern(inner, enum_ty, pattern_span);
                                    None
                                }
                                (None, Some(_)) => {
                                    self.push_error(TypeError::variant_payload_arity(
                                        pattern_span,
                                        format!(
                                            "variant `{}::{}` carries a payload and must be matched with one: `{}::{}(pattern)`",
                                            name.name, variant.name, name.name, variant.name
                                        ),
                                    ));
                                    None
                                }
                                (None, None) => Some(Box::new(Coverage {
                                    keys: vec![KeyCover {
                                        key: CoverageKey::Variant(variant.name.clone()),
                                        sub: None,
                                    }],
                                    ..Default::default()
                                })),
                                (Some(inner), Some(payload_ty)) => {
                                    let sub =
                                        self.check_payload_pattern(inner, payload_ty, pattern_span);
                                    Some(Box::new(Coverage {
                                        keys: vec![KeyCover {
                                            key: CoverageKey::Variant(variant.name.clone()),
                                            sub,
                                        }],
                                        ..Default::default()
                                    }))
                                }
                            }
                        }
                        Err((expected, actual)) => {
                            self.push_error(TypeError::mismatch(
                                pattern_span,
                                self.display(expected),
                                self.display(actual),
                                None,
                            ));
                            None
                        }
                    }
                } else {
                    None
                }
            }
            // An or-pattern payload (session 27): alternatives must bind
            // the same names (E-T34), the coverage is the union of the
            // alternatives' coverages, and an irrefutable alternative
            // (`_`/binding) makes the whole pattern irrefutable.
            Pattern::Or { alternatives, span } => {
                let mut names: Option<Vec<String>> = None;
                let mut merged = Coverage::default();
                for alternative in alternatives {
                    let alternative_coverage =
                        self.check_payload_pattern(alternative, payload_ty, *span);
                    let alternative_names = Self::pattern_binding_names(alternative);
                    match &names {
                        None => names = Some(alternative_names.clone()),
                        Some(first) => {
                            if *first != alternative_names {
                                self.push_error(TypeError::invalid_or_pattern(
                                    *span,
                                    self.or_pattern_mismatch_message(first, &alternative_names),
                                ));
                            }
                        }
                    }
                    let sub = alternative_coverage?;
                    self.merge_arm_coverage(&mut merged, *sub);
                }
                Some(Box::new(merged))
            }
            // Tuple patterns in payload context: not yet fully supported
            // for exhaustiveness — treat as non-covering for now.
            Pattern::Tuple { .. } => None,
            // Struct patterns in payload context: not yet supported.
            Pattern::Struct { .. } => None,
        }
    }

    /// Checks one match pattern (a whole arm's pattern or one or-pattern
    /// alternative, session 27) against the scrutinee type, reporting its
    /// type errors and unifying its bindings, and returns the coverage it
    /// contributes. A `_`/binding pattern contributes catch-all coverage
    /// (`all`); the caller decides whether the arm commits it (guarded
    /// arms commit nothing). A pattern that does not unify with the
    /// scrutinee contributes no coverage, so one broken pattern never
    /// poisons exhaustiveness.
    fn check_arm_pattern(
        &mut self,
        pattern: &Pattern,
        scrutinee_ty: TypeId,
        _arm_span: Span,
    ) -> Coverage {
        match pattern {
            Pattern::Wildcard { .. } => Coverage {
                all: true,
                ..Default::default()
            },
            Pattern::Binding(name) => {
                // The binding copies the scrutinee's value into the arm's
                // scope; its type is the scrutinee's type.
                self.unify_decl(name, scrutinee_ty, name.span);
                Coverage {
                    all: true,
                    ..Default::default()
                }
            }
            Pattern::Bool { value, span } => {
                let expected = self.bool_ty();
                match self.types.unify(scrutinee_ty, expected) {
                    Ok(_) => Coverage {
                        keys: vec![KeyCover {
                            key: CoverageKey::Bool(*value),
                            sub: None,
                        }],
                        ..Default::default()
                    },
                    Err((expected, actual)) => {
                        self.push_error(TypeError::mismatch(
                            *span,
                            self.display(expected),
                            self.display(actual),
                            None,
                        ));
                        Coverage::default()
                    }
                }
            }
            Pattern::Int {
                negative,
                literal,
                span,
            } => {
                let expected = self.int_ty();
                match self.types.unify(scrutinee_ty, expected) {
                    Ok(_) => {
                        let value = self
                            .decode_int_literal(literal)
                            .map(|value| {
                                if *negative {
                                    value.wrapping_neg()
                                } else {
                                    value
                                }
                            })
                            .unwrap_or(0);
                        let mut coverage = Coverage::default();
                        Self::insert_interval(&mut coverage, value, value);
                        coverage
                    }
                    Err((expected, actual)) => {
                        self.push_error(TypeError::mismatch(
                            *span,
                            self.display(expected),
                            self.display(actual),
                            None,
                        ));
                        Coverage::default()
                    }
                }
            }
            Pattern::Range {
                lo,
                hi,
                inclusive,
                span,
            } => {
                let expected = self.int_ty();
                match self.types.unify(scrutinee_ty, expected) {
                    Ok(_) => {
                        let mut coverage = Coverage::default();
                        if let (Some(lo), Some(hi)) =
                            (self.pattern_int_value(lo), self.pattern_int_value(hi))
                        {
                            // Exclusive `..` excludes the upper endpoint;
                            // an endpoint at `i64::MIN` cannot be excluded
                            // (nothing lies below it), so the range is
                            // empty. A `lo > hi` range is empty too; an
                            // empty range covers nothing.
                            let hi = if *inclusive {
                                hi
                            } else {
                                match hi.checked_sub(1) {
                                    Some(hi) => hi,
                                    None => return coverage,
                                }
                            };
                            if lo <= hi {
                                Self::insert_interval(&mut coverage, lo, hi);
                            }
                        }
                        coverage
                    }
                    Err((expected, actual)) => {
                        self.push_error(TypeError::mismatch(
                            *span,
                            self.display(expected),
                            self.display(actual),
                            None,
                        ));
                        Coverage::default()
                    }
                }
            }
            Pattern::EnumVariant {
                name,
                variant,
                payload,
            } => {
                let pattern_span = pattern.span();
                if let Some(enum_ty) = self.enum_variant_type(name, variant, pattern_span) {
                    match self.types.unify(scrutinee_ty, enum_ty) {
                        Ok(_) => {
                            let declared = self.variant_payload(enum_ty, &variant.name);
                            match (payload.as_deref(), declared) {
                                // A payload attached to a unit variant.
                                (Some(inner), None) => {
                                    self.push_error(TypeError::variant_payload_arity(
                                        pattern_span,
                                        format!(
                                            "variant `{}::{}` is a unit variant and cannot carry a payload pattern",
                                            name.name, variant.name
                                        ),
                                    ));
                                    // The payload pattern is still
                                    // checked so its bindings and
                                    // diagnostics surface.
                                    self.check_payload_pattern(inner, scrutinee_ty, pattern_span);
                                    Coverage::default()
                                }
                                // A data-carrying variant matched
                                // without a payload pattern.
                                (None, Some(_)) => {
                                    self.push_error(TypeError::variant_payload_arity(
                                        pattern_span,
                                        format!(
                                            "variant `{}::{}` carries a payload and must be matched with one: `{}::{}(pattern)`",
                                            name.name, variant.name, name.name, variant.name
                                        ),
                                    ));
                                    Coverage::default()
                                }
                                // Unit variant, no payload: fully covers
                                // the variant.
                                (None, None) => Coverage {
                                    keys: vec![KeyCover {
                                        key: CoverageKey::Variant(variant.name.clone()),
                                        sub: None,
                                    }],
                                    ..Default::default()
                                },
                                // Data-carrying variant with a payload
                                // pattern: the variant is covered by
                                // whatever the payload pattern covers.
                                (Some(inner), Some(payload_ty)) => {
                                    let sub =
                                        self.check_payload_pattern(inner, payload_ty, pattern_span);
                                    Coverage {
                                        keys: vec![KeyCover {
                                            key: CoverageKey::Variant(variant.name.clone()),
                                            sub,
                                        }],
                                        ..Default::default()
                                    }
                                }
                            }
                        }
                        Err((expected, actual)) => {
                            self.push_error(TypeError::mismatch(
                                pattern_span,
                                self.display(expected),
                                self.display(actual),
                                None,
                            ));
                            Coverage::default()
                        }
                    }
                } else {
                    // The variant path failed to resolve (its own
                    // diagnostic was reported): the pattern covers
                    // nothing.
                    Coverage::default()
                }
            }
            // Defensive: the parser flattens top-level or-patterns, but a
            // nested or can reach here through payload recursion.
            Pattern::Or { alternatives, span } => {
                let mut merged = Coverage::default();
                for alternative in alternatives {
                    let alt = self.check_arm_pattern(alternative, scrutinee_ty, *span);
                    self.merge_arm_coverage(&mut merged, alt);
                }
                merged
            }
            // Tuple patterns: not yet fully supported for exhaustiveness.
            Pattern::Tuple { .. } => Coverage::default(),
            // Struct patterns: not yet supported in match arms.
            Pattern::Struct { .. } => Coverage::default(),
        }
    }

    /// Checks an or-pattern arm (session 27): every alternative must bind
    /// exactly the same names (E-T34), and the arm's coverage is the union
    /// of its live alternatives' coverage. An alternative an earlier arm —
    /// or an earlier alternative of this arm — already fully covers can
    /// never match; when every alternative is dead the whole arm can never
    /// run (E-T25). Guarded or-arms commit no coverage.
    fn check_or_arm(
        &mut self,
        alternatives: &[Pattern],
        span: Span,
        scrutinee_ty: TypeId,
        guarded: bool,
        coverage: &mut Coverage,
    ) {
        let mut names: Option<Vec<String>> = None;
        let mut any_alive = false;
        // Alternatives accumulate coverage for the within-arm dead check;
        // the outer `coverage` is only committed to for unguarded arms.
        let mut local = coverage.clone();
        for alternative in alternatives {
            let canon = self.types.canonical(scrutinee_ty);
            let alt = self.check_arm_pattern(alternative, canon, span);
            let alternative_names = Self::pattern_binding_names(alternative);
            match &names {
                None => names = Some(alternative_names.clone()),
                Some(first) => {
                    if *first != alternative_names {
                        self.push_error(TypeError::invalid_or_pattern(
                            span,
                            self.or_pattern_mismatch_message(first, &alternative_names),
                        ));
                    }
                }
            }
            if self.coverage_contains(&local, &alt) {
                continue;
            }
            any_alive = true;
            self.merge_arm_coverage(&mut local, alt);
        }
        if !any_alive {
            self.push_error(TypeError::unreachable_match_arm(
                span,
                "this arm can never run: an earlier arm already matches every value this or-pattern could match",
            ));
        } else if !guarded {
            *coverage = local;
        }
    }

    /// Merges the coverage `new` (produced by one live pattern) into the
    /// accumulated coverage `into`: the union of keys (merging variant
    /// payload sub-coverage recursively), integer intervals, and the
    /// catch-all flag. Callers have already rejected fully-covered
    /// patterns, so the only repeats here are the expected partial
    /// overlaps.
    fn merge_arm_coverage(&mut self, into: &mut Coverage, new: Coverage) {
        if new.all {
            into.all = true;
        }
        for key in new.keys {
            match into.keys.iter_mut().find(|entry| entry.key == key.key) {
                None => into.keys.push(key),
                Some(entry) => match (&mut entry.sub, key.sub) {
                    // Both partial: merge the payload coverages.
                    (Some(prev), Some(new_sub)) => {
                        self.merge_arm_coverage(prev, *new_sub);
                    }
                    // An irrefutable payload pattern completes a partially
                    // covered variant.
                    (Some(_), None) => entry.sub = None,
                    // Accumulated is fully covered: a live alternative
                    // never re-covers it (the dead check rejected it).
                    (None, _) => {}
                },
            }
        }
        for &(lo, hi) in &new.intervals {
            Self::insert_interval(into, lo, hi);
        }
    }

    /// Inserts the inclusive interval `[lo, hi]` into the sorted, disjoint,
    /// adjacent-merged integer interval list of `coverage`.
    fn insert_interval(coverage: &mut Coverage, lo: i64, hi: i64) {
        let mut intervals = std::mem::take(&mut coverage.intervals);
        intervals.push((lo, hi));
        intervals.sort_unstable();
        let mut merged: Vec<(i64, i64)> = Vec::with_capacity(intervals.len());
        for (lo, hi) in intervals {
            if let Some(last) = merged.last_mut() {
                // Adjacent intervals merge because integer coverage is
                // contiguous (`[1, 3]` + `[4, 5]` = `[1, 5]`).
                if lo <= last.1.saturating_add(1) {
                    if hi > last.1 {
                        last.1 = hi;
                    }
                    continue;
                }
            }
            merged.push((lo, hi));
        }
        coverage.intervals = merged;
    }

    /// Whether every integer in `[lo, hi]` is covered by `coverage`'s
    /// intervals: the range-pattern unreachable-arm test (session 27).
    fn interval_covered(&self, coverage: &Coverage, lo: i64, hi: i64) -> bool {
        let mut lo = lo;
        for &(ilo, ihi) in &coverage.intervals {
            if ihi < lo {
                continue;
            }
            if ilo > hi {
                break;
            }
            if ilo <= lo {
                if ihi >= hi {
                    return true;
                }
                lo = ihi + 1;
            } else {
                // A gap between the previous interval and this one.
                return false;
            }
        }
        false
    }

    /// Whether `coverage`'s intervals cover the entire `Int` domain
    /// (`i64::MIN..=i64::MAX`), making an `Int` match exhaustive without a
    /// catch-all (session 27).
    fn intervals_cover_domain(&self, coverage: &Coverage) -> bool {
        let mut covered_to = i64::MIN;
        for (index, &(lo, hi)) in coverage.intervals.iter().enumerate() {
            if index == 0 {
                if lo != i64::MIN {
                    return false;
                }
            } else if lo > covered_to.saturating_add(1) {
                return false;
            }
            if hi > covered_to {
                covered_to = hi;
            }
            if covered_to == i64::MAX {
                return true;
            }
        }
        false
    }

    /// Whether `acc` covers every value `cand` covers: the unreachable-arm
    /// test. A catch-all (`all`) covers everything; a candidate with no
    /// coverage (an empty range, a mismatched pattern) is vacuously
    /// covered.
    fn coverage_contains(&self, acc: &Coverage, cand: &Coverage) -> bool {
        if cand.all {
            return acc.all;
        }
        if acc.all {
            return true;
        }
        for key in &cand.keys {
            let Some(entry) = acc.keys.iter().find(|entry| entry.key == key.key) else {
                return false;
            };
            match (&entry.sub, &key.sub) {
                // Accumulated fully covers the value class: contained.
                (None, _) => {}
                // Accumulated is partial; the candidate covers the whole
                // value class: not contained.
                (Some(_), None) => return false,
                // Both partial: the candidate's payload coverage must be
                // contained in the accumulated payload coverage.
                (Some(prev), Some(cand_sub)) => {
                    if !self.coverage_contains(prev, cand_sub) {
                        return false;
                    }
                }
            }
        }
        for &(lo, hi) in &cand.intervals {
            if !self.interval_covered(acc, lo, hi) {
                return false;
            }
        }
        true
    }

    /// The distinct binding names a pattern introduces, sorted, for the
    /// or-pattern consistency rule: every alternative must bind the same
    /// names (E-T34).
    fn pattern_binding_names(pattern: &Pattern) -> Vec<String> {
        let mut names = Vec::new();
        Self::collect_pattern_names(pattern, &mut names);
        names.sort();
        names.dedup();
        names
    }

    /// Collects the binding names a pattern introduces, recursively.
    fn collect_pattern_names(pattern: &Pattern, out: &mut Vec<String>) {
        match pattern {
            Pattern::Binding(name) => out.push(name.name.clone()),
            Pattern::EnumVariant {
                payload: Some(inner),
                ..
            } => Self::collect_pattern_names(inner, out),
            Pattern::Or { alternatives, .. } => {
                for alternative in alternatives {
                    Self::collect_pattern_names(alternative, out);
                }
            }
            Pattern::Wildcard { .. }
            | Pattern::Bool { .. }
            | Pattern::Int { .. }
            | Pattern::Range { .. }
            | Pattern::EnumVariant { payload: None, .. } => {}
            Pattern::Tuple { elements, .. } => {
                for elem in elements {
                    Self::collect_pattern_names(elem, out);
                }
            }
            Pattern::Struct { fields, .. } => {
                for field in fields {
                    match &field.binding {
                        Some(inner) => Self::collect_pattern_names(inner, out),
                        None => out.push(field.name.name.clone()),
                    }
                }
            }
        }
    }

    /// Renders the E-T34 message for or-pattern alternatives binding
    /// different name sets: the names present in one set but not the
    /// other.
    fn or_pattern_mismatch_message(&self, first: &[String], second: &[String]) -> String {
        let mut odd: Vec<&str> = first
            .iter()
            .filter(|name| !second.contains(name))
            .map(|name| name.as_str())
            .collect();
        odd.extend(
            second
                .iter()
                .filter(|name| !first.contains(name))
                .map(|name| name.as_str()),
        );
        odd.sort_unstable();
        format!(
            "or-pattern alternatives must bind the same names: `{}` {} not bound in every alternative",
            odd.join("`, `"),
            if odd.len() == 1 { "is" } else { "are" },
        )
    }

    /// Decodes a range-pattern endpoint (an `Int` pattern, possibly
    /// negated) to its 64-bit value.
    fn pattern_int_value(&self, pattern: &Pattern) -> Option<i64> {
        match pattern {
            Pattern::Int {
                negative, literal, ..
            } => self.decode_int_literal(literal).map(|value| {
                if *negative {
                    value.wrapping_neg()
                } else {
                    value
                }
            }),
            _ => None,
        }
    }

    /// Whether `coverage` covers every value of type `ty`. A catch-all
    /// covers everything; otherwise a `Bool` needs both literals, an `Int`
    /// can never be exhausted without a catch-all, an enum needs every
    /// variant fully covered (a partially covered variant needs its own
    /// payload coverage exhaustive), and every other type can only be
    /// covered by a catch-all.
    fn coverage_exhaustive(&self, coverage: &Coverage, ty: TypeId) -> bool {
        if coverage.all {
            return true;
        }
        match self.types.kind(ty) {
            Some(TypeKind::Bool) => {
                let has_true = coverage
                    .keys
                    .iter()
                    .any(|entry| matches!(entry.key, CoverageKey::Bool(true)));
                let has_false = coverage
                    .keys
                    .iter()
                    .any(|entry| matches!(entry.key, CoverageKey::Bool(false)));
                has_true && has_false
            }
            // An `Int` match is exhaustive without a catch-all only when
            // the covered intervals span the whole domain (session 27:
            // `i64::MIN..=i64::MAX`); every partial coverage needs a
            // catch-all.
            Some(TypeKind::Int) => self.intervals_cover_domain(coverage),
            Some(TypeKind::Enum(id)) => self
                .types
                .enum_info(*id)
                .map(|info| {
                    info.variants.iter().all(|variant| {
                        let Some(entry) = coverage
                            .keys
                            .iter()
                            .find(|entry| {
                                matches!(&entry.key, CoverageKey::Variant(name) if name == &variant.name)
                            })
                        else {
                            return false;
                        };
                        match &entry.sub {
                            None => true,
                            Some(sub) => match variant.payload {
                                None => false,
                                Some(payload_ty) => self.coverage_exhaustive(sub, payload_ty),
                            },
                        }
                    })
                })
                .unwrap_or(true),
            _ => false,
        }
    }

    /// Renders the missing coverage of `coverage` for `ty` into the
    /// non-exhaustive-match message (`E-T24`), mirroring the existing
    /// phrasing for integer, boolean, and enum scrutinees.
    fn exhaustiveness_message(&self, coverage: &Coverage, ty: TypeId) -> String {
        match self.types.kind(ty) {
            Some(TypeKind::Int) => {
                "the match is not exhaustive: integer values cannot all be listed; add a `_` or binding arm".to_string()
            }
            Some(TypeKind::Bool) => {
                "the match is not exhaustive: a `Bool` match must cover both `true` and `false`, or add a `_` or binding arm".to_string()
            }
            Some(TypeKind::Enum(id)) => {
                let missing = self
                    .types
                    .enum_info(*id)
                    .map(|info| {
                        info.variants
                            .iter()
                            .filter(|variant| {
                                let covered = coverage.keys.iter().find(|entry| {
                                    matches!(&entry.key, CoverageKey::Variant(name) if name == &variant.name)
                                });
                                match covered {
                                    None => true,
                                    Some(entry) => match &entry.sub {
                                        None => false,
                                        // A partially covered variant is
                                        // missing only when its payload
                                        // coverage is not exhaustive.
                                        Some(sub) => {
                                            !variant
                                                .payload
                                                .map(|payload_ty| {
                                                    self.coverage_exhaustive(sub, payload_ty)
                                                })
                                                .unwrap_or(false)
                                        }
                                    },
                                }
                            })
                            .map(|variant| {
                                if variant.payload.is_some() {
                                    format!("`{}` (with a matching payload pattern)", variant.name)
                                } else {
                                    format!("`{}`", variant.name)
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                format!(
                    "the match is not exhaustive: the variant{} {} {} not covered; add a `_` or binding arm or cover every variant",
                    if missing.len() == 1 { "" } else { "s" },
                    missing.join(", "),
                    if missing.len() == 1 { "is" } else { "are" },
                )
            }
            _ => "the match is not exhaustive: add a `_` or binding arm".to_string(),
        }
    }

    fn check_if(&mut self, stmt: &IfStmt) {
        self.check_condition(&stmt.cond);
        self.check_block(&stmt.then_block);
        match &stmt.else_branch {
            Some(ElseBranch::If(nested)) => self.check_if(nested),
            Some(ElseBranch::IfExpr(inner)) => {
                self.check_condition(&inner.cond);
                self.check_block(&inner.then_block);
                match &inner.else_branch {
                    ElseBranch::IfExpr(e) => {
                        self.check_condition(&e.cond);
                        self.check_block(&e.then_block);
                    }
                    ElseBranch::Block(b) => self.check_block(b),
                    _ => {}
                }
            }
            Some(ElseBranch::Block(block)) => self.check_block(block),
            None => {}
        }
    }

    /// Types a `for` loop variable from the iterable's element type. Only
    /// ranges are iterable at this stage. An unconstrained iterable is
    /// pinned to `Range<T>` with a fresh element variable (bidirectional
    /// checking: `for` imposes the expected type `Range<_>`), so its type
    /// is determined by the first real constraint; unknown/error iterables
    /// defer silently — their root cause is reported elsewhere.
    fn check_for_var(&mut self, name: &Ident, iter_ty: TypeId, span: Span) {
        let Some(symbol) = self.decls.get(&name.span.start()).copied() else {
            return;
        };
        let var = self.symbol_types[symbol.raw() as usize];
        let canon = self.types.canonical(iter_ty);
        match self.types.kind(canon) {
            Some(TypeKind::Range(elem)) => {
                let _ = self.types.unify(var, *elem);
            }
            Some(TypeKind::Infer(_)) => {
                let elem = self.types.push(TypeKind::Infer(None));
                let range = self.types.push(TypeKind::Range(elem));
                let _ = self.types.unify(canon, range);
                let _ = self.types.unify(var, elem);
            }
            Some(TypeKind::Error) | None => {}
            Some(_) => {
                self.errors
                    .push(TypeError::not_iterable(span, self.display(canon)));
            }
        }
    }

    /// Unifies a declaration's pre-registered type with its initializer's
    /// type. The declaration variable is fresh, so this normally cannot
    /// fail; the error path is defensive.
    fn unify_decl(&mut self, name: &Ident, ty: TypeId, span: Span) {
        let Some(symbol) = self.decls.get(&name.span.start()).copied() else {
            return;
        };
        let var = self.symbol_types[symbol.raw() as usize];
        if let Err((expected, actual)) = self.types.unify(var, ty) {
            self.push_error(TypeError::mismatch(
                span,
                self.display(expected),
                self.display(actual),
                None,
            ));
        }
    }

    /// Checks a tuple destructuring let binding (session 31).
    ///
    /// Verifies that the initializer is a tuple with the correct arity,
    /// then unifies each element pattern's binding with the corresponding
    /// tuple element type.
    fn check_let_destructure(
        &mut self,
        binding: &crate::ast::LetItem,
        pattern: &Pattern,
        init_ty: TypeId,
    ) {
        // Check for optional type annotation first.
        if let Some(ann) = &binding.ty {
            let ann_ty = self.resolve_type(ann);
            if let Err((expected, actual)) = self.types.unify(ann_ty, init_ty) {
                self.push_error(TypeError::mismatch(
                    binding.init.span,
                    self.display(expected),
                    self.display(actual),
                    Some(binding.name.span),
                ));
            }
        }

        match pattern {
            Pattern::Tuple { elements, .. } => {
                self.check_tuple_destructure(elements, init_ty, pattern.span());
            }
            Pattern::Struct {
                name, fields, span, ..
            } => {
                self.check_struct_destructure(name, fields, init_ty, *span);
            }
            _ => {
                self.push_error(TypeError::cannot_destructure(
                    pattern.span(),
                    &self.display(init_ty),
                ));
            }
        }
    }

    /// Checks a tuple destructuring pattern against a tuple type.
    fn check_tuple_destructure(&mut self, elements: &[Pattern], init_ty: TypeId, pat_span: Span) {
        // The initializer must be a tuple type.
        let Some(elem_tys) = self.types.tuple_elems(init_ty).map(|s| s.to_vec()) else {
            self.push_error(TypeError::cannot_destructure(
                pat_span,
                &self.display(init_ty),
            ));
            return;
        };

        // Arity check.
        if elements.len() != elem_tys.len() {
            self.push_error(TypeError::destructure_arity_mismatch(
                pat_span,
                elem_tys.len(),
                elements.len(),
            ));
            return;
        }

        // Unify each element pattern's binding with the tuple element type.
        for (elem_pat, &elem_ty) in elements.iter().zip(elem_tys.iter()) {
            self.check_destructure_pattern_binding(elem_pat, elem_ty);
        }
    }

    /// Checks a struct destructuring pattern against a struct type.
    ///
    /// Validates that the initializer is a struct type, that every field
    /// in the pattern exists on the struct (`E-T39`), that no declared
    /// field is omitted (`E-T40`), and that each field binding unifies
    /// with the corresponding struct field's type.
    fn check_struct_destructure(
        &mut self,
        name: &Ident,
        fields: &[crate::ast::StructPatternField],
        init_ty: TypeId,
        pat_span: Span,
    ) {
        // The initializer must be a struct type.
        let canonical = self.types.canonical(init_ty);
        let struct_id = match self.types.kind(canonical) {
            Some(TypeKind::Struct(id)) => *id,
            _ => {
                self.push_error(TypeError::cannot_destructure(
                    pat_span,
                    &self.display(init_ty),
                ));
                return;
            }
        };

        // Resolve the struct's declared fields.
        let struct_info = match self.types.struct_info(struct_id) {
            Some(info) => info.clone(),
            _ => {
                self.push_error(TypeError::cannot_destructure(
                    pat_span,
                    &self.display(init_ty),
                ));
                return;
            }
        };

        // Validate the struct type name in the pattern matches the
        // initializer's struct type.
        if struct_info.name != name.name {
            self.push_error(TypeError::struct_pattern_type_mismatch(
                name.span,
                &name.name,
                &self.display(init_ty),
            ));
            return;
        }

        // Check that every field in the pattern exists in the struct.
        for field in fields {
            if !struct_info.fields.iter().any(|f| f.name == field.name.name) {
                self.push_error(TypeError::unknown_struct_field_in_pattern(
                    field.name.span,
                    &field.name.name,
                    &struct_info.name,
                ));
            }
        }

        // Check that every declared field is present in the pattern.
        for declared_field in &struct_info.fields {
            if !fields.iter().any(|f| f.name.name == declared_field.name) {
                self.push_error(TypeError::missing_struct_field_in_pattern(
                    pat_span,
                    &declared_field.name,
                    &struct_info.name,
                ));
            }
        }

        // Unify each field pattern's binding with the struct field's type.
        for field in fields {
            if let Some(field_info) = struct_info
                .fields
                .iter()
                .find(|f| f.name == field.name.name)
            {
                let field_ty = field_info.ty;
                match &field.binding {
                    Some(inner) => {
                        self.check_destructure_pattern_binding(inner, field_ty);
                    }
                    None => {
                        // Shorthand: bind to the field name.
                        self.unify_decl(&field.name, field_ty, field.name.span);
                    }
                }
            }
        }
    }

    /// Recursively checks a single pattern inside a destructuring let,
    /// unifying bindings with the expected type.
    fn check_destructure_pattern_binding(&mut self, pattern: &Pattern, expected_ty: TypeId) {
        match pattern {
            Pattern::Binding(name) => {
                self.unify_decl(name, expected_ty, name.span);
            }
            Pattern::Wildcard { .. } => {}
            Pattern::Tuple { elements, span, .. } => {
                // Nested tuple destructuring.
                let Some(inner_elems) = self.types.tuple_elems(expected_ty).map(|s| s.to_vec())
                else {
                    self.push_error(TypeError::cannot_destructure(
                        *span,
                        &self.display(expected_ty),
                    ));
                    return;
                };
                if elements.len() != inner_elems.len() {
                    self.push_error(TypeError::destructure_arity_mismatch(
                        *span,
                        inner_elems.len(),
                        elements.len(),
                    ));
                    return;
                }
                for (elem_pat, &elem_ty) in elements.iter().zip(inner_elems.iter()) {
                    self.check_destructure_pattern_binding(elem_pat, elem_ty);
                }
            }
            Pattern::Struct {
                name, fields, span, ..
            } => {
                // Nested struct destructuring.
                self.check_struct_destructure(name, fields, expected_ty, *span);
            }
            // Literal and enum-variant patterns in let destructuring
            // are rejected — they have no binding to unify.
            other => {
                self.push_error(TypeError::cannot_destructure(
                    other.span(),
                    &self.display(expected_ty),
                ));
            }
        }
    }

    /// Checks a condition expression against the expected type `Bool`.
    ///
    /// This is the bidirectional direction of the checker: the expected
    /// type flows down into the expression, so an unconstrained condition
    /// is pinned to `Bool` (and its chain of inference variables with it)
    /// instead of leaking as unresolved. Error-typed conditions stay
    /// silently unknown — their root cause is reported elsewhere.
    fn check_condition(&mut self, cond: &Expr) {
        let expected = self.bool_ty();
        let _ = self.check_expr_against(cond, expected);
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    /// Types `expr`, records its type under its span, and returns it.
    fn expr_type(&mut self, expr: &Expr) -> TypeId {
        let ty = self.check_expr(expr);
        self.expr_types.push((expr.span, ty));
        ty
    }

    fn check_expr(&mut self, expr: &Expr) -> TypeId {
        match &expr.kind {
            ExprKind::Int => self.types.push(TypeKind::Int),
            ExprKind::Float => self.types.push(TypeKind::Float),
            ExprKind::Str => self.types.push(TypeKind::Str),
            ExprKind::Char => self.types.push(TypeKind::Char),
            ExprKind::Bool(_) => self.types.push(TypeKind::Bool),
            ExprKind::Null => self.types.push(TypeKind::Null),
            ExprKind::Ident(ident) => match self.semantic.resolve(ident.span) {
                Some(symbol) => self.symbol_types[symbol.raw() as usize],
                None => self.types.push(TypeKind::Error),
            },
            ExprKind::Unary { op, operand } => {
                let operand_ty = self.expr_type(operand);
                self.check_unary(*op, operand_ty, expr.span)
            }
            ExprKind::Borrow { mutable, operand } => {
                let operand_ty = self.expr_type(operand);
                let canon = self.types.canonical(operand_ty);
                if matches!(self.types.kind(canon), Some(TypeKind::Infer(_))) {
                    // Deferred: the operand is an unresolved inference
                    // variable (a function parameter not yet pinned by a
                    // call site); re-typed by
                    // `resolve_deferred_members` once it resolves.
                    self.deferred.push(expr.span);
                    self.types.push(TypeKind::Infer(None))
                } else {
                    self.check_borrow(*mutable, operand, operand_ty, expr.span)
                }
            }
            ExprKind::Deref { operand } => {
                let operand_ty = self.expr_type(operand);
                let canon = self.types.canonical(operand_ty);
                if matches!(self.types.kind(canon), Some(TypeKind::Infer(_))) {
                    // Deferred: the operand is an unresolved inference
                    // variable; re-typed once it resolves.
                    self.deferred.push(expr.span);
                    self.types.push(TypeKind::Infer(None))
                } else {
                    self.check_deref(operand_ty, expr.span)
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let lhs_ty = self.expr_type(lhs);
                let rhs_ty = self.expr_type(rhs);
                self.check_binary(*op, lhs_ty, rhs_ty, expr.span)
            }
            ExprKind::Assign { op, target, value } => {
                let value_ty = self.expr_type(value);
                self.check_assign(*op, target, value_ty, value.span, expr.span)
            }
            ExprKind::Range { start, end, .. } => {
                let start_ty = self.expr_type(start);
                let end_ty = self.expr_type(end);
                self.check_range(start_ty, end_ty, expr.span)
            }
            ExprKind::Call { callee, args } => {
                let callee_ty = self.expr_type(callee);
                self.check_call(callee, callee_ty, args, expr.span)
            }
            ExprKind::Member { base, member } => {
                let base_ty = self.expr_type(base);
                let canon = self.types.canonical(base_ty);
                if self.types.is_error(canon) {
                    self.types.push(TypeKind::Error)
                } else if matches!(self.types.kind(canon), Some(TypeKind::Infer(_))) {
                    // Deferred: nothing is known about the base yet; the
                    // fresh variable is re-typed by
                    // `resolve_deferred_members` once the base resolves.
                    self.deferred.push(expr.span);
                    self.types.push(TypeKind::Infer(None))
                } else {
                    self.check_member(canon, member)
                }
            }
            ExprKind::Index { base, index } => {
                let base_ty = self.expr_type(base);
                let index_ty = self.expr_type(index);
                let canon = self.types.canonical(base_ty);
                if self.types.is_error(canon) {
                    self.types.push(TypeKind::Error)
                } else if matches!(self.types.kind(canon), Some(TypeKind::Infer(_))) {
                    self.deferred.push(expr.span);
                    self.types.push(TypeKind::Infer(None))
                } else {
                    self.check_index(canon, index, index_ty, expr.span)
                }
            }
            ExprKind::StructLit { name, fields } => {
                self.check_struct_literal(name, fields, expr.span)
            }
            ExprKind::ArrayLit(elems) => self.check_array_literal(elems, expr.span),
            ExprKind::EnumVariant {
                name,
                variant,
                payload,
            } => self.check_enum_variant(name, variant, payload, expr.span),
            ExprKind::Group(inner) => self.expr_type(inner),
            ExprKind::IfExpr(inner) => {
                self.check_condition(&inner.cond);
                let then_ty = self.check_block_return_type(&inner.then_block);
                let else_ty = self.check_else_return_type(&inner.else_branch);
                if let Err((a, b)) = self.types.unify(then_ty, else_ty) {
                    self.push_error(TypeError::mismatch(
                        inner.span,
                        self.display(a),
                        self.display(b),
                        None,
                    ));
                }
                then_ty
            }
            ExprKind::Block(block) => {
                self.check_block(block);
                match &block.result {
                    Some(result) => self.expr_type(result),
                    None => self.types.push(TypeKind::Unit),
                }
            }
            ExprKind::Tuple(elems) => {
                let elem_tys: Vec<TypeId> = elems.iter().map(|e| self.expr_type(e)).collect();
                self.types.push(TypeKind::Tuple(elem_tys))
            }
            ExprKind::TupleFieldAccess { base, index } => {
                self.check_tuple_field_access(base, index, expr.span)
            }
            ExprKind::WhileExpr { cond, body, .. } => {
                // Session 30: while-expression. A fresh inference variable
                // is the loop's break-value type; `break expr;` inside the
                // body constrains it.
                self.check_condition(cond);
                let result_var = self.types.push(TypeKind::Infer(None));
                let saved = self.loop_result.replace(result_var);
                self.check_block(body);
                self.loop_result = saved;
                result_var
            }
            ExprKind::LoopExpr { body, .. } => {
                // Session 30: loop-expression. Same as while-expression but
                // without a condition.
                let result_var = self.types.push(TypeKind::Infer(None));
                let saved = self.loop_result.replace(result_var);
                self.check_block(body);
                self.loop_result = saved;
                result_var
            }
        }
    }

    /// Types `expr` and then unifies its type with the expected type
    /// `expected`, returning the unified type.
    ///
    /// This is the bidirectional checking mechanism of the checker: type
    /// information flows down from the context (the expected type) as well
    /// as up from the expression. An unconstrained expression adopts
    /// `expected`; a conflicting concrete type is a mismatch diagnostic
    /// (expected type, actual type, exact expression span).
    /// Types `expr` and then unifies its type with the expected type
    /// `expected`, returning the unified type.
    ///
    /// This is the bidirectional checking mechanism of the checker: type
    /// information flows down from the context (the expected type) as well
    /// as up from the expression. An unconstrained expression adopts
    /// `expected`; a conflicting concrete type is a mismatch diagnostic
    /// (expected type, actual type, exact expression span).
    ///
    /// On conflict the freshly pushed error type is returned, but the
    /// expression's recorded type (see [`Checker::expr_type`]) stays the
    /// actual concrete type: the mismatch diagnostic already marks the
    /// program invalid, so later stages see the error through
    /// [`TypeResult::has_errors`].
    fn check_expr_against(&mut self, expr: &Expr, expected: TypeId) -> TypeId {
        let actual = self.expr_type(expr);
        match self.types.unify(actual, expected) {
            Ok(ty) => ty,
            Err(_) => {
                self.push_error(TypeError::mismatch(
                    expr.span,
                    self.display(expected),
                    self.display(actual),
                    None,
                ));
                self.types.push(TypeKind::Error)
            }
        }
    }

    /// Types a prefix unary operation. `-` requires a numeric operand,
    /// `!` a boolean one, and `~` an integer one; the result type is the
    /// operand type.
    ///
    /// `!` and `~` pin an unconstrained operand to `Bool`/`Int` (their
    /// result type uniquely determines the operand type); `-` cannot pin,
    /// since both `Int` and `Float` are valid, so its unconstrained
    /// operand stays unresolved until another constraint decides.
    fn check_unary(&mut self, op: UnaryOp, operand_ty: TypeId, span: Span) -> TypeId {
        let canon = self.types.canonical(operand_ty);
        if self.types.is_error(canon) {
            return self.types.push(TypeKind::Error);
        }
        match op {
            UnaryOp::Neg => {
                if self.is_numeric(canon) {
                    canon
                } else if matches!(self.types.kind(canon), Some(TypeKind::Infer(_))) {
                    // Numeric disjunction: cannot pin, defer honestly.
                    canon
                } else {
                    self.unary_error(op, canon, span)
                }
            }
            UnaryOp::Not => {
                let expected = self.bool_ty();
                match self.types.unify(canon, expected) {
                    Ok(_) => expected,
                    Err(_) => self.unary_error(op, canon, span),
                }
            }
            UnaryOp::BitNot => {
                let expected = self.int_ty();
                match self.types.unify(canon, expected) {
                    Ok(_) => expected,
                    Err(_) => self.unary_error(op, canon, span),
                }
            }
        }
    }

    /// Types a borrow expression: `&operand` (shared, `mutable: false`)
    /// or `&mut operand` (exclusive, `mutable: true`). The operand must be
    /// a borrowable place (not a reference, not a deref, not a value):
    /// violations are `E-T19`. The result is the corresponding reference
    /// type; borrow *conflicts* are the ownership stage's concern
    /// (`E-S12`/`E-S13`).
    fn check_borrow(
        &mut self,
        mutable: bool,
        operand: &Expr,
        operand_ty: TypeId,
        span: Span,
    ) -> TypeId {
        let canon = self.types.canonical(operand_ty);
        if self.types.is_error(canon) {
            return self.types.push(TypeKind::Error);
        }
        if matches!(self.types.kind(canon), Some(TypeKind::Ref { .. })) {
            self.push_error(TypeError::invalid_borrow_target(
                span,
                format!(
                    "cannot borrow `{}`: it is already a reference (no reference-to-reference types)",
                    self.display(canon)
                ),
            ));
            return self.types.push(TypeKind::Error);
        }
        if matches!(self.types.kind(canon), Some(TypeKind::Enum(_))) {
            // Session 16's reference model predates enums and does not
            // include them as reference element types (the referent byte
            // size is fixed at 8 in the backend, which would corrupt a
            // tagged-union deref). Rejecting at the type level keeps the
            // error a clean front-end diagnostic instead of an internal
            // backend error (E-B07).
            self.push_error(TypeError::invalid_borrow_target(
                span,
                format!(
                    "cannot borrow a value of type `{}`: references to enums are not supported",
                    self.display(canon)
                ),
            ));
            return self.types.push(TypeKind::Error);
        }
        if !self.is_borrowable_place(operand) {
            self.push_error(TypeError::invalid_borrow_target(
                span,
                "cannot borrow a non-place value; borrow a variable, member, or element"
                    .to_string(),
            ));
            return self.types.push(TypeKind::Error);
        }
        self.types.push(TypeKind::Ref {
            mutable,
            elem: canon,
        })
    }

    /// Whether the place `expr` is rooted at a dereference somewhere in
    /// its member/index chain (e.g. `(*r).x`, `(*r)[i]`). Reads through
    /// such places are supported (they copy the dereferenced value), but
    /// assignment through them is not part of the reference model (E-T33).
    fn is_deref_rooted(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Member { base, .. } | ExprKind::Index { base, .. } => {
                self.is_deref_rooted(base)
            }
            // `(*r).x`: the parser wraps the deref in a group.
            ExprKind::Group(inner) => self.is_deref_rooted(inner),
            ExprKind::Deref { .. } => true,
            _ => false,
        }
    }

    /// Whether `expr` is a borrowable place: an identifier, a member or
    /// index chain rooted at one, or a group of one. Deref-rooted places
    /// (`*r`, `(*r).x`) are deliberately excluded this session
    /// (reborrowing is deferred — E-T19).
    fn is_borrowable_place(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Ident(_) => true,
            ExprKind::Member { base, .. } | ExprKind::Index { base, .. } => {
                self.is_borrowable_place(base)
            }
            ExprKind::Group(inner) => self.is_borrowable_place(inner),
            _ => false,
        }
    }

    /// Types a dereference: `*operand`. The operand must be a reference
    /// (`&T` or `&mut T`); the result is the referent type. Dereferencing a
    /// non-reference is `E-T20`.
    fn check_deref(&mut self, operand_ty: TypeId, span: Span) -> TypeId {
        let canon = self.types.canonical(operand_ty);
        match self.types.kind(canon) {
            Some(TypeKind::Ref { elem, .. }) => *elem,
            Some(TypeKind::Error) | None => self.types.push(TypeKind::Error),
            _ => {
                self.push_error(TypeError::deref_non_reference(span, self.display(canon)));
                self.types.push(TypeKind::Error)
            }
        }
    }

    /// Reports an invalid unary operand combination and returns the
    /// poisoned error type.
    fn unary_error(&mut self, op: UnaryOp, operand_ty: TypeId, span: Span) -> TypeId {
        self.push_error(TypeError::invalid_operator(
            span,
            op.symbol(),
            format!("type `{}`", self.display(operand_ty)),
        ));
        self.types.push(TypeKind::Error)
    }

    /// Types an infix binary operation, reporting incompatible operand
    /// combinations. Operands with the unknown/error type poison the result
    /// silently: the root cause is reported elsewhere.
    fn check_binary(&mut self, op: BinaryOp, lhs: TypeId, rhs: TypeId, span: Span) -> TypeId {
        let l = self.types.canonical(lhs);
        let r = self.types.canonical(rhs);
        if self.types.is_error(l) || self.types.is_error(r) {
            return self.types.push(TypeKind::Error);
        }
        // A tagged-union enum (one with a data-carrying variant, session
        // 19) cannot be compared with `==`/`!=`: comparing payloads is not
        // defined in this milestone (`E-T30`). Unit-only enums keep the
        // session-17 discriminant comparison.
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne)
            && matches!(self.types.kind(l), Some(TypeKind::Enum(_)))
            && matches!(self.types.kind(r), Some(TypeKind::Enum(_)))
        {
            let tagged = self
                .tagged_enum_name(l)
                .or_else(|| self.tagged_enum_name(r));
            if let Some(name) = tagged {
                self.push_error(TypeError::enum_equality(span, name));
                return self.types.push(TypeKind::Error);
            }
        }
        match self.binary_rule(op, l, r) {
            Some(ty) => ty,
            None => self.emit_operator_error(op.symbol(), l, r, span),
        }
    }

    /// The name of `ty` if it denotes an enum with at least one
    /// data-carrying variant (a tagged union, session 19); `None` for a
    /// unit-only enum or any non-enum type.
    fn tagged_enum_name(&self, ty: TypeId) -> Option<String> {
        let enum_id = self.types.enum_id(ty)?;
        self.types.enum_info(enum_id).and_then(|info| {
            if info
                .variants
                .iter()
                .any(|variant| variant.payload.is_some())
            {
                Some(info.name.clone())
            } else {
                None
            }
        })
    }

    /// Reports an invalid binary operand combination and returns the
    /// poisoned error type. `symbol` is the operator as written, so a
    /// compound assignment reports its own symbol (`+=`) rather than the
    /// underlying binary operator (`+`).
    fn emit_operator_error(&mut self, symbol: &str, l: TypeId, r: TypeId, span: Span) -> TypeId {
        self.push_error(TypeError::invalid_operator(
            span,
            symbol,
            format!("types `{}` and `{}`", self.display(l), self.display(r)),
        ));
        self.types.push(TypeKind::Error)
    }

    /// The type an infix `op` produces for canonical operands `l` and `r`,
    /// or `None` when the combination is invalid.
    ///
    /// An unconstrained operand adopts the constraint the other operand
    /// and operator impose (the minimal inference the current language
    /// requires). Two unconstrained operands are linked to each other and
    /// produce the operator's result type: the linked variable for
    /// operand-typed categories (arithmetic, shift, bitwise) and `Bool` for
    /// the boolean-producing categories (comparison, equality, logical),
    /// whose result is `Bool` regardless of the operand types.
    fn binary_rule(&mut self, op: BinaryOp, l: TypeId, r: TypeId) -> Option<TypeId> {
        let category = OpCategory::of(op);
        let l_unknown = matches!(self.types.kind(l), Some(TypeKind::Infer(_)));
        let r_unknown = matches!(self.types.kind(r), Some(TypeKind::Infer(_)));
        match (l_unknown, r_unknown) {
            (true, true) => {
                let _ = self.types.unify(l, r);
                match category {
                    // The operands' type is determined by the operator:
                    // pin the linked variable so it cannot leak unresolved.
                    OpCategory::Logical => {
                        let expected = self.bool_ty();
                        let _ = self.types.unify(l, expected);
                        Some(expected)
                    }
                    OpCategory::Shift | OpCategory::Bitwise => {
                        let expected = self.int_ty();
                        let _ = self.types.unify(l, expected);
                        Some(expected)
                    }
                    // Comparison/equality produce `Bool` regardless, and
                    // arithmetic preserves the operand type; neither can
                    // pin (any scalar / both numerics are valid), so the
                    // linked operands stay unresolved until constrained.
                    OpCategory::Comparison | OpCategory::Equality => Some(self.bool_ty()),
                    OpCategory::Arithmetic => Some(self.types.canonical(l)),
                }
            }
            (true, false) => self.rule_with_concrete(op, r, l, false),
            (false, true) => self.rule_with_concrete(op, l, r, true),
            (false, false) => self.rule_concrete(op, l, r),
        }
    }

    /// Whether `id` denotes a pointer type (any element).
    fn is_pointer(&self, id: TypeId) -> bool {
        matches!(self.types.kind(id), Some(TypeKind::Ptr(_)))
    }

    /// The result of `op` with one concrete operand `c` and one
    /// unconstrained variable `v`: the variable adopts the operator's
    /// requirement. `None` when the concrete operand cannot satisfy the
    /// operator. `c_is_lhs` records which side the concrete operand is on
    /// (pointer subtraction is directional: only `p - n` is defined).
    fn rule_with_concrete(
        &mut self,
        op: BinaryOp,
        c: TypeId,
        v: TypeId,
        c_is_lhs: bool,
    ) -> Option<TypeId> {
        let kind = self.types.kind(c)?;
        let is_numeric = matches!(kind, TypeKind::Int | TypeKind::Float);
        let is_int = matches!(kind, TypeKind::Int);
        let is_bool = matches!(kind, TypeKind::Bool);
        let is_pointer = matches!(kind, TypeKind::Ptr(_));
        let is_enum = matches!(kind, TypeKind::Enum(_));
        let is_scalar = matches!(
            kind,
            TypeKind::Int
                | TypeKind::Float
                | TypeKind::Bool
                | TypeKind::Char
                | TypeKind::Str
                | TypeKind::Null
        );
        let category = OpCategory::of(op);
        match category {
            OpCategory::Arithmetic | OpCategory::Comparison if is_numeric => {
                let _ = self.types.unify(v, c);
                Some(if category == OpCategory::Arithmetic {
                    c
                } else {
                    self.bool_ty()
                })
            }
            // Pointer arithmetic is byte-addressed and only defined for
            // `+` and `-`, with the pointer on the left for `-` (only
            // `p - n`; `n - p` is not defined). The offset side adopts
            // `Int` and the result is the pointer type.
            OpCategory::Arithmetic
                if is_pointer && (op == BinaryOp::Add || (op == BinaryOp::Sub && c_is_lhs)) =>
            {
                let offset = self.int_ty();
                let _ = self.types.unify(v, offset);
                Some(c)
            }
            OpCategory::Shift | OpCategory::Bitwise if is_int => {
                let _ = self.types.unify(v, c);
                Some(c)
            }
            OpCategory::Equality if is_scalar => {
                let _ = self.types.unify(v, c);
                Some(self.bool_ty())
            }
            // Enum equality (session 17): the unconstrained operand adopts
            // the enum type and the result is `Bool`. Only the same enum
            // type compares equal (nominal); the `unify` above pins the
            // variable to the concrete enum, so a different enum operand
            // later conflicts.
            OpCategory::Equality if is_enum => {
                let _ = self.types.unify(v, c);
                Some(self.bool_ty())
            }
            // `p == q` compares two pointers of the same type: the
            // unconstrained operand adopts the pointer type.
            OpCategory::Equality if is_pointer => {
                let _ = self.types.unify(v, c);
                Some(self.bool_ty())
            }
            OpCategory::Logical if is_bool => {
                let _ = self.types.unify(v, c);
                Some(self.bool_ty())
            }
            _ => None,
        }
    }

    /// The result of `op` for two concrete operands, or `None` when the
    /// combination is invalid.
    fn rule_concrete(&mut self, op: BinaryOp, l: TypeId, r: TypeId) -> Option<TypeId> {
        let lk = self.types.kind(l)?;
        let rk = self.types.kind(r)?;
        let category = OpCategory::of(op);
        let same_numeric = matches!(
            (lk, rk),
            (TypeKind::Int, TypeKind::Int) | (TypeKind::Float, TypeKind::Float)
        );
        let both_int = matches!((lk, rk), (TypeKind::Int, TypeKind::Int));
        let both_bool = matches!((lk, rk), (TypeKind::Bool, TypeKind::Bool));
        let same_scalar = matches!(
            (lk, rk),
            (TypeKind::Int, TypeKind::Int)
                | (TypeKind::Float, TypeKind::Float)
                | (TypeKind::Bool, TypeKind::Bool)
                | (TypeKind::Char, TypeKind::Char)
                | (TypeKind::Str, TypeKind::Str)
                | (TypeKind::Null, TypeKind::Null)
        );
        let l_ptr = matches!(lk, TypeKind::Ptr(_));
        let r_ptr = matches!(rk, TypeKind::Ptr(_));
        // Enum equality (session 17): the same enum type compares equal
        // (`v == Direction::North`); two different enums never unify
        // (nominal), so the `same_enum` check rejects them.
        let same_enum = matches!(
            (lk, rk),
            (TypeKind::Enum(a), TypeKind::Enum(b)) if a == b
        );
        match category {
            OpCategory::Arithmetic if same_numeric => Some(l),
            // Byte-addressed pointer arithmetic is only defined for `+`
            // and `-`: `p + n`, `n + p`, and `p - n` (the pointer is the
            // left operand of `-`). Every other pointer/operator
            // combination is invalid.
            OpCategory::Arithmetic
                if matches!(op, BinaryOp::Add | BinaryOp::Sub)
                    && l_ptr
                    && matches!(rk, TypeKind::Int) =>
            {
                Some(l)
            }
            OpCategory::Arithmetic
                if op == BinaryOp::Add && r_ptr && matches!(lk, TypeKind::Int) =>
            {
                Some(r)
            }
            OpCategory::Shift | OpCategory::Bitwise if both_int => Some(l),
            OpCategory::Comparison if same_numeric => Some(self.bool_ty()),
            OpCategory::Equality if same_scalar => Some(self.bool_ty()),
            OpCategory::Equality if l_ptr && r_ptr => Some(self.bool_ty()),
            OpCategory::Equality if same_enum => Some(self.bool_ty()),
            OpCategory::Logical if both_bool => Some(self.bool_ty()),
            _ => None,
        }
    }

    /// Types a range construction. Both endpoints must be the same numeric
    /// type; the result is `Range<endpoint>`.
    fn check_range(&mut self, start_ty: TypeId, end_ty: TypeId, span: Span) -> TypeId {
        let s = self.types.canonical(start_ty);
        let e = self.types.canonical(end_ty);
        if self.types.is_error(s) || self.types.is_error(e) {
            return self.types.push(TypeKind::Error);
        }
        let s_unknown = matches!(self.types.kind(s), Some(TypeKind::Infer(_)));
        let e_unknown = matches!(self.types.kind(e), Some(TypeKind::Infer(_)));
        let endpoint = match (s_unknown, e_unknown) {
            (true, true) => {
                let _ = self.types.unify(s, e);
                self.types.canonical(s)
            }
            (true, false) => {
                if self.is_numeric(e) {
                    let _ = self.types.unify(s, e);
                    e
                } else {
                    self.range_error(span, s, e);
                    return self.types.push(TypeKind::Error);
                }
            }
            (false, true) => {
                if self.is_numeric(s) {
                    let _ = self.types.unify(e, s);
                    s
                } else {
                    self.range_error(span, s, e);
                    return self.types.push(TypeKind::Error);
                }
            }
            (false, false) => {
                if self.is_numeric(s) && self.types.kind(s) == self.types.kind(e) {
                    s
                } else {
                    self.range_error(span, s, e);
                    return self.types.push(TypeKind::Error);
                }
            }
        };
        self.types.push(TypeKind::Range(endpoint))
    }

    fn range_error(&mut self, span: Span, start: TypeId, end: TypeId) {
        self.push_error(TypeError::invalid_range(
            span,
            format!("`{}` and `{}`", self.display(start), self.display(end)),
        ));
    }

    /// Types a call: the callee must have a function type, the argument
    /// count must match the declared parameters, and each argument must
    /// unify with its parameter (a pointer-typed parameter additionally
    /// accepts the integer literal `0` as the null pointer constant). The
    /// result type is the function's result.
    ///
    /// Callees without a known type (unresolved names, member/index
    /// results, unconstrained function results) defer honestly: the call
    /// produces a fresh unconstrained variable instead of a fabricated
    /// result.
    fn check_call(
        &mut self,
        callee: &Expr,
        callee_ty: TypeId,
        args: &[Expr],
        span: Span,
    ) -> TypeId {
        // Every argument is typed up front (also when the arity is wrong),
        // so later stages can look every expression's type up by span.
        let arg_types: Vec<(Span, TypeId)> = args
            .iter()
            .map(|arg| (arg.span, self.expr_type(arg)))
            .collect();
        let canon = self.types.canonical(callee_ty);
        let (params, result) = match self.types.kind(canon) {
            Some(TypeKind::Fn { params, result }) => (params.clone(), *result),
            Some(TypeKind::Infer(_)) => {
                return self.types.push(TypeKind::Infer(None));
            }
            Some(TypeKind::Error) | None => {
                return self.types.push(TypeKind::Error);
            }
            Some(_) => {
                self.errors
                    .push(TypeError::not_callable(callee.span, self.display(canon)));
                return self.types.push(TypeKind::Error);
            }
        };
        if params.len() != args.len() {
            self.errors
                .push(TypeError::wrong_arg_count(span, params.len(), args.len()));
            return self.types.push(TypeKind::Error);
        }
        let mut poisoned = false;
        for ((param, arg), (arg_span, arg_ty)) in params.iter().zip(args).zip(&arg_types) {
            if let Err((expected, actual)) = self.unify_argument(*param, arg, *arg_ty) {
                self.push_error(TypeError::mismatch(
                    *arg_span,
                    self.display(expected),
                    self.display(actual),
                    None,
                ));
                poisoned = true;
            }
        }
        if poisoned {
            self.types.push(TypeKind::Error)
        } else {
            result
        }
    }

    /// Unifies a call argument with its parameter, applying the
    /// null-pointer-constant rule: a pointer-typed parameter accepts the
    /// integer literal `0` (the null pointer). Everything else goes through
    /// ordinary unification, so a computed integer can never be silently
    /// reinterpreted as a pointer.
    fn unify_argument(
        &mut self,
        param: TypeId,
        arg: &Expr,
        arg_ty: TypeId,
    ) -> Result<TypeId, (TypeId, TypeId)> {
        if self.is_pointer(param)
            && matches!(
                self.types.kind(self.types.canonical(arg_ty)),
                Some(TypeKind::Int)
            )
            && self.is_zero_literal(arg)
        {
            return Ok(param);
        }
        self.types.unify(param, arg_ty)
    }

    /// Whether `expr` is the integer literal `0` in any spelling (decimal,
    /// `0x`/`0o`/`0b`, with `_` separators): the null pointer constant.
    fn is_zero_literal(&self, expr: &Expr) -> bool {
        if !matches!(expr.kind, ExprKind::Int) {
            return false;
        }
        let Some(file) = self.sources.get(expr.span.file()) else {
            return false;
        };
        let Some(text) = file.span_text(expr.span) else {
            return false;
        };
        let digits = text
            .strip_prefix("0x")
            .or_else(|| text.strip_prefix("0X"))
            .or_else(|| text.strip_prefix("0o"))
            .or_else(|| text.strip_prefix("0O"))
            .or_else(|| text.strip_prefix("0b"))
            .or_else(|| text.strip_prefix("0B"))
            .unwrap_or(text);
        !digits.is_empty()
            && digits
                .bytes()
                .filter(|byte| *byte != b'_')
                .all(|b| b == b'0')
    }

    /// Types an assignment.
    ///
    /// The semantic stage owns target writability; this stage adds type
    /// compatibility only, and skips it when the target cannot legally be
    /// assigned at all (an immutable or constant binding), so the
    /// immutable-assignment diagnostic is not doubled by a misleading
    /// cascade. A member/index target's root base binding must be mutable
    /// (semantic analysis reports it otherwise), and the value must unify
    /// with the field/element type, which this stage checks.
    fn check_assign(
        &mut self,
        op: AssignOp,
        target: &Expr,
        value_ty: TypeId,
        value_span: Span,
        span: Span,
    ) -> TypeId {
        match &target.kind {
            ExprKind::Ident(ident) => {
                let target_ty = self.expr_type(target);
                let writable = self
                    .semantic
                    .resolve(ident.span)
                    .and_then(|symbol| self.semantic.symbols().get(symbol))
                    .is_some_and(|symbol| symbol.kind.is_mutable());
                if !writable {
                    return value_ty;
                }
                self.check_value_for_target(op, target_ty, value_ty, value_span, span, target.span)
            }
            ExprKind::Member { .. } | ExprKind::Index { .. } => {
                // A deref-rooted member/index target (`(*r).x = v`) is not
                // part of the reference model (only whole-value `*r = v`
                // is): lowering it would write to a temporary copy of the
                // dereferenced value, silently dropping the assignment, so
                // it is rejected here instead (E-T33).
                if self.is_deref_rooted(target) {
                    self.push_error(TypeError::deref_rooted_assignment(target.span));
                    self.expr_types
                        .push((target.span, self.types.push(TypeKind::Error)));
                    return self.types.push(TypeKind::Error);
                }
                let target_ty = self.expr_type(target);
                // The base binding's writability is a semantic question;
                // skip the type check when it is immutable so the semantic
                // diagnostic is not doubled.
                if let Some(root) = self.root_ident(target) {
                    let writable = self
                        .semantic
                        .resolve(root.span)
                        .and_then(|symbol| self.semantic.symbols().get(symbol))
                        .is_some_and(|symbol| symbol.kind.is_mutable());
                    if !writable {
                        return value_ty;
                    }
                }
                self.check_value_for_target(op, target_ty, value_ty, value_span, span, target.span)
            }
            ExprKind::Deref { operand } => {
                let operand_ty = self.expr_type(operand);
                let canon = self.types.canonical(operand_ty);
                // Snapshot the mutability before mutating `self` (the
                // match on `self.types` borrows it).
                let through_mut = matches!(
                    self.types.kind(canon),
                    Some(TypeKind::Ref { mutable: true, .. })
                );
                let through_shared = matches!(
                    self.types.kind(canon),
                    Some(TypeKind::Ref { mutable: false, .. })
                );
                if through_mut {
                    // `*r = v` through `&mut T`: value must match T (the
                    // referent type). Record the target's referent type
                    // (mirroring the Ident/Member/Index arms, which record
                    // via `expr_type`) so HIR lowering finds a type for
                    // the `*r` place.
                    let target_ty = match self.types.kind(canon) {
                        Some(TypeKind::Ref { elem, .. }) => *elem,
                        _ => self.types.push(TypeKind::Error),
                    };
                    self.expr_types.push((target.span, target_ty));
                    self.check_value_for_target(
                        op,
                        target_ty,
                        value_ty,
                        value_span,
                        span,
                        target.span,
                    )
                } else if through_shared {
                    // Writes through an immutable reference are always
                    // wrong, even when the value type happens to match.
                    self.push_error(TypeError::assign_through_immutable_ref(target.span));
                    self.expr_types
                        .push((target.span, self.types.push(TypeKind::Error)));
                    self.types.push(TypeKind::Error)
                } else {
                    self.expr_types
                        .push((target.span, self.types.push(TypeKind::Error)));
                    self.types.push(TypeKind::Error)
                }
            }
            _ => {
                // Defensive: the parser rejects non-place targets.
                let target_ty = self.expr_type(target);
                self.check_value_for_target(op, target_ty, value_ty, value_span, span, target.span)
            }
        }
    }

    /// Checks a value against a writable target, either directly (`=`,
    /// value must unify with the target) or through the compound operator's
    /// arithmetic rule (`+=` etc., target and value must satisfy the
    /// corresponding binary rule and the result must unify with the
    /// target).
    fn check_value_for_target(
        &mut self,
        op: AssignOp,
        target_ty: TypeId,
        value_ty: TypeId,
        value_span: Span,
        span: Span,
        target_span: Span,
    ) -> TypeId {
        if op == AssignOp::Assign {
            return match self.types.unify(target_ty, value_ty) {
                Ok(_) => value_ty,
                Err((expected, actual)) => {
                    self.push_error(TypeError::mismatch(
                        value_span,
                        self.display(expected),
                        self.display(actual),
                        Some(target_span),
                    ));
                    self.types.push(TypeKind::Error)
                }
            };
        }
        let binary = match op {
            AssignOp::AddAssign => BinaryOp::Add,
            AssignOp::SubAssign => BinaryOp::Sub,
            AssignOp::MulAssign => BinaryOp::Mul,
            AssignOp::DivAssign => BinaryOp::Div,
            AssignOp::RemAssign => BinaryOp::Rem,
            AssignOp::Assign => unreachable!("plain assignment is handled above"),
        };
        let l = self.types.canonical(target_ty);
        let r = self.types.canonical(value_ty);
        let result = if self.types.is_error(l) || self.types.is_error(r) {
            self.types.push(TypeKind::Error)
        } else {
            match self.binary_rule(binary, l, r) {
                Some(ty) => ty,
                None => return self.emit_operator_error(op.symbol(), l, r, span),
            }
        };
        if self.types.is_error(result) {
            return result;
        }
        if let Err((expected, actual)) = self.types.unify(target_ty, result) {
            self.push_error(TypeError::mismatch(
                value_span,
                self.display(expected),
                self.display(actual),
                Some(target_span),
            ));
            return self.types.push(TypeKind::Error);
        }
        result
    }

    // ------------------------------------------------------------------
    // Aggregates: member, index, struct literals, array literals
    // ------------------------------------------------------------------

    /// Types `base.member`: the base must be a struct, and the member must
    /// be one of its declared fields (`E-T07`/`E-T08` otherwise). An
    /// unresolved base defers honestly (nothing is known yet).
    fn check_member(&mut self, base: TypeId, member: &Ident) -> TypeId {
        match self.types.kind(base) {
            Some(TypeKind::Struct(id)) => {
                let info = self
                    .types
                    .struct_info(*id)
                    .expect("registered structs always resolve");
                match info.fields.iter().find(|field| field.name == member.name) {
                    Some(field) => field.ty,
                    None => {
                        self.push_error(TypeError::unknown_member(
                            member.span,
                            &info.name,
                            &member.name,
                        ));
                        self.types.push(TypeKind::Error)
                    }
                }
            }
            Some(TypeKind::Infer(_)) => self.types.push(TypeKind::Infer(None)),
            Some(TypeKind::Error) | None => self.types.push(TypeKind::Error),
            Some(_) => {
                self.push_error(TypeError::member_access_on_non_struct(
                    member.span,
                    &member.name,
                    self.display(base),
                ));
                self.types.push(TypeKind::Error)
            }
        }
    }

    /// Types `base.index` for tuple field access (session 29): the base
    /// must be a tuple type, and `index` must be a non-negative integer
    /// literal within the tuple's length. The result is the element type
    /// at that index.
    fn check_tuple_field_access(&mut self, base: &Expr, index: &Ident, span: Span) -> TypeId {
        let base_ty = self.expr_type(base);
        let base_canonical = self.types.canonical(base_ty);
        match self.types.kind(base_canonical) {
            Some(TypeKind::Tuple(elems)) => {
                // Decode the index from the literal's source text.
                let idx_val: Option<u32> = index.name.parse().ok();
                match idx_val {
                    Some(idx) if (idx as usize) < elems.len() => elems[idx as usize],
                    Some(idx) => {
                        self.push_error(TypeError::invalid_tuple_index(span, idx, elems.len()));
                        self.types.push(TypeKind::Error)
                    }
                    None => {
                        self.push_error(TypeError::invalid_tuple_index(span, 0, elems.len()));
                        self.types.push(TypeKind::Error)
                    }
                }
            }
            Some(TypeKind::Infer(_)) => {
                // The base type is not yet known; infer a unit for now.
                self.types.push(TypeKind::Unit)
            }
            _ => {
                self.push_error(TypeError::member_access_on_non_struct(
                    span,
                    "tuple field",
                    self.display(base_canonical),
                ));
                self.types.push(TypeKind::Error)
            }
        }
    }

    /// Types `base[index]`: the base must be an array, and the index must
    /// be an `Int` (`E-T09`/`E-T10` otherwise); a constant index is
    /// additionally checked against the array's length (`E-T11`). The
    /// result is the element type.
    fn check_index(&mut self, base: TypeId, index: &Expr, index_ty: TypeId, span: Span) -> TypeId {
        let array = match self.types.kind(base) {
            Some(TypeKind::Array { elem, len }) => Some((*elem, *len)),
            _ => None,
        };
        if let Some((elem, len)) = array {
            let idx = self.types.canonical(index_ty);
            let index_ok = match self.types.kind(idx) {
                Some(TypeKind::Int) => true,
                Some(TypeKind::Infer(_)) => {
                    let expected = self.int_ty();
                    let _ = self.types.unify(idx, expected);
                    true
                }
                Some(TypeKind::Error) | None => true,
                Some(_) => false,
            };
            if !index_ok {
                self.errors
                    .push(TypeError::invalid_index_type(index.span, self.display(idx)));
                return self.types.push(TypeKind::Error);
            }
            if let Some(value) = self.constant_index(index) {
                if value < 0 || value as u64 >= len {
                    self.errors
                        .push(TypeError::index_out_of_range(index.span, value, len));
                    return self.types.push(TypeKind::Error);
                }
            }
            return elem;
        }
        match self.types.kind(base) {
            Some(TypeKind::Infer(_)) => self.types.push(TypeKind::Infer(None)),
            Some(TypeKind::Error) | None => self.types.push(TypeKind::Error),
            Some(_) => {
                self.errors
                    .push(TypeError::index_on_non_array(span, self.display(base)));
                self.types.push(TypeKind::Error)
            }
        }
    }

    /// The value of the index expression when it is a constant: an integer
    /// literal or a negated integer literal. Runtime (non-literal) indices
    /// are checked at execution time (`E-R10`).
    fn constant_index(&self, index: &Expr) -> Option<i64> {
        match &index.kind {
            ExprKind::Int => self.decode_int_literal(index),
            ExprKind::Unary {
                op: UnaryOp::Neg,
                operand,
            } => match &operand.kind {
                ExprKind::Int => self.decode_int_literal(operand).map(|v| v.wrapping_neg()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Decodes an integer literal's source text into its (wrapping) 64-bit
    /// value, or `None` when the literal text is unavailable.
    fn decode_int_literal(&self, literal: &Expr) -> Option<i64> {
        if !matches!(literal.kind, ExprKind::Int) {
            return None;
        }
        let file = self.sources.get(literal.span.file())?;
        let text = file.span_text(literal.span)?;
        let (radix, digits) =
            if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                (16u64, rest)
            } else if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
                (8u64, rest)
            } else if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
                (2u64, rest)
            } else {
                (10u64, text)
            };
        let mut value: u64 = 0;
        for byte in digits.bytes() {
            if byte == b'_' {
                continue;
            }
            let digit = match byte {
                b'0'..=b'9' => u64::from(byte - b'0'),
                b'a'..=b'f' => u64::from(byte - b'a' + 10),
                b'A'..=b'F' => u64::from(byte - b'A' + 10),
                _ => return None,
            };
            if digit >= radix {
                return None;
            }
            value = value.wrapping_mul(radix).wrapping_add(digit);
        }
        Some(value as i64)
    }

    /// Types `Name { field: value, ... }`: the name must be a registered
    /// struct (`E-T15` otherwise); every initializer must name a declared
    /// field (`E-T12`), appear at most once (`E-T14`), and unify with the
    /// field's type (`E-T01`); and every declared field must be initialized
    /// (`E-T13`). The result is the struct's type.
    fn check_struct_literal(
        &mut self,
        name: &Ident,
        fields: &[StructFieldInit],
        span: Span,
    ) -> TypeId {
        let Some(reg) = self.structs.get(&name.name) else {
            self.errors
                .push(TypeError::unknown_type(name.span, &name.name));
            return self.types.push(TypeKind::Error);
        };
        // Copy what the loop needs so the tables can be mutated freely.
        let id = reg.id;
        let struct_name = reg.name.clone();
        let declared = self
            .types
            .struct_info(reg.struct_id)
            .map(|info| {
                info.fields
                    .iter()
                    .map(|field| (field.name.clone(), field.ty))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut seen: Vec<(String, Span)> = Vec::new();
        for field in fields {
            let Some(declared_ty) = declared
                .iter()
                .find(|(name, _)| *name == field.name.name)
                .map(|(_, ty)| *ty)
            else {
                self.push_error(TypeError::unknown_struct_field(
                    field.name.span,
                    &struct_name,
                    &field.name.name,
                ));
                continue;
            };
            if let Some((_, first)) = seen.iter().find(|(name, _)| *name == field.name.name) {
                self.push_error(TypeError::duplicate_field_init(
                    field.name.span,
                    &field.name.name,
                    *first,
                ));
                continue;
            }
            seen.push((field.name.name.clone(), field.name.span));
            let value_ty = self.expr_type(&field.value);
            if let Err((expected, actual)) = self.types.unify(declared_ty, value_ty) {
                self.push_error(TypeError::mismatch(
                    field.value.span,
                    self.display(expected),
                    self.display(actual),
                    None,
                ));
            }
        }
        for (declared_name, _) in &declared {
            if !seen.iter().any(|(name, _)| name == declared_name) {
                self.push_error(TypeError::missing_struct_field(
                    span,
                    &struct_name,
                    declared_name,
                ));
            }
        }
        id
    }

    /// Types an enum variant reference `Name::Variant`: the first segment
    /// must name a registered enum (`E-T15` when the type is unknown,
    /// `E-T22` when it names a non-enum type), and the variant must be one
    /// of its declared alternatives (`E-T23`). The result is the enum's
    /// type.
    /// Types an enum variant expression `Name::Variant` or, for a
    /// data-carrying variant (session 19), its construction
    /// `Name::Variant(payload)`. The result is the enum's type.
    ///
    /// A construction's payload must unify with the variant's declared
    /// payload type (`E-T28`); attaching a payload to a unit variant or
    /// omitting one from a data-carrying variant is `E-T29`. On a failed
    /// variant path the payload is still typed so independent problems
    /// keep being reported.
    fn check_enum_variant(
        &mut self,
        name: &Ident,
        variant: &Ident,
        payload: &Option<Box<Expr>>,
        span: Span,
    ) -> TypeId {
        let Some(enum_ty) = self.enum_variant_type(name, variant, span) else {
            if let Some(payload) = payload {
                self.expr_type(payload);
            }
            return self.types.push(TypeKind::Error);
        };
        let declared = self.variant_payload(enum_ty, &variant.name);
        match (payload.as_deref(), declared) {
            (Some(expr), Some(payload_ty)) => {
                let ty = self.expr_type(expr);
                if let Err((expected, actual)) = self.types.unify(payload_ty, ty) {
                    self.push_error(TypeError::variant_payload_mismatch(
                        expr.span,
                        self.display(expected),
                        self.display(actual),
                    ));
                }
                enum_ty
            }
            (Some(expr), None) => {
                self.expr_type(expr);
                self.push_error(TypeError::variant_payload_arity(
                    span,
                    format!(
                        "variant `{}::{}` is a unit variant and cannot carry a payload",
                        name.name, variant.name
                    ),
                ));
                enum_ty
            }
            (None, Some(_)) => {
                self.push_error(TypeError::variant_payload_arity(
                    span,
                    format!(
                        "variant `{}::{}` carries a payload and must be constructed with one: `{}::{}(expr)`",
                        name.name, variant.name, name.name, variant.name
                    ),
                ));
                enum_ty
            }
            (None, None) => enum_ty,
        }
    }

    /// The declared payload type of the variant `variant` of the enum
    /// type `enum_ty`, if the variant is data-carrying.
    fn variant_payload(&self, enum_ty: TypeId, variant: &str) -> Option<TypeId> {
        let enum_id = self.types.enum_id(enum_ty)?;
        self.types.enum_info(enum_id).and_then(|info| {
            info.variants
                .iter()
                .find(|v| v.name == variant)
                .and_then(|v| v.payload)
        })
    }

    /// Resolves `Name::Variant` to the enum's type, reporting the same
    /// diagnostics as the expression form: `E-T15` when the name is not a
    /// known type, `E-T22` when it names a non-enum type, and `E-T23` when
    /// the variant is not declared. Returns `None` on any failure so the
    /// caller skips the pattern (a failed variant pattern covers nothing).
    fn enum_variant_type(&mut self, name: &Ident, variant: &Ident, span: Span) -> Option<TypeId> {
        let Some(reg) = self.enums.get(&name.name) else {
            if self.structs.contains_key(&name.name) {
                self.push_error(TypeError::not_an_enum(
                    span,
                    &name.name,
                    self.display(self.structs.get(&name.name).expect("checked above").id),
                ));
            } else {
                self.errors
                    .push(TypeError::unknown_type(name.span, &name.name));
            }
            return None;
        };
        let variants = self
            .types
            .enum_info(reg.enum_id)
            .map(|info| info.variants.clone())
            .unwrap_or_default();
        if variants.iter().all(|v| v.name != variant.name) {
            self.push_error(TypeError::unknown_variant(
                variant.span,
                &reg.name,
                &variant.name,
            ));
            return None;
        }
        Some(reg.id)
    }

    /// Types `[elem, ...]`: every element must unify with the first
    /// element's type (`E-T01` on conflict); the result is
    /// `Array<elem_type, n>`. An empty literal has no element type to infer
    /// (`E-T17`). The array's layout is validated against the runtime model
    /// (`E-T18`).
    fn check_array_literal(&mut self, elems: &[Expr], span: Span) -> TypeId {
        if elems.is_empty() {
            self.push_error(TypeError::empty_array_literal(span));
            return self.types.push(TypeKind::Error);
        }
        let first_ty = self.expr_type(&elems[0]);
        let first = self.types.canonical(first_ty);
        if self.types.is_error(first) {
            for elem in &elems[1..] {
                self.expr_type(elem);
            }
            return self.types.push(TypeKind::Error);
        }
        for elem in &elems[1..] {
            let ty = self.expr_type(elem);
            if let Err((expected, actual)) = self.types.unify(first, ty) {
                self.push_error(TypeError::mismatch(
                    elem.span,
                    self.display(expected),
                    self.display(actual),
                    None,
                ));
            }
        }
        let elem = self.types.canonical(first);
        let ty = self.types.push(TypeKind::Array {
            elem,
            len: elems.len() as u64,
        });
        if let Err(error) = layout::array_layout(ty, &self.types) {
            self.push_error(TypeError::invalid_aggregate_layout(
                span,
                layout_error_message(&error),
            ));
            return self.types.push(TypeKind::Error);
        }
        ty
    }

    /// The root identifier of a member/index chain, if it bottoms out in an
    /// identifier (`p.x.y` → `p`, `arr[i].x` → `arr`).
    fn root_ident<'e>(&self, expr: &'e Expr) -> Option<&'e Ident> {
        match &expr.kind {
            ExprKind::Ident(ident) => Some(ident),
            ExprKind::Member { base, .. } => self.root_ident(base),
            ExprKind::Index { base, .. } => self.root_ident(base),
            _ => None,
        }
    }

    // ------------------------------------------------------------------
    // Small helpers
    // ------------------------------------------------------------------

    fn bool_ty(&mut self) -> TypeId {
        self.types.push(TypeKind::Bool)
    }

    fn int_ty(&mut self) -> TypeId {
        self.types.push(TypeKind::Int)
    }

    fn is_numeric(&self, id: TypeId) -> bool {
        matches!(self.types.kind(id), Some(TypeKind::Int | TypeKind::Float))
    }

    fn display(&self, id: TypeId) -> String {
        self.types.display(id)
    }
}

/// The human-readable reason a layout could not be computed. Shared with
/// the backend lowering (which validates aggregate layouts again against
/// the same engine).
pub(crate) fn layout_error_message(error: &LayoutError) -> String {
    match error {
        LayoutError::Recursive { name } => {
            format!("the struct `{name}` is recursive and has no finite size")
        }
        LayoutError::Empty { name } => {
            format!("the struct `{name}` must declare at least one field")
        }
        LayoutError::Overflow { name } => format!("the aggregate `{name}` has an overflowing size"),
        LayoutError::TooLarge { name } => format!(
            "the aggregate `{name}` is too large (the runtime limit is {} bytes)",
            crate::runtime::layout::MAX_AGGREGATE_BYTES
        ),
    }
}
