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

/// The borrow state of one root binding (session 16): whether references
/// to it are currently live, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum BorrowState {
    /// No live references.
    #[default]
    None,
    /// `count` live shared (`&T`) borrows.
    Shared(u32),
    /// One live exclusive (`&mut T`) borrow.
    Exclusive,
}

/// A view through a reference: the binding the reference borrows, whether
/// it is mutable, and whether the borrow was already recorded in the
/// borrow-state table (fresh borrows are recorded when they reach a
/// binding; copies and transfers arrive pre-counted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BorrowView {
    /// The borrowed root binding; `None` for caller-provided borrows (a
    /// reference parameter or an unknown-source call result) whose source
    /// is outside this function.
    source: Option<SymbolId>,
    /// Whether the reference is mutable (`&mut`).
    mutable: bool,
    /// Whether the borrow was already counted: `true` for transfers and
    /// deref views (never re-counted), `false` for fresh borrows and
    /// copies (counted when they reach a binding).
    counted: bool,
}

/// The tracked state of one binding.
#[derive(Debug, Clone)]
enum State {
    /// A `Str` value.
    Str(BindingState),
    /// A reference value (session 16): the binding it borrows (`None` for
    /// caller-provided borrows) and whether it is mutable. `&T` copies;
    /// `&mut T` moves (the binding dies with its source cleared).
    Ref {
        /// The borrowed root; `None` for caller borrows or after a move.
        source: Option<SymbolId>,
        /// Whether the reference is mutable (`&mut`).
        mutable: bool,
        /// Whether the reference was moved away.
        dead: bool,
    },
    /// A struct value: per-field states for tracked fields plus whether
    /// the whole struct was moved (any field read then errors, even for
    /// copy-typed fields), plus the live reference fields (session 16).
    Struct {
        /// Per-field liveness for tracked (`Str`/reference-containing)
        /// fields.
        fields: FieldStates,
        /// The whole struct was moved: every field is inaccessible.
        dead: bool,
        /// The struct's reference-typed fields: each live reference the
        /// struct holds, released when the struct dies.
        ref_fields: HashMap<String, BorrowView>,
    },
    /// An array value: whole-array state (per-element liveness is out of
    /// scope; reading an Owned array's element in a transfer position
    /// moves the whole array).
    Array(BindingState),
    /// An enum value (session 19): the payload's state, tracked only when
    /// the enum's payload type may own (a `Str`, a reference, or an
    /// aggregate containing them). A unit-only enum is Copy and is never
    /// tracked. Matching a data-carrying variant with a payload binding
    /// transfers the payload out of the enum (the binding dies).
    Enum {
        /// The payload's liveness.
        state: BindingState,
        /// The reference-typed borrows carried by the payload (a struct
        /// payload's reference fields), released when the enum dies.
        ref_borrows: Vec<(String, BorrowView)>,
    },
}

/// The evaluated value of an expression: its provenance plus, for struct
/// values, the per-field liveness of tracked fields, and (session 16) the
/// reference view it carries when it flows through (or out of) a
/// reference.
#[derive(Debug, Clone)]
struct EvalValue {
    provenance: Provenance,
    /// Per-field liveness for struct values (tracked fields only).
    fields: Option<FieldStates>,
    /// The reference view this value carries, when it is a borrow of (or
    /// a read through) a reference.
    view: Option<BorrowView>,
    /// The reference-typed fields of a struct value, released/transferred
    /// with the value.
    ref_borrows: Vec<(String, BorrowView)>,
}

impl EvalValue {
    fn copy() -> Self {
        Self {
            provenance: Provenance::Immutable,
            fields: None,
            view: None,
            ref_borrows: Vec::new(),
        }
    }

    fn immutable() -> Self {
        Self {
            provenance: Provenance::Immutable,
            fields: None,
            view: None,
            ref_borrows: Vec::new(),
        }
    }

    fn owned() -> Self {
        Self {
            provenance: Provenance::Owned,
            fields: None,
            view: None,
            ref_borrows: Vec::new(),
        }
    }

    fn with_provenance(provenance: Provenance) -> Self {
        Self {
            provenance,
            fields: None,
            view: None,
            ref_borrows: Vec::new(),
        }
    }

    /// A borrow (or deref) view: through a shared reference the value is
    /// Immutable (a read view); through an exclusive one it is Owned (the
    /// exclusive borrow, which moves with the value).
    fn borrow_view(source: Option<SymbolId>, mutable: bool, counted: bool) -> Self {
        Self {
            provenance: if mutable {
                Provenance::Owned
            } else {
                Provenance::Immutable
            },
            fields: None,
            view: Some(BorrowView {
                source,
                mutable,
                counted,
            }),
            ref_borrows: Vec::new(),
        }
    }

    /// A struct value: Owned when any tracked field is live-owned, with
    /// its reference-typed fields carried alongside.
    fn struct_value(fields: FieldStates, ref_borrows: Vec<(String, BorrowView)>) -> Self {
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
            view: None,
            ref_borrows,
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
/// `Str`-typed result is Owned, the per-field provenances of a struct
/// result, and (session 16) whether the function returns a reference and
/// which parameter it derives from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FnResult {
    /// `Some(Owned)` when any `return` value is Owned, `Some(Immutable)`
    /// when every `return` is Immutable, `None` when the function never
    /// returns a `Str`/aggregate value.
    provenance: Option<Provenance>,
    /// Per-field provenances of a struct result (tracked fields only).
    fields: Option<HashMap<String, Provenance>>,
    /// The reference this function returns, when it returns one: its
    /// mutability and the parameter index it derives from (`None` when the
    /// source is not a single identifiable parameter — the conservative
    /// multi-source case).
    ref_result: Option<RefResult>,
}

/// A function's reference result: which parameter's borrow it returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RefResult {
    /// Whether the returned reference is mutable (`&mut`).
    mutable: bool,
    /// The parameter index the reference derives from; `None` for
    /// unknown/multi-source returns.
    param: Option<usize>,
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
        borrows: HashMap::new(),
        scopes: Vec::new(),
        param_indices: HashMap::new(),
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
    /// A reference value (session 16).
    Ref,
    /// A struct with at least one tracked (Str/reference-containing)
    /// field.
    Struct,
    /// An array whose element type may own.
    Array,
    /// An enum with a data-carrying variant whose payload type may own
    /// (session 19): the payload moves with the value.
    Enum,
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
    /// The borrow state of every borrowed root binding (session 16).
    borrows: HashMap<SymbolId, BorrowState>,
    /// The symbols declared in the current scope chain (innermost last),
    /// so borrows are released at scope exit.
    scopes: Vec<Vec<SymbolId>>,
    /// Parameter index by symbol, for reference-return tracking.
    param_indices: HashMap<SymbolId, usize>,
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
            self.borrows.clear();
            self.scopes.clear();
            self.param_indices.clear();
            self.errors.clear();
            self.walk_module();
            if self.fn_results == before {
                break;
            }
        }
    }

    /// Indexes every declaration by its name span so bindings can be
    /// registered and looked up during the walk, including or-pattern
    /// binding aliases (session 27): every occurrence of an or-pattern
    /// binding after its first resolves to the one logical binding.
    fn build_decl_spans(&mut self) {
        for symbol in self.semantic.symbols().iter() {
            self.decl_spans.insert(symbol.span.start(), symbol.id);
        }
        for (span, symbol) in self.semantic.binding_aliases() {
            self.decl_spans.insert(span.start(), *symbol);
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
                // Struct and enum declarations are types, not values: the
                // ownership walk has nothing to analyze in them.
                ItemKind::Struct(_) | ItemKind::Enum(_) => {}
                ItemKind::Module(_) | ItemKind::Use(_) => {
                    // Handled during module discovery, not ownership analysis.
                }
                ItemKind::Pub(pub_item) => {
                    // Recurse into the pub-qualified inner item.
                    match &pub_item.item.kind {
                        ItemKind::Struct(_) | ItemKind::Enum(_) => {}
                        ItemKind::Module(_) | ItemKind::Use(_) | ItemKind::Pub(_) => {}
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
        // Parameters live in a function-level scope (released at function
        // end); reference parameters are caller borrows (source `None`),
        // so their release is a no-op.
        self.scopes.push(Vec::new());
        for (index, param) in f.params.iter().enumerate() {
            if let Some(symbol) = self.symbol_of(&param.name) {
                self.param_indices.insert(symbol, index);
                self.bind_param(symbol);
            }
        }
        let saved = std::mem::take(&mut self.result);
        self.walk_block(&f.body);
        let result = std::mem::take(&mut self.result);
        self.result = saved;
        if let Some(scope) = self.scopes.pop() {
            for symbol in scope {
                self.release_binding(symbol);
            }
        }
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
            Shape::Ref => {
                let mutable = matches!(
                    self.types.types().kind(ty),
                    Some(TypeKind::Ref { mutable: true, .. })
                );
                self.bindings.insert(
                    symbol,
                    State::Ref {
                        source: None,
                        mutable,
                        dead: false,
                    },
                );
            }
            Shape::Struct => {
                let fields = self
                    .tracked_struct_fields(ty)
                    .map(|name| (name, BindingState::Live(Provenance::Owned)))
                    .collect();
                let ref_fields = self
                    .tracked_ref_fields(ty)
                    .map(|name| {
                        let mutable = self.ref_field_mutable(ty, &name);
                        (
                            name,
                            BorrowView {
                                source: None,
                                mutable,
                                counted: true,
                            },
                        )
                    })
                    .collect();
                self.bindings.insert(
                    symbol,
                    State::Struct {
                        fields,
                        dead: false,
                        ref_fields,
                    },
                );
            }
            Shape::Array => {
                self.bindings
                    .insert(symbol, State::Array(BindingState::Live(Provenance::Owned)));
            }
            Shape::Enum => {
                self.bindings.insert(
                    symbol,
                    State::Enum {
                        state: BindingState::Live(Provenance::Owned),
                        ref_borrows: Vec::new(),
                    },
                );
            }
            Shape::Copy => {}
        }
        self.register_scope(symbol);
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
    /// provenance; reference bindings take the value's borrow view (a
    /// fresh or copied borrow is counted; a transfer is not).
    fn bind_name_with_type(&mut self, symbol: SymbolId, ty: TypeId, value: &EvalValue) {
        // Reassigning an existing binding drops its old value's borrows.
        if self.bindings.contains_key(&symbol) {
            self.release_binding(symbol);
        }
        match self.type_shape(ty) {
            Shape::Str => {
                self.bindings
                    .insert(symbol, State::Str(BindingState::Live(value.provenance)));
            }
            Shape::Ref => {
                let mutable = matches!(
                    self.types.types().kind(ty),
                    Some(TypeKind::Ref { mutable: true, .. })
                );
                let view = value.view;
                if let Some(view) = view {
                    if let Some(source) = view.source {
                        if !view.counted {
                            self.borrow_root(source, view.mutable);
                        }
                    }
                }
                self.bindings.insert(
                    symbol,
                    State::Ref {
                        source: view.and_then(|view| view.source),
                        mutable,
                        dead: false,
                    },
                );
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
                let mut ref_fields: HashMap<String, BorrowView> = HashMap::new();
                for (field_name, view) in &value.ref_borrows {
                    if let Some(source) = view.source {
                        if !view.counted {
                            self.borrow_root(source, view.mutable);
                        }
                    }
                    ref_fields.insert(
                        field_name.clone(),
                        BorrowView {
                            source: view.source,
                            mutable: view.mutable,
                            counted: true,
                        },
                    );
                }
                self.bindings.insert(
                    symbol,
                    State::Struct {
                        fields,
                        dead: false,
                        ref_fields,
                    },
                );
            }
            Shape::Array => {
                self.bindings
                    .insert(symbol, State::Array(BindingState::Live(value.provenance)));
            }
            Shape::Enum => {
                // The payload's borrows (a struct payload's reference
                // fields) transfer with the value; fresh borrows are
                // counted so they are released when the enum dies.
                for (_, view) in &value.ref_borrows {
                    if let Some(source) = view.source {
                        if !view.counted {
                            self.borrow_root(source, view.mutable);
                        }
                    }
                }
                self.bindings.insert(
                    symbol,
                    State::Enum {
                        state: BindingState::Live(value.provenance),
                        ref_borrows: value.ref_borrows.clone(),
                    },
                );
            }
            Shape::Copy => {}
        }
        self.register_scope(symbol);
    }

    /// Records a borrow of `source` in the borrow-state table, checking
    /// the session-16 rules: the source must be live (E-S10), mutable for
    /// an exclusive borrow (E-S13), and not borrowed incompatibly (E-S12).
    /// Shared borrows increment; exclusive borrows require no other borrow.
    fn borrow_root(&mut self, source: SymbolId, mutable: bool) {
        let Some(symbol_info) = self.semantic.symbols().get(source) else {
            return;
        };
        let name = symbol_info.name.clone();
        let span = symbol_info.span;
        if matches!(
            self.bindings.get(&source),
            Some(State::Str(BindingState::Dead))
                | Some(State::Array(BindingState::Dead))
                | Some(State::Enum {
                    state: BindingState::Dead,
                    ..
                })
                | Some(State::Ref { dead: true, .. })
                | Some(State::Struct { dead: true, .. })
        ) {
            self.errors
                .push(SemanticError::use_of_moved(name.clone(), span));
            return;
        }
        let current = self.borrows.get(&source).copied().unwrap_or_default();
        if mutable {
            match current {
                BorrowState::None => {}
                _ => {
                    self.errors.push(SemanticError::borrow_conflict(
                        name.clone(),
                        span,
                        format!("cannot borrow `{name}` mutably: it is already borrowed"),
                    ));
                    return;
                }
            }
            let writable = self
                .semantic
                .symbols()
                .get(source)
                .is_some_and(|symbol| symbol.kind.is_mutable());
            if !writable {
                self.errors.push(SemanticError::invalid_borrow(
                    span,
                    format!("cannot mutably borrow `{name}`: it is not mutable"),
                ));
                return;
            }
            self.borrows.insert(source, BorrowState::Exclusive);
        } else {
            match current {
                BorrowState::None => {
                    self.borrows.insert(source, BorrowState::Shared(1));
                }
                BorrowState::Shared(count) => {
                    self.borrows.insert(source, BorrowState::Shared(count + 1));
                }
                BorrowState::Exclusive => {
                    self.errors.push(SemanticError::borrow_conflict(
                        name.clone(),
                        span,
                        format!("cannot borrow `{name}` immutably: it is mutably borrowed"),
                    ));
                }
            }
        }
    }

    fn walk_block(&mut self, block: &crate::ast::Block) {
        // A block is a scope: borrows declared inside it are released when
        // it exits, so a reference cannot outlive its declaring block.
        self.scopes.push(Vec::new());
        for stmt in &block.stmts {
            self.walk_stmt(stmt);
        }
        if let Some(scope) = self.scopes.pop() {
            for symbol in scope {
                self.release_binding(symbol);
            }
        }
    }

    /// Records `symbol` as declared in the current innermost scope, so its
    /// borrows are released at scope exit.
    fn register_scope(&mut self, symbol: SymbolId) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(symbol);
        }
    }

    /// Releases the borrows held by `symbol`'s binding (reference values
    /// and reference-typed struct fields).
    fn release_binding(&mut self, symbol: SymbolId) {
        let borrows = match self.bindings.get(&symbol).cloned() {
            Some(State::Ref {
                source, mutable, ..
            }) => source.map(|source| vec![(source, mutable)]),
            Some(State::Struct { ref_fields, .. }) => Some(
                ref_fields
                    .values()
                    .filter_map(|view| view.source.map(|source| (source, view.mutable)))
                    .collect(),
            ),
            Some(State::Enum { ref_borrows, .. }) => Some(
                ref_borrows
                    .iter()
                    .filter_map(|(_, view)| view.source.map(|source| (source, view.mutable)))
                    .collect(),
            ),
            _ => None,
        };
        if let Some(borrows) = borrows {
            for (source, mutable) in borrows {
                self.release_borrow(source, mutable);
            }
        }
    }

    /// Releases one borrow of `source` (decrementing a shared count,
    /// clearing an exclusive one).
    fn release_borrow(&mut self, source: SymbolId, mutable: bool) {
        let state = self.borrows.get(&source).copied().unwrap_or_default();
        let next = if mutable {
            BorrowState::None
        } else {
            match state {
                BorrowState::Shared(count) if count > 1 => BorrowState::Shared(count - 1),
                _ => BorrowState::None,
            }
        };
        self.borrows.insert(source, next);
    }

    fn walk_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Let(binding) => {
                let value = self.eval_expr(&binding.init, Mode::Transfer);
                // For destructuring patterns, bind each field individually.
                match &binding.pattern {
                    Some(crate::ast::Pattern::Tuple { elements, .. }) => {
                        for elem in elements {
                            if let crate::ast::Pattern::Binding(name) = elem {
                                self.bind(name.span, &EvalValue::copy());
                            }
                        }
                    }
                    Some(crate::ast::Pattern::Struct { fields, .. }) => {
                        for field in fields {
                            match &field.binding {
                                Some(crate::ast::Pattern::Binding(name)) => {
                                    self.bind(name.span, &EvalValue::copy());
                                }
                                None => {
                                    self.bind(field.name.span, &EvalValue::copy());
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {
                        self.bind(binding.name.span, &value);
                    }
                }
            }
            StmtKind::Const(binding) => {
                let value = self.eval_expr(&binding.init, Mode::Transfer);
                self.bind(binding.name.span, &value);
            }
            StmtKind::Return(Some(value)) => {
                let evaluated = self.eval_expr(value, Mode::Transfer);
                self.merge_result(value, &evaluated);
            }
            StmtKind::Return(None) | StmtKind::Continue => {}
            StmtKind::Break(value) => {
                if let Some(value) = value {
                    self.eval_expr(value, Mode::Transfer);
                }
            }
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
            StmtKind::Match(stmt) => self.walk_match(stmt),
            StmtKind::Expr(expr) => {
                self.eval_expr(expr, Mode::Observe);
            }
        }
    }

    /// Walks a `match` statement: the scrutinee is observed (matchable
    /// types — `Int`, `Bool`, and unit-only enums — never move), and each
    /// arm's body is walked as its own block scope. A top-level pattern
    /// binding copies the scrutinee value, so it binds as a copy; a
    /// data-carrying variant pattern (session 19) binds the payload by
    /// value — an Owned payload moves out of the scrutinee into the
    /// binding, leaving the scrutinee's payload consumed on every arm that
    /// binds one. Arms inherit the enclosing loop/function context (their
    /// bodies may `break`, `continue`, or `return`).
    fn walk_match(&mut self, stmt: &crate::ast::MatchStmt) {
        let scrutinee_value = self.eval_expr(&stmt.scrutinee, Mode::Observe);
        let mut moved_payload = false;
        for arm in &stmt.arms {
            match &arm.pattern {
                crate::ast::Pattern::Binding(name) => {
                    // A scalar copy: no ownership state is tracked, but the
                    // binding is registered so its scope release is
                    // correct.
                    self.bind(name.span, &EvalValue::copy());
                }
                crate::ast::Pattern::Or { alternatives, .. } => {
                    // Or-pattern alternatives share one binding per name;
                    // a payload binding in any alternative moves the
                    // scrutinee's payload, exactly like a plain payload
                    // pattern (session 27).
                    for alternative in alternatives {
                        if let crate::ast::Pattern::EnumVariant {
                            payload: Some(inner),
                            ..
                        } = alternative
                        {
                            let provenance =
                                self.payload_provenance(&stmt.scrutinee, &scrutinee_value);
                            if self.payload_binding_transfers(inner, provenance) {
                                moved_payload = true;
                            }
                            self.bind_payload_pattern(inner, provenance);
                        }
                    }
                }
                crate::ast::Pattern::EnumVariant {
                    payload: Some(inner),
                    ..
                } => {
                    let provenance = self.payload_provenance(&stmt.scrutinee, &scrutinee_value);
                    if self.payload_binding_transfers(inner, provenance) {
                        moved_payload = true;
                    }
                    self.bind_payload_pattern(inner, provenance);
                }
                crate::ast::Pattern::Tuple { elements, .. } => {
                    // Tuple pattern bindings: each element pattern binds
                    // its sub-values. For now, treat each binding as a
                    // copy (the tuple itself is observed, not consumed).
                    for elem in elements {
                        if let crate::ast::Pattern::Binding(name) = elem {
                            self.bind(name.span, &EvalValue::copy());
                        }
                    }
                }
                _ => {}
            }
            // A guard (session 27) reads the pattern's bindings in the
            // arm's scope, before the body.
            if let Some(guard) = &arm.guard {
                self.eval_expr(guard, Mode::Observe);
            }
            self.walk_block(&arm.body);
        }
        if moved_payload {
            // The payload moved out of the scrutinee on some arm: the
            // enum's payload is consumed (a use of the scrutinee after the
            // match is a use of a moved value). The consumption is at the
            // same granularity as the transfer rules: a member chain's
            // innermost field dies (partial move, like `let x = s.e;`),
            // an index read moves the whole array root, and an
            // identifier root dies whole.
            self.mark_scrutinee_consumed(&stmt.scrutinee);
        }
    }

    /// Marks the value consumed by an owned-payload match consumed, at
    /// the transfer-position granularity: the innermost field of a member
    /// chain, the whole array root of an index read, or the whole
    /// identifier root otherwise.
    fn mark_scrutinee_consumed(&mut self, scrutinee: &Expr) {
        match &scrutinee.kind {
            ExprKind::Member { base, member } => self.mark_field_moved(base, member),
            ExprKind::Index { base, .. } => self.mark_root_dead(base),
            _ => self.mark_root_dead(scrutinee),
        }
    }

    /// The provenance of a matched enum's payload: the tracked state of
    /// the scrutinee's binding when it is a tracked enum local, otherwise
    /// the fallback provenance the scrutinee evaluation produced.
    fn payload_provenance(&self, scrutinee: &Expr, fallback: &EvalValue) -> Provenance {
        let Some(root) = self.root_ident(scrutinee) else {
            return fallback.provenance;
        };
        let Some(symbol) = self.semantic.resolve(root.span) else {
            return fallback.provenance;
        };
        match self.bindings.get(&symbol) {
            Some(State::Enum {
                state: BindingState::Live(provenance),
                ..
            }) => *provenance,
            _ => fallback.provenance,
        }
    }

    /// Whether a payload pattern binds a value (a `name` binding or a
    /// nested variant's payload binding) whose payload provenance is
    /// Owned, so the payload moves out of the scrutinee. An or-pattern
    /// alternative that binds an owned payload moves it, like any other
    /// payload binding (session 27).
    fn payload_binding_transfers(
        &self,
        pattern: &crate::ast::Pattern,
        provenance: Provenance,
    ) -> bool {
        match pattern {
            crate::ast::Pattern::Binding(_) => provenance == Provenance::Owned,
            crate::ast::Pattern::EnumVariant {
                payload: Some(inner),
                ..
            } => self.payload_binding_transfers(inner, provenance),
            crate::ast::Pattern::Or { alternatives, .. } => alternatives
                .iter()
                .any(|alt| self.payload_binding_transfers(alt, provenance)),
            crate::ast::Pattern::Tuple { elements, .. } => elements
                .iter()
                .any(|e| self.payload_binding_transfers(e, provenance)),
            _ => false,
        }
    }

    /// Binds every payload pattern binding (`E::V(x)` → `x`) with the
    /// payload's provenance, recursively for nested variant patterns and
    /// or-pattern alternatives (session 27).
    fn bind_payload_pattern(&mut self, pattern: &crate::ast::Pattern, provenance: Provenance) {
        match pattern {
            crate::ast::Pattern::Binding(name) => {
                self.bind(name.span, &EvalValue::with_provenance(provenance));
            }
            crate::ast::Pattern::EnumVariant {
                payload: Some(inner),
                ..
            } => self.bind_payload_pattern(inner, provenance),
            crate::ast::Pattern::Or { alternatives, .. } => {
                for alternative in alternatives {
                    self.bind_payload_pattern(alternative, provenance);
                }
            }
            crate::ast::Pattern::Tuple { elements, .. } => {
                for elem in elements {
                    self.bind_payload_pattern(elem, provenance);
                }
            }
            _ => {}
        }
    }

    fn walk_if(&mut self, stmt: &IfStmt) {
        self.eval_expr(&stmt.cond, Mode::Observe);
        self.walk_block(&stmt.then_block);
        match &stmt.else_branch {
            Some(ElseBranch::If(nested)) => self.walk_if(nested),
            Some(ElseBranch::IfExpr(inner)) => self.walk_if_expr(inner),
            Some(ElseBranch::Block(block)) => self.walk_block(block),
            None => {}
        }
    }

    fn walk_if_expr(&mut self, expr: &crate::ast::IfExpr) {
        self.eval_expr(&expr.cond, Mode::Observe);
        self.walk_block(&expr.then_block);
        match &expr.else_branch {
            ElseBranch::IfExpr(inner) => self.walk_if_expr(inner),
            ElseBranch::Block(block) => self.walk_block(block),
            ElseBranch::If(nested) => self.walk_if(nested),
        }
    }

    /// Records a `return` value's provenance into the current function's
    /// result (a struct result's per-field provenances merge too).
    fn merge_result(&mut self, expr: &Expr, value: &EvalValue) {
        match value.provenance {
            Provenance::Owned => self.result.provenance = Some(Provenance::Owned),
            Provenance::Immutable => {
                if self.result.provenance.is_none() {
                    self.result.provenance = Some(Provenance::Immutable);
                }
            }
        }
        // A reference return: only caller borrows (source `None`) may
        // escape; returning a borrow of a function-local value is a
        // dangling reference (E-S14).
        if let Some(view) = value.view {
            if let Some(source) = view.source {
                let name = self
                    .semantic
                    .symbols()
                    .get(source)
                    .map(|symbol| symbol.name.clone())
                    .unwrap_or_else(|| "value".to_string());
                self.errors.push(SemanticError::dangling_reference(
                    expr.span,
                    format!(
                        "cannot return a reference to `{name}`: it would not outlive the function"
                    ),
                ));
            } else {
                // A caller borrow. Record which parameter it derives from
                // when the return expression is a direct reference
                // parameter; otherwise conservatively record no single
                // source.
                let param = match &expr.kind {
                    ExprKind::Ident(ident) => self
                        .semantic
                        .resolve(ident.span)
                        .and_then(|symbol| self.param_indices.get(&symbol).copied()),
                    ExprKind::Group(inner) => self.param_index_of_expr(inner),
                    _ => None,
                };
                let candidate = RefResult {
                    mutable: view.mutable,
                    param,
                };
                match self.result.ref_result {
                    None => self.result.ref_result = Some(candidate),
                    Some(existing) => {
                        // Multi-source or conflicting returns: keep the
                        // conservative unknown-source result.
                        if existing.mutable != candidate.mutable
                            || existing.param != candidate.param
                        {
                            self.result.ref_result = Some(RefResult {
                                mutable: existing.mutable || candidate.mutable,
                                param: None,
                            });
                        }
                    }
                }
            }
        }
        // Returning an aggregate that carries a reference to a local
        // value would dangle too.
        for (_, view) in &value.ref_borrows {
            if let Some(source) = view.source {
                self.errors.push(SemanticError::dangling_reference(
                    expr.span,
                    "cannot return a value containing a reference to a local value".to_string(),
                ));
                let _ = source;
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

    /// The parameter index of `expr` when it is a plain parameter
    /// reference (used for reference-return tracking through grouping).
    fn param_index_of_expr(&self, expr: &Expr) -> Option<usize> {
        match &expr.kind {
            ExprKind::Ident(ident) => self
                .semantic
                .resolve(ident.span)
                .and_then(|symbol| self.param_indices.get(&symbol).copied()),
            ExprKind::Group(inner) => self.param_index_of_expr(inner),
            _ => None,
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
            ExprKind::Borrow { mutable, operand } => self.eval_borrow(*mutable, operand),
            ExprKind::Deref { operand } => {
                // `*r`: the referent, as a read view through the
                // reference. The operand must be a live reference
                // binding; a deref through an exclusive reference is the
                // point of the borrow (no conflict), through a shared one
                // it is a read-only view.
                let value = self.eval_expr(operand, Mode::Observe);
                match value.view {
                    Some(view) => EvalValue::borrow_view(view.source, view.mutable, true),
                    // Defensive: the type checker rejects derefs of
                    // non-references, so this branch never runs in a
                    // clean pipeline.
                    None => EvalValue::immutable(),
                }
            }
            ExprKind::Try { operand } => self.eval_expr(operand, mode),
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
            ExprKind::Call {
                callee,
                args,
                type_args: _,
            } => self.eval_call(callee, args),
            ExprKind::Member { base, member } => {
                let base_value = self.eval_expr(base, Mode::Observe);
                if !self.expr_may_own(expr.span) {
                    return EvalValue::copy();
                }
                // A reference-typed field: reading it copies a shared
                // borrow or moves an exclusive one (per-field transfer).
                if let Some((_, view)) = base_value
                    .ref_borrows
                    .iter()
                    .find(|(name, _)| name == &member.name)
                {
                    return self.read_ref_field(base, member, *view, mode);
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
                                // The base `eval_expr` above already
                                // reported the dead array; reporting again
                                // would duplicate the diagnostic.
                                BindingState::Dead => EvalValue::owned(),
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
                let mut ref_borrows = Vec::new();
                for field in fields {
                    let value = self.eval_expr(&field.value, Mode::Transfer);
                    provenances.insert(
                        field.name.name.clone(),
                        BindingState::Live(value.provenance),
                    );
                    if let Some(view) = value.view {
                        if view.source.is_some() {
                            ref_borrows.push((field.name.name.clone(), view));
                        }
                    }
                }
                EvalValue::struct_value(provenances, ref_borrows)
            }
            // A unit variant reference is an immutable discriminant
            // constant (session 17). A data-carrying construction (session
            // 19) transfers its payload into the value: the enum's
            // provenance — and any reference-typed payload borrows —
            // follow the payload's.
            ExprKind::EnumVariant { payload, .. } => match payload {
                None => EvalValue::immutable(),
                Some(payload) => {
                    let value = self.eval_expr(payload, Mode::Transfer);
                    EvalValue {
                        provenance: value.provenance,
                        fields: None,
                        view: None,
                        ref_borrows: value.ref_borrows,
                    }
                }
            },
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
            ExprKind::IfExpr(inner) => {
                self.eval_expr(&inner.cond, Mode::Observe);
                self.walk_block(&inner.then_block);
                match &inner.else_branch {
                    ElseBranch::IfExpr(e) => self.walk_if_expr(e),
                    ElseBranch::Block(b) => self.walk_block(b),
                    ElseBranch::If(s) => self.walk_if(s),
                }
                EvalValue::copy()
            }
            ExprKind::Block(block) => {
                self.walk_block(block);
                EvalValue::copy()
            }
            ExprKind::Tuple(elems) => {
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
            ExprKind::TupleFieldAccess { base, .. } => {
                // Reading a tuple field observes the value without
                // consuming it.
                self.eval_expr(base, Mode::Observe);
                EvalValue::copy()
            }
            ExprKind::WhileExpr { cond, body, .. } => {
                self.eval_expr(cond, Mode::Observe);
                self.walk_block(body);
                EvalValue::copy()
            }
            ExprKind::LoopExpr { body, .. } => {
                self.walk_block(body);
                EvalValue::copy()
            }
            ExprKind::MatchExpr(m) => {
                self.eval_expr(&m.scrutinee, Mode::Observe);
                let mut moved_payload = false;
                for arm in &m.arms {
                    match &arm.pattern {
                        crate::ast::Pattern::Binding(name) => {
                            self.bind(name.span, &EvalValue::copy());
                        }
                        crate::ast::Pattern::Or { alternatives, .. } => {
                            for alternative in alternatives {
                                if let crate::ast::Pattern::EnumVariant {
                                    payload: Some(inner),
                                    ..
                                } = alternative
                                {
                                    let provenance =
                                        self.payload_provenance(&m.scrutinee, &EvalValue::copy());
                                    if self.payload_binding_transfers(inner, provenance) {
                                        moved_payload = true;
                                    }
                                    self.bind_payload_pattern(inner, provenance);
                                }
                            }
                        }
                        crate::ast::Pattern::EnumVariant {
                            payload: Some(inner),
                            ..
                        } => {
                            let provenance =
                                self.payload_provenance(&m.scrutinee, &EvalValue::copy());
                            if self.payload_binding_transfers(inner, provenance) {
                                moved_payload = true;
                            }
                            self.bind_payload_pattern(inner, provenance);
                        }
                        crate::ast::Pattern::Tuple { elements, .. } => {
                            for elem in elements {
                                if let crate::ast::Pattern::Binding(name) = elem {
                                    self.bind(name.span, &EvalValue::copy());
                                }
                            }
                        }
                        _ => {}
                    }
                    if let Some(guard) = &arm.guard {
                        self.eval_expr(guard, Mode::Observe);
                    }
                    self.eval_expr(&arm.body, Mode::Observe);
                }
                if moved_payload {
                    self.mark_scrutinee_consumed(&m.scrutinee);
                }
                EvalValue::copy()
            }
            ExprKind::Closure {
                params: _, body, ..
            } => {
                // Closures move captured values. For V1, evaluate the body
                // in observe mode to discover uses; captures are handled at
                // semantic analysis time.
                self.eval_expr(body, Mode::Observe);
                EvalValue::copy()
            }
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
        // A mutably-borrowed binding cannot be read or transferred at all;
        // reads of a shared-borrowed binding are fine (the transfer and
        // mutation paths add their own shared-borrow checks).
        if matches!(self.borrows.get(&symbol), Some(BorrowState::Exclusive)) {
            self.errors.push(SemanticError::borrow_conflict(
                ident.name.clone(),
                ident.span,
                format!("cannot use `{}`: it is mutably borrowed", ident.name),
            ));
        }
        match self.bindings.get(&symbol).cloned() {
            Some(State::Str(state)) => self.transfer_str(symbol, ident, state, mode),
            Some(State::Ref {
                source,
                mutable,
                dead: false,
            }) => {
                if mode == Mode::Transfer && mutable {
                    // Move-only: the exclusive borrow transfers and this
                    // binding dies (its source is cleared so release is a
                    // no-op).
                    self.bindings.insert(
                        symbol,
                        State::Ref {
                            source: None,
                            mutable,
                            dead: true,
                        },
                    );
                    EvalValue::borrow_view(source, mutable, true)
                } else {
                    // A shared reference (or an observed exclusive one)
                    // is a copy/read view; binding it counts a new borrow.
                    EvalValue::borrow_view(source, mutable, false)
                }
            }
            Some(State::Ref { dead: true, .. }) => {
                self.errors
                    .push(SemanticError::use_of_moved(ident.name.clone(), ident.span));
                EvalValue::borrow_view(None, false, true)
            }
            Some(State::Struct {
                fields,
                dead,
                ref_fields,
            }) => {
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
                let moving = mode == Mode::Transfer && has_owned;
                if moving {
                    if self.is_borrowed(symbol) {
                        self.errors.push(SemanticError::borrow_conflict(
                            ident.name.clone(),
                            ident.span,
                            format!("cannot move `{}`: it is borrowed", ident.name),
                        ));
                        return EvalValue::owned();
                    }
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
                    // The whole struct moves: its reference fields
                    // transfer to the value (the new binding keeps them).
                    let ref_borrows: Vec<(String, BorrowView)> = ref_fields
                        .iter()
                        .map(|(name, view)| (name.clone(), *view))
                        .collect();
                    self.bindings.insert(
                        symbol,
                        State::Struct {
                            fields: all_dead,
                            dead: true,
                            ref_fields: HashMap::new(),
                        },
                    );
                    EvalValue::struct_value(fields, ref_borrows)
                } else {
                    // Copy: shared reference fields are copied (each copy
                    // adds a live borrow); the binding keeps its own.
                    let mut ref_borrows = Vec::new();
                    for (name, view) in &ref_fields {
                        if !view.mutable {
                            if let Some(source) = view.source {
                                self.borrow_root(source, false);
                            }
                        }
                        ref_borrows.push((name.clone(), *view));
                    }
                    EvalValue::struct_value(fields, ref_borrows)
                }
            }
            Some(State::Array(state)) => match state {
                BindingState::Live(Provenance::Immutable) => EvalValue::immutable(),
                BindingState::Live(Provenance::Owned) => {
                    if mode == Mode::Transfer {
                        if self.is_borrowed(symbol) {
                            self.errors.push(SemanticError::borrow_conflict(
                                ident.name.clone(),
                                ident.span,
                                format!("cannot move `{}`: it is borrowed", ident.name),
                            ));
                            return EvalValue::owned();
                        }
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
            // An enum whose payload may own (session 19) moves like a
            // struct: an Owned payload transfers with the value; an
            // Immutable payload copies; a Dead payload is a use-after-move.
            Some(State::Enum { state, ref_borrows }) => match state {
                BindingState::Dead => {
                    self.errors
                        .push(SemanticError::use_of_moved(ident.name.clone(), ident.span));
                    EvalValue::owned()
                }
                BindingState::Live(Provenance::Immutable) => {
                    // Copy: shared payload borrows are re-counted (each
                    // copy adds a live borrow); the binding keeps its own.
                    let mut carried = Vec::new();
                    for (name, view) in &ref_borrows {
                        if !view.mutable {
                            if let Some(source) = view.source {
                                self.borrow_root(source, false);
                            }
                        }
                        carried.push((name.clone(), *view));
                    }
                    EvalValue {
                        provenance: Provenance::Immutable,
                        fields: None,
                        view: None,
                        ref_borrows: carried,
                    }
                }
                BindingState::Live(Provenance::Owned) => {
                    if mode == Mode::Transfer {
                        if self.is_borrowed(symbol) {
                            self.errors.push(SemanticError::borrow_conflict(
                                ident.name.clone(),
                                ident.span,
                                format!("cannot move `{}`: it is borrowed", ident.name),
                            ));
                            return EvalValue::owned();
                        }
                        // The whole enum moves: its payload borrows
                        // transfer to the value.
                        let carried: Vec<(String, BorrowView)> = ref_borrows
                            .iter()
                            .map(|(name, view)| (name.clone(), *view))
                            .collect();
                        self.bindings.insert(
                            symbol,
                            State::Enum {
                                state: BindingState::Dead,
                                ref_borrows: Vec::new(),
                            },
                        );
                        EvalValue {
                            provenance: Provenance::Owned,
                            fields: None,
                            view: None,
                            ref_borrows: carried,
                        }
                    } else {
                        EvalValue {
                            provenance: Provenance::Owned,
                            fields: None,
                            view: None,
                            ref_borrows: Vec::new(),
                        }
                    }
                }
            },
            None => EvalValue::copy(),
        }
    }

    /// Evaluates a borrow expression: `&place` (shared) or `&mut place`
    /// (exclusive). The borrow is recorded immediately (checked against
    /// the frozen rules: live source, no conflicting borrow, mutable
    /// source for `&mut`) and the returned view is pre-counted, so a
    /// subsequent binding records the same borrow without re-counting.
    fn eval_borrow(&mut self, mutable: bool, operand: &Expr) -> EvalValue {
        let Some(root) = self.root_ident(operand) else {
            // Not a local-rooted place: no tracked borrow. The type
            // checker rejects non-place and deref-rooted borrows, so this
            // is defensive.
            return EvalValue::borrow_view(None, mutable, true);
        };
        let Some(symbol) = self.semantic.resolve(root.span) else {
            return EvalValue::borrow_view(None, mutable, true);
        };
        let name = root.name.clone();
        let span = root.span;
        if self
            .semantic
            .symbols()
            .get(symbol)
            .is_some_and(|symbol| symbol.kind == SymbolKind::Const)
        {
            self.errors.push(SemanticError::invalid_borrow(
                span,
                format!("cannot borrow `{name}`: it is a constant"),
            ));
            return EvalValue::borrow_view(None, mutable, true);
        }
        if matches!(
            self.bindings.get(&symbol),
            Some(State::Str(BindingState::Dead))
                | Some(State::Array(BindingState::Dead))
                | Some(State::Enum {
                    state: BindingState::Dead,
                    ..
                })
                | Some(State::Ref { dead: true, .. })
                | Some(State::Struct { dead: true, .. })
        ) {
            self.errors
                .push(SemanticError::use_of_moved(name.clone(), span));
            return EvalValue::borrow_view(None, mutable, true);
        }
        let current = self.borrows.get(&symbol).copied().unwrap_or_default();
        if mutable {
            match current {
                BorrowState::None => {}
                _ => {
                    self.errors.push(SemanticError::borrow_conflict(
                        name.clone(),
                        span,
                        format!("cannot borrow `{name}` mutably: it is already borrowed"),
                    ));
                    return EvalValue::borrow_view(None, mutable, true);
                }
            }
            let writable = self
                .semantic
                .symbols()
                .get(symbol)
                .is_some_and(|symbol| symbol.kind.is_mutable());
            if !writable {
                self.errors.push(SemanticError::invalid_borrow(
                    span,
                    format!("cannot mutably borrow `{name}`: it is not mutable"),
                ));
                return EvalValue::borrow_view(None, mutable, true);
            }
            self.borrows.insert(symbol, BorrowState::Exclusive);
        } else {
            match current {
                BorrowState::None => {
                    self.borrows.insert(symbol, BorrowState::Shared(1));
                }
                BorrowState::Shared(count) => {
                    self.borrows.insert(symbol, BorrowState::Shared(count + 1));
                }
                BorrowState::Exclusive => {
                    self.errors.push(SemanticError::borrow_conflict(
                        name.clone(),
                        span,
                        format!("cannot borrow `{name}` immutably: it is mutably borrowed"),
                    ));
                    return EvalValue::borrow_view(None, mutable, true);
                }
            }
        }
        EvalValue::borrow_view(Some(symbol), mutable, true)
    }

    /// Reads a reference-typed struct field: a shared field copies (the
    /// struct keeps its borrow; the copy adds a live reference), an
    /// exclusive field moves out (the borrow transfers; the field becomes
    /// dead so the whole struct cannot be moved).
    fn read_ref_field(
        &mut self,
        base: &Expr,
        member: &Ident,
        view: BorrowView,
        mode: Mode,
    ) -> EvalValue {
        let _ = mode;
        let Some(root) = self.root_ident(base) else {
            return EvalValue::borrow_view(view.source, view.mutable, true);
        };
        let Some(symbol) = self.semantic.resolve(root.span) else {
            return EvalValue::borrow_view(view.source, view.mutable, true);
        };
        let mut found = None;
        if let Some(State::Struct {
            ref_fields,
            dead: false,
            ..
        }) = self.bindings.get_mut(&symbol)
        {
            if let Some(field_view) = ref_fields.get(&member.name).copied() {
                if field_view.mutable {
                    // Exclusive: moves out of the struct; the borrow
                    // transfers to the reader.
                    ref_fields.remove(&member.name);
                    found = Some(field_view);
                } else {
                    // Shared: the copy adds another live reference; the
                    // struct keeps its own.
                    found = Some(field_view);
                    if let Some(source) = field_view.source {
                        self.borrow_root(source, false);
                    }
                }
            }
        }
        match found {
            Some(field_view) => {
                if field_view.mutable {
                    if let Some(State::Struct { fields, .. }) = self.bindings.get_mut(&symbol) {
                        fields.insert(member.name.clone(), BindingState::Dead);
                    }
                }
                EvalValue::borrow_view(field_view.source, field_view.mutable, true)
            }
            None => {
                // The field was already moved out (or the base is not a
                // tracked struct).
                self.errors.push(SemanticError::use_of_moved(
                    member.name.clone(),
                    member.span,
                ));
                EvalValue::borrow_view(None, view.mutable, true)
            }
        }
    }

    /// Whether `symbol` is currently borrowed (shared or exclusive).
    fn is_borrowed(&self, symbol: SymbolId) -> bool {
        !matches!(self.borrows.get(&symbol), None | Some(BorrowState::None))
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
                    // Consuming/moving an owned string while it is
                    // borrowed invalidates the reference.
                    if self.is_borrowed(symbol) {
                        self.errors.push(SemanticError::borrow_conflict(
                            ident.name.clone(),
                            ident.span,
                            format!("cannot move `{}`: it is borrowed", ident.name),
                        ));
                        return EvalValue::owned();
                    }
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
                ref_fields,
                dead: false,
            }) => EvalValue::struct_value(
                fields
                    .iter()
                    .map(|(name, state)| (name.clone(), live_or_immutable(*state)))
                    .collect(),
                ref_fields
                    .iter()
                    .map(|(name, view)| (name.clone(), *view))
                    .collect(),
            ),
            _ => EvalValue::immutable(),
        }
    }

    /// Marks the tracked field `member` of the base binding dead (a
    /// per-field move). The base must be a plain identifier (or a group of
    /// one) whose binding is a tracked struct. Moving a field out of a
    /// borrowed struct invalidates the reference.
    fn mark_field_moved(&mut self, base: &Expr, member: &Ident) {
        let Some(root) = self.root_ident(base) else {
            return;
        };
        let Some(symbol) = self.semantic.resolve(root.span) else {
            return;
        };
        if self.is_borrowed(symbol) {
            self.errors.push(SemanticError::borrow_conflict(
                root.name.clone(),
                root.span,
                format!(
                    "cannot move field `{}` of `{}`: it is borrowed",
                    member.name, root.name
                ),
            ));
            return;
        }
        if let Some(State::Struct {
            fields,
            ref_fields,
            dead: false,
        }) = self.bindings.get_mut(&symbol)
        {
            // A reference-typed field moves its borrow out with it.
            if let Some(view) = ref_fields.remove(&member.name) {
                let _ = view;
            }
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
            Some(State::Enum { state, ref_borrows }) => {
                *state = BindingState::Dead;
                ref_borrows.clear();
            }
            Some(State::Ref {
                source,
                dead,
                mutable,
            }) => {
                // The moved reference's borrow stays live (conservative:
                // it is not tracked through the move) but this binding no
                // longer holds it.
                *source = None;
                *dead = true;
                let _ = mutable;
            }
            Some(State::Struct {
                fields,
                dead,
                ref_fields,
            }) => {
                *dead = true;
                for state in fields.values_mut() {
                    *state = BindingState::Dead;
                }
                // The moved value's references are no longer tracked
                // (conservative: their borrows stay live until the
                // binding's borrows are released at scope exit).
                ref_fields.clear();
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
            // Compound assignment is numeric-only; no ownership effect, but
            // writing through a borrowed value is still a conflict.
            if let Some(root) = self.root_ident(target) {
                if let Some(symbol) = self.semantic.resolve(root.span) {
                    if self.is_borrowed(symbol) {
                        self.errors.push(SemanticError::borrow_conflict(
                            root.name.clone(),
                            root.span,
                            format!("cannot assign to `{}`: it is borrowed", root.name),
                        ));
                    }
                }
            }
            self.eval_expr(target, Mode::Observe);
            return;
        }
        match &target.kind {
            ExprKind::Ident(ident) => {
                if let Some(symbol) = self.semantic.resolve(ident.span) {
                    if self.is_borrowed(symbol) {
                        self.errors.push(SemanticError::borrow_conflict(
                            ident.name.clone(),
                            ident.span,
                            format!("cannot assign to `{}`: it is borrowed", ident.name),
                        ));
                        return;
                    }
                    // Rebind the target from the value; assigning to a
                    // dead binding resurrects it.
                    if let Some(ty) = self.types.symbol_type(symbol) {
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
                        if self.is_borrowed(symbol) {
                            self.errors.push(SemanticError::borrow_conflict(
                                ident.name.clone(),
                                ident.span,
                                format!("cannot assign to `{}`: it is borrowed", ident.name),
                            ));
                            return;
                        }
                        // Assigning to a reference-typed field replaces
                        // its borrow with the new value's (recorded before
                        // the binding is mutated).
                        let new_view = value.view.map(|view| {
                            if let Some(source) = view.source {
                                if !view.counted {
                                    self.borrow_root(source, view.mutable);
                                }
                            }
                            BorrowView {
                                source: view.source,
                                mutable: view.mutable,
                                counted: true,
                            }
                        });
                        match self.bindings.get_mut(&symbol) {
                            Some(State::Struct {
                                fields,
                                ref_fields,
                                dead: false,
                            }) => {
                                fields.insert(
                                    member.name.clone(),
                                    BindingState::Live(value.provenance),
                                );
                                if let Some(view) = new_view {
                                    ref_fields.insert(member.name.clone(), view);
                                } else {
                                    ref_fields.remove(&member.name);
                                }
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
                        if self.is_borrowed(symbol) {
                            self.errors.push(SemanticError::borrow_conflict(
                                ident.name.clone(),
                                ident.span,
                                format!("cannot assign to `{}`: it is borrowed", ident.name),
                            ));
                            return;
                        }
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
            ExprKind::Deref { operand } => {
                // `*r = v`: allowed only through an exclusive reference.
                let value_operand = self.eval_expr(operand, Mode::Observe);
                match value_operand.view {
                    Some(view) if view.mutable => {
                        // Store through the exclusive borrow: allowed.
                        let _ = value;
                    }
                    Some(_) => {
                        self.errors.push(SemanticError::borrow_conflict_detail(
                            target.span,
                            "cannot assign through an immutable reference".to_string(),
                        ));
                    }
                    None => {
                        // Defensive: the type checker rejects derefs of
                        // non-references and stores through `&T`.
                    }
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
                ref_fields,
                dead: false,
            }) => {
                for state in fields.values_mut() {
                    *state = BindingState::Live(Provenance::Owned);
                }
                ref_fields.clear();
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
        // Evaluate arguments in transfer mode and record the borrows of
        // reference arguments (a `&x` / `&mut x` borrow is held for the
        // duration of the call).
        let mut arg_views: Vec<Option<BorrowView>> = Vec::with_capacity(args.len());
        for arg in args {
            let value = self.eval_expr(arg, Mode::Transfer);
            // A copied shared-reference argument records a new borrow for
            // the call; fresh borrows were already recorded by
            // `eval_borrow` and transfers carry theirs.
            if let Some(view) = value.view {
                if let Some(source) = view.source {
                    if !view.counted {
                        self.borrow_root(source, view.mutable);
                    }
                }
            }
            arg_views.push(value.view);
        }
        let result = match &callee.kind {
            ExprKind::Ident(ident) => self
                .semantic
                .resolve(ident.span)
                .and_then(|symbol| self.fn_results.get(&symbol).cloned()),
            _ => None,
        };
        // A call whose result is a reference carries the borrow of the
        // parameter it derives from; the borrow of every other reference
        // argument ends when the call completes.
        let ref_result = result.as_ref().and_then(|result| result.ref_result);
        match ref_result {
            Some(ref_result) => {
                let kept: Vec<usize> = match ref_result.param {
                    Some(param) => vec![param],
                    // Unknown source: conservatively keep every
                    // reference-argument borrow alive.
                    None => (0..args.len()).collect(),
                };
                for (index, view) in arg_views.iter().enumerate() {
                    if let Some(view) = view {
                        if let Some(source) = view.source {
                            if !kept.contains(&index) {
                                self.release_borrow(source, view.mutable);
                            }
                        }
                    }
                }
                let source = match ref_result.param {
                    Some(param) => arg_views
                        .get(param)
                        .copied()
                        .flatten()
                        .and_then(|view| view.source),
                    None => None,
                };
                EvalValue::borrow_view(source, ref_result.mutable, true)
            }
            None => {
                // The call consumed its reference arguments: their
                // borrows end.
                for view in arg_views.iter().flatten() {
                    if let Some(source) = view.source {
                        self.release_borrow(source, view.mutable);
                    }
                }
                match result {
                    Some(result) => match result.provenance {
                        Some(Provenance::Owned) => match result.fields {
                            Some(fields) => EvalValue::struct_value(
                                fields
                                    .into_iter()
                                    .map(|(name, provenance)| {
                                        (name, BindingState::Live(provenance))
                                    })
                                    .collect(),
                                Vec::new(),
                            ),
                            None => EvalValue::owned(),
                        },
                        Some(Provenance::Immutable) => match result.fields {
                            Some(fields) => EvalValue::struct_value(
                                fields
                                    .into_iter()
                                    .map(|(name, provenance)| {
                                        (name, BindingState::Live(provenance))
                                    })
                                    .collect(),
                                Vec::new(),
                            ),
                            None => EvalValue::immutable(),
                        },
                        None => EvalValue::copy(),
                    },
                    // Unknown callee result (not yet analyzed):
                    // conservative Owned.
                    None => EvalValue::owned(),
                }
            }
        }
    }

    /// Evaluates an intrinsic call following its argument convention.
    fn eval_intrinsic(&mut self, intrinsic: &'static Intrinsic, args: &[Expr]) -> EvalValue {
        let is_str_free = intrinsic.name == "rt_str_free";
        let is_set_byte = intrinsic.name == "rt_str_set_byte";
        let is_vec_free = intrinsic.name == "rt_vec_free";
        let is_vec_push = intrinsic.name == "rt_vec_push";
        let is_vec_get = intrinsic.name == "rt_vec_get";
        let is_vec_len = intrinsic.name == "rt_vec_len";
        for (index, arg) in args.iter().enumerate() {
            let is_str_arg = matches!(intrinsic.params.get(index), Some(IntrinsicType::Str));
            let is_vec_arg = matches!(intrinsic.params.get(index), Some(IntrinsicType::Vec));
            if is_vec_arg {
                if is_vec_free {
                    // Consume: the Vec is moved and freed.
                    let value = self.eval_expr(arg, Mode::Transfer);
                    if value.view.is_some() {
                        self.errors.push(SemanticError::borrow_conflict_detail(
                            arg.span,
                            "cannot free a Vec through a reference".to_string(),
                        ));
                    }
                } else if is_vec_push {
                    // First arg (the Vec) is consumed and returned; second is read.
                    if index == 0 {
                        self.eval_expr(arg, Mode::Transfer);
                    } else {
                        self.eval_expr(arg, Mode::Observe);
                    }
                } else if is_vec_get || is_vec_len {
                    // Read borrow: the Vec is borrowed, not consumed.
                    self.eval_expr(arg, Mode::Observe);
                } else {
                    self.eval_expr(arg, Mode::Observe);
                }
                continue;
            }
            if !is_str_arg {
                self.eval_expr(arg, Mode::Observe);
                continue;
            }
            if is_str_free {
                // Consume: the string is moved (the blob is destroyed);
                // moving a borrowed value is a conflict (E-S12) and
                // freeing through a reference is always a conflict.
                let value = self.eval_expr(arg, Mode::Transfer);
                if value.view.is_some() {
                    self.errors.push(SemanticError::borrow_conflict_detail(
                        arg.span,
                        "cannot free a string through a reference".to_string(),
                    ));
                }
            } else if is_set_byte {
                // Mutate borrow: through an exclusive reference this is
                // the point of the borrow; through a shared one it is a
                // conflict (E-S12); on an Immutable value it is E-S11.
                let value = self.eval_expr(arg, Mode::Observe);
                match value.view {
                    Some(view) => {
                        if !view.mutable {
                            self.errors.push(SemanticError::borrow_conflict_detail(
                                arg.span,
                                "cannot mutate a string through a shared reference".to_string(),
                            ));
                        }
                    }
                    None => {
                        if let Some(root) = self.root_ident(arg) {
                            if let Some(symbol) = self.semantic.resolve(root.span) {
                                if self.is_borrowed(symbol) {
                                    self.errors.push(SemanticError::borrow_conflict(
                                        root.name.clone(),
                                        root.span,
                                        format!("cannot mutate `{}`: it is borrowed", root.name),
                                    ));
                                }
                            }
                        }
                        if value.provenance == Provenance::Immutable {
                            self.errors.push(self.immutable_mutation_error(arg));
                        }
                    }
                }
            } else {
                // Read borrow. Reading through a reference is always fine;
                // reading a mutably-borrowed binding is a conflict
                // (reported by `eval_ident`).
                self.eval_expr(arg, Mode::Observe);
            }
        }
        if intrinsic.result == IntrinsicType::Str {
            // String-producing intrinsics (rt_str_alloc, rt_str_concat,
            // rt_str_from_int, rt_str_from_bool) always produce owned strings.
            EvalValue::owned()
        } else if intrinsic.result == IntrinsicType::Vec {
            // Vec-producing intrinsics produce owned values.
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
            Some(TypeKind::Ref { .. }) => Shape::Ref,
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
            // An enum with a data-carrying variant whose payload may own is
            // tracked (session 19); a unit-only or copy-payload enum is
            // Copy.
            Some(TypeKind::Enum(_)) => {
                if self.may_own(ty) {
                    Shape::Enum
                } else {
                    Shape::Copy
                }
            }
            _ => Shape::Copy,
        }
    }

    /// The names of the struct's tracked (Str/reference-containing)
    /// fields.
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

    /// The names of the struct's reference-typed fields (session 16).
    fn tracked_ref_fields(&self, ty: TypeId) -> impl Iterator<Item = String> + '_ {
        let info = match self.types.types().kind(ty) {
            Some(TypeKind::Struct(id)) => self.types.types().struct_info(*id),
            _ => None,
        };
        info.into_iter().flat_map(|info| {
            info.fields
                .iter()
                .filter(|field| {
                    matches!(
                        self.types.types().kind(field.ty),
                        Some(TypeKind::Ref { .. })
                    )
                })
                .map(|field| field.name.clone())
        })
    }

    /// Whether the reference-typed field `name` of struct `ty` is mutable.
    fn ref_field_mutable(&self, ty: TypeId, name: &str) -> bool {
        let info = match self.types.types().kind(ty) {
            Some(TypeKind::Struct(id)) => self.types.types().struct_info(*id),
            _ => None,
        };
        info.and_then(|info| {
            info.fields
                .iter()
                .find(|field| field.name == name)
                .and_then(|field| match self.types.types().kind(field.ty) {
                    Some(TypeKind::Ref { mutable, .. }) => Some(*mutable),
                    _ => None,
                })
        })
        .unwrap_or(false)
    }

    /// Whether a type may (transitively) own heap storage (or hold a
    /// reference that must be tracked).
    fn may_own(&self, ty: TypeId) -> bool {
        match self.types.types().kind(ty) {
            Some(TypeKind::Str) | Some(TypeKind::Ref { .. }) => true,
            Some(TypeKind::Array { elem, .. }) => self.may_own(*elem),
            Some(TypeKind::Struct(id)) => self
                .types
                .types()
                .struct_info(*id)
                .is_some_and(|info| info.fields.iter().any(|field| self.may_own(field.ty))),
            // An enum may own when any data-carrying variant's payload type
            // may own (session 19).
            Some(TypeKind::Enum(id)) => self.types.types().enum_info(*id).is_some_and(|info| {
                info.variants
                    .iter()
                    .any(|variant| variant.payload.is_some_and(|payload| self.may_own(payload)))
            }),
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
