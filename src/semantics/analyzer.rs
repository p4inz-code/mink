//! Semantic analysis: the AST → semantic-result traversal.
//!
//! The analyzer walks the parsed [`Ast`](crate::ast::Ast) once, constructing
//! lexical scopes and symbols, resolving name references, and validating the
//! semantic rules currently supported by MINK:
//!
//! - declaration collection and duplicate-definition detection
//! - name resolution (module scope is order-independent; block scopes require
//!   declaration-before-use)
//! - assignment writability (`let` is immutable by default; only `let mut` is
//!   writable; `const` and function names are never writable)
//! - control-flow context (`break`/`continue` only inside loops; `return`
//!   only inside functions)
//!
//! The analyzer never panics on structurally valid ASTs: every lookup is
//! guarded, and independent problems are reported together so later
//! declarations and references are still analyzed (error recovery, not
//! stop-at-first-error).
//!
//! The semantic rules are documented in `docs/language/CORE_LANGUAGE.md` §24
//! and `docs/implementation/SEMANTIC_ANALYSIS_IMPLEMENTATION.md`.

use std::collections::HashMap;

use crate::ast::{
    Ast, Block, ElseBranch, EnumItem, Expr, ExprKind, FnItem, Ident, IfStmt, Item, ItemKind, Stmt,
    StmtKind, StructItem,
};
use crate::source::Span;

use super::SemanticResult;
use super::error::SemanticError;
use super::symbol::{ScopeId, ScopeKind, ScopeTable, SymbolId, SymbolKind, SymbolTable};

/// Runs semantic analysis over `ast`, returning the semantic result.
///
/// The analysis is deterministic: scopes, symbols, resolutions, and errors
/// are produced in source order.
pub(crate) fn analyze_ast(ast: &Ast) -> SemanticResult {
    let mut analyzer = Analyzer::new();
    let module = analyzer.collect_module_scope(ast);
    for item in &ast.items {
        analyzer.analyze_item(item, module);
    }
    SemanticResult::new(
        analyzer.symbols,
        analyzer.scopes,
        analyzer.resolutions,
        analyzer.errors,
    )
}

/// Lexical context carried through statement/expression traversal.
#[derive(Debug, Clone, Copy)]
struct Ctx {
    /// The scope in which names resolve.
    scope: ScopeId,
    /// Whether traversal is inside a loop body (`while`, `for`, `loop`).
    in_loop: bool,
    /// Whether traversal is inside a function body.
    in_function: bool,
}

/// A registered type declaration (a struct or an enum): its name's span,
/// used to detect duplicate type declarations (the first declaration of a
/// name wins). Struct and enum names share the type namespace, separate
/// from the value namespace of [`SymbolTable`]; the type checker resolves
/// type *structure* (struct fields, enum variants, member access) from the
/// AST directly.
#[derive(Debug, Clone)]
struct TypeDecl {
    /// Span of the declared name.
    span: Span,
    /// Whether the declaration is a `struct` or an `enum`. A collision
    /// between the two kinds is still a duplicate type name (they share
    /// the namespace); the error kind follows the *later* declaration.
    kind: TypeDeclKind,
}

/// The kind of a registered type declaration, for duplicate reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeDeclKind {
    /// A `struct` declaration.
    Struct,
    /// An `enum` declaration.
    Enum,
}

/// The semantic analyzer: one pass over the AST producing symbols, scopes,
/// name resolutions, and semantic errors.
struct Analyzer {
    /// Every declared symbol, in collection order.
    symbols: SymbolTable,
    /// Every lexical scope, in creation order.
    scopes: ScopeTable,
    /// Resolved name references: identifier span → resolved symbol.
    /// Inserted in source order; `SemanticResult` sorts and indexes them.
    resolutions: Vec<(Span, SymbolId)>,
    /// Semantic errors, in the order they were found.
    errors: Vec<SemanticError>,
    /// The registered type declarations (type namespace): name → decl.
    /// Holds both structs and enums; the first declaration of a name wins.
    types: HashMap<String, TypeDecl>,
}

impl Analyzer {
    fn new() -> Self {
        Self {
            symbols: SymbolTable::new(),
            scopes: ScopeTable::new(),
            resolutions: Vec::new(),
            errors: Vec::new(),
            types: HashMap::new(),
        }
    }

    /// Creates the module scope and binds every top-level declaration into
    /// it. Module scope is order-independent: all names are collected before
    /// any item body is analyzed, so a top-level declaration is visible
    /// throughout its module regardless of position.
    ///
    /// The runtime intrinsics (`rt_alloc`, …) are predeclared **before** the
    /// source items: they are part of the module scope, their names are
    /// reserved (a source declaration with the same name is a duplicate
    /// definition reported at the source declaration), and their symbols are
    /// collected first so the type checker's declaration-span index is never
    /// shadowed by a synthetic span.
    fn collect_module_scope(&mut self, ast: &Ast) -> ScopeId {
        let module = self.scopes.push(ScopeKind::Module, None);
        for intrinsic in crate::runtime::intrinsics::ALL {
            // A synthetic identifier: intrinsics have no source location.
            let ident = Ident {
                name: intrinsic.name.to_string(),
                span: crate::source::Span::new(crate::source::SourceId::new(0), 0..0),
            };
            self.bind(&ident, SymbolKind::Intrinsic, module);
        }
        for item in &ast.items {
            match &item.kind {
                ItemKind::Fn(f) => self.bind(&f.name, SymbolKind::Fn, module),
                ItemKind::Struct(s) => self.register_struct(s),
                ItemKind::Enum(e) => self.register_enum(e),
                ItemKind::Let(binding) => self.bind(
                    &binding.name,
                    SymbolKind::Let {
                        mutable: binding.mutable,
                    },
                    module,
                ),
                ItemKind::Const(binding) => self.bind(&binding.name, SymbolKind::Const, module),
            }
        }
        module
    }

    /// Registers a struct declaration in the type namespace, reporting a
    /// duplicate type name (E-S08) and duplicate fields (E-S09). The first
    /// declaration of a name wins; struct names are never value symbols, so
    /// they do not collide with functions, bindings, or other value
    /// declarations.
    fn register_struct(&mut self, s: &StructItem) {
        self.register_type(
            &s.name.name,
            s.name.span,
            TypeDeclKind::Struct,
            |span, original| SemanticError::duplicate_struct(s.name.name.clone(), span, original),
        );
        let registered = matches!(
            self.types.get(&s.name.name),
            Some(TypeDecl {
                span,
                kind: TypeDeclKind::Struct,
            }) if *span == s.name.span
        );
        if !registered {
            return;
        }
        // Duplicate fields within one declaration are reported here; the
        // type checker resolves the declared field types (the first
        // declaration of each field name wins for layout, mirroring this
        // duplicate policy).
        let mut seen: Vec<(String, Span)> = Vec::with_capacity(s.fields.len());
        for field in &s.fields {
            if let Some((_, original)) = seen.iter().find(|(name, _)| name == &field.name.name) {
                self.errors.push(SemanticError::duplicate_field(
                    field.name.name.clone(),
                    field.name.span,
                    *original,
                ));
            } else {
                seen.push((field.name.name.clone(), field.name.span));
            }
        }
    }

    /// Registers an enum declaration in the type namespace, reporting a
    /// duplicate type name (E-S15) and duplicate variants (E-S16). The
    /// first declaration of a name wins; enum names are never value
    /// symbols, so they do not collide with functions, bindings, or other
    /// value declarations.
    fn register_enum(&mut self, e: &EnumItem) {
        self.register_type(
            &e.name.name,
            e.name.span,
            TypeDeclKind::Enum,
            |span, original| SemanticError::duplicate_enum(e.name.name.clone(), span, original),
        );
        let registered = matches!(
            self.types.get(&e.name.name),
            Some(TypeDecl {
                span,
                kind: TypeDeclKind::Enum,
            }) if *span == e.name.span
        );
        if !registered {
            return;
        }
        let mut seen: Vec<(String, Span)> = Vec::with_capacity(e.variants.len());
        for variant in &e.variants {
            if let Some((_, original)) = seen.iter().find(|(name, _)| name == &variant.name.name) {
                self.errors.push(SemanticError::duplicate_variant(
                    variant.name.name.clone(),
                    variant.name.span,
                    *original,
                ));
            } else {
                seen.push((variant.name.name.clone(), variant.name.span));
            }
        }
    }

    /// The shared type-namespace registration: reports a duplicate when
    /// `name` is already registered (the first declaration wins) and
    /// otherwise records the declaration. The duplicate diagnostic is
    /// built by `error` (its kind follows the later declaration).
    fn register_type(
        &mut self,
        name: &str,
        span: Span,
        kind: TypeDeclKind,
        error: impl FnOnce(Span, Span) -> SemanticError,
    ) {
        match self.types.get(name) {
            Some(first) => self.errors.push(error(span, first.span)),
            None => {
                self.types.insert(name.to_string(), TypeDecl { span, kind });
            }
        }
    }

    /// Analyzes one top-level item. Module-scope declarations are already
    /// bound by [`Analyzer::collect_module_scope`]; this only analyzes their
    /// bodies and initializers.
    fn analyze_item(&mut self, item: &Item, module: ScopeId) {
        match &item.kind {
            ItemKind::Fn(f) => self.analyze_fn(f, module),
            // Struct and enum declarations were registered during
            // module-scope collection; their field types and variants are
            // resolved by the type checker.
            ItemKind::Struct(_) | ItemKind::Enum(_) => {}
            ItemKind::Let(binding) => {
                self.analyze_expr(&binding.init, Ctx::module(module));
            }
            ItemKind::Const(binding) => {
                self.analyze_expr(&binding.init, Ctx::module(module));
            }
        }
    }

    /// Analyzes a function: creates the function's declaration scope, binds
    /// its parameters, then analyzes the body statements in that scope.
    ///
    /// The function body block *is* the function's declaration scope:
    /// parameters and the body's own `let`/`const` declarations share one
    /// scope, so a parameter/local collision is a duplicate definition.
    fn analyze_fn(&mut self, f: &FnItem, module: ScopeId) {
        let scope = self.scopes.push(ScopeKind::Function, Some(module));
        for param in &f.params {
            self.bind(&param.name, SymbolKind::Param, scope);
        }
        let ctx = Ctx {
            scope,
            in_loop: false,
            in_function: true,
        };
        self.analyze_block(&f.body, ctx);
    }

    /// Declares `name` in `scope`, reporting a duplicate definition if the
    /// scope already declares it. On a duplicate, the original declaration
    /// wins and no new symbol is registered, so later references resolve
    /// deterministically without cascading errors.
    fn bind(&mut self, name: &Ident, kind: SymbolKind, scope: ScopeId) {
        let existing = self.scopes.get(scope).and_then(|s| s.lookup(&name.name));
        match existing {
            Some(first) => {
                let original = self
                    .symbols
                    .get(first)
                    .map(|symbol| symbol.span)
                    .unwrap_or(name.span);
                self.errors.push(SemanticError::duplicate(
                    name.name.clone(),
                    name.span,
                    original,
                ));
            }
            None => {
                let id = self.symbols.push(kind, name.name.clone(), name.span, scope);
                if let Some(scope_ref) = self.scopes.get_mut(scope) {
                    scope_ref.bind(name.name.clone(), id);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Statements
    // ------------------------------------------------------------------

    fn analyze_block(&mut self, block: &Block, ctx: Ctx) {
        for stmt in &block.stmts {
            self.analyze_stmt(stmt, ctx);
        }
    }

    fn analyze_stmt(&mut self, stmt: &Stmt, ctx: Ctx) {
        match &stmt.kind {
            StmtKind::Let(binding) => {
                // The initializer is analyzed before the binding is entered,
                // so a binding is not visible in its own initializer
                // (declaration-before-use within a block scope).
                self.analyze_expr(&binding.init, ctx);
                self.bind(
                    &binding.name,
                    SymbolKind::Let {
                        mutable: binding.mutable,
                    },
                    ctx.scope,
                );
            }
            StmtKind::Const(binding) => {
                self.analyze_expr(&binding.init, ctx);
                self.bind(&binding.name, SymbolKind::Const, ctx.scope);
            }
            StmtKind::Return(value) => {
                if !ctx.in_function {
                    // Defensive: the frozen grammar only allows statements
                    // inside function bodies, so this is currently
                    // unreachable from parser-produced ASTs.
                    self.errors
                        .push(SemanticError::return_outside_function(stmt.span));
                }
                if let Some(value) = value {
                    self.analyze_expr(value, ctx);
                }
            }
            StmtKind::Break => {
                if !ctx.in_loop {
                    self.errors
                        .push(SemanticError::break_outside_loop(stmt.span));
                }
            }
            StmtKind::Continue => {
                if !ctx.in_loop {
                    self.errors
                        .push(SemanticError::continue_outside_loop(stmt.span));
                }
            }
            StmtKind::If(stmt) => self.analyze_if(stmt, ctx),
            StmtKind::While { cond, body } => {
                self.analyze_expr(cond, ctx);
                let scope = self.scopes.push(ScopeKind::Block, Some(ctx.scope));
                self.analyze_block(
                    body,
                    Ctx {
                        scope,
                        in_loop: true,
                        ..ctx
                    },
                );
            }
            StmtKind::For {
                name,
                iterable,
                body,
            } => {
                // The iterable is analyzed in the enclosing scope; the loop
                // variable is declared in the loop body's scope.
                self.analyze_expr(iterable, ctx);
                let scope = self.scopes.push(ScopeKind::Block, Some(ctx.scope));
                self.bind(name, SymbolKind::ForVar, scope);
                self.analyze_block(
                    body,
                    Ctx {
                        scope,
                        in_loop: true,
                        ..ctx
                    },
                );
            }
            StmtKind::Loop(body) => {
                let scope = self.scopes.push(ScopeKind::Block, Some(ctx.scope));
                self.analyze_block(
                    body,
                    Ctx {
                        scope,
                        in_loop: true,
                        ..ctx
                    },
                );
            }
            StmtKind::Expr(expr) => self.analyze_expr(expr, ctx),
        }
    }

    fn analyze_if(&mut self, stmt: &IfStmt, ctx: Ctx) {
        self.analyze_expr(&stmt.cond, ctx);
        let then_scope = self.scopes.push(ScopeKind::Block, Some(ctx.scope));
        self.analyze_block(
            &stmt.then_block,
            Ctx {
                scope: then_scope,
                ..ctx
            },
        );
        match &stmt.else_branch {
            // Else-if chains keep their condition in the enclosing scope.
            Some(ElseBranch::If(nested)) => self.analyze_if(nested, ctx),
            Some(ElseBranch::Block(block)) => {
                let else_scope = self.scopes.push(ScopeKind::Block, Some(ctx.scope));
                self.analyze_block(
                    block,
                    Ctx {
                        scope: else_scope,
                        ..ctx
                    },
                );
            }
            None => {}
        }
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    fn analyze_expr(&mut self, expr: &Expr, ctx: Ctx) {
        match &expr.kind {
            ExprKind::Int
            | ExprKind::Float
            | ExprKind::Str
            | ExprKind::Char
            | ExprKind::Bool(_)
            | ExprKind::Null => {}
            ExprKind::Ident(ident) => self.resolve_name(ident, ctx),
            ExprKind::Unary { operand, .. } => self.analyze_expr(operand, ctx),
            ExprKind::Borrow { operand, .. } | ExprKind::Deref { operand } => {
                self.analyze_expr(operand, ctx);
            }
            ExprKind::Binary { lhs, rhs, .. } => {
                self.analyze_expr(lhs, ctx);
                self.analyze_expr(rhs, ctx);
            }
            ExprKind::Assign { target, value, .. } => {
                self.analyze_expr(value, ctx);
                self.analyze_assignment_target(target, ctx);
            }
            ExprKind::Range { start, end, .. } => {
                self.analyze_expr(start, ctx);
                self.analyze_expr(end, ctx);
            }
            ExprKind::Call { callee, args } => {
                self.analyze_call_target(callee, ctx);
                for arg in args {
                    self.analyze_expr(arg, ctx);
                }
            }
            ExprKind::Member { base, .. } => {
                // The member name is a field selector, not a scope name; it
                // belongs to the type system.
                self.analyze_expr(base, ctx);
            }
            ExprKind::Index { base, index } => {
                self.analyze_expr(base, ctx);
                self.analyze_expr(index, ctx);
            }
            ExprKind::StructLit { name: _, fields } => {
                // The struct name is a type name, not a value name: it is
                // never resolved as a scope name (unknown struct types are
                // reported by the type checker). Only the field *values*
                // are ordinary expressions.
                for field in fields {
                    self.analyze_expr(&field.value, ctx);
                }
            }
            ExprKind::EnumVariant { .. } => {
                // The enum and variant names are type/variant names, never
                // value names: they are not resolved as scope names
                // (unknown enums and variants are reported by the type
                // checker).
            }
            ExprKind::ArrayLit(elems) => {
                for elem in elems {
                    self.analyze_expr(elem, ctx);
                }
            }
            ExprKind::Group(inner) => self.analyze_expr(inner, ctx),
        }
    }

    /// Analyzes a call target. A plain identifier is resolved as a name; any
    /// other callee (member, call result, group, …) is analyzed normally.
    /// Whether the resolved symbol is actually callable is a type-system
    /// question and is deliberately deferred.
    fn analyze_call_target(&mut self, callee: &Expr, ctx: Ctx) {
        match &callee.kind {
            ExprKind::Ident(ident) => self.resolve_name(ident, ctx),
            _ => self.analyze_expr(callee, ctx),
        }
    }

    /// Resolves `ident` against the lexical scope chain.
    ///
    /// A successful resolution is recorded (span → symbol) for later
    /// compiler stages; a failed one produces an unresolved-name error.
    /// Every unresolved use is an independent diagnostic — an unresolved name
    /// used twice reports twice, which is not a cascade.
    fn resolve_name(&mut self, ident: &Ident, ctx: Ctx) {
        match self.lookup(&ident.name, ctx.scope) {
            Some(id) => self.resolutions.push((ident.span, id)),
            None => self
                .errors
                .push(SemanticError::unresolved(ident.name.clone(), ident.span)),
        }
    }

    /// Validates an assignment target semantically: the target must resolve,
    /// and a writable symbol must be reassignable.
    ///
    /// - Identifier targets: resolved, then checked for writability.
    /// - Member/index targets: the base expression is analyzed, and the
    ///   root base (when it is an identifier) must be writable — writing a
    ///   field or element through an immutable binding is an immutable/
    ///   constant assignment (`E-S03`/`E-S04`), like assigning the binding
    ///   directly.
    /// - Any other target shape (unreachable from parser-produced ASTs, where
    ///   the parser enforces place targets) is analyzed as a plain expression.
    fn analyze_assignment_target(&mut self, target: &Expr, ctx: Ctx) {
        match &target.kind {
            ExprKind::Ident(ident) => self.check_writability(ident, ctx),
            ExprKind::Member { .. } | ExprKind::Index { .. } => {
                self.analyze_expr(target, ctx);
                // The chain's root base must be a mutable binding. The base
                // analysis above already resolved it (and reported an
                // unresolved name); this only adds the writability check, so
                // no resolution is recorded twice and no error is doubled.
                if let Some(root) = root_base_ident(target) {
                    self.check_base_writability(root, ctx);
                }
            }
            _ => self.analyze_expr(target, ctx),
        }
    }

    /// Checks that the root identifier of a member/index assignment target
    /// is a writable binding. Unlike [`Analyzer::check_writability`], this
    /// does not push a resolution (the target's base analysis already did)
    /// and does not re-report an unresolved name.
    fn check_base_writability(&mut self, ident: &Ident, ctx: Ctx) {
        match self.lookup(&ident.name, ctx.scope) {
            Some(id) => match self.symbols.get(id).map(|symbol| symbol.kind) {
                Some(SymbolKind::Const) => self.errors.push(SemanticError::const_assignment(
                    ident.name.clone(),
                    ident.span,
                )),
                Some(kind) if !kind.is_mutable() => self.errors.push(
                    SemanticError::immutable_assignment(ident.name.clone(), ident.span),
                ),
                _ => {}
            },
            None => {
                // The base analysis already reported the unresolved name.
            }
        }
    }

    /// Resolves an identifier assignment target and rejects assignments to
    /// symbols that are not mutable.
    fn check_writability(&mut self, ident: &Ident, ctx: Ctx) {
        match self.lookup(&ident.name, ctx.scope) {
            Some(id) => {
                self.resolutions.push((ident.span, id));
                let kind = self.symbols.get(id).map(|symbol| symbol.kind);
                match kind {
                    Some(SymbolKind::Const) => self.errors.push(SemanticError::const_assignment(
                        ident.name.clone(),
                        ident.span,
                    )),
                    Some(kind) if !kind.is_mutable() => self.errors.push(
                        SemanticError::immutable_assignment(ident.name.clone(), ident.span),
                    ),
                    _ => {}
                }
            }
            None => self
                .errors
                .push(SemanticError::unresolved(ident.name.clone(), ident.span)),
        }
    }

    /// Looks `name` up by walking the scope chain from `scope` outward,
    /// returning the innermost declaration. O(1) per scope; total work is
    /// linear in source size.
    fn lookup(&self, name: &str, mut scope: ScopeId) -> Option<SymbolId> {
        loop {
            if let Some(id) = self.scopes.get(scope).and_then(|s| s.lookup(name)) {
                return Some(id);
            }
            let parent = self.scopes.get(scope).and_then(|s| s.parent)?;
            scope = parent;
        }
    }
}

impl Ctx {
    /// The module-scope context: no loop, no function.
    fn module(scope: ScopeId) -> Self {
        Self {
            scope,
            in_loop: false,
            in_function: false,
        }
    }
}

/// The root identifier of a member/index chain, if the chain bottoms out in
/// an identifier (`p.x.y` → `p`, `arr[i].x` → `arr`). Call results, groups,
/// and literals have no root identifier.
fn root_base_ident(expr: &Expr) -> Option<&Ident> {
    match &expr.kind {
        ExprKind::Ident(ident) => Some(ident),
        ExprKind::Member { base, .. } => root_base_ident(base),
        ExprKind::Index { base, .. } => root_base_ident(base),
        _ => None,
    }
}
