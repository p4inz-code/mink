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
//! The embedded MINK runtime services (see [`runtime`](super::runtime))
//! use this same convention, so intrinsic calls lower to ordinary stack
//! calls.
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
//! ## Image layout
//!
//! `.text` starts with the entry-point stub, then the user functions in
//! source order, then the embedded runtime services and their message
//! data. The loader maps `.data` (module bindings), `.bss` (the runtime
//! state: heap arena and liveness table), `.idata` (the `kernel32`
//! imports used by the runtime's output/error paths), and `.reloc`.
//!
//! ## Addressing
//!
//! All references are relative: module bindings, runtime state, and the
//! import table are reached RIP-relative and control transfers use
//! `rel32`, so the image needs no base-relocation fixups.

use crate::ast::{BinaryOp, UnaryOp};
use crate::mir::BlockId;

use super::super::ir::{
    BInstKind, BOperand, BProgram, BTerminator, BType, PlaceAddrStep, RuntimeService,
};
use super::EmittedImage;
use super::pe;
use super::runtime;

/// A general-purpose register for the runtime emitter. Values follow the
/// x86-64 encoding (0–7 are `rax`…`rdi`, 8–15 are `r8`–`r15`), so the REX
/// and ModRM bits are derived arithmetically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reg {
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rsp = 4,
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

/// A per-local pair of slot offsets from `rbp` (negative). Word 0 is the
/// value's first word (for `Range`: the normalized exclusive end; for an
/// aggregate: byte 0 of the value); word 1 is the second word for the
/// two-word types (`Range`, a 16-byte struct), and `0` otherwise.
/// Aggregate values occupy `local.words` consecutive words *below* word 0
/// in chunk order: byte `b` of a value lives at `word0 - 8*(b/8) +
/// (b%8)`, so each 8-byte chunk sits at `[word0 - 8k, word0 - 8k + 7]`
/// in normal byte order and sub-word pieces (booleans) stay inside their
/// own chunk instead of colliding with the qword below it.
type Slots = Vec<(i32, i32)>;

/// Computes each local's stack-slot offsets from `rbp`, plus the total
/// number of words the frame needs.
fn slots(f: &super::super::ir::BFunction) -> (Slots, usize) {
    let mut result = Vec::with_capacity(f.locals.len());
    let mut words = 0usize;
    for local in &f.locals {
        let width = local.words as usize;
        let word0 = -(8 * (words + 1) as i32);
        let word1 = if width == 2 {
            -(8 * (words + 2) as i32)
        } else {
            0
        };
        result.push((word0, word1));
        words += width;
    }
    (result, words)
}

/// The frame size in bytes, rounded up to 16 (stack alignment).
fn frame_size(total_words: usize) -> i32 {
    let bytes = 8 * total_words;
    // Round up to a multiple of 16 so the stack stays aligned after the
    // `push rbp` prologue.
    (bytes.div_ceil(16) * 16) as i32
}

/// A patch: a `disp32` field that must be filled once layout is known.
pub(crate) struct Patch {
    /// Byte offset of the `disp32` field within the code section.
    pub(crate) offset: usize,
    /// What the field must point at.
    pub(crate) kind: PatchKind,
}

/// The target of a `disp32` patch.
pub(crate) enum PatchKind {
    /// A block within a function (relative within `.text`). Resolved at
    /// patch time, when every block's offset is known.
    Block {
        /// The function's index in the program.
        function: usize,
        /// The target block's id.
        block: u32,
    },
    /// A user function (relative within `.text`).
    Function(usize),
    /// A module binding (RIP-relative from the patch site).
    Static(usize),
    /// An embedded runtime service (relative within `.text`).
    RuntimeService(RuntimeService),
    /// Runtime state in `.bss` (RIP-relative from the patch site), by
    /// offset within the `.bss` section.
    Bss(u32),
    /// An entry of the import address table (RIP-relative from the patch
    /// site), by index.
    Iat(u32),
    /// A runtime label: an absolute offset within `.text`, bound before
    /// patching.
    Label(u32),
}

/// The code emitter: a byte buffer plus the patches to resolve after
/// layout, and the runtime labels bound during emission. The fields are
/// `pub(crate)` so the emitter's own modules can borrow them disjointly
/// during patch resolution.
pub(crate) struct Code {
    pub(crate) buf: Vec<u8>,
    pub(crate) patches: Vec<Patch>,
    /// Runtime labels: id → absolute offset within `.text`. Bound by the
    /// runtime emitter before patching.
    pub(crate) labels: Vec<Option<usize>>,
}

impl Code {
    pub(crate) fn new() -> Self {
        Self {
            buf: Vec::new(),
            patches: Vec::new(),
            labels: Vec::new(),
        }
    }

    /// The current length of the code buffer (an absolute `.text` offset).
    pub(crate) fn len(&self) -> usize {
        self.buf.len()
    }

    pub(crate) fn u8(&mut self, byte: u8) {
        self.buf.push(byte);
    }

    pub(crate) fn bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    pub(crate) fn i32_le(&mut self, value: i32) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    /// Records a patch at the current position and reserves the `disp32`.
    pub(crate) fn patch(&mut self, kind: PatchKind) {
        self.patches.push(Patch {
            offset: self.buf.len(),
            kind,
        });
        self.buf.extend_from_slice(&0u32.to_le_bytes());
    }

    /// Creates a new runtime label, to be bound with [`Code::bind_label`].
    pub(crate) fn label(&mut self) -> u32 {
        let id = self.labels.len() as u32;
        self.labels.push(None);
        id
    }

    /// Binds `label` to the current position (an absolute `.text` offset).
    pub(crate) fn bind_label(&mut self, label: u32) {
        self.labels[label as usize] = Some(self.buf.len());
    }

    // ------------------------------------------------------------------
    // Generic register encodings (REX.W 64-bit unless noted)
    // ------------------------------------------------------------------

    /// `mov r64, r64`. Opcode `89` moves the `reg` field (the source) into
    /// the `r/m` field (the destination).
    pub(crate) fn mov_rr(&mut self, dst: Reg, src: Reg) {
        self.rex_w(src, dst);
        self.u8(0x89);
        self.modrm_rm_reg(dst, src);
    }

    /// `mov r64, [base + disp]`.
    pub(crate) fn mov_r_mem(&mut self, dst: Reg, base: Reg, disp: i32) {
        self.rex_w(dst, base);
        self.u8(0x8B);
        self.mem_modrm(dst, base, disp);
    }

    /// `mov r32, [base + disp]` (zero-extends).
    pub(crate) fn mov_r32_mem(&mut self, dst: Reg, base: Reg, disp: i32) {
        self.rex(dst, base);
        self.u8(0x8B);
        self.mem_modrm(dst, base, disp);
    }

    /// `mov [base + disp], r64`.
    pub(crate) fn mov_mem_r(&mut self, base: Reg, disp: i32, src: Reg) {
        self.rex_w(src, base);
        self.u8(0x89);
        self.mem_modrm(src, base, disp);
    }

    /// `mov byte [base + disp], r8` (the low byte of `src`).
    pub(crate) fn mov_mem_r8(&mut self, base: Reg, disp: i32, src: Reg) {
        self.rex(src, base);
        self.u8(0x88);
        self.mem_modrm(src, base, disp);
    }

    /// `movzx r32, byte [base + disp]` (loads a byte, zero-extending it
    /// into a 64-bit register).
    pub(crate) fn movzx_byte(&mut self, dst: Reg, base: Reg, disp: i32) {
        self.rex(dst, base);
        self.u8(0x0F);
        self.u8(0xB6);
        self.mem_modrm(dst, base, disp);
    }

    /// `mov qword [base + disp], imm32`.
    pub(crate) fn mov_mem_imm32(&mut self, base: Reg, disp: i32, imm: i32) {
        self.rex_w(Reg::Rax, base);
        self.u8(0xC7);
        self.mem_modrm(Reg::Rax, base, disp); // /0
        self.i32_le(imm);
    }

    /// `mov byte [base + disp], imm8`.
    pub(crate) fn mov_mem_imm8(&mut self, base: Reg, disp: i32, imm: u8) {
        self.rex(Reg::Rax, base);
        self.u8(0xC6);
        self.mem_modrm(Reg::Rax, base, disp); // /0
        self.u8(imm);
    }

    /// `mov r64, imm32` (zero-extends; writes the 32-bit register).
    /// The register is encoded in the opcode (`B8+rd`), so its extension
    /// bit is REX.B — the ModRM-style helpers use REX.R instead.
    pub(crate) fn mov_r32_imm32(&mut self, dst: Reg, imm: u32) {
        self.rex(Reg::Rax, dst);
        self.u8(0xB8 | (dst as u8 & 7));
        self.i32_le(imm as i32);
    }

    /// `movabs r64, imm64` (the register is opcode-encoded, so its
    /// extension bit is REX.B, like [`Self::mov_r32_imm32`]).
    pub(crate) fn movabs(&mut self, dst: Reg, imm: u64) {
        self.rex_w(Reg::Rax, dst);
        self.u8(0xB8 | (dst as u8 & 7));
        self.buf.extend_from_slice(&imm.to_le_bytes());
    }

    /// `lea r64, [base + disp]`.
    pub(crate) fn lea_r_mem(&mut self, dst: Reg, base: Reg, disp: i32) {
        self.rex_w(dst, base);
        self.u8(0x8D);
        self.mem_modrm(dst, base, disp);
    }

    /// `cmp a, b` (`flags = a - b`). Opcode `39` computes `r/m - reg`.
    pub(crate) fn cmp_rr(&mut self, a: Reg, b: Reg) {
        self.rex_w(b, a);
        self.u8(0x39);
        self.modrm_rm_reg(a, b);
    }

    /// `cmp r64, [base + disp]`.
    pub(crate) fn cmp_r_mem(&mut self, a: Reg, base: Reg, disp: i32) {
        self.rex_w(a, base);
        self.u8(0x3B);
        self.mem_modrm(a, base, disp);
    }

    /// `cmp qword [base + disp], imm8`.
    pub(crate) fn cmp_mem_imm8(&mut self, base: Reg, disp: i32, imm: u8) {
        self.rex_w(Reg::Rdi, base);
        self.u8(0x83);
        self.mem_modrm(Reg::Rdi, base, disp); // /7
        self.u8(imm);
    }

    /// `cmp byte [base+disp], imm8` — a single-byte comparison.
    pub(crate) fn cmp_mem8_imm8(&mut self, base: Reg, disp: i32, imm: u8) {
        self.rex(Reg::Rdi, base);
        self.u8(0x80);
        self.mem_modrm(Reg::Rdi, base, disp); // /7
        self.u8(imm);
    }

    /// `cmp r64, imm8`.
    pub(crate) fn cmp_r_imm8(&mut self, a: Reg, imm: u8) {
        self.rex_w(Reg::Rax, a);
        self.u8(0x83);
        self.u8(0xF8 | (a as u8 & 7)); // /7
        self.u8(imm);
    }

    /// `cmp r64, imm32`.
    pub(crate) fn cmp_r_imm32(&mut self, a: Reg, imm: u32) {
        self.rex_w(Reg::Rax, a);
        self.u8(0x81);
        self.u8(0xF8 | (a as u8 & 7)); // /7
        self.i32_le(imm as i32);
    }

    /// `test r64, r64`.
    pub(crate) fn test_rr(&mut self, a: Reg, b: Reg) {
        self.rex_w(a, b);
        self.u8(0x85);
        self.modrm_reg(a, b);
    }

    /// `test r64, imm32`.
    pub(crate) fn test_r_imm32(&mut self, a: Reg, imm: u32) {
        self.rex_w(Reg::Rax, a);
        self.u8(0xF7);
        self.u8(0xC0 | (a as u8 & 7)); // /0
        self.i32_le(imm as i32);
    }

    /// `and a, b` (`a &= b`). Opcode `21` ands `reg` into `r/m`.
    pub(crate) fn and_rr(&mut self, a: Reg, b: Reg) {
        self.rex_w(b, a);
        self.u8(0x21);
        self.modrm_rm_reg(a, b);
    }

    /// `or a, b` (`a |= b`). Opcode `09` ors `reg` into `r/m`.
    pub(crate) fn or_rr(&mut self, a: Reg, b: Reg) {
        self.rex_w(b, a);
        self.u8(0x09);
        self.modrm_rm_reg(a, b);
    }

    /// `xor a, b` (`a ^= b`). Opcode `31` xors `reg` into `r/m`.
    pub(crate) fn xor_rr(&mut self, a: Reg, b: Reg) {
        self.rex_w(b, a);
        self.u8(0x31);
        self.modrm_rm_reg(a, b);
    }

    /// `shl r64, cl`. Opcode `D3 /4`.
    pub(crate) fn shl_r_cl(&mut self, a: Reg) {
        self.rex_w(Reg::Rax, a);
        self.u8(0xD3);
        self.u8(0xE0 | (a as u8 & 7)); // /4
    }

    /// `shr r64, imm8` (logical). Opcode `C1 /5`.
    pub(crate) fn shr_r_imm8(&mut self, a: Reg, imm: u8) {
        self.rex_w(Reg::Rax, a);
        self.u8(0xC1);
        self.u8(0xE8 | (a as u8 & 7)); // /5
        self.u8(imm);
    }

    /// `not r64`. Opcode `F7 /2`.
    pub(crate) fn not_r(&mut self, a: Reg) {
        self.rex_w(Reg::Rax, a);
        self.u8(0xF7);
        self.u8(0xD0 | (a as u8 & 7)); // /2
    }

    // ------------------------------------------------------------------
    // SSE2 helpers (session 24: Float arithmetic, comparisons, printing)
    // ------------------------------------------------------------------
    // The xmm registers are hardcoded to 0/1/2 in the ModRM reg field;
    // `mem_modrm` emits the ModRM byte for `[base + disp]` with a GP
    // register's number, and passing `Rax`/`Rcx`/`Rdx` (0/1/2) yields the
    // identical byte for xmm0/xmm1/xmm2. No REX prefix is needed for
    // xmm0-7.

    /// `movsd [base + disp], xmm0`.
    pub(crate) fn movsd_mem_xmm0(&mut self, base: Reg, disp: i32) {
        self.bytes(&[0xF2, 0x0F, 0x11]);
        self.mem_modrm(Reg::Rax, base, disp);
    }

    /// `addsd xmm0, xmm1`.
    pub(crate) fn addsd_xmm0_xmm1(&mut self) {
        self.bytes(&[0xF2, 0x0F, 0x58, 0xC1]);
    }

    /// `subsd xmm0, xmm1`.
    pub(crate) fn subsd_xmm0_xmm1(&mut self) {
        self.bytes(&[0xF2, 0x0F, 0x5C, 0xC1]);
    }

    /// `mulsd xmm0, xmm1`.
    pub(crate) fn mulsd_xmm0_xmm1(&mut self) {
        self.bytes(&[0xF2, 0x0F, 0x59, 0xC1]);
    }

    /// `divsd xmm0, xmm1`.
    pub(crate) fn divsd_xmm0_xmm1(&mut self) {
        self.bytes(&[0xF2, 0x0F, 0x5E, 0xC1]);
    }

    /// `ucomisd xmm0, xmm1` (sets ZF/PF/CF like a compare).
    pub(crate) fn ucomisd_xmm0_xmm1(&mut self) {
        self.bytes(&[0x66, 0x0F, 0x2E, 0xC1]);
    }

    /// `movq xmm0, rax` (the low 64 bits of `rax` as a double).
    pub(crate) fn movq_xmm0_rax(&mut self) {
        self.bytes(&[0x66, 0x48, 0x0F, 0x6E, 0xC0]);
    }

    /// `movq rax, xmm0` (the double's bit pattern).
    pub(crate) fn movq_rax_xmm0(&mut self) {
        self.bytes(&[0x66, 0x48, 0x0F, 0x7E, 0xC0]);
    }

    /// `setcc al` (sets the low byte of `rax`; no REX).
    pub(crate) fn setcc_al(&mut self, condition: u8) {
        self.bytes(&[0x0F, condition, 0xC0]);
    }

    /// `setcc dl` (sets the low byte of `rdx`; no REX).
    pub(crate) fn setcc_dl(&mut self, condition: u8) {
        self.bytes(&[0x0F, condition, 0xC2]);
    }

    /// `and al, dl`.
    pub(crate) fn and_al_dl(&mut self) {
        self.bytes(&[0x20, 0xD0]);
    }

    /// `or al, dl`.
    pub(crate) fn or_al_dl(&mut self) {
        self.bytes(&[0x08, 0xD0]);
    }

    /// `movzx rax, al` (zero-extends the comparison byte).
    pub(crate) fn movzx_rax_al(&mut self) {
        self.bytes(&[0x0F, 0xB6, 0xC0]);
    }

    /// `add a, b` (`a += b`). Opcode `01` adds `reg` into `r/m`.
    pub(crate) fn add_rr(&mut self, a: Reg, b: Reg) {
        self.rex_w(b, a);
        self.u8(0x01);
        self.modrm_rm_reg(a, b);
    }

    /// `add r64, imm8`.
    pub(crate) fn add_r_imm8(&mut self, a: Reg, imm: u8) {
        self.rex_w(Reg::Rax, a);
        self.u8(0x83);
        self.u8(0xC0 | (a as u8 & 7)); // /0
        self.u8(imm);
    }

    /// `sub a, b` (`a -= b`). Opcode `29` subtracts `reg` from `r/m`.
    pub(crate) fn sub_rr(&mut self, a: Reg, b: Reg) {
        self.rex_w(b, a);
        self.u8(0x29);
        self.modrm_rm_reg(a, b);
    }

    /// `sub r64, imm32`.
    pub(crate) fn sub_r_imm32(&mut self, a: Reg, imm: u32) {
        self.rex_w(Reg::Rax, a);
        self.u8(0x81);
        self.u8(0xE8 | (a as u8 & 7)); // /5
        self.i32_le(imm as i32);
    }

    /// `and r64, imm8` (sign-extended).
    pub(crate) fn and_r_imm8(&mut self, a: Reg, imm: u8) {
        self.rex_w(Reg::Rax, a);
        self.u8(0x83);
        self.u8(0xE0 | (a as u8 & 7)); // /4
        self.u8(imm);
    }

    /// `xor r32, r32` (zero-extends).
    pub(crate) fn xor_rr32(&mut self, a: Reg, b: Reg) {
        self.rex(a, b);
        self.u8(0x31);
        self.modrm_reg(a, b);
    }

    /// `shl r64, imm8`.
    pub(crate) fn shl_r_imm8(&mut self, a: Reg, imm: u8) {
        self.rex_w(Reg::Rax, a);
        self.u8(0xC1);
        self.u8(0xE0 | (a as u8 & 7)); // /4
        self.u8(imm);
    }

    /// `neg r64`.
    pub(crate) fn neg_r(&mut self, a: Reg) {
        self.rex_w(Reg::Rax, a);
        self.u8(0xF7);
        self.u8(0xD8 | (a as u8 & 7)); // /3
    }

    /// `dec r64`.
    pub(crate) fn dec_r(&mut self, a: Reg) {
        self.rex_w(Reg::Rax, a);
        self.u8(0xFF);
        self.u8(0xC8 | (a as u8 & 7)); // /1
    }

    /// `mul r64` (unsigned `rax * a`, product in `rdx:rax`).
    pub(crate) fn mul_r(&mut self, a: Reg) {
        self.rex_w(Reg::Rax, a);
        self.u8(0xF7);
        self.u8(0xE0 | (a as u8 & 7)); // /4
    }

    /// `div r64` (unsigned `rdx:rax / a`, quotient in `rax`, remainder in
    /// `rdx`).
    pub(crate) fn div_r(&mut self, a: Reg) {
        self.rex_w(Reg::Rax, a);
        self.u8(0xF7);
        self.u8(0xF0 | (a as u8 & 7)); // /6
    }

    // ------------------------------------------------------------------
    // RIP-relative encodings (patched)
    // ------------------------------------------------------------------

    /// `mov r64, [rip + disp32]`.
    pub(crate) fn mov_r_rip(&mut self, dst: Reg, kind: PatchKind) {
        self.rex_w(dst, Reg::Rax);
        self.u8(0x8B);
        self.u8(0x05 | ((dst as u8 & 7) << 3));
        self.patch(kind);
    }

    /// `mov [rip + disp32], r64`.
    pub(crate) fn mov_rip_r(&mut self, src: Reg, kind: PatchKind) {
        self.rex_w(src, Reg::Rax);
        self.u8(0x89);
        self.u8(0x05 | ((src as u8 & 7) << 3));
        self.patch(kind);
    }

    /// `mov qword [rip + disp32], imm32`.
    pub(crate) fn mov_rip_imm32(&mut self, kind: PatchKind, imm: i32) {
        self.bytes(&[0x48, 0xC7, 0x05]);
        self.patch(kind);
        self.i32_le(imm);
    }

    /// `lea r64, [rip + disp32]`.
    pub(crate) fn lea_r_rip(&mut self, dst: Reg, kind: PatchKind) {
        self.rex_w(dst, Reg::Rax);
        self.u8(0x8D);
        self.u8(0x05 | ((dst as u8 & 7) << 3));
        self.patch(kind);
    }

    /// `call [rip + disp32]` (through the import address table).
    pub(crate) fn call_rip(&mut self, kind: PatchKind) {
        self.bytes(&[0xFF, 0x15]);
        self.patch(kind);
    }

    /// `call rel32`.
    pub(crate) fn call_patch(&mut self, kind: PatchKind) {
        self.u8(0xE8);
        self.patch(kind);
    }

    // ------------------------------------------------------------------
    // REX/ModRM primitives
    // ------------------------------------------------------------------

    /// The REX prefix: `0x48` (W) plus the R and B extension bits for
    /// `reg`/`rm` registers 8–15.
    fn rex_w(&mut self, reg: Reg, rm: Reg) {
        self.u8(0x48 | rex_rb(reg, rm));
    }

    /// The REX prefix without W (32/8-bit operand size).
    fn rex(&mut self, reg: Reg, rm: Reg) {
        self.u8(0x40 | rex_rb(reg, rm));
    }

    /// The ModRM byte for a register-to-register operation with `reg` in
    /// the ModRM reg field and `rm` in the r/m field.
    fn modrm_reg(&mut self, reg: Reg, rm: Reg) {
        self.u8(0xC0 | ((reg as u8 & 7) << 3) | (rm as u8 & 7));
    }

    /// The ModRM byte for a register-to-register operation whose first
    /// operand is the r/m field and whose second operand is the reg field
    /// (the `89`/`01`/`29`/`39` direction).
    fn modrm_rm_reg(&mut self, first: Reg, second: Reg) {
        self.u8(0xC0 | ((second as u8 & 7) << 3) | (first as u8 & 7));
    }

    /// The ModRM byte (plus SIB and displacement) for `[rm + disp]`,
    /// with `reg` in the ModRM reg field. `rsp`/`r12` bases need a SIB
    /// byte; `rbp`/`r13` bases with a zero displacement need an explicit
    /// `disp8` (ModRM `mod=0, rm=5` is RIP-relative).
    fn mem_modrm(&mut self, reg: Reg, base: Reg, disp: i32) {
        let sib_base = base == Reg::Rsp || base == Reg::R12;
        let mod_ = if disp == 0 && base != Reg::Rbp && base != Reg::R13 {
            0
        } else if (-128..=127).contains(&disp) {
            1
        } else {
            2
        };
        let rm = if sib_base { 4 } else { base as u8 & 7 };
        self.u8((mod_ << 6) | ((reg as u8 & 7) << 3) | rm);
        if sib_base {
            self.u8(0x24); // no index, base rsp
        }
        match mod_ {
            1 => self.u8(disp as u8),
            2 => self.i32_le(disp),
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Encodings shared with the existing instruction set
    // ------------------------------------------------------------------

    /// `mov rax, imm64`.
    fn movabs_rax(&mut self, value: i64) {
        self.movabs(Reg::Rax, value as u64);
    }

    /// `mov rcx, imm64`.
    fn movabs_rcx(&mut self, value: i64) {
        self.movabs(Reg::Rcx, value as u64);
    }

    /// `mov rax, [rbp + disp]` (disp8 when it fits, else disp32).
    fn mov_rax_rbp(&mut self, disp: i32) {
        self.mov_r_mem(Reg::Rax, Reg::Rbp, disp);
    }

    /// `mov [rbp + disp], rax`.
    fn mov_rbp_rax(&mut self, disp: i32) {
        self.mov_mem_r(Reg::Rbp, disp, Reg::Rax);
    }

    /// `mov rcx, [rbp + disp]`.
    fn mov_rcx_rbp(&mut self, disp: i32) {
        self.mov_r_mem(Reg::Rcx, Reg::Rbp, disp);
    }

    /// `mov [rip + disp32], rax` (patched later).
    fn mov_rip_rax(&mut self, static_index: usize) {
        self.mov_rip_r(Reg::Rax, PatchKind::Static(static_index));
    }

    /// `push rax`.
    fn push_rax(&mut self) {
        self.u8(0x50);
    }

    /// `xor eax, eax`.
    fn xor_eax(&mut self) {
        self.xor_rr32(Reg::Rax, Reg::Rax);
    }

    /// `test rax, rax`.
    fn test_rax(&mut self) {
        self.test_rr(Reg::Rax, Reg::Rax);
    }

    /// `cmp rax, rcx`.
    fn cmp_rax_rcx(&mut self) {
        self.cmp_rr(Reg::Rax, Reg::Rcx);
    }

    /// `cmp rax, [rbp + disp]`.
    fn cmp_rax_rbp(&mut self, disp: i32) {
        self.cmp_r_mem(Reg::Rax, Reg::Rbp, disp);
    }

    /// `op rax, rcx` for a binary ALU operation.
    fn alu_rax_rcx(&mut self, opcode: u8) {
        self.bytes(&[0x48, opcode, 0xC8]);
    }

    /// `imul rax, rcx`.
    fn imul_rax_rcx(&mut self) {
        self.bytes(&[0x48, 0x0F, 0xAF, 0xC1]);
    }

    /// `imul rax, rax, imm32` (sign-extended immediate).
    pub(crate) fn imul_rax_imm32(&mut self, imm: u32) {
        self.bytes(&[0x48, 0x69, 0xC0]);
        self.i32_le(imm as i32);
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
        self.mov_rr(Reg::Rax, Reg::Rdx);
    }

    /// `shl/shr/sar rax, cl`.
    fn shift_rax_cl(&mut self, opcode: u8) {
        self.bytes(&[0x48, 0xD3, opcode]);
    }

    /// `neg rax`.
    fn neg_rax(&mut self) {
        self.neg_r(Reg::Rax);
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
        self.mem_modrm(Reg::Rax, Reg::Rbp, disp);
        self.u8(0x01);
    }

    /// `sub rsp, imm` (imm8 when it fits, else imm32).
    pub(crate) fn sub_rsp(&mut self, imm: i32) {
        self.bytes(&[0x48, 0x81, 0xEC]);
        self.i32_le(imm);
    }

    /// `add rsp, imm` (imm8 when it fits, else imm32).
    pub(crate) fn add_rsp(&mut self, imm: i32) {
        self.bytes(&[0x48, 0x81, 0xC4]);
        self.i32_le(imm);
    }

    /// `call rel32` to a user function (patched later).
    fn call(&mut self, function_index: usize) {
        self.u8(0xE8);
        self.patch(PatchKind::Function(function_index));
    }

    /// `call rax` — indirect call through the RAX register.
    /// Used for function pointers and closures (session 37).
    /// Encoding: FF D0 (ModRM: mod=11, reg=/2, rm=rax)
    pub(crate) fn call_rax(&mut self) {
        self.u8(0xFF);
        self.u8(0xD0); // ModRM: 11_010_000
    }

    /// `jmp rel32` to a runtime label (patched later).
    pub(crate) fn jmp_label(&mut self, label: u32) {
        self.jmp(PatchKind::Label(label));
    }

    /// `jcc rel32` to a runtime label (patched later).
    pub(crate) fn jcc_label(&mut self, opcode: u8, label: u32) {
        self.jcc(opcode, PatchKind::Label(label));
    }

    /// `jmp rel32` (patched later).
    pub(crate) fn jmp(&mut self, kind: PatchKind) {
        self.u8(0xE9);
        self.patch(kind);
    }

    /// `jcc rel32` (patched later).
    pub(crate) fn jcc(&mut self, opcode: u8, kind: PatchKind) {
        self.bytes(&[0x0F, opcode]);
        self.patch(kind);
    }

    /// `leave; ret`.
    pub(crate) fn leave_ret(&mut self) {
        self.bytes(&[0xC9, 0xC3]);
    }
}

/// The R and B extension bits for a ModRM `reg`/`rm` pair.
fn rex_rb(reg: Reg, rm: Reg) -> u8 {
    ((reg as u8 >> 3) & 1) << 2 | ((rm as u8 >> 3) & 1)
}

/// Emits the x86-64 machine code for `program` and wraps it in a PE image.
///
/// `entry` is the index of the `main` function (validated by the caller).
/// The image layout is:
///
/// - the entry-point stub: capture the process-entry stack pointer, run
///   runtime initialization, call `main`, then call the leak-checked exit
///   service with `main`'s result;
/// - the user functions, in source order;
/// - the embedded runtime services and their message data;
/// - `.data` (module bindings), `.bss` (runtime state), `.idata`
///   (`kernel32` imports), and `.reloc` sections.
pub(crate) fn emit_pe(program: &BProgram, entry: usize) -> EmittedImage {
    let mut code = Code::new();

    // ------------------------------------------------------------------
    // Labels. The string-blob labels are created up front so function
    // bodies can reference them; the region bounds are bound around the
    // string data and recorded by `rt_init`. All are bound before patch
    // resolution.
    // ------------------------------------------------------------------
    let string_labels: Vec<u32> = (0..program.strings.len()).map(|_| code.label()).collect();
    let str_data_start_label = code.label();
    let str_data_end_label = code.label();

    // ------------------------------------------------------------------
    // Entry-point stub. `ret` from the exit service terminates the
    // process with the result as the exit code.
    // ------------------------------------------------------------------
    let entry_offset = 0usize;
    code.mov_rip_r(
        Reg::Rsp,
        PatchKind::Bss(crate::runtime::abi::BSS.entry_rsp as u32),
    );
    code.sub_rsp(8); // align: entry rsp ≡ 8 (mod 16), now ≡ 0
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Init));
    code.call(entry);
    // The exit service takes `main`'s result as a stack argument (the
    // MINK calling convention). One argument word is odd, so the
    // alignment padding is pushed before the argument — the exit service
    // reads the code at `[rbp + 16]`.
    code.sub_rsp(8);
    code.push_rax();
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Exit));
    // Unreachable defensive tail (the exit service restores the entry
    // stack pointer and never returns).
    code.add_rsp(16);
    code.u8(0xC3);

    // ------------------------------------------------------------------
    // Functions, in source order.
    // ------------------------------------------------------------------

    let mut function_starts = Vec::with_capacity(program.functions.len());
    let mut function_block_starts = Vec::with_capacity(program.functions.len());
    for (index, f) in program.functions.iter().enumerate() {
        let (start, block_starts) =
            emit_function(&mut code, f, index, &string_labels, &program.statics);
        function_starts.push(start);
        function_block_starts.push(block_starts);
    }

    // ------------------------------------------------------------------
    // Embedded runtime: services, then the message data.
    // ------------------------------------------------------------------
    let runtime_offsets =
        runtime::emit_services(&mut code, str_data_start_label, str_data_end_label);
    runtime::emit_data(&mut code, &runtime_offsets);

    // ------------------------------------------------------------------
    // Immutable string data: each literal's blob is its length prefix
    // (8 bytes, little-endian) followed by its UTF-8 bytes. `LoadStr`
    // patches to the blob's address here.
    // ------------------------------------------------------------------
    code.bind_label(str_data_start_label);
    for (index, string) in program.strings.iter().enumerate() {
        code.bind_label(string_labels[index]);
        code.bytes(&(string.bytes.len() as u64).to_le_bytes());
        code.bytes(&string.bytes);
    }
    code.bind_label(str_data_end_label);

    // ------------------------------------------------------------------
    // Module bindings: each binding's value image region, in source
    // order. Every region is a multiple of 8 bytes, so the bases stay
    // word-aligned; `static_bases[i]` is the byte offset of binding `i`
    // within `.data` and the `Static` patches resolve against it.
    // ------------------------------------------------------------------
    let mut static_bases = Vec::with_capacity(program.statics.len());
    let mut data = Vec::new();
    for s in &program.statics {
        static_bases.push(data.len());
        data.extend_from_slice(&s.bytes);
    }

    // ------------------------------------------------------------------
    // Layout and patch resolution.
    // ------------------------------------------------------------------
    let text_size = code.len() as u32;
    let layout = pe::layout(
        text_size,
        data.len() as u32,
        crate::runtime::abi::BSS.size as u32,
        pe::IDATA_SIZE,
        12, // the relocation block is always one 12-byte page entry
    );
    let text_rva = layout.text_rva;
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
                (layout.data_rva as i64 + static_bases[*index] as i64)
                    - (text_rva as i64 + patch.offset as i64 + 4)
            }
            PatchKind::RuntimeService(service) => {
                runtime_offsets.of(*service) as i64 - (patch.offset as i64 + 4)
            }
            PatchKind::Bss(offset) => {
                (layout.bss_rva as i64 + *offset as i64)
                    - (text_rva as i64 + patch.offset as i64 + 4)
            }
            PatchKind::Iat(index) => {
                (layout.idata_rva as i64 + pe::IAT_OFFSET as i64 + 8 * *index as i64)
                    - (text_rva as i64 + patch.offset as i64 + 4)
            }
            PatchKind::Label(id) => {
                let target = code.labels[*id as usize]
                    .expect("runtime labels are bound before patch resolution")
                    as i64;
                target - (patch.offset as i64 + 4)
            }
        };
        code.buf[patch.offset..patch.offset + 4].copy_from_slice(&(disp as i32).to_le_bytes());
    }

    let idata = pe::build_idata(layout.idata_rva);
    let bytes = pe::build(
        &code.buf,
        &data,
        layout,
        &idata,
        &reloc,
        entry_offset as u32,
    );
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
    string_labels: &[u32],
    statics: &[super::super::ir::BStatic],
) -> (usize, Vec<u32>) {
    let start = code.len();
    let (slots, total_words) = slots(f);
    let frame = frame_size(total_words);

    // Prologue.
    code.u8(0x55); // push rbp
    code.bytes(&[0x48, 0x89, 0xE5]); // mov rbp, rsp
    if frame > 0 {
        code.sub_rsp(frame);
    }

    // Copy parameters into their slots. Each parameter occupies
    // `words` stack words, pushed rightmost-first by the caller so word 0
    // is on top; the callee reads word `k` at `arg_base + 8k` and stores
    // it at `word0 - 8k` (the value's `k`-th word).
    let mut arg_words = 0usize;
    for &param in &f.params {
        let words = f
            .local(param)
            .map(|local| local.words as usize)
            .unwrap_or(1);
        let arg_base = 16 + 8 * arg_words;
        let word0 = slots[param.raw() as usize].0;
        for k in 0..words {
            code.mov_rax_rbp(arg_base as i32 + 8 * k as i32);
            code.mov_rbp_rax(word0 - 8 * k as i32);
        }
        arg_words += words;
    }
    // A multi-word result is returned through the caller-allocated return
    // slot whose address is the hidden argument right after the visible
    // parameters (session 22).
    let param_words = arg_words;

    // The function's shared `E-R10` failure block: bound after the last
    // block, referenced by every generated array bounds check in the
    // function. `rt_fail` never returns, so the block needs no terminator.
    let fail_label = code.label();

    // Blocks, in order. Starts are filled in as blocks are emitted; forward
    // jumps patch by block id after layout, so the full table is allocated
    // up front.
    let mut block_starts = vec![0u32; f.blocks.len()];
    for block in &f.blocks {
        block_starts[block.id.raw() as usize] = (code.len() - start) as u32;
        emit_block(
            code,
            f,
            &slots,
            block,
            function_index,
            string_labels,
            fail_label,
            param_words,
            statics,
        );
    }
    code.bind_label(fail_label);
    runtime::fail(code, 10); // E-R10
    (start, block_starts)
}

/// The patch kind for a jump/branch target within the current function.
fn block_target(function_index: usize, target: BlockId) -> PatchKind {
    PatchKind::Block {
        function: function_index,
        block: target.raw(),
    }
}

#[allow(clippy::too_many_arguments)] // the emitter context is threaded per block
fn emit_block(
    code: &mut Code,
    f: &super::super::ir::BFunction,
    slots: &Slots,
    block: &super::super::ir::BBlock,
    function_index: usize,
    string_labels: &[u32],
    fail_label: u32,
    param_words: usize,
    statics: &[super::super::ir::BStatic],
) {
    for inst in &block.insts {
        emit_inst(code, f, slots, inst, string_labels, fail_label, statics);
    }
    match &block.terminator {
        BTerminator::Return { value, .. } => {
            // A multi-word result (session 22: structs, arrays, tagged-
            // union enums) is returned through the caller-allocated return
            // slot: copy the value image into the memory addressed by the
            // hidden argument (word `k` at `slot_addr - 8k`, matching the
            // slot convention).
            if f.result_words > 1 {
                code.mov_r_mem(Reg::Rcx, Reg::Rbp, (16 + 8 * param_words) as i32);
                let size = (8 * f.result_words as usize) as u32;
                match value {
                    Some(operand) => {
                        store_into(code, slots, *operand, Reg::Rcx, size);
                    }
                    // A bare return in an aggregate-returning function
                    // contributes no value; zero the region for
                    // determinism.
                    None => {
                        for k in 0..f.result_words as usize {
                            code.movabs_rax(0);
                            code.mov_mem_r(Reg::Rcx, -((8 * k) as i32), Reg::Rax);
                        }
                    }
                }
                code.leave_ret();
                return;
            }
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
/// the leftmost ends on top). Multi-word values (`Range`, aggregates) push
/// their words last-word-first, so the callee reads word `k` at
/// `[rbp + 16 + 8k]` (word 0 on top). Returns the number of words pushed.
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
            let words = f.local(id).map(|local| local.words as usize).unwrap_or(1);
            let word0 = slots[id.raw() as usize].0;
            for k in (0..words).rev() {
                code.mov_rax_rbp(word0 - 8 * k as i32);
                code.push_rax();
            }
            words
        }
    }
}

/// The number of stack words an operand occupies (mirrors `push_operand`).
fn operand_words(f: &super::super::ir::BFunction, operand: BOperand) -> usize {
    match operand {
        BOperand::Const(_) => 1,
        BOperand::Local(id) => f.local(id).map(|local| local.words as usize).unwrap_or(1),
    }
}

/// Emits the call sequence for a runtime service: push the alignment
/// padding first (when the argument count is odd), then the arguments
/// rightmost-first (the runtime uses the same convention as user
/// functions — argument 1 must be on top of the stack at the call so the
/// callee reads it at `[rbp + 16]`), call, and store the result.
fn emit_runtime_call(
    code: &mut Code,
    f: &super::super::ir::BFunction,
    slots: &Slots,
    target: crate::mir::LocalId,
    service: RuntimeService,
    args: &[BOperand],
) {
    let mut words = 0usize;
    for arg in args {
        words += operand_words(f, *arg);
    }
    let pad = words % 2;
    if pad == 1 {
        code.sub_rsp(8);
    }
    for arg in args.iter().rev() {
        push_operand(code, f, slots, *arg);
    }
    code.call_patch(PatchKind::RuntimeService(service));
    if words + pad > 0 {
        code.add_rsp((8 * (words + pad)) as i32);
    }
    code.mov_rbp_rax(slots[target.raw() as usize].0);
}

#[allow(clippy::too_many_arguments)] // the emitter context is threaded per instruction
fn emit_inst(
    code: &mut Code,
    f: &super::super::ir::BFunction,
    slots: &Slots,
    inst: &super::super::ir::BInst,
    string_labels: &[u32],
    fail_label: u32,
    statics: &[super::super::ir::BStatic],
) {
    match &inst.kind {
        BInstKind::LoadLocal { target, src } => {
            // The copy spans the local's full width: one word for scalars,
            // two for ranges, `words` for aggregate values.
            let words = f.local(*src).map(|local| local.words as usize).unwrap_or(1);
            let (dst0, _) = slots[target.raw() as usize];
            let (src0, _) = slots[src.raw() as usize];
            for k in 0..words {
                code.mov_rax_rbp(src0 - 8 * k as i32);
                code.mov_rbp_rax(dst0 - 8 * k as i32);
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
            // The data region holds the value in normal byte order (byte
            // `b` at `base + b`); the slot's chunked image (byte `b` at
            // `word0 - 8*(b/8) + (b%8)`) is filled with the matching
            // byte-exact copy, so sub-word values (boolean tails,
            // sub-word element arrays) land at their chunk positions.
            code.lea_r_rip(Reg::Rcx, PatchKind::Static(*static_index));
            let size = statics[*static_index].size;
            copy_static_into_slot(code, slots, *target, Reg::Rcx, size);
        }
        BInstKind::LoadFnPtr {
            target,
            function_index,
        } => {
            // Session 37: load the address of a function into the target slot.
            // The address is a RIP-relative reference to the function's
            // entry point, patched during image layout.
            code.lea_r_rip(Reg::Rax, PatchKind::Function(*function_index));
            code.mov_rbp_rax(slots[target.raw() as usize].0);
        }
        BInstKind::StoreStatic { static_index, src } => {
            let size = statics[*static_index].size;
            if size <= 8 {
                // A single-word (or single-byte) binding is stored as its
                // word value.
                eval_rax(code, slots, *src);
                code.mov_rip_rax(*static_index);
            } else {
                // A multi-word aggregate binding: copy the slot's chunked
                // image into the data region's normal byte order with the
                // byte-exact store used for every value move.
                code.lea_r_rip(Reg::Rcx, PatchKind::Static(*static_index));
                match src {
                    // Defensive: a clean pipeline never stores a constant
                    // into a multi-word binding (aggregate values are
                    // materialized into locals); zero the region for
                    // determinism rather than emitting nothing.
                    BOperand::Const(value) => {
                        code.movabs_rax(*value);
                        for k in 0..size.div_ceil(8) {
                            code.mov_mem_r(Reg::Rcx, (8 * k) as i32, Reg::Rax);
                        }
                    }
                    BOperand::Local(id) => {
                        store_slot_to_static(code, slots, *id, Reg::Rcx, size);
                    }
                }
            }
        }
        BInstKind::LoadStr {
            target,
            string_index,
        } => {
            // The string value is the blob's address in the image's
            // immutable data (the length prefix), reached RIP-relative.
            code.lea_r_rip(Reg::Rax, PatchKind::Label(string_labels[*string_index]));
            code.mov_rbp_rax(slots[target.raw() as usize].0);
        }
        BInstKind::Unary { target, op, src } => {
            let float_neg =
                *op == UnaryOp::Neg && f.local(*target).map(|local| local.ty) == Some(BType::Float);
            eval_rax(code, slots, *src);
            match op {
                UnaryOp::Neg if float_neg => {
                    // Float negation flips the sign bit (IEEE-754): `-x`
                    // is the same magnitude with the opposite sign, which
                    // is exactly a bitwise xor of the sign bit — no SSE
                    // required.
                    // i64::MIN is the sign-bit mask: `-x` == xor of the
                    // sign bit (works for ±0 and NaN too).
                    code.movabs_rcx(i64::MIN);
                    code.xor_rr(Reg::Rax, Reg::Rcx);
                }
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
            ty,
            lhs,
            rhs,
        } => {
            if *ty == BType::Float {
                emit_float_binary(code, f, slots, *target, *op, *lhs, *rhs);
            } else {
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
                    // The language's `>>` is defined as an arithmetic
                    // shift (signed semantics), matching the 64-bit
                    // integer model.
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
        }
        BInstKind::Call {
            target,
            callee,
            args,
        } => {
            // The alignment padding is pushed before the arguments (odd
            // argument counts), so argument 1 is on top of the stack at
            // the call and the callee reads it at `[rbp + 16]`.
            //
            // A multi-word result (session 22: structs, arrays, tagged-
            // union enums) is returned through a caller-allocated return
            // slot: the target slot's address is pushed as a hidden
            // argument (rightmost, so it sits just after the visible
            // parameters), the callee copies its result into it, and the
            // caller does not read `rax`.
            let aggregate = f
                .local(*target)
                .map(|local| local.words > 1)
                .unwrap_or(false);
            let mut words = 0usize;
            for arg in args {
                words += operand_words(f, *arg);
            }
            let total = words + usize::from(aggregate);
            let pad = total % 2;
            if pad == 1 {
                code.sub_rsp(8);
            }
            if aggregate {
                code.lea_r_mem(Reg::Rax, Reg::Rbp, slots[target.raw() as usize].0);
                code.push_rax();
            }
            for arg in args.iter().rev() {
                push_operand(code, f, slots, *arg);
            }
            code.call(*callee);
            if total > 0 {
                code.add_rsp((8 * total) as i32);
            }
            if !aggregate {
                code.mov_rbp_rax(slots[target.raw() as usize].0);
            }
        }
        BInstKind::RuntimeCall {
            target,
            service,
            args,
        } => emit_runtime_call(code, f, slots, *target, *service, args),
        BInstKind::IndirectCall {
            target,
            fn_ptr,
            args,
        } => {
            // Session 37: indirect call through a function pointer.
            // Same calling convention as direct calls, but the callee
            // address is loaded from a register instead of a fixed label.
            let aggregate = f
                .local(*target)
                .map(|local| local.words > 1)
                .unwrap_or(false);
            let mut words = 0usize;
            for arg in args {
                words += operand_words(f, *arg);
            }
            let total = words + usize::from(aggregate);
            let pad = total % 2;
            if pad == 1 {
                code.sub_rsp(8);
            }
            if aggregate {
                code.lea_r_mem(Reg::Rax, Reg::Rbp, slots[target.raw() as usize].0);
                code.push_rax();
            }
            for arg in args.iter().rev() {
                push_operand(code, f, slots, *arg);
            }
            // Load the function pointer into rax.
            match fn_ptr {
                BOperand::Local(id) => {
                    code.mov_r_mem(Reg::Rax, Reg::Rbp, slots[id.raw() as usize].0);
                }
                BOperand::Const(addr) => {
                    code.movabs(Reg::Rax, *addr as u64);
                }
            }
            code.call_rax();
            if total > 0 {
                code.add_rsp((8 * total) as i32);
            }
            if !aggregate {
                code.mov_rbp_rax(slots[target.raw() as usize].0);
            }
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
        BInstKind::FieldLoad {
            target,
            base,
            field_ty,
            byte_offset,
            size,
        } => {
            let base_word0 = slots[base.raw() as usize].0;
            // The field's first byte is at its value-byte offset: within
            // its 8-byte chunk in normal order, chunks stacked downward
            // (byte `b` at `word0 - 8*(b/8) + (b%8)`), so a sub-word field
            // at offsets 1..7 sits at the top of the first chunk instead
            // of colliding with the qword below it.
            let offset = *byte_offset as i32;
            code.lea_r_mem(
                Reg::Rcx,
                Reg::Rbp,
                base_word0 - (offset - offset % 8) + offset % 8,
            );
            copy_into_slot(code, slots, *target, Reg::Rcx, *size);
            let _ = field_ty;
        }
        BInstKind::FieldStore {
            base,
            field_ty,
            byte_offset,
            size,
            src,
        } => {
            let base_word0 = slots[base.raw() as usize].0;
            let offset = *byte_offset as i32;
            code.lea_r_mem(
                Reg::Rcx,
                Reg::Rbp,
                base_word0 - (offset - offset % 8) + offset % 8,
            );
            store_into(code, slots, *src, Reg::Rcx, *size);
            let _ = field_ty;
        }
        BInstKind::IndexLoad {
            target,
            base,
            elem_ty,
            stride,
            len,
            index,
        } => {
            // Bounds check: the unsigned compare treats a negative index
            // as huge, so one `jae` covers both out-of-range directions
            // (`E-R10`). The fail path is the function's shared
            // `E-R10` block (bound after the last block), so a valid
            // access never falls into it.
            eval_rax(code, slots, *index);
            code.cmp_r_imm32(Reg::Rax, *len as u32);
            code.jcc(0x83, PatchKind::Label(fail_label)); // jae
            // rax = index * stride; the element address is the value-byte
            // offset `rax` within the chunked image: `word0 - rax +
            // 2*(rax % 8)`. Full-word strides leave `rax % 8 == 0`, so
            // the correction is a no-op for them; sub-word elements
            // (booleans) land at their chunk positions instead of
            // colliding with the qword below.
            code.imul_rax_imm32(*stride);
            code.lea_r_mem(Reg::Rcx, Reg::Rbp, slots[base.raw() as usize].0);
            code.sub_rr(Reg::Rcx, Reg::Rax);
            code.and_r_imm8(Reg::Rax, 7);
            code.add_rr(Reg::Rcx, Reg::Rax);
            code.add_rr(Reg::Rcx, Reg::Rax);
            copy_into_slot(code, slots, *target, Reg::Rcx, *stride);
            let _ = elem_ty;
        }
        BInstKind::IndexStore {
            base,
            elem_ty,
            stride,
            len,
            index,
            src,
        } => {
            eval_rax(code, slots, *index);
            code.cmp_r_imm32(Reg::Rax, *len as u32);
            code.jcc(0x83, PatchKind::Label(fail_label)); // jae
            code.imul_rax_imm32(*stride);
            code.lea_r_mem(Reg::Rcx, Reg::Rbp, slots[base.raw() as usize].0);
            code.sub_rr(Reg::Rcx, Reg::Rax);
            code.and_r_imm8(Reg::Rax, 7);
            code.add_rr(Reg::Rcx, Reg::Rax);
            code.add_rr(Reg::Rcx, Reg::Rax);
            store_into(code, slots, *src, Reg::Rcx, *stride);
            let _ = elem_ty;
        }
        BInstKind::PlaceStore {
            base,
            steps,
            size,
            src,
        } => {
            // Walk the place chain from the root's first word. Field
            // steps move to the field's chunk position; index steps are
            // bounds-checked (`E-R10`) and move to the element's chunk
            // position (`word0 - off + 2*(off % 8)` for `off =
            // index * stride`, a no-op for full-word strides). The fail
            // path is the function's shared `E-R10` block, so valid
            // chains never fall into it.
            code.lea_r_mem(Reg::Rcx, Reg::Rbp, slots[base.raw() as usize].0);
            for step in steps {
                match step {
                    PlaceAddrStep::Field { byte_offset } => {
                        let offset = *byte_offset as i32;
                        code.sub_r_imm32(Reg::Rcx, (offset - offset % 8) as u32);
                        code.add_r_imm8(Reg::Rcx, (offset % 8) as u8);
                    }
                    PlaceAddrStep::Index { index, stride, len } => {
                        eval_rax(code, slots, *index);
                        code.cmp_r_imm32(Reg::Rax, *len as u32);
                        code.jcc(0x83, PatchKind::Label(fail_label)); // jae
                        code.imul_rax_imm32(*stride);
                        code.sub_rr(Reg::Rcx, Reg::Rax);
                        code.and_r_imm8(Reg::Rax, 7);
                        code.add_rr(Reg::Rcx, Reg::Rax);
                        code.add_rr(Reg::Rcx, Reg::Rax);
                    }
                }
            }
            store_into(code, slots, *src, Reg::Rcx, *size);
        }
        BInstKind::RefAddr {
            target,
            base,
            steps,
        } => {
            // `&place`: compute the place's machine address (the root's
            // first-word address walked by the same field/index steps as
            // `PlaceStore`) and store it into the reference slot. The
            // running address stays in `rcx`; `rax` is scratch for index
            // arithmetic.
            let target_word0 = slots[target.raw() as usize].0;
            code.lea_r_mem(Reg::Rcx, Reg::Rbp, slots[base.raw() as usize].0);
            for step in steps {
                match step {
                    PlaceAddrStep::Field { byte_offset } => {
                        let offset = *byte_offset as i32;
                        code.sub_r_imm32(Reg::Rcx, (offset - offset % 8) as u32);
                        code.add_r_imm8(Reg::Rcx, (offset % 8) as u8);
                    }
                    PlaceAddrStep::Index { index, stride, len } => {
                        eval_rax(code, slots, *index);
                        code.cmp_r_imm32(Reg::Rax, *len as u32);
                        code.jcc(0x83, PatchKind::Label(fail_label)); // jae
                        code.imul_rax_imm32(*stride);
                        code.sub_rr(Reg::Rcx, Reg::Rax);
                        code.and_r_imm8(Reg::Rax, 7);
                        code.add_rr(Reg::Rcx, Reg::Rax);
                        code.add_rr(Reg::Rcx, Reg::Rax);
                    }
                }
            }
            code.mov_mem_r(Reg::Rbp, target_word0, Reg::Rcx);
        }
        BInstKind::RefLoad {
            target,
            reference,
            elem_ty,
            size,
        } => {
            // `*r` read: copy `size` bytes from the referenced address.
            // The address lives in `rcx` so `copy_into_slot`'s `rax`
            // scratch never clobbers it (mirroring `FieldLoad`).
            eval_rcx(code, slots, *reference);
            copy_into_slot(code, slots, *target, Reg::Rcx, *size);
            let _ = elem_ty;
        }
        BInstKind::RefStore {
            reference,
            elem_ty,
            size,
            src,
        } => {
            // `*r = v`: store `size` bytes to the referenced address.
            // The address lives in `rcx` so `store_into`'s `rax` scratch
            // never clobbers it (mirroring `FieldStore`).
            eval_rcx(code, slots, *reference);
            store_into(code, slots, *src, Reg::Rcx, *size);
            let _ = elem_ty;
        }
        BInstKind::EnumInit {
            target,
            discriminant,
            payload,
            tag_offset,
            payload_offset,
            payload_size,
        } => {
            // A tagged-union construction (session 19): the discriminant
            // (tag) word is written at its offset, then the variant's own
            // payload bytes are copied into the payload area.
            let word0 = slots[target.raw() as usize].0;
            code.movabs_rax(*discriminant);
            code.mov_rbp_rax(word0 - *tag_offset as i32);
            if let Some(payload) = payload {
                let dst = word0 - *payload_offset as i32;
                code.lea_r_mem(Reg::Rcx, Reg::Rbp, dst);
                store_into(code, slots, *payload, Reg::Rcx, *payload_size);
            }
        }
        BInstKind::EnumTag {
            target,
            value,
            tag_offset,
        } => {
            // The tag is the discriminant word at its offset within the
            // enum value (for the current layout, the first word).
            let value_word0 = slots[value.raw() as usize].0;
            code.mov_rax_rbp(value_word0 - *tag_offset as i32);
            code.mov_rbp_rax(slots[target.raw() as usize].0);
        }
        BInstKind::EnumPayload {
            target,
            value,
            payload_offset,
            payload_size,
        } => {
            // The payload area starts `payload_offset` bytes below the
            // value's first word (for the current layout the payload
            // offset is always 8, the start of the second chunk);
            // `payload_size` bytes are copied exactly (the payload's own
            // size, never the shared area's full width).
            let value_word0 = slots[value.raw() as usize].0;
            code.lea_r_mem(Reg::Rcx, Reg::Rbp, value_word0 - *payload_offset as i32);
            copy_into_slot(code, slots, *target, Reg::Rcx, *payload_size);
        }
    }
}

/// Emits the SSE2 evaluation of a float binary operation (session 24):
/// loads the operands into `xmm0`/`xmm1`, applies the operator, and
/// stores the result — a `Float` for arithmetic (in `xmm0`), a `Bool`
/// `0`/`1` for comparisons and equality (in `rax`) — into the target
/// slot. `%` is the IEEE truncation remainder `x - y*trunc(x/y)`, with
/// `trunc` computed by masking the fractional mantissa bits toward zero
/// (SSE2 has no remainder instruction).
fn emit_float_binary(
    code: &mut Code,
    f: &super::super::ir::BFunction,
    slots: &Slots,
    target: crate::mir::LocalId,
    op: BinaryOp,
    lhs: BOperand,
    rhs: BOperand,
) {
    load_float(code, slots, lhs, Reg::Rax); // xmm0
    load_float(code, slots, rhs, Reg::Rcx); // xmm1
    use BinaryOp::*;
    let arithmetic = matches!(op, Add | Sub | Mul | Div | Rem);
    match op {
        Add => code.addsd_xmm0_xmm1(),
        Sub => code.subsd_xmm0_xmm1(),
        Mul => code.mulsd_xmm0_xmm1(),
        Div => code.divsd_xmm0_xmm1(),
        Rem => {
            // xmm0 = x/y; truncate toward zero by clearing the fractional
            // mantissa bits (for |v| < 2^52; NaN/Inf are left alone, and
            // |v| < 1 truncates to zero).
            let done = code.label();
            let zero = code.label();
            code.divsd_xmm0_xmm1(); // xmm0 = x / y
            code.movq_rax_xmm0();
            code.mov_rr(Reg::Rcx, Reg::Rax); // a copy for the exponent
            code.shr_r_imm8(Reg::Rcx, 52);
            code.movabs(Reg::Rdx, 0x7FF);
            code.and_rr(Reg::Rcx, Reg::Rdx); // exponent field
            code.cmp_r_imm32(Reg::Rcx, 0x7FF);
            code.jcc_label(0x83, done); // jae: NaN/Inf stay as-is
            code.cmp_r_imm32(Reg::Rcx, 1023);
            code.jcc_label(0x82, zero); // jb: |v| < 1, trunc = 0
            code.movabs(Reg::Rdx, 1075);
            code.sub_rr(Reg::Rdx, Reg::Rcx); // shift = 1075 - exp (2..=52)
            code.mov_rr(Reg::Rcx, Reg::Rdx); // shift into rcx for `shl r8, cl`
            code.movabs(Reg::R8, 1);
            code.shl_r_cl(Reg::R8); // 1 << shift
            code.dec_r(Reg::R8); // mask = 2^shift - 1
            code.not_r(Reg::R8); // ~mask
            code.and_rr(Reg::Rax, Reg::R8);
            code.jmp_label(done);
            code.bind_label(zero);
            code.xor_rr32(Reg::Rax, Reg::Rax);
            code.bind_label(done);
            code.movq_xmm0_rax(); // trunc(x/y)
            // rem = x - y*trunc(x/y): reload `x` into xmm2.
            code.mulsd_xmm0_xmm1();
            load_float(code, slots, lhs, Reg::Rdx); // xmm2
            code.bytes(&[0xF2, 0x0F, 0x5C, 0xD0]); // subsd xmm2, xmm0
            code.bytes(&[0xF2, 0x0F, 0x10, 0xC2]); // movsd xmm0, xmm2
        }
        Lt => emit_float_compare(code, 0x92, 0x9B, CompareForm::OrderedAnd), // setb && setnp
        Le => emit_float_compare(code, 0x96, 0x9B, CompareForm::OrderedAnd), // setbe && setnp
        Gt => emit_float_compare(code, 0x97, 0, CompareForm::Plain),         // seta
        Ge => emit_float_compare(code, 0x93, 0, CompareForm::Plain),         // setae
        Eq => emit_float_compare(code, 0x94, 0x9B, CompareForm::OrderedAnd), // sete && setnp
        Ne => emit_float_compare(code, 0x95, 0x9A, CompareForm::UnorderedOr), // setne || setp
        _ => unreachable!("the type checker rejects non-float operators on Float"),
    }
    if arithmetic {
        code.movsd_mem_xmm0(Reg::Rbp, slots[target.raw() as usize].0);
    } else {
        code.mov_rbp_rax(slots[target.raw() as usize].0);
    }
    let _ = f;
}

/// Loads a float operand into an xmm register: a local's slot with
/// `movsd`, or a constant's bit pattern via `movabs` + `movq`. The `reg`
/// argument is the GP register whose number matches the xmm register
/// (0/1/2 → `Rax`/`Rcx`/`Rdx`), which `mem_modrm` uses for the ModRM
/// field.
fn load_float(code: &mut Code, slots: &Slots, operand: BOperand, reg: Reg) {
    match operand {
        BOperand::Const(value) => {
            code.movabs_rax(value);
            // `movq xmmN, rax`: REX.W + the ModRM reg field (bits 3-5)
            // holding the xmm destination; rm is rax (000).
            code.bytes(&[0x66, 0x48, 0x0F, 0x6E, 0xC0 | ((reg as u8 & 7) << 3)]);
        }
        BOperand::Local(id) => {
            code.bytes(&[0xF2, 0x0F, 0x10]);
            code.mem_modrm(reg, Reg::Rbp, slots[id.raw() as usize].0); // movsd xmmN, [rbp+disp]
        }
    }
}

/// How a float comparison's setcc sequence combines with the parity flag
/// (which `ucomisd` sets when either operand is NaN): ordered comparisons
/// mask the unordered case out (`&& !PF`), `!=` is true when unordered
/// (`|| PF`), and `>`/`>=` fall out false from the flags alone.
#[derive(Clone, Copy)]
enum CompareForm {
    /// `setcc al; setnp dl; and al, dl` — true only when ordered and `cc`.
    OrderedAnd,
    /// `setcc al; setp dl; or al, dl` — true when unordered or `cc`.
    UnorderedOr,
    /// `setcc al` alone (unordered yields false from the flags).
    Plain,
}

/// Emits `ucomisd xmm0, xmm1` followed by the comparison's setcc
/// sequence, leaving the `Bool` result (`0`/`1`) zero-extended in `rax`.
fn emit_float_compare(code: &mut Code, setcc: u8, extra: u8, form: CompareForm) {
    code.ucomisd_xmm0_xmm1();
    match form {
        CompareForm::OrderedAnd => {
            code.setcc_al(setcc);
            code.setcc_dl(extra);
            code.and_al_dl();
        }
        CompareForm::UnorderedOr => {
            code.setcc_al(setcc);
            code.setcc_dl(extra);
            code.or_al_dl();
        }
        CompareForm::Plain => {
            code.setcc_al(setcc);
        }
    }
    code.movzx_rax_al();
}

/// Copies `size` bytes from the memory at `src` (a register holding the
/// source address of the value's first byte) into the destination local's
/// slot image, preserving the byte layout: aggregate value bytes sit in
/// normal byte order within each 8-byte chunk, and chunks are stacked
/// downward (byte `b` of a value lives at `word0 - 8*(b/8) + (b%8)`).
/// Word-sized runs move full 8-byte words; the remainder is moved byte by
/// byte so fields that are not word-aligned (booleans, nested all-bool
/// structs) keep their chunk positions exactly.
fn copy_into_slot(code: &mut Code, slots: &Slots, dst: crate::mir::LocalId, src: Reg, size: u32) {
    let word0 = slots[dst.raw() as usize].0;
    let words = size / 8;
    for k in 0..words {
        let k = (8 * k) as i32;
        code.mov_r_mem(Reg::Rax, src, -k);
        code.mov_mem_r(Reg::Rbp, word0 - k, Reg::Rax);
    }
    for k in 0..(size % 8) {
        // The remainder bytes sit at the start of the piece's last chunk,
        // in normal byte order (the same relative positions in both the
        // source region and the destination slot).
        code.movzx_byte(Reg::Rax, src, -8 * words as i32 + k as i32);
        code.mov_mem_r8(Reg::Rbp, word0 - 8 * words as i32 + k as i32, Reg::Rax);
    }
}

/// Copies `size` bytes from a static data region (normal byte order:
/// byte `b` at `[base + b]`) into the destination local's slot image
/// (byte `b` at `[word0 - 8*(b/8) + (b%8)]`). Full words move as qwords;
/// the sub-word remainder moves byte by byte, so sub-word values
/// (booleans, sub-word element arrays) land at their chunk positions.
fn copy_static_into_slot(
    code: &mut Code,
    slots: &Slots,
    dst: crate::mir::LocalId,
    src: Reg,
    size: u32,
) {
    let word0 = slots[dst.raw() as usize].0;
    let words = size / 8;
    for k in 0..words {
        let k = (8 * k) as i32;
        code.mov_r_mem(Reg::Rax, src, k);
        code.mov_mem_r(Reg::Rbp, word0 - k, Reg::Rax);
    }
    for k in 0..(size % 8) {
        // The remainder bytes sit at the start of the piece's last chunk,
        // so the data byte at `base + words*8 + k` lands at the slot's
        // corresponding chunk position.
        let byte = (words * 8 + k) as i32;
        code.movzx_byte(Reg::Rax, src, byte);
        code.mov_mem_r8(Reg::Rbp, word0 - 8 * words as i32 + k as i32, Reg::Rax);
    }
}

/// Stores a source local's downward slot image into a static data region
/// (normal byte order: byte `b` at `[base + b]`), the inverse of
/// [`copy_static_into_slot`].
fn store_slot_to_static(
    code: &mut Code,
    slots: &Slots,
    src: crate::mir::LocalId,
    dst: Reg,
    size: u32,
) {
    let word0 = slots[src.raw() as usize].0;
    let words = size / 8;
    for k in 0..words {
        let k = (8 * k) as i32;
        code.mov_rax_rbp(word0 - k);
        code.mov_mem_r(dst, k, Reg::Rax);
    }
    for k in 0..(size % 8) {
        let byte = (words * 8 + k) as i32;
        code.movzx_byte(Reg::Rax, Reg::Rbp, word0 - 8 * words as i32 + k as i32);
        code.mov_mem_r8(dst, byte, Reg::Rax);
    }
}

/// Stores `src` into the memory at `dst` (a register holding the
/// destination address — the first byte of a field, element, or payload
/// region, whose bytes sit in chunk order): `size` bytes copied from the
/// source local's slot image, or a single byte/word for a scalar constant.
fn store_into(code: &mut Code, slots: &Slots, src: BOperand, dst: Reg, size: u32) {
    match src {
        BOperand::Const(value) => {
            // Scalar constants are only ever stored into single-byte
            // (boolean) or single-word (integer-class) fields/elements.
            code.movabs_rax(value);
            if size == 1 {
                code.mov_mem_r8(dst, 0, Reg::Rax);
            } else {
                code.mov_mem_r(dst, 0, Reg::Rax);
            }
        }
        BOperand::Local(id) => {
            let word0 = slots[id.raw() as usize].0;
            let words = size / 8;
            for k in 0..words {
                let k = (8 * k) as i32;
                code.mov_r_mem(Reg::Rax, Reg::Rbp, word0 - k);
                code.mov_mem_r(dst, -k, Reg::Rax);
            }
            for k in 0..(size % 8) {
                code.movzx_byte(Reg::Rax, Reg::Rbp, word0 - 8 * words as i32 + k as i32);
                code.mov_mem_r8(dst, -8 * words as i32 + k as i32, Reg::Rax);
            }
        }
    }
}
