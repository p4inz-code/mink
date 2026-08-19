//! Explicit memory-layout representation.
//!
//! [`MemoryLayout`] is the authoritative size/alignment description of a
//! value, the representation future structs, arrays, and strings will be
//! built from. The language has three scalar layout classes
//! ([`ValueClass`]); the aggregate machinery — [`struct_layout`],
//! [`array_layout`], field offsets, alignment, size, and the guarantee
//! that layout is deterministic and documented — is the foundation for
//! structs and arrays (session 14, see
//! `docs/implementation/AGGREGATE_TYPES_IMPLEMENTATION.md`).
//!
//! Rules of the memory model:
//!
//! - every value's size is a multiple of its alignment;
//! - alignment is always a power of two;
//! - a `Word` value occupies one 8-byte word (an integer or a boolean
//!   stored as `0`/`1`);
//! - a `Range<Int>` occupies two consecutive 8-byte words;
//! - a `Unit` value occupies no storage: it is the type of an expression
//!   that produces no value, and it is never materialized;
//! - heap blocks are rounded up to the allocator alignment (16) regardless
//!   of the value they hold, so every block can hold any value class.
//!
//! ## Aggregate layout (session 14)
//!
//! Struct and array values use a deterministic C-style byte layout computed
//! from the type table by [`struct_layout`] and [`array_layout`]:
//!
//! - a struct's fields are placed in declaration order, each rounded up to
//!   its alignment; the struct's size is rounded up to its alignment and
//!   its alignment is the maximum field alignment;
//! - an array's elements are laid out consecutively with stride equal to
//!   the element size; its alignment is the element alignment and its size
//!   is `len * elem_size`;
//! - layout is computed with checked arithmetic and recursion tracking, so
//!   a recursive struct (a struct reachable from itself by value) and a
//!   size overflow are reported as structured [`LayoutError`]s instead of
//!   panicking;
//! - every aggregate value is additionally bounded by
//!   [`MAX_AGGREGATE_BYTES`]: a value larger than the runtime heap can
//!   never be stored by this runtime, so its type is rejected deterministically.

use super::abi::{ALLOC_ALIGNMENT, WORD_SIZE};
use crate::typecheck::{EnumId, StructId, TypeId, TypeKind, TypeTable};

/// One entry on the by-value recursion path of a layout computation.
///
/// Struct and enum ids live in separate namespaces, so the path carries the
/// kind of each aggregate to tell `struct P` apart from `enum P` (both can
/// be raw id 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathEntry {
    Struct(StructId),
    Enum(EnumId),
}

/// The layout classes the current memory model distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueClass {
    /// One machine word: `Int` or `Bool` (stored as `0`/`1`).
    Word,
    /// Two consecutive words: `Range<Int>`.
    TwoWords,
    /// No storage: a value that is never materialized.
    Unit,
}

/// The size (in bytes) and alignment (in bytes) of a [`ValueClass`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryLayout {
    /// The size of the value in bytes.
    pub size: u64,
    /// The alignment of the value in bytes (a power of two).
    pub align: u64,
}

impl MemoryLayout {
    /// The layout of [`ValueClass::Word`].
    pub fn word() -> Self {
        Self {
            size: WORD_SIZE,
            align: WORD_SIZE,
        }
    }

    /// The layout of [`ValueClass::TwoWords`].
    pub fn two_words() -> Self {
        Self {
            size: 2 * WORD_SIZE,
            align: WORD_SIZE,
        }
    }

    /// The layout of [`ValueClass::Unit`]: zero size, alignment 1.
    pub fn unit() -> Self {
        Self { size: 0, align: 1 }
    }

    /// The layout of a [`ValueClass`].
    pub fn of(class: ValueClass) -> Self {
        match class {
            ValueClass::Word => Self::word(),
            ValueClass::TwoWords => Self::two_words(),
            ValueClass::Unit => Self::unit(),
        }
    }

    /// The size of a [`ValueClass`].
    pub fn size_of(class: ValueClass) -> u64 {
        Self::of(class).size
    }

    /// The alignment of a [`ValueClass`].
    pub fn align_of(class: ValueClass) -> u64 {
        Self::of(class).align
    }

    /// The size of a heap block holding a value of `class`: the value's
    /// size rounded up to the allocator alignment, so every block is
    /// independently addressable at the allocator's alignment.
    pub fn heap_block_size(class: ValueClass) -> u64 {
        let size = Self::size_of(class);
        if size == 0 {
            0
        } else {
            size.div_ceil(ALLOC_ALIGNMENT) * ALLOC_ALIGNMENT
        }
    }
}

/// The maximum byte size of a single aggregate value (struct or array).
///
/// A value larger than the fixed 1 MiB heap can never be stored anywhere by
/// this runtime, so its type is rejected deterministically. The bound also
/// keeps stack frames, argument copies, and generated code proportional to
/// the memory model.
pub const MAX_AGGREGATE_BYTES: u64 = 1024 * 1024;

/// Why an aggregate's layout could not be computed. Every failure is
/// deterministic and reported as a structured error (never a panic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    /// A struct is reachable from itself by value (directly or through
    /// other structs/arrays), so it has no finite size. `name` is the
    /// struct whose layout was requested.
    Recursive {
        /// The name of the struct whose layout was requested.
        name: String,
    },
    /// A struct declares no fields, so it has no layout. `name` is the
    /// struct.
    Empty {
        /// The name of the struct.
        name: String,
    },
    /// An aggregate's size computation overflowed 64 bits. `name` is the
    /// rendered type whose layout was requested.
    Overflow {
        /// The name of the aggregate whose size overflowed.
        name: String,
    },
    /// The aggregate is larger than [`MAX_AGGREGATE_BYTES`]. `name` is the
    /// rendered type.
    TooLarge {
        /// The name of the aggregate that exceeds the bound.
        name: String,
    },
}

/// The byte layout of one struct field: its offset within the struct
/// value, its byte size, and its alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldLayout {
    /// The field's byte offset within the struct value.
    pub offset: u64,
    /// The field's byte size.
    pub size: u64,
    /// The field's alignment.
    pub align: u64,
}

/// The deterministic byte layout of a struct value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructLayout {
    /// The struct's byte size (rounded up to its alignment).
    pub size: u64,
    /// The struct's alignment.
    pub align: u64,
    /// One layout per declared field, in declaration order.
    pub fields: Vec<FieldLayout>,
}

/// The deterministic byte layout of an array value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrayLayout {
    /// The array's byte size.
    pub size: u64,
    /// The array's alignment.
    pub align: u64,
    /// The element size in bytes (the array's stride).
    pub elem_size: u64,
    /// The element alignment.
    pub elem_align: u64,
    /// The element count.
    pub len: u64,
}

/// The byte layout of one variant's payload within an enum's tagged-union
/// layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VariantPayloadLayout {
    /// The variant's effective discriminant (session 20): the tag word
    /// value written by construction and tested by pattern matching.
    pub discriminant: i64,
    /// The payload byte size (0 for a unit variant).
    pub size: u64,
    /// The payload alignment (1 for a unit variant).
    pub align: u64,
}

/// The deterministic byte layout of an enum value (session 19).
///
/// An enum with **only unit variants** keeps the session-17 layout: a
/// single word holding the discriminant (`tagged == false`). An enum with
/// **any data-carrying variant** is a tagged union (`tagged == true`): a
/// discriminant word at `tag_offset` followed by a payload area at
/// `payload_offset` that is shared by every variant (each variant stores
/// its payload at the same offset; only the discriminant distinguishes
/// them). The payload area is sized and aligned for the largest variant
/// payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumLayout {
    /// The enum's byte size (rounded up to its alignment).
    pub size: u64,
    /// The enum's alignment.
    pub align: u64,
    /// Whether the enum has any data-carrying variant (a tagged union).
    /// A unit-only enum is a single word holding its discriminant.
    pub tagged: bool,
    /// The byte offset of the discriminant word within the value. For a
    /// unit-only enum the value *is* the discriminant, so this is 0.
    pub tag_offset: u64,
    /// The byte offset of the payload area within the value; 0 for a
    /// unit-only enum.
    pub payload_offset: u64,
    /// The byte size of the payload area (the largest variant payload
    /// rounded up to its alignment); 0 for a unit-only enum.
    pub payload_size: u64,
    /// The alignment of the payload area; 1 for a unit-only enum.
    pub payload_align: u64,
    /// The payload layout of every variant, in declaration order.
    pub variants: Vec<VariantPayloadLayout>,
}

/// The byte size and alignment of a scalar type. Aggregate and function
/// types return `None` (they are handled by the recursive layout walker).
/// An enum is scalar only when every variant is a unit variant (its value
/// is the discriminant word); an enum with a data-carrying variant is an
/// aggregate handled by the recursive walker.
fn scalar_layout(kind: &TypeKind) -> Option<(u64, u64)> {
    match kind {
        TypeKind::Int
        | TypeKind::Float
        | TypeKind::Str
        | TypeKind::Null
        | TypeKind::Ptr(_)
        | TypeKind::Ref { .. } => Some((WORD_SIZE, WORD_SIZE)),
        TypeKind::Bool | TypeKind::Char => Some((1, 1)),
        TypeKind::Range(_) => Some((2 * WORD_SIZE, WORD_SIZE)),
        // Error and unresolved-inference types never reach a layout in a
        // clean pipeline (they only arise from earlier diagnostics); give
        // them a zero-size layout so validation can continue reporting
        // independent problems instead of stopping.
        TypeKind::Error | TypeKind::Infer(_) => Some((0, 1)),
        TypeKind::Unit => Some((0, 1)),
        TypeKind::Enum(_)
        | TypeKind::Struct(_)
        | TypeKind::Array { .. }
        | TypeKind::Fn { .. }
        | TypeKind::Tuple(_) => None,
    }
}

/// Computes the deterministic byte layout of the struct registered under
/// `id`, resolving field types through `types`.
///
/// Returns the layout, or a structured [`LayoutError`] when the struct is
/// recursive or oversized. The result is deterministic: field offsets
/// follow declaration order and the C-style alignment rules, so identical
/// declarations always yield identical layouts.
pub fn struct_layout(id: StructId, types: &TypeTable) -> Result<StructLayout, LayoutError> {
    let info = types
        .struct_info(id)
        .expect("struct ids always resolve in the owning table");
    if info.fields.is_empty() {
        return Err(LayoutError::Empty {
            name: info.name.clone(),
        });
    }
    let mut path = Vec::new();
    path.push(PathEntry::Struct(id));
    let mut fields = Vec::with_capacity(info.fields.len());
    let mut offset = 0u64;
    let mut max_align = 1u64;
    for field in &info.fields {
        let (size, align) = layout_of(types, field.ty, &mut path, &info.name)?;
        offset = round_up(offset, align).ok_or_else(|| LayoutError::Overflow {
            name: info.name.clone(),
        })?;
        fields.push(FieldLayout {
            offset,
            size,
            align,
        });
        offset = offset
            .checked_add(size)
            .ok_or_else(|| LayoutError::Overflow {
                name: info.name.clone(),
            })?;
        max_align = max_align.max(align);
    }
    path.pop();
    let size = round_up(offset, max_align).ok_or_else(|| LayoutError::Overflow {
        name: info.name.clone(),
    })?;
    if size > MAX_AGGREGATE_BYTES {
        return Err(LayoutError::TooLarge {
            name: info.name.clone(),
        });
    }
    Ok(StructLayout {
        size,
        align: max_align,
        fields,
    })
}

/// Computes the deterministic byte layout of the array type `array` (an
/// `Array { elem, len }` type).
///
/// Returns the layout, or a structured [`LayoutError`] when the element
/// type is recursive/oversized, the size computation overflows, or the
/// array exceeds [`MAX_AGGREGATE_BYTES`].
pub fn array_layout(array: TypeId, types: &TypeTable) -> Result<ArrayLayout, LayoutError> {
    let (elem, len) = match types.kind(array) {
        Some(TypeKind::Array { elem, len }) => (*elem, *len),
        // Defensive: only array types are laid out through this entry.
        _ => {
            return Err(LayoutError::Overflow {
                name: types.display(array),
            });
        }
    };
    let mut path = Vec::new();
    let (elem_size, elem_align) = layout_of(types, elem, &mut path, &types.display(array))?;
    let size = elem_size
        .checked_mul(len)
        .ok_or_else(|| LayoutError::Overflow {
            name: types.display(array),
        })?;
    if size > MAX_AGGREGATE_BYTES {
        return Err(LayoutError::TooLarge {
            name: types.display(array),
        });
    }
    Ok(ArrayLayout {
        size,
        align: elem_align,
        elem_size,
        elem_align,
        len,
    })
}

/// Computes the deterministic byte layout of a tuple type (session 29).
///
/// Returns the layout as a [`StructLayout`] (reusing the struct layout
/// model since tuples use the same C-style packing: fields in order,
/// padded for alignment). An empty tuple `()` is unit-sized (size 0,
/// alignment 1).
pub fn tuple_layout(elems: &[TypeId], types: &TypeTable) -> Result<StructLayout, LayoutError> {
    if elems.is_empty() {
        // Unit type: zero-sized, alignment 1.
        return Ok(StructLayout {
            size: 0,
            align: 1,
            fields: Vec::new(),
        });
    }
    let mut path = Vec::new();
    let mut fields = Vec::with_capacity(elems.len());
    let mut offset = 0u64;
    let mut max_align = 1u64;
    for elem_ty in elems {
        let (size, align) = layout_of(types, *elem_ty, &mut path, "tuple")?;
        offset = round_up(offset, align).ok_or_else(|| LayoutError::Overflow {
            name: "tuple".to_string(),
        })?;
        fields.push(FieldLayout {
            offset,
            size,
            align,
        });
        offset = offset
            .checked_add(size)
            .ok_or_else(|| LayoutError::Overflow {
                name: "tuple".to_string(),
            })?;
        max_align = max_align.max(align);
    }
    let size = round_up(offset, max_align).ok_or_else(|| LayoutError::Overflow {
        name: "tuple".to_string(),
    })?;
    if size > MAX_AGGREGATE_BYTES {
        return Err(LayoutError::TooLarge {
            name: "tuple".to_string(),
        });
    }
    Ok(StructLayout {
        size,
        align: max_align,
        fields,
    })
}

/// Computes the deterministic byte layout of the enum registered under
/// `id`, resolving payload types through `types`.
///
/// Returns the layout, or a structured [`LayoutError`] when a payload type
/// is recursive/oversized or the size computation overflows. A unit-only
/// enum is a single discriminant word; an enum with any data-carrying
/// variant is a tagged union: the discriminant word followed by a payload
/// area sized for the largest variant payload (see [`EnumLayout`]). The
/// result is deterministic: discriminants and payloads follow declaration
/// order and the C-style alignment rules, so identical declarations always
/// yield identical layouts.
pub fn enum_layout(id: EnumId, types: &TypeTable) -> Result<EnumLayout, LayoutError> {
    let mut path = Vec::new();
    path.push(PathEntry::Enum(id));
    enum_layout_inner(id, types, &mut path)
}

/// The tagged-union computation behind [`enum_layout`], sharing the
/// caller's by-value recursion path so mutually recursive payloads are
/// detected (the caller has already pushed this enum's id).
fn enum_layout_inner(
    id: EnumId,
    types: &TypeTable,
    path: &mut Vec<PathEntry>,
) -> Result<EnumLayout, LayoutError> {
    let info = types
        .enum_info(id)
        .expect("enum ids always resolve in the owning table");
    let mut variants = Vec::with_capacity(info.variants.len());
    let mut max_payload_size = 0u64;
    let mut max_payload_align = 1u64;
    let mut any_payload = false;
    for variant in &info.variants {
        let Some(payload_ty) = variant.payload else {
            variants.push(VariantPayloadLayout {
                discriminant: variant.discriminant,
                size: 0,
                align: 1,
            });
            continue;
        };
        any_payload = true;
        let (size, align) = layout_of(types, payload_ty, path, &info.name)?;
        variants.push(VariantPayloadLayout {
            discriminant: variant.discriminant,
            size,
            align,
        });
        max_payload_size = max_payload_size.max(size);
        max_payload_align = max_payload_align.max(align);
    }
    if !any_payload {
        // Session-17 layout: the value is the discriminant word.
        return Ok(EnumLayout {
            size: WORD_SIZE,
            align: WORD_SIZE,
            tagged: false,
            tag_offset: 0,
            payload_offset: 0,
            payload_size: 0,
            payload_align: 1,
            variants,
        });
    }
    // Tagged-union layout: the discriminant word, then the payload area
    // (shared by every variant), aligned to the largest payload alignment.
    let payload_offset =
        round_up(WORD_SIZE, max_payload_align).ok_or_else(|| LayoutError::Overflow {
            name: info.name.clone(),
        })?;
    let payload_size =
        round_up(max_payload_size, max_payload_align).ok_or_else(|| LayoutError::Overflow {
            name: info.name.clone(),
        })?;
    let size = payload_offset
        .checked_add(payload_size)
        .ok_or_else(|| LayoutError::Overflow {
            name: info.name.clone(),
        })?;
    if size > MAX_AGGREGATE_BYTES {
        return Err(LayoutError::TooLarge {
            name: info.name.clone(),
        });
    }
    Ok(EnumLayout {
        size,
        align: WORD_SIZE.max(max_payload_align),
        tagged: true,
        tag_offset: 0,
        payload_offset,
        payload_size,
        payload_align: max_payload_align,
        variants,
    })
}

/// The scalar size/alignment of `ty`'s canonical kind, if `ty` is a scalar
/// type (aggregates return `None`).
///
/// An enum is scalar only when every variant is a unit variant (its value
/// is the discriminant word, session 17); an enum with a data-carrying
/// variant (session 19) is a tagged union, an aggregate handled by
/// [`enum_layout`].
pub fn scalar_size_align(types: &TypeTable, ty: TypeId) -> Option<(u64, u64)> {
    match types.kind(ty)? {
        TypeKind::Enum(id) => {
            let info = types.enum_info(*id)?;
            if info
                .variants
                .iter()
                .all(|variant| variant.payload.is_none())
            {
                Some((WORD_SIZE, WORD_SIZE))
            } else {
                None
            }
        }
        other => scalar_layout(other),
    }
}

/// Rounds `value` up to `alignment` (checked).
fn round_up(value: u64, alignment: u64) -> Option<u64> {
    value
        .checked_add(alignment - 1)
        .map(|v| v & !(alignment - 1))
}

/// Computes the byte size and alignment of the value of type `ty`,
/// tracking the struct ids on the current by-value path in `path`
/// (recursion detection) and applying the aggregate size bound.
fn layout_of(
    types: &TypeTable,
    ty: TypeId,
    path: &mut Vec<PathEntry>,
    owner: &str,
) -> Result<(u64, u64), LayoutError> {
    match types.kind(ty).cloned() {
        Some(TypeKind::Struct(id)) => {
            let info = types.struct_info(id).ok_or_else(|| LayoutError::Overflow {
                name: owner.to_string(),
            })?;
            if info.fields.is_empty() {
                return Err(LayoutError::Empty {
                    name: info.name.clone(),
                });
            }
            if path.contains(&PathEntry::Struct(id)) {
                return Err(LayoutError::Recursive {
                    name: info.name.clone(),
                });
            }
            path.push(PathEntry::Struct(id));
            let mut offset = 0u64;
            let mut max_align = 1u64;
            for field in &info.fields {
                let (size, align) = layout_of(types, field.ty, path, &info.name)?;
                offset = round_up(offset, align).ok_or_else(|| LayoutError::Overflow {
                    name: info.name.clone(),
                })?;
                offset = offset
                    .checked_add(size)
                    .ok_or_else(|| LayoutError::Overflow {
                        name: info.name.clone(),
                    })?;
                max_align = max_align.max(align);
            }
            path.pop();
            let size = round_up(offset, max_align).ok_or_else(|| LayoutError::Overflow {
                name: info.name.clone(),
            })?;
            if size > MAX_AGGREGATE_BYTES {
                return Err(LayoutError::TooLarge {
                    name: info.name.clone(),
                });
            }
            Ok((size, max_align))
        }
        Some(TypeKind::Array { elem, len }) => {
            let name = types.display(ty);
            let (elem_size, elem_align) = layout_of(types, elem, path, &name)?;
            let size = elem_size
                .checked_mul(len)
                .ok_or_else(|| LayoutError::Overflow { name: name.clone() })?;
            if size > MAX_AGGREGATE_BYTES {
                return Err(LayoutError::TooLarge { name });
            }
            Ok((size, elem_align))
        }
        Some(TypeKind::Enum(id)) => {
            let info = types.enum_info(id).ok_or_else(|| LayoutError::Overflow {
                name: owner.to_string(),
            })?;
            // A unit-only enum is a single discriminant word; an enum with
            // a data-carrying variant is a tagged union (recursion through
            // the payload is detected by the path tracking).
            if info
                .variants
                .iter()
                .all(|variant| variant.payload.is_none())
            {
                return Ok((WORD_SIZE, WORD_SIZE));
            }
            if path.contains(&PathEntry::Enum(id)) {
                return Err(LayoutError::Recursive {
                    name: info.name.clone(),
                });
            }
            path.push(PathEntry::Enum(id));
            let layout = enum_layout_inner(id, types, path)?;
            path.pop();
            Ok((layout.size, layout.align))
        }
        Some(other) => scalar_layout(&other).ok_or_else(|| LayoutError::Overflow {
            name: types.display(ty),
        }),
        None => Err(LayoutError::Overflow {
            name: owner.to_string(),
        }),
    }
}
