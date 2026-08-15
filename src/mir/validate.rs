//! Structural MIR validation.
//!
//! [`validate`] checks the integrity of a [`MirProgram`] before it is
//! trusted by later stages and tooling:
//!
//! - **valid block references** — every terminator target exists
//!   (`E-M07`);
//! - **valid local references** — every statement, operand, and parameter
//!   references an existing local (`E-M08`);
//! - **valid type references** — every `TypeId` resolves in the program's
//!   type table (`E-M09`);
//! - **deterministic block ordering** — the block at index `i` has id `i`,
//!   so the entry block is first (`E-M10`);
//! - **valid parameters** — parameters are the first locals, in order
//!   (`E-M11`).
//!
//! Missing terminators are impossible **by construction**: blocks are only
//! produced by the lowering builder, which ends each block with exactly one
//! terminator, and a builder that somehow leaves one unterminated reports
//! `E-M06` (a structured error, never a panic).
//!
//! Lowering always produces valid MIR; validation therefore exists to
//! defend the pipeline and tooling against malformed hand-built or mutated
//! programs, and every problem found is reported instead of panicking.

use crate::source::Span;
use crate::typecheck::TypeId;

use super::error::MirError;
use super::{
    BlockId, LocalId, MirBlock, MirFn, MirItemKind, MirOperand, MirOperandKind, MirPlaceStepKind,
    MirProgram, MirRvalue, MirRvalueKind, MirStatic, MirStmt, MirStmtKind, MirTarget,
    MirTargetKind, MirTerminator,
};

/// Validates `program`, returning every structural problem found
/// (`E-M07`…`E-M11`) or `Ok(())`.
///
/// Errors are produced in a deterministic order: item by item, and within
/// an item by block and statement.
pub(crate) fn validate(program: &MirProgram) -> Result<(), Vec<MirError>> {
    let mut errors = Vec::new();
    let type_count = program.types.len();
    for item in &program.items {
        match &item.kind {
            MirItemKind::Fn(function) => validate_fn(function, &mut errors, type_count),
            MirItemKind::Let(stat) | MirItemKind::Const(stat) => {
                validate_static(stat, &mut errors, type_count)
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_static(stat: &MirStatic, errors: &mut Vec<MirError>, type_count: usize) {
    check_type(stat.ty, stat.span, errors, type_count);
    check_type(stat.name.ty, stat.name.span, errors, type_count);
    let local_count = stat.locals.len();
    for local in &stat.locals {
        check_type(local.ty, local.span, errors, type_count);
    }
    for stmt in &stat.stmts {
        check_stmt(stmt, local_count, errors, type_count);
    }
    check_operand(&stat.value, local_count, errors, type_count);
}

fn validate_fn(function: &MirFn, errors: &mut Vec<MirError>, type_count: usize) {
    let local_count = function.locals.len();
    let block_count = function.blocks.len();
    check_type(function.ty, function.span, errors, type_count);
    check_type(function.name.ty, function.name.span, errors, type_count);
    // Parameters must be the first locals, in order.
    for (index, param) in function.params.iter().enumerate() {
        if param.raw() as usize >= local_count {
            errors.push(MirError::invalid_local_reference(
                function.name.span,
                format!(
                    "parameter local {:?} is out of bounds (function has {local_count} locals)",
                    param
                ),
            ));
        } else if param.raw() as usize != index {
            errors.push(MirError::param_local_mismatch(
                function.name.span,
                format!("parameter local {:?} is not local {index}", param),
            ));
        }
    }
    for local in &function.locals {
        check_type(local.ty, local.span, errors, type_count);
    }
    for (index, block) in function.blocks.iter().enumerate() {
        check_block(block, index, local_count, block_count, errors, type_count);
    }
}

fn check_block(
    block: &MirBlock,
    index: usize,
    local_count: usize,
    block_count: usize,
    errors: &mut Vec<MirError>,
    type_count: usize,
) {
    if block.id.raw() as usize != index {
        errors.push(MirError::block_id_mismatch(
            block.span,
            format!("block at index {index} has id {:?}", block.id),
        ));
    }
    for stmt in &block.stmts {
        check_stmt(stmt, local_count, errors, type_count);
    }
    check_terminator(
        &block.terminator,
        local_count,
        block_count,
        errors,
        type_count,
    );
}

fn check_stmt(stmt: &MirStmt, local_count: usize, errors: &mut Vec<MirError>, type_count: usize) {
    match &stmt.kind {
        MirStmtKind::Assign { target, rvalue } => {
            check_target(target, local_count, errors, type_count);
            check_rvalue(rvalue, local_count, errors, type_count);
        }
    }
}

fn check_target(
    target: &MirTarget,
    local_count: usize,
    errors: &mut Vec<MirError>,
    type_count: usize,
) {
    check_type(target.ty, target.span, errors, type_count);
    match &target.kind {
        MirTargetKind::Local(id) => check_local(id, target.span, local_count, errors),
        MirTargetKind::Static(_) => {}
        MirTargetKind::Member { base, .. } => check_operand(base, local_count, errors, type_count),
        MirTargetKind::Index { base, index } => {
            check_operand(base, local_count, errors, type_count);
            check_operand(index, local_count, errors, type_count);
        }
        MirTargetKind::Place { root, steps } => {
            check_local(root, target.span, local_count, errors);
            for step in steps {
                if let MirPlaceStepKind::Index(index) = &step.kind {
                    check_operand(index, local_count, errors, type_count);
                }
            }
        }
        MirTargetKind::Deref { operand } => check_operand(operand, local_count, errors, type_count),
    }
}

fn check_rvalue(
    rvalue: &MirRvalue,
    local_count: usize,
    errors: &mut Vec<MirError>,
    type_count: usize,
) {
    check_type(rvalue.ty, rvalue.span, errors, type_count);
    match &rvalue.kind {
        MirRvalueKind::Use(operand) => check_operand(operand, local_count, errors, type_count),
        MirRvalueKind::Unary { operand, .. } => {
            check_operand(operand, local_count, errors, type_count)
        }
        MirRvalueKind::Binary { lhs, rhs, .. } => {
            check_operand(lhs, local_count, errors, type_count);
            check_operand(rhs, local_count, errors, type_count);
        }
        MirRvalueKind::Call { callee, args } => {
            check_operand(callee, local_count, errors, type_count);
            for arg in args {
                check_operand(arg, local_count, errors, type_count);
            }
        }
        MirRvalueKind::Range { start, end, .. } => {
            check_operand(start, local_count, errors, type_count);
            check_operand(end, local_count, errors, type_count);
        }
        MirRvalueKind::RangeNext { range } | MirRvalueKind::RangeFinished { range } => {
            check_operand(range, local_count, errors, type_count)
        }
        MirRvalueKind::Member { base, .. } => check_operand(base, local_count, errors, type_count),
        MirRvalueKind::Index { base, index } => {
            check_operand(base, local_count, errors, type_count);
            check_operand(index, local_count, errors, type_count);
        }
        MirRvalueKind::RefAddr { root, steps, .. } => {
            check_local(root, rvalue.span, local_count, errors);
            for step in steps {
                if let MirPlaceStepKind::Index(index) = &step.kind {
                    check_operand(index, local_count, errors, type_count);
                }
            }
        }
        MirRvalueKind::Deref { operand } => check_operand(operand, local_count, errors, type_count),
        MirRvalueKind::StructLit { fields } => {
            for (_, value) in fields {
                check_operand(value, local_count, errors, type_count);
            }
        }
        MirRvalueKind::ArrayLit { elems } => {
            for elem in elems {
                check_operand(elem, local_count, errors, type_count);
            }
        }
    }
}

fn check_operand(
    operand: &MirOperand,
    local_count: usize,
    errors: &mut Vec<MirError>,
    type_count: usize,
) {
    check_type(operand.ty, operand.span, errors, type_count);
    match &operand.kind {
        MirOperandKind::Local(id) => check_local(id, operand.span, local_count, errors),
        MirOperandKind::Constant(constant) => {
            check_type(constant.ty, constant.span, errors, type_count)
        }
        MirOperandKind::Static(_) => {
            // Module-item references carry no symbol table in MIR; the
            // symbol's existence is guaranteed by the front end.
        }
    }
}

fn check_terminator(
    terminator: &MirTerminator,
    local_count: usize,
    block_count: usize,
    errors: &mut Vec<MirError>,
    type_count: usize,
) {
    match terminator {
        MirTerminator::Return { value, .. } => {
            if let Some(value) = value {
                check_operand(value, local_count, errors, type_count);
            }
        }
        MirTerminator::Jump { target, span } => check_block_ref(target, *span, block_count, errors),
        MirTerminator::Branch {
            cond,
            then_block,
            else_block,
            span,
        } => {
            check_operand(cond, local_count, errors, type_count);
            check_block_ref(then_block, *span, block_count, errors);
            check_block_ref(else_block, *span, block_count, errors);
        }
    }
}

fn check_local(id: &LocalId, span: Span, local_count: usize, errors: &mut Vec<MirError>) {
    if id.raw() as usize >= local_count {
        errors.push(MirError::invalid_local_reference(
            span,
            format!(
                "local {:?} is out of bounds (function has {local_count} locals)",
                id
            ),
        ));
    }
}

fn check_block_ref(id: &BlockId, span: Span, block_count: usize, errors: &mut Vec<MirError>) {
    if id.raw() as usize >= block_count {
        errors.push(MirError::invalid_block_reference(
            span,
            format!(
                "block {:?} is out of bounds (function has {block_count} blocks)",
                id
            ),
        ));
    }
}

fn check_type(id: TypeId, span: Span, errors: &mut Vec<MirError>, type_count: usize) {
    if id.raw() as usize >= type_count {
        errors.push(MirError::invalid_type_reference(
            span,
            format!(
                "type {:?} is out of bounds (table has {type_count} types)",
                id
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for validation on hand-built programs. Validation runs on
    //! lowered programs in `tests/mir.rs`; these tests fabricate malformed
    //! programs directly to exercise every structural error class, using
    //! the crate-internal type-table constructors.

    use crate::mir::{
        BlockId, LocalId, MirBlock, MirConstant, MirConstantKind, MirFn, MirIdent, MirItem,
        MirItemKind, MirOperand, MirOperandKind, MirProgram, MirRvalue, MirRvalueKind, MirStatic,
        MirStmt, MirStmtKind, MirTarget, MirTargetKind, MirTerminator,
    };
    use crate::source::{SourceId, Span};
    use crate::typecheck::{TypeId, TypeKind, TypeTable};

    use super::validate;
    use crate::mir::error::MirErrorKind;

    fn span_at(offset: u32) -> Span {
        Span::new(SourceId::new(0), offset..offset)
    }

    /// A function with one empty entry block and a bare return, built from
    /// real type ids.
    fn trivial_fn(int_ty: TypeId) -> MirFn {
        let entry = BlockId::new(0);
        let first = LocalId::new(0);
        let ident = MirIdent {
            name: "f".to_string(),
            span: span_at(0),
            symbol: crate::semantics::SymbolId::new(0),
            ty: int_ty,
        };
        MirFn {
            name: ident,
            params: vec![first],
            locals: vec![
                crate::mir::MirLocal {
                    name: "p".to_string(),
                    symbol: None,
                    ty: int_ty,
                    mutable: false,
                    span: span_at(1),
                },
                crate::mir::MirLocal {
                    name: "q".to_string(),
                    symbol: None,
                    ty: int_ty,
                    mutable: false,
                    span: span_at(1),
                },
            ],
            blocks: vec![MirBlock {
                id: entry,
                stmts: Vec::new(),
                terminator: MirTerminator::Return {
                    value: None,
                    span: span_at(2),
                },
                span: span_at(2),
            }],
            span: span_at(3),
            ty: int_ty,
        }
    }

    fn program_with(function: MirFn, table: TypeTable) -> MirProgram {
        MirProgram {
            items: vec![MirItem {
                kind: MirItemKind::Fn(function),
                span: span_at(3),
            }],
            types: table,
            intrinsic_symbols: Vec::new(),
        }
    }

    #[test]
    fn well_formed_program_validates() {
        let mut table = TypeTable::new();
        let int_ty = table.push(TypeKind::Int);
        let program = program_with(trivial_fn(int_ty), table);
        assert!(validate(&program).is_ok());
    }

    #[test]
    fn dangling_block_reference_is_reported() {
        let mut table = TypeTable::new();
        let int_ty = table.push(TypeKind::Int);
        let mut function = trivial_fn(int_ty);
        function.blocks[0].terminator = MirTerminator::Jump {
            target: BlockId::new(9),
            span: span_at(2),
        };
        let program = program_with(function, table);
        let errors = validate(&program).unwrap_err();
        assert_eq!(errors.len(), 1, "errors: {errors:?}");
        assert_eq!(errors[0].kind(), MirErrorKind::InvalidBlockReference);
        assert_eq!(errors[0].code(), "E-M07");
        assert!(errors[0].detail().unwrap().contains("9"));
    }

    #[test]
    fn dangling_local_reference_is_reported() {
        let mut table = TypeTable::new();
        let int_ty = table.push(TypeKind::Int);
        let mut function = trivial_fn(int_ty);
        function.blocks[0].stmts.push(MirStmt {
            kind: MirStmtKind::Assign {
                target: MirTarget {
                    kind: MirTargetKind::Local(LocalId::new(7)),
                    span: span_at(4),
                    ty: int_ty,
                },
                rvalue: MirRvalue {
                    kind: MirRvalueKind::Use(MirOperand {
                        kind: MirOperandKind::Local(LocalId::new(0)),
                        span: span_at(4),
                        ty: int_ty,
                    }),
                    span: span_at(4),
                    ty: int_ty,
                },
            },
            span: span_at(4),
        });
        let program = program_with(function, table);
        let errors = validate(&program).unwrap_err();
        assert_eq!(errors.len(), 1, "errors: {errors:?}");
        assert_eq!(errors[0].kind(), MirErrorKind::InvalidLocalReference);
        assert_eq!(errors[0].code(), "E-M08");
    }

    #[test]
    fn unknown_type_reference_is_reported() {
        let mut table = TypeTable::new();
        let int_ty = table.push(TypeKind::Int);
        // A type id beyond the table.
        let bogus = TypeId::new(table.len() as u32 + 5);
        let mut function = trivial_fn(int_ty);
        // The operand's own type stays valid; only the constant's type is
        // bogus, so exactly one E-M09 is reported.
        function.blocks[0].terminator = MirTerminator::Return {
            value: Some(MirOperand {
                kind: MirOperandKind::Constant(MirConstant {
                    kind: MirConstantKind::Int,
                    span: span_at(5),
                    ty: bogus,
                }),
                span: span_at(5),
                ty: int_ty,
            }),
            span: span_at(2),
        };
        let program = program_with(function, table);
        let errors = validate(&program).unwrap_err();
        assert_eq!(errors.len(), 1, "errors: {errors:?}");
        assert_eq!(errors[0].kind(), MirErrorKind::InvalidTypeReference);
        assert_eq!(errors[0].code(), "E-M09");
    }

    #[test]
    fn unordered_blocks_are_reported() {
        let mut table = TypeTable::new();
        let int_ty = table.push(TypeKind::Int);
        let mut function = trivial_fn(int_ty);
        // Swap the ids: the block at index 0 claims a different id.
        function.blocks[0].id = BlockId::new(1);
        let program = program_with(function, table);
        let errors = validate(&program).unwrap_err();
        assert_eq!(errors.len(), 1, "errors: {errors:?}");
        assert_eq!(errors[0].kind(), MirErrorKind::BlockIdMismatch);
        assert_eq!(errors[0].code(), "E-M10");
    }

    #[test]
    fn misplaced_parameter_local_is_reported() {
        let mut table = TypeTable::new();
        let int_ty = table.push(TypeKind::Int);
        let mut function = trivial_fn(int_ty);
        // Claim a parameter that is in bounds but not the first local.
        function.params = vec![LocalId::new(1)];
        let program = program_with(function, table);
        let errors = validate(&program).unwrap_err();
        assert_eq!(errors.len(), 1, "errors: {errors:?}");
        assert_eq!(errors[0].kind(), MirErrorKind::ParamLocalMismatch);
        assert_eq!(errors[0].code(), "E-M11");
    }

    #[test]
    fn static_items_validate() {
        let mut table = TypeTable::new();
        let int_ty = table.push(TypeKind::Int);
        let stat = MirStatic {
            name: MirIdent {
                name: "a".to_string(),
                span: span_at(6),
                symbol: crate::semantics::SymbolId::new(1),
                ty: int_ty,
            },
            mutable: false,
            locals: Vec::new(),
            stmts: Vec::new(),
            value: MirOperand {
                kind: MirOperandKind::Constant(MirConstant {
                    kind: MirConstantKind::Int,
                    span: span_at(7),
                    ty: int_ty,
                }),
                span: span_at(7),
                ty: int_ty,
            },
            span: span_at(8),
            ty: int_ty,
        };
        let program = MirProgram {
            items: vec![MirItem {
                kind: MirItemKind::Let(stat),
                span: span_at(8),
            }],
            types: table,
            intrinsic_symbols: Vec::new(),
        };
        assert!(validate(&program).is_ok());
    }
}
