//! x86-64 code generation — the first native target.
//!
//! Turns verified backend instructions ([`BProgram`](super::super::ir::BProgram))
//! into x86-64 machine code and wraps it in a Windows PE image
//! ([`pe`](super::pe)). The emitter is a small, self-contained code
//! generator: it encodes exactly the instructions the backend IR needs and
//! assembles the image itself — no external assembler or linker, and no
//! unsafe code.
//!
//! ## Calling convention
//!
//! Because MINK controls both sides of every call, the emitter defines its
//! own simple convention instead of the platform ABI:
//!
//! - arguments are pushed on the stack (rightmost first), each argument
//!   occupying one word (8 bytes) or, for `Range` values, two;
//! - the callee reads argument `i` at `[rbp + 16 + 8 * words_before_i]`;
//! - the result is returned in `rax` (the low 32 bits form the process
//!   exit code when the callee is the entry function);
//! - the stack stays 16-byte aligned at every `call` (padding is inserted
//!   when an odd number of argument words is pushed);
//! - callee-saved registers are exactly `rbp`/`rsp`; everything else is
//!   scratch.
//!
//! ## Value model
//!
//! - `Int` and `Bool` are 64-bit words (`Bool` is `0`/`1`);
//! - `Range<Int>` is two words: the normalized exclusive end and the
//!   iteration cursor. Iteration is the defined backend semantics: a range
//!   yields its cursor and advances it; exhaustion is `cursor >= end` for
//!   exclusive ranges, `cursor >= end + 1` for inclusive ones (the end is
//!   normalized at construction, wrapping on overflow);
//! - unit functions return `0`.
//!
//! ## Addressing
//!
//! All references are relative: module bindings are reached RIP-relative
//! and control transfers use `rel32`, so the image needs no base-relocation
//! fixups.

use crate::ast::{BinaryOp, UnaryOp};
use crate::mir::BlockId;

use super::super::ir::{BInstKind, BOperand, BProgram, BTerminator, BType};
use super::EmittedImage;
use super::pe;

/// A per-local pair of slot offsets from `rbp` (negative). Word 0 is the
/// value's first word (for `Range`: the normalized exclusive end); word 1
/// is its second (the iteration cursor); both are `0` for single-word
/// types.
type Slots = Vec<(i32, i32)>;

/// Computes each local's stack-slot offsets from `rbp`.
fn slots(f: &super::super::ir::BFunction) -> Slots {
    let mut result = Vec::with_capacity(f.locals.len());
    let mut words = 0usize;
    for local in &f.locals {
        let width = local.ty.words();
        let word0 = -(8 * (words + 1) as i32);
        let word1 = if width == 2 {
            -(8 * (words + 2) as i32)
        } else {
            0
        };
        result.push((word0, word1));
        words += width;
    }
    result
}

/// The frame size in bytes, rounded up to 16 (stack alignment).
fn frame_size(slots: &Slots) -> i32 {
    let bytes = if slots.is_empty() {
        0
    } else {
        8 * (slots.len() + slots.iter().filter(|(_, second)| *second != 0).count())
    };
    // Round up to a multiple of 16 so the stack stays aligned after the
    // `push rbp` prologue.
    (bytes.div_ceil(16) * 16) as i32
}

/// A patch: a `disp32` field that must be filled once layout is known.
struct Patch {
    /// Byte offset of the `disp32` field within the code section.
    offset: usize,
    /// What the field must point at.
    kind: PatchKind,
}

enum PatchKind {
    /// A block within a function (relative within `.text`). Resolved at
    /// patch time, when every block's offset is known.
    Block {
        /// The function's index in the program.
        function: usize,
        /// The target block's id.
        block: u32,
    },
    /// A function (relative within `.text`).
    Function(usize),
    /// A module binding (RIP-relative from the patch site).
    Static(usize),
}

/// The code emitter: a byte buffer plus the patches to resolve after
/// layout.
struct Code {
    buf: Vec<u8>,
    patches: Vec<Patch>,
}

impl Code {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            patches: Vec::new(),
        }
    }

    fn u8(&mut self, byte: u8) {
        self.buf.push(byte);
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    fn i32_le(&mut self, value: i32) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    /// Records a patch at the current position and reserves the `disp32`.
    fn patch(&mut self, kind: PatchKind) {
        self.patches.push(Patch {
            offset: self.buf.len(),
            kind,
        });
        self.buf.extend_from_slice(&0u32.to_le_bytes());
    }

    // ------------------------------------------------------------------
    // Encodings
    // ------------------------------------------------------------------

    /// `mov rax, imm64`.
    fn movabs_rax(&mut self, value: i64) {
        self.bytes(&[0x48, 0xB8]);
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    /// `mov rcx, imm64`.
    fn movabs_rcx(&mut self, value: i64) {
        self.bytes(&[0x48, 0xB9]);
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    /// `mov rax, [rbp + disp]` (disp8 when it fits, else disp32).
    fn mov_rax_rbp(&mut self, disp: i32) {
        self.bytes(&[0x48, 0x8B]);
        self.rbp_disp(disp, 0);
    }

    /// `mov [rbp + disp], rax`.
    fn mov_rbp_rax(&mut self, disp: i32) {
        self.bytes(&[0x48, 0x89]);
        self.rbp_disp(disp, 0);
    }

    /// `mov rcx, [rbp + disp]`.
    fn mov_rcx_rbp(&mut self, disp: i32) {
        self.bytes(&[0x48, 0x8B]);
        self.rbp_disp(disp, 1);
    }

    /// `mov rax, [rip + disp32]` (patched later).
    fn mov_rax_rip(&mut self, static_index: usize) {
        self.bytes(&[0x48, 0x8B, 0x05]);
        self.patch(PatchKind::Static(static_index));
    }

    /// `mov [rip + disp32], rax` (patched later).
    fn mov_rip_rax(&mut self, static_index: usize) {
        self.bytes(&[0x48, 0x89, 0x05]);
        self.patch(PatchKind::Static(static_index));
    }

    /// The ModRM displacement for `[rbp + disp]`: disp8 when it fits,
    /// disp32 otherwise.
    fn rbp_disp(&mut self, disp: i32, reg: u8) {
        if (-128..=127).contains(&disp) {
            self.u8(0x40 | (reg << 3) | 0x05);
            self.u8(disp as u8);
        } else {
            self.u8(0x80 | (reg << 3) | 0x05);
            self.i32_le(disp);
        }
    }

    /// `push rax`.
    fn push_rax(&mut self) {
        self.u8(0x50);
    }

    /// `xor eax, eax`.
    fn xor_eax(&mut self) {
        self.bytes(&[0x31, 0xC0]);
    }

    /// `test rax, rax`.
    fn test_rax(&mut self) {
        self.bytes(&[0x48, 0x85, 0xC0]);
    }

    /// `cmp rax, rcx`.
    fn cmp_rax_rcx(&mut self) {
        self.bytes(&[0x48, 0x39, 0xC8]);
    }

    /// `cmp rax, [rbp + disp]`.
    fn cmp_rax_rbp(&mut self, disp: i32) {
        self.bytes(&[0x48, 0x3B]);
        self.rbp_disp(disp, 0);
    }

    /// `op rax, rcx` for a binary ALU operation.
    fn alu_rax_rcx(&mut self, opcode: u8) {
        self.bytes(&[0x48, opcode, 0xC8]);
    }

    /// `imul rax, rcx`.
    fn imul_rax_rcx(&mut self) {
        self.bytes(&[0x48, 0x0F, 0xAF, 0xC1]);
    }

    /// `cqo` (sign-extend rax into rdx:rax).
    fn cqo(&mut self) {
        self.bytes(&[0x48, 0x99]);
    }

    /// `idiv rcx`.
    fn idiv_rcx(&mut self) {
        self.bytes(&[0x48, 0xF7, 0xF9]);
    }

    /// `mov rax, rdx`.
    fn mov_rax_rdx(&mut self) {
        self.bytes(&[0x48, 0x89, 0xD0]);
    }

    /// `shl/shr/sar rax, cl`.
    fn shift_rax_cl(&mut self, opcode: u8) {
        self.bytes(&[0x48, 0xD3, opcode]);
    }

    /// `neg rax`.
    fn neg_rax(&mut self) {
        self.bytes(&[0x48, 0xF7, 0xD8]);
    }

    /// `not rax`.
    fn not_rax(&mut self) {
        self.bytes(&[0x48, 0xF7, 0xD0]);
    }

    /// `setcc al` then `movzx eax, al`.
    fn setcc_rax(&mut self, condition: u8) {
        self.bytes(&[0x0F, condition, 0xC0]);
        self.bytes(&[0x0F, 0xB6, 0xC0]);
    }

    /// `add rax, 1`.
    fn add_rax_one(&mut self) {
        self.bytes(&[0x48, 0x83, 0xC0, 0x01]);
    }

    /// `add qword [rbp + disp], 1`.
    fn add_rbp_one(&mut self, disp: i32) {
        self.bytes(&[0x48, 0x83]);
        self.rbp_disp(disp, 0);
        self.u8(0x01);
    }

    /// `sub rsp, imm` (imm8 when it fits, else imm32).
    fn sub_rsp(&mut self, imm: i32) {
        self.bytes(&[0x48, 0x81, 0xEC]);
        self.i32_le(imm);
    }

    /// `add rsp, imm` (imm8 when it fits, else imm32).
    fn add_rsp(&mut self, imm: i32) {
        self.bytes(&[0x48, 0x81, 0xC4]);
        self.i32_le(imm);
    }

    /// `call rel32` (patched later).
    fn call(&mut self, function_index: usize) {
        self.u8(0xE8);
        self.patch(PatchKind::Function(function_index));
    }

    /// `jmp rel32` (patched later).
    fn jmp(&mut self, kind: PatchKind) {
        self.u8(0xE9);
        self.patch(kind);
    }

    /// `jcc rel32` (patched later).
    fn jcc(&mut self, opcode: u8, kind: PatchKind) {
        self.bytes(&[0x0F, opcode]);
        self.patch(kind);
    }

    /// `leave; ret`.
    fn leave_ret(&mut self) {
        self.bytes(&[0xC9, 0xC3]);
    }
}

/// Emits the x86-64 machine code for `program` and wraps it in a PE image.
///
/// `entry` is the index of the `main` function (validated by the caller);
/// the image's entry point calls it and returns its result as the exit
/// code.
pub(crate) fn emit_pe(program: &BProgram, entry: usize) -> EmittedImage {
    let mut code = Code::new();

    // ------------------------------------------------------------------
    // Entry-point stub: align the stack, call `main`, restore, return.
    // `ret` from the entry point terminates the process with eax as the
    // exit code.
    // ------------------------------------------------------------------
    let entry_offset = 0usize;
    code.sub_rsp(8);
    code.call(entry);
    code.add_rsp(8);
    code.u8(0xC3); // ret

    // ------------------------------------------------------------------
    // Functions, in source order.
    // ------------------------------------------------------------------
    let mut function_starts = Vec::with_capacity(program.functions.len());
    let mut function_block_starts = Vec::with_capacity(program.functions.len());
    for (index, f) in program.functions.iter().enumerate() {
        let (start, block_starts) = emit_function(&mut code, f, index);
        function_starts.push(start);
        function_block_starts.push(block_starts);
    }

    // ------------------------------------------------------------------
    // Module bindings: one 8-byte word each, in source order.
    // ------------------------------------------------------------------
    let mut data = Vec::with_capacity(program.statics.len() * 8);
    for s in &program.statics {
        data.extend_from_slice(&s.value.to_le_bytes());
    }

    // ------------------------------------------------------------------
    // Layout and patch resolution.
    // ------------------------------------------------------------------
    let text_size = code.buf.len() as u32;
    let text_rva = pe::TEXT_RVA;
    let data_rva = (text_rva + text_size).div_ceil(0x1000) * 0x1000;
    let reloc = pe::relocation_block(text_rva);
    for patch in &code.patches {
        let disp = match &patch.kind {
            PatchKind::Block { function, block } => {
                let target = function_starts[*function] as i64
                    + function_block_starts[*function][*block as usize] as i64;
                target - (patch.offset as i64 + 4)
            }
            PatchKind::Function(index) => {
                function_starts[*index] as i64 - (patch.offset as i64 + 4)
            }
            PatchKind::Static(index) => {
                // RIP-relative: target VA minus the address after the
                // instruction. The image base cancels out.
                (data_rva as i64 + 8 * *index as i64) - (text_rva as i64 + patch.offset as i64 + 4)
            }
        };
        code.buf[patch.offset..patch.offset + 4].copy_from_slice(&(disp as i32).to_le_bytes());
    }

    let bytes = pe::build(&code.buf, &data, &reloc, entry_offset as u32);
    EmittedImage {
        bytes,
        functions: program.functions.len(),
        statics: program.statics.len(),
        entry: "main".to_string(),
    }
}

/// Emits one function; returns its absolute start offset in `.text` and
/// every block's relative start offset within it.
fn emit_function(
    code: &mut Code,
    f: &super::super::ir::BFunction,
    function_index: usize,
) -> (usize, Vec<u32>) {
    let start = code.buf.len();
    let slots = slots(f);
    let frame = frame_size(&slots);

    // Prologue.
    code.u8(0x55); // push rbp
    code.bytes(&[0x48, 0x89, 0xE5]); // mov rbp, rsp
    if frame > 0 {
        code.sub_rsp(frame);
    }

    // Copy parameters into their slots.
    let mut arg_words = 0usize;
    for &param in &f.params {
        let ty = f.local(param).map(|local| local.ty).unwrap_or(BType::Int);
        let arg_base = 16 + 8 * arg_words;
        let (word0, word1) = slots[param.raw() as usize];
        code.mov_rax_rbp(arg_base as i32);
        code.mov_rbp_rax(word0);
        if ty == BType::Range {
            code.mov_rax_rbp(arg_base as i32 + 8);
            code.mov_rbp_rax(word1);
        }
        arg_words += ty.words();
    }

    // Blocks, in order. Starts are filled in as blocks are emitted; forward
    // jumps patch by block id after layout, so the full table is allocated
    // up front.
    let mut block_starts = vec![0u32; f.blocks.len()];
    for block in &f.blocks {
        block_starts[block.id.raw() as usize] = (code.buf.len() - start) as u32;
        emit_block(code, f, &slots, block, function_index);
    }
    (start, block_starts)
}

/// The patch kind for a jump/branch target within the current function.
fn block_target(function_index: usize, target: BlockId) -> PatchKind {
    PatchKind::Block {
        function: function_index,
        block: target.raw(),
    }
}

fn emit_block(
    code: &mut Code,
    f: &super::super::ir::BFunction,
    slots: &Slots,
    block: &super::super::ir::BBlock,
    function_index: usize,
) {
    for inst in &block.insts {
        emit_inst(code, f, slots, inst);
    }
    match &block.terminator {
        BTerminator::Return { value, .. } => {
            // Unit functions (and bare returns in non-unit functions) return
            // zero; a value return loads the value into rax first.
            let has_value = value.is_some() && f.result != BType::Unit;
            if let Some(operand) = value {
                if has_value {
                    eval_rax(code, slots, *operand);
                }
            } else {
                code.xor_eax();
            }
            code.leave_ret();
        }
        BTerminator::Jump { target, .. } => {
            code.jmp(block_target(function_index, *target));
        }
        BTerminator::Branch {
            cond,
            then_block,
            else_block,
            ..
        } => {
            eval_rax(code, slots, *cond);
            code.test_rax();
            code.jcc(
                0x85, // jne
                block_target(function_index, *then_block),
            );
            code.jmp(block_target(function_index, *else_block));
        }
    }
}

/// Evaluates an operand into `rax`.
fn eval_rax(code: &mut Code, slots: &Slots, operand: BOperand) {
    match operand {
        BOperand::Const(value) => code.movabs_rax(value),
        BOperand::Local(id) => code.mov_rax_rbp(slots[id.raw() as usize].0),
    }
}

/// Evaluates an operand into `rcx`. Constants load directly into `rcx` so
/// `rax` (which may hold the other operand) is never clobbered.
fn eval_rcx(code: &mut Code, slots: &Slots, operand: BOperand) {
    match operand {
        BOperand::Const(value) => code.movabs_rcx(value),
        BOperand::Local(id) => code.mov_rcx_rbp(slots[id.raw() as usize].0),
    }
}

/// Pushes an argument onto the stack (rightmost argument pushed first, so
/// the leftmost ends on top). `Range` arguments push their second word
/// first, so the callee reads word 0 at `[rbp + 16]`. Returns the number
/// of words pushed.
fn push_operand(
    code: &mut Code,
    f: &super::super::ir::BFunction,
    slots: &Slots,
    operand: BOperand,
) -> usize {
    match operand {
        BOperand::Const(value) => {
            code.movabs_rax(value);
            code.push_rax();
            1
        }
        BOperand::Local(id) => {
            let ty = f.local(id).map(|local| local.ty).unwrap_or(BType::Int);
            let (word0, word1) = slots[id.raw() as usize];
            if ty == BType::Range {
                code.mov_rax_rbp(word1);
                code.push_rax();
                code.mov_rax_rbp(word0);
                code.push_rax();
                2
            } else {
                code.mov_rax_rbp(word0);
                code.push_rax();
                1
            }
        }
    }
}

fn emit_inst(
    code: &mut Code,
    f: &super::super::ir::BFunction,
    slots: &Slots,
    inst: &super::super::ir::BInst,
) {
    match &inst.kind {
        BInstKind::LoadLocal { target, src } => {
            let ty = f.local(*src).map(|local| local.ty).unwrap_or(BType::Int);
            let (dst0, dst1) = slots[target.raw() as usize];
            let (src0, src1) = slots[src.raw() as usize];
            code.mov_rax_rbp(src0);
            code.mov_rbp_rax(dst0);
            if ty == BType::Range {
                code.mov_rax_rbp(src1);
                code.mov_rbp_rax(dst1);
            }
        }
        BInstKind::LoadConst { target, value } => {
            code.movabs_rax(*value);
            code.mov_rbp_rax(slots[target.raw() as usize].0);
        }
        BInstKind::LoadStatic {
            target,
            static_index,
        } => {
            code.mov_rax_rip(*static_index);
            code.mov_rbp_rax(slots[target.raw() as usize].0);
        }
        BInstKind::StoreStatic { static_index, src } => {
            eval_rax(code, slots, *src);
            code.mov_rip_rax(*static_index);
        }
        BInstKind::Unary { target, op, src } => {
            eval_rax(code, slots, *src);
            match op {
                UnaryOp::Neg => code.neg_rax(),
                UnaryOp::BitNot => code.not_rax(),
                UnaryOp::Not => {
                    code.test_rax();
                    code.setcc_rax(0x94); // sete: !x is true when x == 0
                }
            }
            code.mov_rbp_rax(slots[target.raw() as usize].0);
        }
        BInstKind::Binary {
            target,
            op,
            lhs,
            rhs,
        } => {
            eval_rax(code, slots, *lhs);
            eval_rcx(code, slots, *rhs);
            use BinaryOp::*;
            match op {
                Add => code.alu_rax_rcx(0x01),
                Sub => code.alu_rax_rcx(0x29),
                Mul => code.imul_rax_rcx(),
                Div => {
                    code.cqo();
                    code.idiv_rcx();
                }
                Rem => {
                    code.cqo();
                    code.idiv_rcx();
                    code.mov_rax_rdx();
                }
                Shl => code.shift_rax_cl(0xE0),
                // The language's `>>` is defined as an arithmetic shift
                // (signed semantics), matching the 64-bit integer model.
                Shr => code.shift_rax_cl(0xF8),
                Lt => {
                    code.cmp_rax_rcx();
                    code.setcc_rax(0x9C); // setl
                }
                Le => {
                    code.cmp_rax_rcx();
                    code.setcc_rax(0x9E); // setle
                }
                Gt => {
                    code.cmp_rax_rcx();
                    code.setcc_rax(0x9F); // setg
                }
                Ge => {
                    code.cmp_rax_rcx();
                    code.setcc_rax(0x9D); // setge
                }
                Eq => {
                    code.cmp_rax_rcx();
                    code.setcc_rax(0x94); // sete
                }
                Ne => {
                    code.cmp_rax_rcx();
                    code.setcc_rax(0x95); // setne
                }
                BitAnd => code.alu_rax_rcx(0x21),
                BitXor => code.alu_rax_rcx(0x31),
                BitOr => code.alu_rax_rcx(0x09),
                And => code.alu_rax_rcx(0x21),
                Or => code.alu_rax_rcx(0x09),
            }
            code.mov_rbp_rax(slots[target.raw() as usize].0);
        }
        BInstKind::Call {
            target,
            callee,
            args,
        } => {
            let mut words = 0usize;
            for arg in args.iter().rev() {
                words += push_operand(code, f, slots, *arg);
            }
            let pad = words % 2;
            if pad == 1 {
                code.sub_rsp(8);
            }
            code.call(*callee);
            if words + pad > 0 {
                code.add_rsp((8 * (words + pad)) as i32);
            }
            code.mov_rbp_rax(slots[target.raw() as usize].0);
        }
        BInstKind::RangeInit {
            target,
            start,
            end,
            inclusive,
        } => {
            let (word0, word1) = slots[target.raw() as usize];
            // Normalized exclusive end (end + 1 for inclusive ranges).
            eval_rax(code, slots, *end);
            if *inclusive {
                code.add_rax_one();
            }
            code.mov_rbp_rax(word0);
            // Iteration cursor starts at the range start.
            eval_rax(code, slots, *start);
            code.mov_rbp_rax(word1);
        }
        BInstKind::RangeNext { target, range } => {
            let (_, range_cursor) = slots[range.raw() as usize];
            // rax = cursor; cursor += 1; target = rax.
            code.mov_rax_rbp(range_cursor);
            code.add_rbp_one(range_cursor);
            code.mov_rbp_rax(slots[target.raw() as usize].0);
        }
        BInstKind::RangeFinished { target, range } => {
            let (range_end, range_cursor) = slots[range.raw() as usize];
            // finished = cursor >= normalized end.
            code.mov_rax_rbp(range_cursor);
            code.cmp_rax_rbp(range_end);
            code.setcc_rax(0x9D); // setge
            code.mov_rbp_rax(slots[target.raw() as usize].0);
        }
    }
}
