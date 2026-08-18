//! AST → HIR lowering.
//!
//! The [`lower`] entry point walks the parsed [`Ast`] once and produces a
//! [`HirProgram`] by consuming — never re-running — the session-05
//! [`SemanticResult`] and the session-06/07 [`TypeResult`]:
//!
//! - declaration names are resolved through a span→symbol index built from
//!   the semantic symbol table;
//! - identifier references are resolved through
//!   [`SemanticResult::resolve`];
//! - every expression and binding is typed through the exact-span lookup
//!   [`TypeResult::expr_type_exact`], and the recorded type is stored in
//!   canonical form (inference variables resolved to the type they denote);
//! - syntax-only `Group` nodes are eliminated: the inner node is kept with
//!   the parentheses' span.
//!
//! Lowering is deterministic (source order) and never panics on malformed
//! input: inconsistencies (an identifier with no resolved symbol, a missing
//! recorded type, a function symbol that is not a function type) are
//! collected as structured [`HirError`]s and returned as an `Err`, with
//! fallback nodes produced so analysis can continue and report every
//! independent problem.

use std::collections::HashMap;

use crate::ast::{
    Ast, Block, ConstItem, ElseBranch, Expr, ExprKind, FnItem, Ident, IfStmt, Item, ItemKind,
    LetItem, MatchArm, MatchStmt, Param, Pattern, Stmt, StmtKind, StructItem,
};
use crate::semantics::{SemanticResult, SymbolId};
use crate::source::Span;
use crate::typecheck::{TypeId, TypeKind, TypeResult, TypeTable};

use super::error::HirError;
use super::{
    HirBlock, HirConst, HirElseBranch, HirEnum, HirExpr, HirExprKind, HirFn, HirIdent, HirIf,
    HirItem, HirItemKind, HirLet, HirMatch, HirMatchArm, HirName, HirParam, HirPattern, HirProgram,
    HirStmt, HirStmtKind, HirStruct,
};

/// Lowers `ast` with its semantic and type results into HIR.
///
/// Returns the lowered [`HirProgram`], or every [`HirError`] collected in
/// source order when the input is internally inconsistent (unresolved
/// identifiers, missing recorded types, non-function function symbols).
/// These failures are only reachable on malformed input — a program that
/// passed semantic and type analysis always lowers successfully.
pub fn lower(
    ast: &Ast,
    semantic: &SemanticResult,
    types: &TypeResult,
) -> Result<HirProgram, Vec<HirError>> {
    let mut lowerer = Lowerer::new(ast, semantic, types);
    lowerer.run();
    // Record every predeclared runtime intrinsic symbol, so later stages
    // can recognize intrinsic references without re-running name
    // resolution. The mapping is stable: symbol id → intrinsic table id.
    let intrinsic_symbols = semantic
        .symbols()
        .iter()
        .filter(|symbol| symbol.kind == crate::semantics::SymbolKind::Intrinsic)
        .filter_map(|symbol| {
            crate::runtime::intrinsics::id_of(&symbol.name).map(|id| (symbol.id, id))
        })
        .collect();
    if lowerer.errors.is_empty() {
        Ok(HirProgram {
            items: lowerer.items,
            types: lowerer.table,
            intrinsic_symbols,
        })
    } else {
        Err(lowerer.errors)
    }
}

/// The lowering traversal. Owns the produced HIR and every fallback it had
/// to fabricate; when [`Lowerer::errors`] is non-empty the program is
/// discarded and the errors are returned instead.
struct Lowerer<'a> {
    ast: &'a Ast,
    semantic: &'a SemanticResult,
    types: &'a TypeResult,
    /// Declaration name span start → symbol id, built from the semantic
    /// symbol table so declaration names resolve without re-running name
    /// resolution.
    decls: HashMap<u32, SymbolId>,
    /// The type table backing every [`TypeId`] stored in the HIR. Cloned
    /// from the type result so the program is self-contained; lookups are
    /// canonicalized through this clone.
    table: TypeTable,
    /// Lazily created fallback error type for nodes whose recorded type is
    /// missing (defensive; only reachable on malformed input).
    error_ty: Option<TypeId>,
    items: Vec<HirItem>,
    errors: Vec<HirError>,
}

impl<'a> Lowerer<'a> {
    fn new(ast: &'a Ast, semantic: &'a SemanticResult, types: &'a TypeResult) -> Self {
        let mut decls = HashMap::new();
        for symbol in semantic.symbols().iter() {
            decls.insert(symbol.span.start(), symbol.id);
        }
        // Or-pattern binding aliases (session 27): every occurrence of an
        // or-pattern binding after its first resolves to the same symbol,
        // so every alternative's binding lowers to the one logical local.
        for (span, symbol) in semantic.binding_aliases() {
            decls.insert(span.start(), *symbol);
        }
        Self {
            ast,
            semantic,
            types,
            decls,
            table: types.types().clone(),
            error_ty: None,
            items: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn run(&mut self) {
        for item in &self.ast.items {
            let lowered = self.lower_item(item);
            self.items.push(lowered);
        }
    }

    // ------------------------------------------------------------------
    // Items
    // ------------------------------------------------------------------

    fn lower_item(&mut self, item: &Item) -> HirItem {
        let kind = match &item.kind {
            ItemKind::Fn(f) => HirItemKind::Fn(self.lower_fn(f, item.span)),
            ItemKind::Struct(s) => HirItemKind::Struct(self.lower_struct(s)),
            ItemKind::Enum(e) => HirItemKind::Enum(self.lower_enum(e)),
            ItemKind::Let(binding) => HirItemKind::Let(self.lower_let(binding, item.span)),
            ItemKind::Const(binding) => HirItemKind::Const(self.lower_const(binding, item.span)),
        };
        HirItem {
            kind,
            span: item.span,
        }
    }

    /// A struct declaration lowers to a plain name: struct names are type
    /// names, not symbols, and their fields live in the type table.
    fn lower_struct(&mut self, s: &StructItem) -> HirStruct {
        HirStruct {
            name: HirName {
                name: s.name.name.clone(),
                span: s.name.span,
            },
            span: s.span,
        }
    }

    /// An enum declaration lowers to a plain name: enum names are type
    /// names, not symbols, and their variants live in the type table.
    fn lower_enum(&mut self, e: &crate::ast::EnumItem) -> HirEnum {
        HirEnum {
            name: HirName {
                name: e.name.name.clone(),
                span: e.name.span,
            },
            span: e.span,
        }
    }

    fn lower_fn(&mut self, f: &FnItem, span: Span) -> HirFn {
        let name = self.lower_decl_ident(&f.name);
        let ty = self.fn_type(&name);
        let params = f
            .params
            .iter()
            .map(|param| self.lower_param(param))
            .collect();
        let body = self.lower_block(&f.body);
        HirFn {
            name,
            params,
            body,
            span,
            ty,
        }
    }

    fn lower_param(&mut self, param: &Param) -> HirParam {
        let name = self.lower_decl_ident(&param.name);
        HirParam {
            ty: name.ty,
            name,
            span: param.span,
        }
    }

    fn lower_let(&mut self, binding: &LetItem, span: Span) -> HirLet {
        let name = self.lower_decl_ident(&binding.name);
        HirLet {
            ty: name.ty,
            name,
            mutable: binding.mutable,
            init: Box::new(self.lower_expr(&binding.init)),
            span,
        }
    }

    fn lower_const(&mut self, binding: &ConstItem, span: Span) -> HirConst {
        let name = self.lower_decl_ident(&binding.name);
        HirConst {
            ty: name.ty,
            name,
            init: Box::new(self.lower_expr(&binding.init)),
            span,
        }
    }

    // ------------------------------------------------------------------
    // Statements
    // ------------------------------------------------------------------

    fn lower_block(&mut self, block: &Block) -> HirBlock {
        HirBlock {
            stmts: block
                .stmts
                .iter()
                .map(|stmt| self.lower_stmt(stmt))
                .collect(),
            span: block.span,
        }
    }

    fn lower_stmt(&mut self, stmt: &Stmt) -> HirStmt {
        let kind = match &stmt.kind {
            StmtKind::Let(binding) => HirStmtKind::Let(self.lower_let(binding, stmt.span)),
            StmtKind::Const(binding) => HirStmtKind::Const(self.lower_const(binding, stmt.span)),
            StmtKind::Return(value) => {
                HirStmtKind::Return(value.as_ref().map(|expr| self.lower_expr(expr)))
            }
            StmtKind::Break => HirStmtKind::Break,
            StmtKind::Continue => HirStmtKind::Continue,
            StmtKind::If(stmt) => HirStmtKind::If(self.lower_if(stmt)),
            StmtKind::While { cond, body } => HirStmtKind::While {
                cond: self.lower_expr(cond),
                body: self.lower_block(body),
            },
            StmtKind::For {
                name,
                iterable,
                body,
            } => HirStmtKind::For {
                var: self.lower_decl_ident(name),
                iterable: self.lower_expr(iterable),
                body: self.lower_block(body),
            },
            StmtKind::Loop(body) => HirStmtKind::Loop(self.lower_block(body)),
            StmtKind::Match(stmt) => HirStmtKind::Match(self.lower_match(stmt)),
            StmtKind::Expr(expr) => HirStmtKind::Expr(self.lower_expr(expr)),
        };
        HirStmt {
            kind,
            span: stmt.span,
        }
    }

    fn lower_match(&mut self, stmt: &MatchStmt) -> HirMatch {
        HirMatch {
            scrutinee: self.lower_expr(&stmt.scrutinee),
            arms: stmt
                .arms
                .iter()
                .map(|arm| self.lower_match_arm(arm))
                .collect(),
            span: stmt.span,
        }
    }

    fn lower_match_arm(&mut self, arm: &MatchArm) -> HirMatchArm {
        HirMatchArm {
            pattern: self.lower_pattern(&arm.pattern),
            guard: arm.guard.as_ref().map(|guard| self.lower_expr(guard)),
            body: self.lower_block(&arm.body),
            span: arm.span,
        }
    }

    /// Lowers a match pattern, resolving pattern bindings through the
    /// declaration index like any other binding. A data-carrying variant
    /// pattern's payload pattern is lowered recursively.
    fn lower_pattern(&mut self, pattern: &Pattern) -> HirPattern {
        match pattern {
            Pattern::Wildcard { span } => HirPattern::Wildcard { span: *span },
            Pattern::Binding(ident) => HirPattern::Binding(self.lower_decl_ident(ident)),
            Pattern::EnumVariant {
                name,
                variant,
                payload,
            } => HirPattern::EnumVariant {
                name: Box::new(HirName {
                    name: name.name.clone(),
                    span: name.span,
                }),
                variant: Box::new(HirName {
                    name: variant.name.clone(),
                    span: variant.span,
                }),
                payload: payload
                    .as_ref()
                    .map(|inner| Box::new(self.lower_pattern(inner))),
                span: pattern.span(),
            },
            Pattern::Bool { value, span } => HirPattern::Bool {
                value: *value,
                span: *span,
            },
            Pattern::Int {
                negative,
                literal,
                span,
            } => HirPattern::Int {
                negative: *negative,
                literal_span: literal.span,
                span: *span,
            },
            Pattern::Range {
                lo,
                hi,
                inclusive,
                span,
            } => {
                // Endpoints are integer literal patterns (the parser
                // guarantees it); extract their token spans and signs.
                let (lo_negative, lo_span) = Self::int_endpoint(lo);
                let (hi_negative, hi_span) = Self::int_endpoint(hi);
                HirPattern::Range {
                    lo_negative,
                    lo_span,
                    hi_negative,
                    hi_span,
                    inclusive: *inclusive,
                    span: *span,
                }
            }
            Pattern::Or { alternatives, span } => HirPattern::Or {
                alternatives: alternatives
                    .iter()
                    .map(|alt| self.lower_pattern(alt))
                    .collect(),
                span: *span,
            },
        }
    }

    /// The `(negative, span)` of a range-pattern endpoint (an `Int`
    /// pattern, possibly negated). The parser guarantees the endpoint is
    /// an integer literal; the fallback is defensive.
    fn int_endpoint(pattern: &Pattern) -> (bool, Span) {
        match pattern {
            Pattern::Int {
                negative, literal, ..
            } => (*negative, literal.span),
            _ => (false, pattern.span()),
        }
    }

    fn lower_if(&mut self, stmt: &IfStmt) -> HirIf {
        let else_branch = stmt.else_branch.as_ref().map(|branch| match branch {
            ElseBranch::If(nested) => HirElseBranch::If(Box::new(self.lower_if(nested))),
            ElseBranch::Block(block) => HirElseBranch::Block(self.lower_block(block)),
        });
        HirIf {
            cond: self.lower_expr(&stmt.cond),
            then_block: self.lower_block(&stmt.then_block),
            else_branch,
            span: stmt.span,
        }
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    fn lower_expr(&mut self, expr: &Expr) -> HirExpr {
        let kind = match &expr.kind {
            ExprKind::Int => HirExprKind::Int,
            ExprKind::Float => HirExprKind::Float,
            ExprKind::Str => HirExprKind::Str,
            ExprKind::Char => HirExprKind::Char,
            ExprKind::Bool(value) => HirExprKind::Bool(*value),
            ExprKind::Null => HirExprKind::Null,
            ExprKind::Ident(ident) => self.lower_var(ident),
            ExprKind::Unary { op, operand } => HirExprKind::Unary {
                op: *op,
                operand: Box::new(self.lower_expr(operand)),
            },
            ExprKind::Borrow { mutable, operand } => HirExprKind::Borrow {
                mutable: *mutable,
                operand: Box::new(self.lower_expr(operand)),
            },
            ExprKind::Deref { operand } => HirExprKind::Deref {
                operand: Box::new(self.lower_expr(operand)),
            },
            ExprKind::Binary { op, lhs, rhs } => HirExprKind::Binary {
                op: *op,
                lhs: Box::new(self.lower_expr(lhs)),
                rhs: Box::new(self.lower_expr(rhs)),
            },
            ExprKind::Assign { op, target, value } => HirExprKind::Assign {
                op: *op,
                target: Box::new(self.lower_expr(target)),
                value: Box::new(self.lower_expr(value)),
            },
            ExprKind::Range {
                inclusive,
                start,
                end,
            } => HirExprKind::Range {
                inclusive: *inclusive,
                start: Box::new(self.lower_expr(start)),
                end: Box::new(self.lower_expr(end)),
            },
            ExprKind::Call { callee, args } => HirExprKind::Call {
                callee: Box::new(self.lower_expr(callee)),
                args: args.iter().map(|arg| self.lower_expr(arg)).collect(),
            },
            ExprKind::Member { base, member } => HirExprKind::Member {
                base: Box::new(self.lower_expr(base)),
                member: HirName {
                    name: member.name.clone(),
                    span: member.span,
                },
            },
            ExprKind::Index { base, index } => HirExprKind::Index {
                base: Box::new(self.lower_expr(base)),
                index: Box::new(self.lower_expr(index)),
            },
            ExprKind::StructLit { name, fields } => HirExprKind::StructLit {
                name: HirName {
                    name: name.name.clone(),
                    span: name.span,
                },
                fields: fields
                    .iter()
                    .map(|field| {
                        (
                            HirName {
                                name: field.name.name.clone(),
                                span: field.name.span,
                            },
                            self.lower_expr(&field.value),
                        )
                    })
                    .collect(),
            },
            ExprKind::ArrayLit(elems) => {
                HirExprKind::ArrayLit(elems.iter().map(|elem| self.lower_expr(elem)).collect())
            }
            ExprKind::EnumVariant {
                name,
                variant,
                payload,
            } => HirExprKind::EnumVariant {
                name: Box::new(HirName {
                    name: name.name.clone(),
                    span: name.span,
                }),
                variant: Box::new(HirName {
                    name: variant.name.clone(),
                    span: variant.span,
                }),
                payload: payload
                    .as_ref()
                    .map(|inner| Box::new(self.lower_expr(inner))),
            },
            ExprKind::Group(inner) => {
                // Syntax-only grouping: lower the inner expression and keep
                // the parentheses' span, so the node covers the source text
                // as written. The group's type is the inner type; the group
                // span is normally recorded too, but the inner type is
                // authoritative, so a missing group-span entry is not an
                // error.
                let inner = self.lower_expr(inner);
                let ty = self
                    .types
                    .expr_type_exact(expr.span)
                    .map(|id| self.table.canonical(id))
                    .unwrap_or(inner.ty);
                return HirExpr {
                    kind: inner.kind,
                    span: expr.span,
                    ty,
                };
            }
        };
        let ty = self.expr_type(expr.span);
        HirExpr {
            kind,
            span: expr.span,
            ty,
        }
    }

    fn lower_var(&mut self, ident: &Ident) -> HirExprKind {
        match self.semantic.resolve(ident.span) {
            Some(symbol) => match self.symbol_type(symbol) {
                Some(ty) => HirExprKind::Var(HirIdent {
                    name: ident.name.clone(),
                    span: ident.span,
                    symbol,
                    ty,
                }),
                None => {
                    self.errors.push(HirError::missing_type(ident.span));
                    HirExprKind::Var(self.fallback_ident(ident))
                }
            },
            None => {
                self.errors.push(HirError::unresolved(ident.span));
                HirExprKind::Var(self.fallback_ident(ident))
            }
        }
    }

    // ------------------------------------------------------------------
    // Resolution and typing helpers
    // ------------------------------------------------------------------

    /// The symbol a declaration name refers to, from the span→symbol index.
    fn decl_symbol(&self, name: &Ident) -> Option<SymbolId> {
        self.decls.get(&name.span.start()).copied()
    }

    /// Lower a declaration name (function name, parameter, binding, loop
    /// variable): its symbol from the declaration index and its type from
    /// the type result. On an inconsistent input a structured error is
    /// recorded and a fallback identifier produced.
    fn lower_decl_ident(&mut self, name: &Ident) -> HirIdent {
        match self.decl_symbol(name) {
            Some(symbol) => match self.symbol_type(symbol) {
                Some(ty) => HirIdent {
                    name: name.name.clone(),
                    span: name.span,
                    symbol,
                    ty,
                },
                None => {
                    self.errors.push(HirError::missing_type(name.span));
                    self.fallback_ident(name)
                }
            },
            None => {
                self.errors.push(HirError::unresolved(name.span));
                self.fallback_ident(name)
            }
        }
    }

    /// The canonical type recorded for `symbol`, if any.
    fn symbol_type(&self, symbol: SymbolId) -> Option<TypeId> {
        self.types
            .symbol_type(symbol)
            .map(|id| self.table.canonical(id))
    }

    /// The canonical type recorded for the expression covering exactly
    /// `span`, or a structured error plus the fallback error type.
    fn expr_type(&mut self, span: Span) -> TypeId {
        match self.types.expr_type_exact(span) {
            Some(id) => self.table.canonical(id),
            None => {
                self.errors.push(HirError::missing_type(span));
                self.error_type()
            }
        }
    }

    /// The function type of a lowered function name: the name's type must
    /// be a `Fn` type (the checker always registers one for function
    /// symbols); anything else is an internal inconsistency.
    fn fn_type(&mut self, name: &HirIdent) -> TypeId {
        if self.table.is_error(name.ty) {
            // The name itself already failed to resolve or type; the
            // original problem is reported, do not add noise.
            return name.ty;
        }
        match self.table.kind(name.ty) {
            Some(TypeKind::Fn { .. }) => name.ty,
            _ => {
                self.errors.push(HirError::invalid_function_type(
                    name.span,
                    self.table.display(name.ty),
                ));
                self.error_type()
            }
        }
    }

    /// The shared fallback error type, created on first use.
    fn error_type(&mut self) -> TypeId {
        *self
            .error_ty
            .get_or_insert_with(|| self.table.push(TypeKind::Error))
    }

    /// A fallback identifier for an inconsistent declaration or reference.
    ///
    /// The symbol id `0` is fabricated and meaningless: a structured
    /// [`HirError`] was already recorded for the real problem, and the
    /// program is discarded when errors are present.
    fn fallback_ident(&mut self, name: &Ident) -> HirIdent {
        HirIdent {
            name: name.name.clone(),
            span: name.span,
            symbol: SymbolId::new(0),
            ty: self.error_type(),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Internal-failure tests: the lowering error paths that the public
    //! pipeline (clean semantic + type results) can never reach. These use
    //! the crate-internal result constructors to fabricate inconsistent
    //! inputs and assert the structured errors produced.

    use std::path::Path;

    use crate::ast::Ast;
    use crate::parser;
    use crate::semantics::{self, SemanticResult};
    use crate::source::{SourceId, SourceMap, Span};
    use crate::typecheck::{TypeKind, TypeResult, TypeTable};

    use super::super::error::HirErrorKind;
    use super::lower;

    /// Parses and semantically analyzes `src`, asserting it is syntactically
    /// valid. The file is registered as the first source (id `0`).
    fn parse_and_analyze(src: &str) -> (SourceMap, Ast, SemanticResult) {
        let mut sources = SourceMap::new();
        let id = sources.add(Path::new("t.mink"), src);
        let file = sources.get(id).unwrap();
        let parsed = parser::parse(file);
        assert!(parsed.is_valid());
        let (ast, _, _) = parsed.into_parts();
        let semantic = semantics::analyze(&ast);
        (sources, ast, semantic)
    }

    fn text_span(src: &str, needle: &str) -> Span {
        let start = src
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` not found"));
        Span::new(
            SourceId::new(0),
            start as u32..start as u32 + needle.len() as u32,
        )
    }

    #[test]
    fn missing_expression_and_symbol_types_are_reported() {
        let src = "fn f() { let x = 1; }";
        let (_sources, ast, semantic) = parse_and_analyze(src);
        // Fabricate a type result with no recorded expression or symbol
        // types at all.
        let table = TypeTable::new();
        let types = TypeResult::new(Vec::new(), Vec::new(), Vec::new(), table);
        let errors = lower(&ast, &semantic, &types).unwrap_err();
        // The initializer `1` is one of the expressions with no recorded
        // type; every missing type is reported (never a panic).
        assert!(
            errors.iter().any(|error| {
                error.kind() == HirErrorKind::MissingType && error.span() == text_span(src, "1")
            }),
            "errors: {errors:?}"
        );
        // Errors are deterministic and source ordered.
        assert!(
            errors
                .windows(2)
                .all(|pair| pair[0].span().start() <= pair[1].span().start())
        );
        assert!(
            errors
                .iter()
                .all(|error| error.kind() == HirErrorKind::MissingType)
        );
    }

    #[test]
    fn non_function_symbol_type_is_reported() {
        let src = "fn f() {}";
        let (_sources, ast, semantic) = parse_and_analyze(src);
        let f = semantic.symbols().iter().find(|s| s.name == "f").unwrap();
        let mut table = TypeTable::new();
        let placeholder = table.push(TypeKind::Error);
        let int_ty = table.push(TypeKind::Int);
        let mut symbol_types = vec![placeholder; semantic.symbols().len()];
        symbol_types[f.id.raw() as usize] = int_ty;
        let types = TypeResult::new(Vec::new(), symbol_types, Vec::new(), table);
        let errors = lower(&ast, &semantic, &types).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].kind(), HirErrorKind::InvalidFunctionType);
        assert_eq!(errors[0].code(), "E-H03");
        assert_eq!(errors[0].span(), f.span);
        assert_eq!(errors[0].detail(), Some("Int"));
    }
}
