//! Optimized MIR → backend-instruction lowering.
//!
//! The [`lower`] entry point walks the optimized [`MirProgram`] once and
//! produces the target-independent [`BProgram`] that emitters consume. It
//! never re-runs name resolution, type checking, or MIR analysis: it only
//! consumes the answers MIR already carries — classified types, locals,
//! operands, and control flow.
//!
//! The first native subset deliberately supports a small scalar core:
//!
//! - **types** — `Int` (64-bit), `Bool`, `Range<Int>` as a two-word value,
//!   and unit (a function that produces no value);
//! - **values** — integer and boolean literals (decoded from the source
//!   text; the backend is the first stage to decode literal values),
//!   local loads, module-binding loads and stores;
//! - **operations** — arithmetic (`+ - * / %`), shifts (`<< >>`), bitwise
//!   (`& ^ | ~`), comparisons and equality (`< <= > >= == !=`), logical
//!   (`&& || !`), negation (`-`), range construction and iteration, and
//!   direct function calls;
//! - **control flow** — `if`/`else`, `while`, `for` over ranges, `loop`,
//!   `break`, `continue`, and `return`.
//!
//! Everything else — floating-point, strings, characters, `null`, member
//! and index places, function values, module bindings that need runtime
//! initialization — is **rejected with a structured error**
//! ([`BackendError`], `E-B01`…) instead of being silently miscompiled.
//! Lowering reports every independent problem in deterministic source
//! order; a single error keeps the program from reaching an emitter.
//!
//! Lowering is deterministic (source order) and defensive: statements that
//! touch a value whose type is unsupported are skipped, because their error
//! was already reported when the value's type was classified, so one root
//! cause never cascades into a swarm of diagnostics.

use std::collections::HashMap;

use crate::mir::{
    MirConstantKind, MirFn, MirItemKind, MirOperand, MirOperandKind, MirProgram, MirRvalueKind,
    MirStatic, MirStmtKind, MirTargetKind, MirTerminator,
};
use crate::runtime::layout;
use crate::semantics::SymbolId;
use crate::source::{SourceMap, Span};
use crate::typecheck::{EnumId, StructId, TypeId, TypeKind, layout_error_message};

use super::error::BackendError;
use super::ir::{
    BBlock, BFunction, BInst, BInstKind, BLocal, BOperand, BProgram, BStatic, BString, BTerminator,
    BType, LowerResult, RuntimeService,
};

/// The resolved target of a call: a user function or an embedded runtime
/// service.
enum Callee {
    /// A user function, by index into [`BProgram::functions`].
    Function(usize),
    /// A runtime service (`rt_*` intrinsic).
    Runtime(RuntimeService),
}

/// The resolved storage root of a multi-step place assignment: a function
/// local slot, or a module binding that must be read-modify-written.
#[derive(Clone, Copy)]
enum RootHandling {
    /// A local slot; the place store writes it directly.
    Local(crate::mir::LocalId),
    /// A module binding: the value is loaded into a temporary, written,
    /// and stored back. `ty` is the binding's MIR type (the temporary's
    /// shape for place-step resolution).
    Static { slot: usize, ty: TypeId },
}

/// A decoded literal constant: a machine word or a string-blob reference.
///
/// String literals have no machine constant form — their value is the
/// address of the decoded blob in the image — so they lower to
/// [`BInstKind::LoadStr`] instead of [`BInstKind::LoadConst`].
enum DecodedConstant {
    /// An integer or boolean value, decoded to its 64-bit machine value.
    Word(i64),
    /// A string literal: an index into [`BProgram::strings`].
    Str(usize),
}

/// The value of an ASCII hex digit, or a structured decode error.
fn hex_digit(byte: u8, span: Span) -> Result<u8, BackendError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(BackendError::decode_error(
            span,
            "invalid hex digit in string escape",
        )),
    }
}

/// The word count a slot needs so that the slot that follows it never
/// overwrites this value's lowest bytes.
///
/// Aggregate value bytes sit in normal byte order within each 8-byte
/// chunk, and chunks are stacked downward from the slot's first word:
/// byte `b` of a value lives at `word0 - 8*(b/8) + (b%8)`, so chunk `k`
/// occupies `[word0 - 8k, word0 - 8k + 7]` and sub-word pieces (booleans,
/// sub-word elements and tails) stay inside their own chunk. `bottom` is
/// the lowest byte offset (below `word0`) the value can reach; the next
/// slot is placed `8 * words` bytes below `word0` and its first full word
/// covers `[word0 - 8 * words, word0 - 8 * words + 7]`, so the regions
/// stay disjoint when `8 * words >= bottom + 8`. The conservative
/// `bottom + 8` bound means a pure sub-word value (an all-bool struct, a
/// `[Bool; N]` array) still receives one guard word of padding even
/// though its bytes now stay within `ceil(size / 8)` chunks — harmless
/// slack that keeps slot sizing independent of the value's contents.
fn value_bottom_words(size: u64, bottom: u64) -> u64 {
    let _ = size;
    (bottom + 8).div_ceil(8)
}

/// The lowest byte offset (below a value's first word) a struct occupies,
/// computed conservatively: the maximum, over fields, of `offset +
/// size - 1` for sub-word fields and `offset + size - 8` for full-word
/// fields (a full-word field's qword starts at `word0 - offset` and
/// covers `[word0 - offset, word0 - offset + 7]`). This is always at
/// least the new chunked convention's bottom (`8 * (ceil(size/8) - 1)`),
/// so slot sizing stays valid for both.
fn struct_bottom_offset(layout: &layout::StructLayout) -> u64 {
    layout
        .fields
        .iter()
        .map(|field| {
            let tail = if field.size % 8 == 0 {
                field.size - 8
            } else {
                field.size - 1
            };
            field.offset + tail
        })
        .max()
        .unwrap_or(0)
}

/// The lowest byte offset (below a value's first word) an array occupies,
/// computed conservatively (see [`struct_bottom_offset`]): element `k`
/// starts at `word0 - k * elem_size`; a full-word element's final qword
/// bottoms out at `elem_size - 8`, a sub-word element's final byte at
/// `elem_size - 1`.
fn array_bottom_offset(layout: &layout::ArrayLayout) -> u64 {
    let tail = if layout.elem_size % 8 == 0 {
        layout.elem_size - 8
    } else {
        layout.elem_size - 1
    };
    layout.len.saturating_sub(1) * layout.elem_size + tail
}

/// The lowest byte offset (below a value's first word) a tagged-union
/// enum occupies, computed conservatively: the discriminant word starts
/// at `word0 - tag_offset`, and the payload area behaves like a struct
/// field at `payload_offset`.
fn enum_bottom_offset(layout: &layout::EnumLayout) -> u64 {
    let tag = layout.tag_offset; // the tag is a full word
    let payload = layout.payload_offset
        + if layout.payload_size % 8 == 0 {
            layout.payload_size - 8
        } else {
            layout.payload_size - 1
        };
    tag.max(payload)
}

/// Lowers an optimized [`MirProgram`] into a [`BProgram`].
///
/// Returns the lowered program, or every [`BackendError`] collected in
/// deterministic order when the input contains constructs outside the
/// native subset (or is internally inconsistent).
pub(crate) fn lower(program: &MirProgram, sources: &SourceMap) -> LowerResult {
    let mut lowerer = Lowerer::new(program, sources);
    lowerer.run();
    if lowerer.errors.is_empty() {
        Ok(BProgram {
            functions: lowerer.functions,
            statics: lowerer.statics,
            strings: lowerer.strings,
        })
    } else {
        Err(lowerer.errors)
    }
}

/// The program-wide lowering traversal.
struct Lowerer<'a> {
    program: &'a MirProgram,
    sources: &'a SourceMap,
    /// Symbol → function index, for `Static` operands that name functions.
    fn_index: HashMap<SymbolId, usize>,
    /// Symbol → static slot, for `Static` operands that name module
    /// bindings. Slots cover every module binding (supported or not) so
    /// references always resolve; unsupported bindings are skipped.
    static_slots: HashMap<SymbolId, usize>,
    /// The classification of each static slot; `None` means the binding's
    /// type was already rejected.
    static_types: Vec<Option<BType>>,
    /// The MIR type id of each static slot, for resolving aggregate
    /// structure (place steps, value images) during lowering.
    static_mir_types: Vec<TypeId>,
    /// The decoded value image of each successfully lowered binding (slot
    /// index → image bytes), so a later binding's constant initializer may
    /// reference an earlier one by copying its image.
    static_images: HashMap<usize, Vec<u8>>,
    functions: Vec<BFunction>,
    statics: Vec<BStatic>,
    /// The decoded string literals, in first-use order.
    strings: Vec<BString>,
    /// Bytes → string index, so identical literals share one image blob
    /// (deterministic deduplication).
    string_index: HashMap<Vec<u8>, usize>,
    errors: Vec<BackendError>,
    /// Locals of the function currently being lowered (a copy of the MIR
    /// function's locals, classified, plus any lowering temporaries).
    fn_locals: Vec<BLocal>,
    /// Parallel to `fn_locals`: the classification of each local; `None`
    /// means the local's type was already rejected.
    fn_classified: Vec<Option<BType>>,
    /// Parallel to `fn_locals`: the MIR type id of each local, for
    /// resolving aggregate structure (place steps) during instruction
    /// lowering.
    fn_local_types: Vec<TypeId>,
    /// The instruction buffer of the block currently being lowered.
    fn_insts: Vec<BInst>,
    /// Why a struct/array type was rejected: type id → reason, recorded
    /// when classification fails so diagnostics can explain the failure.
    unsupported_aggregates: HashMap<TypeId, String>,
}

impl<'a> Lowerer<'a> {
    fn new(program: &'a MirProgram, sources: &'a SourceMap) -> Self {
        let mut fn_index = HashMap::new();
        let mut static_slots = HashMap::new();
        let mut fn_count = 0;
        let mut static_count = 0;
        for item in &program.items {
            match &item.kind {
                MirItemKind::Fn(f) => {
                    fn_index.insert(f.name.symbol, fn_count);
                    fn_count += 1;
                }
                MirItemKind::Let(binding) | MirItemKind::Const(binding) => {
                    static_slots.insert(binding.name.symbol, static_count);
                    static_count += 1;
                }
            }
        }
        Self {
            program,
            sources,
            fn_index,
            static_slots,
            static_types: Vec::new(),
            static_mir_types: Vec::new(),
            static_images: HashMap::new(),
            functions: Vec::new(),
            statics: Vec::new(),
            strings: Vec::new(),
            string_index: HashMap::new(),
            errors: Vec::new(),
            fn_locals: Vec::new(),
            fn_classified: Vec::new(),
            fn_local_types: Vec::new(),
            fn_insts: Vec::new(),
            unsupported_aggregates: HashMap::new(),
        }
    }

    fn run(&mut self) {
        // Pre-pass: classify every module binding so function bodies can
        // resolve (and skip) references to unsupported bindings.
        for item in &self.program.items {
            if let MirItemKind::Let(binding) | MirItemKind::Const(binding) = &item.kind {
                self.classify_static(binding);
            }
        }
        // Lower items in source order.
        for item in &self.program.items {
            match &item.kind {
                MirItemKind::Fn(f) => self.lower_fn(f),
                MirItemKind::Let(binding) | MirItemKind::Const(binding) => {
                    self.lower_static(binding);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Classification
    // ------------------------------------------------------------------

    /// Classifies a MIR type id into a backend value type.
    ///
    /// `Int`, `Bool`, `Float`, `Char`, `Null`, `Range<Int>`, `Ptr<Int>`,
    /// and `Str` are representable (session 24 adds the three remaining
    /// scalar types); an unresolved inference type is unit (a value that
    /// is never meaningfully read); structs and arrays are representable
    /// when their deterministic layout is finite and every field/element
    /// type is itself representable (a failed aggregate records its reason
    /// in [`Lowerer::unsupported_aggregates`] for the diagnostic);
    /// everything else (the error type, `Ptr` over another element,
    /// `Range` over another element, function types) is unsupported and
    /// classifies to `None`.
    fn classify(&mut self, ty: TypeId) -> Option<BType> {
        match self.program.types.kind(ty) {
            Some(TypeKind::Int) => Some(BType::Int),
            Some(TypeKind::Bool) => Some(BType::Bool),
            Some(TypeKind::Float) => Some(BType::Float),
            Some(TypeKind::Char) => Some(BType::Char),
            Some(TypeKind::Null) => Some(BType::Null),
            Some(TypeKind::Ptr(elem)) => match self.program.types.kind(*elem) {
                Some(TypeKind::Int) => Some(BType::Ptr),
                _ => None,
            },
            Some(TypeKind::Str) => Some(BType::Str),
            // References (session 16) are word-sized addresses regardless
            // of mutability; the referent's shape is carried by the
            // load/store instructions (`RefLoad`/`RefStore`), not the
            // reference's own type.
            Some(TypeKind::Ref { elem, .. }) => match self.classify(*elem) {
                Some(BType::Unit) | None => None,
                Some(_) => Some(BType::Ref),
            },
            Some(TypeKind::Range(elem)) => match self.program.types.kind(*elem) {
                Some(TypeKind::Int) => Some(BType::Range),
                _ => None,
            },
            Some(TypeKind::Struct(id)) => self.classify_struct(ty, *id),
            Some(TypeKind::Array { .. }) => self.classify_array(ty),
            Some(TypeKind::Tuple(elems)) => self.classify_tuple(ty, elems),
            // Enums (session 17) with only unit variants are single-word
            // discriminant values. An enum with a data-carrying variant
            // (session 19) is a tagged union: representable when its
            // layout is finite and every payload type is itself
            // representable (a failed enum records its reason for the
            // diagnostic, mirroring `classify_struct`).
            Some(TypeKind::Enum(id)) => self.classify_enum(ty, *id),
            // `kind` follows resolved inference chains; an unresolved
            // variable is the only `Infer` that remains. Unit is the type
            // of intrinsics that produce no value.
            Some(TypeKind::Infer(_)) | Some(TypeKind::Unit) => Some(BType::Unit),
            Some(TypeKind::Error | TypeKind::Fn { .. }) | None => None,
        }
    }

    /// Classifies a struct type: it must have a finite deterministic layout
    /// (not recursive, not empty, not oversized) and every declared field
    /// type must itself classify. On failure the reason is recorded for the
    /// diagnostic and `None` is returned. Recursion is bounded by the
    /// layout engine's cycle detection, which runs before the field walk.
    fn classify_struct(&mut self, ty: TypeId, id: StructId) -> Option<BType> {
        let reason = match layout::struct_layout(id, &self.program.types) {
            Err(error) => Some(layout_error_message(&error)),
            Ok(_) => {
                let info = self
                    .program
                    .types
                    .struct_info(id)
                    .expect("struct ids always resolve");
                let mut bad: Option<String> = None;
                for field in &info.fields {
                    if self.classify(field.ty).is_none() {
                        bad = Some(format!(
                            "its field `{}` has an unsupported type `{}`",
                            field.name,
                            self.display(field.ty)
                        ));
                        break;
                    }
                }
                bad
            }
        };
        match reason {
            None => Some(BType::Struct),
            Some(reason) => {
                self.unsupported_aggregates.insert(ty, reason);
                None
            }
        }
    }

    /// Classifies an enum type: a unit-only enum is a single-word
    /// discriminant value; an enum with a data-carrying variant must have
    /// a finite tagged-union layout and every payload type must itself
    /// classify. On failure the reason is recorded for the diagnostic and
    /// `None` is returned.
    fn classify_enum(&mut self, ty: TypeId, id: EnumId) -> Option<BType> {
        let info = self
            .program
            .types
            .enum_info(id)
            .expect("enum ids always resolve");
        if info
            .variants
            .iter()
            .all(|variant| variant.payload.is_none())
        {
            return Some(BType::Enum);
        }
        let reason = match layout::enum_layout(id, &self.program.types) {
            Err(error) => Some(layout_error_message(&error)),
            Ok(_) => {
                let mut bad: Option<String> = None;
                for variant in &info.variants {
                    if let Some(payload_ty) = variant.payload {
                        if self.classify(payload_ty).is_none() {
                            bad = Some(format!(
                                "its variant `{}` has an unsupported payload type `{}`",
                                variant.name,
                                self.display(payload_ty)
                            ));
                            break;
                        }
                    }
                }
                bad
            }
        };
        match reason {
            None => Some(BType::Enum),
            Some(reason) => {
                self.unsupported_aggregates.insert(ty, reason);
                None
            }
        }
    }

    /// Classifies an array type: it must have a finite layout within the
    /// runtime memory model and its element type must classify. On failure
    /// the reason is recorded and `None` is returned.
    fn classify_array(&mut self, ty: TypeId) -> Option<BType> {
        let reason = match layout::array_layout(ty, &self.program.types) {
            Err(error) => Some(layout_error_message(&error)),
            Ok(_) => {
                let elem = match self.program.types.kind(ty) {
                    Some(TypeKind::Array { elem, .. }) => *elem,
                    _ => return None,
                };
                if self.classify(elem).is_none() {
                    Some(format!(
                        "its element type `{}` is not supported",
                        self.display(elem)
                    ))
                } else {
                    None
                }
            }
        };
        match reason {
            None => Some(BType::Array),
            Some(reason) => {
                self.unsupported_aggregates.insert(ty, reason);
                None
            }
        }
    }

    /// Classifies a tuple type (session 29): it must have a finite
    /// deterministic layout and every element type must classify.
    fn classify_tuple(&mut self, ty: TypeId, elems: &[TypeId]) -> Option<BType> {
        let reason = match layout::tuple_layout(elems, &self.program.types) {
            Err(error) => Some(layout_error_message(&error)),
            Ok(_) => {
                // Check that every element type is supported.
                elems
                    .iter()
                    .find(|e| self.classify(**e).is_none())
                    .map(|e| format!("its element type `{}` is not supported", self.display(*e)))
            }
        };
        match reason {
            None => Some(BType::Struct), // tuples reuse struct layout
            Some(reason) => {
                self.unsupported_aggregates.insert(ty, reason);
                None
            }
        }
    }

    /// The number of stack words a value of `ty` (classified as
    /// `classified`) occupies: `ceil(size / 8)` for aggregates, the scalar
    /// width otherwise. A tagged-union enum (one with a data-carrying
    /// variant, session 19) spans its layout's words; a unit-only enum is
    /// one discriminant word.
    fn words_of(&self, ty: TypeId, classified: BType) -> u32 {
        match classified {
            BType::Struct | BType::Array => self.aggregate_words(ty),
            BType::Enum => {
                if let Some(id) = self.program.types.enum_id(ty) {
                    match layout::enum_layout(id, &self.program.types) {
                        Ok(layout) if layout.tagged => {
                            (value_bottom_words(layout.size, enum_bottom_offset(&layout))).max(1)
                                as u32
                        }
                        _ => 1,
                    }
                } else {
                    1
                }
            }
            _ => classified.words() as u32,
        }
    }

    /// The stack word count of an aggregate-typed value, from its layout.
    /// A validated aggregate always has a layout; the fallback keeps a
    /// defensive path from panicking on inconsistent input.
    fn aggregate_words(&self, ty: TypeId) -> u32 {
        let (size, bottom) = match self.program.types.kind(ty) {
            Some(TypeKind::Struct(id)) => match layout::struct_layout(*id, &self.program.types) {
                Ok(layout) => (layout.size, struct_bottom_offset(&layout)),
                Err(_) => (0, 0),
            },
            Some(TypeKind::Array { .. }) => match layout::array_layout(ty, &self.program.types) {
                Ok(layout) => (layout.size, array_bottom_offset(&layout)),
                Err(_) => (0, 0),
            },
            Some(TypeKind::Tuple(elems)) => {
                match layout::tuple_layout(elems, &self.program.types) {
                    Ok(sl) => (sl.size, struct_bottom_offset(&sl)),
                    Err(_) => (0, 0),
                }
            }
            _ => (0, 0),
        };
        value_bottom_words(size, bottom).max(1) as u32
    }

    /// An unsupported-type error for `ty`, appending the recorded aggregate
    /// reason (recursive struct, oversized value, unsupported field) when
    /// classification recorded one.
    fn unsupported_type_error(&self, span: Span, ty: TypeId) -> BackendError {
        let detail = match self.unsupported_aggregates.get(&ty) {
            Some(reason) => format!(
                "the type `{}` is not supported by the native subset ({reason})",
                self.display(ty)
            ),
            None => format!(
                "the type `{}` is not supported by the native subset",
                self.display(ty)
            ),
        };
        BackendError::unsupported_type(span, detail)
    }

    /// The display name of `ty`, for diagnostics.
    fn display(&self, ty: TypeId) -> String {
        self.program.types.display(ty)
    }

    /// Classifies a module binding's type and reserves its static slot.
    ///
    /// Unsupported binding types report one error and keep the slot marked
    /// `None`; function bodies that reference the binding skip their
    /// statements instead of re-reporting. Word bindings (integers and
    /// booleans) and aggregate bindings (structs, arrays, and enums —
    /// session 22) are representable; strings, pointers, references,
    /// ranges, and unit values are rejected because their initializers
    /// cannot be decoded to constant data (strings need a patched image
    /// address; references are local-only).
    fn classify_static(&mut self, s: &MirStatic) {
        let slot = self.static_slots[&s.name.symbol];
        match self.classify(s.ty) {
            Some(
                ty @ (BType::Int
                | BType::Bool
                | BType::Float
                | BType::Char
                | BType::Null
                | BType::Struct
                | BType::Array
                | BType::Enum),
            ) => {
                self.static_types.push(Some(ty));
                self.static_mir_types.push(s.ty);
            }
            _ => {
                self.errors.push(BackendError::unsupported_type(
                    s.span,
                    format!(
                        "module bindings of type `{}` are not supported",
                        self.display(s.ty)
                    ),
                ));
                self.static_types.push(None);
                self.static_mir_types.push(s.ty);
            }
        }
        debug_assert_eq!(self.static_types.len(), slot + 1);
        debug_assert_eq!(self.static_mir_types.len(), slot + 1);
    }

    /// The classified result type of a function, or a structured error.
    fn classify_result(&mut self, f: &MirFn) -> Option<BType> {
        let result = match self.program.types.kind(f.ty) {
            Some(TypeKind::Fn { result, .. }) => *result,
            _ => {
                // Defensive: function symbols always receive a `Fn` type.
                self.errors.push(BackendError::unsupported_type(
                    f.span,
                    "function has no function type",
                ));
                return None;
            }
        };
        match self.classify(result) {
            // A range cannot be returned: the calling convention returns
            // values through `rax` (or, since session 22, a caller-
            // allocated return slot for aggregate values); a range is an
            // iteration value, not a data value, and stays rejected.
            Some(BType::Range) => {
                self.errors.push(BackendError::unsupported_type(
                    f.span,
                    format!("cannot return a value of type `{}`", self.display(result)),
                ));
                None
            }
            Some(classified) => Some(classified),
            None => {
                self.errors.push(BackendError::unsupported_type(
                    f.span,
                    format!(
                        "the function's result type `{}` is not supported",
                        self.display(result)
                    ),
                ));
                None
            }
        }
    }

    // ------------------------------------------------------------------
    // Items
    // ------------------------------------------------------------------

    fn lower_static(&mut self, s: &MirStatic) {
        let slot = self.static_slots[&s.name.symbol];
        let Some(ty) = self.static_types[slot] else {
            // The binding's type was already rejected; skip it.
            return;
        };
        // Word bindings (integers and booleans): the initializer is a
        // single decoded constant. A unit-only enum binding is also a
        // single word, but its value is a variant discriminant decoded
        // through the same constant path; a one-word *struct* binding goes
        // through the aggregate image path below (its literal needs
        // materialization).
        if matches!(
            ty,
            BType::Int | BType::Bool | BType::Float | BType::Char | BType::Null
        ) {
            if !s.stmts.is_empty() || !s.locals.is_empty() {
                self.errors.push(BackendError::unsupported_static(
                    s.span,
                    "the native subset supports only module bindings initialized by a constant",
                ));
                return;
            }
            let value = match &s.value.kind {
                MirOperandKind::Constant(constant) => match self.decode_constant(constant) {
                    Ok(DecodedConstant::Word(value)) => value,
                    // Defensive: module bindings of string type are
                    // rejected by classification before this point (their
                    // value would need a patched blob address, which the
                    // constant model cannot represent).
                    Ok(DecodedConstant::Str(_)) => {
                        self.errors.push(BackendError::unsupported_static(
                            s.span,
                            "string literals cannot initialize module bindings yet",
                        ));
                        return;
                    }
                    Err(error) => {
                        self.errors.push(error);
                        return;
                    }
                },
                _ => {
                    self.errors.push(BackendError::unsupported_static(
                        s.span,
                        format!(
                            "the initializer of `{}` references other values; only constants are supported",
                            s.name.name
                        ),
                    ));
                    return;
                }
            };
            let bytes = value.to_le_bytes().to_vec();
            self.static_images.insert(slot, bytes.clone());
            self.statics.push(BStatic {
                name: s.name.name.clone(),
                symbol: s.name.symbol,
                mutable: s.mutable,
                ty,
                value,
                size: 8,
                bytes,
                span: s.span,
            });
            return;
        }
        // Aggregate bindings (session 22): constant-evaluate the
        // initializer into the value's image bytes.
        match self.eval_static_image(s, ty) {
            Ok(bytes) => {
                self.static_images.insert(slot, bytes.clone());
                self.statics.push(BStatic {
                    name: s.name.name.clone(),
                    symbol: s.name.symbol,
                    mutable: s.mutable,
                    ty,
                    value: 0,
                    size: self.aggregate_value_size(s.ty) as u32,
                    bytes,
                    span: s.span,
                });
            }
            Err(error) => {
                self.errors.push(error);
            }
        }
    }

    /// The byte size of an aggregate-typed value, from its layout (the
    /// static image's value size, used for byte-exact load/store copies).
    fn aggregate_value_size(&self, ty: TypeId) -> u64 {
        match self.program.types.kind(ty) {
            Some(TypeKind::Struct(id)) => layout::struct_layout(*id, &self.program.types)
                .map(|layout| layout.size)
                .unwrap_or(8),
            Some(TypeKind::Array { .. }) => layout::array_layout(ty, &self.program.types)
                .map(|layout| layout.size)
                .unwrap_or(8),
            Some(TypeKind::Tuple(elems)) => layout::tuple_layout(elems, &self.program.types)
                .map(|sl| sl.size)
                .unwrap_or(8),
            Some(TypeKind::Enum(id)) => layout::enum_layout(*id, &self.program.types)
                .map(|layout| layout.size)
                .unwrap_or(8),
            _ => 8,
        }
    }

    /// Constant-evaluates an aggregate module binding's initializer into
    /// its value image: `words * 8` bytes in normal byte order (byte `b`
    /// of the value at offset `b`), with the tail rounded up to a full
    /// word. The initializer must be literal-shaped — `Use` of a constant
    /// or of an earlier binding, or a struct/array/enum literal over such
    /// values — anything else (`E-B05`) is rejected; string, pointer, and
    /// reference constants have no constant image and are rejected the
    /// same way (their values need runtime or patched addresses).
    fn eval_static_image(
        &mut self,
        s: &MirStatic,
        _classified: BType,
    ) -> Result<Vec<u8>, BackendError> {
        let mut images: Vec<Option<Vec<u8>>> = vec![None; s.locals.len()];
        for stmt in &s.stmts {
            let MirStmtKind::Assign { target, rvalue } = &stmt.kind;
            let MirTargetKind::Local(id) = &target.kind else {
                return Err(BackendError::unsupported_static(
                    stmt.span,
                    "a module binding's initializer must be a constant literal",
                ));
            };
            let image = self.eval_static_rvalue(rvalue, &images)?;
            images[id.raw() as usize] = Some(image);
        }
        let image = match &s.value.kind {
            MirOperandKind::Local(id) => images[id.raw() as usize].clone().ok_or_else(|| {
                BackendError::unsupported_static(
                    s.span,
                    "a module binding's initializer must be a constant literal",
                )
            })?,
            MirOperandKind::Constant(constant) => self.static_constant_image(constant)?,
            MirOperandKind::Static(symbol) => {
                let Some(&slot) = self.static_slots.get(symbol) else {
                    return Err(BackendError::unsupported_static(
                        s.span,
                        "the initializer references an unknown module binding",
                    ));
                };
                self.static_images.get(&slot).cloned().ok_or_else(|| {
                    BackendError::unsupported_static(
                        s.span,
                        format!(
                            "the initializer of `{}` references an unsupported or later module binding",
                            s.name.name
                        ),
                    )
                })?
            }
        };
        // The region is `ceil(size / 8) * 8` bytes (word-aligned, at
        // least one word); the value's own bytes are the image prefix.
        // The padded *slot* word count is not used here: the data region
        // holds only the value's bytes.
        let size = self.aggregate_value_size(s.ty) as usize;
        let region = size.div_ceil(8) * 8;
        let mut bytes = vec![0u8; region];
        let copy = image.len().min(region);
        bytes[..copy].copy_from_slice(&image[..copy]);
        Ok(bytes)
    }

    /// Constant-evaluates one rvalue of a module binding's initializer
    /// into its value image. `images` holds the already-evaluated
    /// temporaries.
    fn eval_static_rvalue(
        &mut self,
        rvalue: &crate::mir::MirRvalue,
        images: &[Option<Vec<u8>>],
    ) -> Result<Vec<u8>, BackendError> {
        match &rvalue.kind {
            MirRvalueKind::Use(operand) => self.static_operand_image(operand, images),
            MirRvalueKind::StructLit { fields } => {
                let (info, layout) = self.struct_layout_of(rvalue.ty, rvalue.span)?;
                let mut image = vec![0u8; layout.size as usize];
                for (member, value) in fields {
                    let index = info
                        .fields
                        .iter()
                        .position(|field| field.name == member.name)
                        .ok_or_else(|| {
                            BackendError::invalid_backend_ir(
                                member.span,
                                format!(
                                    "struct literal initializes undeclared field `{}`",
                                    member.name
                                ),
                            )
                        })?;
                    let field_size = layout.fields[index].size as usize;
                    let operand_image = self.static_operand_image(value, images)?;
                    let offset = layout.fields[index].offset as usize;
                    let copy = operand_image.len().min(field_size);
                    image[offset..offset + copy].copy_from_slice(&operand_image[..copy]);
                }
                Ok(image)
            }
            MirRvalueKind::ArrayLit { elems } => {
                let (_, stride, len) = self.resolve_array(rvalue.ty, rvalue.span)?;
                let stride = stride as usize;
                let mut image = vec![0u8; stride * len as usize];
                for (i, elem) in elems.iter().enumerate() {
                    let elem_image = self.static_operand_image(elem, images)?;
                    let offset = i * stride;
                    let copy = elem_image.len().min(stride);
                    image[offset..offset + copy].copy_from_slice(&elem_image[..copy]);
                }
                Ok(image)
            }
            MirRvalueKind::TupleLit { elems } => {
                if let Some(TypeKind::Tuple(tuple_elems)) = self.program.types.kind(rvalue.ty) {
                    let tl =
                        layout::tuple_layout(tuple_elems, &self.program.types).map_err(|e| {
                            BackendError::invalid_backend_ir(rvalue.span, layout_error_message(&e))
                        })?;
                    let mut image = vec![0u8; tl.size as usize];
                    for (i, elem) in elems.iter().enumerate() {
                        let elem_image = self.static_operand_image(elem, images)?;
                        let fl = &tl.fields[i];
                        let offset = fl.offset as usize;
                        let copy = elem_image.len().min(fl.size as usize);
                        image[offset..offset + copy].copy_from_slice(&elem_image[..copy]);
                    }
                    Ok(image)
                } else {
                    Err(BackendError::invalid_backend_ir(
                        rvalue.span,
                        "a tuple literal has a non-tuple type",
                    ))
                }
            }
            MirRvalueKind::EnumInit {
                discriminant,
                payload,
            } => {
                let enum_id = self.program.types.enum_id(rvalue.ty).ok_or_else(|| {
                    BackendError::invalid_backend_ir(
                        rvalue.span,
                        "an enum construction has a non-enum type",
                    )
                })?;
                let layout = layout::enum_layout(enum_id, &self.program.types)
                    .map_err(|_| self.unsupported_type_error(rvalue.span, rvalue.ty))?;
                let variant = layout
                    .variants
                    .iter()
                    .find(|v| v.discriminant == *discriminant)
                    .ok_or_else(|| {
                        BackendError::invalid_backend_ir(
                            rvalue.span,
                            format!(
                                "enum construction references unknown discriminant {discriminant}"
                            ),
                        )
                    })?;
                let mut image = vec![0u8; layout.size as usize];
                let tag_offset = layout.tag_offset as usize;
                let tag = discriminant.to_le_bytes();
                image[tag_offset..tag_offset + 8].copy_from_slice(&tag);
                if let Some(payload) = payload {
                    let payload_image = self.static_operand_image(payload, images)?;
                    let payload_offset = layout.payload_offset as usize;
                    let copy = payload_image.len().min(variant.size as usize);
                    image[payload_offset..payload_offset + copy]
                        .copy_from_slice(&payload_image[..copy]);
                }
                Ok(image)
            }
            _ => Err(BackendError::unsupported_static(
                rvalue.span,
                "a module binding's initializer must be a constant literal",
            )),
        }
    }

    /// The value image of an initializer operand: a constant (an 8-byte
    /// word for integers, booleans, and enum discriminants), a temporary
    /// from `images`, or an earlier module binding's image.
    fn static_operand_image(
        &mut self,
        operand: &MirOperand,
        images: &[Option<Vec<u8>>],
    ) -> Result<Vec<u8>, BackendError> {
        match &operand.kind {
            MirOperandKind::Local(id) => images[id.raw() as usize].clone().ok_or_else(|| {
                BackendError::unsupported_static(
                    operand.span,
                    "a module binding's initializer must be a constant literal",
                )
            }),
            MirOperandKind::Constant(constant) => self.static_constant_image(constant),
            MirOperandKind::Static(symbol) => {
                let Some(&slot) = self.static_slots.get(symbol) else {
                    return Err(BackendError::unsupported_static(
                        operand.span,
                        "the initializer references an unknown module binding",
                    ));
                };
                self.static_images.get(&slot).cloned().ok_or_else(|| {
                    BackendError::unsupported_static(
                        operand.span,
                        "the initializer references an unsupported or later module binding",
                    )
                })
            }
        }
    }

    /// The 8-byte little-endian word image of a constant initializer
    /// value. Only integer, boolean, and enum-discriminant constants have
    /// constant images; string constants (and anything else) need a
    /// patched or runtime address and are rejected (`E-B05`).
    fn static_constant_image(
        &mut self,
        constant: &crate::mir::MirConstant,
    ) -> Result<Vec<u8>, BackendError> {
        match self.decode_constant(constant) {
            Ok(DecodedConstant::Word(value)) => Ok(value.to_le_bytes().to_vec()),
            Ok(DecodedConstant::Str(_)) => Err(BackendError::unsupported_static(
                constant.span,
                "string literals cannot initialize module bindings yet",
            )),
            Err(error) => Err(error),
        }
    }

    fn lower_fn(&mut self, f: &MirFn) {
        let Some(result) = self.classify_result(f) else {
            // The result-type error was reported; the body cannot be
            // emitted meaningfully.
            return;
        };
        // The result's slot word count: 1 for every scalar, `ceil(size /
        // 8)` for aggregate results (a multi-word result is returned
        // through a caller-allocated return slot).
        let result_words = match self.program.types.kind(f.ty) {
            Some(TypeKind::Fn {
                result: result_ty, ..
            }) => self.words_of(*result_ty, result),
            _ => 1,
        };
        // Classify every local up front. Unsupported locals report one
        // error each and classify to `None`; statements that touch them are
        // skipped during instruction lowering so the same root cause is not
        // reported twice.
        self.fn_locals.clear();
        self.fn_classified.clear();
        self.fn_local_types.clear();
        for local in &f.locals {
            self.fn_local_types.push(local.ty);
            match self.classify(local.ty) {
                Some(ty) => {
                    self.fn_classified.push(Some(ty));
                    self.fn_locals.push(BLocal {
                        name: local.name.clone(),
                        symbol: local.symbol,
                        ty,
                        words: self.words_of(local.ty, ty),
                        mutable: local.mutable,
                        span: local.span,
                    });
                }
                None => {
                    self.errors.push(BackendError::unsupported_type(
                        local.span,
                        format!(
                            "the type `{}` is not supported by the native subset",
                            self.display(local.ty)
                        ),
                    ));
                    self.fn_classified.push(None);
                    self.fn_locals.push(BLocal {
                        name: local.name.clone(),
                        symbol: local.symbol,
                        ty: BType::Int,
                        words: 1,
                        mutable: local.mutable,
                        span: local.span,
                    });
                }
            }
        }
        let mut blocks = Vec::with_capacity(f.blocks.len());
        for block in &f.blocks {
            blocks.push(self.lower_block(block));
        }
        self.functions.push(BFunction {
            name: f.name.name.clone(),
            symbol: f.name.symbol,
            params: f.params.clone(),
            locals: std::mem::take(&mut self.fn_locals),
            blocks,
            result,
            result_words,
            span: f.span,
        });
        self.fn_classified.clear();
        self.fn_local_types.clear();
    }

    /// Allocates a lowering temporary of the MIR type `ty` (classified as
    /// `classified`) and returns its id. Aggregate-typed temporaries get
    /// their word count from the layout; scalars use their width.
    fn alloc_temp(&mut self, ty: TypeId, classified: BType, span: Span) -> crate::mir::LocalId {
        let id = crate::mir::LocalId::new(self.fn_classified.len() as u32);
        let words = self.words_of(ty, classified);
        self.fn_classified.push(Some(classified));
        self.fn_local_types.push(ty);
        self.fn_locals.push(BLocal {
            name: String::new(),
            symbol: None,
            ty: classified,
            words,
            mutable: false,
            span,
        });
        id
    }

    /// Appends an instruction to the block currently being lowered.
    fn push(&mut self, kind: BInstKind, span: Span) {
        self.fn_insts.push(BInst { kind, span });
    }

    fn lower_block(&mut self, block: &crate::mir::MirBlock) -> BBlock {
        for stmt in &block.stmts {
            let start = self.fn_insts.len();
            if self.lower_stmt(stmt).is_none() {
                // The statement was skipped or errored; drop any partial
                // temporary loads it emitted so the block stays clean.
                self.fn_insts.truncate(start);
            }
        }
        let terminator = self.lower_terminator(&block.terminator);
        BBlock {
            id: block.id,
            insts: std::mem::take(&mut self.fn_insts),
            terminator,
            span: block.span,
        }
    }

    /// Whether a local classifies to `None` (its unsupported-type error was
    /// already reported).
    fn local_is_unsupported(&self, id: crate::mir::LocalId) -> bool {
        self.fn_classified
            .get(id.raw() as usize)
            .copied()
            .flatten()
            .is_none()
    }

    /// Whether a static slot classifies to `None`.
    fn static_is_unsupported(&self, slot: usize) -> bool {
        self.static_types.get(slot).copied().flatten().is_none()
    }

    /// Whether an operand touches an unsupported local or module binding.
    fn operand_is_unsupported(&self, operand: &MirOperand) -> bool {
        match &operand.kind {
            MirOperandKind::Local(id) => self.local_is_unsupported(*id),
            MirOperandKind::Static(symbol) => self
                .static_slots
                .get(symbol)
                .is_some_and(|&slot| self.static_is_unsupported(slot)),
            MirOperandKind::Constant(_) => false,
        }
    }

    /// Whether an rvalue's operands touch an unsupported local or binding.
    fn rvalue_touches_unsupported(&self, rvalue: &crate::mir::MirRvalue) -> bool {
        match &rvalue.kind {
            MirRvalueKind::Use(operand)
            | MirRvalueKind::Unary { operand, .. }
            | MirRvalueKind::RangeNext { range: operand }
            | MirRvalueKind::RangeFinished { range: operand } => {
                self.operand_is_unsupported(operand)
            }
            MirRvalueKind::Binary { lhs, rhs, .. } => {
                self.operand_is_unsupported(lhs) || self.operand_is_unsupported(rhs)
            }
            MirRvalueKind::Call { callee, args } => {
                self.operand_is_unsupported(callee)
                    || args.iter().any(|arg| self.operand_is_unsupported(arg))
            }
            MirRvalueKind::Range { start, end, .. } => {
                self.operand_is_unsupported(start) || self.operand_is_unsupported(end)
            }
            MirRvalueKind::Member { base, .. } => self.operand_is_unsupported(base),
            MirRvalueKind::Index { base, index } => {
                self.operand_is_unsupported(base) || self.operand_is_unsupported(index)
            }
            MirRvalueKind::RefAddr { root, steps, .. } => {
                self.local_is_unsupported(*root)
                    || steps.iter().any(|step| match &step.kind {
                        crate::mir::MirPlaceStepKind::Index(index) => {
                            self.operand_is_unsupported(index)
                        }
                        crate::mir::MirPlaceStepKind::Field(_) => false,
                    })
            }
            MirRvalueKind::Deref { operand } => self.operand_is_unsupported(operand),
            MirRvalueKind::StructLit { fields } => fields
                .iter()
                .any(|(_, operand)| self.operand_is_unsupported(operand)),
            MirRvalueKind::ArrayLit { elems } => elems
                .iter()
                .any(|operand| self.operand_is_unsupported(operand)),
            MirRvalueKind::TupleLit { elems } => elems
                .iter()
                .any(|operand| self.operand_is_unsupported(operand)),
            MirRvalueKind::EnumInit { payload, .. } => payload
                .as_ref()
                .is_some_and(|operand| self.operand_is_unsupported(operand)),
            MirRvalueKind::EnumTag { value } => self.operand_is_unsupported(value),
            MirRvalueKind::EnumPayload { value } => self.operand_is_unsupported(value),
        }
    }

    /// Lowers one statement, pushing its instructions.
    ///
    /// Returns `None` when the statement was skipped (it touches an
    /// unsupported value whose error was already reported) or when its own
    /// error was reported; `Some(())` when it was emitted.
    fn lower_stmt(&mut self, stmt: &crate::mir::MirStmt) -> Option<()> {
        let (target, rvalue) = match &stmt.kind {
            MirStmtKind::Assign { target, rvalue } => (target, rvalue),
        };
        match &target.kind {
            MirTargetKind::Local(id) => {
                if self.local_is_unsupported(*id) || self.rvalue_touches_unsupported(rvalue) {
                    return None;
                }
                match self.lower_rvalue_into(*id, rvalue) {
                    Ok(()) => Some(()),
                    Err(error) => {
                        self.errors.push(error);
                        None
                    }
                }
            }
            MirTargetKind::Static(symbol) => {
                let Some(&slot) = self.static_slots.get(symbol) else {
                    self.errors.push(BackendError::invalid_backend_ir(
                        stmt.span,
                        "assignment target references an unknown module binding",
                    ));
                    return None;
                };
                if self.static_is_unsupported(slot) || self.rvalue_touches_unsupported(rvalue) {
                    return None;
                }
                match self.lower_rvalue_to_operand(rvalue) {
                    Ok(src) => {
                        self.push(
                            BInstKind::StoreStatic {
                                static_index: slot,
                                src,
                            },
                            stmt.span,
                        );
                        Some(())
                    }
                    Err(error) => {
                        self.errors.push(error);
                        None
                    }
                }
            }
            MirTargetKind::Member { base, member } => {
                if self.rvalue_touches_unsupported(rvalue) || self.operand_is_unsupported(base) {
                    return None;
                }
                let result = (|| {
                    let base_id = self.operand_slot(base, target.span)?;
                    let (field_ty, offset, size, _) =
                        self.resolve_member(base.ty, member, target.span)?;
                    let src = self.lower_rvalue_to_operand(rvalue)?;
                    self.push(
                        BInstKind::FieldStore {
                            base: base_id,
                            field_ty,
                            byte_offset: offset,
                            size,
                            src,
                        },
                        stmt.span,
                    );
                    Ok::<(), BackendError>(())
                })();
                match result {
                    Ok(()) => Some(()),
                    Err(error) => {
                        self.errors.push(error);
                        None
                    }
                }
            }
            MirTargetKind::Index { base, index } => {
                if self.rvalue_touches_unsupported(rvalue)
                    || self.operand_is_unsupported(base)
                    || self.operand_is_unsupported(index)
                {
                    return None;
                }
                let result = (|| {
                    let base_id = self.operand_slot(base, target.span)?;
                    let (elem_ty, stride, len) = self.resolve_array(base.ty, target.span)?;
                    let index_op = self.eval_operand(index)?;
                    let src = self.lower_rvalue_to_operand(rvalue)?;
                    self.push(
                        BInstKind::IndexStore {
                            base: base_id,
                            elem_ty,
                            stride,
                            len,
                            index: index_op,
                            src,
                        },
                        stmt.span,
                    );
                    Ok::<(), BackendError>(())
                })();
                match result {
                    Ok(()) => Some(()),
                    Err(error) => {
                        self.errors.push(error);
                        None
                    }
                }
            }
            MirTargetKind::Place {
                root,
                root_ty,
                steps,
            } => {
                if self.rvalue_touches_unsupported(rvalue)
                    || steps.iter().any(|step| match &step.kind {
                        crate::mir::MirPlaceStepKind::Index(index) => {
                            self.operand_is_unsupported(index)
                        }
                        crate::mir::MirPlaceStepKind::Field(_) => false,
                    })
                {
                    return None;
                }
                // A local root stores through the root slot. A module-
                // storage root (session 22) stores through a read-modify-
                // write: the binding is loaded into a temporary, the place
                // is written inside it, and the whole value is stored
                // back — so `a[i] = v` and `g.rows[1].y = v` reach the
                // module binding, never a temporary copy.
                let root_handling = match root {
                    crate::mir::MirPlaceRoot::Local(id) => {
                        if self.local_is_unsupported(*id) {
                            return None;
                        }
                        RootHandling::Local(*id)
                    }
                    crate::mir::MirPlaceRoot::Static(symbol) => {
                        let Some(&slot) = self.static_slots.get(symbol) else {
                            self.errors.push(BackendError::invalid_backend_ir(
                                stmt.span,
                                "assignment target references an unknown module binding",
                            ));
                            return None;
                        };
                        if self.static_is_unsupported(slot) {
                            return None;
                        }
                        RootHandling::Static { slot, ty: *root_ty }
                    }
                };
                let result = (|| {
                    let (base, write_back) = match root_handling {
                        RootHandling::Local(id) => (id, None),
                        RootHandling::Static { slot, ty } => {
                            let classified = self.classify(ty).expect("supported statics classify");
                            let temp = self.alloc_temp(ty, classified, stmt.span);
                            self.push(
                                BInstKind::LoadStatic {
                                    target: temp,
                                    static_index: slot,
                                },
                                stmt.span,
                            );
                            (temp, Some(slot))
                        }
                    };
                    let (addr_steps, size) = self.resolve_place(base, steps, target.span)?;
                    let src = self.lower_rvalue_to_operand(rvalue)?;
                    self.push(
                        BInstKind::PlaceStore {
                            base,
                            steps: addr_steps,
                            size,
                            src,
                        },
                        stmt.span,
                    );
                    if let Some(slot) = write_back {
                        self.push(
                            BInstKind::StoreStatic {
                                static_index: slot,
                                src: BOperand::Local(base),
                            },
                            stmt.span,
                        );
                    }
                    Ok::<(), BackendError>(())
                })();
                match result {
                    Ok(()) => Some(()),
                    Err(error) => {
                        self.errors.push(error);
                        None
                    }
                }
            }
            MirTargetKind::Deref { operand } => {
                if self.rvalue_touches_unsupported(rvalue) || self.operand_is_unsupported(operand) {
                    return None;
                }
                let result = (|| {
                    // The target's type is the referent type; the operand
                    // holds the reference (whose type carries the
                    // referent) — `referent_info` needs the reference type.
                    let (elem_ty, size) = self.referent_info(operand.ty, target.span)?;
                    let reference = self.eval_operand(operand)?;
                    let src = self.lower_rvalue_to_operand(rvalue)?;
                    self.push(
                        BInstKind::RefStore {
                            reference,
                            elem_ty,
                            size,
                            src,
                        },
                        stmt.span,
                    );
                    Ok::<(), BackendError>(())
                })();
                match result {
                    Ok(()) => Some(()),
                    Err(error) => {
                        self.errors.push(error);
                        None
                    }
                }
            }
        }
    }

    /// Lowers an rvalue into `target`, pushing the instruction(s).
    ///
    /// Struct and array literals are multi-instruction: they store their
    /// fields/elements into the target slot one at a time (the backend
    /// resolves the offsets from the deterministic layout).
    fn lower_rvalue_into(
        &mut self,
        target: crate::mir::LocalId,
        rvalue: &crate::mir::MirRvalue,
    ) -> Result<(), BackendError> {
        match &rvalue.kind {
            MirRvalueKind::StructLit { fields } => {
                let (info, layout) = self.struct_layout_of(rvalue.ty, rvalue.span)?;
                for (member, value) in fields {
                    let index = info
                        .fields
                        .iter()
                        .position(|field| field.name == member.name)
                        .ok_or_else(|| {
                            BackendError::invalid_backend_ir(
                                member.span,
                                format!(
                                    "struct literal initializes undeclared field `{}`",
                                    member.name
                                ),
                            )
                        })?;
                    let field = &info.fields[index];
                    let field_ty = self
                        .classify(field.ty)
                        .ok_or_else(|| self.unsupported_type_error(member.span, field.ty))?;
                    let field_layout = &layout.fields[index];
                    let src = self.eval_operand(value)?;
                    self.push(
                        BInstKind::FieldStore {
                            base: target,
                            field_ty,
                            byte_offset: field_layout.offset as u32,
                            size: field_layout.size as u32,
                            src,
                        },
                        value.span,
                    );
                }
                return Ok(());
            }
            MirRvalueKind::ArrayLit { elems } => {
                let (elem_ty, stride, len) = self.resolve_array(rvalue.ty, rvalue.span)?;
                for (i, elem) in elems.iter().enumerate() {
                    let src = self.eval_operand(elem)?;
                    self.push(
                        BInstKind::IndexStore {
                            base: target,
                            elem_ty,
                            stride,
                            len,
                            index: BOperand::Const(i as i64),
                            src,
                        },
                        elem.span,
                    );
                }
                return Ok(());
            }
            MirRvalueKind::TupleLit { elems } => {
                // Tuple literal (session 29): store each element into the
                // target at its deterministic byte offset.
                let tuple_ty = rvalue.ty;
                if let Some(TypeKind::Tuple(tuple_elems)) = self.program.types.kind(tuple_ty) {
                    let tl =
                        layout::tuple_layout(tuple_elems, &self.program.types).map_err(|e| {
                            BackendError::invalid_backend_ir(rvalue.span, layout_error_message(&e))
                        })?;
                    for (i, elem) in elems.iter().enumerate() {
                        let src = self.eval_operand(elem)?;
                        let fl = &tl.fields[i];
                        let bt = self.classify(tuple_elems[i]).ok_or_else(|| {
                            self.unsupported_type_error(rvalue.span, tuple_elems[i])
                        })?;
                        self.push(
                            BInstKind::FieldStore {
                                base: target,
                                field_ty: bt,
                                byte_offset: fl.offset as u32,
                                size: fl.size as u32,
                                src,
                            },
                            elem.span,
                        );
                    }
                }
                return Ok(());
            }
            MirRvalueKind::EnumInit {
                discriminant,
                payload,
            } => {
                // A data-carrying construction (session 19): the tag word
                // is written and the variant's own payload bytes are
                // copied into the payload area (the emitter never copies
                // the shared area's full width, so it never reads past a
                // smaller payload's slot).
                let enum_id = self.program.types.enum_id(rvalue.ty).ok_or_else(|| {
                    BackendError::invalid_backend_ir(
                        rvalue.span,
                        "an enum construction has a non-enum type",
                    )
                })?;
                let layout = layout::enum_layout(enum_id, &self.program.types)
                    .map_err(|_| self.unsupported_type_error(rvalue.span, rvalue.ty))?;
                // The discriminant is the tag *value* (an explicit `V = n`
                // discriminant or the implicit continuation), not an index
                // into the variant list, so the payload geometry is found
                // by matching the value.
                let variant_layout = layout
                    .variants
                    .iter()
                    .find(|v| v.discriminant == *discriminant)
                    .ok_or_else(|| {
                        BackendError::invalid_backend_ir(
                            rvalue.span,
                            format!(
                                "enum construction references unknown discriminant {discriminant}"
                            ),
                        )
                    })?;
                let payload = match payload {
                    Some(operand) => Some(self.eval_operand(operand)?),
                    None => None,
                };
                self.push(
                    BInstKind::EnumInit {
                        target,
                        discriminant: *discriminant,
                        payload,
                        tag_offset: layout.tag_offset as u32,
                        payload_offset: layout.payload_offset as u32,
                        payload_size: variant_layout.size as u32,
                    },
                    rvalue.span,
                );
                return Ok(());
            }
            _ => {}
        }
        let kind = match &rvalue.kind {
            MirRvalueKind::Use(operand) => match &operand.kind {
                MirOperandKind::Local(src) => BInstKind::LoadLocal { target, src: *src },
                MirOperandKind::Constant(constant) => match self.decode_constant(constant)? {
                    DecodedConstant::Word(value) => BInstKind::LoadConst { target, value },
                    DecodedConstant::Str(string_index) => BInstKind::LoadStr {
                        target,
                        string_index,
                    },
                },
                MirOperandKind::Static(symbol) => BInstKind::LoadStatic {
                    target,
                    static_index: self.resolve_static(*symbol, rvalue.span)?,
                },
            },
            MirRvalueKind::Unary { op, operand } => BInstKind::Unary {
                target,
                op: *op,
                src: self.eval_operand(operand)?,
            },
            MirRvalueKind::Binary { op, lhs, rhs } => {
                let ty = self
                    .classify(lhs.ty)
                    .ok_or_else(|| self.unsupported_type_error(rvalue.span, lhs.ty))?;
                BInstKind::Binary {
                    target,
                    op: *op,
                    ty,
                    lhs: self.eval_operand(lhs)?,
                    rhs: self.eval_operand(rhs)?,
                }
            }
            MirRvalueKind::Call { callee, args } => {
                let mut lowered_args = Vec::with_capacity(args.len());
                for arg in args {
                    lowered_args.push(self.eval_operand(arg)?);
                }
                match self.resolve_callee(callee, rvalue.span)? {
                    Callee::Function(callee_index) => BInstKind::Call {
                        target,
                        callee: callee_index,
                        args: lowered_args,
                    },
                    Callee::Runtime(service) => BInstKind::RuntimeCall {
                        target,
                        service,
                        args: lowered_args,
                    },
                }
            }
            MirRvalueKind::Range {
                inclusive,
                start,
                end,
            } => BInstKind::RangeInit {
                target,
                start: self.eval_operand(start)?,
                end: self.eval_operand(end)?,
                inclusive: *inclusive,
            },
            MirRvalueKind::RangeNext { range } => BInstKind::RangeNext {
                target,
                range: self.range_slot(range, rvalue.span)?,
            },
            MirRvalueKind::RangeFinished { range } => BInstKind::RangeFinished {
                target,
                range: self.range_slot(range, rvalue.span)?,
            },
            MirRvalueKind::Member { base, member } => {
                let base_id = self.operand_slot(base, rvalue.span)?;
                let (field_ty, offset, size, _) =
                    self.resolve_member(base.ty, member, rvalue.span)?;
                BInstKind::FieldLoad {
                    target,
                    base: base_id,
                    field_ty,
                    byte_offset: offset,
                    size,
                }
            }
            MirRvalueKind::Index { base, index } => {
                let base_id = self.operand_slot(base, rvalue.span)?;
                let (elem_ty, stride, len) = self.resolve_array(base.ty, rvalue.span)?;
                BInstKind::IndexLoad {
                    target,
                    base: base_id,
                    elem_ty,
                    stride,
                    len,
                    index: self.eval_operand(index)?,
                }
            }
            MirRvalueKind::RefAddr {
                mutable: _,
                root,
                steps,
            } => {
                let (addr_steps, _) = self.resolve_place(*root, steps, rvalue.span)?;
                BInstKind::RefAddr {
                    target,
                    base: *root,
                    steps: addr_steps,
                }
            }
            MirRvalueKind::Deref { operand } => {
                let (elem_ty, size) = self.referent_info(operand.ty, rvalue.span)?;
                BInstKind::RefLoad {
                    target,
                    reference: self.eval_operand(operand)?,
                    elem_ty,
                    size,
                }
            }
            MirRvalueKind::EnumTag { value } => {
                let enum_id = self.program.types.enum_id(value.ty).ok_or_else(|| {
                    BackendError::invalid_backend_ir(
                        rvalue.span,
                        "an enum-tag extraction has a non-enum value",
                    )
                })?;
                let layout = layout::enum_layout(enum_id, &self.program.types)
                    .map_err(|_| self.unsupported_type_error(rvalue.span, value.ty))?;
                let value = self.operand_slot(value, rvalue.span)?;
                BInstKind::EnumTag {
                    target,
                    value,
                    tag_offset: layout.tag_offset as u32,
                }
            }
            MirRvalueKind::EnumPayload { value } => {
                let enum_id = self.program.types.enum_id(value.ty).ok_or_else(|| {
                    BackendError::invalid_backend_ir(
                        rvalue.span,
                        "an enum-payload extraction has a non-enum value",
                    )
                })?;
                let layout = layout::enum_layout(enum_id, &self.program.types)
                    .map_err(|_| self.unsupported_type_error(rvalue.span, value.ty))?;
                let size = self.value_byte_size(rvalue.ty).ok_or_else(|| {
                    BackendError::unsupported_type(
                        rvalue.span,
                        format!(
                            "the payload type `{}` is not supported by the native subset",
                            self.display(rvalue.ty)
                        ),
                    )
                })?;
                let value = self.operand_slot(value, rvalue.span)?;
                BInstKind::EnumPayload {
                    target,
                    value,
                    payload_offset: layout.payload_offset as u32,
                    payload_size: size,
                }
            }
            // Handled above (multi-instruction literal materialization and
            // tagged-union construction).
            MirRvalueKind::StructLit { .. }
            | MirRvalueKind::ArrayLit { .. }
            | MirRvalueKind::TupleLit { .. }
            | MirRvalueKind::EnumInit { .. } => unreachable!(),
        };
        self.push(kind, rvalue.span);
        Ok(())
    }

    /// Lowers an rvalue into an operand for the store-to-static path:
    /// operand-shaped rvalues evaluate directly; other rvalues are computed
    /// into a temporary slot and the temporary's load is returned.
    fn lower_rvalue_to_operand(
        &mut self,
        rvalue: &crate::mir::MirRvalue,
    ) -> Result<BOperand, BackendError> {
        if let MirRvalueKind::Use(operand) = &rvalue.kind {
            return self.eval_operand(operand);
        }
        let ty = match self.classify(rvalue.ty) {
            Some(ty) => ty,
            // Defensive: a clean pipeline never stores an
            // unsupported-typed value (the type checker rejects the
            // assignment); report it.
            None => {
                return Err(BackendError::unsupported_type(
                    rvalue.span,
                    format!(
                        "the value's type `{}` is not supported by the native subset",
                        self.display(rvalue.ty)
                    ),
                ));
            }
        };
        let temp = self.alloc_temp(rvalue.ty, ty, rvalue.span);
        self.lower_rvalue_into(temp, rvalue)?;
        Ok(BOperand::Local(temp))
    }

    /// Resolves a `Static` operand to a static slot, rejecting function
    /// references.
    fn resolve_static(&mut self, symbol: SymbolId, span: Span) -> Result<usize, BackendError> {
        if let Some(&slot) = self.static_slots.get(&symbol) {
            return Ok(slot);
        }
        if self.fn_index.contains_key(&symbol) {
            return Err(BackendError::unsupported_rvalue(
                span,
                "a function cannot be used as a value",
            ));
        }
        Err(BackendError::invalid_backend_ir(
            span,
            "operand references an unknown module item",
        ))
    }

    /// Resolves a call's callee operand to a user function index or an
    /// embedded runtime service.
    fn resolve_callee(&mut self, callee: &MirOperand, span: Span) -> Result<Callee, BackendError> {
        match &callee.kind {
            MirOperandKind::Static(symbol) => {
                if let Some(&index) = self.fn_index.get(symbol) {
                    return Ok(Callee::Function(index));
                }
                // A predeclared runtime intrinsic: the program carries the
                // symbol → intrinsic mapping; map the intrinsic to its
                // machine service.
                if let Some((_, intrinsic)) = self
                    .program
                    .intrinsic_symbols
                    .iter()
                    .find(|(symbol_id, _)| symbol_id == symbol)
                {
                    let name = intrinsic.get().name;
                    return Self::runtime_service(name)
                        .map(Callee::Runtime)
                        .ok_or_else(|| {
                            BackendError::invalid_backend_ir(
                                span,
                                format!("the intrinsic `{name}` has no runtime service mapping"),
                            )
                        });
                }
                if self.static_slots.contains_key(symbol) {
                    return Err(BackendError::unsupported_callee(
                        span,
                        "the callee is a module binding, not a function",
                    ));
                }
                Err(BackendError::invalid_backend_ir(
                    span,
                    "callee references an unknown module item",
                ))
            }
            _ => Err(BackendError::unsupported_callee(
                span,
                "the native subset supports only direct calls to module-level functions",
            )),
        }
    }

    /// The runtime service an intrinsic name maps to, if any. Only the
    /// callable subset of services is exposed to generated code.
    fn runtime_service(name: &str) -> Option<RuntimeService> {
        let service = match name {
            "rt_alloc" => RuntimeService::Alloc,
            "rt_free" => RuntimeService::Free,
            "rt_mem_load" => RuntimeService::MemLoad,
            "rt_mem_store" => RuntimeService::MemStore,
            "rt_str_alloc" => RuntimeService::StrAlloc,
            "rt_str_free" => RuntimeService::StrFree,
            "rt_str_len" => RuntimeService::StrLen,
            "rt_str_byte" => RuntimeService::StrByte,
            "rt_str_set_byte" => RuntimeService::StrSetByte,
            "rt_print_str" => RuntimeService::PrintStr,
            "rt_exit" => RuntimeService::Exit,
            "rt_print_int" => RuntimeService::PrintInt,
            "rt_print_float" => RuntimeService::PrintFloat,
            "rt_print_char" => RuntimeService::PrintChar,
            _ => return None,
        };
        debug_assert!(service.is_callable());
        Some(service)
    }

    /// The slot of a range being iterated: range iteration reads a local
    /// slot, so only local operands are supported here.
    fn range_slot(
        &mut self,
        range: &MirOperand,
        span: Span,
    ) -> Result<crate::mir::LocalId, BackendError> {
        match &range.kind {
            MirOperandKind::Local(id) => Ok(*id),
            _ => Err(BackendError::unsupported_rvalue(
                span,
                "range iteration must read a local range value",
            )),
        }
    }

    /// Materializes an operand into a local slot, returning the slot id.
    ///
    /// Member/index bases are aggregate *values*, which live in slots, so
    /// a base that is not already a local is evaluated into a temporary.
    /// In a clean pipeline bases are always locals (aggregates cannot be
    /// constants or module bindings); the constant/static paths are
    /// defensive.
    fn operand_slot(
        &mut self,
        operand: &MirOperand,
        span: Span,
    ) -> Result<crate::mir::LocalId, BackendError> {
        match &operand.kind {
            MirOperandKind::Local(id) => Ok(*id),
            MirOperandKind::Constant(constant) => {
                let classified = self
                    .classify(operand.ty)
                    .ok_or_else(|| self.unsupported_type_error(span, operand.ty))?;
                let temp = self.alloc_temp(operand.ty, classified, span);
                match self.decode_constant(constant)? {
                    DecodedConstant::Word(value) => {
                        self.push(
                            BInstKind::LoadConst {
                                target: temp,
                                value,
                            },
                            span,
                        );
                    }
                    DecodedConstant::Str(_) => {
                        return Err(BackendError::invalid_backend_ir(
                            span,
                            "a string constant cannot be a member/index base",
                        ));
                    }
                }
                Ok(temp)
            }
            MirOperandKind::Static(symbol) => {
                let slot = self.resolve_static(*symbol, span)?;
                let classified = self.static_types[slot]
                    .expect("unsupported statics are skipped before operand evaluation");
                let temp = self.alloc_temp(operand.ty, classified, span);
                self.push(
                    BInstKind::LoadStatic {
                        target: temp,
                        static_index: slot,
                    },
                    span,
                );
                Ok(temp)
            }
        }
    }

    /// The struct info and deterministic layout of the struct type `ty`.
    fn struct_layout_of(
        &self,
        ty: TypeId,
        span: Span,
    ) -> Result<(crate::typecheck::StructInfo, layout::StructLayout), BackendError> {
        let id = match self.program.types.kind(ty) {
            Some(TypeKind::Struct(id)) => *id,
            _ => {
                return Err(BackendError::invalid_backend_ir(
                    span,
                    "a member access base is not a struct value",
                ));
            }
        };
        let info =
            self.program.types.struct_info(id).ok_or_else(|| {
                BackendError::invalid_backend_ir(span, "struct id does not resolve")
            })?;
        let layout = layout::struct_layout(id, &self.program.types).map_err(|error| {
            BackendError::invalid_backend_ir(span, layout_error_message(&error))
        })?;
        Ok((info.clone(), layout))
    }

    /// Resolves `base.member` (with `base_ty` the base operand's type)
    /// into the field's classified type, byte offset, byte size, and MIR
    /// type id (for continuing a place walk), from the struct's
    /// deterministic layout.
    fn resolve_member(
        &mut self,
        base_ty: TypeId,
        member: &crate::mir::MirName,
        span: Span,
    ) -> Result<(BType, u32, u32, TypeId), BackendError> {
        // Check if the base type is a tuple (session 29): the member
        // name is the element index as a string.
        if let Some(TypeKind::Tuple(elems)) = self.program.types.kind(base_ty) {
            let index: usize = member.name.parse().map_err(|_| {
                BackendError::invalid_backend_ir(
                    member.span,
                    format!("invalid tuple index `{}`", member.name),
                )
            })?;
            if index >= elems.len() {
                return Err(BackendError::invalid_backend_ir(
                    member.span,
                    format!(
                        "tuple index `{}` is out of range; the tuple has {} element{}",
                        member.name,
                        elems.len(),
                        if elems.len() == 1 { "" } else { "s" }
                    ),
                ));
            }
            let elem_ty = elems[index];
            let bt = self
                .classify(elem_ty)
                .ok_or_else(|| self.unsupported_type_error(member.span, elem_ty))?;
            let tl = layout::tuple_layout(elems, &self.program.types)
                .map_err(|e| BackendError::invalid_backend_ir(span, layout_error_message(&e)))?;
            let fl = &tl.fields[index];
            return Ok((bt, fl.offset as u32, fl.size as u32, elem_ty));
        }
        let (info, slayout) = self.struct_layout_of(base_ty, span)?;
        let index = info
            .fields
            .iter()
            .position(|field| field.name == member.name)
            .ok_or_else(|| {
                BackendError::invalid_backend_ir(
                    member.span,
                    format!("the struct `{}` has no field `{}`", info.name, member.name),
                )
            })?;
        let field = &info.fields[index];
        let field_ty = self
            .classify(field.ty)
            .ok_or_else(|| self.unsupported_type_error(member.span, field.ty))?;
        let field_layout = slayout.fields[index];
        Ok((
            field_ty,
            field_layout.offset as u32,
            field_layout.size as u32,
            field.ty,
        ))
    }

    /// The classified type and byte size of a reference's referent, used
    /// by `RefLoad`/`RefStore`. The referent must classify (a reference to
    /// an unsupported type is itself unsupported — `classify` already
    /// rejects it, so this is defensive).
    fn referent_info(&mut self, ref_ty: TypeId, span: Span) -> Result<(BType, u32), BackendError> {
        let elem = match self.program.types.kind(ref_ty) {
            Some(TypeKind::Ref { elem, .. }) => *elem,
            _ => {
                return Err(BackendError::invalid_backend_ir(
                    span,
                    "a deref operand is not a reference",
                ));
            }
        };
        let classified = self.classify(elem).ok_or_else(|| {
            BackendError::unsupported_type(
                span,
                format!(
                    "cannot dereference a reference to `{}`: the referent type is not supported",
                    self.display(elem)
                ),
            )
        })?;
        let size = match classified {
            BType::Struct | BType::Array => self.aggregate_bytes(elem),
            BType::Int
            | BType::Float
            | BType::Null
            | BType::Ptr
            | BType::Ref
            | BType::Str
            | BType::Enum => 8,
            BType::Bool | BType::Char => 1,
            BType::Range => 16,
            BType::Unit => 0,
        };
        Ok((classified, size))
    }

    /// The exact byte size of an aggregate-typed value, from its layout.
    fn aggregate_bytes(&self, ty: TypeId) -> u32 {
        match self.program.types.kind(ty) {
            Some(TypeKind::Struct(id)) => layout::struct_layout(*id, &self.program.types)
                .map(|layout| layout.size as u32)
                .unwrap_or(0),
            Some(TypeKind::Array { .. }) => layout::array_layout(ty, &self.program.types)
                .map(|layout| layout.size as u32)
                .unwrap_or(0),
            _ => 0,
        }
    }

    /// The exact byte size of a value of `ty`, from its layout (structs,
    /// arrays, and tagged-union enums) or its scalar width. Used to size
    /// the payload copy of an [`MirRvalueKind::EnumPayload`] extraction.
    fn value_byte_size(&mut self, ty: TypeId) -> Option<u32> {
        match self.classify(ty)? {
            BType::Struct | BType::Array => Some(self.aggregate_bytes(ty)),
            BType::Enum => {
                let id = self.program.types.enum_id(ty)?;
                layout::enum_layout(id, &self.program.types)
                    .ok()
                    .map(|layout| layout.size as u32)
            }
            // `Null` is word-sized (the layout treats it like a pointer
            // slot); `Char` is a single byte (layout `(1, 1)`).
            BType::Int | BType::Ptr | BType::Ref | BType::Str | BType::Float | BType::Null => {
                Some(8)
            }
            BType::Bool | BType::Char => Some(1),
            BType::Range => Some(16),
            BType::Unit => Some(0),
        }
    }

    /// Resolves a multi-step storage place into the address steps the
    /// emitter walks (field byte offsets and bounds-checked index strides)
    /// plus the target's byte size, walking the chain from the root
    /// local's MIR type through the deterministic layout.
    fn resolve_place(
        &mut self,
        root: crate::mir::LocalId,
        steps: &[crate::mir::MirPlaceStep],
        span: Span,
    ) -> Result<(Vec<super::ir::PlaceAddrStep>, u32), BackendError> {
        let mut current_ty = self
            .fn_local_types
            .get(root.raw() as usize)
            .copied()
            .ok_or_else(|| {
                BackendError::invalid_backend_ir(span, "a place root references an unknown local")
            })?;
        let mut addr_steps = Vec::with_capacity(steps.len());
        let mut size = 0u32;
        for step in steps {
            match &step.kind {
                crate::mir::MirPlaceStepKind::Field(member) => {
                    let (_, offset, field_size, field_ty) =
                        self.resolve_member(current_ty, member, span)?;
                    addr_steps.push(super::ir::PlaceAddrStep::Field {
                        byte_offset: offset,
                    });
                    size = field_size;
                    current_ty = field_ty;
                }
                crate::mir::MirPlaceStepKind::Index(index) => {
                    let (_, stride, len) = self.resolve_array(current_ty, span)?;
                    let index_op = self.eval_operand(index)?;
                    addr_steps.push(super::ir::PlaceAddrStep::Index {
                        index: index_op,
                        stride,
                        len,
                    });
                    current_ty = match self.program.types.kind(current_ty) {
                        Some(TypeKind::Array { elem, .. }) => *elem,
                        // Defensive: a clean pipeline always indexes arrays.
                        _ => current_ty,
                    };
                    size = stride;
                }
            }
        }
        Ok((addr_steps, size))
    }

    /// Resolves the array type `ty` into the element's classified type,
    /// the element byte size (the array's stride), and the array's
    /// length.
    fn resolve_array(&mut self, ty: TypeId, span: Span) -> Result<(BType, u32, u64), BackendError> {
        let (elem, len) = match self.program.types.kind(ty) {
            Some(TypeKind::Array { elem, len }) => (*elem, *len),
            _ => {
                return Err(BackendError::invalid_backend_ir(
                    span,
                    "an index access base is not an array value",
                ));
            }
        };
        let elem_ty = self
            .classify(elem)
            .ok_or_else(|| self.unsupported_type_error(span, elem))?;
        let layout = layout::array_layout(ty, &self.program.types).map_err(|error| {
            BackendError::invalid_backend_ir(span, layout_error_message(&error))
        })?;
        Ok((elem_ty, layout.elem_size as u32, len))
    }

    /// Evaluates an operand into its backend form, decoding constants and
    /// materializing module-binding reads.
    ///
    /// A module-binding operand is read through an explicit load into a
    /// temporary slot.
    fn eval_operand(&mut self, operand: &MirOperand) -> Result<BOperand, BackendError> {
        match &operand.kind {
            MirOperandKind::Local(id) => Ok(BOperand::Local(*id)),
            MirOperandKind::Constant(constant) => match self.decode_constant(constant)? {
                DecodedConstant::Word(value) => Ok(BOperand::Const(value)),
                // A string literal has no machine constant form: its value
                // is the blob's address, loaded through an explicit
                // instruction into a temporary slot.
                DecodedConstant::Str(string_index) => {
                    let temp = self.alloc_temp(operand.ty, BType::Str, constant.span);
                    self.push(
                        BInstKind::LoadStr {
                            target: temp,
                            string_index,
                        },
                        constant.span,
                    );
                    Ok(BOperand::Local(temp))
                }
            },
            MirOperandKind::Static(symbol) => {
                let slot = self.resolve_static(*symbol, operand.span)?;
                let ty = self.static_types[slot]
                    .expect("unsupported statics are skipped before operand evaluation");
                let temp = self.alloc_temp(operand.ty, ty, operand.span);
                self.push(
                    BInstKind::LoadStatic {
                        target: temp,
                        static_index: slot,
                    },
                    operand.span,
                );
                Ok(BOperand::Local(temp))
            }
        }
    }

    /// Decodes a literal constant into its backend form: a machine word
    /// (integer or boolean) or a reference to a decoded string blob.
    ///
    /// Integers and strings are decoded from the literal's source text (the
    /// backend is the first stage to decode literal values; the text is
    /// recovered via the source map). Booleans carry their value directly.
    /// Identical string literals are deduplicated: they share one image
    /// blob. Every other literal kind is rejected — and, since unsupported
    /// literal kinds always pair with unsupported local types, this path is
    /// defensive: a clean pipeline reports the type error first.
    fn decode_constant(
        &mut self,
        constant: &crate::mir::MirConstant,
    ) -> Result<DecodedConstant, BackendError> {
        match constant.kind {
            MirConstantKind::Bool(value) => Ok(DecodedConstant::Word(i64::from(value))),
            MirConstantKind::Int => Ok(DecodedConstant::Word(self.decode_int(constant.span)?)),
            // An enum variant constant (session 17): the discriminant is
            // already computed by the front end; the word value is the
            // discriminant itself.
            MirConstantKind::Enum { variant } => Ok(DecodedConstant::Word(variant)),
            MirConstantKind::Str => {
                let bytes = self.decode_str(constant.span)?;
                let index = match self.string_index.get(&bytes) {
                    Some(&index) => index,
                    None => {
                        let index = self.strings.len();
                        self.strings.push(BString {
                            bytes: bytes.clone(),
                            span: constant.span,
                        });
                        self.string_index.insert(bytes, index);
                        index
                    }
                };
                Ok(DecodedConstant::Str(index))
            }
            // Session 24: floating-point and character literals are
            // decoded from their source text (their values are carried as
            // words — the double's bit pattern, the character's byte); the
            // `null` literal is the zero word.
            MirConstantKind::Float => Ok(DecodedConstant::Word(self.decode_float(constant.span)?)),
            MirConstantKind::Char => Ok(DecodedConstant::Word(self.decode_char(constant.span)?)),
            MirConstantKind::Null => Ok(DecodedConstant::Word(0)),
        }
    }

    /// Decodes a string literal's source text (including its quotes) into
    /// the literal's UTF-8 byte contents, decoding every escape sequence
    /// (`\n`, `\r`, `\t`, `\0`, `\\`, `\"`, `\'`, `\xHH`, `\u{...}`).
    /// The lexer validates escapes, so this path is defensive: malformed
    /// text is reported as a structured decode error instead of panicking.
    fn decode_str(&self, span: Span) -> Result<Vec<u8>, BackendError> {
        let Some(file) = self.sources.get(span.file()) else {
            return Err(BackendError::decode_error(
                span,
                "no source file for the string literal",
            ));
        };
        let Some(text) = file.span_text(span) else {
            return Err(BackendError::decode_error(
                span,
                "string literal text is unavailable",
            ));
        };
        let bytes = text.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'"' || bytes[bytes.len() - 1] != b'"' {
            return Err(BackendError::decode_error(
                span,
                "malformed string literal token",
            ));
        }
        let mut out = Vec::with_capacity(bytes.len() - 2);
        let mut index = 1;
        let end = bytes.len() - 1;
        while index < end {
            let byte = bytes[index];
            if byte == b'\\' {
                index += 1;
                if index >= end {
                    return Err(BackendError::decode_error(
                        span,
                        "unterminated escape sequence in string literal",
                    ));
                }
                let escape = bytes[index];
                index += 1;
                match escape {
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'0' => out.push(0),
                    b'\\' => out.push(b'\\'),
                    b'"' => out.push(b'"'),
                    b'\'' => out.push(b'\''),
                    b'x' => {
                        if index + 2 > end {
                            return Err(BackendError::decode_error(
                                span,
                                "incomplete `\\x` escape in string literal",
                            ));
                        }
                        let hi = hex_digit(bytes[index], span)?;
                        let lo = hex_digit(bytes[index + 1], span)?;
                        out.push(hi << 4 | lo);
                        index += 2;
                    }
                    b'u' => {
                        if index >= end || bytes[index] != b'{' {
                            return Err(BackendError::decode_error(
                                span,
                                "malformed `\\u` escape in string literal",
                            ));
                        }
                        index += 1;
                        let digits_start = index;
                        let mut value: u32 = 0;
                        while index < end && bytes[index] != b'}' {
                            if index - digits_start >= 6 {
                                return Err(BackendError::decode_error(
                                    span,
                                    "`\\u` escape has more than six digits",
                                ));
                            }
                            value =
                                value.wrapping_mul(16) + u32::from(hex_digit(bytes[index], span)?);
                            index += 1;
                        }
                        if index >= end || bytes[index] != b'}' || index == digits_start {
                            return Err(BackendError::decode_error(
                                span,
                                "malformed `\\u` escape in string literal",
                            ));
                        }
                        index += 1;
                        let ch = char::from_u32(value).ok_or_else(|| {
                            BackendError::decode_error(
                                span,
                                "`\\u` escape is not a valid Unicode scalar value",
                            )
                        })?;
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                    }
                    _ => {
                        return Err(BackendError::decode_error(
                            span,
                            "unknown escape sequence in string literal",
                        ));
                    }
                }
            } else {
                // A raw character: copy its full UTF-8 encoding.
                let ch = text[index..end].chars().next().ok_or_else(|| {
                    BackendError::decode_error(span, "malformed UTF-8 in string literal")
                })?;
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                index += ch.len_utf8();
            }
        }
        Ok(out)
    }

    /// Decodes an integer literal's source text into a 64-bit two's
    /// complement value, supporting decimal, `0x`/`0o`/`0b` radix prefixes,
    /// and `_` digit separators (the lexer's literal forms). Values that do
    /// not fit wrap, matching the 64-bit integer model.
    fn decode_int(&self, span: Span) -> Result<i64, BackendError> {
        let Some(file) = self.sources.get(span.file()) else {
            return Err(BackendError::decode_error(
                span,
                "no source file for the literal",
            ));
        };
        let Some(text) = file.span_text(span) else {
            return Err(BackendError::decode_error(
                span,
                "literal text is unavailable",
            ));
        };
        let (radix, digits) =
            if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                (16, rest)
            } else if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
                (8, rest)
            } else if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
                (2, rest)
            } else {
                (10, text)
            };
        let mut value: u64 = 0;
        for byte in digits.bytes() {
            if byte == b'_' {
                continue;
            }
            let digit = match byte {
                b'0'..=b'9' => u64::from(byte - b'0'),
                b'a'..=b'f' => u64::from(byte - b'a' + 10),
                b'A'..=b'F' => u64::from(byte - b'A' + 10),
                // A clean pipeline never produces an out-of-radix character
                // (the lexer would have rejected the literal).
                _ => {
                    return Err(BackendError::decode_error(
                        span,
                        "literal contains a character that is not a digit of its radix",
                    ));
                }
            };
            if digit >= radix as u64 {
                return Err(BackendError::decode_error(
                    span,
                    "literal contains a digit outside its radix",
                ));
            }
            value = value.wrapping_mul(radix as u64).wrapping_add(digit);
        }
        Ok(value as i64)
    }

    /// Decodes a floating-point literal's source text into its 64-bit
    /// IEEE-754 bit pattern (stored as a machine word). Digit separators
    /// (`_`) are stripped; the lexer has already validated the literal's
    /// shape, so a parse failure here is defensive.
    fn decode_float(&self, span: Span) -> Result<i64, BackendError> {
        let Some(file) = self.sources.get(span.file()) else {
            return Err(BackendError::decode_error(
                span,
                "no source file for the floating-point literal",
            ));
        };
        let Some(text) = file.span_text(span) else {
            return Err(BackendError::decode_error(
                span,
                "floating-point literal text is unavailable",
            ));
        };
        let cleaned: String = text.chars().filter(|c| *c != '_').collect();
        let value: f64 = cleaned
            .parse()
            .map_err(|_| BackendError::decode_error(span, "invalid floating-point literal"))?;
        Ok(value.to_bits() as i64)
    }

    /// Decodes a character literal's source text (including its quotes)
    /// into its byte value: the character's code point, which must fit in
    /// one byte (the runtime's char model is byte-sized, matching the
    /// `(1, 1)` char layout). Escapes (`\n`, `\r`, `\t`, `\0`, `\\`,
    /// `\"`, `\'`, `\xHH`, `\u{...}`) decode to their scalar value; a
    /// scalar above 255 has no byte representation and is rejected
    /// deterministically rather than silently truncated. The lexer has
    /// already validated the literal, so failures here are defensive.
    fn decode_char(&self, span: Span) -> Result<i64, BackendError> {
        let Some(file) = self.sources.get(span.file()) else {
            return Err(BackendError::decode_error(
                span,
                "no source file for the character literal",
            ));
        };
        let Some(text) = file.span_text(span) else {
            return Err(BackendError::decode_error(
                span,
                "character literal text is unavailable",
            ));
        };
        let bytes = text.as_bytes();
        if bytes.len() < 3 || bytes[0] != b'\'' || bytes[bytes.len() - 1] != b'\'' {
            return Err(BackendError::decode_error(
                span,
                "malformed character literal token",
            ));
        }
        let inner = &bytes[1..bytes.len() - 1];
        let scalar: u32 = if inner[0] == b'\\' {
            self.decode_char_escape(inner, span)?
        } else {
            // A single raw character (the lexer guarantees exactly one).
            let s = std::str::from_utf8(inner).map_err(|_| {
                BackendError::decode_error(span, "character literal is not valid UTF-8")
            })?;
            let mut chars = s.chars();
            let ch = chars
                .next()
                .ok_or_else(|| BackendError::decode_error(span, "empty character literal"))?;
            if chars.next().is_some() {
                return Err(BackendError::decode_error(
                    span,
                    "character literal contains more than one character",
                ));
            }
            ch as u32
        };
        if scalar > 0xFF {
            return Err(BackendError::decode_error(
                span,
                "character value does not fit in one byte; the native char model is byte-sized",
            ));
        }
        Ok(i64::from(scalar))
    }

    /// Decodes one character-literal escape (the inner bytes after the
    /// backslash) into its Unicode scalar value.
    fn decode_char_escape(&self, inner: &[u8], span: Span) -> Result<u32, BackendError> {
        let escape = *inner.get(1).ok_or_else(|| {
            BackendError::decode_error(span, "unterminated escape sequence in character literal")
        })?;
        let scalar = match escape {
            b'n' => u32::from(b'\n'),
            b'r' => u32::from(b'\r'),
            b't' => u32::from(b'\t'),
            b'0' => 0,
            b'\\' => u32::from(b'\\'),
            b'"' => u32::from(b'"'),
            b'\'' => u32::from(b'\''),
            b'x' => {
                if inner.len() != 4 {
                    return Err(BackendError::decode_error(
                        span,
                        "incomplete `\\x` escape in character literal",
                    ));
                }
                let hi = hex_digit(inner[2], span)?;
                let lo = hex_digit(inner[3], span)?;
                u32::from(hi << 4 | lo)
            }
            b'u' => {
                if inner.len() < 4 || inner[2] != b'{' || inner[inner.len() - 1] != b'}' {
                    return Err(BackendError::decode_error(
                        span,
                        "malformed `\\u` escape in character literal",
                    ));
                }
                let digits = &inner[3..inner.len() - 1];
                if digits.is_empty() || digits.len() > 6 {
                    return Err(BackendError::decode_error(
                        span,
                        "`\\u` escape has an invalid digit count",
                    ));
                }
                let mut value: u32 = 0;
                for byte in digits {
                    value = value
                        .wrapping_mul(16)
                        .wrapping_add(u32::from(hex_digit(*byte, span)?));
                }
                if char::from_u32(value).is_none() {
                    return Err(BackendError::decode_error(
                        span,
                        "`\\u` escape is not a valid Unicode scalar value",
                    ));
                }
                value
            }
            _ => {
                return Err(BackendError::decode_error(
                    span,
                    "unknown escape sequence in character literal",
                ));
            }
        };
        Ok(scalar)
    }

    /// Lowers a block's terminator, pushing any temporary loads its operand
    /// evaluation needs.
    fn lower_terminator(&mut self, terminator: &MirTerminator) -> BTerminator {
        match terminator {
            MirTerminator::Return { value, span } => {
                let value = match value {
                    Some(operand) if !self.operand_is_unsupported(operand) => {
                        match self.eval_operand(operand) {
                            Ok(operand) => Some(operand),
                            Err(error) => {
                                self.errors.push(error);
                                None
                            }
                        }
                    }
                    // A bare return, or a return of an unsupported value
                    // whose error was already reported.
                    _ => None,
                };
                BTerminator::Return { value, span: *span }
            }
            MirTerminator::Jump { target, span } => BTerminator::Jump {
                target: *target,
                span: *span,
            },
            MirTerminator::Branch {
                cond,
                then_block,
                else_block,
                span,
            } => {
                let cond = if self.operand_is_unsupported(cond) {
                    BOperand::Const(0)
                } else {
                    match self.eval_operand(cond) {
                        Ok(operand) => operand,
                        Err(error) => {
                            self.errors.push(error);
                            BOperand::Const(0)
                        }
                    }
                };
                BTerminator::Branch {
                    cond,
                    then_block: *then_block,
                    else_block: *else_block,
                    span: *span,
                }
            }
        }
    }
}
