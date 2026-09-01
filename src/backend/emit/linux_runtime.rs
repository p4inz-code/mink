//! Linux x86_64 runtime services — filesystem foundation.
//!
//! Provides the core MINK runtime services on Linux using raw x86_64 syscalls.
//!
//! Implemented services:
//! - Init, Alloc (full liveness table), Free (full liveness table),
//!   MemLoad, MemStore (full validation)
//! - Exit (with leak check), Fail (error reporting)
//! - PrintStr, PrintInt, PrintFloat, PrintChar
//! - StrAlloc, StrFree, StrLen, StrByte, StrSetByte
//! - StrValidate, StrValidateHeap
//! - StrConcat, StrEq, StrFromInt, StrFromBool
//! - IntToFloat, FloatToInt
//! - WriteStdout, WriteStderr
//! - ToCstr, FreeCstr
//! - FsRead, FsWrite, FsExists, FsFileSize, FsCreateDir, FsRemoveDir,
//!   FsRemoveFile, FsCopy, FsMove, FsGetCwd, FsSetCwd
//!
//! Stub services (emit a trap for unimplemented subsystems):
//! - Vec*, Process*, Time*, Random*, Env*, Net*, Crypto*

use std::collections::HashMap;

use super::super::ir::RuntimeService;
use super::x86_64::{Code, PatchKind, Reg};
use crate::runtime::abi::{BSS, HEAP_SIZE, MAX_LIVE_ALLOCS};
use crate::runtime::error::RuntimeErrorKind;

// ===========================================================================
// Linux syscall numbers
// ===========================================================================

const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_STAT: u64 = 4;
const SYS_LSEEK: u64 = 8;
const SYS_ACCESS: u64 = 21;
const SYS_GETCWD: u64 = 79;
const SYS_CHDIR: u64 = 80;
const SYS_RENAME: u64 = 82;
const SYS_MKDIR: u64 = 83;
const SYS_RMDIR: u64 = 84;
const SYS_UNLINK: u64 = 87;
const SYS_EXIT: u64 = 60;

// ===========================================================================
// Linux file constants
// ===========================================================================

const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 1;
const O_CREAT: i32 = 0o100;
const O_TRUNC: i32 = 0o1000;
const F_OK: i32 = 0;
const O_NOCTTY: i32 = 0o400;

// stat buffer size on x86_64 Linux (144 bytes)
const STAT_BUF_SIZE: i32 = 144;
// st_mode offset in stat struct
const STAT_MODE_OFFSET: i32 = 24;
// st_size offset in stat struct
const STAT_SIZE_OFFSET: i32 = 48;
// S_IFMT and S_IFREG for checking regular file type
const S_IFMT: u64 = 0o170000;
const S_IFREG: u64 = 0o100000;

// ===========================================================================
// Liveness table layout (matching Windows ABI)
// ===========================================================================

/// The liveness table size in bytes.
const TABLE_BYTES: i32 = (MAX_LIVE_ALLOCS as u32 * 24) as i32;

/// The offsets of the runtime services within .text, plus the labels
/// the services reference (bound by the caller).
#[derive(Debug, Clone)]
pub(crate) struct RuntimeOffsets {
    services: HashMap<RuntimeService, u32>,
    /// Label bounding the start of the image's immutable string-data region.
    pub(crate) str_data_start_label: u32,
    /// Label bounding one past the end of that region.
    pub(crate) str_data_end_label: u32,
}

impl RuntimeOffsets {
    pub(crate) fn of(&self, service: RuntimeService) -> u32 {
        *self
            .services
            .get(&service)
            .expect("runtime service not emitted")
    }
}

// ===========================================================================
// Helper functions
// ===========================================================================

/// Standard function prologue: push rbp; mov rbp, rsp
fn prologue(code: &mut Code) {
    code.u8(0x55); // push rbp
    code.bytes(&[0x48, 0x89, 0xE5]); // mov rbp, rsp
}

/// Emit `mov rcx, error_number; call rt_fail` — a fatal error path.
fn fail(code: &mut Code, number: u32) {
    code.mov_r32_imm32(Reg::Rcx, number);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Fail));
}

/// Helper: write `rcx`-pointed bytes of `rdx` length to stdout via syscall.
fn syscall_write_stdout(code: &mut Code) {
    code.mov_rdi_imm32(1); // fd = stdout
    // rsi and rdx must be set by caller
    code.movabs(Reg::Rax, SYS_WRITE);
    code.syscall();
}

/// Helper: write `rcx`-pointed bytes of `rdx` length to stderr via syscall.
fn syscall_write_stderr(code: &mut Code) {
    code.mov_rdi_imm32(2); // fd = stderr
    // rsi and rdx must be set by caller
    code.movabs(Reg::Rax, SYS_WRITE);
    code.syscall();
}

/// Helper: convert MINK Str (ptr in reg) to NUL-terminated C string via
/// RuntimeService::ToCstr. Returns the CStr pointer in RAX.
/// Clobbers RCX. Caller must have 16-byte aligned RSP.
fn to_cstr(code: &mut Code, str_reg: Reg) {
    if str_reg != Reg::Rax {
        code.mov_rr(Reg::Rax, str_reg);
    }
    code.sub_rsp(8); // padding for 1-arg call
    code.u8(0x50); // push rax (str ptr)
    code.call_patch(PatchKind::RuntimeService(RuntimeService::ToCstr));
    code.add_rsp(16); // pop arg + padding
}

/// Helper: free a CStr via RuntimeService::FreeCstr. Clobbers RAX.
fn free_cstr(code: &mut Code, cstr_reg: Reg) {
    if cstr_reg != Reg::Rax {
        code.mov_rr(Reg::Rax, cstr_reg);
    }
    code.sub_rsp(8); // padding
    code.u8(0x50); // push rax (cstr ptr)
    code.call_patch(PatchKind::RuntimeService(RuntimeService::FreeCstr));
    code.add_rsp(16); // pop arg + padding
}

// ===========================================================================
// Service: Init
// ===========================================================================

fn emit_init(code: &mut Code, offsets: &RuntimeOffsets) {
    prologue(code);
    // Save entry RSP to BSS (use RIP-relative addressing for BSS)
    code.mov_rip_r(Reg::Rsp, PatchKind::Bss(BSS.entry_rsp as u32));
    // Reset bump cursor and free list
    code.mov_rip_imm32(PatchKind::Bss(BSS.cursor as u32), 0);
    code.mov_rip_imm32(PatchKind::Bss(BSS.free_head as u32), 0);
    // Record immutable string-data bounds in BSS for StrValidate
    code.lea_r_rip(Reg::Rax, PatchKind::Label(offsets.str_data_start_label));
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(BSS.str_data_start as u32));
    code.lea_r_rip(Reg::Rax, PatchKind::Label(offsets.str_data_end_label));
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(BSS.str_data_end as u32));
    // Initialize RNG seed (non-zero for xorshift64*)
    code.movabs(Reg::Rax, 1u64);
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(BSS.rng_state as u32));
    code.leave_ret();
}

// ===========================================================================
// Service: Alloc (full liveness table scan)
// ===========================================================================

fn emit_alloc(code: &mut Code) {
    prologue(code);
    // Spill slot [rbp-8] holds the aligned size.
    code.sub_rsp(16);

    // Validate and align the size.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.test_rr(Reg::Rax, Reg::Rax);
    let bad_size = code.label();
    code.jcc_label(0x8E, bad_size); // jle
    code.add_r_imm8(Reg::Rax, 15);
    code.and_r_imm8(Reg::Rax, 0xF0); // align up to 16
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);

    // Reuse the most recently freed block when the free list is nonempty
    // AND the freed block is large enough for the new allocation.
    let bump = code.label();
    let record = code.label();
    let too_small = code.label();
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(BSS.free_head as u32));
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x84, bump); // jz (empty free list)
    // Read the saved size from [block+8] and compare with needed size.
    code.mov_r_mem(Reg::Rdx, Reg::Rax, 8); // Rdx = old_size
    code.cmp_r_mem(Reg::Rdx, Reg::Rbp, -8); // compare old_size vs needed
    code.jcc_label(0x8C, too_small); // jl (freed block too small)
    // Block is large enough — pop and reuse.
    code.mov_r_mem(Reg::Rcx, Reg::Rax, 0); // next = [block]
    code.mov_rip_r(Reg::Rcx, PatchKind::Bss(BSS.free_head as u32));
    code.jmp_label(record);
    // Freed block too small — fall through to bump allocation.
    code.bind_label(too_small);

    // Otherwise bump the cursor within the arena bounds.
    code.bind_label(bump);
    code.mov_r_rip(Reg::Rcx, PatchKind::Bss(BSS.cursor as u32));
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, -8);
    code.mov_rr(Reg::Rax, Reg::Rcx);
    code.add_rr(Reg::Rax, Reg::Rdx); // new offset
    code.cmp_r_imm32(Reg::Rax, HEAP_SIZE as u32);
    let oom = code.label();
    code.jcc_label(0x87, oom); // ja
    code.mov_rr(Reg::Rax, Reg::Rcx); // block offset
    code.lea_r_rip(Reg::Rcx, PatchKind::Bss(BSS.arena as u32));
    code.add_rr(Reg::Rax, Reg::Rcx); // absolute block address
    // Update cursor
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, -8);
    code.mov_rr(Reg::Rcx, Reg::Rax);
    // Recompute new cursor = block_offset + size
    code.lea_r_rip(Reg::R10, PatchKind::Bss(BSS.arena as u32));
    code.sub_rr(Reg::Rax, Reg::R10); // back to offset
    code.add_rr(Reg::Rax, Reg::Rdx); // offset + size
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(BSS.cursor as u32));
    code.mov_rr(Reg::Rax, Reg::Rcx); // restore block addr

    // Record the allocation in the first dead table slot.
    code.bind_label(record);
    let scan = code.label();
    let found = code.label();
    let table_full = code.label();
    code.lea_r_rip(Reg::Rcx, PatchKind::Bss(BSS.table as u32));
    code.lea_r_mem(Reg::Rdx, Reg::Rcx, TABLE_BYTES);
    code.bind_label(scan);
    code.cmp_rr(Reg::Rcx, Reg::Rdx);
    code.jcc_label(0x83, table_full); // jae
    code.cmp_mem_imm8(Reg::Rcx, 16, 0);
    code.jcc_label(0x84, found); // je
    code.add_r_imm8(Reg::Rcx, 24);
    code.jmp_label(scan);
    code.bind_label(found);
    code.mov_mem_r(Reg::Rcx, 0, Reg::Rax);
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, -8);
    code.mov_mem_r(Reg::Rcx, 8, Reg::Rdx);
    code.mov_mem_imm32(Reg::Rcx, 16, 1);
    code.leave_ret();

    code.bind_label(bad_size);
    fail(code, 8); // E-R08
    code.bind_label(oom);
    fail(code, 2); // E-R02
    code.bind_label(table_full);
    fail(code, 3); // E-R03
}

// ===========================================================================
// Service: Free (full liveness table scan)
// ===========================================================================

fn emit_free(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);

    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.test_rr(Reg::Rax, Reg::Rax);
    let invalid = code.label();
    code.jcc_label(0x84, invalid); // jz (null free)
    code.test_r_imm32(Reg::Rax, 15);
    let misaligned = code.label();
    code.jcc_label(0x85, misaligned); // jnz

    // Find the live table slot whose start equals the pointer.
    let scan = code.label();
    let next = code.label();
    code.lea_r_rip(Reg::Rcx, PatchKind::Bss(BSS.table as u32));
    code.lea_r_mem(Reg::Rdx, Reg::Rcx, TABLE_BYTES);
    code.bind_label(scan);
    code.cmp_rr(Reg::Rcx, Reg::Rdx);
    code.jcc_label(0x83, invalid); // jae
    code.cmp_r_mem(Reg::Rax, Reg::Rcx, 0);
    code.jcc_label(0x85, next); // jne
    code.cmp_mem_imm8(Reg::Rcx, 16, 0);
    code.jcc_label(0x84, invalid); // je (dead: double free)
    // Save the block's original size at [block+8] for the free-list
    // allocator to check on reuse.
    code.mov_r_mem(Reg::Rdx, Reg::Rcx, 8); // Rdx = slot.size
    code.mov_mem_r(Reg::Rax, 8, Reg::Rdx); // [block+8] = size

    // Mark dead and push onto the LIFO free list.
    code.mov_mem_imm32(Reg::Rcx, 16, 0);
    code.mov_r_rip(Reg::Rdx, PatchKind::Bss(BSS.free_head as u32));
    code.mov_mem_r(Reg::Rax, 0, Reg::Rdx);
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(BSS.free_head as u32));
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();

    code.bind_label(next);
    code.add_r_imm8(Reg::Rcx, 24);
    code.jmp_label(scan);

    code.bind_label(misaligned);
    fail(code, 7); // E-R07
    code.bind_label(invalid);
    fail(code, 4); // E-R04
}

// ===========================================================================
// Service: MemLoad (full validation)
// ===========================================================================

fn emit_memload(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);

    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.test_r_imm32(Reg::Rax, 7);
    let misaligned = code.label();
    code.jcc_label(0x85, misaligned); // jnz

    let scan = code.label();
    let next = code.label();
    let oob = code.label();
    code.lea_r_rip(Reg::Rcx, PatchKind::Bss(BSS.table as u32));
    code.lea_r_mem(Reg::Rdx, Reg::Rcx, TABLE_BYTES);
    code.bind_label(scan);
    code.cmp_rr(Reg::Rcx, Reg::Rdx);
    code.jcc_label(0x83, oob); // jae
    code.cmp_mem_imm8(Reg::Rcx, 16, 0);
    code.jcc_label(0x84, next); // je — dead entry
    code.mov_r_mem(Reg::R8, Reg::Rcx, 0); // start
    code.cmp_rr(Reg::Rax, Reg::R8);
    code.jcc_label(0x82, next); // jb
    code.mov_r_mem(Reg::R9, Reg::Rcx, 8); // size
    code.add_rr(Reg::R9, Reg::R8); // end = start + size
    code.cmp_rr(Reg::Rax, Reg::R9);
    code.jcc_label(0x83, next); // jae
    code.mov_rr(Reg::R10, Reg::Rax);
    code.add_r_imm8(Reg::R10, 8);
    code.cmp_rr(Reg::R10, Reg::R9);
    code.jcc_label(0x87, oob); // ja
    code.mov_r_mem(Reg::Rax, Reg::Rax, 0);
    code.leave_ret();

    code.bind_label(next);
    code.add_r_imm8(Reg::Rcx, 24);
    code.jmp_label(scan);

    code.bind_label(misaligned);
    fail(code, 7); // E-R07
    code.bind_label(oob);
    fail(code, 5); // E-R05
}

// ===========================================================================
// Service: MemStore (full validation)
// ===========================================================================

fn emit_memstore(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);

    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, 24);
    code.test_r_imm32(Reg::Rax, 7);
    let misaligned = code.label();
    code.jcc_label(0x85, misaligned); // jnz

    let scan = code.label();
    let next = code.label();
    let oob = code.label();
    code.lea_r_rip(Reg::Rdx, PatchKind::Bss(BSS.table as u32));
    code.lea_r_mem(Reg::R8, Reg::Rdx, TABLE_BYTES);
    code.bind_label(scan);
    code.cmp_rr(Reg::Rdx, Reg::R8);
    code.jcc_label(0x83, oob); // jae
    code.cmp_mem_imm8(Reg::Rdx, 16, 0);
    code.jcc_label(0x84, next); // je — dead entry
    code.mov_r_mem(Reg::R9, Reg::Rdx, 0); // start
    code.cmp_rr(Reg::Rax, Reg::R9);
    code.jcc_label(0x82, next); // jb
    code.mov_r_mem(Reg::R10, Reg::Rdx, 8); // size
    code.add_rr(Reg::R10, Reg::R9); // end
    code.cmp_rr(Reg::Rax, Reg::R10);
    code.jcc_label(0x83, next); // jae
    code.mov_rr(Reg::R11, Reg::Rax); // r11 = addr + 8
    code.add_r_imm8(Reg::R11, 8);
    code.cmp_rr(Reg::R11, Reg::R10);
    code.jcc_label(0x87, oob); // ja
    code.mov_mem_r(Reg::Rax, 0, Reg::Rcx);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();

    code.bind_label(next);
    code.add_r_imm8(Reg::Rdx, 24);
    code.jmp_label(scan);

    code.bind_label(misaligned);
    fail(code, 7); // E-R07
    code.bind_label(oob);
    fail(code, 5); // E-R05
}

// ===========================================================================
// Service: Exit (leak check + sys_exit)
// ===========================================================================

fn emit_exit(code: &mut Code) {
    prologue(code);
    // Check for leaks in liveness table
    let scan = code.label();
    let next = code.label();
    let no_leak = code.label();
    code.lea_r_rip(Reg::Rcx, PatchKind::Bss(BSS.table as u32));
    code.lea_r_mem(Reg::Rdx, Reg::Rcx, TABLE_BYTES);
    code.bind_label(scan);
    code.cmp_rr(Reg::Rcx, Reg::Rdx);
    code.jcc_label(0x83, no_leak); // jae — end of table, no leak
    code.cmp_mem_imm8(Reg::Rcx, 16, 0);
    code.jcc_label(0x84, next); // je — dead entry
    // Found a live allocation — leak!
    code.movabs(Reg::Rcx, RuntimeErrorKind::Leak as u64);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Fail));
    code.int3();
    code.bind_label(next);
    code.add_r_imm8(Reg::Rcx, 24);
    code.jmp_label(scan);

    code.bind_label(no_leak);
    // No leak — exit with code from [rbp+16]
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, 16);
    code.movabs(Reg::Rax, SYS_EXIT);
    code.syscall();
    code.int3();
}

// ===========================================================================
// Service: Fail (exit with 100 + error_number)
// ===========================================================================

fn emit_fail(code: &mut Code) {
    prologue(code);
    // Exit with 100 + error_number (in rcx)
    code.mov_r32_imm32(Reg::Rax, 100);
    code.add_rr(Reg::Rax, Reg::Rcx);
    code.mov_rdi_rax();
    code.movabs(Reg::Rax, SYS_EXIT);
    code.syscall();
    code.int3();
}

// ===========================================================================
// Service: StrAlloc (allocate length-prefixed string)
// ===========================================================================

fn emit_str_alloc(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16); // [rbp-8] holds the size
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.test_rr(Reg::Rax, Reg::Rax);
    let bad_size = code.label();
    code.jcc_label(0x8C, bad_size); // js
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);

    // Allocate 8 + size bytes through `rt_alloc`
    code.add_r_imm8(Reg::Rax, 8);
    code.sub_rsp(8);
    code.u8(0x50); // push rax (size)
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Alloc));
    code.add_rsp(16);

    // Write the length prefix: [rax] = size.
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, -8);
    code.mov_mem_r(Reg::Rax, 0, Reg::Rdx);
    code.leave_ret();

    code.bind_label(bad_size);
    fail(code, 8); // E-R08
}

// ===========================================================================
// Service: StrFree
// ===========================================================================

fn emit_str_free(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.sub_rsp(8);
    code.u8(0x50); // push rax
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Free));
    code.add_rsp(16);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

// ===========================================================================
// Service: StrLen
// ===========================================================================

fn emit_str_len(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrValidate));
    code.mov_r_mem(Reg::Rax, Reg::Rax, 0); // len = [s]
    code.leave_ret();
}

// ===========================================================================
// Service: StrByte
// ===========================================================================

fn emit_str_byte(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrValidate));
    code.mov_r_mem(Reg::R8, Reg::Rbp, 24); // index
    code.test_rr(Reg::R8, Reg::R8);
    let oob = code.label();
    code.jcc_label(0x8C, oob); // js
    code.mov_r_mem(Reg::R9, Reg::Rax, 0); // len
    code.cmp_rr(Reg::R8, Reg::R9);
    code.jcc_label(0x83, oob); // jae
    code.add_r_imm8(Reg::Rax, 8); // data base
    code.add_rr(Reg::Rax, Reg::R8); // data base + index
    code.movzx_byte(Reg::Rax, Reg::Rax, 0);
    code.leave_ret();

    code.bind_label(oob);
    fail(code, 9); // E-R09
}

// ===========================================================================
// Service: StrSetByte
// ===========================================================================

fn emit_str_set_byte(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrValidateHeap));
    code.mov_r_mem(Reg::R8, Reg::Rbp, 24); // index
    code.test_rr(Reg::R8, Reg::R8);
    let oob = code.label();
    code.jcc_label(0x8C, oob); // js
    code.mov_r_mem(Reg::R9, Reg::Rax, 0); // len
    code.cmp_rr(Reg::R8, Reg::R9);
    code.jcc_label(0x83, oob); // jae
    code.add_r_imm8(Reg::Rax, 8); // data base
    code.add_rr(Reg::Rax, Reg::R8); // data base + index
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, 32); // value
    code.mov_mem_r8(Reg::Rax, 0, Reg::Rcx);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();

    code.bind_label(oob);
    fail(code, 9); // E-R09
}

// ===========================================================================
// Service: StrValidate (internal: validate string pointer)
// Accepts a live heap-block start OR a pointer into the image's immutable
// string-data region.
// ===========================================================================

fn emit_str_validate(code: &mut Code, heap_only: bool) {
    prologue(code);
    code.sub_rsp(16);

    // Scan the liveness table for a live entry whose start equals s.
    let scan = code.label();
    let next = code.label();
    let not_heap = code.label();
    let not_heap_next = code.label();
    let fail5 = code.label();
    code.lea_r_rip(Reg::Rcx, PatchKind::Bss(BSS.table as u32));
    code.lea_r_mem(Reg::Rdx, Reg::Rcx, TABLE_BYTES);
    code.bind_label(scan);
    code.cmp_rr(Reg::Rcx, Reg::Rdx);
    code.jcc_label(0x83, not_heap); // jae
    code.cmp_mem_imm8(Reg::Rcx, 16, 0);
    code.jcc_label(0x84, next); // je — dead entry
    code.cmp_r_mem(Reg::Rax, Reg::Rcx, 0);
    code.jcc_label(0x85, next); // jne — different start
    code.leave_ret(); // a live heap string

    code.bind_label(next);
    code.add_r_imm8(Reg::Rcx, 24);
    code.jmp_label(scan);

    // Not a heap block: only the image's immutable string-data region is
    // a valid home for a literal string (and only for reads).
    code.bind_label(not_heap);
    if heap_only {
        code.jmp_label(fail5);
    } else {
        // Check BSS stdout/stderr capture buffers
        code.lea_r_rip(Reg::Rcx, PatchKind::Bss(BSS.stdout_buf as u32));
        code.cmp_rr(Reg::Rax, Reg::Rcx);
        code.jcc_label(0x84, not_heap_next); // je — valid stdout_buf
        code.lea_r_rip(Reg::Rcx, PatchKind::Bss(BSS.stderr_buf as u32));
        code.cmp_rr(Reg::Rax, Reg::Rcx);
        code.jcc_label(0x84, not_heap_next); // je — valid stderr_buf
        // Check image immutable string-data region
        code.mov_r_rip(Reg::Rcx, PatchKind::Bss(BSS.str_data_start as u32));
        code.mov_r_rip(Reg::Rdx, PatchKind::Bss(BSS.str_data_end as u32));
        code.cmp_rr(Reg::Rax, Reg::Rcx);
        code.jcc_label(0x82, fail5); // jb
        code.cmp_rr(Reg::Rax, Reg::Rdx);
        code.jcc_label(0x83, fail5); // jae
        code.bind_label(not_heap_next);
        code.leave_ret();
    }

    code.bind_label(fail5);
    fail(code, 5); // E-R05
}

// ===========================================================================
// Service: StrConcat
// ===========================================================================

fn emit_str_concat(code: &mut Code) {
    prologue(code);
    code.sub_rsp(40);

    // Validate and save a.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrValidate));
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // save a
    code.mov_r_mem(Reg::R8, Reg::Rax, 0); // len_a
    code.mov_mem_r(Reg::Rbp, -24, Reg::R8); // save len_a

    // Validate and save b.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 24);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrValidate));
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax); // save b
    code.mov_r_mem(Reg::R9, Reg::Rax, 0); // len_b
    code.mov_mem_r(Reg::Rbp, -32, Reg::R9); // save len_b

    // total = len_a + len_b
    code.add_rr(Reg::R8, Reg::R9); // R8 = total

    // Allocate 8 + total bytes.
    code.mov_rr(Reg::Rax, Reg::R8);
    code.add_r_imm8(Reg::Rax, 8);
    code.sub_rsp(8);
    code.u8(0x50); // push rax
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Alloc));
    code.add_rsp(16);
    code.mov_mem_r(Reg::Rbp, -40, Reg::Rax); // save new data ptr

    // Write length prefix: [rax] = total.
    code.mov_r_mem(Reg::R8, Reg::Rbp, -24);
    code.mov_r_mem(Reg::R9, Reg::Rbp, -32);
    code.add_rr(Reg::R8, Reg::R9);
    code.mov_mem_r(Reg::Rax, 0, Reg::R8);

    // Copy a bytes: dst = new+8, src = a+8, count = len_a.
    code.mov_r_mem(Reg::R8, Reg::Rbp, -24); // len_a
    code.test_rr(Reg::R8, Reg::R8);
    let copy_b_setup = code.label();
    code.jcc_label(0x84, copy_b_setup); // je (len_a == 0)
    code.mov_r32_imm32(Reg::R9, 0); // i = 0
    let copy_a = code.label();
    code.bind_label(copy_a);
    code.mov_r_mem(Reg::R10, Reg::Rbp, -8); // a ptr
    code.add_r_imm8(Reg::R10, 8); // a+8
    code.add_rr(Reg::R10, Reg::R9); // a+8+i
    code.movzx_byte(Reg::R10, Reg::R10, 0); // byte from a
    code.mov_r_mem(Reg::R11, Reg::Rbp, -40); // new ptr
    code.add_r_imm8(Reg::R11, 8); // new+8
    code.add_rr(Reg::R11, Reg::R9); // new+8+i
    code.mov_mem_r8(Reg::R11, 0, Reg::R10); // store byte
    code.add_r_imm8(Reg::R9, 1); // i++
    code.cmp_rr(Reg::R9, Reg::R8); // i < len_a?
    code.jcc_label(0x8C, copy_a); // jl

    // Copy b bytes: dst = new+8+len_a, src = b+8, count = len_b.
    code.bind_label(copy_b_setup);
    code.mov_r_mem(Reg::R8, Reg::Rbp, -32); // len_b
    code.test_rr(Reg::R8, Reg::R8);
    let done = code.label();
    code.jcc_label(0x84, done); // je (len_b == 0)
    code.mov_r32_imm32(Reg::R9, 0); // i = 0
    let copy_b = code.label();
    code.bind_label(copy_b);
    code.mov_r_mem(Reg::R10, Reg::Rbp, -16); // b ptr
    code.add_r_imm8(Reg::R10, 8); // b+8
    code.add_rr(Reg::R10, Reg::R9); // b+8+i
    code.movzx_byte(Reg::R10, Reg::R10, 0); // byte from b
    code.mov_r_mem(Reg::R11, Reg::Rbp, -40); // new ptr
    code.add_r_imm8(Reg::R11, 8); // new+8
    code.mov_r_mem(Reg::R12, Reg::Rbp, -24); // len_a
    code.add_rr(Reg::R11, Reg::R12); // new+8+len_a
    code.add_rr(Reg::R11, Reg::R9); // new+8+len_a+i
    code.mov_mem_r8(Reg::R11, 0, Reg::R10); // store byte
    code.add_r_imm8(Reg::R9, 1); // i++
    code.cmp_rr(Reg::R9, Reg::R8); // i < len_b?
    code.jcc_label(0x8C, copy_b); // jl

    code.bind_label(done);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -40); // return new data ptr
    code.leave_ret();
}

// ===========================================================================
// Service: StrEq
// ===========================================================================

fn emit_str_eq(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);

    // Validate a.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrValidate));
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // save a data ptr
    code.mov_r_mem(Reg::R8, Reg::Rax, 0); // len_a

    // Validate b.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 24);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrValidate));
    code.mov_r_mem(Reg::R9, Reg::Rax, 0); // len_b

    // Compare lengths.
    code.cmp_rr(Reg::R8, Reg::R9);
    let not_equal = code.label();
    code.jcc_label(0x85, not_equal); // jne

    // len_a == 0 means both are empty -> equal.
    code.test_rr(Reg::R8, Reg::R8);
    let equal = code.label();
    code.jcc_label(0x84, equal); // je

    // Byte-by-byte comparison.
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    code.add_r_imm8(Reg::Rcx, 8); // a+8
    code.lea_r_mem(Reg::Rdx, Reg::Rax, 8); // b+8
    let cmp_loop = code.label();
    code.bind_label(cmp_loop);
    code.movzx_byte(Reg::R10, Reg::Rcx, 0);
    code.movzx_byte(Reg::R11, Reg::Rdx, 0);
    code.cmp_rr(Reg::R10, Reg::R11);
    code.jcc_label(0x85, not_equal); // jne
    code.add_r_imm8(Reg::Rcx, 1);
    code.add_r_imm8(Reg::Rdx, 1);
    code.dec_r(Reg::R8);
    code.jcc_label(0x85, cmp_loop); // jnz

    code.bind_label(equal);
    code.add_rsp(8);
    code.mov_r32_imm32(Reg::Rax, 1); // true
    code.leave_ret();

    code.bind_label(not_equal);
    code.add_rsp(8);
    code.xor_rr32(Reg::Rax, Reg::Rax); // false
    code.leave_ret();
}

// ===========================================================================
// Service: StrFromInt
// ===========================================================================

fn emit_str_from_int(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);

    // rax = value.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);

    // Cursor starts at print_buf + 31.
    code.lea_r_rip(Reg::R8, PatchKind::Bss(BSS.print_buf as u32));
    code.add_r_imm8(Reg::R8, 31);

    // Handle zero: write '0'.
    code.test_rr(Reg::Rax, Reg::Rax);
    let non_zero = code.label();
    code.jcc_label(0x85, non_zero); // jnz
    code.mov_mem_imm8(Reg::R8, 0, b'0');
    code.dec_r(Reg::R8);
    let build_done = code.label();
    code.jmp_label(build_done);

    code.bind_label(non_zero);
    code.test_rr(Reg::Rax, Reg::Rax);
    let digit_loop = code.label();
    code.jcc_label(0x89, digit_loop); // jns
    code.neg_r(Reg::Rax);

    code.bind_label(digit_loop);
    code.xor_rr32(Reg::Rdx, Reg::Rdx);
    code.mov_r32_imm32(Reg::Rcx, 10);
    code.div_r(Reg::Rcx); // RAX=quot, RDX=rem
    code.add_r_imm8(Reg::Rdx, b'0');
    code.mov_mem_r8(Reg::R8, 0, Reg::Rdx);
    code.dec_r(Reg::R8);
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x85, digit_loop); // jnz

    // Write '-' for negative values.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.test_rr(Reg::Rax, Reg::Rax);
    let after_sign = code.label();
    code.jcc_label(0x89, after_sign); // jns
    code.mov_mem_imm8(Reg::R8, 0, b'-');
    code.dec_r(Reg::R8);
    code.bind_label(after_sign);

    code.bind_label(build_done);
    // R8 points to one byte before the first character.
    code.lea_r_mem(Reg::R9, Reg::R8, 1); // data start
    code.lea_r_rip(Reg::R10, PatchKind::Bss(BSS.print_buf as u32));
    code.add_r_imm8(Reg::R10, 31);
    code.sub_rr(Reg::R10, Reg::R8); // length

    // Save length and data start before alloc.
    code.mov_mem_r(Reg::Rbp, -8, Reg::R10);
    code.mov_mem_r(Reg::Rbp, -16, Reg::R9);

    // Allocate 8 + length bytes.
    code.mov_rr(Reg::Rax, Reg::R10);
    code.add_r_imm8(Reg::Rax, 8);
    code.sub_rsp(8);
    code.u8(0x50); // push rax
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Alloc));
    code.add_rsp(16);

    // Reload.
    code.mov_r_mem(Reg::R10, Reg::Rbp, -8);
    code.mov_r_mem(Reg::R9, Reg::Rbp, -16);

    // Write length prefix.
    code.mov_mem_r(Reg::Rax, 0, Reg::R10);

    // Save block start for return.
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax);

    // Copy digits.
    code.add_r_imm8(Reg::Rax, 8);
    code.mov_r32_imm32(Reg::R11, 0); // i
    let copy_loop = code.label();
    code.bind_label(copy_loop);
    code.cmp_rr(Reg::R11, Reg::R10);
    let copy_done = code.label();
    code.jcc_label(0x8D, copy_done); // jge
    code.movzx_byte(Reg::R12, Reg::R9, 0);
    code.mov_mem_r8(Reg::Rax, 0, Reg::R12);
    code.add_r_imm8(Reg::R9, 1);
    code.add_r_imm8(Reg::Rax, 1);
    code.add_r_imm8(Reg::R11, 1);
    code.jmp_label(copy_loop);

    code.bind_label(copy_done);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16);
    code.leave_ret();
}

// ===========================================================================
// Service: StrFromBool
// ===========================================================================

fn emit_str_from_bool(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);

    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.test_rr(Reg::Rax, Reg::Rax);
    let emit_false = code.label();
    code.jcc_label(0x84, emit_false); // je

    // true: allocate 4 bytes.
    code.sub_rsp(8);
    code.mov_r32_imm32(Reg::Rax, 4);
    code.u8(0x50);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);
    code.mov_mem_imm8(Reg::Rax, 8, b't');
    code.mov_mem_imm8(Reg::Rax, 9, b'r');
    code.mov_mem_imm8(Reg::Rax, 10, b'u');
    code.mov_mem_imm8(Reg::Rax, 11, b'e');
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.add_rsp(8);
    code.leave_ret();

    code.bind_label(emit_false);
    // false: allocate 5 bytes.
    code.sub_rsp(8);
    code.mov_r32_imm32(Reg::Rax, 5);
    code.u8(0x50);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);
    code.mov_mem_imm8(Reg::Rax, 8, b'f');
    code.mov_mem_imm8(Reg::Rax, 9, b'a');
    code.mov_mem_imm8(Reg::Rax, 10, b'l');
    code.mov_mem_imm8(Reg::Rax, 11, b's');
    code.mov_mem_imm8(Reg::Rax, 12, b'e');
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.add_rsp(8);
    code.leave_ret();
}

// ===========================================================================
// Service: PrintStr
// ===========================================================================

fn emit_print_str(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);

    // str ptr at [rbp+16]
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.test_rr(Reg::Rax, Reg::Rax);
    let done = code.label();
    code.jcc_label(0x84, done); // jz — null

    // Load length from [str_ptr]
    code.mov_r_mem(Reg::Rdx, Reg::Rax, 0); // len
    code.add_r_imm8(Reg::Rax, 8); // data = str_ptr + 8

    // write(1, data, len)
    code.mov_rdi_imm32(1);
    code.mov_rsi_rax();
    code.movabs(Reg::Rax, SYS_WRITE);
    code.syscall();

    // Print newline
    code.mov_r32_imm32(Reg::R10, 10);
    code.mov_mem_r(Reg::Rsp, 0, Reg::R10);
    code.mov_rdi_imm32(1);
    code.mov_rsi_rsp();
    code.mov_r32_imm32(Reg::Rdx, 1);
    code.movabs(Reg::Rax, SYS_WRITE);
    code.syscall();

    code.bind_label(done);
    code.leave_ret();
}

// ===========================================================================
// Service: PrintInt
// ===========================================================================

fn emit_print_int(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // save value

    // Build digits from end of print_buf downward (same as Windows).
    code.lea_r_rip(Reg::R8, PatchKind::Bss(BSS.print_buf as u32));
    code.add_r_imm8(Reg::R8, 31);

    // Handle zero
    code.test_rr(Reg::Rax, Reg::Rax);
    let non_zero = code.label();
    code.jcc_label(0x85, non_zero);
    code.mov_mem_imm8(Reg::R8, 0, b'0');
    code.dec_r(Reg::R8);
    let build_done = code.label();
    code.jmp_label(build_done);

    code.bind_label(non_zero);
    // Handle negative
    code.test_rr(Reg::Rax, Reg::Rax);
    let digit_loop = code.label();
    code.jcc_label(0x89, digit_loop); // jns
    code.neg_r(Reg::Rax);

    code.bind_label(digit_loop);
    code.xor_rr32(Reg::Rdx, Reg::Rdx);
    code.mov_r32_imm32(Reg::Rcx, 10);
    code.div_r(Reg::Rcx);
    code.add_r_imm8(Reg::Rdx, b'0');
    code.mov_mem_r8(Reg::R8, 0, Reg::Rdx);
    code.dec_r(Reg::R8);
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x85, digit_loop);

    // Handle negative sign
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.test_rr(Reg::Rax, Reg::Rax);
    let after_sign = code.label();
    code.jcc_label(0x89, after_sign);
    code.mov_mem_imm8(Reg::R8, 0, b'-');
    code.dec_r(Reg::R8);
    code.bind_label(after_sign);

    code.bind_label(build_done);
    // R8 points to one byte before the first character.
    // Write from R8+1 to print_buf+31.
    code.lea_r_mem(Reg::Rcx, Reg::R8, 1); // start of digits
    code.lea_r_rip(Reg::Rax, PatchKind::Bss(BSS.print_buf as u32));
    code.add_r_imm8(Reg::Rax, 32);
    code.sub_rr(Reg::Rax, Reg::Rcx); // length
    code.mov_rr(Reg::Rdx, Reg::Rax);
    code.mov_rdi_imm32(1);
    code.mov_rr(Reg::Rsi, Reg::Rcx);
    code.movabs(Reg::Rax, SYS_WRITE);
    code.syscall();

    code.leave_ret();
}

// ===========================================================================
// Service: PrintFloat (placeholder: prints "0.0")
// ===========================================================================

fn emit_print_float(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);
    code.movabs(Reg::R10, 0x000000002E3030);
    code.mov_mem_r(Reg::Rsp, 0, Reg::R10);
    code.mov_rdi_imm32(1);
    code.mov_rsi_rsp();
    code.mov_r32_imm32(Reg::Rdx, 3);
    code.movabs(Reg::Rax, SYS_WRITE);
    code.syscall();
    code.add_rsp(16);
    code.leave_ret();
}

// ===========================================================================
// Service: PrintChar
// ===========================================================================

fn emit_print_char(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_mem_r(Reg::Rsp, 0, Reg::Rax);
    code.mov_rdi_imm32(1);
    code.mov_rsi_rsp();
    code.mov_r32_imm32(Reg::Rdx, 1);
    code.movabs(Reg::Rax, SYS_WRITE);
    code.syscall();
    code.add_rsp(16);
    code.leave_ret();
}

// ===========================================================================
// Service: IntToFloat
// ===========================================================================

fn emit_int_to_float(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.sub_rsp(8);
    code.mov_mem_r(Reg::Rsp, 0, Reg::Rax);
    // cvtsi2sd xmm0, [rsp]
    code.bytes(&[0xF2, 0x48, 0x0F, 0x2A, 0x04, 0x24]);
    // movsd [rsp], xmm0
    code.bytes(&[0xF2, 0x0F, 0x11, 0x04, 0x24]);
    code.mov_r_mem(Reg::Rax, Reg::Rsp, 0);
    code.add_rsp(8);
    code.leave_ret();
}

// ===========================================================================
// Service: FloatToInt
// ===========================================================================

fn emit_float_to_int(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.sub_rsp(8);
    code.mov_mem_r(Reg::Rsp, 0, Reg::Rax);
    // movsd xmm0, [rsp]
    code.bytes(&[0xF2, 0x0F, 0x10, 0x04, 0x24]);
    code.add_rsp(8);
    // cvttsd2si rax, xmm0
    code.bytes(&[0xF2, 0x48, 0x0F, 0x2C, 0xC0]);
    code.leave_ret();
}

// ===========================================================================
// Service: WriteStdout (internal)
// ===========================================================================

fn emit_write_stdout(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, 16); // buf
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, 24); // len
    code.mov_rdi_imm32(1); // fd = stdout (overwrites buf, but rdi was loaded first)
    // Actually: rdi = buf (arg0), then rsi = buf, rdx = len
    // But WriteStdout takes (buf, len) via private convention: rcx=buf, rdx=len
    // Looking at the Windows version: it uses rcx=buf, rdx=len (private convention)
    // On Linux, we need: rdi=fd, rsi=buf, rdx=len
    // So: first load buf into rsi, len into rdx, then fd into rdi
    code.mov_r_mem(Reg::Rsi, Reg::Rbp, 16); // buf
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, 24); // len
    code.mov_rdi_imm32(1); // fd = stdout
    code.movabs(Reg::Rax, SYS_WRITE);
    code.syscall();
    code.leave_ret();
}

// ===========================================================================
// Service: WriteStderr (internal)
// ===========================================================================

fn emit_write_stderr(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rsi, Reg::Rbp, 16); // buf
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, 24); // len
    code.mov_rdi_imm32(2); // fd = stderr
    code.movabs(Reg::Rax, SYS_WRITE);
    code.syscall();
    code.leave_ret();
}

// ===========================================================================
// Service: ToCstr (allocate NUL-terminated copy)
// ===========================================================================

fn emit_to_cstr(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);
    code.mov_r_mem(Reg::R10, Reg::Rbp, 16); // s ptr
    code.mov_r_mem(Reg::Rcx, Reg::R10, 0); // len
    code.add_r_imm8(Reg::Rcx, 1); // len + 1 for NUL
    code.mov_rr(Reg::Rax, Reg::Rcx);
    code.sub_rsp(8);
    code.u8(0x50); // push rax (size)
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Alloc));
    code.add_rsp(16);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // save buf

    // Copy loop
    code.mov_r_mem(Reg::R10, Reg::Rbp, 16); // s ptr
    code.mov_r_mem(Reg::R11, Reg::R10, 0); // len
    code.mov_r_mem(Reg::R9, Reg::Rbp, -8); // buf
    code.mov_rr(Reg::R10, Reg::Rax); // R10 = buf cursor
    let loop_start = code.label();
    let loop_done = code.label();
    code.bind_label(loop_start);
    code.test_rr(Reg::R11, Reg::R11);
    code.jcc_label(0x84, loop_done); // jz done
    // R8 = s+8 (data start)
    code.mov_r_mem(Reg::R8, Reg::Rbp, 16);
    code.add_r_imm8(Reg::R8, 8);
    // Compute offset = original_len - R11
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, 16);
    code.mov_r_mem(Reg::Rdx, Reg::Rdx, 0); // original len
    code.mov_rr(Reg::Rax, Reg::Rdx);
    code.sub_rr(Reg::Rax, Reg::R11); // offset
    // Load byte from s[8+offset]
    code.add_rr(Reg::R8, Reg::Rax);
    code.movzx_byte(Reg::Rdx, Reg::R8, 0);
    // Store byte to buf[offset]
    code.mov_r_mem(Reg::R9, Reg::Rbp, -8);
    code.add_rr(Reg::R9, Reg::Rax);
    code.mov_mem_r8(Reg::R9, 0, Reg::Rdx);
    code.sub_r_imm32(Reg::R11, 1);
    code.jmp_label(loop_start);
    code.bind_label(loop_done);
    // Null terminate
    code.mov_r_mem(Reg::R9, Reg::Rbp, -8);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_r_mem(Reg::Rax, Reg::Rax, 0); // len
    code.add_rr(Reg::R9, Reg::Rax);
    code.mov_mem_imm32(Reg::R9, 0, 0); // buf[len] = 0
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.leave_ret();
}

// ===========================================================================
// Service: FreeCstr
// ===========================================================================

fn emit_free_cstr(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.u8(0x50);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Free));
    code.add_rsp(8);
    code.leave_ret();
}

// ===========================================================================
// Service: FsExists (access F_OK)
// ===========================================================================

fn emit_fs_exists(code: &mut Code) {
    prologue(code);
    // [rbp+16] = path Str ptr
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    to_cstr(code, Reg::Rax);
    // Spill: [rbp-8]=cstr
    code.sub_rsp(32);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);

    // access(cstr, F_OK)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -8); // path
    code.movabs(Reg::Rsi, F_OK as u64); // mode
    code.movabs(Reg::Rax, SYS_ACCESS);
    code.syscall();

    // rax = 0 means exists, negative means doesn't
    code.test_rr(Reg::Rax, Reg::Rax);
    let not_found = code.label();
    code.jcc_label(0x85, not_found); // jnz (access returned non-zero)
    // Exists
    code.movabs(Reg::Rax, 1u64);
    let done = code.label();
    code.jmp_label(done);
    code.bind_label(not_found);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.bind_label(done);

    // Save return value before free_cstr clobbers RAX
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16);
    code.leave_ret();
}

// ===========================================================================
// Service: FsFileSize (stat + st_size, or -1 for dirs/errors)
// ===========================================================================

fn emit_fs_file_size(code: &mut Code) {
    prologue(code);
    // Allocate stat buffer on stack: 144 bytes (already 16-aligned)
    code.sub_rsp(STAT_BUF_SIZE as i32 + 16); // +16 for alignment/spill

    // [rbp+16] = path Str ptr
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    to_cstr(code, Reg::Rax);
    // [rbp-8] = cstr, [rbp-16..rbp-160] = stat buffer
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);

    // stat(cstr, &stat_buf)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -8); // path
    code.lea_r_mem(Reg::Rsi, Reg::Rbp, -160); // stat buffer
    code.movabs(Reg::Rax, SYS_STAT);
    code.syscall();

    // rax = 0 on success, negative on error
    code.test_rr(Reg::Rax, Reg::Rax);
    let err = code.label();
    code.jcc_label(0x85, err); // jnz (error)

    // Check if it's a regular file: (st_mode & S_IFMT) == S_IFREG
    code.mov_r32_mem(Reg::Rax, Reg::Rbp, -160 + STAT_MODE_OFFSET); // st_mode
    code.movabs(Reg::R10, S_IFMT);
    code.and_rr(Reg::Rax, Reg::R10); // mode & S_IFMT
    code.movabs(Reg::R10, S_IFREG);
    code.cmp_rr(Reg::Rax, Reg::R10);
    let not_reg = code.label();
    code.jcc_label(0x85, not_reg); // jne (not regular file → treat as error)

    // Return st_size
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -160 + STAT_SIZE_OFFSET);
    let done = code.label();
    code.jmp_label(done);

    code.bind_label(not_reg);
    code.bind_label(err);
    // Return -1 (error)
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.bind_label(done);

    // Save return value
    code.mov_mem_r(Reg::Rbp, -168, Reg::Rax);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -168);
    code.leave_ret();
}

// ===========================================================================
// Service: FsRead (open + fstat + read + close)
// ===========================================================================

fn emit_fs_read(code: &mut Code) {
    prologue(code);
    // Allocate stat buffer on stack
    code.sub_rsp(STAT_BUF_SIZE as i32 + 32); // stat + spill

    // [rbp+16] = path Str ptr
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    to_cstr(code, Reg::Rax);
    // [rbp-8]=cstr, [rbp-16]=fd, [rbp-24]=result, [rbp-32..-176]=stat
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);

    // open(cstr, O_RDONLY, 0)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -8);
    code.movabs(Reg::Rsi, O_RDONLY as u64);
    code.xor_rr32(Reg::Rdx, Reg::Rdx);
    code.movabs(Reg::Rax, SYS_OPEN);
    code.syscall();

    // rax = fd (>= 0 on success, negative on error)
    code.test_rr(Reg::Rax, Reg::Rax);
    let err = code.label();
    code.jcc_label(0x8C, err); // js (negative = error)
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax); // save fd

    // fstat(fd, &stat_buf)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -16);
    code.lea_r_mem(Reg::Rsi, Reg::Rbp, -176);
    code.movabs(Reg::Rax, 5u64); // SYS_FSTAT
    code.syscall();

    // rax = 0 on success
    code.test_rr(Reg::Rax, Reg::Rax);
    let fstat_err = code.label();
    code.jcc_label(0x85, fstat_err); // jnz

    // Get file size from stat
    code.mov_r_mem(Reg::R10, Reg::Rbp, -176 + STAT_SIZE_OFFSET); // st_size
    code.mov_mem_r(Reg::Rbp, -24, Reg::R10); // save size

    // StrAlloc(size)
    code.mov_rr(Reg::Rax, Reg::R10);
    code.sub_rsp(8);
    code.u8(0x50);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.mov_mem_r(Reg::Rbp, -32, Reg::Rax); // save result ptr

    // read(fd, result+8, size)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -16); // fd
    code.mov_rr(Reg::Rsi, Reg::Rax);
    code.add_r_imm8(Reg::Rsi, 8); // buf = result + 8
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, -24); // count
    code.movabs(Reg::Rax, SYS_READ);
    code.syscall();

    // close(fd)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -16);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();

    // Return result
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -32);
    let done = code.label();
    code.jmp_label(done);

    // Error path: return empty string
    code.bind_label(fstat_err);
    // close fd first
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -16);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();

    code.bind_label(err);
    code.sub_rsp(8);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.u8(0x50);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.bind_label(done);

    // Save return value before free_cstr
    code.mov_mem_r(Reg::Rbp, -32, Reg::Rax);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -32);
    code.leave_ret();
}

// ===========================================================================
// Service: FsWrite (open + write + close)
// ===========================================================================

fn emit_fs_write(code: &mut Code) {
    prologue(code);
    code.sub_rsp(32); // [rbp-8]=cstr, [rbp-16]=fd, [rbp-24]=bytes_written

    // [rbp+16] = path, [rbp+24] = data
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    to_cstr(code, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // cstr

    // open(cstr, O_WRONLY|O_CREAT|O_TRUNC, 0644)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -8);
    code.movabs(Reg::Rsi, (O_WRONLY | O_CREAT | O_TRUNC) as u64);
    code.movabs(Reg::Rdx, 0o644u64); // mode
    code.movabs(Reg::Rax, SYS_OPEN);
    code.syscall();

    code.test_rr(Reg::Rax, Reg::Rax);
    let err = code.label();
    code.jcc_label(0x8C, err); // js
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax); // save fd

    // write(fd, data+8, [data])
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -16); // fd
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, 24); // data ptr
    code.mov_r_mem(Reg::R8, Reg::Rdx, 0); // data len
    code.add_r_imm8(Reg::Rdx, 8); // skip length prefix
    code.mov_rr(Reg::Rsi, Reg::Rdx); // buf = data + 8
    code.mov_rr(Reg::Rdx, Reg::R8); // count = data len
    code.movabs(Reg::Rax, SYS_WRITE);
    code.syscall();

    // rax = bytes written (or negative on error)
    code.mov_mem_r(Reg::Rbp, -24, Reg::Rax); // save bytes_written

    // close(fd)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -16);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();

    let done = code.label();
    code.jmp_label(done);

    // Error path
    code.bind_label(err);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.mov_mem_r(Reg::Rbp, -24, Reg::Rax);

    code.bind_label(done);
    // Free path CStr
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    // Return bytes_written
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -24);
    code.leave_ret();
}

// ===========================================================================
// Service: FsCreateDir (mkdir)
// ===========================================================================

fn emit_fs_create_dir(code: &mut Code) {
    prologue(code);
    code.sub_rsp(32);

    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    to_cstr(code, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // cstr

    // mkdir(cstr, 0755)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -8);
    code.movabs(Reg::Rsi, 0o755u64);
    code.movabs(Reg::Rax, SYS_MKDIR);
    code.syscall();

    // rax = 0 on success, negative on error
    code.test_rr(Reg::Rax, Reg::Rax);
    let is_ok = code.label();
    code.jcc_label(0x84, is_ok); // jz (success)
    code.mov_r32_imm32(Reg::Rax, 1); // error = 1
    let end = code.label();
    code.jmp_label(end);
    code.bind_label(is_ok);
    code.xor_rr32(Reg::Rax, Reg::Rax); // success = 0
    code.bind_label(end);

    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16);
    code.leave_ret();
}

// ===========================================================================
// Service: FsRemoveDir (rmdir)
// ===========================================================================

fn emit_fs_remove_dir(code: &mut Code) {
    prologue(code);
    code.sub_rsp(32);

    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    to_cstr(code, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);

    // rmdir(cstr)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -8);
    code.movabs(Reg::Rax, SYS_RMDIR);
    code.syscall();

    code.test_rr(Reg::Rax, Reg::Rax);
    let is_ok = code.label();
    code.jcc_label(0x84, is_ok);
    code.mov_r32_imm32(Reg::Rax, 1);
    let end = code.label();
    code.jmp_label(end);
    code.bind_label(is_ok);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.bind_label(end);

    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16);
    code.leave_ret();
}

// ===========================================================================
// Service: FsRemoveFile (unlink)
// ===========================================================================

fn emit_fs_remove_file(code: &mut Code) {
    prologue(code);
    code.sub_rsp(32);

    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    to_cstr(code, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);

    // unlink(cstr)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -8);
    code.movabs(Reg::Rax, SYS_UNLINK);
    code.syscall();

    code.test_rr(Reg::Rax, Reg::Rax);
    let is_ok = code.label();
    code.jcc_label(0x84, is_ok);
    code.mov_r32_imm32(Reg::Rax, 1);
    let end = code.label();
    code.jmp_label(end);
    code.bind_label(is_ok);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.bind_label(end);

    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16);
    code.leave_ret();
}

// ===========================================================================
// Service: FsCopy (open src + open dst + read/write loop + close both)
// ===========================================================================

fn emit_fs_copy(code: &mut Code) {
    prologue(code);
    code.sub_rsp(48); // [rbp-8]=src_cstr, [rbp-16]=dst_cstr, [rbp-24]=result,
    // [rbp-32]=src_fd, [rbp-40]=dst_fd

    // Convert src to CStr
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    to_cstr(code, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // src_cstr

    // Convert dst to CStr
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 24);
    to_cstr(code, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax); // dst_cstr

    // open(src, O_RDONLY, 0)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -8);
    code.movabs(Reg::Rsi, O_RDONLY as u64);
    code.xor_rr32(Reg::Rdx, Reg::Rdx);
    code.movabs(Reg::Rax, SYS_OPEN);
    code.syscall();
    code.test_rr(Reg::Rax, Reg::Rax);
    let err = code.label();
    code.jcc_label(0x8C, err); // js
    code.mov_mem_r(Reg::Rbp, -32, Reg::Rax); // src_fd

    // open(dst, O_WRONLY|O_CREAT|O_TRUNC, 0644)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -16);
    code.movabs(Reg::Rsi, (O_WRONLY | O_CREAT | O_TRUNC) as u64);
    code.movabs(Reg::Rdx, 0o644u64);
    code.movabs(Reg::Rax, SYS_OPEN);
    code.syscall();
    code.test_rr(Reg::Rax, Reg::Rax);
    let dst_err = code.label();
    code.jcc_label(0x8C, dst_err); // js
    code.mov_mem_r(Reg::Rbp, -40, Reg::Rax); // dst_fd

    // Simple copy: read up to 4096 bytes at a time from src, write to dst
    // We'll use BSS stdout_buf (4096 bytes) as our copy buffer
    let copy_loop = code.label();
    let copy_done = code.label();
    code.bind_label(copy_loop);

    // read(src_fd, stdout_buf, 4096)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -32); // src_fd
    code.lea_r_rip(Reg::Rsi, PatchKind::Bss(BSS.stdout_buf as u32)); // buf
    code.movabs(Reg::Rdx, 4096u64); // count
    code.movabs(Reg::Rax, SYS_READ);
    code.syscall();

    // rax = bytes read (0 = EOF, negative = error)
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x8E, copy_done); // jle (0=EOF, negative=error)

    // Save bytes_read
    code.mov_r_mem(Reg::R10, Reg::Rbp, -40); // dst_fd (was saved before write)

    // write(dst_fd, stdout_buf, bytes_read)
    code.mov_rdi_r10(); // dst_fd
    code.lea_r_rip(Reg::Rsi, PatchKind::Bss(BSS.stdout_buf as u32));
    code.movabs(Reg::Rdx, 0); // will be overwritten
    code.mov_rr(Reg::Rdx, Reg::Rax); // count = bytes_read
    code.movabs(Reg::Rax, SYS_WRITE);
    code.syscall();

    code.jmp_label(copy_loop); // continue copying

    code.bind_label(copy_done);
    // close both fds
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -32);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -40);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();

    // Success
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -24, Reg::Rax);
    let cleanup = code.label();
    code.jmp_label(cleanup);

    code.bind_label(dst_err);
    // Close src_fd
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -32);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();
    code.bind_label(err);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.mov_mem_r(Reg::Rbp, -24, Reg::Rax);

    code.bind_label(cleanup);
    // Free both CStrs
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -16);
    free_cstr(code, Reg::Rcx);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -24);
    code.leave_ret();
}

// ===========================================================================
// Service: FsMove (rename)
// ===========================================================================

fn emit_fs_move(code: &mut Code) {
    prologue(code);
    code.sub_rsp(48);

    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    to_cstr(code, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // src_cstr

    code.mov_r_mem(Reg::Rax, Reg::Rbp, 24);
    to_cstr(code, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax); // dst_cstr

    // rename(src, dst)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -8);
    code.mov_r_mem(Reg::Rsi, Reg::Rbp, -16);
    code.movabs(Reg::Rax, SYS_RENAME);
    code.syscall();

    code.test_rr(Reg::Rax, Reg::Rax);
    let is_ok = code.label();
    code.jcc_label(0x84, is_ok);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    let end = code.label();
    code.jmp_label(end);
    code.bind_label(is_ok);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.bind_label(end);
    code.mov_mem_r(Reg::Rbp, -24, Reg::Rax);

    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -16);
    free_cstr(code, Reg::Rcx);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -24);
    code.leave_ret();
}

// ===========================================================================
// Service: FsGetCwd
// ===========================================================================

fn emit_fs_get_cwd(code: &mut Code) {
    prologue(code);
    // Allocate stack buffer for getcwd (4096 bytes + 16 for alignment/spill)
    code.sub_rsp(4112); // [rbp-8] = result ptr, [rbp-16..-4112] = buf

    // getcwd(buf, 4096)
    code.lea_r_mem(Reg::Rdi, Reg::Rbp, -4112); // buf
    code.movabs(Reg::Rsi, 4096u64); // size
    code.movabs(Reg::Rax, SYS_GETCWD);
    code.syscall();

    // rax = buf on success, NULL (0) on error
    code.test_rr(Reg::Rax, Reg::Rax);
    let err = code.label();
    code.jcc_label(0x84, err); // jz (NULL = error)

    // Calculate strlen of the result
    // buf is at [rbp-4112], iterate until NUL
    code.lea_r_mem(Reg::R8, Reg::Rbp, -4112); // buf start
    code.mov_r32_imm32(Reg::R9, 0); // len
    let len_loop = code.label();
    let len_done = code.label();
    code.bind_label(len_loop);
    code.movzx_byte(Reg::R10, Reg::R8, 0);
    code.test_rr(Reg::R10, Reg::R10);
    code.jcc_label(0x84, len_done); // jz (found NUL)
    code.add_r_imm8(Reg::R8, 1);
    code.add_r_imm8(Reg::R9, 1);
    code.jmp_label(len_loop);
    code.bind_label(len_done);

    // StrAlloc(len)
    code.mov_rr(Reg::Rax, Reg::R9);
    code.sub_rsp(8);
    code.u8(0x50);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // save result ptr

    // Copy buf to result+8
    code.lea_r_mem(Reg::R8, Reg::Rbp, -4112); // src
    code.mov_rr(Reg::R9, Reg::Rax); // dst = result
    code.add_r_imm8(Reg::R9, 8);
    // Reload length from result's length prefix
    code.mov_r_mem(Reg::R10, Reg::Rax, 0); // len
    code.mov_r32_imm32(Reg::R11, 0); // i
    let copy_loop = code.label();
    let copy_done = code.label();
    code.bind_label(copy_loop);
    code.cmp_rr(Reg::R11, Reg::R10);
    code.jcc_label(0x8D, copy_done); // jge
    code.movzx_byte(Reg::R12, Reg::R8, 0);
    code.mov_mem_r8(Reg::R9, 0, Reg::R12);
    code.add_r_imm8(Reg::R8, 1);
    code.add_r_imm8(Reg::R9, 1);
    code.add_r_imm8(Reg::R11, 1);
    code.jmp_label(copy_loop);
    code.bind_label(copy_done);

    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.add_rsp(4112);
    code.leave_ret();

    code.bind_label(err);
    // Return empty string
    code.sub_rsp(8);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.u8(0x50);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.add_rsp(4112);
    code.leave_ret();
}

// ===========================================================================
// Service: FsSetCwd (chdir)
// ===========================================================================

fn emit_fs_set_cwd(code: &mut Code) {
    prologue(code);
    code.sub_rsp(32);

    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    to_cstr(code, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);

    // chdir(cstr)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -8);
    code.movabs(Reg::Rax, SYS_CHDIR);
    code.syscall();

    // rax = 0 on success, negative on error
    code.test_rr(Reg::Rax, Reg::Rax);
    let is_ok = code.label();
    code.jcc_label(0x84, is_ok);
    code.mov_r32_imm32(Reg::Rax, 1);
    let end = code.label();
    code.jmp_label(end);
    code.bind_label(is_ok);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.bind_label(end);

    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16);
    code.leave_ret();
}

// ===========================================================================
// Stub services (unimplemented subsystems)
// ===========================================================================

/// Emit a stub that returns 0 in rax.
fn emit_stub_int(code: &mut Code) {
    prologue(code);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// Emit a stub that returns an empty string via StrAlloc(0).
fn emit_stub_str(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.u8(0x50); // push 0
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.leave_ret();
}

// ===========================================================================
// Service emission
// ===========================================================================

/// Emit all runtime services. Returns the offsets.
pub(crate) fn emit_services(
    code: &mut Code,
    str_data_start_label: u32,
    str_data_end_label: u32,
) -> RuntimeOffsets {
    let mut services = HashMap::new();
    let offsets = RuntimeOffsets {
        services: HashMap::new(),
        str_data_start_label,
        str_data_end_label,
    };

    macro_rules! emit {
        ($service:expr, $func:expr) => {{
            let off = code.len() as u32;
            $func(code);
            services.insert($service, off);
        }};
    }

    // --- Core services ---
    emit!(RuntimeService::Init, |code: &mut Code| {
        emit_init(code, &offsets)
    });
    emit!(RuntimeService::Alloc, emit_alloc);
    emit!(RuntimeService::Free, emit_free);
    emit!(RuntimeService::MemLoad, emit_memload);
    emit!(RuntimeService::MemStore, emit_memstore);

    // --- String services ---
    emit!(RuntimeService::StrAlloc, emit_str_alloc);
    emit!(RuntimeService::StrFree, emit_str_free);
    emit!(RuntimeService::StrLen, emit_str_len);
    emit!(RuntimeService::StrByte, emit_str_byte);
    emit!(RuntimeService::StrSetByte, emit_str_set_byte);
    emit!(RuntimeService::StrConcat, emit_str_concat);
    emit!(RuntimeService::StrEq, emit_str_eq);
    emit!(RuntimeService::StrFromInt, emit_str_from_int);
    emit!(RuntimeService::StrFromBool, emit_str_from_bool);

    // --- Print services ---
    emit!(RuntimeService::PrintStr, emit_print_str);
    emit!(RuntimeService::PrintInt, emit_print_int);
    emit!(RuntimeService::PrintFloat, emit_print_float);
    emit!(RuntimeService::PrintChar, emit_print_char);

    // --- I/O services ---
    emit!(RuntimeService::WriteStdout, emit_write_stdout);
    emit!(RuntimeService::WriteStderr, emit_write_stderr);

    // --- Internal validation ---
    emit!(RuntimeService::StrValidate, |code: &mut Code| {
        emit_str_validate(code, false)
    });
    emit!(RuntimeService::StrValidateHeap, |code: &mut Code| {
        emit_str_validate(code, true)
    });

    // --- Control flow ---
    emit!(RuntimeService::Exit, emit_exit);
    emit!(RuntimeService::Fail, emit_fail);

    // --- Numeric conversion ---
    emit!(RuntimeService::IntToFloat, emit_int_to_float);
    emit!(RuntimeService::FloatToInt, emit_float_to_int);

    // --- C string conversion ---
    emit!(RuntimeService::ToCstr, emit_to_cstr);
    emit!(RuntimeService::FreeCstr, emit_free_cstr);

    // --- Filesystem services ---
    emit!(RuntimeService::FsExists, emit_fs_exists);
    emit!(RuntimeService::FsFileSize, emit_fs_file_size);
    emit!(RuntimeService::FsRead, emit_fs_read);
    emit!(RuntimeService::FsWrite, emit_fs_write);
    emit!(RuntimeService::FsCreateDir, emit_fs_create_dir);
    emit!(RuntimeService::FsRemoveDir, emit_fs_remove_dir);
    emit!(RuntimeService::FsRemoveFile, emit_fs_remove_file);
    emit!(RuntimeService::FsCopy, emit_fs_copy);
    emit!(RuntimeService::FsMove, emit_fs_move);
    emit!(RuntimeService::FsGetCwd, emit_fs_get_cwd);
    emit!(RuntimeService::FsSetCwd, emit_fs_set_cwd);

    // --- Vec services (stub) ---
    emit!(RuntimeService::VecNew, emit_stub_int);
    emit!(RuntimeService::VecPush, emit_stub_int);
    emit!(RuntimeService::VecGet, emit_stub_int);
    emit!(RuntimeService::VecLen, emit_stub_int);
    emit!(RuntimeService::VecFree, emit_stub_int);
    emit!(RuntimeService::VecSet, emit_stub_int);
    emit!(RuntimeService::VecPop, emit_stub_int);
    emit!(RuntimeService::VecRemove, emit_stub_int);

    // --- Process services (stub) ---
    emit!(RuntimeService::ProcessRun, emit_stub_int);
    emit!(RuntimeService::ProcessStdout, emit_stub_str);
    emit!(RuntimeService::ProcessStderr, emit_stub_str);
    emit!(RuntimeService::ProcessStdoutLen, emit_stub_int);
    emit!(RuntimeService::ProcessStderrLen, emit_stub_int);
    emit!(RuntimeService::ProcessId, emit_stub_int);

    // --- Time services (stub) ---
    emit!(RuntimeService::TimeNow, emit_stub_int);
    emit!(RuntimeService::TimeMillis, emit_stub_int);
    emit!(RuntimeService::TimeTicks, emit_stub_int);
    emit!(RuntimeService::TimeFreq, emit_stub_int);
    emit!(RuntimeService::TimeFiletime, emit_stub_int);
    emit!(RuntimeService::TimeFiletimeHigh, emit_stub_int);

    // --- Random services (stub) ---
    emit!(RuntimeService::RandomSeed, emit_stub_int);
    emit!(RuntimeService::RandomNext, emit_stub_int);

    // --- Environment services (stub) ---
    emit!(RuntimeService::EnvGet, emit_stub_str);
    emit!(RuntimeService::EnvSet, emit_stub_int);
    emit!(RuntimeService::EnvHas, emit_stub_int);
    emit!(RuntimeService::EnvRemove, emit_stub_int);

    // --- Network services (stub) ---
    emit!(RuntimeService::NetWsaStartup, emit_stub_int);
    emit!(RuntimeService::NetWsaCleanup, emit_stub_int);
    emit!(RuntimeService::NetWsaLastError, emit_stub_int);
    emit!(RuntimeService::NetSocket, emit_stub_int);
    emit!(RuntimeService::NetConnect, emit_stub_int);
    emit!(RuntimeService::NetBind, emit_stub_int);
    emit!(RuntimeService::NetListen, emit_stub_int);
    emit!(RuntimeService::NetAccept, emit_stub_int);
    emit!(RuntimeService::NetSend, emit_stub_int);
    emit!(RuntimeService::NetRecv, emit_stub_str);
    emit!(RuntimeService::NetClose, emit_stub_int);
    emit!(RuntimeService::NetShutdown, emit_stub_int);
    emit!(RuntimeService::NetGetAddrInfo, emit_stub_str);
    emit!(RuntimeService::NetFreeAddrInfo, emit_stub_int);
    emit!(RuntimeService::NetGetHostName, emit_stub_str);
    emit!(RuntimeService::NetHtons, emit_stub_int);

    // --- Crypto services (stub) ---
    emit!(RuntimeService::CryptoInit, emit_stub_int);
    emit!(RuntimeService::CryptoRandomBytes, emit_stub_int);
    emit!(RuntimeService::CryptoRandomInt, emit_stub_int);
    emit!(RuntimeService::CryptoSecureZero, emit_stub_int);

    RuntimeOffsets {
        services,
        str_data_start_label: offsets.str_data_start_label,
        str_data_end_label: offsets.str_data_end_label,
    }
}

/// Emit runtime data (placeholder for Linux BSS).
pub(crate) fn emit_data(_code: &mut Code, _offsets: &RuntimeOffsets) {
    // BSS is managed by the ELF loader on Linux.
}
