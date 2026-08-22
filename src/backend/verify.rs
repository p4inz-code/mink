//! Backend instruction verification.
//!
//! [`verify`] checks the structural integrity of a [`BProgram`] before an
//! emitter consumes it: blocks are ordered by id with the entry block
//! first, parameter locals are the first locals, every instruction and
//! terminator references locals, blocks, statics, and functions that exist,
//! and every reference has a consistent type. Lowering always produces
//! valid programs, so this defends the pipeline and tooling against
//! malformed hand-built or mutated programs — reporting every problem as a
//! structured [`BackendError`] (`E-B07`) instead of panicking.

use crate::ast::{BinaryOp, UnaryOp};
use crate::mir::{BlockId, LocalId};
use crate::source::Span;

use super::error::BackendError;
use super::ir::{BInstKind, BOperand, BProgram, BTerminator, BType};

/// Verifies the structural integrity of `program`.
///
/// Returns every problem found as a [`BackendError`] (`E-B07`), in
/// deterministic order, or `Ok(())` when the program is valid.
pub(crate) fn verify(program: &BProgram) -> Result<(), Vec<BackendError>> {
    let mut errors = Vec::new();
    for function in &program.functions {
        verify_function(program, function, &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn error(span: Span, detail: impl Into<String>) -> BackendError {
    BackendError::invalid_backend_ir(span, detail)
}

fn verify_function(
    program: &BProgram,
    function: &super::ir::BFunction,
    errors: &mut Vec<BackendError>,
) {
    // Blocks ordered by id, entry block first.
    for (index, block) in function.blocks.iter().enumerate() {
        if block.id.raw() as usize != index {
            errors.push(error(
                block.span,
                format!(
                    "function `{}`: block {} is not ordered (index {index})",
                    function.name,
                    block.id.raw()
                ),
            ));
        }
    }
    if function
        .blocks
        .first()
        .is_some_and(|block| block.id != function.entry())
    {
        errors.push(error(
            function.span,
            format!("function `{}`: entry block is not block 0", function.name),
        ));
    }
    // Parameter locals are the first locals.
    for (index, param) in function.params.iter().enumerate() {
        if param.raw() as usize != index {
            errors.push(error(
                function.span,
                format!(
                    "function `{}`: parameter {index} is not the local at index {index}",
                    function.name
                ),
            ));
        }
    }
    for block in &function.blocks {
        for inst in &block.insts {
            verify_inst(program, function, inst, errors);
        }
        verify_terminator(function, &block.terminator, errors);
    }
}

/// The local's slot type; a dangling id reports `None`.
fn local_type(function: &super::ir::BFunction, id: LocalId) -> Option<BType> {
    function.local(id).map(|local| local.ty)
}

fn verify_operand(
    function: &super::ir::BFunction,
    operand: &BOperand,
    span: Span,
    errors: &mut Vec<BackendError>,
) {
    if let BOperand::Local(id) = operand {
        if local_type(function, *id).is_none() {
            errors.push(error(
                span,
                format!(
                    "function `{}`: operand references unknown local {}",
                    function.name,
                    id.raw()
                ),
            ));
        }
    }
}

fn verify_inst(
    program: &BProgram,
    function: &super::ir::BFunction,
    inst: &super::ir::BInst,
    errors: &mut Vec<BackendError>,
) {
    match &inst.kind {
        BInstKind::LoadLocal { target, src } => {
            verify_target(function, *target, inst.span, errors);
            if local_type(function, *src).is_none() {
                errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: load references unknown local {}",
                        function.name,
                        src.raw()
                    ),
                ));
            }
        }
        BInstKind::LoadConst { target, .. } => {
            verify_target(function, *target, inst.span, errors);
        }
        BInstKind::LoadStatic {
            target,
            static_index,
        } => {
            verify_target(function, *target, inst.span, errors);
            verify_static_index(program, *static_index, inst.span, errors);
            // The target slot must match the binding's shape: the same
            // word count, and a matching classified type for word
            // bindings (aggregate bindings land in aggregate slots).
            if let Some(static_binding) = program.statics.get(*static_index) {
                let words = static_binding.bytes.len() / 8;
                let local_words = function.local(*target).map(|local| local.words as usize);
                // Sub-word values carry an extra guard word (session 22
                // slot padding), so the slot may be one word wider than
                // the binding's image region.
                if local_words.is_none() || local_words.unwrap() < words {
                    errors.push(error(
                        inst.span,
                        format!(
                            "function `{}`: a static load writes a slot of {} word(s), but binding {} occupies {words}",
                            function.name,
                            local_words.unwrap_or(0),
                            static_index
                        ),
                    ));
                }
                if words == 1 {
                    if let Some(local) = function.local(*target) {
                        // Word bindings are integers, booleans, or unit-
                        // only enum discriminants; the slot must carry the
                        // same classified type.
                        let compatible = local.ty == static_binding.ty;
                        if !compatible {
                            errors.push(error(
                                inst.span,
                                format!(
                                    "function `{}`: a static load must write a slot matching the binding's type",
                                    function.name
                                ),
                            ));
                        }
                    }
                }
            }
        }
        BInstKind::LoadFnPtr {
            target,
            function_index,
        } => {
            verify_target(function, *target, inst.span, errors);
            if program.functions.get(*function_index).is_none() {
                errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: LoadFnPtr references unknown function index {function_index}",
                        function.name
                    ),
                ));
            }
            if function.local(*target).map(|local| local.ty) != Some(BType::FnPtr) {
                errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: LoadFnPtr target must be FnPtr",
                        function.name
                    ),
                ));
            }
        }
        BInstKind::LoadStr {
            target,
            string_index,
        } => {
            verify_target(function, *target, inst.span, errors);
            if program.strings.get(*string_index).is_none() {
                errors.push(error(
                    inst.span,
                    format!("instruction references unknown string literal {string_index}"),
                ));
            }
            if function.local(*target).map(|local| local.ty) != Some(BType::Str) {
                errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: a string load must write a string slot",
                        function.name
                    ),
                ));
            }
        }
        BInstKind::StoreStatic { static_index, src } => {
            verify_static_index(program, *static_index, inst.span, errors);
            verify_operand(function, src, inst.span, errors);
            // A stored local must carry the binding's full width (a
            // multi-word binding is written as a whole value).
            if let BOperand::Local(id) = src {
                if let Some(static_binding) = program.statics.get(*static_index) {
                    let words = static_binding.bytes.len() / 8;
                    // The source slot may carry the guard word for
                    // sub-word bindings; it must at least span the
                    // binding's image region.
                    if function
                        .local(*id)
                        .map(|local| local.words as usize)
                        .unwrap_or(0)
                        < words
                    {
                        errors.push(error(
                            inst.span,
                            format!(
                                "function `{}`: a static store writes {} word(s) into binding {}, which occupies {words}",
                                function.name,
                                function.local(*id).map(|local| local.words as usize).unwrap_or(0),
                                static_index
                            ),
                        ));
                    }
                }
            }
        }
        BInstKind::Unary { target, op, src } => {
            verify_target(function, *target, inst.span, errors);
            verify_operand(function, src, inst.span, errors);
            verify_unary_types(function, *target, *op, src, inst.span, errors);
        }
        BInstKind::Binary {
            target,
            op,
            ty,
            lhs,
            rhs,
        } => {
            verify_target(function, *target, inst.span, errors);
            verify_operand(function, lhs, inst.span, errors);
            verify_operand(function, rhs, inst.span, errors);
            verify_binary_types(function, *target, *op, *ty, lhs, rhs, inst.span, errors);
        }
        BInstKind::Call {
            target,
            callee,
            args,
        } => {
            verify_target(function, *target, inst.span, errors);
            if program.functions.get(*callee).is_none() {
                errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: call references unknown function index {callee}",
                        function.name
                    ),
                ));
            }
            // A multi-word result (session 22) is returned through the
            // caller-allocated slot; the target slot must span the callee's
            // result width.
            if let Some(callee_fn) = program.functions.get(*callee) {
                if callee_fn.result_words > 1
                    && function.local(*target).map(|local| local.words)
                        != Some(callee_fn.result_words)
                {
                    errors.push(error(
                        inst.span,
                        format!(
                            "function `{}`: call result slot does not span the callee's {} result word(s)",
                            function.name,
                            callee_fn.result_words
                        ),
                    ));
                }
            }
            for arg in args {
                verify_operand(function, arg, inst.span, errors);
            }
        }
        BInstKind::RuntimeCall {
            target,
            service,
            args,
        } => {
            verify_target(function, *target, inst.span, errors);
            // Only the callable subset of services is reachable from
            // generated code; the rest are entry-stub or internal.
            if !service.is_callable() {
                errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: runtime service `{service:?}` is not callable from generated code",
                        function.name
                    ),
                ));
            }
            if args.len() != service.arity() {
                errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: runtime service `{service:?}` expects {} argument(s), found {}",
                        function.name,
                        service.arity(),
                        args.len()
                    ),
                ));
            }
            for arg in args {
                verify_operand(function, arg, inst.span, errors);
            }
        }
        BInstKind::IndirectCall {
            target,
            fn_ptr,
            args,
        } => {
            verify_target(function, *target, inst.span, errors);
            verify_operand(function, fn_ptr, inst.span, errors);
            for arg in args {
                verify_operand(function, arg, inst.span, errors);
            }
        }
        BInstKind::RangeInit {
            target, start, end, ..
        } => {
            verify_target(function, *target, inst.span, errors);
            verify_operand(function, start, inst.span, errors);
            verify_operand(function, end, inst.span, errors);
        }
        BInstKind::RangeNext { target, range } => {
            verify_target(function, *target, inst.span, errors);
            verify_range_operand(function, *range, inst.span, errors);
            if function.local(*target).map(|local| local.ty) != Some(BType::Int) {
                errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: a range-next result must be an integer",
                        function.name
                    ),
                ));
            }
        }
        BInstKind::RangeFinished { target, range } => {
            verify_target(function, *target, inst.span, errors);
            verify_range_operand(function, *range, inst.span, errors);
            if function.local(*target).map(|local| local.ty) != Some(BType::Bool) {
                errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: a range-completion result must be a boolean",
                        function.name
                    ),
                ));
            }
        }
        BInstKind::FieldLoad {
            target,
            base,
            field_ty,
            byte_offset,
            size,
        } => {
            verify_target(function, *target, inst.span, errors);
            verify_aggregate_base(function, *base, BType::Struct, inst.span, errors);
            if function.local(*target).map(|local| local.ty) != Some(*field_ty) {
                errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: a field load result must match the field type",
                        function.name
                    ),
                ));
            }
            let _ = (byte_offset, size);
        }
        BInstKind::FieldStore {
            base,
            field_ty,
            src,
            ..
        } => {
            verify_aggregate_base(function, *base, BType::Struct, inst.span, errors);
            verify_operand(function, src, inst.span, errors);
            if let BOperand::Local(id) = src {
                if function.local(*id).map(|local| local.ty) != Some(*field_ty) {
                    errors.push(error(
                        inst.span,
                        format!(
                            "function `{}`: a field store source must match the field type",
                            function.name
                        ),
                    ));
                }
            }
        }
        BInstKind::IndexLoad {
            target,
            base,
            elem_ty,
            len,
            index,
            ..
        } => {
            verify_target(function, *target, inst.span, errors);
            verify_aggregate_base(function, *base, BType::Array, inst.span, errors);
            verify_operand(function, index, inst.span, errors);
            if function.local(*target).map(|local| local.ty) != Some(*elem_ty) {
                errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: an index load result must match the element type",
                        function.name
                    ),
                ));
            }
            let _ = len;
        }
        BInstKind::IndexStore {
            base,
            elem_ty,
            len,
            index,
            src,
            ..
        } => {
            verify_aggregate_base(function, *base, BType::Array, inst.span, errors);
            verify_operand(function, index, inst.span, errors);
            verify_operand(function, src, inst.span, errors);
            if let BOperand::Local(id) = src {
                if function.local(*id).map(|local| local.ty) != Some(*elem_ty) {
                    errors.push(error(
                        inst.span,
                        format!(
                            "function `{}`: an index store source must match the element type",
                            function.name
                        ),
                    ));
                }
            }
            let _ = len;
        }
        BInstKind::PlaceStore {
            base, steps, src, ..
        } => {
            // The root of a place chain may be a struct or an array
            // (e.g. `arr[0].f = x`), so accept either aggregate kind.
            match function.local(*base).map(|local| local.ty) {
                Some(BType::Struct) | Some(BType::Array) => {}
                _ => errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: a place store must root at a struct or array",
                        function.name
                    ),
                )),
            }
            verify_operand(function, src, inst.span, errors);
            // Every index step's operand must be well-typed.
            for step in steps {
                if let crate::backend::ir::PlaceAddrStep::Index { index, .. } = step {
                    verify_operand(function, index, inst.span, errors);
                }
            }
        }
        BInstKind::RefAddr {
            target,
            base,
            steps,
            ..
        } => {
            // A reference address is a word-sized slot computed from a
            // struct/array/any local root.
            verify_target(function, *target, inst.span, errors);
            if function.local(*target).map(|local| local.ty) != Some(BType::Ref) {
                errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: a reference address must write a reference slot",
                        function.name
                    ),
                ));
            }
            if function.local(*base).is_none() {
                errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: a reference address references an unknown local {}",
                        function.name,
                        base.raw()
                    ),
                ));
            }
            // Every index step's operand must be well-typed.
            for step in steps {
                if let crate::backend::ir::PlaceAddrStep::Index { index, .. } = step {
                    verify_operand(function, index, inst.span, errors);
                }
            }
        }
        BInstKind::RefLoad {
            target,
            reference,
            elem_ty,
            ..
        } => {
            verify_target(function, *target, inst.span, errors);
            verify_operand(function, reference, inst.span, errors);
            // A reference load writes the referent's slot: `Int`/`Bool`/
            // `Range` are loaded as their own type; `Ref`, `Ptr`, `Str`
            // and aggregates land in a slot of the same or aggregate kind.
            match (elem_ty, function.local(*target).map(|local| local.ty)) {
                (BType::Ref, Some(BType::Ref))
                | (BType::Ptr, Some(BType::Ptr))
                | (BType::Str, Some(BType::Str))
                | (BType::Int, Some(BType::Int))
                | (BType::Bool, Some(BType::Bool))
                | (BType::Range, Some(BType::Range))
                | (BType::Struct, Some(BType::Struct))
                | (BType::Array, Some(BType::Array)) => {}
                _ => errors.push(error(
                    inst.span,
                    format!(
                        "function `{}`: a reference load writes a mismatched slot",
                        function.name
                    ),
                )),
            }
        }
        BInstKind::RefStore { reference, src, .. } => {
            verify_operand(function, reference, inst.span, errors);
            verify_operand(function, src, inst.span, errors);
            // The reference operand must be a `Ref`-typed value.
            if let BOperand::Local(id) = reference {
                if function.local(*id).map(|local| local.ty) != Some(BType::Ref) {
                    errors.push(error(
                        inst.span,
                        format!(
                            "function `{}`: a reference store must store through a reference",
                            function.name
                        ),
                    ));
                }
            }
        }
        BInstKind::EnumInit {
            target, payload, ..
        } => {
            verify_target(function, *target, inst.span, errors);
            if let Some(payload) = payload {
                verify_operand(function, payload, inst.span, errors);
            }
        }
        BInstKind::EnumTag { target, value, .. } => {
            verify_target(function, *target, inst.span, errors);
            verify_enum_value(function, *value, inst.span, errors);
        }
        BInstKind::EnumPayload { target, value, .. } => {
            verify_target(function, *target, inst.span, errors);
            verify_enum_value(function, *value, inst.span, errors);
        }
    }
}

/// The value of an enum-tag/payload access must be an `Enum`-typed local.
fn verify_enum_value(
    function: &super::ir::BFunction,
    value: LocalId,
    span: Span,
    errors: &mut Vec<BackendError>,
) {
    match function.local(value).map(|local| local.ty) {
        Some(BType::Enum) => {}
        _ => errors.push(error(
            span,
            format!(
                "function `{}`: an enum access reads a local that is not an enum",
                function.name
            ),
        )),
    }
}

/// The base slot of a field/index access must be an aggregate local of the
/// expected kind (`Struct` for field accesses, `Array` for index accesses).
fn verify_aggregate_base(
    function: &super::ir::BFunction,
    base: LocalId,
    expected: BType,
    span: Span,
    errors: &mut Vec<BackendError>,
) {
    match function.local(base).map(|local| local.ty) {
        Some(actual) if actual == expected => {}
        _ => errors.push(error(
            span,
            format!(
                "function `{}`: aggregate access reads a local that is not a {}",
                function.name,
                expected.name()
            ),
        )),
    }
}

fn verify_target(
    function: &super::ir::BFunction,
    target: LocalId,
    span: Span,
    errors: &mut Vec<BackendError>,
) {
    if local_type(function, target).is_none() {
        errors.push(error(
            span,
            format!(
                "function `{}`: instruction writes to unknown local {}",
                function.name,
                target.raw()
            ),
        ));
    }
}

fn verify_static_index(
    program: &BProgram,
    index: usize,
    span: Span,
    errors: &mut Vec<BackendError>,
) {
    if program.statics.get(index).is_none() {
        errors.push(error(
            span,
            format!("instruction references unknown module binding {index}"),
        ));
    }
}

/// The iterated slot of a range instruction must hold a `Range` value.
fn verify_range_operand(
    function: &super::ir::BFunction,
    range: LocalId,
    span: Span,
    errors: &mut Vec<BackendError>,
) {
    match function.local(range).map(|local| local.ty) {
        Some(BType::Range) => {}
        _ => errors.push(error(
            span,
            format!(
                "function `{}`: range iteration reads a local that is not a range",
                function.name
            ),
        )),
    }
}

/// Unary operations: the result must be the operator's result type (`Int`
/// for `-`/`~`, `Bool` for `!`) and a local operand must have the
/// operator's operand type. Negation accepts `Int` or `Float` (the target
/// slot tells them apart); constants carry no distinct boolean/integer/
/// float marker at the machine level, so they pass the operand check.
fn verify_unary_types(
    function: &super::ir::BFunction,
    target: LocalId,
    op: UnaryOp,
    src: &BOperand,
    span: Span,
    errors: &mut Vec<BackendError>,
) {
    let (expected_operand, expected_result) = match op {
        UnaryOp::Neg => {
            // `-x` on a Float target is float negation; on an Int target
            // it is integer negation. A local operand must share the
            // target's numeric type.
            let ty = function
                .local(target)
                .map(|local| local.ty)
                .unwrap_or(BType::Int);
            let operand_ty = if ty == BType::Float {
                BType::Float
            } else {
                BType::Int
            };
            (operand_ty, ty)
        }
        UnaryOp::BitNot => (BType::Int, BType::Int),
        UnaryOp::Not => (BType::Bool, BType::Bool),
    };
    if function.local(target).map(|local| local.ty) != Some(expected_result) {
        errors.push(error(
            span,
            format!(
                "function `{}`: unary result type does not match the operator",
                function.name
            ),
        ));
    }
    if let BOperand::Local(id) = src {
        if function.local(*id).map(|local| local.ty) != Some(expected_operand) {
            errors.push(error(
                span,
                format!(
                    "function `{}`: unary operand type does not match the operator",
                    function.name
                ),
            ));
        }
    }
}

/// Binary operations: the target type must be the operator's result type
/// (operand type for arithmetic/shift/bitwise, `Bool` for every
/// boolean-producing operator) and local operands must have the operator's
/// operand type — `Int` for arithmetic, shifts, bitwise, and comparisons;
/// `Float` for the session-24 SSE2 arithmetic/comparison/equality path;
/// `Bool` for logical operators; `Int`, `Bool`, `Float`, `Char`, `Null`,
/// `Str`, or a unit-only `Enum` for equality. Constants pass the operand
/// check (see [`verify_unary_types`]).
#[allow(clippy::too_many_arguments)] // the verification context is threaded per call
fn verify_binary_types(
    function: &super::ir::BFunction,
    target: LocalId,
    op: BinaryOp,
    ty: BType,
    lhs: &BOperand,
    rhs: &BOperand,
    span: Span,
    errors: &mut Vec<BackendError>,
) {
    use BinaryOp::*;
    let produces_bool = matches!(op, Lt | Le | Gt | Ge | Eq | Ne | And | Or);
    let operand_ty = |operand: &BOperand| match operand {
        BOperand::Local(id) => function.local(*id).map(|local| local.ty),
        BOperand::Const(_) => None,
    };
    let lhs_ty = operand_ty(lhs);
    let rhs_ty = operand_ty(rhs);
    let lhs_ptr = lhs_ty == Some(BType::Ptr);
    let rhs_ptr = rhs_ty == Some(BType::Ptr);
    // Pointer forms: `Ptr ± Int -> Ptr` (byte-addressed arithmetic) and
    // `Ptr == Ptr -> Bool`. Arithmetic allows exactly one pointer side;
    // `-` is directional (the pointer must be the left operand, so only
    // `p - n` is valid), and `+` accepts `p + n` / `n + p`; equality
    // needs both sides to be pointers.
    if lhs_ptr || rhs_ptr {
        let valid = match op {
            Add => lhs_ptr != rhs_ptr,
            Sub => lhs_ptr && !rhs_ptr,
            Eq | Ne => lhs_ptr && rhs_ptr,
            _ => false,
        };
        if !valid {
            errors.push(error(
                span,
                format!(
                    "function `{}`: binary pointer operand combination does not match the operator",
                    function.name
                ),
            ));
        }
        let expected_result = if produces_bool {
            BType::Bool
        } else {
            BType::Ptr
        };
        if function.local(target).map(|local| local.ty) != Some(expected_result) {
            errors.push(error(
                span,
                format!(
                    "function `{}`: binary result type does not match the operator",
                    function.name
                ),
            ));
        }
        // The non-pointer side of pointer arithmetic must be an integer.
        for (operand, is_ptr) in [(lhs, lhs_ptr), (rhs, rhs_ptr)] {
            if let BOperand::Local(id) = operand {
                if !is_ptr && function.local(*id).map(|local| local.ty) != Some(BType::Int) {
                    errors.push(error(
                        span,
                        format!(
                            "function `{}`: the non-pointer operand of pointer arithmetic must be an integer",
                            function.name
                        ),
                    ));
                }
            }
        }
        return;
    }
    // The floating-point path (session 24): arithmetic keeps the
    // operand type, comparisons and equality produce `Bool`, and
    // logical operators can never be float (the type checker pins
    // them to `Bool`).
    if ty == BType::Float {
        if matches!(op, And | Or) {
            errors.push(error(
                span,
                format!(
                    "function `{}`: a float binary instruction cannot be a logical operator",
                    function.name
                ),
            ));
        }
        let expected_result = if produces_bool {
            BType::Bool
        } else {
            BType::Float
        };
        if function.local(target).map(|local| local.ty) != Some(expected_result) {
            errors.push(error(
                span,
                format!(
                    "function `{}`: binary result type does not match the operator",
                    function.name
                ),
            ));
        }
        for operand in [lhs, rhs] {
            if let BOperand::Local(id) = operand {
                if function.local(*id).map(|local| local.ty) != Some(BType::Float) {
                    errors.push(error(
                        span,
                        format!(
                            "function `{}`: binary operand type does not match the operator",
                            function.name
                        ),
                    ));
                }
            }
        }
        return;
    }
    // Equality on the remaining scalars (`Char`, `Null`, `Str`, a
    // unit-only `Enum`) is word equality with a `Bool` result; the
    // generic rules below already accept it (`Eq`/`Ne` skip the
    // operand check and require a `Bool` target).
    let expected_operand: Option<BType> = match op {
        And | Or => Some(BType::Bool),
        Lt | Le | Gt | Ge => Some(BType::Int),
        // Equality accepts any scalar; `Int` and `Bool` share one word.
        Eq | Ne => None,
        _ => Some(BType::Int),
    };
    let expected_result = if produces_bool {
        BType::Bool
    } else {
        BType::Int
    };
    if function.local(target).map(|local| local.ty) != Some(expected_result) {
        errors.push(error(
            span,
            format!(
                "function `{}`: binary result type does not match the operator",
                function.name
            ),
        ));
    }
    if let Some(expected_operand) = expected_operand {
        for operand in [lhs, rhs] {
            if let BOperand::Local(id) = operand {
                if function.local(*id).map(|local| local.ty) != Some(expected_operand) {
                    errors.push(error(
                        span,
                        format!(
                            "function `{}`: binary operand type does not match the operator",
                            function.name
                        ),
                    ));
                }
            }
        }
    }
}

fn verify_terminator(
    function: &super::ir::BFunction,
    terminator: &BTerminator,
    errors: &mut Vec<BackendError>,
) {
    match terminator {
        BTerminator::Return { value, span } => {
            if let Some(operand) = value {
                verify_operand(function, operand, *span, errors);
            }
        }
        BTerminator::Jump { target, span } => {
            verify_block_target(function, *target, *span, errors);
        }
        BTerminator::Branch {
            cond,
            then_block,
            else_block,
            span,
        } => {
            verify_operand(function, cond, *span, errors);
            verify_block_target(function, *then_block, *span, errors);
            verify_block_target(function, *else_block, *span, errors);
        }
    }
}

fn verify_block_target(
    function: &super::ir::BFunction,
    target: BlockId,
    span: Span,
    errors: &mut Vec<BackendError>,
) {
    if function.blocks.get(target.raw() as usize).is_none() {
        errors.push(error(
            span,
            format!(
                "function `{}`: terminator references unknown block {}",
                function.name,
                target.raw()
            ),
        ));
    }
}
