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

/// One value a refutable match pattern covers, used for exhaustiveness
/// and duplicate-arm detection. Patterns of the same key match the same
/// value, so a repeated key is an unreachable arm.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CoverageKey {
    /// The boolean literal `true` or `false`.
    Bool(bool),
    /// An integer literal's decoded value (negative literals negated).
    Int(i64),
    /// An enum variant, by name.
    Variant(String),
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
        }
    }

    fn run(&mut self) {
        // Enums are registered before struct field types are resolved:
        // a struct field may be of enum type, and field resolution needs
        // every type name visible (module-scope order independence).
        // Enums themselves have no field types (unit variants), so they
        // never depend on structs.
        self.register_enums();
        self.register_structs();
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
                        self.unify_decl(&binding.name, ty, binding.init.span);
                    }
                }
                ItemKind::Const(binding) => {
                    let (ty, recomputed) = self.resolve_deferred_expr(&binding.init);
                    if recomputed {
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
                    self.unify_decl(&binding.name, ty, binding.init.span);
                }
            }
            StmtKind::Const(binding) => {
                let (ty, recomputed) = self.resolve_deferred_expr(&binding.init);
                if recomputed {
                    self.unify_decl(&binding.name, ty, binding.init.span);
                }
            }
            StmtKind::Return(Some(value)) => {
                let (ty, recomputed) = self.resolve_deferred_expr(value);
                if recomputed {
                    self.recheck_return(ty, value.span);
                }
            }
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
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
            ExprKind::Int
            | ExprKind::Float
            | ExprKind::Str
            | ExprKind::Char
            | ExprKind::Bool(_)
            | ExprKind::Null
            | ExprKind::Ident(_)
            | ExprKind::EnumVariant { .. } => (
                self.recorded_ty(expr.span)
                    .unwrap_or_else(|| self.types.push(TypeKind::Error)),
                false,
            ),
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
    fn register_structs(&mut self) {
        // Phase 1: register names. Duplicate names are reported by semantic
        // analysis; the first declaration's type is authoritative.
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
        // Phase 2: resolve field types. The first declaration of each name
        // is identified by its span (later duplicates are skipped).
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
        // Phase 3: validate every registered struct's layout. Recursive or
        // oversized structs are reported once, at the declaration.
        let mut layout_errors: Vec<TypeError> = Vec::new();
        for reg in self.structs.values() {
            if let Err(error) = layout::struct_layout(reg.struct_id, &self.types) {
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

    /// Registers every enum declaration into the type table (first
    /// declaration of each name wins, mirroring semantic analysis) and
    /// records every variant with its deterministic discriminant
    /// (declaration order, starting at 0).
    fn register_enums(&mut self) {
        // Phase 1: register names. Duplicate names are reported by semantic
        // analysis; the first declaration's type is authoritative.
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
        // Phase 2: record variants. The first declaration of each name is
        // identified by its span (later duplicates are skipped).
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
            let variants = e
                .variants
                .iter()
                .enumerate()
                .map(|(index, variant)| EnumVariantInfo {
                    name: variant.name.name.clone(),
                    discriminant: index as u32,
                })
                .collect();
            self.types.set_enum_variants(reg.enum_id, variants);
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
    /// result are inference variables; every other symbol gets a fresh
    /// inference variable that its declaration or usage later resolves.
    fn pre_register(&mut self) {
        // Function name span start → declared parameter count.
        let mut fn_arity: HashMap<u32, usize> = HashMap::new();
        for item in &self.ast.items {
            if let ItemKind::Fn(f) = &item.kind {
                fn_arity.insert(f.name.span.start(), f.params.len());
            }
        }
        for symbol in self.semantic.symbols().iter() {
            let ty = match symbol.kind {
                SymbolKind::Fn => {
                    let params = (0..fn_arity.get(&symbol.span.start()).copied().unwrap_or(0))
                        .map(|_| self.types.push(TypeKind::Infer(None)))
                        .collect::<Vec<_>>();
                    let result = self.types.push(TypeKind::Infer(None));
                    self.types.push(TypeKind::Fn { params, result })
                }
                SymbolKind::Intrinsic => self.intrinsic_type(symbol),
                _ => self.types.push(TypeKind::Infer(None)),
            };
            self.symbol_types[symbol.id.raw() as usize] = ty;
            self.decls.insert(symbol.span.start(), symbol.id);
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
                self.unify_decl(&binding.name, ty, binding.init.span);
            }
            ItemKind::Const(binding) => {
                let ty = self.expr_type(&binding.init);
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

    fn check_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Let(binding) => {
                let ty = self.expr_type(&binding.init);
                self.unify_decl(&binding.name, ty, binding.init.span);
            }
            StmtKind::Const(binding) => {
                let ty = self.expr_type(&binding.init);
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
            StmtKind::Break | StmtKind::Continue => {}
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
        let mut covered: Vec<CoverageKey> = Vec::new();
        let mut has_catch_all = false;
        for arm in &stmt.arms {
            // Re-canonicalize per arm: an earlier refutable pattern may
            // have pinned an unresolved scrutinee to its type.
            canon = self.types.canonical(scrutinee_ty);
            if has_catch_all {
                // Every value already matches the earlier `_`/binding arm.
                self.push_error(TypeError::unreachable_match_arm(
                    arm.pattern.span(),
                    "this arm can never run: an earlier `_` or binding arm already matches every value",
                ));
                continue;
            }
            match &arm.pattern {
                Pattern::Wildcard { .. } => has_catch_all = true,
                Pattern::Binding(name) => {
                    // The binding copies the scrutinee's value into the
                    // arm's scope; its type is the scrutinee's type.
                    has_catch_all = true;
                    self.unify_decl(name, canon, name.span);
                }
                Pattern::Bool { value, span } => {
                    let expected = self.bool_ty();
                    match self.types.unify(canon, expected) {
                        Ok(_) => {
                            self.record_coverage(&mut covered, CoverageKey::Bool(*value), *span)
                        }
                        Err((expected, actual)) => self.push_error(TypeError::mismatch(
                            *span,
                            self.display(expected),
                            self.display(actual),
                            None,
                        )),
                    }
                }
                Pattern::Int {
                    negative,
                    literal,
                    span,
                } => {
                    let expected = self.int_ty();
                    match self.types.unify(canon, expected) {
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
                            self.record_coverage(&mut covered, CoverageKey::Int(value), *span);
                        }
                        Err((expected, actual)) => self.push_error(TypeError::mismatch(
                            *span,
                            self.display(expected),
                            self.display(actual),
                            None,
                        )),
                    }
                }
                Pattern::EnumVariant { name, variant } => {
                    let pattern_span = arm.pattern.span();
                    if let Some(enum_ty) = self.enum_variant_type(name, variant, pattern_span) {
                        match self.types.unify(canon, enum_ty) {
                            Ok(_) => self.record_coverage(
                                &mut covered,
                                CoverageKey::Variant(variant.name.clone()),
                                pattern_span,
                            ),
                            Err((expected, actual)) => self.push_error(TypeError::mismatch(
                                pattern_span,
                                self.display(expected),
                                self.display(actual),
                                None,
                            )),
                        }
                    }
                }
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
        match self.types.kind(canon) {
            Some(TypeKind::Int) => {
                if !has_catch_all {
                    self.push_error(TypeError::non_exhaustive_match(
                        stmt.span,
                        "the match is not exhaustive: integer values cannot all be listed; add a `_` or binding arm",
                    ));
                }
            }
            Some(TypeKind::Bool) => {
                let has_true = covered
                    .iter()
                    .any(|key| matches!(key, CoverageKey::Bool(true)));
                let has_false = covered
                    .iter()
                    .any(|key| matches!(key, CoverageKey::Bool(false)));
                if !has_catch_all && !(has_true && has_false) {
                    self.push_error(TypeError::non_exhaustive_match(
                        stmt.span,
                        "the match is not exhaustive: a `Bool` match must cover both `true` and `false`, or add a `_` or binding arm",
                    ));
                }
            }
            Some(TypeKind::Enum(id)) => {
                let covered_names: Vec<&str> = covered
                    .iter()
                    .filter_map(|key| match key {
                        CoverageKey::Variant(name) => Some(name.as_str()),
                        _ => None,
                    })
                    .collect();
                let missing = self
                    .types
                    .enum_info(*id)
                    .map(|info| {
                        info.variants
                            .iter()
                            .filter(|variant| !covered_names.contains(&variant.name.as_str()))
                            .map(|variant| format!("`{}`", variant.name))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if !has_catch_all && !missing.is_empty() {
                    self.push_error(TypeError::non_exhaustive_match(
                        stmt.span,
                        format!(
                            "the match is not exhaustive: the variant{} {} {} not covered; add a `_` or binding arm or cover every variant",
                            if missing.len() == 1 { "" } else { "s" },
                            missing.join(", "),
                            if missing.len() == 1 { "is" } else { "are" },
                        ),
                    ));
                }
            }
            _ => {}
        }
    }

    /// Records `key` as covered by an arm, rejecting a repeat: a pattern
    /// an earlier arm already matches can never run (`E-T25`).
    fn record_coverage(&mut self, covered: &mut Vec<CoverageKey>, key: CoverageKey, span: Span) {
        if covered.contains(&key) {
            self.push_error(TypeError::unreachable_match_arm(
                span,
                "this arm can never run: an earlier arm already matches the same value",
            ));
        } else {
            covered.push(key);
        }
    }

    fn check_if(&mut self, stmt: &IfStmt) {
        self.check_condition(&stmt.cond);
        self.check_block(&stmt.then_block);
        match &stmt.else_branch {
            Some(ElseBranch::If(nested)) => self.check_if(nested),
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
            ExprKind::EnumVariant { name, variant } => {
                self.check_enum_variant(name, variant, expr.span)
            }
            ExprKind::Group(inner) => self.expr_type(inner),
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
        match self.binary_rule(op, l, r) {
            Some(ty) => ty,
            None => self.emit_operator_error(op.symbol(), l, r, span),
        }
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
    fn check_enum_variant(&mut self, name: &Ident, variant: &Ident, span: Span) -> TypeId {
        self.enum_variant_type(name, variant, span)
            .unwrap_or_else(|| self.types.push(TypeKind::Error))
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
