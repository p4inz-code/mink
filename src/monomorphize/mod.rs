//! Monomorphization: creates concrete copies of generic functions, structs,
//! and enums at their use sites.
//!
//! After parsing, this pass walks the AST and:
//! 1. Replaces calls to generic functions with calls to concrete copies.
//! 2. Resolves `NamedApp` type references to concrete struct/enum names.
//! 3. Resolves struct literal and enum variant names to concrete names.
//! 4. Creates concrete struct/enum declarations for each instantiation.
//!
//! Critical design note: monomorphized items are clones of the original
//! generic declarations. To prevent span-based resolution conflicts in the
//! semantic analyzer and type checker, ALL identifiers in cloned items are
//! reassigned to unique synthetic spans.

use std::collections::HashMap;

use crate::ast::*;
use crate::source::{SourceId, Span};

/// A mapping from generic type parameter names to concrete type arguments.
type TypeSubstitution = HashMap<String, Ty>;

/// The monomorphization state: tracks generic functions/structs/enums and
/// their concrete instantiations.
pub struct Monomorphizer {
    /// Generic function declarations: name → (generic params, function item).
    generic_fns: HashMap<String, (Vec<GenericParam>, FnItem)>,
    /// Generic struct declarations: name → (generic params, struct item).
    generic_structs: HashMap<String, (Vec<GenericParam>, StructItem)>,
    /// Generic enum declarations: name → (generic params, enum item).
    generic_enums: HashMap<String, (Vec<GenericParam>, EnumItem)>,
    /// Concrete function instantiations produced so far: "name__T1_T2" → FnItem.
    instantiations: HashMap<String, FnItem>,
    /// Concrete struct instantiations: "name__T1_T2" → StructItem.
    concrete_structs: HashMap<String, StructItem>,
    /// Concrete enum instantiations: "name__T1_T2" → EnumItem.
    concrete_enums: HashMap<String, EnumItem>,
    /// Generated closure functions: "__closure_N" → FnItem.
    closure_fns: HashMap<String, FnItem>,
    /// Captured variable names per closure: "__closure_N" → ["y", "z", ...].
    closure_captures: HashMap<String, Vec<String>>,
    /// Counter for generating unique synthetic spans.
    synthetic_id: u32,
    /// Counter for generating unique closure names.
    closure_counter: u32,
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
            generic_structs: HashMap::new(),
            generic_enums: HashMap::new(),
            instantiations: HashMap::new(),
            concrete_structs: HashMap::new(),
            concrete_enums: HashMap::new(),
            closure_fns: HashMap::new(),
            closure_captures: HashMap::new(),
            synthetic_id: 0,
            closure_counter: 0,
        }
    }

    /// Generates the next unique synthetic span.
    fn next_span(&mut self) -> Span {
        self.synthetic_id += 1;
        Span::new(SourceId::new(0), self.synthetic_id..self.synthetic_id)
    }

    /// Runs monomorphization on an AST, replacing all generic uses with
    /// concrete copies.
    pub fn run(&mut self, ast: &mut Ast) {
        // Phase 1: Collect all generic declarations (functions, structs, enums).
        for item in &ast.items {
            match &item.kind {
                ItemKind::Fn(f) if !f.generic_params.is_empty() => {
                    self.generic_fns
                        .insert(f.name.name.clone(), (f.generic_params.clone(), f.clone()));
                }
                ItemKind::Struct(s) if !s.generic_params.is_empty() => {
                    self.generic_structs
                        .insert(s.name.name.clone(), (s.generic_params.clone(), s.clone()));
                }
                ItemKind::Enum(e) if !e.generic_params.is_empty() => {
                    self.generic_enums
                        .insert(e.name.name.clone(), (e.generic_params.clone(), e.clone()));
                }
                ItemKind::Pub(pub_item) => match &pub_item.item.kind {
                    ItemKind::Fn(f) if !f.generic_params.is_empty() => {
                        self.generic_fns
                            .insert(f.name.name.clone(), (f.generic_params.clone(), f.clone()));
                    }
                    ItemKind::Struct(s) if !s.generic_params.is_empty() => {
                        self.generic_structs
                            .insert(s.name.name.clone(), (s.generic_params.clone(), s.clone()));
                    }
                    ItemKind::Enum(e) if !e.generic_params.is_empty() => {
                        self.generic_enums
                            .insert(e.name.name.clone(), (e.generic_params.clone(), e.clone()));
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        // Phase 2: Resolve all NamedApp types in the AST.
        // This walks type annotations in function signatures, let bindings,
        // struct fields, enum payloads, etc. It replaces NamedApp types
        // (like Pair<Int>) with Named types (like Pair__Int).
        self.resolve_all_named_app_types(ast);

        // Phase 3: Walk all expressions and monomorphize.
        for item in &mut ast.items {
            self.monomorphize_item(item);
        }

        // Phase 3.5: Rewrite closure call sites.
        // After Phase 3, closures are replaced with Ident("__closure_N").
        // But call sites still use the variable name (e.g., `add_y(32)`).
        // This phase finds `let x = __closure_N` bindings and rewrites
        // `x(args)` to `__closure_N(captured_vars..., args)`.
        self.rewrite_closure_calls(ast);

        // Phase 4: Remove generic declarations and add concrete copies.
        ast.items.retain(|item| match &item.kind {
            ItemKind::Fn(f) => f.generic_params.is_empty(),
            ItemKind::Struct(s) => s.generic_params.is_empty(),
            ItemKind::Enum(e) => e.generic_params.is_empty(),
            ItemKind::Pub(pub_item) => match &pub_item.item.kind {
                ItemKind::Fn(f) => f.generic_params.is_empty(),
                ItemKind::Struct(s) => s.generic_params.is_empty(),
                ItemKind::Enum(e) => e.generic_params.is_empty(),
                _ => true,
            },
            _ => true,
        });
        let mut new_items: Vec<Item> = Vec::new();
        for (_key, fn_item) in self.instantiations.drain() {
            new_items.push(Item {
                kind: ItemKind::Fn(fn_item),
                span: Span::new(SourceId::new(0), 0..0),
            });
        }
        for (_key, struct_item) in self.concrete_structs.drain() {
            new_items.push(Item {
                kind: ItemKind::Struct(struct_item),
                span: Span::new(SourceId::new(0), 0..0),
            });
        }
        for (_key, enum_item) in self.concrete_enums.drain() {
            new_items.push(Item {
                kind: ItemKind::Enum(enum_item),
                span: Span::new(SourceId::new(0), 0..0),
            });
        }
        for (_key, closure_fn) in self.closure_fns.drain() {
            new_items.push(Item {
                kind: ItemKind::Fn(closure_fn),
                span: Span::new(SourceId::new(0), 0..0),
            });
        }
        ast.items.extend(new_items);
    }

    // ------------------------------------------------------------------
    // Phase 2: Resolve NamedApp types
    // ------------------------------------------------------------------

    /// Walks all types in the AST and resolves NamedApp types to concrete
    /// Named types where all type arguments are already concrete.
    fn resolve_all_named_app_types(&mut self, ast: &mut Ast) {
        for item in &mut ast.items {
            self.resolve_named_app_in_item(item);
        }
    }

    fn resolve_named_app_in_item(&mut self, item: &mut Item) {
        match &mut item.kind {
            ItemKind::Fn(f) => {
                for param in &mut f.params {
                    if let Some(ty) = &mut param.ty {
                        self.resolve_named_app_type(ty);
                    }
                }
                if let Some(ret) = &mut f.return_ty {
                    self.resolve_named_app_type(ret);
                }
            }
            ItemKind::Struct(s) => {
                for field in &mut s.fields {
                    self.resolve_named_app_type(&mut field.ty);
                }
            }
            ItemKind::Enum(e) => {
                for variant in &mut e.variants {
                    if let Some(ty) = &mut variant.payload {
                        self.resolve_named_app_type(ty);
                    }
                }
            }
            ItemKind::Let(binding) => {
                if let Some(ty) = &mut binding.ty {
                    self.resolve_named_app_type(ty);
                }
            }
            ItemKind::Const(binding) => {
                if let Some(ty) = &mut binding.ty {
                    self.resolve_named_app_type(ty);
                }
            }
            ItemKind::Pub(pub_item) => self.resolve_named_app_in_item(pub_item.item.as_mut()),
            ItemKind::Module(_) | ItemKind::Use(_) => {}
        }
    }

    /// Resolves a NamedApp type to a concrete Named type.
    /// Only resolves when the name refers to a generic struct/enum AND
    /// all type arguments are concrete (no generic parameters remain).
    fn resolve_named_app_type(&mut self, ty: &mut Ty) {
        // First, resolve any nested NamedApp types in the args.
        match &mut ty.kind {
            TyKind::NamedApp { args, .. } => {
                for arg in args.iter_mut() {
                    self.resolve_named_app_type(arg);
                }
            }
            TyKind::Ptr(inner) => self.resolve_named_app_type(inner),
            TyKind::Ref { inner, .. } => self.resolve_named_app_type(inner),
            TyKind::Array { elem, .. } => self.resolve_named_app_type(elem),
            TyKind::Tuple(elems) => {
                for elem in elems.iter_mut() {
                    self.resolve_named_app_type(elem);
                }
            }
            _ => return,
        }

        // Now try to resolve the NamedApp itself.
        // Extract the name and args, then drop the borrow so we can mutate ty.
        let (type_name, type_args, type_span) = if let TyKind::NamedApp { name, args } = &ty.kind {
            (name.name.clone(), args.clone(), ty.span)
        } else {
            return;
        };

        // Check if all args are concrete (no generic params).
        if !type_args.iter().all(|a| self.ty_is_concrete(a)) {
            return;
        }

        // Check if the name is a generic struct.
        if let Some((generic_params, struct_item)) = self.generic_structs.get(&type_name) {
            let generic_params = generic_params.clone();
            let struct_item = struct_item.clone();
            let subst = self.build_substitution(&generic_params, &type_args);
            let concrete_name = self.mangle_name(&type_name, &subst);
            if !self.concrete_structs.contains_key(&concrete_name) {
                let concrete = self.create_concrete_struct(&struct_item, &subst, &concrete_name);
                self.concrete_structs
                    .insert(concrete_name.clone(), concrete);
            }
            *ty = Ty {
                kind: TyKind::Named(Ident {
                    name: concrete_name.clone(),
                    span: type_span,
                }),
                span: type_span,
            };
            return;
        }

        // Check if the name is a generic enum.
        if let Some((generic_params, enum_item)) = self.generic_enums.get(&type_name) {
            let generic_params = generic_params.clone();
            let enum_item = enum_item.clone();
            let subst = self.build_substitution(&generic_params, &type_args);
            let concrete_name = self.mangle_name(&type_name, &subst);
            if !self.concrete_enums.contains_key(&concrete_name) {
                let concrete = self.create_concrete_enum(&enum_item, &subst, &concrete_name);
                self.concrete_enums.insert(concrete_name.clone(), concrete);
            }
            *ty = Ty {
                kind: TyKind::Named(Ident {
                    name: concrete_name.clone(),
                    span: type_span,
                }),
                span: type_span,
            };
        }
    }

    /// Returns true if a type contains no generic parameters.
    fn ty_is_concrete(&self, ty: &Ty) -> bool {
        match &ty.kind {
            TyKind::Named(ident) => {
                // A single uppercase letter is a generic parameter
                // (parser stores them as Named, not GenericParam).
                !(ident.name.len() == 1 && ident.name.chars().next().unwrap().is_uppercase())
            }
            TyKind::GenericParam(_) => false,
            TyKind::NamedApp { args, .. } => args.iter().all(|a| self.ty_is_concrete(a)),
            TyKind::Ptr(inner) => self.ty_is_concrete(inner),
            TyKind::Ref { inner, .. } => self.ty_is_concrete(inner),
            TyKind::Array { elem, .. } => self.ty_is_concrete(elem),
            TyKind::Tuple(elems) => elems.iter().all(|e| self.ty_is_concrete(e)),
        }
    }

    /// Builds a substitution from generic params and type args.
    fn build_substitution(&self, generic_params: &[GenericParam], args: &[Ty]) -> TypeSubstitution {
        let mut subst = TypeSubstitution::new();
        for (param, arg) in generic_params.iter().zip(args.iter()) {
            subst.insert(param.name.name.clone(), arg.clone());
        }
        subst
    }

    // ------------------------------------------------------------------
    // Phase 3: Walk expressions and monomorphize
    // ------------------------------------------------------------------

    fn monomorphize_item(&mut self, item: &mut Item) {
        match &mut item.kind {
            ItemKind::Fn(f) => {
                // Also resolve types in the function signature.
                for param in &mut f.params {
                    if let Some(ty) = &mut param.ty {
                        self.resolve_named_app_type(ty);
                    }
                }
                if let Some(ret) = &mut f.return_ty {
                    self.resolve_named_app_type(ret);
                }
                self.monomorphize_block(&mut f.body);
            }
            ItemKind::Let(binding) => {
                if let Some(ty) = &mut binding.ty {
                    self.resolve_named_app_type(ty);
                }
                self.monomorphize_expr(&mut binding.init);
            }
            ItemKind::Const(binding) => {
                if let Some(ty) = &mut binding.ty {
                    self.resolve_named_app_type(ty);
                }
                self.monomorphize_expr(&mut binding.init);
            }
            ItemKind::Pub(pub_item) => self.monomorphize_item(pub_item.item.as_mut()),
            ItemKind::Struct(s) => {
                for field in &mut s.fields {
                    self.resolve_named_app_type(&mut field.ty);
                }
            }
            ItemKind::Enum(e) => {
                for variant in &mut e.variants {
                    if let Some(ty) = &mut variant.payload {
                        self.resolve_named_app_type(ty);
                    }
                }
            }
            ItemKind::Module(_) | ItemKind::Use(_) => {}
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
            StmtKind::Let(binding) => {
                if let Some(ty) = &mut binding.ty {
                    self.resolve_named_app_type(ty);
                }
                self.monomorphize_expr(&mut binding.init);
            }
            StmtKind::Const(binding) => {
                if let Some(ty) = &mut binding.ty {
                    self.resolve_named_app_type(ty);
                }
                self.monomorphize_expr(&mut binding.init);
            }
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
            ExprKind::StructLit { name, fields } => {
                for field in fields.iter_mut() {
                    self.monomorphize_expr(&mut field.value);
                }
                // Try to resolve the struct name if it's a generic struct.
                self.resolve_struct_lit_name(name, fields);
            }
            ExprKind::ArrayLit(elems) => {
                for e in elems.iter_mut() {
                    self.monomorphize_expr(e);
                }
            }
            ExprKind::EnumVariant {
                name,
                variant,
                payload,
            } => {
                if let Some(p) = payload {
                    self.monomorphize_expr(p);
                }
                // Try to resolve the enum name if it's a generic enum.
                self.resolve_enum_variant_name(name, variant, payload);
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
            ExprKind::Closure { params, body, .. } => {
                // Desugar closure into a named function.
                self.closure_counter += 1;
                let name = format!("__closure_{}", self.closure_counter);
                let name_span = self.next_span();

                // Collect free variables in the body.
                let mut param_names = std::collections::HashSet::new();
                for p in params.iter() {
                    param_names.insert(p.name.name.clone());
                }
                let mut free_vars = std::collections::HashSet::new();
                collect_free_vars(body, &param_names, &mut free_vars);

                if !free_vars.is_empty() {
                    // Capturing closure: add captured variables as extra parameters.
                    let mut all_params = params.clone();
                    let mut sorted_free: Vec<String> = free_vars.iter().cloned().collect();
                    sorted_free.sort();
                    for fv in &sorted_free {
                        all_params.insert(
                            0,
                            ClosureParam {
                                name: Ident {
                                    name: fv.clone(),
                                    span: self.next_span(),
                                },
                                ty: None,
                                span: self.next_span(),
                            },
                        );
                    }
                    self.closure_captures
                        .insert(name.clone(), sorted_free.clone());
                    self.desugar_closure_to_fn(&name, name_span, &all_params, body);
                } else {
                    self.desugar_closure_to_fn(&name, name_span, params, body);
                }

                // Replace the closure expression with a reference to the generated function.
                *expr = Expr {
                    kind: ExprKind::Ident(Ident {
                        name,
                        span: name_span,
                    }),
                    span: expr.span,
                };
            }
        }
    }

    // ------------------------------------------------------------------
    // Closure desugaring helpers
    // ------------------------------------------------------------------

    /// Desugars a closure body into a named function and registers it.
    fn desugar_closure_to_fn(
        &mut self,
        name: &str,
        name_span: Span,
        params: &[ClosureParam],
        body: &Expr,
    ) {
        let fn_params: Vec<Param> = params
            .iter()
            .map(|p| Param {
                name: Ident {
                    name: p.name.name.clone(),
                    span: self.next_span(),
                },
                ty: p.ty.clone(),
                span: self.next_span(),
            })
            .collect();

        // Create the function body with a return statement wrapping the closure body.
        // Clone the body but do NOT reassign spans: the semantic analyzer and
        // type checker need the original spans to resolve types correctly.
        let body_clone = body.clone();
        let body_block = Block {
            stmts: vec![Stmt {
                kind: StmtKind::Return(Some(body_clone)),
                span: self.next_span(),
            }],
            result: None,
            span: self.next_span(),
        };

        let fn_item = FnItem {
            name: Ident {
                name: name.to_string(),
                span: name_span,
            },
            generic_params: Vec::new(),
            params: fn_params,
            return_ty: None,
            body: body_block,
        };

        self.closure_fns.insert(name.to_string(), fn_item);
    }

    // ------------------------------------------------------------------
    // Closure call-site rewriting
    // ------------------------------------------------------------------

    /// After Phase 3, closures have been replaced with Ident("__closure_N")
    /// in their let-binding init, but call sites still reference the variable
    /// name. This method:
    /// 1. Finds `let x = __closure_N` bindings to build a var→closure map.
    /// 2. Rewrites `x(args)` → `__closure_N(captured..., args)`.
    fn rewrite_closure_calls(&mut self, ast: &mut Ast) {
        // Step 1: Recursively collect let x = __closure_N bindings from all scopes.
        let mut var_to_closure: HashMap<String, (String, Vec<String>)> = HashMap::new();
        for item in &ast.items {
            match &item.kind {
                ItemKind::Fn(f) => {
                    Self::collect_closure_bindings_in_block(
                        &f.body,
                        &self.closure_captures,
                        &mut var_to_closure,
                    );
                }
                ItemKind::Let(let_item) => {
                    Self::collect_closure_binding(
                        &let_item.init,
                        &let_item.name.name,
                        &self.closure_captures,
                        &mut var_to_closure,
                    );
                }
                ItemKind::Pub(pub_item) => {
                    if let ItemKind::Fn(f) = &pub_item.item.kind {
                        Self::collect_closure_bindings_in_block(
                            &f.body,
                            &self.closure_captures,
                            &mut var_to_closure,
                        );
                    }
                }
                _ => {}
            }
        }
        // Step 2: Rewrite call sites in all function bodies.
        // Pass each function's parameter names so Case 1 skips params
        // (they are not closure variables from outer scopes).
        for item in &mut ast.items {
            match &mut item.kind {
                ItemKind::Fn(f) => {
                    let params: std::collections::HashSet<String> =
                        f.params.iter().map(|p| p.name.name.clone()).collect();
                    self.rewrite_closure_calls_in_block(&mut f.body, &var_to_closure, &params);
                }
                ItemKind::Let(let_item) => {
                    let empty = std::collections::HashSet::new();
                    self.rewrite_closure_calls_in_expr(&mut let_item.init, &var_to_closure, &empty);
                }
                ItemKind::Pub(pub_item) => {
                    if let ItemKind::Fn(f) = &mut pub_item.item.kind {
                        let params: std::collections::HashSet<String> =
                            f.params.iter().map(|p| p.name.name.clone()).collect();
                        self.rewrite_closure_calls_in_block(&mut f.body, &var_to_closure, &params);
                    }
                }
                _ => {}
            }
        }
    }

    /// Collects closure bindings recursively from a let-init expression.
    fn collect_closure_binding(
        expr: &Expr,
        var_name: &str,
        captures: &HashMap<String, Vec<String>>,
        out: &mut HashMap<String, (String, Vec<String>)>,
    ) {
        if let ExprKind::Ident(ident) = &expr.kind {
            if let Some(caps) = captures.get(&ident.name) {
                out.insert(var_name.to_string(), (ident.name.clone(), caps.clone()));
            }
        }
    }

    /// Collects closure bindings recursively from a block (all nested scopes).
    fn collect_closure_bindings_in_block(
        block: &Block,
        captures: &HashMap<String, Vec<String>>,
        out: &mut HashMap<String, (String, Vec<String>)>,
    ) {
        for stmt in &block.stmts {
            match &stmt.kind {
                StmtKind::Let(binding) => {
                    Self::collect_closure_binding(&binding.init, &binding.name.name, captures, out);
                }
                StmtKind::If(if_stmt) => {
                    Self::collect_closure_bindings_in_block(&if_stmt.then_block, captures, out);
                }
                StmtKind::While { body, .. } => {
                    Self::collect_closure_bindings_in_block(body, captures, out);
                }
                StmtKind::Loop(body) => {
                    Self::collect_closure_bindings_in_block(body, captures, out);
                }
                StmtKind::For { body, .. } => {
                    Self::collect_closure_bindings_in_block(body, captures, out);
                }
                StmtKind::Match(m) => {
                    for arm in &m.arms {
                        Self::collect_closure_bindings_in_block(&arm.body, captures, out);
                    }
                }
                _ => {}
            }
        }
        if let Some(result) = &block.result {
            Self::collect_closure_binding_in_expr(result, captures, out);
        }
    }

    /// Helper: collect closure bindings from an expression.
    fn collect_closure_binding_in_expr(
        expr: &Expr,
        captures: &HashMap<String, Vec<String>>,
        out: &mut HashMap<String, (String, Vec<String>)>,
    ) {
        match &expr.kind {
            ExprKind::Block(b) => {
                Self::collect_closure_bindings_in_block(b, captures, out);
            }
            ExprKind::IfExpr(inner) => {
                Self::collect_closure_bindings_in_block(&inner.then_block, captures, out);
                if let ElseBranch::Block(b) = &inner.else_branch {
                    Self::collect_closure_bindings_in_block(b, captures, out)
                }
            }
            _ => {}
        }
    }

    /// Recursively rewrites closure calls within a block.
    fn rewrite_closure_calls_in_block(
        &mut self,
        block: &mut Block,
        var_to_closure: &HashMap<String, (String, Vec<String>)>,
        fn_params: &std::collections::HashSet<String>,
    ) {
        for stmt in &mut block.stmts {
            match &mut stmt.kind {
                StmtKind::Let(let_item) => {
                    self.rewrite_closure_calls_in_expr(
                        &mut let_item.init,
                        var_to_closure,
                        fn_params,
                    );
                }
                StmtKind::Expr(e) => {
                    self.rewrite_closure_calls_in_expr(e, var_to_closure, fn_params);
                }
                StmtKind::Return(Some(e)) => {
                    self.rewrite_closure_calls_in_expr(e, var_to_closure, fn_params);
                }
                StmtKind::Break(Some(e)) => {
                    self.rewrite_closure_calls_in_expr(e, var_to_closure, fn_params);
                }
                StmtKind::If(if_stmt) => {
                    self.rewrite_closure_calls_in_expr(
                        &mut if_stmt.cond,
                        var_to_closure,
                        fn_params,
                    );
                    self.rewrite_closure_calls_in_block(
                        &mut if_stmt.then_block,
                        var_to_closure,
                        fn_params,
                    );
                }
                StmtKind::While { cond, body } => {
                    self.rewrite_closure_calls_in_expr(cond, var_to_closure, fn_params);
                    self.rewrite_closure_calls_in_block(body, var_to_closure, fn_params);
                }
                StmtKind::Loop(body) => {
                    self.rewrite_closure_calls_in_block(body, var_to_closure, fn_params);
                }
                StmtKind::For { iterable, body, .. } => {
                    self.rewrite_closure_calls_in_expr(iterable, var_to_closure, fn_params);
                    self.rewrite_closure_calls_in_block(body, var_to_closure, fn_params);
                }
                StmtKind::Match(m) => {
                    self.rewrite_closure_calls_in_expr(&mut m.scrutinee, var_to_closure, fn_params);
                    for arm in &mut m.arms {
                        if let Some(guard) = &mut arm.guard {
                            self.rewrite_closure_calls_in_expr(guard, var_to_closure, fn_params);
                        }
                        self.rewrite_closure_calls_in_block(
                            &mut arm.body,
                            var_to_closure,
                            fn_params,
                        );
                    }
                }
                StmtKind::Const(const_item) => {
                    self.rewrite_closure_calls_in_expr(
                        &mut const_item.init,
                        var_to_closure,
                        fn_params,
                    );
                }
                StmtKind::Return(None) | StmtKind::Break(None) | StmtKind::Continue => {}
            }
        }
        if let Some(result) = &mut block.result {
            self.rewrite_closure_calls_in_expr(result, var_to_closure, fn_params);
        }
    }

    /// Recursively rewrites closure calls within an expression.
    fn rewrite_closure_calls_in_expr(
        &mut self,
        expr: &mut Expr,
        var_to_closure: &HashMap<String, (String, Vec<String>)>,
        fn_params: &std::collections::HashSet<String>,
    ) {
        match &mut expr.kind {
            ExprKind::Call { callee, args, .. } => {
                // First, recurse into callee and args.
                self.rewrite_closure_calls_in_expr(callee, var_to_closure, fn_params);
                for arg in args.iter_mut() {
                    self.rewrite_closure_calls_in_expr(arg, var_to_closure, fn_params);
                }
                // Then check if callee is a closure variable.
                // Skip if callee is a function parameter (not a local closure var).
                if let ExprKind::Ident(ident) = &mut callee.kind {
                    if !fn_params.contains(&ident.name) {
                        if let Some((closure_name, caps)) = var_to_closure.get(&ident.name).cloned()
                        {
                            // Inject captured variables as leading arguments.
                            let cap_args: Vec<Expr> = caps
                                .iter()
                                .map(|cap| {
                                    let span = self.next_span();
                                    Expr {
                                        kind: ExprKind::Ident(Ident {
                                            name: cap.clone(),
                                            span,
                                        }),
                                        span,
                                    }
                                })
                                .collect();
                            let mut new_args = cap_args;
                            new_args.append(args);
                            *args = new_args;
                            // Update callee to the actual closure function name.
                            ident.name = closure_name;
                        }
                    } // end !fn_params check
                }
            }
            ExprKind::Binary { lhs, rhs, .. } => {
                self.rewrite_closure_calls_in_expr(lhs, var_to_closure, fn_params);
                self.rewrite_closure_calls_in_expr(rhs, var_to_closure, fn_params);
            }
            ExprKind::Unary { operand, .. } => {
                self.rewrite_closure_calls_in_expr(operand, var_to_closure, fn_params);
            }
            ExprKind::Assign { target, value, .. } => {
                self.rewrite_closure_calls_in_expr(target, var_to_closure, fn_params);
                self.rewrite_closure_calls_in_expr(value, var_to_closure, fn_params);
            }
            ExprKind::Block(b) => {
                self.rewrite_closure_calls_in_block(b, var_to_closure, fn_params);
            }
            ExprKind::IfExpr(inner) => {
                self.rewrite_closure_calls_in_expr(&mut inner.cond, var_to_closure, fn_params);
                self.rewrite_closure_calls_in_block(
                    &mut inner.then_block,
                    var_to_closure,
                    fn_params,
                );
                match &mut inner.else_branch {
                    ElseBranch::Block(b) => {
                        self.rewrite_closure_calls_in_block(b, var_to_closure, fn_params)
                    }
                    ElseBranch::IfExpr(e) => {
                        let mut wrapper = Expr {
                            kind: ExprKind::IfExpr(e.clone()),
                            span: e.span,
                        };
                        self.rewrite_closure_calls_in_expr(&mut wrapper, var_to_closure, fn_params);
                        if let ExprKind::IfExpr(new_inner) = wrapper.kind {
                            *e = new_inner;
                        }
                    }
                    _ => {}
                }
            }
            ExprKind::Tuple(elems) | ExprKind::ArrayLit(elems) => {
                for e in elems.iter_mut() {
                    self.rewrite_closure_calls_in_expr(e, var_to_closure, fn_params);
                }
            }
            ExprKind::Member { base, .. } => {
                self.rewrite_closure_calls_in_expr(base, var_to_closure, fn_params);
            }
            ExprKind::Index { base, index } => {
                self.rewrite_closure_calls_in_expr(base, var_to_closure, fn_params);
                self.rewrite_closure_calls_in_expr(index, var_to_closure, fn_params);
            }
            ExprKind::StructLit { fields, .. } => {
                for f in fields.iter_mut() {
                    self.rewrite_closure_calls_in_expr(&mut f.value, var_to_closure, fn_params);
                }
            }
            ExprKind::EnumVariant { payload, .. } => {
                if let Some(p) = payload {
                    self.rewrite_closure_calls_in_expr(p, var_to_closure, fn_params);
                }
            }
            ExprKind::Range { start, end, .. } => {
                self.rewrite_closure_calls_in_expr(start, var_to_closure, fn_params);
                self.rewrite_closure_calls_in_expr(end, var_to_closure, fn_params);
            }
            ExprKind::WhileExpr { cond, body, .. } => {
                self.rewrite_closure_calls_in_expr(cond, var_to_closure, fn_params);
                self.rewrite_closure_calls_in_block(body, var_to_closure, fn_params);
            }
            ExprKind::LoopExpr { body, .. } => {
                self.rewrite_closure_calls_in_block(body, var_to_closure, fn_params);
            }
            ExprKind::MatchExpr(m) => {
                self.rewrite_closure_calls_in_expr(&mut m.scrutinee, var_to_closure, fn_params);
                for arm in &mut m.arms {
                    if let Some(guard) = &mut arm.guard {
                        self.rewrite_closure_calls_in_expr(guard, var_to_closure, fn_params);
                    }
                    self.rewrite_closure_calls_in_expr(&mut arm.body, var_to_closure, fn_params);
                }
            }
            ExprKind::Group(inner) => {
                self.rewrite_closure_calls_in_expr(inner, var_to_closure, fn_params);
            }
            ExprKind::TupleFieldAccess { base, .. } => {
                self.rewrite_closure_calls_in_expr(base, var_to_closure, fn_params);
            }
            ExprKind::Borrow { operand, .. } | ExprKind::Deref { operand } => {
                self.rewrite_closure_calls_in_expr(operand, var_to_closure, fn_params);
            }
            ExprKind::Int
            | ExprKind::Float
            | ExprKind::Str
            | ExprKind::Char
            | ExprKind::Bool(_)
            | ExprKind::Null
            | ExprKind::Ident(_) => {}
            ExprKind::Closure { .. } => {}
        }
    }

    // ------------------------------------------------------------------
    // Struct literal name resolution
    // ------------------------------------------------------------------

    /// Resolves a struct literal name from a generic struct name to a
    /// concrete name. Infers type arguments from the field values.
    fn resolve_struct_lit_name(&mut self, name: &mut Ident, fields: &[StructFieldInit]) {
        if !self.generic_structs.contains_key(&name.name) {
            return;
        }

        let generic_params = self.generic_structs[&name.name].0.clone();
        let struct_item = self.generic_structs[&name.name].1.clone();

        // Try to infer type args from the struct literal fields.
        let mut subst = TypeSubstitution::new();
        for field_init in fields {
            // Find the corresponding field declaration in the generic struct.
            if let Some(declared_field) = struct_item
                .fields
                .iter()
                .find(|f| f.name.name == field_init.name.name)
            {
                self.match_type_against_expr_for_struct(
                    &declared_field.ty,
                    &field_init.value,
                    &mut subst,
                );
            }
        }

        // Check that all generic params were inferred.
        let all_inferred = generic_params
            .iter()
            .all(|gp| subst.contains_key(&gp.name.name));
        if !all_inferred {
            return;
        }

        let concrete_name = self.mangle_name(&name.name, &subst);
        if !self.concrete_structs.contains_key(&concrete_name) {
            let concrete = self.create_concrete_struct(&struct_item, &subst, &concrete_name);
            self.concrete_structs
                .insert(concrete_name.clone(), concrete);
        }
        name.name = concrete_name;
    }

    /// Matches a struct field type against an expression to infer generic
    /// parameters. Similar to `match_type_against_expr` but also handles
    /// Named types that might be generic parameters.
    fn match_type_against_expr_for_struct(
        &self,
        ty: &Ty,
        expr: &Expr,
        subst: &mut TypeSubstitution,
    ) {
        match &ty.kind {
            TyKind::GenericParam(ident) => {
                if let Some(concrete) = self.infer_type_from_expr(expr) {
                    subst.insert(ident.name.clone(), concrete);
                }
            }
            TyKind::Named(ident) => {
                // A single uppercase letter is a generic parameter.
                if ident.name.len() == 1 && ident.name.chars().next().unwrap().is_uppercase() {
                    if let Some(concrete) = self.infer_type_from_expr(expr) {
                        subst.insert(ident.name.clone(), concrete);
                    }
                }
            }
            TyKind::Tuple(elems) => {
                if let ExprKind::Tuple(tuple_elems) = &expr.kind {
                    for (elem_ty, tuple_elem) in elems.iter().zip(tuple_elems.iter()) {
                        self.match_type_against_expr_for_struct(elem_ty, tuple_elem, subst);
                    }
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Enum variant name resolution
    // ------------------------------------------------------------------

    /// Resolves an enum variant name from a generic enum name to a
    /// concrete name. Infers type arguments from the payload.
    fn resolve_enum_variant_name(
        &mut self,
        name: &mut Ident,
        _variant: &mut Ident,
        payload: &Option<Box<Expr>>,
    ) {
        if !self.generic_enums.contains_key(&name.name) {
            return;
        }

        let generic_params = self.generic_enums[&name.name].0.clone();
        let enum_item = self.generic_enums[&name.name].1.clone();

        // Try to infer type args from the payload.
        let mut subst = TypeSubstitution::new();
        if let Some(payload_expr) = payload {
            // Find the payload type of the matched variant.
            // We need to look at ALL variants to find one with a generic payload.
            for variant in &enum_item.variants {
                if let Some(payload_ty) = &variant.payload {
                    self.match_type_against_expr_for_struct(payload_ty, payload_expr, &mut subst);
                }
            }
        }

        // Check that all generic params were inferred.
        let all_inferred = generic_params
            .iter()
            .all(|gp| subst.contains_key(&gp.name.name));
        if !all_inferred {
            return;
        }

        let concrete_name = self.mangle_name(&name.name, &subst);
        if !self.concrete_enums.contains_key(&concrete_name) {
            let concrete = self.create_concrete_enum(&enum_item, &subst, &concrete_name);
            self.concrete_enums.insert(concrete_name.clone(), concrete);
        }
        name.name = concrete_name;
    }

    // ------------------------------------------------------------------
    // Inference: infer type arguments from expressions
    // ------------------------------------------------------------------

    /// Infers type arguments for a generic function call.
    fn infer_type_args(
        &self,
        generic_params: &[GenericParam],
        fn_params: &[Param],
        args: &[Expr],
        explicit_type_args: Option<&[Ty]>,
    ) -> Option<TypeSubstitution> {
        let mut subst = TypeSubstitution::new();

        // If explicit type arguments are provided, use them directly.
        if let Some(explicit) = explicit_type_args {
            for (gp, ty) in generic_params.iter().zip(explicit.iter()) {
                subst.insert(gp.name.name.clone(), ty.clone());
            }
            return Some(subst);
        }

        // Otherwise, infer from argument expressions.
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

    // ------------------------------------------------------------------
    // Name mangling
    // ------------------------------------------------------------------

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
    // Concrete type creation
    // ------------------------------------------------------------------

    /// Creates a concrete struct by substituting generic parameters.
    fn create_concrete_struct(
        &mut self,
        struct_item: &StructItem,
        subst: &TypeSubstitution,
        concrete_name: &str,
    ) -> StructItem {
        let mut new_struct = struct_item.clone();
        new_struct.generic_params = Vec::new();
        new_struct.name = Ident {
            name: concrete_name.to_string(),
            span: self.next_span(),
        };
        // Substitute types in fields.
        for field in &mut new_struct.fields {
            self.substitute_type(&mut field.ty, subst);
            field.name.span = self.next_span();
            field.span = self.next_span();
        }
        new_struct.span = self.next_span();
        new_struct
    }

    /// Creates a concrete enum by substituting generic parameters.
    fn create_concrete_enum(
        &mut self,
        enum_item: &EnumItem,
        subst: &TypeSubstitution,
        concrete_name: &str,
    ) -> EnumItem {
        let mut new_enum = enum_item.clone();
        new_enum.generic_params = Vec::new();
        new_enum.name = Ident {
            name: concrete_name.to_string(),
            span: self.next_span(),
        };
        // Substitute types in variant payloads.
        for variant in &mut new_enum.variants {
            if let Some(ty) = &mut variant.payload {
                self.substitute_type(ty, subst);
            }
            variant.name.span = self.next_span();
            variant.span = self.next_span();
        }
        new_enum.span = self.next_span();
        new_enum
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
                self.resolve_named_app_type(ty);
                self.reassign_type_spans(ty);
            }
        }

        // Reassign return type spans.
        if let Some(ret_ty) = &mut new_fn.return_ty {
            self.substitute_type(ret_ty, subst);
            self.resolve_named_app_type(ret_ty);
            self.reassign_type_spans(ret_ty);
        }

        // Substitute types in the body, then resolve NamedApp, then
        // resolve struct literal / enum variant names using the same
        // substitution, then reassign spans.
        self.substitute_block(&mut new_fn.body, subst);
        self.resolve_all_named_app_in_block(&mut new_fn.body);
        self.resolve_names_in_block(&mut new_fn.body, subst);
        self.reassign_block_spans(&mut new_fn.body);

        new_fn
    }

    /// Resolves all NamedApp types inside a block.
    fn resolve_all_named_app_in_block(&mut self, block: &mut Block) {
        for stmt in &mut block.stmts {
            self.resolve_all_named_app_in_stmt(stmt);
        }
        if let Some(result) = &mut block.result {
            self.resolve_all_named_app_in_expr(result);
        }
    }

    // ------------------------------------------------------------------
    // Name resolution inside monomorphized function bodies
    // ------------------------------------------------------------------

    /// Resolves struct literal and enum variant names inside a block
    /// using the given type substitution. This handles the case where
    /// a generic function body references a generic struct/enum by its
    /// original name — after type substitution, we know the concrete
    /// type arguments, so we can resolve the name.
    fn resolve_names_in_block(&mut self, block: &mut Block, subst: &TypeSubstitution) {
        for stmt in &mut block.stmts {
            self.resolve_names_in_stmt(stmt, subst);
        }
        if let Some(result) = &mut block.result {
            self.resolve_names_in_expr(result, subst);
        }
    }

    fn resolve_names_in_stmt(&mut self, stmt: &mut Stmt, subst: &TypeSubstitution) {
        match &mut stmt.kind {
            StmtKind::Let(binding) => {
                self.resolve_names_in_expr(&mut binding.init, subst);
            }
            StmtKind::Const(binding) => {
                self.resolve_names_in_expr(&mut binding.init, subst);
            }
            StmtKind::Return(Some(e)) | StmtKind::Break(Some(e)) => {
                self.resolve_names_in_expr(e, subst);
            }
            StmtKind::Return(None) | StmtKind::Break(None) | StmtKind::Continue => {}
            StmtKind::If(if_stmt) => {
                self.resolve_names_in_expr(&mut if_stmt.cond, subst);
                self.resolve_names_in_block(&mut if_stmt.then_block, subst);
                if let Some(branch) = &mut if_stmt.else_branch {
                    self.resolve_names_in_else_branch(branch, subst);
                }
            }
            StmtKind::While { cond, body } => {
                self.resolve_names_in_expr(cond, subst);
                self.resolve_names_in_block(body, subst);
            }
            StmtKind::For { iterable, body, .. } => {
                self.resolve_names_in_expr(iterable, subst);
                self.resolve_names_in_block(body, subst);
            }
            StmtKind::Loop(body) => self.resolve_names_in_block(body, subst),
            StmtKind::Match(m) => {
                self.resolve_names_in_expr(&mut m.scrutinee, subst);
                for arm in &mut m.arms {
                    if let Some(guard) = &mut arm.guard {
                        self.resolve_names_in_expr(guard, subst);
                    }
                    self.resolve_names_in_block(&mut arm.body, subst);
                }
            }
            StmtKind::Expr(e) => self.resolve_names_in_expr(e, subst),
        }
    }

    fn resolve_names_in_else_branch(&mut self, branch: &mut ElseBranch, subst: &TypeSubstitution) {
        match branch {
            ElseBranch::Block(b) => self.resolve_names_in_block(b, subst),
            ElseBranch::If(inner) => {
                self.resolve_names_in_expr(&mut inner.cond, subst);
                self.resolve_names_in_block(&mut inner.then_block, subst);
                if let Some(next) = &mut inner.else_branch {
                    self.resolve_names_in_else_branch(next, subst);
                }
            }
            ElseBranch::IfExpr(inner) => {
                self.resolve_names_in_expr(&mut inner.cond, subst);
                self.resolve_names_in_block(&mut inner.then_block, subst);
                self.resolve_names_in_else_branch(&mut inner.else_branch, subst);
            }
        }
    }

    fn resolve_names_in_expr(&mut self, expr: &mut Expr, subst: &TypeSubstitution) {
        match &mut expr.kind {
            ExprKind::Call { callee, args, .. } => {
                self.resolve_names_in_expr(callee, subst);
                for arg in args.iter_mut() {
                    self.resolve_names_in_expr(arg, subst);
                }
            }
            ExprKind::Unary { operand, .. } => self.resolve_names_in_expr(operand, subst),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.resolve_names_in_expr(lhs, subst);
                self.resolve_names_in_expr(rhs, subst);
            }
            ExprKind::Assign { target, value, .. } => {
                self.resolve_names_in_expr(target, subst);
                self.resolve_names_in_expr(value, subst);
            }
            ExprKind::Range { start, end, .. } => {
                self.resolve_names_in_expr(start, subst);
                self.resolve_names_in_expr(end, subst);
            }
            ExprKind::Member { base, .. } => self.resolve_names_in_expr(base, subst),
            ExprKind::Index { base, index } => {
                self.resolve_names_in_expr(base, subst);
                self.resolve_names_in_expr(index, subst);
            }
            ExprKind::StructLit { name, fields } => {
                for field in fields.iter_mut() {
                    self.resolve_names_in_expr(&mut field.value, subst);
                }
                // Resolve struct name using the substitution.
                if self.generic_structs.contains_key(&name.name) {
                    // Build concrete substitution: only include params that
                    // the struct actually has.
                    if let Some((gp, _)) = self.generic_structs.get(&name.name) {
                        let struct_subst: TypeSubstitution = subst
                            .iter()
                            .filter(|(k, _)| gp.iter().any(|p| p.name.name == **k))
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        if struct_subst.len() == gp.len() {
                            let concrete_name = self.mangle_name(&name.name, &struct_subst);
                            if !self.concrete_structs.contains_key(&concrete_name) {
                                let template = self.generic_structs[&name.name].1.clone();
                                let concrete = self.create_concrete_struct(
                                    &template,
                                    &struct_subst,
                                    &concrete_name,
                                );
                                self.concrete_structs
                                    .insert(concrete_name.clone(), concrete);
                            }
                            name.name = concrete_name;
                        }
                    }
                }
            }
            ExprKind::ArrayLit(elems) => {
                for e in elems.iter_mut() {
                    self.resolve_names_in_expr(e, subst);
                }
            }
            ExprKind::EnumVariant {
                name,
                variant: _,
                payload,
            } => {
                if let Some(p) = payload {
                    self.resolve_names_in_expr(p, subst);
                }
                // Resolve enum name using the substitution.
                if self.generic_enums.contains_key(&name.name) {
                    if let Some((gp, _)) = self.generic_enums.get(&name.name) {
                        let enum_subst: TypeSubstitution = subst
                            .iter()
                            .filter(|(k, _)| gp.iter().any(|p| p.name.name == **k))
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        if enum_subst.len() == gp.len() {
                            let concrete_name = self.mangle_name(&name.name, &enum_subst);
                            if !self.concrete_enums.contains_key(&concrete_name) {
                                let template = self.generic_enums[&name.name].1.clone();
                                let concrete = self.create_concrete_enum(
                                    &template,
                                    &enum_subst,
                                    &concrete_name,
                                );
                                self.concrete_enums.insert(concrete_name.clone(), concrete);
                            }
                            name.name = concrete_name;
                        }
                    }
                }
            }
            ExprKind::IfExpr(inner) => {
                self.resolve_names_in_expr(&mut inner.cond, subst);
                self.resolve_names_in_block(&mut inner.then_block, subst);
                self.resolve_names_in_else_branch(&mut inner.else_branch, subst);
            }
            ExprKind::Block(b) => self.resolve_names_in_block(b, subst),
            ExprKind::Tuple(elems) => {
                for e in elems.iter_mut() {
                    self.resolve_names_in_expr(e, subst);
                }
            }
            ExprKind::TupleFieldAccess { base, .. } => self.resolve_names_in_expr(base, subst),
            ExprKind::WhileExpr { cond, body, .. } => {
                self.resolve_names_in_expr(cond, subst);
                self.resolve_names_in_block(body, subst);
            }
            ExprKind::LoopExpr { body, .. } => self.resolve_names_in_block(body, subst),
            ExprKind::MatchExpr(m) => {
                self.resolve_names_in_expr(&mut m.scrutinee, subst);
                for arm in m.arms.iter_mut() {
                    if let Some(guard) = &mut arm.guard {
                        self.resolve_names_in_expr(guard, subst);
                    }
                    self.resolve_names_in_expr(&mut arm.body, subst);
                }
            }
            ExprKind::Borrow { operand, .. } | ExprKind::Deref { operand } => {
                self.resolve_names_in_expr(operand, subst);
            }
            ExprKind::Group(inner) => self.resolve_names_in_expr(inner, subst),
            ExprKind::Closure { body, .. } => self.resolve_names_in_expr(body, subst),
            ExprKind::Int
            | ExprKind::Float
            | ExprKind::Str
            | ExprKind::Char
            | ExprKind::Bool(_)
            | ExprKind::Null
            | ExprKind::Ident(_) => {}
        }
    }

    fn resolve_all_named_app_in_stmt(&mut self, stmt: &mut Stmt) {
        match &mut stmt.kind {
            StmtKind::Let(binding) => {
                if let Some(ty) = &mut binding.ty {
                    self.resolve_named_app_type(ty);
                }
                self.resolve_all_named_app_in_expr(&mut binding.init);
            }
            StmtKind::Const(binding) => {
                if let Some(ty) = &mut binding.ty {
                    self.resolve_named_app_type(ty);
                }
                self.resolve_all_named_app_in_expr(&mut binding.init);
            }
            StmtKind::Return(Some(e)) | StmtKind::Break(Some(e)) => {
                self.resolve_all_named_app_in_expr(e);
            }
            StmtKind::Return(None) | StmtKind::Break(None) | StmtKind::Continue => {}
            StmtKind::If(if_stmt) => {
                self.resolve_all_named_app_in_expr(&mut if_stmt.cond);
                self.resolve_all_named_app_in_block(&mut if_stmt.then_block);
                if let Some(branch) = &mut if_stmt.else_branch {
                    self.resolve_all_named_app_in_else_branch(branch);
                }
            }
            StmtKind::While { cond, body } => {
                self.resolve_all_named_app_in_expr(cond);
                self.resolve_all_named_app_in_block(body);
            }
            StmtKind::For { iterable, body, .. } => {
                self.resolve_all_named_app_in_expr(iterable);
                self.resolve_all_named_app_in_block(body);
            }
            StmtKind::Loop(body) => self.resolve_all_named_app_in_block(body),
            StmtKind::Match(m) => {
                self.resolve_all_named_app_in_expr(&mut m.scrutinee);
                for arm in &mut m.arms {
                    if let Some(guard) = &mut arm.guard {
                        self.resolve_all_named_app_in_expr(guard);
                    }
                    self.resolve_all_named_app_in_block(&mut arm.body);
                }
            }
            StmtKind::Expr(e) => self.resolve_all_named_app_in_expr(e),
        }
    }

    fn resolve_all_named_app_in_else_branch(&mut self, branch: &mut ElseBranch) {
        match branch {
            ElseBranch::Block(b) => self.resolve_all_named_app_in_block(b),
            ElseBranch::If(inner) => {
                self.resolve_all_named_app_in_expr(&mut inner.cond);
                self.resolve_all_named_app_in_block(&mut inner.then_block);
                if let Some(next) = &mut inner.else_branch {
                    self.resolve_all_named_app_in_else_branch(next);
                }
            }
            ElseBranch::IfExpr(inner) => {
                self.resolve_all_named_app_in_expr(&mut inner.cond);
                self.resolve_all_named_app_in_block(&mut inner.then_block);
                self.resolve_all_named_app_in_else_branch(&mut inner.else_branch);
            }
        }
    }

    fn resolve_all_named_app_in_expr(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Call { callee, args, .. } => {
                self.resolve_all_named_app_in_expr(callee);
                for arg in args.iter_mut() {
                    self.resolve_all_named_app_in_expr(arg);
                }
            }
            ExprKind::Unary { operand, .. } => self.resolve_all_named_app_in_expr(operand),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.resolve_all_named_app_in_expr(lhs);
                self.resolve_all_named_app_in_expr(rhs);
            }
            ExprKind::Assign { target, value, .. } => {
                self.resolve_all_named_app_in_expr(target);
                self.resolve_all_named_app_in_expr(value);
            }
            ExprKind::Range { start, end, .. } => {
                self.resolve_all_named_app_in_expr(start);
                self.resolve_all_named_app_in_expr(end);
            }
            ExprKind::Member { base, .. } => self.resolve_all_named_app_in_expr(base),
            ExprKind::Index { base, index } => {
                self.resolve_all_named_app_in_expr(base);
                self.resolve_all_named_app_in_expr(index);
            }
            ExprKind::StructLit { name, fields } => {
                for field in fields.iter_mut() {
                    self.resolve_all_named_app_in_expr(&mut field.value);
                }
                self.resolve_struct_lit_name(name, fields);
            }
            ExprKind::ArrayLit(elems) => {
                for e in elems.iter_mut() {
                    self.resolve_all_named_app_in_expr(e);
                }
            }
            ExprKind::EnumVariant {
                name,
                variant,
                payload,
            } => {
                if let Some(p) = payload {
                    self.resolve_all_named_app_in_expr(p);
                }
                self.resolve_enum_variant_name(name, variant, payload);
            }
            ExprKind::IfExpr(inner) => {
                self.resolve_all_named_app_in_expr(&mut inner.cond);
                self.resolve_all_named_app_in_block(&mut inner.then_block);
                self.resolve_all_named_app_in_else_branch(&mut inner.else_branch);
            }
            ExprKind::Block(b) => self.resolve_all_named_app_in_block(b),
            ExprKind::Tuple(elems) => {
                for e in elems.iter_mut() {
                    self.resolve_all_named_app_in_expr(e);
                }
            }
            ExprKind::TupleFieldAccess { base, .. } => self.resolve_all_named_app_in_expr(base),
            ExprKind::WhileExpr { cond, body, .. } => {
                self.resolve_all_named_app_in_expr(cond);
                self.resolve_all_named_app_in_block(body);
            }
            ExprKind::LoopExpr { body, .. } => self.resolve_all_named_app_in_block(body),
            ExprKind::MatchExpr(m) => {
                self.resolve_all_named_app_in_expr(&mut m.scrutinee);
                for arm in m.arms.iter_mut() {
                    if let Some(guard) = &mut arm.guard {
                        self.resolve_all_named_app_in_expr(guard);
                    }
                    self.resolve_all_named_app_in_expr(&mut arm.body);
                }
            }
            ExprKind::Borrow { operand, .. } | ExprKind::Deref { operand } => {
                self.resolve_all_named_app_in_expr(operand);
            }
            ExprKind::Group(inner) => self.resolve_all_named_app_in_expr(inner),
            ExprKind::Closure { body, .. } => self.resolve_all_named_app_in_expr(body),
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
            ExprKind::Closure { body, .. } => self.substitute_expr(body, subst),
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
            ExprKind::Closure { body, params, .. } => {
                for p in params.iter_mut() {
                    p.span = self.next_span();
                    p.name.span = self.next_span();
                }
                self.reassign_expr_spans(body);
            }
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

// ------------------------------------------------------------------
// Closure desugaring
// ------------------------------------------------------------------

/// Collects free variable names in an expression.
/// A free variable is a name that is referenced but not in the `bound` set
/// (parameter names, let-bound names, etc.).
fn collect_free_vars(
    expr: &Expr,
    bound: &std::collections::HashSet<String>,
    out: &mut std::collections::HashSet<String>,
) {
    match &expr.kind {
        ExprKind::Ident(ident) => {
            if !bound.contains(&ident.name) {
                out.insert(ident.name.clone());
            }
        }
        ExprKind::Unary { operand, .. } => collect_free_vars(operand, bound, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_free_vars(lhs, bound, out);
            collect_free_vars(rhs, bound, out);
        }
        ExprKind::Call { callee, args, .. } => {
            collect_free_vars(callee, bound, out);
            for arg in args {
                collect_free_vars(arg, bound, out);
            }
        }
        ExprKind::Member { base, .. } => collect_free_vars(base, bound, out),
        ExprKind::Index { base, index } => {
            collect_free_vars(base, bound, out);
            collect_free_vars(index, bound, out);
        }
        ExprKind::Assign { target, value, .. } => {
            collect_free_vars(target, bound, out);
            collect_free_vars(value, bound, out);
        }
        ExprKind::Group(inner) => collect_free_vars(inner, bound, out),
        ExprKind::Block(block) => {
            let mut inner_bound = bound.clone();
            for stmt in &block.stmts {
                match &stmt.kind {
                    StmtKind::Let(binding) => {
                        collect_free_vars(&binding.init, &inner_bound, out);
                        inner_bound.insert(binding.name.name.clone());
                    }
                    StmtKind::Expr(e) | StmtKind::Return(Some(e)) => {
                        collect_free_vars(e, &inner_bound, out);
                    }
                    StmtKind::Return(None) | StmtKind::Break(None) | StmtKind::Continue => {}
                    StmtKind::Break(Some(e)) => collect_free_vars(e, &inner_bound, out),
                    StmtKind::If(if_stmt) => {
                        collect_free_vars(&if_stmt.cond, &inner_bound, out);
                        collect_free_vars_block(&if_stmt.then_block, &inner_bound, out);
                    }
                    StmtKind::While { cond, body } => {
                        collect_free_vars(cond, &inner_bound, out);
                        collect_free_vars_block(body, &inner_bound, out);
                    }
                    StmtKind::Loop(body) => {
                        collect_free_vars_block(body, &inner_bound, out);
                    }
                    StmtKind::For {
                        name,
                        iterable,
                        body,
                    } => {
                        collect_free_vars(iterable, &inner_bound, out);
                        let mut loop_bound = inner_bound.clone();
                        loop_bound.insert(name.name.clone());
                        collect_free_vars_block(body, &loop_bound, out);
                    }
                    StmtKind::Match(m) => {
                        collect_free_vars(&m.scrutinee, &inner_bound, out);
                        for arm in &m.arms {
                            collect_free_vars_block(&arm.body, &inner_bound, out);
                        }
                    }
                    StmtKind::Const(binding) => {
                        collect_free_vars(&binding.init, &inner_bound, out);
                    }
                }
            }
            if let Some(result) = &block.result {
                collect_free_vars(result, &inner_bound, out);
            }
        }
        ExprKind::Tuple(elems) | ExprKind::ArrayLit(elems) => {
            for e in elems {
                collect_free_vars(e, bound, out);
            }
        }
        ExprKind::IfExpr(inner) => {
            collect_free_vars(&inner.cond, bound, out);
            collect_free_vars_block(&inner.then_block, bound, out);
            match &inner.else_branch {
                ElseBranch::Block(b) => collect_free_vars_block(b, bound, out),
                ElseBranch::IfExpr(e) => collect_free_vars(
                    &Expr {
                        kind: ExprKind::IfExpr(e.clone()),
                        span: e.span,
                    },
                    bound,
                    out,
                ),
                _ => {}
            }
        }
        ExprKind::Range { start, end, .. } => {
            collect_free_vars(start, bound, out);
            collect_free_vars(end, bound, out);
        }
        ExprKind::WhileExpr { cond, body, .. } => {
            collect_free_vars(cond, bound, out);
            collect_free_vars_block(body, bound, out);
        }
        ExprKind::LoopExpr { body, .. } => {
            collect_free_vars_block(body, bound, out);
        }
        ExprKind::MatchExpr(m) => {
            collect_free_vars(&m.scrutinee, bound, out);
            for arm in &m.arms {
                collect_free_vars(&arm.body, bound, out);
            }
        }
        ExprKind::Closure { params, body, .. } => {
            let mut closure_bound = bound.clone();
            for p in params {
                closure_bound.insert(p.name.name.clone());
            }
            collect_free_vars(body, &closure_bound, out);
        }
        ExprKind::StructLit { fields, .. } => {
            for f in fields {
                collect_free_vars(&f.value, bound, out);
            }
        }
        ExprKind::EnumVariant { payload, .. } => {
            if let Some(p) = payload {
                collect_free_vars(p, bound, out);
            }
        }
        ExprKind::Borrow { operand, .. } | ExprKind::Deref { operand } => {
            collect_free_vars(operand, bound, out);
        }
        ExprKind::TupleFieldAccess { base, .. } => {
            collect_free_vars(base, bound, out);
        }
        ExprKind::Int
        | ExprKind::Float
        | ExprKind::Str
        | ExprKind::Char
        | ExprKind::Bool(_)
        | ExprKind::Null => {}
    }
}

/// Helper: collects free vars across a block.
fn collect_free_vars_block(
    block: &Block,
    bound: &std::collections::HashSet<String>,
    out: &mut std::collections::HashSet<String>,
) {
    let mut inner_bound = bound.clone();
    for stmt in &block.stmts {
        match &stmt.kind {
            StmtKind::Let(binding) => {
                collect_free_vars(&binding.init, &inner_bound, out);
                inner_bound.insert(binding.name.name.clone());
            }
            StmtKind::Expr(e) | StmtKind::Return(Some(e)) | StmtKind::Break(Some(e)) => {
                collect_free_vars(e, &inner_bound, out);
            }
            StmtKind::Return(None) | StmtKind::Break(None) | StmtKind::Continue => {}
            _ => {}
        }
    }
    if let Some(result) = &block.result {
        collect_free_vars(result, &inner_bound, out);
    }
}
