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

use std::collections::HashMap;

use crate::ast::{
    AssignOp, Ast, BinaryOp, Block, ElseBranch, Expr, ExprKind, FnItem, Ident, IfStmt, Item,
    ItemKind, Stmt, StmtKind, UnaryOp,
};
use crate::semantics::{SemanticResult, SymbolId, SymbolKind};
use crate::source::Span;

use super::TypeResult;
use super::error::TypeError;
use super::ty::{TypeId, TypeKind, TypeTable};

/// Runs type analysis over `ast`, consuming the semantic result.
///
/// The analysis is deterministic: symbol types, expression types, and
/// errors are produced in source order.
pub(crate) fn check_ast(ast: &Ast, semantic: &SemanticResult) -> TypeResult {
    let mut checker = Checker::new(ast, semantic);
    checker.run();
    checker.finish()
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
struct Checker<'a> {
    ast: &'a Ast,
    semantic: &'a SemanticResult,
    types: TypeTable,
    /// The type of every symbol, indexed by `SymbolId::raw()`.
    symbol_types: Vec<TypeId>,
    /// Declaration name span start → symbol id, for binding lookups.
    decls: HashMap<u32, SymbolId>,
    /// Expression types in traversal order: (expression span, type).
    expr_types: Vec<(Span, TypeId)>,
    /// Type errors, in the order they were found.
    errors: Vec<TypeError>,
    /// The current function's result type, while inside a function body.
    fn_result: Option<TypeId>,
}

impl<'a> Checker<'a> {
    fn new(ast: &'a Ast, semantic: &'a SemanticResult) -> Self {
        let mut types = TypeTable::new();
        // Placeholder for symbol slots until pre-registration fills them.
        let placeholder = types.push(TypeKind::Error);
        Self {
            ast,
            semantic,
            types,
            symbol_types: vec![placeholder; semantic.symbols().len()],
            decls: HashMap::new(),
            expr_types: Vec::new(),
            errors: Vec::new(),
            fn_result: None,
        }
    }

    fn run(&mut self) {
        self.pre_register();
        for item in &self.ast.items {
            self.check_item(item);
        }
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
                _ => self.types.push(TypeKind::Infer(None)),
            };
            self.symbol_types[symbol.id.raw() as usize] = ty;
            self.decls.insert(symbol.span.start(), symbol.id);
        }
    }

    // ------------------------------------------------------------------
    // Items and statements
    // ------------------------------------------------------------------

    fn check_item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Fn(f) => self.check_fn(f),
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
                            self.errors.push(TypeError::mismatch(
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
                let cond_ty = self.expr_type(cond);
                self.require_bool(cond_ty, cond.span);
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
            StmtKind::Expr(expr) => {
                self.expr_type(expr);
            }
        }
    }

    fn check_if(&mut self, stmt: &IfStmt) {
        let cond_ty = self.expr_type(&stmt.cond);
        self.require_bool(cond_ty, stmt.cond.span);
        self.check_block(&stmt.then_block);
        match &stmt.else_branch {
            Some(ElseBranch::If(nested)) => self.check_if(nested),
            Some(ElseBranch::Block(block)) => self.check_block(block),
            None => {}
        }
    }

    /// Types a `for` loop variable from the iterable's element type. Only
    /// ranges are iterable at this stage; unconstrained or unknown
    /// iterables defer silently.
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
            Some(TypeKind::Infer(_)) | Some(TypeKind::Error) | None => {}
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
            self.errors.push(TypeError::mismatch(
                span,
                self.display(expected),
                self.display(actual),
                None,
            ));
        }
    }

    /// Requires a condition expression to be boolean. Unknown and error
    /// types are accepted silently — their root cause is reported
    /// elsewhere, and an unconstrained variable cannot be judged yet.
    fn require_bool(&mut self, ty: TypeId, span: Span) {
        let canon = self.types.canonical(ty);
        match self.types.kind(canon) {
            Some(TypeKind::Bool) | Some(TypeKind::Infer(_)) | Some(TypeKind::Error) | None => {}
            Some(_) => {
                self.errors
                    .push(TypeError::mismatch(span, "Bool", self.display(canon), None));
            }
        }
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
                let args: Vec<(Span, TypeId)> = args
                    .iter()
                    .map(|arg| (arg.span, self.expr_type(arg)))
                    .collect();
                self.check_call(callee, callee_ty, &args, expr.span)
            }
            ExprKind::Member { base, .. } => {
                let base_ty = self.expr_type(base);
                if self.types.is_error(self.types.canonical(base_ty)) {
                    self.types.push(TypeKind::Error)
                } else {
                    // Member typing depends on user-defined types, which do
                    // not exist yet. The expression's type is an
                    // unconstrained inference variable: the operation is
                    // deferred honestly, never silently accepted as a
                    // specific type.
                    self.types.push(TypeKind::Infer(None))
                }
            }
            ExprKind::Index { base, index } => {
                let base_ty = self.expr_type(base);
                self.expr_type(index);
                if self.types.is_error(self.types.canonical(base_ty)) {
                    self.types.push(TypeKind::Error)
                } else {
                    self.types.push(TypeKind::Infer(None))
                }
            }
            ExprKind::Group(inner) => self.expr_type(inner),
        }
    }

    /// Types a prefix unary operation. `-` requires a numeric operand,
    /// `!` a boolean one, and `~` an integer one; the result type is the
    /// operand type.
    fn check_unary(&mut self, op: UnaryOp, operand_ty: TypeId, span: Span) -> TypeId {
        let canon = self.types.canonical(operand_ty);
        if self.types.is_error(canon) {
            return self.types.push(TypeKind::Error);
        }
        let valid = match op {
            UnaryOp::Neg => matches!(
                self.types.kind(canon),
                Some(TypeKind::Int | TypeKind::Float | TypeKind::Infer(_))
            ),
            UnaryOp::Not => matches!(
                self.types.kind(canon),
                Some(TypeKind::Bool | TypeKind::Infer(_))
            ),
            UnaryOp::BitNot => matches!(
                self.types.kind(canon),
                Some(TypeKind::Int | TypeKind::Infer(_))
            ),
        };
        if valid {
            canon
        } else {
            self.errors.push(TypeError::invalid_operator(
                span,
                op.symbol(),
                format!("type `{}`", self.display(canon)),
            ));
            self.types.push(TypeKind::Error)
        }
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
        self.errors.push(TypeError::invalid_operator(
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
                    OpCategory::Comparison | OpCategory::Equality | OpCategory::Logical => {
                        Some(self.bool_ty())
                    }
                    OpCategory::Arithmetic | OpCategory::Shift | OpCategory::Bitwise => {
                        Some(self.types.canonical(l))
                    }
                }
            }
            (true, false) => self.rule_with_concrete(category, r, l),
            (false, true) => self.rule_with_concrete(category, l, r),
            (false, false) => self.rule_concrete(category, l, r),
        }
    }

    /// The result of an operator (via `category`) with one concrete
    /// operand `c` and one unconstrained variable `v`: the variable adopts
    /// the operator's requirement. `None` when the concrete operand cannot
    /// satisfy the operator.
    fn rule_with_concrete(&mut self, category: OpCategory, c: TypeId, v: TypeId) -> Option<TypeId> {
        let kind = self.types.kind(c)?;
        let is_numeric = matches!(kind, TypeKind::Int | TypeKind::Float);
        let is_int = matches!(kind, TypeKind::Int);
        let is_bool = matches!(kind, TypeKind::Bool);
        let is_scalar = matches!(
            kind,
            TypeKind::Int
                | TypeKind::Float
                | TypeKind::Bool
                | TypeKind::Char
                | TypeKind::Str
                | TypeKind::Null
        );
        match category {
            OpCategory::Arithmetic | OpCategory::Comparison if is_numeric => {
                let _ = self.types.unify(v, c);
                Some(if category == OpCategory::Arithmetic {
                    c
                } else {
                    self.bool_ty()
                })
            }
            OpCategory::Shift | OpCategory::Bitwise if is_int => {
                let _ = self.types.unify(v, c);
                Some(c)
            }
            OpCategory::Equality if is_scalar => {
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

    /// The result of an operator (via `category`) for two concrete
    /// operands, or `None` when the combination is invalid.
    fn rule_concrete(&mut self, category: OpCategory, l: TypeId, r: TypeId) -> Option<TypeId> {
        let lk = self.types.kind(l)?;
        let rk = self.types.kind(r)?;
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
        match category {
            OpCategory::Arithmetic if same_numeric => Some(l),
            OpCategory::Shift | OpCategory::Bitwise if both_int => Some(l),
            OpCategory::Comparison if same_numeric => Some(self.bool_ty()),
            OpCategory::Equality if same_scalar => Some(self.bool_ty()),
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
        self.errors.push(TypeError::invalid_range(
            span,
            format!("`{}` and `{}`", self.display(start), self.display(end)),
        ));
    }

    /// Types a call: the callee must have a function type, the argument
    /// count must match the declared parameters, and each argument must
    /// unify with its parameter. The result type is the function's result.
    ///
    /// Callees without a known type (unresolved names, member/index
    /// results, unconstrained function results) defer honestly: the call
    /// produces a fresh unconstrained variable instead of a fabricated
    /// result.
    fn check_call(
        &mut self,
        callee: &Expr,
        callee_ty: TypeId,
        args: &[(Span, TypeId)],
        span: Span,
    ) -> TypeId {
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
        for (param, (arg_span, arg_ty)) in params.iter().zip(args) {
            if let Err((expected, actual)) = self.types.unify(*param, *arg_ty) {
                self.errors.push(TypeError::mismatch(
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

    /// Types an assignment.
    ///
    /// The semantic stage owns target writability; this stage adds type
    /// compatibility only, and skips it when the target cannot legally be
    /// assigned at all (an immutable or constant binding), so the
    /// immutable-assignment diagnostic is not doubled by a misleading
    /// cascade. Member/index targets are deferred until user-defined types
    /// exist; their base and index expressions are still typed.
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
                self.expr_type(target);
                value_ty
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
                    self.errors.push(TypeError::mismatch(
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
            self.errors.push(TypeError::mismatch(
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
    // Small helpers
    // ------------------------------------------------------------------

    fn bool_ty(&mut self) -> TypeId {
        self.types.push(TypeKind::Bool)
    }

    fn is_numeric(&self, id: TypeId) -> bool {
        matches!(self.types.kind(id), Some(TypeKind::Int | TypeKind::Float))
    }

    fn display(&self, id: TypeId) -> String {
        self.types.display(id)
    }
}
