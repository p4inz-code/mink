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
    ItemKind, Stmt, StmtKind, UnaryOp,
};
use crate::semantics::{SemanticResult, SymbolId, SymbolKind};
use crate::source::{SourceMap, Span};

use super::TypeResult;
use super::error::TypeError;
use super::ty::{TypeId, TypeKind, TypeTable};

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
    /// The source map, for reading literal source text (the
    /// null-pointer-constant rule).
    sources: &'a SourceMap,
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
            StmtKind::Expr(expr) => {
                self.expr_type(expr);
            }
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
            self.errors.push(TypeError::mismatch(
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
                self.errors.push(TypeError::mismatch(
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

    /// Reports an invalid unary operand combination and returns the
    /// poisoned error type.
    fn unary_error(&mut self, op: UnaryOp, operand_ty: TypeId, span: Span) -> TypeId {
        self.errors.push(TypeError::invalid_operator(
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
