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
        }
        BInstKind::StoreStatic { static_index, src } => {
            verify_static_index(program, *static_index, inst.span, errors);
            verify_operand(function, src, inst.span, errors);
        }
        BInstKind::Unary { target, op, src } => {
            verify_target(function, *target, inst.span, errors);
            verify_operand(function, src, inst.span, errors);
            verify_unary_types(function, *target, *op, src, inst.span, errors);
        }
        BInstKind::Binary {
            target,
            op,
            lhs,
            rhs,
        } => {
            verify_target(function, *target, inst.span, errors);
            verify_operand(function, lhs, inst.span, errors);
            verify_operand(function, rhs, inst.span, errors);
            verify_binary_types(function, *target, *op, lhs, rhs, inst.span, errors);
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
/// operator's operand type. Constants carry no distinct boolean/integer
/// marker at the machine level, so they pass the operand check.
fn verify_unary_types(
    function: &super::ir::BFunction,
    target: LocalId,
    op: UnaryOp,
    src: &BOperand,
    span: Span,
    errors: &mut Vec<BackendError>,
) {
    let (expected_operand, expected_result) = match op {
        UnaryOp::Neg | UnaryOp::BitNot => (BType::Int, BType::Int),
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
/// `Bool` for logical operators; `Int` or `Bool` for equality. Constants
/// pass the operand check (see [`verify_unary_types`]).
fn verify_binary_types(
    function: &super::ir::BFunction,
    target: LocalId,
    op: BinaryOp,
    lhs: &BOperand,
    rhs: &BOperand,
    span: Span,
    errors: &mut Vec<BackendError>,
) {
    use BinaryOp::*;
    let produces_bool = matches!(op, Lt | Le | Gt | Ge | Eq | Ne | And | Or);
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
