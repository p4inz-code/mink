//! Linux x86_64 runtime services — minimal V1 implementation.
//!
//! Provides the core MINK runtime services on Linux using raw x86_64 syscalls.
//! For V1, only the essential services are implemented:
//! - Init, Alloc, Free, MemLoad, MemStore
//! - Exit (with leak check)
//! - Fail (error reporting)
//! - PrintStr, PrintInt, PrintFloat, PrintChar
//!
//! Filesystem, process, networking, etc. are NOT yet implemented on Linux.

use std::collections::HashMap;

use super::super::ir::RuntimeService;
use super::x86_64::{Code, PatchKind, Reg};
use crate::runtime::abi::{BSS, HEAP_SIZE, MAX_LIVE_ALLOCS};
use crate::runtime::error::RuntimeErrorKind;

/// Linux syscall numbers
const SYS_WRITE: u64 = 1;
const SYS_EXIT: u64 = 60;

/// The offsets of the runtime services within .text.
#[derive(Debug, Clone)]
pub(crate) struct RuntimeOffsets {
    services: HashMap<RuntimeService, u32>,
}

impl RuntimeOffsets {
    pub(crate) fn of(&self, service: RuntimeService) -> u32 {
        *self
            .services
            .get(&service)
            .expect("runtime service not emitted")
    }
}

/// Standard function prologue: push rbp; mov rbp, rsp
fn prologue(code: &mut Code) {
    code.u8(0x55); // push rbp
    code.bytes(&[0x48, 0x89, 0xE5]); // mov rbp, rsp
}

/// Write a string to stdout using syscall write(1, buf, len).
fn syscall_write_stdout(code: &mut Code) {
    code.mov_rdi_imm32(1); // fd = stdout
    // rsi and rdx must be set by caller
    code.movabs(Reg::Rax, SYS_WRITE);
    code.syscall();
}

/// Emit all runtime services. Returns the offsets.
pub(crate) fn emit_services(
    code: &mut Code,
    _str_data_start_label: u32,
    _str_data_end_label: u32,
) -> RuntimeOffsets {
    let mut services = HashMap::new();

    // --- Init ---
    let off = code.len() as u32;
    emit_init(code);
    services.insert(RuntimeService::Init, off);

    // --- Alloc ---
    let off = code.len() as u32;
    emit_alloc(code);
    services.insert(RuntimeService::Alloc, off);

    // --- Free ---
    let off = code.len() as u32;
    emit_free(code);
    services.insert(RuntimeService::Free, off);

    // --- MemLoad ---
    let off = code.len() as u32;
    emit_memload(code);
    services.insert(RuntimeService::MemLoad, off);

    // --- MemStore ---
    let off = code.len() as u32;
    emit_memstore(code);
    services.insert(RuntimeService::MemStore, off);

    // --- Exit ---
    let off = code.len() as u32;
    emit_exit(code);
    services.insert(RuntimeService::Exit, off);

    // --- Fail ---
    let off = code.len() as u32;
    emit_fail(code);
    services.insert(RuntimeService::Fail, off);

    // --- PrintStr ---
    let off = code.len() as u32;
    emit_print_str(code);
    services.insert(RuntimeService::PrintStr, off);

    // --- PrintInt ---
    let off = code.len() as u32;
    emit_print_int(code);
    services.insert(RuntimeService::PrintInt, off);

    // --- PrintFloat ---
    let off = code.len() as u32;
    emit_print_float(code);
    services.insert(RuntimeService::PrintFloat, off);

    // --- PrintChar ---
    let off = code.len() as u32;
    emit_print_char(code);
    services.insert(RuntimeService::PrintChar, off);

    RuntimeOffsets { services }
}

fn emit_init(code: &mut Code) {
    // Save entry RSP to BSS
    code.mov_mem_r(Reg::Rbp, BSS.entry_rsp as i32, Reg::Rsp);
    code.leave_ret();
}

fn emit_alloc(code: &mut Code) {
    // Simplified bump allocator
    prologue(code);
    code.sub_rsp(8);

    // Load requested size from [rbp+16]
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    // Round up to 16-byte alignment
    code.add_r_imm8(Reg::Rax, 15);
    code.mov_r32_imm32(Reg::R10, !15u32);
    code.and_rr(Reg::Rax, Reg::R10);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // save aligned size

    // Bump: result = arena_base + cursor
    code.mov_r_mem(Reg::R10, Reg::Rbp, BSS.arena as i32);
    code.mov_r_mem(Reg::R11, Reg::Rbp, BSS.cursor as i32);
    code.add_rr(Reg::Rax, Reg::R10); // base + cursor = new block
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax); // save block addr

    // Update cursor
    code.mov_r_mem(Reg::R10, Reg::Rbp, -8); // size
    code.mov_r_mem(Reg::R11, Reg::Rbp, BSS.cursor as i32);
    code.add_rr(Reg::R11, Reg::R10);
    // Check overflow
    code.movabs(Reg::Rax, HEAP_SIZE);
    code.cmp_rr(Reg::R11, Reg::Rax);
    let overflow = code.label();
    code.jcc_label(0x87, overflow); // ja

    code.mov_mem_r(Reg::Rbp, BSS.cursor as i32, Reg::R11);

    // Record in liveness table (simplified: just store at table[0])
    code.mov_r_mem(Reg::R10, Reg::Rbp, -16); // block addr
    code.mov_r_mem(Reg::R11, Reg::Rbp, -8); // size
    code.movabs(Reg::Rax, BSS.table as u64);
    code.mov_mem_r(Reg::Rax, 0, Reg::R10); // addr
    code.mov_mem_r(Reg::Rax, 8, Reg::R11); // size
    code.mov_r32_imm32(Reg::R10, 1);
    code.mov_mem_r(Reg::Rax, 16, Reg::R10); // live = 1

    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16); // return block addr
    code.leave_ret();

    code.bind_label(overflow);
    code.movabs(Reg::Rcx, RuntimeErrorKind::OutOfMemory as u64);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Fail));
    code.int3();
}

fn emit_free(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // ptr
    code.test_rr(Reg::Rax, Reg::Rax);
    let done = code.label();
    code.jcc_label(0x84, done); // jz — null, no-op
    // Mark as not live in liveness table (simplified)
    code.movabs(Reg::R10, BSS.table as u64);
    code.mov_r_mem(Reg::R11, Reg::R10, 0); // stored addr
    code.cmp_rr(Reg::Rax, Reg::R11);
    let not_found = code.label();
    code.jcc_label(0x85, not_found); // jne
    code.mov_r32_imm32(Reg::R11, 0);
    code.mov_mem_r(Reg::R10, 16, Reg::R11); // live = 0
    code.bind_label(not_found);
    code.bind_label(done);
    code.leave_ret();
}

fn emit_memload(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_r_mem(Reg::Rax, Reg::Rax, 0);
    code.leave_ret();
}

fn emit_memstore(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // ptr
    code.mov_r_mem(Reg::R10, Reg::Rbp, 24); // val
    code.mov_mem_r(Reg::Rax, 0, Reg::R10);
    code.leave_ret();
}

fn emit_exit(code: &mut Code) {
    prologue(code);
    // Check for leaks in liveness table
    code.movabs(Reg::R10, BSS.table as u64);
    code.mov_r_mem(Reg::R11, Reg::R10, 16); // live flag
    code.test_rr(Reg::R11, Reg::R11);
    let leak = code.label();
    code.jcc_label(0x85, leak); // jnz

    // No leak — exit with code from [rbp+16]
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, 16);
    code.movabs(Reg::Rax, SYS_EXIT);
    code.syscall();
    code.int3();

    code.bind_label(leak);
    code.movabs(Reg::Rcx, RuntimeErrorKind::Leak as u64);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Fail));
    code.int3();
}

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

fn emit_print_int(code: &mut Code) {
    prologue(code);
    code.sub_rsp(32); // buffer for digits

    // value at [rbp+16]
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);

    // Handle negative
    let not_neg = code.label();
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x89, not_neg); // jns

    // Print '-'
    code.mov_r32_imm32(Reg::R10, 45);
    code.mov_mem_r(Reg::Rsp, 0, Reg::R10);
    code.mov_rdi_imm32(1);
    code.mov_rsi_rsp();
    code.mov_r32_imm32(Reg::Rdx, 1);
    code.movabs(Reg::Rax, SYS_WRITE);
    code.syscall();
    code.neg_rax();

    code.bind_label(not_neg);

    // Convert to string (simple loop)
    // Store digits in reverse at rsp
    code.mov_r32_imm32(Reg::R10, 0); // count
    let div_loop = code.label();
    code.bind_label(div_loop);
    code.xor_rdx();
    code.mov_r32_imm32(Reg::R11, 10);
    code.div_r(Reg::R11); // rax = quotient, rdx = remainder
    code.add_r_imm8(Reg::Rdx, 48); // + '0'
    code.mov_mem_r_idx(Reg::Rsp, Reg::R10, Reg::Rdx, 1);
    code.add_r_imm8(Reg::R10, 1);
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x85, div_loop); // jnz

    // Write the digits (they're stored in reverse, so write from end)
    code.mov_rdi_imm32(1);
    code.mov_rsi_rsp();
    code.mov_rdx_rax(); // length = count in r10, but rax was clobbered
    // Actually r10 has the count. Use mov rdx, r10
    code.mov_rr(Reg::Rdx, Reg::R10);
    code.movabs(Reg::Rax, SYS_WRITE);
    code.syscall();

    code.leave_ret();
}

fn emit_print_float(code: &mut Code) {
    prologue(code);
    // Placeholder: print "0.0"
    code.sub_rsp(16);
    // "0.0\0" in little-endian
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

/// Emit runtime data (placeholder for Linux BSS).
pub(crate) fn emit_data(_code: &mut Code, _offsets: &RuntimeOffsets) {
    // BSS is managed by the ELF loader on Linux.
}
