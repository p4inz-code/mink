//! Core MINK type representation: types, type identity, and unification.
//!
//! The type system uses a small arena of [`TypeKind`]s addressed by stable
//! [`TypeId`]s and owned by a [`TypeTable`]. Concrete types (`Int`, `Float`,
//! …, `Range`, `Ptr`, `Fn`) are interned, so identical types share one id and
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

/// Stable identity of a user-declared struct within a [`TypeTable`].
///
/// Struct ids are assigned sequentially as struct declarations are
/// registered (in source order) and index the table's [`StructInfo`]
/// list, so a struct type is identified nominally: two declarations with
/// the same name are two distinct types (duplicate declarations are
/// rejected by semantic analysis; the first declaration wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StructId(u32);

/// Stable identity of a user-declared enum within a [`TypeTable`].
///
/// Enum ids are assigned sequentially as enum declarations are registered
/// (in source order) and index the table's [`EnumInfo`] list, so an enum
/// type is identified nominally: two declarations with the same name are
/// two distinct types (duplicate declarations are rejected by semantic
/// analysis; the first declaration wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EnumId(u32);

impl EnumId {
    /// The raw numeric value of this id.
    pub fn raw(self) -> u32 {
        self.0
    }

    /// Creates an id from its raw numeric value.
    pub(crate) fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// The declared variants of an enum: each variant's name and its
/// discriminant, in declaration order. Discriminants are assigned
/// deterministically (declaration order, starting at 0). A unit variant's
/// value is a single word holding its discriminant; a data-carrying
/// variant (session 19) additionally stores a payload of its declared
/// type, and the enum's layout is a tagged union (see
/// `docs/implementation/ENUM_TYPES_IMPLEMENTATION.md`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumInfo {
    /// The enum's declared name.
    pub name: String,
    /// The variants, in declaration order.
    pub variants: Vec<EnumVariantInfo>,
}

/// One variant of an [`EnumInfo`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumVariantInfo {
    /// The variant's name.
    pub name: String,
    /// The variant's effective discriminant (session 20): an explicit
    /// `V = n` literal's wrapping 64-bit value, or the previous variant's
    /// value plus one (declaration order, starting at 0). The value is the
    /// tag word written by construction and tested by pattern matching.
    pub discriminant: i64,
    /// The payload type for a data-carrying variant, if any; `None` for a
    /// unit variant. Only the declared payload type is stored — the tag
    /// and payload offsets within the enum's tagged-union layout are
    /// computed by `crate::runtime::layout`.
    pub payload: Option<TypeId>,
}

impl StructId {
    /// The raw numeric value of this id.
    pub fn raw(self) -> u32 {
        self.0
    }

    /// Creates an id from its raw numeric value.
    pub(crate) fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// The declared fields of a struct: each field's name and the [`TypeId`]
/// of its declared type, in declaration order. Field offsets and sizes are
/// computed by the layout engine (`crate::runtime::layout`), never stored
/// here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructInfo {
    /// The struct's declared name.
    pub name: String,
    /// The fields, in declaration order.
    pub fields: Vec<StructFieldInfo>,
}

/// One field of a [`StructInfo`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructFieldInfo {
    /// The field's name.
    pub name: String,
    /// The field's declared type.
    pub ty: TypeId,
}

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
    /// The unit type: a value that produces nothing. Only intrinsic
    /// results use it today (`rt_free`, `rt_exit`, …); a function that
    /// falls off the end keeps an unresolved inference result, which the
    /// backend classifies as unit. Unit unifies only with itself, so an
    /// expression that produces no value cannot be used where a value is
    /// required.
    Unit,
    /// A range `start..end` / `start..=end` over values of `element`.
    Range(TypeId),
    /// A pointer to a value of type `element` (session 13): the type of
    /// addresses in the runtime model. Pointers are interned like other
    /// concrete types, so `Ptr<Int>` and `Ptr<Str>` are distinct stable
    /// types that compare equal by identity. A pointer is a single machine
    /// word holding an address; arithmetic is byte-addressed and the
    /// runtime validates alignment and bounds at every access (see
    /// `docs/implementation/STRING_MEMORY_IMPLEMENTATION.md`). `Ptr` is a
    /// value type, distinct from `Str` — a string is *represented* as a
    /// blob address but is not a pointer type, so strings cannot be
    /// dereferenced through the raw memory intrinsics.
    Ptr(TypeId),
    /// A reference to a value of type `elem` (session 16): `&T`
    /// (immutable/shared, `mutable: false`) or `&mut T`
    /// (mutable/exclusive, `mutable: true`). References are interned like
    /// other concrete types and unify structurally; `&T` never unifies
    /// with `&mut T`, `Ptr`, `Str`, or a value type. A reference is a
    /// single machine word holding the machine address of a stack slot
    /// (or of a field/element region inside one); mutability is purely a
    /// compile-time concept enforced by the borrow checker (see
    /// `docs/implementation/REFERENCES_BORROWING_IMPLEMENTATION.md`).
    Ref {
        /// Whether the reference is mutable (`&mut T`).
        mutable: bool,
        /// The referent type.
        elem: TypeId,
    },
    /// A user-declared struct: a named record of typed fields (session 14).
    /// The struct's fields live in the table's [`StructInfo`] list, indexed
    /// by the [`StructId`]; types are nominal, so two declarations are two
    /// distinct types. The deterministic byte layout (field offsets,
    /// alignment, size) is computed by `crate::runtime::layout`.
    Struct(StructId),
    /// A user-declared enum: a closed set of named alternatives (session
    /// 17). The enum's variants live in the table's [`EnumInfo`] list,
    /// indexed by the [`EnumId`]; types are nominal, so two declarations
    /// are two distinct types. A unit variant's value is a single machine
    /// word holding its discriminant (assigned in declaration order,
    /// starting at 0); an enum with data-carrying variants (session 19) is
    /// laid out as a tagged union (see `crate::runtime::layout`).
    Enum(EnumId),
    /// A fixed-length array: `len` consecutive values of type `elem`
    /// (session 14). Arrays are value types with deterministic layout;
    /// the element count is part of the type identity, so `Array<Int, 2>`
    /// and `Array<Int, 3>` are distinct types. `len` is always positive
    /// and its layout must fit the runtime memory model (validated by type
    /// analysis and the layout engine).
    Array {
        /// The element type.
        elem: TypeId,
        /// The number of elements.
        len: u64,
    },
    /// A tuple type (session 29): `(T1, T2, ...)` — a fixed-length,
    /// heterogeneous sequence of types. An empty tuple `()` is the unit
    /// type (equivalent to [`TypeKind::Unit`]). A single-element tuple
    /// `(T,)` is distinct from `T`. Tuple types are structurally
    /// compared and interned: `(Int, Bool)` is the same type everywhere.
    Tuple(Vec<TypeId>),
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
    /// The user-declared structs, in registration order. A struct type's
    /// [`StructId`] indexes this list; the fields are filled in after all
    /// struct declarations are registered (module-scope order independence).
    structs: Vec<StructInfo>,
    /// The user-declared enums, in registration order. An enum type's
    /// [`EnumId`] indexes this list; the variants are filled in after all
    /// enum declarations are registered (module-scope order independence).
    enums: Vec<EnumInfo>,
}

impl TypeTable {
    /// Creates an empty type table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new user-declared enum and returns its type id. The
    /// enum is registered with no variants yet (variants are recorded
    /// through [`TypeTable::set_enum_variants`] once every enum
    /// declaration is visible), so enum types can be referenced before
    /// their declaration. Each declaration registers a distinct id; the
    /// type is never interned (enum types are nominal).
    pub(crate) fn push_enum(&mut self, name: String) -> TypeId {
        let id = EnumId::new(self.enums.len() as u32);
        self.enums.push(EnumInfo {
            name,
            variants: Vec::new(),
        });
        self.push(TypeKind::Enum(id))
    }

    /// Records the resolved variants of the enum registered under `id`.
    /// Called once, after every enum declaration is registered.
    pub(crate) fn set_enum_variants(&mut self, id: EnumId, variants: Vec<EnumVariantInfo>) {
        if let Some(info) = self.enums.get_mut(id.raw() as usize) {
            info.variants = variants;
        }
    }

    /// The enum registered under `id`, if any.
    pub fn enum_info(&self, id: EnumId) -> Option<&EnumInfo> {
        self.enums.get(id.raw() as usize)
    }

    /// All registered enums, in registration order.
    pub fn enums(&self) -> &[EnumInfo] {
        &self.enums
    }

    /// The enum id of the enum type `ty`, if `ty` denotes an enum.
    pub fn enum_id(&self, ty: TypeId) -> Option<EnumId> {
        match self.kind(ty) {
            Some(TypeKind::Enum(id)) => Some(*id),
            _ => None,
        }
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

    /// Whether `id` currently denotes a fully determined type: following
    /// resolved inference variables must not end at an unresolved one.
    ///
    /// The error type counts as resolved — it is a known, deliberate
    /// outcome — so this answers the inference question "has every
    /// determinable type been determined?" (see
    /// `docs/implementation/TYPE_INFERENCE_IMPLEMENTATION.md` §5).
    pub fn is_resolved(&self, id: TypeId) -> bool {
        !matches!(self.kind(id), Some(TypeKind::Infer(_)))
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
            (TypeKind::Ptr(ia), TypeKind::Ptr(ib)) => {
                self.unify(*ia, *ib)?;
                Ok(a)
            }
            (
                TypeKind::Ref {
                    mutable: ma,
                    elem: ea,
                },
                TypeKind::Ref {
                    mutable: mb,
                    elem: eb,
                },
            ) => {
                // `&T` and `&mut T` never unify; identical mutability
                // unifies the element types structurally.
                if ma != mb {
                    return Err((a, b));
                }
                self.unify(*ea, *eb)?;
                Ok(a)
            }
            (TypeKind::Array { elem: ea, len: la }, TypeKind::Array { elem: eb, len: lb }) => {
                if la != lb {
                    return Err((a, b));
                }
                self.unify(*ea, *eb)?;
                Ok(a)
            }
            // Tuples unify element-wise: same length and same element
            // types. An empty tuple unifies only with another empty tuple
            // (or Unit, since () == Unit).
            (TypeKind::Tuple(elems_a), TypeKind::Tuple(elems_b)) => {
                if elems_a.len() != elems_b.len() {
                    return Err((a, b));
                }
                for (ea, eb) in elems_a.iter().zip(elems_b.iter()) {
                    self.unify(*ea, *eb)?;
                }
                Ok(a)
            }
            // Struct and enum types are nominal: only the identical
            // struct/enum unifies with itself (caught by the `a == b` check
            // above); neither ever unifies with any other type, including a
            // differently registered declaration.
            (TypeKind::Struct(_), _) | (_, TypeKind::Struct(_)) => Err((a, b)),
            (TypeKind::Enum(_), _) | (_, TypeKind::Enum(_)) => Err((a, b)),
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
            Some(TypeKind::Unit) => "Unit".to_string(),
            Some(TypeKind::Range(elem)) => format!("Range<{}>", self.display(*elem)),
            Some(TypeKind::Ptr(elem)) => format!("Ptr<{}>", self.display(*elem)),
            Some(TypeKind::Ref { mutable, elem }) => {
                if *mutable {
                    format!("&mut {}", self.display(*elem))
                } else {
                    format!("&{}", self.display(*elem))
                }
            }
            Some(TypeKind::Struct(id)) => self
                .struct_info(*id)
                .map(|info| info.name.clone())
                .unwrap_or_else(|| format!("Struct#{}", id.raw())),
            Some(TypeKind::Enum(id)) => self
                .enum_info(*id)
                .map(|info| info.name.clone())
                .unwrap_or_else(|| format!("Enum#{}", id.raw())),
            Some(TypeKind::Array { elem, len }) => {
                format!("Array<{}, {len}>", self.display(*elem))
            }
            Some(TypeKind::Tuple(elems)) => {
                if elems.is_empty() {
                    "()".to_string()
                } else {
                    let inner = elems
                        .iter()
                        .map(|e| self.display(*e))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("({inner})")
                }
            }
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

    /// Whether `id` is a tuple type (session 29).
    pub fn is_tuple(&self, id: TypeId) -> bool {
        matches!(self.kind(id), Some(TypeKind::Tuple(_)))
    }

    /// The element types of a tuple, if `id` denotes a tuple.
    pub fn tuple_elems(&self, id: TypeId) -> Option<&[TypeId]> {
        match self.kind(id) {
            Some(TypeKind::Tuple(elems)) => Some(elems),
            _ => None,
        }
    }

    /// Registers a new user-declared struct and returns its type id. The
    /// struct is registered with no fields yet (fields are resolved and
    /// recorded through [`TypeTable::set_struct_fields`] once every struct
    /// declaration is visible), so struct types can reference each other
    /// regardless of declaration order. Each declaration registers a
    /// distinct id; the type is never interned (struct types are nominal).
    pub(crate) fn push_struct(&mut self, name: String) -> TypeId {
        let id = StructId::new(self.structs.len() as u32);
        self.structs.push(StructInfo {
            name,
            fields: Vec::new(),
        });
        self.push(TypeKind::Struct(id))
    }

    /// Records the resolved fields of the struct registered under `id`.
    /// Called once, after every struct declaration is registered.
    pub(crate) fn set_struct_fields(&mut self, id: StructId, fields: Vec<StructFieldInfo>) {
        if let Some(info) = self.structs.get_mut(id.raw() as usize) {
            info.fields = fields;
        }
    }

    /// The struct registered under `id`, if any.
    pub fn struct_info(&self, id: StructId) -> Option<&StructInfo> {
        self.structs.get(id.raw() as usize)
    }

    /// All registered structs, in registration order.
    pub fn structs(&self) -> &[StructInfo] {
        &self.structs
    }

    /// The struct id of the struct type `ty`, if `ty` denotes a struct.
    pub fn struct_id(&self, ty: TypeId) -> Option<StructId> {
        match self.kind(ty) {
            Some(TypeKind::Struct(id)) => Some(*id),
            _ => None,
        }
    }

    /// The type of a named field on the struct type `ty`, if `ty` is a
    /// struct and the field exists.
    pub fn struct_field_type(&self, ty: TypeId, field_name: &str) -> Option<TypeId> {
        let id = self.struct_id(ty)?;
        let info = self.struct_info(id)?;
        info.fields
            .iter()
            .find(|f| f.name == field_name)
            .map(|f| f.ty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Interns the type `Ptr<Int>`.
    fn ptr_int(table: &mut TypeTable) -> TypeId {
        let elem = table.push(TypeKind::Int);
        table.push(TypeKind::Ptr(elem))
    }

    #[test]
    fn pointer_types_are_interned() {
        let mut table = TypeTable::new();
        let a = ptr_int(&mut table);
        let b = ptr_int(&mut table);
        assert_eq!(a, b, "identical pointer types share one id");
    }

    #[test]
    fn pointer_element_types_distinguish_identity() {
        let mut table = TypeTable::new();
        let int = table.push(TypeKind::Int);
        let unit = table.push(TypeKind::Unit);
        let ptr_int = table.push(TypeKind::Ptr(int));
        let ptr_unit = table.push(TypeKind::Ptr(unit));
        assert_ne!(ptr_int, ptr_unit, "Ptr<Int> and Ptr<Unit> are distinct");
        assert_eq!(table.display(ptr_int), "Ptr<Int>");
        assert_eq!(table.display(ptr_unit), "Ptr<Unit>");
    }

    #[test]
    fn pointer_unification_is_structural() {
        let mut table = TypeTable::new();
        let a = ptr_int(&mut table);
        let b = ptr_int(&mut table);
        assert_eq!(table.unify(a, b).unwrap(), a);
        // A pointer unifies with a fresh inference variable, resolving the
        // variable to the pointer type.
        let var = table.push(TypeKind::Infer(None));
        let unified = table.unify(var, a).unwrap();
        assert_eq!(table.canonical(var), a);
        assert_eq!(unified, a);
    }

    #[test]
    fn array_types_are_interned_by_element_and_length() {
        let mut table = TypeTable::new();
        let int = table.push(TypeKind::Int);
        let a = table.push(TypeKind::Array { elem: int, len: 2 });
        let b = table.push(TypeKind::Array { elem: int, len: 2 });
        let c = table.push(TypeKind::Array { elem: int, len: 3 });
        let float = table.push(TypeKind::Float);
        let d = table.push(TypeKind::Array {
            elem: float,
            len: 2,
        });
        assert_eq!(a, b, "identical array types share one id");
        assert_ne!(a, c, "length is part of the type identity");
        assert_ne!(a, d, "element type is part of the type identity");
        assert_eq!(table.display(a), "Array<Int, 2>");
        assert_eq!(table.display(d), "Array<Float, 2>");
    }

    #[test]
    fn array_unification_is_structural() {
        let mut table = TypeTable::new();
        let int = table.push(TypeKind::Int);
        let a = table.push(TypeKind::Array { elem: int, len: 2 });
        let var = table.push(TypeKind::Infer(None));
        let elem_var = table.push(TypeKind::Infer(None));
        let other = table.push(TypeKind::Array {
            elem: elem_var,
            len: 2,
        });
        assert!(table.unify(var, a).is_ok());
        assert_eq!(table.canonical(var), a);
        // The element variable adopts the element type.
        assert!(table.unify(other, a).is_ok());
        assert_eq!(table.canonical(elem_var), int);
        // Different lengths never unify.
        let len3 = table.push(TypeKind::Array { elem: int, len: 3 });
        assert!(table.unify(a, len3).is_err());
    }

    #[test]
    fn struct_types_are_nominal_and_hold_fields() {
        let mut table = TypeTable::new();
        let int = table.push(TypeKind::Int);
        let point = table.push_struct("Point".to_string());
        let other = table.push_struct("Point".to_string());
        // Two declarations are two distinct types (semantic analysis
        // rejects duplicates; the first declaration wins).
        assert_ne!(point, other);
        let id = table.struct_id(point).unwrap();
        table.set_struct_fields(
            id,
            vec![StructFieldInfo {
                name: "x".to_string(),
                ty: int,
            }],
        );
        let info = table.struct_info(id).unwrap();
        assert_eq!(info.name, "Point");
        assert_eq!(info.fields.len(), 1);
        assert_eq!(info.fields[0].name, "x");
        assert_eq!(table.display(point), "Point");
        // A struct unifies only with itself.
        assert!(table.unify(point, point).is_ok());
        assert!(table.unify(point, other).is_err());
        assert!(table.unify(point, int).is_err());
        let var = table.push(TypeKind::Infer(None));
        assert!(table.unify(var, point).is_ok());
        assert_eq!(table.canonical(var), point);
    }

    #[test]
    fn enum_types_are_nominal_and_hold_variants() {
        let mut table = TypeTable::new();
        let direction = table.push_enum("Direction".to_string());
        let other = table.push_enum("Direction".to_string());
        // Two declarations are two distinct types (semantic analysis
        // rejects duplicates; the first declaration wins).
        assert_ne!(direction, other);
        let id = table.enum_id(direction).unwrap();
        table.set_enum_variants(
            id,
            vec![
                EnumVariantInfo {
                    name: "North".to_string(),
                    discriminant: 0,
                    payload: None,
                },
                EnumVariantInfo {
                    name: "South".to_string(),
                    discriminant: 1,
                    payload: None,
                },
            ],
        );
        let info = table.enum_info(id).unwrap();
        assert_eq!(info.name, "Direction");
        assert_eq!(info.variants.len(), 2);
        assert_eq!(info.variants[0].name, "North");
        assert_eq!(info.variants[0].discriminant, 0);
        assert_eq!(table.display(direction), "Direction");
        // An enum unifies only with itself.
        assert!(table.unify(direction, direction).is_ok());
        assert!(table.unify(direction, other).is_err());
        let int = table.push(TypeKind::Int);
        assert!(table.unify(direction, int).is_err());
        let var = table.push(TypeKind::Infer(None));
        assert!(table.unify(var, direction).is_ok());
        assert_eq!(table.canonical(var), direction);
    }

    /// Interns the type `&T` (shared).
    fn ref_shared(table: &mut TypeTable, elem: TypeId) -> TypeId {
        table.push(TypeKind::Ref {
            mutable: false,
            elem,
        })
    }

    /// Interns the type `&mut T` (exclusive).
    fn ref_mut(table: &mut TypeTable, elem: TypeId) -> TypeId {
        table.push(TypeKind::Ref {
            mutable: true,
            elem,
        })
    }

    #[test]
    fn reference_types_are_interned_and_displayed() {
        let mut table = TypeTable::new();
        let int = table.push(TypeKind::Int);
        let a = ref_shared(&mut table, int);
        let b = ref_shared(&mut table, int);
        assert_eq!(a, b, "identical reference types share one id");
        assert_eq!(table.display(a), "&Int");
        let m = ref_mut(&mut table, int);
        assert_ne!(a, m, "&T and &mut T are distinct types");
        assert_eq!(table.display(m), "&mut Int");
        // The element type is part of the identity.
        let str_ty = table.push(TypeKind::Str);
        let shared_str = ref_shared(&mut table, str_ty);
        assert_ne!(a, shared_str);
        assert_eq!(table.display(shared_str), "&Str");
    }

    #[test]
    fn reference_unification_is_structural_and_mutability_sensitive() {
        let mut table = TypeTable::new();
        let int = table.push(TypeKind::Int);
        let shared = ref_shared(&mut table, int);
        let exclusive = ref_mut(&mut table, int);
        // Identical references unify; mutability never does.
        let shared2 = ref_shared(&mut table, int);
        assert!(table.unify(shared, shared2).is_ok());
        assert!(table.unify(shared, exclusive).is_err());
        let exclusive2 = ref_mut(&mut table, int);
        assert!(table.unify(exclusive, exclusive2).is_ok());
        // A reference resolves a fresh inference variable.
        let var = table.push(TypeKind::Infer(None));
        assert!(table.unify(var, shared).is_ok());
        assert_eq!(table.canonical(var), shared);
        // Element mismatches surface as element conflicts (like `Ptr`).
        let str_ty = table.push(TypeKind::Str);
        let shared_str = ref_shared(&mut table, str_ty);
        let err = table.unify(shared, shared_str).unwrap_err();
        assert_eq!(table.canonical(err.0), int);
        assert_eq!(table.canonical(err.1), str_ty);
        // A reference never unifies with a value type or a pointer.
        assert!(table.unify(shared, int).is_err());
        let ptr = table.push(TypeKind::Ptr(int));
        assert!(table.unify(shared, ptr).is_err());
    }

    #[test]
    fn pointer_conflicts_are_reported() {
        let mut table = TypeTable::new();
        let ptr_int = ptr_int(&mut table);
        let elem = table.push(TypeKind::Str);
        let ptr_str = table.push(TypeKind::Ptr(elem));
        let err = table.unify(ptr_int, ptr_str).unwrap_err();
        // A `Ptr<Int>` vs `Ptr<Str>` conflict surfaces as a conflict of the
        // element types (consistent with `Range`), which the caller renders.
        let (a, b) = err;
        assert_eq!(table.canonical(a), table.push(TypeKind::Int));
        assert_eq!(table.canonical(b), table.push(TypeKind::Str));
        // A pointer never unifies with a non-pointer concrete type.
        assert!(table.unify(ptr_int, elem).is_err());
    }
}
