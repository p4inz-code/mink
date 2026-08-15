//! Ownership and borrowing analysis (session 15).
//!
//! A minimal, sound ownership foundation: values that own heap storage
//! (`Str` blobs from `rt_str_alloc`, and aggregates containing them) are
//! **moved** on transfer — binding initialization, assignment, struct/array
//! literal elements, `return`, user-function arguments, and `rt_str_free` —
//! and using a moved value is a compile-time error (`E-S10`). String
//! literals are **Immutable** (immortal image data) and copy freely; the
//! only way to mutate a string (`rt_str_set_byte`) requires an Owned value
//! (`E-S11`). Reads — intrinsic read arguments, operator operands,
//! conditions — are implicit function-local borrows that leave the binding
//! live and owned.
//!
//! The analysis is a deterministic, scope-aware dataflow walk over the
//! [`Ast`] consuming the semantic result (resolution, symbols) and the type
//! result (expression/symbol types, struct field tables). It never mutates
//! the AST and requires no runtime or backend support: moves are a
//! compile-time fiction, and the runtime remains the safety backstop
//! (`E-R04`–`E-R10`).
//!
//! The frozen rules are documented in
//! `docs/implementation/OWNERSHIP_IMPLEMENTATION.md` §1.

use std::collections::HashMap;

use crate::ast::{
    AssignOp, Ast, ElseBranch, Expr, ExprKind, FnItem, Ident, IfStmt, ItemKind, Stmt, StmtKind,
};
use crate::runtime::intrinsics::{self, Intrinsic, IntrinsicType};
use crate::semantics::{SemanticError, SemanticResult, SymbolId, SymbolKind};
use crate::source::Span;
use crate::typecheck::{TypeId, TypeKind, TypeResult};

/// Whether a value that may own heap storage is owned or immortal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// Immortal literal data: copying is always safe.
    Immutable,
    /// Heap-owned: exactly one owner; transfers move.
    Owned,
}

/// The liveness of one value or field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingState {
    /// The value is live with the given provenance.
    Live(Provenance),
    /// The value was moved away; any use is an error.
    Dead,
}

/// The per-field state of a struct binding, for fields whose type may own
/// heap storage (Str, or an aggregate containing one).
type FieldStates = HashMap<String, BindingState>;

/// The tracked state of one binding.
#[derive(Debug, Clone)]
enum State {
    /// A `Str` value.
    Str(BindingState),
    /// A struct value: per-field states for tracked fields plus whether
    /// the whole struct was moved (any field read then errors, even for
    /// copy-typed fields).
    Struct {
        /// Per-field liveness for tracked (`Str`-containing) fields.
        fields: FieldStates,
        /// The whole struct was moved: every field is inaccessible.
        dead: bool,
    },
    /// An array value: whole-array state (per-element liveness is out of
    /// scope; reading an Owned array's element in a transfer position
    /// moves the whole array).
    Array(BindingState),
}

/// The evaluated value of an expression: its provenance plus, for struct
/// values, the per-field liveness of tracked fields.
#[derive(Debug, Clone)]
struct EvalValue {
    provenance: Provenance,
    /// Per-field liveness for struct values (tracked fields only).
    fields: Option<FieldStates>,
}

impl EvalValue {
    fn copy() -> Self {
        Self {
            provenance: Provenance::Immutable,
            fields: None,
        }
    }

    fn immutable() -> Self {
        Self {
            provenance: Provenance::Immutable,
            fields: None,
        }
    }

    fn owned() -> Self {
        Self {
            provenance: Provenance::Owned,
            fields: None,
        }
    }

    fn with_provenance(provenance: Provenance) -> Self {
        Self {
            provenance,
            fields: None,
        }
    }

    /// A struct value: Owned when any tracked field is live-owned.
    fn struct_value(fields: FieldStates) -> Self {
        let provenance = if fields
            .values()
            .any(|state| matches!(state, BindingState::Live(Provenance::Owned)))
        {
            Provenance::Owned
        } else {
            Provenance::Immutable
        };
        Self {
            provenance,
            fields: Some(fields),
        }
    }
}

/// The result of ownership analysis.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnershipResult {
    errors: Vec<SemanticError>,
}

impl OwnershipResult {
    /// The ownership diagnostics, in traversal order (the driver sorts
    /// them by source position).
    pub fn errors(&self) -> &[SemanticError] {
        &self.errors
    }

    /// Whether the analysis reported any ownership errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// The per-function result provenance computed by the fixpoint: whether a
/// `Str`-typed result is Owned, and the per-field provenances of a struct
/// result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FnResult {
    /// `Some(Owned)` when any `return` value is Owned, `Some(Immutable)`
    /// when every `return` is Immutable, `None` when the function never
    /// returns a `Str`/aggregate value.
    provenance: Option<Provenance>,
    /// Per-field provenances of a struct result (tracked fields only).
    fields: Option<HashMap<String, Provenance>>,
}

/// Runs ownership analysis over `ast` given the semantic and type results
/// (both must be error-free for the analysis to be meaningful; the driver
/// only calls it then).
pub fn check(ast: &Ast, semantic: &SemanticResult, types: &TypeResult) -> OwnershipResult {
    let mut analyzer = Analyzer {
        ast,
        semantic,
        types,
        decl_spans: HashMap::new(),
        bindings: HashMap::new(),
        errors: Vec::new(),
        result: FnResult::default(),
        fn_results: HashMap::new(),
    };
    analyzer.run();
    OwnershipResult {
        errors: analyzer.errors,
    }
}

/// The mode an expression is evaluated in: transfer positions move Owned
/// values; observation positions borrow them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The expression's value is being transferred: an Owned value moves.
    Transfer,
    /// The expression is being observed (read, borrowed): no move.
    Observe,
}

/// The ownership-relevant shape of a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// A `Str` value.
    Str,
    /// A struct with at least one tracked (Str-containing) field.
    Struct,
    /// An array whose element type may own.
    Array,
    /// Nothing is tracked: copying is always safe.
    Copy,
}

struct Analyzer<'a> {
    ast: &'a Ast,
    semantic: &'a SemanticResult,
    types: &'a TypeResult,
    /// Declaration name span start → symbol id, for binding lookups.
    decl_spans: HashMap<u32, SymbolId>,
    /// The tracked binding states, keyed by symbol id.
    bindings: HashMap<SymbolId, State>,
    /// Ownership errors, in traversal order.
    errors: Vec<SemanticError>,
    /// The current function's accumulated result provenance.
    result: FnResult,
    /// Every function's computed result provenance (the fixpoint state).
    fn_results: HashMap<SymbolId, FnResult>,
}

impl<'a> Analyzer<'a> {
    fn run(&mut self) {
        self.build_decl_spans();
        // Result provenance of a function can depend on the results of the
        // functions it calls, so iterate to a fixpoint: unknown callee
        // results are Owned (conservative), and each pass can only refine
        // `Owned` toward `Immutable` as callee information improves. The
        // cap is defensive; convergence is guaranteed by the finite
        // lattice, and the final pass's errors are authoritative.
        let cap = self.ast.items.len() + 2;
        for _ in 0..cap {
            let before = self.fn_results.clone();
            self.bindings.clear();
            self.errors.clear();
            self.walk_module();
            if self.fn_results == before {
                break;
            }
        }
    }

    /// Indexes every declaration by its name span so bindings can be
    /// registered and looked up during the walk.
    fn build_decl_spans(&mut self) {
        for symbol in self.semantic.symbols().iter() {
            self.decl_spans.insert(symbol.span.start(), symbol.id);
        }
    }

    /// The symbol a declaration name resolves to (declarations are always
    /// registered by the semantic analyzer).
    fn symbol_of(&self, name: &Ident) -> Option<SymbolId> {
        self.decl_spans.get(&name.span.start()).copied()
    }

    // ------------------------------------------------------------------
    // Module and statement walks
    // ------------------------------------------------------------------

    fn walk_module(&mut self) {
        for item in &self.ast.items {
            match &item.kind {
                ItemKind::Struct(_) => {}
                ItemKind::Let(binding) => {
                    let value = self.eval_expr(&binding.init, Mode::Transfer);
                    self.bind(binding.name.span, &value);
                }
                ItemKind::Const(binding) => {
                    let value = self.eval_expr(&binding.init, Mode::Transfer);
                    self.bind(binding.name.span, &value);
                }
                ItemKind::Fn(f) => {
                    if let Some(symbol) = self.symbol_of(&f.name) {
                        let result = self.analyze_fn(f);
                        self.fn_results.insert(symbol, result);
                    }
                }
            }
        }
    }

    fn analyze_fn(&mut self, f: &FnItem) -> FnResult {
        for param in &f.params {
            if let Some(symbol) = self.symbol_of(&param.name) {
                self.bind_param(symbol);
            }
        }
        let saved = std::mem::take(&mut self.result);
        self.walk_block(&f.body);
        let result = std::mem::take(&mut self.result);
        self.result = saved;
        result
    }

    /// Registers a parameter's state: parameters own their values
    /// (conservatively, every tracked field of a struct parameter is
    /// Owned).
    fn bind_param(&mut self, symbol: SymbolId) {
        let Some(ty) = self.types.symbol_type(symbol) else {
            return;
        };
        match self.type_shape(ty) {
            Shape::Str => {
                self.bindings
                    .insert(symbol, State::Str(BindingState::Live(Provenance::Owned)));
            }
            Shape::Struct => {
                let fields = self
                    .tracked_struct_fields(ty)
                    .map(|name| (name, BindingState::Live(Provenance::Owned)))
                    .collect();
                self.bindings.insert(
                    symbol,
                    State::Struct {
                        fields,
                        dead: false,
                    },
                );
            }
            Shape::Array => {
                self.bindings
                    .insert(symbol, State::Array(BindingState::Live(Provenance::Owned)));
            }
            Shape::Copy => {}
        }
    }

    /// Binds a newly declared name to the evaluated value of its
    /// initializer.
    fn bind(&mut self, name: Span, value: &EvalValue) {
        let Some(symbol) = self.decl_spans.get(&name.start()).copied() else {
            return;
        };
        let Some(ty) = self.types.symbol_type(symbol) else {
            return;
        };
        self.bind_name_with_type(symbol, ty, value);
    }

    /// Binds `symbol` (whose type is `ty`) from `value`: struct bindings
    /// take per-field liveness from the value when known and conservatively
    /// Owned fields otherwise; Str/array bindings take the value's
    /// provenance.
    fn bind_name_with_type(&mut self, symbol: SymbolId, ty: TypeId, value: &EvalValue) {
        match self.type_shape(ty) {
            Shape::Str => {
                self.bindings
                    .insert(symbol, State::Str(BindingState::Live(value.provenance)));
            }
            Shape::Struct => {
                let fields = self
                    .tracked_struct_fields(ty)
                    .map(|field_name| {
                        let state = value
                            .fields
                            .as_ref()
                            .and_then(|fields| fields.get(&field_name))
                            .copied()
                            // Unknown per-field provenance (a nested or
                            // call result): conservatively Owned. Dead
                            // states from an invalid move never reach a
                            // binding (the move already errored); map them
                            // defensively to live-immutable.
                            .unwrap_or(BindingState::Live(Provenance::Owned));
                        (field_name, live_or_immutable(state))
                    })
                    .collect();
                self.bindings.insert(
                    symbol,
                    State::Struct {
                        fields,
                        dead: false,
                    },
                );
            }
            Shape::Array => {
                self.bindings
                    .insert(symbol, State::Array(BindingState::Live(value.provenance)));
            }
            Shape::Copy => {}
        }
    }

    fn walk_block(&mut self, block: &crate::ast::Block) {
        for stmt in &block.stmts {
            self.walk_stmt(stmt);
        }
    }

    fn walk_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Let(binding) => {
                let value = self.eval_expr(&binding.init, Mode::Transfer);
                self.bind(binding.name.span, &value);
            }
            StmtKind::Const(binding) => {
                let value = self.eval_expr(&binding.init, Mode::Transfer);
                self.bind(binding.name.span, &value);
            }
            StmtKind::Return(Some(value)) => {
                let evaluated = self.eval_expr(value, Mode::Transfer);
                self.merge_result(&evaluated);
            }
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
            StmtKind::If(stmt) => self.walk_if(stmt),
            StmtKind::While { cond, body } => {
                self.eval_expr(cond, Mode::Observe);
                self.walk_block(body);
            }
            StmtKind::For {
                name: _,
                iterable,
                body,
            } => {
                self.eval_expr(iterable, Mode::Observe);
                // The `for` variable is the range element type (a copy
                // type today), so it is never tracked.
                self.walk_block(body);
            }
            StmtKind::Loop(body) => self.walk_block(body),
            StmtKind::Expr(expr) => {
                self.eval_expr(expr, Mode::Observe);
            }
        }
    }

    fn walk_if(&mut self, stmt: &IfStmt) {
        self.eval_expr(&stmt.cond, Mode::Observe);
        self.walk_block(&stmt.then_block);
        match &stmt.else_branch {
            Some(ElseBranch::If(nested)) => self.walk_if(nested),
            Some(ElseBranch::Block(block)) => self.walk_block(block),
            None => {}
        }
    }

    /// Records a `return` value's provenance into the current function's
    /// result (a struct result's per-field provenances merge too).
    fn merge_result(&mut self, value: &EvalValue) {
        match value.provenance {
            Provenance::Owned => self.result.provenance = Some(Provenance::Owned),
            Provenance::Immutable => {
                if self.result.provenance.is_none() {
                    self.result.provenance = Some(Provenance::Immutable);
                }
            }
        }
        let Some(fields) = &value.fields else {
            return;
        };
        let merged = self.result.fields.get_or_insert_with(HashMap::new);
        for (name, state) in fields {
            let entry = merged.entry(name.clone()).or_insert(Provenance::Immutable);
            if matches!(state, BindingState::Live(Provenance::Owned)) {
                *entry = Provenance::Owned;
            }
        }
    }

    // ------------------------------------------------------------------
    // Expression evaluation
    // ------------------------------------------------------------------

    /// Evaluates `expr` in the given mode and returns the value's
    /// provenance (and, for struct values, per-field liveness).
    fn eval_expr(&mut self, expr: &Expr, mode: Mode) -> EvalValue {
        match &expr.kind {
            ExprKind::Int
            | ExprKind::Float
            | ExprKind::Bool(_)
            | ExprKind::Char
            | ExprKind::Null => EvalValue::copy(),
            ExprKind::Str => EvalValue::immutable(),
            ExprKind::Ident(ident) => self.eval_ident(ident, mode),
            ExprKind::Unary { operand, .. } => {
                self.eval_expr(operand, Mode::Observe);
                EvalValue::copy()
            }
            ExprKind::Binary { lhs, rhs, .. } => {
                self.eval_expr(lhs, Mode::Observe);
                self.eval_expr(rhs, Mode::Observe);
                EvalValue::copy()
            }
            ExprKind::Range { start, end, .. } => {
                self.eval_expr(start, Mode::Observe);
                self.eval_expr(end, Mode::Observe);
                EvalValue::copy()
            }
            ExprKind::Assign { op, target, value } => {
                let evaluated = self.eval_expr(value, Mode::Transfer);
                self.apply_assignment(op, target, &evaluated);
                evaluated
            }
            ExprKind::Call { callee, args } => self.eval_call(callee, args),
            ExprKind::Member { base, member } => {
                let base_value = self.eval_expr(base, Mode::Observe);
                if !self.expr_may_own(expr.span) {
                    return EvalValue::copy();
                }
                if let Some(fields) = &base_value.fields {
                    match fields.get(&member.name) {
                        Some(BindingState::Live(provenance)) => {
                            if mode == Mode::Transfer && *provenance == Provenance::Owned {
                                self.mark_field_moved(base, member);
                            }
                            return EvalValue::with_provenance(*provenance);
                        }
                        Some(BindingState::Dead) => {
                            self.errors.push(SemanticError::use_of_moved(
                                member.name.clone(),
                                member.span,
                            ));
                            return EvalValue::owned();
                        }
                        None => return EvalValue::copy(),
                    }
                }
                // The base's field provenance is unknown (a nested place
                // or a value from a call): conservatively Owned, and a
                // transfer moves the root binding whole.
                if mode == Mode::Transfer {
                    self.mark_root_dead(base);
                }
                EvalValue::owned()
            }
            ExprKind::Index { base, index } => {
                let base_value = self.eval_expr(base, Mode::Observe);
                self.eval_expr(index, Mode::Observe);
                if !self.expr_may_own(expr.span) {
                    return EvalValue::copy();
                }
                if let ExprKind::Ident(ident) = &base.kind {
                    if let Some(symbol) = self.semantic.resolve(ident.span) {
                        if let Some(State::Array(state)) = self.bindings.get(&symbol).cloned() {
                            return match state {
                                BindingState::Dead => {
                                    self.errors.push(SemanticError::use_of_moved(
                                        ident.name.clone(),
                                        ident.span,
                                    ));
                                    EvalValue::owned()
                                }
                                BindingState::Live(Provenance::Immutable) => EvalValue::immutable(),
                                BindingState::Live(Provenance::Owned) => {
                                    if mode == Mode::Transfer {
                                        self.bindings
                                            .insert(symbol, State::Array(BindingState::Dead));
                                    }
                                    EvalValue::owned()
                                }
                            };
                        }
                    }
                }
                let _ = base_value;
                // Conservative nested base.
                if mode == Mode::Transfer {
                    self.mark_root_dead(base);
                }
                EvalValue::owned()
            }
            ExprKind::StructLit { fields, .. } => {
                let mut provenances: FieldStates = HashMap::new();
                for field in fields {
                    let value = self.eval_expr(&field.value, Mode::Transfer);
                    provenances.insert(
                        field.name.name.clone(),
                        BindingState::Live(value.provenance),
                    );
                }
                EvalValue::struct_value(provenances)
            }
            ExprKind::ArrayLit(elems) => {
                let mut owned = false;
                for elem in elems {
                    let value = self.eval_expr(elem, Mode::Transfer);
                    if value.provenance == Provenance::Owned {
                        owned = true;
                    }
                }
                EvalValue::with_provenance(if owned {
                    Provenance::Owned
                } else {
                    Provenance::Immutable
                })
            }
            ExprKind::Group(inner) => self.eval_expr(inner, mode),
        }
    }

    /// Evaluates a name reference. `const` bindings always copy their
    /// value (each use is an independent immutable value); ordinary
    /// bindings move an Owned value in transfer mode and borrow it in
    /// observe mode.
    fn eval_ident(&mut self, ident: &Ident, mode: Mode) -> EvalValue {
        let Some(symbol) = self.semantic.resolve(ident.span) else {
            return EvalValue::copy();
        };
        if self
            .semantic
            .symbols()
            .get(symbol)
            .is_some_and(|symbol| symbol.kind == SymbolKind::Const)
        {
            return self.copy_const(symbol);
        }
        match self.bindings.get(&symbol).cloned() {
            Some(State::Str(state)) => self.transfer_str(symbol, ident, state, mode),
            Some(State::Struct { fields, dead }) => {
                if dead {
                    self.errors
                        .push(SemanticError::use_of_moved(ident.name.clone(), ident.span));
                    return EvalValue::owned();
                }
                // A struct is Copy iff it holds no Owned value; only an
                // Owned-containing struct moves as a whole in transfer
                // mode. All-Immutable structs copy and stay usable.
                let has_owned = fields.values().any(|state| {
                    matches!(
                        state,
                        BindingState::Live(Provenance::Owned) | BindingState::Dead
                    )
                });
                if mode == Mode::Transfer && has_owned {
                    // Order-independent: fields is a HashMap, so scan in
                    // sorted name order to keep the diagnostic stable.
                    let mut names: Vec<&String> = fields
                        .iter()
                        .filter_map(|(name, state)| {
                            matches!(state, BindingState::Dead).then_some(name)
                        })
                        .collect();
                    names.sort();
                    if let Some(field) = names.first() {
                        self.errors.push(SemanticError::use_of_moved_detail(
                            ident.name.clone(),
                            ident.span,
                            format!("cannot move `{}`: field `{field}` was moved", ident.name),
                        ));
                        return EvalValue::owned();
                    }
                    let all_dead = fields
                        .keys()
                        .map(|name| (name.clone(), BindingState::Dead))
                        .collect();
                    self.bindings.insert(
                        symbol,
                        State::Struct {
                            fields: all_dead,
                            dead: true,
                        },
                    );
                }
                EvalValue::struct_value(fields)
            }
            Some(State::Array(state)) => match state {
                BindingState::Live(Provenance::Immutable) => EvalValue::immutable(),
                BindingState::Live(Provenance::Owned) => {
                    if mode == Mode::Transfer {
                        self.bindings
                            .insert(symbol, State::Array(BindingState::Dead));
                    }
                    EvalValue::owned()
                }
                BindingState::Dead => {
                    self.errors
                        .push(SemanticError::use_of_moved(ident.name.clone(), ident.span));
                    EvalValue::owned()
                }
            },
            None => EvalValue::copy(),
        }
    }

    /// Transfers (or observes) a Str binding's value.
    fn transfer_str(
        &mut self,
        symbol: SymbolId,
        ident: &Ident,
        state: BindingState,
        mode: Mode,
    ) -> EvalValue {
        match state {
            BindingState::Live(Provenance::Immutable) => EvalValue::immutable(),
            BindingState::Live(Provenance::Owned) => {
                if mode == Mode::Transfer {
                    self.bindings.insert(symbol, State::Str(BindingState::Dead));
                }
                EvalValue::owned()
            }
            BindingState::Dead => {
                self.errors
                    .push(SemanticError::use_of_moved(ident.name.clone(), ident.span));
                EvalValue::owned()
            }
        }
    }

    /// Reads a `const` binding: its value is inlined at every use, so
    /// reading copies it and never moves it.
    fn copy_const(&self, symbol: SymbolId) -> EvalValue {
        match self.bindings.get(&symbol) {
            Some(State::Str(BindingState::Live(provenance)))
            | Some(State::Array(BindingState::Live(provenance))) => {
                EvalValue::with_provenance(*provenance)
            }
            Some(State::Struct {
                fields,
                dead: false,
            }) => EvalValue::struct_value(
                fields
                    .iter()
                    .map(|(name, state)| (name.clone(), live_or_immutable(*state)))
                    .collect(),
            ),
            _ => EvalValue::immutable(),
        }
    }

    /// Marks the tracked field `member` of the base binding dead (a
    /// per-field move). The base must be a plain identifier (or a group of
    /// one) whose binding is a tracked struct.
    fn mark_field_moved(&mut self, base: &Expr, member: &Ident) {
        let Some(root) = self.root_ident(base) else {
            return;
        };
        let Some(symbol) = self.semantic.resolve(root.span) else {
            return;
        };
        if let Some(State::Struct {
            fields,
            dead: false,
        }) = self.bindings.get_mut(&symbol)
        {
            fields.insert(member.name.clone(), BindingState::Dead);
        }
    }

    /// Marks the root binding of a place expression dead (a conservative
    /// whole-value move for nested places whose field liveness is not
    /// tracked).
    fn mark_root_dead(&mut self, place: &Expr) {
        let Some(root) = self.root_ident(place) else {
            return;
        };
        let Some(symbol) = self.semantic.resolve(root.span) else {
            return;
        };
        match self.bindings.get_mut(&symbol) {
            Some(State::Str(state)) => *state = BindingState::Dead,
            Some(State::Array(state)) => *state = BindingState::Dead,
            Some(State::Struct { fields, dead }) => {
                *dead = true;
                for state in fields.values_mut() {
                    *state = BindingState::Dead;
                }
            }
            None => {}
        }
    }

    /// The root identifier of a place expression (through member/index
    /// selections and grouping).
    fn root_ident<'e>(&self, expr: &'e Expr) -> Option<&'e Ident> {
        match &expr.kind {
            ExprKind::Ident(ident) => Some(ident),
            ExprKind::Member { base, .. } => self.root_ident(base),
            ExprKind::Index { base, .. } => self.root_ident(base),
            ExprKind::Group(inner) => self.root_ident(inner),
            _ => None,
        }
    }

    // ------------------------------------------------------------------
    // Assignment
    // ------------------------------------------------------------------

    /// Applies an assignment: the value has already been evaluated in
    /// transfer mode; update the target's tracked state.
    fn apply_assignment(&mut self, op: &AssignOp, target: &Expr, value: &EvalValue) {
        if *op != AssignOp::Assign {
            // Compound assignment is numeric-only; no ownership effect.
            self.eval_expr(target, Mode::Observe);
            return;
        }
        match &target.kind {
            ExprKind::Ident(ident) => {
                if let Some(symbol) = self.semantic.resolve(ident.span) {
                    // Rebind the target from the value; assigning to a
                    // dead binding resurrects it.
                    if let Some(ty) = self.types.symbol_type(symbol) {
                        self.bindings.remove(&symbol);
                        self.bind_name_with_type(symbol, ty, value);
                    }
                }
            }
            ExprKind::Member { base, member } => {
                let base_value = self.eval_expr(base, Mode::Observe);
                if !self.expr_may_own(target.span) {
                    return;
                }
                if let ExprKind::Ident(ident) = &base.kind {
                    if let Some(symbol) = self.semantic.resolve(ident.span) {
                        match self.bindings.get_mut(&symbol) {
                            Some(State::Struct {
                                fields,
                                dead: false,
                            }) => {
                                fields.insert(
                                    member.name.clone(),
                                    BindingState::Live(value.provenance),
                                );
                                return;
                            }
                            Some(State::Struct { dead: true, .. }) => {
                                self.errors.push(SemanticError::use_of_moved(
                                    ident.name.clone(),
                                    ident.span,
                                ));
                                return;
                            }
                            _ => {}
                        }
                    }
                }
                let _ = base_value;
                // Conservative nested target: an Owned value stored
                // through a nested path makes the root value Owned.
                if value.provenance == Provenance::Owned {
                    self.mark_root_owned(base);
                }
            }
            ExprKind::Index { base, index } => {
                let base_value = self.eval_expr(base, Mode::Observe);
                self.eval_expr(index, Mode::Observe);
                if !self.expr_may_own(target.span) {
                    return;
                }
                if let ExprKind::Ident(ident) = &base.kind {
                    if let Some(symbol) = self.semantic.resolve(ident.span) {
                        if let Some(State::Array(state)) = self.bindings.get_mut(&symbol) {
                            if matches!(state, BindingState::Dead) {
                                self.errors.push(SemanticError::use_of_moved(
                                    ident.name.clone(),
                                    ident.span,
                                ));
                            } else if value.provenance == Provenance::Owned {
                                *state = BindingState::Live(Provenance::Owned);
                            }
                            return;
                        }
                    }
                }
                let _ = base_value;
                if value.provenance == Provenance::Owned {
                    self.mark_root_owned(base);
                }
            }
            _ => {
                // Defensive: the parser rejects non-place targets.
                self.eval_expr(target, Mode::Observe);
            }
        }
    }

    /// Marks the root binding of a place expression as holding an Owned
    /// value (conservative handling of assignments through nested paths
    /// whose field liveness is not tracked).
    fn mark_root_owned(&mut self, place: &Expr) {
        let Some(root) = self.root_ident(place) else {
            return;
        };
        let Some(symbol) = self.semantic.resolve(root.span) else {
            return;
        };
        match self.bindings.get_mut(&symbol) {
            Some(State::Str(state)) => *state = BindingState::Live(Provenance::Owned),
            Some(State::Array(state)) => *state = BindingState::Live(Provenance::Owned),
            Some(State::Struct {
                fields,
                dead: false,
            }) => {
                for state in fields.values_mut() {
                    *state = BindingState::Live(Provenance::Owned);
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Calls
    // ------------------------------------------------------------------

    /// Evaluates a call: intrinsic arguments follow the intrinsic's
    /// convention (read/mutate borrows, consume for `rt_str_free`); user
    /// function arguments move. The result's provenance comes from the
    /// callee's computed result (or Owned when unknown).
    fn eval_call(&mut self, callee: &Expr, args: &[Expr]) -> EvalValue {
        // Intrinsics are identified by name.
        if let ExprKind::Ident(ident) = &callee.kind {
            if let Some(intrinsic) = intrinsics::by_name(&ident.name) {
                return self.eval_intrinsic(intrinsic, args);
            }
        }
        // User function.
        self.eval_expr(callee, Mode::Observe);
        for arg in args {
            self.eval_expr(arg, Mode::Transfer);
        }
        let result = match &callee.kind {
            ExprKind::Ident(ident) => self
                .semantic
                .resolve(ident.span)
                .and_then(|symbol| self.fn_results.get(&symbol).cloned()),
            _ => None,
        };
        match result {
            Some(result) => match result.provenance {
                Some(Provenance::Owned) => match result.fields {
                    Some(fields) => EvalValue::struct_value(
                        fields
                            .into_iter()
                            .map(|(name, provenance)| (name, BindingState::Live(provenance)))
                            .collect(),
                    ),
                    None => EvalValue::owned(),
                },
                Some(Provenance::Immutable) => match result.fields {
                    Some(fields) => EvalValue::struct_value(
                        fields
                            .into_iter()
                            .map(|(name, provenance)| (name, BindingState::Live(provenance)))
                            .collect(),
                    ),
                    None => EvalValue::immutable(),
                },
                None => EvalValue::copy(),
            },
            // Unknown callee result (not yet analyzed): conservative Owned.
            None => EvalValue::owned(),
        }
    }

    /// Evaluates an intrinsic call following its argument convention.
    fn eval_intrinsic(&mut self, intrinsic: &'static Intrinsic, args: &[Expr]) -> EvalValue {
        let is_free = intrinsic.name == "rt_str_free";
        let is_set_byte = intrinsic.name == "rt_str_set_byte";
        for (index, arg) in args.iter().enumerate() {
            let is_str_arg = matches!(intrinsic.params.get(index), Some(IntrinsicType::Str));
            if !is_str_arg {
                self.eval_expr(arg, Mode::Observe);
                continue;
            }
            if is_free {
                // Consume: the string is moved (the blob is destroyed).
                self.eval_expr(arg, Mode::Transfer);
            } else if is_set_byte {
                // Mutate borrow: the value must be Owned (E-S11), and it
                // stays live.
                let value = self.eval_expr(arg, Mode::Observe);
                if value.provenance == Provenance::Immutable {
                    self.errors.push(self.immutable_mutation_error(arg));
                }
            } else {
                // Read borrow.
                self.eval_expr(arg, Mode::Observe);
            }
        }
        if intrinsic.result == IntrinsicType::Str {
            // Only rt_str_alloc produces a string; it is always owned.
            EvalValue::owned()
        } else {
            EvalValue::copy()
        }
    }

    /// Builds the E-S11 diagnostic for a mutating-immutable-string error on
    /// `arg`, naming the argument when it is a plain name.
    fn immutable_mutation_error(&self, arg: &Expr) -> SemanticError {
        match &arg.kind {
            ExprKind::Ident(ident) => {
                SemanticError::mutating_immutable_string(ident.name.clone(), ident.span)
            }
            ExprKind::Member { member, .. } => {
                SemanticError::mutating_immutable_string(member.name.clone(), member.span)
            }
            _ => SemanticError::mutating_immutable_string_detail(
                arg.span,
                "cannot mutate an immutable string".to_string(),
            ),
        }
    }

    // ------------------------------------------------------------------
    // Type helpers
    // ------------------------------------------------------------------

    /// Whether the expression's recorded type may (transitively) own heap
    /// storage.
    fn expr_may_own(&self, span: Span) -> bool {
        self.types
            .expr_type_exact(span)
            .is_some_and(|ty| self.may_own(ty))
    }

    /// The ownership-relevant shape of a type.
    fn type_shape(&self, ty: TypeId) -> Shape {
        match self.types.types().kind(ty) {
            Some(TypeKind::Str) => Shape::Str,
            Some(TypeKind::Struct(id)) => {
                let has_tracked = self
                    .types
                    .types()
                    .struct_info(*id)
                    .is_some_and(|info| info.fields.iter().any(|f| self.may_own(f.ty)));
                if has_tracked {
                    Shape::Struct
                } else {
                    Shape::Copy
                }
            }
            Some(TypeKind::Array { elem, .. }) if self.may_own(*elem) => Shape::Array,
            _ => Shape::Copy,
        }
    }

    /// The names of the struct's tracked (Str-containing) fields.
    fn tracked_struct_fields(&self, ty: TypeId) -> impl Iterator<Item = String> + '_ {
        let info = match self.types.types().kind(ty) {
            Some(TypeKind::Struct(id)) => self.types.types().struct_info(*id),
            _ => None,
        };
        info.into_iter().flat_map(|info| {
            info.fields
                .iter()
                .filter(|field| self.may_own(field.ty))
                .map(|field| field.name.clone())
        })
    }

    /// Whether a type may (transitively) own heap storage.
    fn may_own(&self, ty: TypeId) -> bool {
        match self.types.types().kind(ty) {
            Some(TypeKind::Str) => true,
            Some(TypeKind::Array { elem, .. }) => self.may_own(*elem),
            Some(TypeKind::Struct(id)) => self
                .types
                .types()
                .struct_info(*id)
                .is_some_and(|info| info.fields.iter().any(|field| self.may_own(field.ty))),
            _ => false,
        }
    }
}

/// Maps a Dead field state to a live-immutable state (defensive: a Dead
/// state from an invalid move never legitimately reaches a fresh binding).
fn live_or_immutable(state: BindingState) -> BindingState {
    match state {
        BindingState::Live(provenance) => BindingState::Live(provenance),
        BindingState::Dead => BindingState::Live(Provenance::Immutable),
    }
}
