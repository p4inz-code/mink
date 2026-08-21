//! Monomorphization: creates concrete copies of generic functions at call sites.
//!
//! After parsing, this pass walks the AST and replaces calls to generic
//! functions with calls to concrete (monomorphized) copies. Each generic
//! function instantiation with a unique set of concrete type arguments
//! produces one concrete function with the type parameters substituted.
//!
//! Critical design note: monomorphized functions are clones of the original
//! generic function. To prevent span-based resolution conflicts in the
//! semantic analyzer and type checker, ALL identifiers in the cloned function
//! are reassigned to unique synthetic spans.

use std::collections::HashMap;

use crate::ast::*;
use crate::source::{SourceId, Span};

/// A mapping from generic type parameter names to concrete type arguments.
type TypeSubstitution = HashMap<String, Ty>;

/// The monomorphization state: tracks generic functions and their
/// concrete instantiations.
pub struct Monomorphizer {
    /// Generic function declarations: name → (generic params, function item).
    generic_fns: HashMap<String, (Vec<GenericParam>, FnItem)>,
    /// Concrete instantiations produced so far: "name__T1_T2" → FnItem.
    instantiations: HashMap<String, FnItem>,
    /// Counter for generating unique synthetic spans.
    synthetic_id: u32,
}

impl Default for Monomorphizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Monomorphizer {
    /// Creates a new monomorphizer.
    pub fn new() -> Self {
        Self {
            generic_fns: HashMap::new(),
            instantiations: HashMap::new(),
            synthetic_id: 0,
        }
    }

    /// Generates the next unique synthetic span.
    fn next_span(&mut self) -> Span {
        self.synthetic_id += 1;
        Span::new(SourceId::new(0), self.synthetic_id..self.synthetic_id)
    }

    /// Runs monomorphization on an AST, returning the modified AST with
    /// all generic function calls replaced by calls to concrete copies.
    pub fn run(&mut self, ast: &mut Ast) {
        // Phase 1: Collect generic function declarations.
        for item in &ast.items {
            match &item.kind {
                ItemKind::Fn(f) if !f.generic_params.is_empty() => {
                    self.generic_fns
                        .insert(f.name.name.clone(), (f.generic_params.clone(), f.clone()));
                }
                ItemKind::Pub(pub_item) => {
                    if let ItemKind::Fn(f) = &pub_item.item.kind {
                        if !f.generic_params.is_empty() {
                            self.generic_fns
                                .insert(f.name.name.clone(), (f.generic_params.clone(), f.clone()));
                        }
                    }
                }
                _ => {}
            }
        }

        // Phase 2: Walk all expressions and monomorphize calls.
        for item in &mut ast.items {
            self.monomorphize_item(item);
        }

        // Phase 3: Remove generic function declarations and add concrete
        // instantiations. Generic function bodies are not valid after
        // monomorphization (they reference type parameters that the type
        // checker doesn't understand).
        ast.items.retain(|item| match &item.kind {
            ItemKind::Fn(f) => f.generic_params.is_empty(),
            ItemKind::Pub(pub_item) => {
                if let ItemKind::Fn(f) = &pub_item.item.kind {
                    f.generic_params.is_empty()
                } else {
                    true
                }
            }
            _ => true,
        });
        let mut new_items: Vec<Item> = Vec::new();
        for (_key, fn_item) in self.instantiations.drain() {
            new_items.push(Item {
                kind: ItemKind::Fn(fn_item),
                span: Span::new(SourceId::new(0), 0..0),
            });
        }
        ast.items.extend(new_items);
    }

    fn monomorphize_item(&mut self, item: &mut Item) {
        match &mut item.kind {
            ItemKind::Fn(f) => self.monomorphize_block(&mut f.body),
            ItemKind::Let(binding) => self.monomorphize_expr(&mut binding.init),
            ItemKind::Const(binding) => self.monomorphize_expr(&mut binding.init),
            ItemKind::Pub(pub_item) => self.monomorphize_item(pub_item.item.as_mut()),
            ItemKind::Struct(_) | ItemKind::Enum(_) | ItemKind::Module(_) | ItemKind::Use(_) => {}
        }
    }

    fn monomorphize_block(&mut self, block: &mut Block) {
        for stmt in &mut block.stmts {
            self.monomorphize_stmt(stmt);
        }
        if let Some(result) = &mut block.result {
            self.monomorphize_expr(result);
        }
    }

    fn monomorphize_stmt(&mut self, stmt: &mut Stmt) {
        match &mut stmt.kind {
            StmtKind::Let(binding) => self.monomorphize_expr(&mut binding.init),
            StmtKind::Const(binding) => self.monomorphize_expr(&mut binding.init),
            StmtKind::Return(Some(e)) | StmtKind::Break(Some(e)) => {
                self.monomorphize_expr(e);
            }
            StmtKind::Return(None) | StmtKind::Break(None) | StmtKind::Continue => {}
            StmtKind::If(if_stmt) => {
                self.monomorphize_expr(&mut if_stmt.cond);
                self.monomorphize_block(&mut if_stmt.then_block);
                match &mut if_stmt.else_branch {
                    Some(ElseBranch::Block(b)) => self.monomorphize_block(b),
                    Some(ElseBranch::If(inner)) => {
                        self.monomorphize_expr(&mut inner.cond);
                        self.monomorphize_block(&mut inner.then_block);
                        let mut current = &mut inner.else_branch;
                        while let Some(branch) = current {
                            match branch {
                                ElseBranch::Block(b) => {
                                    self.monomorphize_block(b);
                                    break;
                                }
                                ElseBranch::If(inner) => {
                                    self.monomorphize_expr(&mut inner.cond);
                                    self.monomorphize_block(&mut inner.then_block);
                                    current = &mut inner.else_branch;
                                }
                                ElseBranch::IfExpr(inner) => {
                                    self.monomorphize_if_expr(inner);
                                    break;
                                }
                            }
                        }
                    }
                    Some(ElseBranch::IfExpr(inner)) => self.monomorphize_if_expr(inner),
                    None => {}
                }
            }
            StmtKind::While { cond, body } => {
                self.monomorphize_expr(cond);
                self.monomorphize_block(body);
            }
            StmtKind::For { iterable, body, .. } => {
                self.monomorphize_expr(iterable);
                self.monomorphize_block(body);
            }
            StmtKind::Loop(body) => self.monomorphize_block(body),
            StmtKind::Match(m) => {
                self.monomorphize_expr(&mut m.scrutinee);
                for arm in &mut m.arms {
                    if let Some(guard) = &mut arm.guard {
                        self.monomorphize_expr(guard);
                    }
                    self.monomorphize_block(&mut arm.body);
                }
            }
            StmtKind::Expr(e) => self.monomorphize_expr(e),
        }
    }

    fn monomorphize_if_expr(&mut self, expr: &mut IfExpr) {
        self.monomorphize_expr(&mut expr.cond);
        self.monomorphize_block(&mut expr.then_block);
        match &mut expr.else_branch {
            ElseBranch::Block(b) => self.monomorphize_block(b),
            ElseBranch::If(inner) => {
                self.monomorphize_expr(&mut inner.cond);
                self.monomorphize_block(&mut inner.then_block);
                let mut current = &mut inner.else_branch;
                while let Some(branch) = current {
                    match branch {
                        ElseBranch::Block(b) => {
                            self.monomorphize_block(b);
                            break;
                        }
                        ElseBranch::If(inner) => {
                            self.monomorphize_expr(&mut inner.cond);
                            self.monomorphize_block(&mut inner.then_block);
                            current = &mut inner.else_branch;
                        }
                        ElseBranch::IfExpr(inner) => {
                            self.monomorphize_if_expr(inner);
                            break;
                        }
                    }
                }
            }
            ElseBranch::IfExpr(inner) => self.monomorphize_if_expr(inner),
        }
    }

    fn monomorphize_expr(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Call {
                callee,
                args,
                type_args,
            } => {
                self.monomorphize_expr(callee);
                for arg in args.iter_mut() {
                    self.monomorphize_expr(arg);
                }

                // Check if this is a call to a generic function.
                let fn_name = if let ExprKind::Ident(ident) = &callee.kind {
                    Some(ident.name.clone())
                } else {
                    None
                };
                if let Some(name) = fn_name {
                    if let Some((generic_params, fn_item)) = self.generic_fns.get(&name) {
                        let generic_params = generic_params.clone();
                        let fn_item = fn_item.clone();
                        if let Some(subst) = self.infer_type_args(
                            &generic_params,
                            &fn_item.params,
                            args,
                            type_args.as_deref(),
                        ) {
                            let concrete_name = self.mangle_name(&name, &subst);
                            if !self.instantiations.contains_key(&concrete_name) {
                                let concrete_fn =
                                    self.instantiate_fn(&fn_item, &subst, &concrete_name);
                                self.instantiations
                                    .insert(concrete_name.clone(), concrete_fn);
                            }
                            if let ExprKind::Ident(ident) = &mut callee.kind {
                                ident.name = concrete_name;
                            }
                            *type_args = None;
                        }
                    }
                }
            }
            ExprKind::Unary { operand, .. } => self.monomorphize_expr(operand),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.monomorphize_expr(lhs);
                self.monomorphize_expr(rhs);
            }
            ExprKind::Assign { target, value, .. } => {
                self.monomorphize_expr(target);
                self.monomorphize_expr(value);
            }
            ExprKind::Range { start, end, .. } => {
                self.monomorphize_expr(start);
                self.monomorphize_expr(end);
            }
            ExprKind::Member { base, .. } => self.monomorphize_expr(base),
            ExprKind::Index { base, index } => {
                self.monomorphize_expr(base);
                self.monomorphize_expr(index);
            }
            ExprKind::StructLit { fields, .. } => {
                for field in fields.iter_mut() {
                    self.monomorphize_expr(&mut field.value);
                }
            }
            ExprKind::ArrayLit(elems) => {
                for e in elems.iter_mut() {
                    self.monomorphize_expr(e);
                }
            }
            ExprKind::EnumVariant { payload, .. } => {
                if let Some(p) = payload {
                    self.monomorphize_expr(p);
                }
            }
            ExprKind::IfExpr(inner) => self.monomorphize_if_expr(inner),
            ExprKind::Block(b) => self.monomorphize_block(b),
            ExprKind::Tuple(elems) => {
                for e in elems.iter_mut() {
                    self.monomorphize_expr(e);
                }
            }
            ExprKind::TupleFieldAccess { base, .. } => self.monomorphize_expr(base),
            ExprKind::WhileExpr { cond, body, .. } => {
                self.monomorphize_expr(cond);
                self.monomorphize_block(body);
            }
            ExprKind::LoopExpr { body, .. } => self.monomorphize_block(body),
            ExprKind::MatchExpr(m) => {
                self.monomorphize_expr(&mut m.scrutinee);
                for arm in m.arms.iter_mut() {
                    if let Some(guard) = &mut arm.guard {
                        self.monomorphize_expr(guard);
                    }
                    self.monomorphize_expr(&mut arm.body);
                }
            }
            ExprKind::Int
            | ExprKind::Float
            | ExprKind::Str
            | ExprKind::Char
            | ExprKind::Bool(_)
            | ExprKind::Null
            | ExprKind::Ident(_) => {}
            ExprKind::Borrow { operand, .. } | ExprKind::Deref { operand } => {
                self.monomorphize_expr(operand);
            }
            ExprKind::Group(inner) => self.monomorphize_expr(inner),
        }
    }

    /// Infers type arguments for a generic function call.
    fn infer_type_args(
        &self,
        generic_params: &[GenericParam],
        fn_params: &[Param],
        args: &[Expr],
        _explicit_type_args: Option<&[Ty]>,
    ) -> Option<TypeSubstitution> {
        let mut subst = TypeSubstitution::new();

        for (param, arg) in fn_params.iter().zip(args.iter()) {
            if let Some(param_ty) = &param.ty {
                self.match_type_against_expr(param_ty, arg, &mut subst);
            }
        }

        for gp in generic_params {
            if !subst.contains_key(&gp.name.name) {
                return None;
            }
        }

        Some(subst)
    }

    fn match_type_against_expr(&self, ty: &Ty, expr: &Expr, subst: &mut TypeSubstitution) {
        match &ty.kind {
            TyKind::GenericParam(ident) => {
                if let Some(concrete) = self.infer_type_from_expr(expr) {
                    subst.insert(ident.name.clone(), concrete);
                }
            }
            TyKind::Named(ident) => {
                // A Named type might be a generic parameter reference.
                // Check if it's in our generic params list (indicated by
                // being a single uppercase letter like T, U, V).
                if ident.name.len() == 1 && ident.name.chars().next().unwrap().is_uppercase() {
                    if let Some(concrete) = self.infer_type_from_expr(expr) {
                        subst.insert(ident.name.clone(), concrete);
                    }
                }
            }
            TyKind::NamedApp { args, .. } => {
                if let ExprKind::Call {
                    args: call_args, ..
                } = &expr.kind
                {
                    for (ty_arg, call_arg) in args.iter().zip(call_args.iter()) {
                        self.match_type_against_expr(ty_arg, call_arg, subst);
                    }
                }
            }
            TyKind::Tuple(elems) => {
                if let ExprKind::Tuple(tuple_elems) = &expr.kind {
                    for (elem_ty, tuple_elem) in elems.iter().zip(tuple_elems.iter()) {
                        self.match_type_against_expr(elem_ty, tuple_elem, subst);
                    }
                }
            }
            _ => {}
        }
    }

    fn infer_type_from_expr(&self, expr: &Expr) -> Option<Ty> {
        let name = match &expr.kind {
            ExprKind::Int => "Int",
            ExprKind::Float => "Float",
            ExprKind::Bool(_) => "Bool",
            ExprKind::Char => "Char",
            ExprKind::Str => "Str",
            ExprKind::Null => "Null",
            _ => return None,
        };
        Some(Ty {
            kind: TyKind::Named(Ident {
                name: name.to_string(),
                span: expr.span,
            }),
            span: expr.span,
        })
    }

    fn mangle_name(&mut self, base: &str, subst: &TypeSubstitution) -> String {
        let mut parts = vec![base.to_string()];
        let mut sorted: Vec<_> = subst.iter().collect();
        sorted.sort_by_key(|(k, _)| (*k).clone());
        for (_param_name, ty) in sorted {
            parts.push(self.type_to_string(ty));
        }
        parts.join("__")
    }

    fn type_to_string(&self, ty: &Ty) -> String {
        match &ty.kind {
            TyKind::Named(ident) => ident.name.clone(),
            TyKind::GenericParam(ident) => ident.name.clone(),
            TyKind::Ptr(inner) => format!("Ptr_{}", self.type_to_string(inner)),
            TyKind::Ref { inner, mutable } => {
                let prefix = if *mutable { "RefMut" } else { "Ref" };
                format!("{}_{}", prefix, self.type_to_string(inner))
            }
            TyKind::Array { elem, .. } => format!("Arr_{}", self.type_to_string(elem)),
            TyKind::Tuple(elems) => {
                let inner: Vec<_> = elems.iter().map(|e| self.type_to_string(e)).collect();
                format!("Tuple_{}", inner.join("_"))
            }
            TyKind::NamedApp { name, args } => {
                let inner: Vec<_> = args.iter().map(|a| self.type_to_string(a)).collect();
                format!("{}_{}", name.name, inner.join("_"))
            }
        }
    }

    // ------------------------------------------------------------------
    // Instantiation: clone + substitute types + reassign all spans
    // ------------------------------------------------------------------

    fn instantiate_fn(
        &mut self,
        fn_item: &FnItem,
        subst: &TypeSubstitution,
        concrete_name: &str,
    ) -> FnItem {
        let mut new_fn = fn_item.clone();
        new_fn.generic_params = Vec::new();

        // Set the concrete name and assign a unique synthetic span.
        new_fn.name = Ident {
            name: concrete_name.to_string(),
            span: self.next_span(),
        };

        // Reassign parameter names to unique spans.
        for param in &mut new_fn.params {
            param.name.span = self.next_span();
            if let Some(ty) = &mut param.ty {
                self.substitute_type(ty, subst);
                self.reassign_type_spans(ty);
            }
        }

        // Reassign return type spans.
        if let Some(ret_ty) = &mut new_fn.return_ty {
            self.substitute_type(ret_ty, subst);
            self.reassign_type_spans(ret_ty);
        }

        // Substitute types in the body, then reassign all body spans.
        self.substitute_block(&mut new_fn.body, subst);
        self.reassign_block_spans(&mut new_fn.body);

        new_fn
    }

    // ------------------------------------------------------------------
    // Type substitution
    // ------------------------------------------------------------------

    fn substitute_type(&self, ty: &mut Ty, subst: &TypeSubstitution) {
        match &mut ty.kind {
            TyKind::GenericParam(ident) => {
                if let Some(concrete) = subst.get(&ident.name) {
                    *ty = concrete.clone();
                }
            }
            TyKind::Named(ident) => {
                // Named types may be generic parameter references
                // (parser stores them as Named, not GenericParam).
                if let Some(concrete) = subst.get(&ident.name) {
                    *ty = concrete.clone();
                }
            }
            TyKind::NamedApp { args, .. } => {
                for arg in args.iter_mut() {
                    self.substitute_type(arg, subst);
                }
            }
            TyKind::Ptr(inner) => self.substitute_type(inner, subst),
            TyKind::Ref { inner, .. } => self.substitute_type(inner, subst),
            TyKind::Array { elem, .. } => self.substitute_type(elem, subst),
            TyKind::Tuple(elems) => {
                for elem in elems.iter_mut() {
                    self.substitute_type(elem, subst);
                }
            }
        }
    }

    fn substitute_block(&self, block: &mut Block, subst: &TypeSubstitution) {
        for stmt in &mut block.stmts {
            self.substitute_stmt(stmt, subst);
        }
        if let Some(result) = &mut block.result {
            self.substitute_expr(result, subst);
        }
    }

    fn substitute_stmt(&self, stmt: &mut Stmt, subst: &TypeSubstitution) {
        match &mut stmt.kind {
            StmtKind::Let(binding) => {
                if let Some(ty) = &mut binding.ty {
                    self.substitute_type(ty, subst);
                }
                self.substitute_expr(&mut binding.init, subst);
            }
            StmtKind::Const(binding) => {
                if let Some(ty) = &mut binding.ty {
                    self.substitute_type(ty, subst);
                }
                self.substitute_expr(&mut binding.init, subst);
            }
            StmtKind::Return(Some(e)) | StmtKind::Break(Some(e)) => {
                self.substitute_expr(e, subst);
            }
            StmtKind::Return(None) | StmtKind::Break(None) | StmtKind::Continue => {}
            StmtKind::If(if_stmt) => {
                self.substitute_expr(&mut if_stmt.cond, subst);
                self.substitute_block(&mut if_stmt.then_block, subst);
                if let Some(branch) = &mut if_stmt.else_branch {
                    self.substitute_else_branch(branch, subst);
                }
            }
            StmtKind::While { cond, body } => {
                self.substitute_expr(cond, subst);
                self.substitute_block(body, subst);
            }
            StmtKind::For { iterable, body, .. } => {
                self.substitute_expr(iterable, subst);
                self.substitute_block(body, subst);
            }
            StmtKind::Loop(body) => self.substitute_block(body, subst),
            StmtKind::Match(m) => {
                self.substitute_expr(&mut m.scrutinee, subst);
                for arm in &mut m.arms {
                    if let Some(guard) = &mut arm.guard {
                        self.substitute_expr(guard, subst);
                    }
                    self.substitute_block(&mut arm.body, subst);
                }
            }
            StmtKind::Expr(e) => self.substitute_expr(e, subst),
        }
    }

    fn substitute_else_branch(&self, branch: &mut ElseBranch, subst: &TypeSubstitution) {
        match branch {
            ElseBranch::Block(b) => self.substitute_block(b, subst),
            ElseBranch::If(inner) => {
                self.substitute_expr(&mut inner.cond, subst);
                self.substitute_block(&mut inner.then_block, subst);
                if let Some(next) = &mut inner.else_branch {
                    self.substitute_else_branch(next, subst);
                }
            }
            ElseBranch::IfExpr(inner) => self.substitute_if_expr(inner, subst),
        }
    }

    fn substitute_if_expr(&self, expr: &mut IfExpr, subst: &TypeSubstitution) {
        self.substitute_expr(&mut expr.cond, subst);
        self.substitute_block(&mut expr.then_block, subst);
        self.substitute_else_branch(&mut expr.else_branch, subst);
    }

    fn substitute_expr(&self, expr: &mut Expr, subst: &TypeSubstitution) {
        match &mut expr.kind {
            ExprKind::Call { callee, args, .. } => {
                self.substitute_expr(callee, subst);
                for arg in args.iter_mut() {
                    self.substitute_expr(arg, subst);
                }
            }
            ExprKind::Unary { operand, .. } => self.substitute_expr(operand, subst),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.substitute_expr(lhs, subst);
                self.substitute_expr(rhs, subst);
            }
            ExprKind::Assign { target, value, .. } => {
                self.substitute_expr(target, subst);
                self.substitute_expr(value, subst);
            }
            ExprKind::Range { start, end, .. } => {
                self.substitute_expr(start, subst);
                self.substitute_expr(end, subst);
            }
            ExprKind::Member { base, .. } => self.substitute_expr(base, subst),
            ExprKind::Index { base, index } => {
                self.substitute_expr(base, subst);
                self.substitute_expr(index, subst);
            }
            ExprKind::StructLit { fields, .. } => {
                for field in fields.iter_mut() {
                    self.substitute_expr(&mut field.value, subst);
                }
            }
            ExprKind::ArrayLit(elems) => {
                for e in elems.iter_mut() {
                    self.substitute_expr(e, subst);
                }
            }
            ExprKind::EnumVariant { payload, .. } => {
                if let Some(p) = payload {
                    self.substitute_expr(p, subst);
                }
            }
            ExprKind::IfExpr(inner) => self.substitute_if_expr(inner, subst),
            ExprKind::Block(b) => self.substitute_block(b, subst),
            ExprKind::Tuple(elems) => {
                for e in elems.iter_mut() {
                    self.substitute_expr(e, subst);
                }
            }
            ExprKind::TupleFieldAccess { base, .. } => self.substitute_expr(base, subst),
            ExprKind::WhileExpr { cond, body, .. } => {
                self.substitute_expr(cond, subst);
                self.substitute_block(body, subst);
            }
            ExprKind::LoopExpr { body, .. } => self.substitute_block(body, subst),
            ExprKind::MatchExpr(m) => {
                self.substitute_expr(&mut m.scrutinee, subst);
                for arm in m.arms.iter_mut() {
                    if let Some(guard) = &mut arm.guard {
                        self.substitute_expr(guard, subst);
                    }
                    self.substitute_expr(&mut arm.body, subst);
                }
            }
            ExprKind::Borrow { operand, .. } | ExprKind::Deref { operand } => {
                self.substitute_expr(operand, subst);
            }
            ExprKind::Group(inner) => self.substitute_expr(inner, subst),
            ExprKind::Int
            | ExprKind::Float
            | ExprKind::Str
            | ExprKind::Char
            | ExprKind::Bool(_)
            | ExprKind::Null
            | ExprKind::Ident(_) => {}
        }
    }

    // ------------------------------------------------------------------
    // Span reassignment: give every identifier in a monomorphized
    // function a unique synthetic span to prevent resolution conflicts.
    // ------------------------------------------------------------------

    fn reassign_type_spans(&mut self, ty: &mut Ty) {
        ty.span = self.next_span();
        match &mut ty.kind {
            TyKind::Named(ident) | TyKind::GenericParam(ident) => {
                ident.span = self.next_span();
            }
            TyKind::NamedApp { name, args } => {
                name.span = self.next_span();
                for arg in args.iter_mut() {
                    self.reassign_type_spans(arg);
                }
            }
            TyKind::Ptr(inner) => self.reassign_type_spans(inner),
            TyKind::Ref { inner, .. } => self.reassign_type_spans(inner),
            TyKind::Array { elem, .. } => self.reassign_type_spans(elem),
            TyKind::Tuple(elems) => {
                for elem in elems.iter_mut() {
                    self.reassign_type_spans(elem);
                }
            }
        }
    }

    fn reassign_block_spans(&mut self, block: &mut Block) {
        for stmt in &mut block.stmts {
            self.reassign_stmt_spans(stmt);
        }
        if let Some(result) = &mut block.result {
            self.reassign_expr_spans(result);
        }
    }

    fn reassign_stmt_spans(&mut self, stmt: &mut Stmt) {
        stmt.span = self.next_span();
        match &mut stmt.kind {
            StmtKind::Let(binding) => {
                binding.name.span = self.next_span();
                if let Some(ref mut pattern) = binding.pattern {
                    self.reassign_pattern_spans(pattern);
                }
                if let Some(ty) = &mut binding.ty {
                    self.reassign_type_spans(ty);
                }
                self.reassign_expr_spans(&mut binding.init);
            }
            StmtKind::Const(binding) => {
                binding.name.span = self.next_span();
                if let Some(ty) = &mut binding.ty {
                    self.reassign_type_spans(ty);
                }
                self.reassign_expr_spans(&mut binding.init);
            }
            StmtKind::Return(Some(e)) | StmtKind::Break(Some(e)) => {
                self.reassign_expr_spans(e);
            }
            StmtKind::Return(None) | StmtKind::Break(None) | StmtKind::Continue => {}
            StmtKind::If(if_stmt) => {
                self.reassign_expr_spans(&mut if_stmt.cond);
                self.reassign_block_spans(&mut if_stmt.then_block);
                if let Some(branch) = &mut if_stmt.else_branch {
                    self.reassign_else_branch_spans(branch);
                }
            }
            StmtKind::While { cond, body } => {
                self.reassign_expr_spans(cond);
                self.reassign_block_spans(body);
            }
            StmtKind::For { iterable, body, .. } => {
                self.reassign_expr_spans(iterable);
                self.reassign_block_spans(body);
            }
            StmtKind::Loop(body) => self.reassign_block_spans(body),
            StmtKind::Match(m) => {
                self.reassign_expr_spans(&mut m.scrutinee);
                for arm in &mut m.arms {
                    if let Some(guard) = &mut arm.guard {
                        self.reassign_expr_spans(guard);
                    }
                    self.reassign_block_spans(&mut arm.body);
                }
            }
            StmtKind::Expr(e) => self.reassign_expr_spans(e),
        }
    }

    fn reassign_else_branch_spans(&mut self, branch: &mut ElseBranch) {
        match branch {
            ElseBranch::Block(b) => self.reassign_block_spans(b),
            ElseBranch::If(inner) => {
                self.reassign_expr_spans(&mut inner.cond);
                self.reassign_block_spans(&mut inner.then_block);
                if let Some(next) = &mut inner.else_branch {
                    self.reassign_else_branch_spans(next);
                }
            }
            ElseBranch::IfExpr(inner) => self.reassign_if_expr_spans(inner),
        }
    }

    fn reassign_if_expr_spans(&mut self, expr: &mut IfExpr) {
        self.reassign_expr_spans(&mut expr.cond);
        self.reassign_block_spans(&mut expr.then_block);
        self.reassign_else_branch_spans(&mut expr.else_branch);
    }

    fn reassign_expr_spans(&mut self, expr: &mut Expr) {
        expr.span = self.next_span();
        match &mut expr.kind {
            ExprKind::Ident(ident) => {
                ident.span = expr.span;
            }
            ExprKind::Call { callee, args, .. } => {
                self.reassign_expr_spans(callee);
                for arg in args.iter_mut() {
                    self.reassign_expr_spans(arg);
                }
            }
            ExprKind::Unary { operand, .. } => self.reassign_expr_spans(operand),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.reassign_expr_spans(lhs);
                self.reassign_expr_spans(rhs);
            }
            ExprKind::Assign { target, value, .. } => {
                self.reassign_expr_spans(target);
                self.reassign_expr_spans(value);
            }
            ExprKind::Range { start, end, .. } => {
                self.reassign_expr_spans(start);
                self.reassign_expr_spans(end);
            }
            ExprKind::Member { base, member, .. } => {
                self.reassign_expr_spans(base);
                member.span = self.next_span();
            }
            ExprKind::Index { base, index } => {
                self.reassign_expr_spans(base);
                self.reassign_expr_spans(index);
            }
            ExprKind::StructLit { name, fields } => {
                name.span = self.next_span();
                for field in fields.iter_mut() {
                    field.name.span = self.next_span();
                    self.reassign_expr_spans(&mut field.value);
                }
            }
            ExprKind::ArrayLit(elems) => {
                for e in elems.iter_mut() {
                    self.reassign_expr_spans(e);
                }
            }
            ExprKind::EnumVariant {
                name,
                variant,
                payload,
            } => {
                name.span = self.next_span();
                variant.span = self.next_span();
                if let Some(p) = payload {
                    self.reassign_expr_spans(p);
                }
            }
            ExprKind::IfExpr(inner) => self.reassign_if_expr_spans(inner),
            ExprKind::Block(b) => self.reassign_block_spans(b),
            ExprKind::Tuple(elems) => {
                for e in elems.iter_mut() {
                    self.reassign_expr_spans(e);
                }
            }
            ExprKind::TupleFieldAccess { base, .. } => self.reassign_expr_spans(base),
            ExprKind::WhileExpr { cond, body, .. } => {
                self.reassign_expr_spans(cond);
                self.reassign_block_spans(body);
            }
            ExprKind::LoopExpr { body, .. } => self.reassign_block_spans(body),
            ExprKind::MatchExpr(m) => {
                self.reassign_expr_spans(&mut m.scrutinee);
                for arm in m.arms.iter_mut() {
                    if let Some(guard) = &mut arm.guard {
                        self.reassign_expr_spans(guard);
                    }
                    self.reassign_expr_spans(&mut arm.body);
                }
            }
            ExprKind::Borrow { operand, .. } | ExprKind::Deref { operand } => {
                self.reassign_expr_spans(operand);
            }
            ExprKind::Group(inner) => self.reassign_expr_spans(inner),
            ExprKind::Int
            | ExprKind::Float
            | ExprKind::Str
            | ExprKind::Char
            | ExprKind::Bool(_)
            | ExprKind::Null => {}
        }
    }

    fn reassign_pattern_spans(&mut self, pattern: &mut Pattern) {
        match pattern {
            Pattern::Wildcard { span } => {
                *span = self.next_span();
            }
            Pattern::Binding(ident) => {
                ident.span = self.next_span();
            }
            Pattern::EnumVariant {
                name,
                variant,
                payload,
                ..
            } => {
                name.span = self.next_span();
                variant.span = self.next_span();
                if let Some(p) = payload {
                    self.reassign_pattern_spans(p);
                }
            }
            Pattern::Bool { span, .. } => {
                *span = self.next_span();
            }
            Pattern::Int { literal, span, .. } => {
                *span = self.next_span();
                self.reassign_expr_spans(literal);
            }
            Pattern::Range { lo, hi, span, .. } => {
                *span = self.next_span();
                self.reassign_pattern_spans(lo);
                self.reassign_pattern_spans(hi);
            }
            Pattern::Or { alternatives, span } => {
                *span = self.next_span();
                for p in alternatives.iter_mut() {
                    self.reassign_pattern_spans(p);
                }
            }
            Pattern::Tuple { elements, span } => {
                *span = self.next_span();
                for elem in elements.iter_mut() {
                    self.reassign_pattern_spans(elem);
                }
            }
            Pattern::Struct { name, fields, span } => {
                *span = self.next_span();
                name.span = self.next_span();
                for field in fields.iter_mut() {
                    field.name.span = self.next_span();
                    field.span = self.next_span();
                    if let Some(p) = &mut field.binding {
                        self.reassign_pattern_spans(p);
                    }
                }
            }
        }
    }
}

/// Runs monomorphization on the given AST.
pub fn monomorphize(ast: &mut Ast) {
    let mut mono = Monomorphizer::new();
    mono.run(ast);
}
