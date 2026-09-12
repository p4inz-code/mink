//! Collection element descriptors (Session 101, Wave B).
//!
//! Every `Vec<T>`, `Map<K, V>`, and `Set<T>` buffer embeds the byte size
//! of its elements plus the id of a **collection descriptor** in a
//! per-program static table. The descriptor drives the runtime's
//! element-stride arithmetic, the recursive ownership free (a collection
//! frees its elements when freed), the deep clone used by enumeration
//! (`rt_map_keys`/`rt_map_values`/`rt_set_elements`), and the key hashing
//! and equality contract of `Map`/`Set`.
//!
//! Descriptor entry layout (16 bytes each, little-endian):
//!
//! ```text
//! [0..4]  elem_size: u32    bytes per element (always a multiple of 8)
//! [4]     tag: u8           0 plain, 1 str, 2 vec, 3 map, 4 set,
//!                           5 struct, 6 enum, 7 array
//! [5]     hash: u8          0 none, 1 word, 2 str-bytes
//! [6..8]  pad
//! [8..12] ref: u32          vec/set/array -> element desc; map -> key desc;
//!                           enum -> unused (NONE)
//! [12..16] ref2: u32        map -> value desc; struct/enum -> field/variant
//!                           list byte offset; array -> element count
//! ```
//!
//! Field/variant lists (byte blobs, referenced by byte offset):
//!
//! ```text
//! struct:  [count u32][pad u32][(offset u32, desc u32) * count]
//! enum:    [count u32][tag_offset u32][payload_offset u32]
//!          [discriminant i64, payload_desc u32, pad u32] * count
//! ```
//!
//! The table is deterministic: entries are created in first-use order
//! (source order), and identical types share one entry via a cache.

use std::collections::HashMap;

use crate::runtime::layout;
use crate::typecheck::{TypeId, TypeKind, TypeTable};

/// Sentinel: a descriptor field that references nothing.
pub(crate) const DESC_NONE: u32 = u32::MAX;

// Descriptor tags.
pub(crate) const TAG_PLAIN: u8 = 0;
pub(crate) const TAG_STR: u8 = 1;
pub(crate) const TAG_VEC: u8 = 2;
pub(crate) const TAG_MAP: u8 = 3;
pub(crate) const TAG_SET: u8 = 4;
pub(crate) const TAG_STRUCT: u8 = 5;
pub(crate) const TAG_ENUM: u8 = 6;
pub(crate) const TAG_ARRAY: u8 = 7;

// Key hashing/equality kinds.
pub(crate) const HASH_NONE: u8 = 0;
pub(crate) const HASH_WORD: u8 = 1;
pub(crate) const HASH_STR: u8 = 2;

/// The decoded form of one descriptor entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CollDesc {
    pub(crate) elem_size: u32,
    pub(crate) tag: u8,
    pub(crate) hash: u8,
    pub(crate) ref_: u32,
    pub(crate) ref2: u32,
}

/// Builds the per-program descriptor table from the program's type table.
pub(crate) struct DescBuilder<'a> {
    types: &'a TypeTable,
    entries: Vec<CollDesc>,
    /// Byte blobs: one per struct field list or enum variant list.
    field_lists: Vec<Vec<u8>>,
    cache: HashMap<TypeId, u32>,
}

impl<'a> DescBuilder<'a> {
    pub(crate) fn new(types: &'a TypeTable) -> Self {
        Self {
            types,
            entries: Vec::new(),
            field_lists: Vec::new(),
            cache: HashMap::new(),
        }
    }

    /// The descriptor id for `ty`, creating the entry (and any recursive
    /// entries) on first use. Errors when `ty` has no finite layout (only
    /// reachable from already-rejected programs).
    pub(crate) fn desc(&mut self, ty: TypeId) -> Result<u32, String> {
        let ty = self.types.canonical(ty);
        if let Some(&id) = self.cache.get(&ty) {
            return Ok(id);
        }
        let entry = self.build(ty)?;
        let id = self.entries.len() as u32;
        self.entries.push(entry);
        self.cache.insert(ty, id);
        Ok(id)
    }

    /// The descriptor id of the legacy word-element shape: an 8-byte
    /// `TAG_PLAIN` word-hashable value. Used when an unannotated
    /// `rt_vec_new`/`rt_map_new`/`rt_set_new` call leaves the element or
    /// key type an unresolved inference variable (Session 101 backward
    /// compatibility with the Session 41/57 word-element contract). The
    /// id is cached so repeated fallbacks share one entry.
    pub(crate) fn desc_word(&mut self) -> u32 {
        const WORD_KEY: u32 = u32::MAX - 1; // never a real TypeId raw value
        if let Some(&id) = self.cache.get(&TypeId::new(WORD_KEY)) {
            return id;
        }
        let entry = CollDesc {
            elem_size: 8,
            tag: TAG_PLAIN,
            hash: HASH_WORD,
            ref_: DESC_NONE,
            ref2: DESC_NONE,
        };
        let id = self.entries.len() as u32;
        self.entries.push(entry);
        self.cache.insert(TypeId::new(WORD_KEY), id);
        id
    }

    /// The serialized descriptor entries (16 bytes each).
    pub(crate) fn entries_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.entries.len() * 16);
        for entry in &self.entries {
            out.extend_from_slice(&entry.elem_size.to_le_bytes());
            out.push(entry.tag);
            out.push(entry.hash);
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&entry.ref_.to_le_bytes());
            out.extend_from_slice(&entry.ref2.to_le_bytes());
        }
        out
    }

    /// The serialized field/variant lists, concatenated; a descriptor's
    /// list index is its byte offset into this region.
    pub(crate) fn lists_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for list in &self.field_lists {
            out.extend_from_slice(list);
        }
        out
    }

    /// The complete serialized table: (descriptor entries, field lists).
    pub(crate) fn into_bytes(self) -> (Vec<u8>, Vec<u8>) {
        (self.entries_bytes(), self.lists_bytes())
    }

    /// Appends a field list and returns its byte offset into the lists
    /// region.
    fn push_field_list(&mut self, bytes: Vec<u8>) -> u32 {
        let offset = self.field_lists.iter().map(|l| l.len()).sum::<usize>() as u32;
        self.field_lists.push(bytes);
        offset
    }

    fn build(&mut self, ty: TypeId) -> Result<CollDesc, String> {
        use TypeKind as K;
        match self.types.kind(ty) {
            Some(K::Int | K::Bool | K::Char) => Ok(CollDesc {
                elem_size: 8,
                tag: TAG_PLAIN,
                hash: HASH_WORD,
                ref_: DESC_NONE,
                ref2: DESC_NONE,
            }),
            Some(K::Float | K::Ptr(_) | K::Ref { .. } | K::Null | K::Fn { .. }) => Ok(CollDesc {
                elem_size: 8,
                tag: TAG_PLAIN,
                hash: HASH_NONE,
                ref_: DESC_NONE,
                ref2: DESC_NONE,
            }),
            Some(K::Str) => Ok(CollDesc {
                elem_size: 8,
                tag: TAG_STR,
                hash: HASH_STR,
                ref_: DESC_NONE,
                ref2: DESC_NONE,
            }),
            Some(K::Range(_)) => Ok(CollDesc {
                elem_size: 16,
                tag: TAG_PLAIN,
                hash: HASH_NONE,
                ref_: DESC_NONE,
                ref2: DESC_NONE,
            }),
            Some(K::Vec(elem)) => Ok(CollDesc {
                elem_size: 8,
                tag: TAG_VEC,
                hash: HASH_NONE,
                ref_: self.desc(*elem)?,
                ref2: DESC_NONE,
            }),
            Some(K::Set(elem)) => Ok(CollDesc {
                elem_size: 8,
                tag: TAG_SET,
                hash: HASH_NONE,
                ref_: self.desc(*elem)?,
                ref2: DESC_NONE,
            }),
            Some(K::Map(key, value)) => Ok(CollDesc {
                elem_size: 8,
                tag: TAG_MAP,
                hash: HASH_NONE,
                ref_: self.desc(*key)?,
                ref2: self.desc(*value)?,
            }),
            Some(K::Struct(id)) => {
                let layout =
                    layout::struct_layout(*id, self.types).map_err(|e| layout_error_message(&e))?;
                let info = self
                    .types
                    .struct_info(*id)
                    .expect("struct ids always resolve in the owning table");
                let mut bytes = Vec::with_capacity(8 + 8 * info.fields.len());
                bytes.extend_from_slice(&(info.fields.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&0u32.to_le_bytes());
                for (field, field_layout) in info.fields.iter().zip(&layout.fields) {
                    bytes.extend_from_slice(&(field_layout.offset as u32).to_le_bytes());
                    bytes.extend_from_slice(&self.desc(field.ty)?.to_le_bytes());
                }
                Ok(CollDesc {
                    elem_size: layout.size as u32,
                    tag: TAG_STRUCT,
                    hash: HASH_NONE,
                    ref_: DESC_NONE,
                    ref2: self.push_field_list(bytes),
                })
            }
            Some(K::Tuple(elems)) => {
                let layout = layout::tuple_layout(elems, self.types)
                    .map_err(|e| layout_error_message(&e))?;
                let mut bytes = Vec::with_capacity(8 + 8 * elems.len());
                bytes.extend_from_slice(&(elems.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&0u32.to_le_bytes());
                for (elem, field_layout) in elems.iter().zip(&layout.fields) {
                    bytes.extend_from_slice(&(field_layout.offset as u32).to_le_bytes());
                    bytes.extend_from_slice(&self.desc(*elem)?.to_le_bytes());
                }
                Ok(CollDesc {
                    elem_size: layout.size as u32,
                    tag: TAG_STRUCT,
                    hash: HASH_NONE,
                    ref_: DESC_NONE,
                    ref2: self.push_field_list(bytes),
                })
            }
            Some(K::Enum(id)) => {
                let info = self
                    .types
                    .enum_info(*id)
                    .expect("enum ids always resolve in the owning table");
                let layout =
                    layout::enum_layout(*id, self.types).map_err(|e| layout_error_message(&e))?;
                if !layout.tagged {
                    // Unit-only enum: a single word holding the
                    // discriminant; word-hashable as a Map/Set key.
                    return Ok(CollDesc {
                        elem_size: 8,
                        tag: TAG_ENUM,
                        hash: HASH_WORD,
                        ref_: DESC_NONE,
                        ref2: DESC_NONE,
                    });
                }
                let mut bytes = Vec::with_capacity(12 + 16 * info.variants.len());
                bytes.extend_from_slice(&(info.variants.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&(layout.tag_offset as u32).to_le_bytes());
                bytes.extend_from_slice(&(layout.payload_offset as u32).to_le_bytes());
                for variant in &info.variants {
                    let desc = match variant.payload {
                        Some(payload_ty) => self.desc(payload_ty)?,
                        None => DESC_NONE,
                    };
                    bytes.extend_from_slice(&variant.discriminant.to_le_bytes());
                    bytes.extend_from_slice(&desc.to_le_bytes());
                    bytes.extend_from_slice(&0u32.to_le_bytes());
                }
                Ok(CollDesc {
                    elem_size: layout.size as u32,
                    tag: TAG_ENUM,
                    hash: HASH_NONE,
                    ref_: DESC_NONE,
                    ref2: self.push_field_list(bytes),
                })
            }
            Some(K::Array { elem, len }) => {
                let layout =
                    layout::array_layout(ty, self.types).map_err(|e| layout_error_message(&e))?;
                Ok(CollDesc {
                    elem_size: layout.size as u32,
                    tag: TAG_ARRAY,
                    hash: HASH_NONE,
                    ref_: self.desc(*elem)?,
                    ref2: *len as u32,
                })
            }
            Some(K::Unit | K::Never | K::Infer(_) | K::Error) | None => Err(format!(
                "type `{}` has no collection descriptor",
                self.types.display(ty)
            )),
        }
    }
}

/// The human-readable reason a layout failed, for diagnostics.
fn layout_error_message(error: &layout::LayoutError) -> String {
    match error {
        layout::LayoutError::Recursive { name } => {
            format!("type `{name}` is recursive and has no finite layout")
        }
        layout::LayoutError::Empty { name } => {
            format!("type `{name}` has no fields, so it has no layout")
        }
        layout::LayoutError::Overflow { name } => {
            format!("the layout of `{name}` overflows the memory model")
        }
        layout::LayoutError::TooLarge { name } => {
            format!("the layout of `{name}` exceeds the maximum aggregate size")
        }
    }
}
