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
use crate::typecheck::{StructId, TypeId, TypeKind, TypeTable};

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

/// The byte size and alignment of a scalar type. Aggregate and function
/// types return `None` (they are handled by the recursive layout walker).
fn scalar_layout(kind: &TypeKind) -> Option<(u64, u64)> {
    match kind {
        TypeKind::Int
        | TypeKind::Float
        | TypeKind::Str
        | TypeKind::Null
        | TypeKind::Ptr(_)
        | TypeKind::Ref { .. }
        | TypeKind::Enum(_) => Some((WORD_SIZE, WORD_SIZE)),
        TypeKind::Bool | TypeKind::Char => Some((1, 1)),
        TypeKind::Range(_) => Some((2 * WORD_SIZE, WORD_SIZE)),
        // Error and unresolved-inference types never reach a layout in a
        // clean pipeline (they only arise from earlier diagnostics); give
        // them a zero-size layout so validation can continue reporting
        // independent problems instead of stopping.
        TypeKind::Error | TypeKind::Infer(_) => Some((0, 1)),
        TypeKind::Unit => Some((0, 1)),
        TypeKind::Struct(_) | TypeKind::Array { .. } | TypeKind::Fn { .. } => None,
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
    path.push(id.raw());
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

/// The scalar size/alignment of `ty`'s canonical kind, if `ty` is a scalar
/// type (aggregates return `None`).
pub fn scalar_size_align(types: &TypeTable, ty: TypeId) -> Option<(u64, u64)> {
    scalar_layout(types.kind(ty)?)
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
    path: &mut Vec<u32>,
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
            if path.contains(&id.raw()) {
                return Err(LayoutError::Recursive {
                    name: info.name.clone(),
                });
            }
            path.push(id.raw());
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
        Some(other) => scalar_layout(&other).ok_or_else(|| LayoutError::Overflow {
            name: types.display(ty),
        }),
        None => Err(LayoutError::Overflow {
            name: owner.to_string(),
        }),
    }
}
