//! Core MINK type representation: types, type identity, and unification.
//!
//! The type system uses a small arena of [`TypeKind`]s addressed by stable
//! [`TypeId`]s and owned by a [`TypeTable`]. Concrete types (`Int`, `Float`,
//! …, `Range`, `Fn`) are interned, so identical types share one id and
//! compare equal by identity; inference variables are arena slots whose
//! content unification can resolve, mirroring the classic union-find style
//! used by many statically typed compilers.
//!
//! [`TypeTable::unify`] is the single authoritative type-comparison
//! mechanism: it answers whether two types are compatible and, when one is
//! an inference variable, records the relationship. Callers translate a
//! unification failure into a type diagnostic (see
//! `docs/implementation/TYPE_SYSTEM_IMPLEMENTATION.md`).

use std::collections::HashMap;

/// Stable identity of a type within a [`TypeTable`].
///
/// Ids are assigned sequentially as types are registered and remain valid
/// for the lifetime of the table that created them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeId(u32);

impl TypeId {
    /// The raw numeric value of this id.
    pub fn raw(self) -> u32 {
        self.0
    }

    /// Creates an id from its raw numeric value.
    ///
    /// Ids should normally be produced by a [`TypeTable`]; constructing one
    /// directly is only useful for tests and tooling that manages types
    /// itself.
    pub(crate) fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// The kind of a type stored in a [`TypeTable`].
///
/// This milestone implements exactly the core types the frozen grammar and
/// the session-06 type-system decisions require
/// (`docs/language/CORE_LANGUAGE.md` §26): the scalar core types, ranges,
/// and function types. User-defined types, generics, and the advanced type
/// forms described in `docs/language/TYPE_SYSTEM.md` are later milestones.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeKind {
    /// The unknown/error type: produced for expressions that cannot be
    /// typed because of an earlier (semantic or type) error. It absorbs
    /// every unification, so one root error never cascades into a swarm of
    /// misleading secondary diagnostics (see
    /// `docs/implementation/TYPE_SYSTEM_IMPLEMENTATION.md` §8).
    Error,
    /// Integer values. The current language defines a single integer type;
    /// exact widths are a runtime/ABI decision
    /// (`docs/language/TYPE_SYSTEM.md` §3).
    Int,
    /// Floating-point values.
    Float,
    /// Boolean values.
    Bool,
    /// A single Unicode scalar value.
    Char,
    /// A string of Unicode scalar values.
    Str,
    /// The `null` literal. Null is a distinct concrete type — it is not a
    /// bottom type, and it unifies with nothing except itself.
    Null,
    /// A range `start..end` / `start..=end` over values of `element`.
    Range(TypeId),
    /// A function taking `params` and producing `result`.
    Fn {
        /// The parameter types, in declaration order.
        params: Vec<TypeId>,
        /// The result type.
        result: TypeId,
    },
    /// An inference variable: a placeholder whose type is not yet known.
    /// `Some(target)` records the type the variable has been unified with;
    /// `None` means it is still unconstrained.
    Infer(Option<TypeId>),
}

/// An arena of types addressed by stable [`TypeId`]s.
///
/// Concrete types are interned: pushing `Int` twice returns the same id,
/// so type identity is cheap and structural comparison is rarely needed.
/// Inference variables are never interned — each is a distinct slot that
/// unification can resolve.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypeTable {
    kinds: Vec<TypeKind>,
    interned: HashMap<TypeKind, TypeId>,
}

impl TypeTable {
    /// Creates an empty type table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers (or reuses) a type and returns its stable id.
    ///
    /// Concrete types are interned, so pushing the same kind twice shares
    /// one id. Inference variables and the error type always create a
    /// fresh slot.
    pub(crate) fn push(&mut self, kind: TypeKind) -> TypeId {
        if !matches!(kind, TypeKind::Infer(_) | TypeKind::Error) {
            if let Some(&id) = self.interned.get(&kind) {
                return id;
            }
        }
        let id = TypeId::new(self.kinds.len() as u32);
        self.kinds.push(kind.clone());
        if !matches!(kind, TypeKind::Infer(_) | TypeKind::Error) {
            self.interned.insert(kind, id);
        }
        id
    }

    /// The canonical id of `id`: follows resolved inference variables to
    /// the type they currently denote. An unresolved variable is its own
    /// canonical id.
    pub fn canonical(&self, mut id: TypeId) -> TypeId {
        while let Some(TypeKind::Infer(Some(next))) = self.kinds.get(id.0 as usize) {
            id = *next;
        }
        id
    }

    /// Like [`TypeTable::canonical`], but compresses the walked path so
    /// repeated canonicalization through long inference chains stays cheap
    /// (path compression; see [`TypeTable::unify`]).
    fn canonical_mut(&mut self, mut id: TypeId) -> TypeId {
        // First walk to find the root.
        let mut root = id;
        while let Some(TypeKind::Infer(Some(next))) = self.kinds.get(root.0 as usize) {
            root = *next;
        }
        // Then re-point every slot on the path at the root. The slot is
        // cloned into the guard so the loop body can mutate the table
        // freely (`TypeKind` is small, and the cloned slot is cheap).
        while let TypeKind::Infer(Some(next)) = self.kinds[id.0 as usize].clone() {
            self.kinds[id.0 as usize] = TypeKind::Infer(Some(root));
            id = next;
        }
        root
    }

    /// The kind of `id`, following resolved inference variables.
    ///
    /// This is the canonical view: a resolved variable reports the kind it
    /// was unified with, and an unresolved one reports
    /// [`TypeKind::Infer`]`(None)`.
    pub fn kind(&self, id: TypeId) -> Option<&TypeKind> {
        self.kinds.get(self.canonical(id).0 as usize)
    }

    /// Whether `id` currently denotes the unknown/error type.
    pub fn is_error(&self, id: TypeId) -> bool {
        matches!(self.kind(id), Some(TypeKind::Error))
    }

    /// Attempts to unify `a` and `b`, returning the unified type.
    ///
    /// This is the single authoritative type-comparison mechanism:
    ///
    /// - identical types unify to themselves;
    /// - the error type absorbs any type (the result is the error type);
    /// - an inference variable adopts the other type;
    /// - structurally equal composite types unify recursively
    ///   ([`TypeKind::Range`], [`TypeKind::Fn`]);
    /// - anything else is a conflict, and the two canonical types involved
    ///   are returned so the caller can render a diagnostic.
    ///
    /// Unification is deterministic and never panics: ids are always
    /// looked up guarded, recursion is bounded by type nesting depth, and
    /// walked inference chains are path-compressed so repeated unifies and
    /// lookups stay amortized near-constant.
    pub fn unify(&mut self, a: TypeId, b: TypeId) -> Result<TypeId, (TypeId, TypeId)> {
        let a = self.canonical_mut(a);
        let b = self.canonical_mut(b);
        if a == b {
            return Ok(a);
        }
        // The error type absorbs any type: the result is the error type,
        // and an unconstrained inference variable is linked to it so later
        // uses of that variable stay silently unknown too (cascade control).
        if self.is_error(a) || self.is_error(b) {
            let (error, other) = if self.is_error(a) { (a, b) } else { (b, a) };
            if matches!(self.kinds.get(other.0 as usize), Some(TypeKind::Infer(_))) {
                self.kinds[other.0 as usize] = TypeKind::Infer(Some(error));
            }
            return Ok(error);
        }
        let ka = self.kinds[a.0 as usize].clone();
        let kb = self.kinds[b.0 as usize].clone();
        match (&ka, &kb) {
            (TypeKind::Infer(_), _) => {
                self.kinds[a.0 as usize] = TypeKind::Infer(Some(b));
                Ok(b)
            }
            (_, TypeKind::Infer(_)) => {
                self.kinds[b.0 as usize] = TypeKind::Infer(Some(a));
                Ok(a)
            }
            (TypeKind::Range(ia), TypeKind::Range(ib)) => {
                self.unify(*ia, *ib)?;
                Ok(a)
            }
            (
                TypeKind::Fn {
                    params: pa,
                    result: ra,
                },
                TypeKind::Fn {
                    params: pb,
                    result: rb,
                },
            ) => {
                if pa.len() != pb.len() {
                    return Err((a, b));
                }
                for (x, y) in pa.iter().zip(pb) {
                    self.unify(*x, *y)?;
                }
                self.unify(*ra, *rb)?;
                Ok(a)
            }
            _ => {
                if ka == kb {
                    Ok(a)
                } else {
                    Err((a, b))
                }
            }
        }
    }

    /// The human-readable name of `id`'s canonical type, used by
    /// diagnostics and tooling (for example `Int`, `Range<Int>`,
    /// `fn(Int) -> Bool`). An unresolved inference variable displays as
    /// `unresolved`; the error type as `unknown`.
    pub fn display(&self, id: TypeId) -> String {
        match self.kind(id) {
            Some(TypeKind::Error) | None => "unknown".to_string(),
            Some(TypeKind::Int) => "Int".to_string(),
            Some(TypeKind::Float) => "Float".to_string(),
            Some(TypeKind::Bool) => "Bool".to_string(),
            Some(TypeKind::Char) => "Char".to_string(),
            Some(TypeKind::Str) => "Str".to_string(),
            Some(TypeKind::Null) => "Null".to_string(),
            Some(TypeKind::Range(elem)) => format!("Range<{}>", self.display(*elem)),
            Some(TypeKind::Fn { params, result }) => {
                let params = params
                    .iter()
                    .map(|param| self.display(*param))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("fn({params}) -> {}", self.display(*result))
            }
            Some(TypeKind::Infer(_)) => "unresolved".to_string(),
        }
    }

    /// Number of type slots in the table.
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    /// Whether the table contains no types.
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }
}
