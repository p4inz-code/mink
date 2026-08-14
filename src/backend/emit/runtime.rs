//! The machine-level MINK runtime: hand-assembled runtime services.
//!
//! Every emitted image embeds a small runtime that provides process
//! initialization, the deterministic heap, error/exit handling, and
//! integer output. The services are emitted as ordinary x86-64 functions
//! appended to `.text` after the user functions, using the same calling
//! convention as generated code (stack arguments, result in `rax`,
//! 16-byte-aligned stack at every `call`, `rbp`/`rsp` the only
//! callee-saved registers). The ABI — `.bss` layout, arena size, liveness
//! table, error codes — is defined once in `src/runtime/abi.rs` and
//! `src/runtime/error.rs`; this module is its machine implementation.
//!
//! ## Services
//!
//! - [`RuntimeService::Init`] — sets the bump cursor to the arena base;
//!   called by the entry stub before `main`;
//! - [`RuntimeService::Alloc`] — `rt_alloc(size)`: a validated bump
//!   allocator with LIFO free-list reuse and a fixed-size liveness table;
//! - [`RuntimeService::Free`] — `rt_free(ptr)`: validates the pointer
//!   against the liveness table (alignment, liveness, exact start) and
//!   returns the block to the free list;
//! - [`RuntimeService::MemLoad`] / [`RuntimeService::MemStore`] — the
//!   validated 8-byte accessors behind `rt_mem_load` / `rt_mem_store`;
//! - [`RuntimeService::Exit`] — the leak-checked process exit: verifies
//!   that no live allocations remain (a leak is `E-R06`), then restores
//!   the process-entry stack pointer and returns — the loader turns the
//!   result into the exit code. The entry stub invokes it with `main`'s
//!   result; `rt_exit` calls it directly;
//! - [`RuntimeService::Fail`] — internal: writes the structured
//!   diagnostic for an error number to stderr and terminates with exit
//!   code `100 + number`. Never returns;
//! - [`RuntimeService::WriteStdout`] / [`RuntimeService::WriteStderr`] —
//!   internal thunks that write a buffer to the console through
//!   `kernel32`'s `GetStdHandle`/`WriteFile` (imported via the image's
//!   import table).
//!
//! ## Internal calling convention
//!
//! `rt_fail` and the write thunks are runtime-internal and use a private
//! register convention: `rt_fail(rcx = error number)` and
//! `write_stdout/write_stderr(rcx = buffer, rdx = length)`. The services
//! that call them keep the stack 16-byte aligned at the call site, so the
//! Win32 calls inside the thunks are correctly aligned too.

use std::collections::HashMap;

use super::super::ir::RuntimeService;
use super::x86_64::{Code, PatchKind, Reg};
use crate::runtime::abi::{BSS, HEAP_SIZE, MAX_LIVE_ALLOCS};
use crate::runtime::error::RuntimeErrorKind;

/// The liveness table size in bytes.
const TABLE_BYTES: i32 = (MAX_LIVE_ALLOCS as u32 * 24) as i32;

/// The offsets of the machine services within `.text`, plus the labels
/// the services reference (bound by [`emit_data`]).
#[derive(Debug, Clone)]
pub(crate) struct RuntimeOffsets {
    services: HashMap<RuntimeService, u32>,
    /// Label of the error-message blob in `.text`.
    pub(crate) msg_blob_label: u32,
    /// Label of the error-index table in `.text`.
    pub(crate) msg_table_label: u32,
    /// Label of the `\r\n` constant in `.text`.
    pub(crate) crlf_label: u32,
}

impl RuntimeOffsets {
    /// The absolute `.text` offset of a service.
    pub(crate) fn of(&self, service: RuntimeService) -> u32 {
        self.services[&service]
    }
}

/// Emits every runtime service into `code`, returning their offsets and
/// the data labels the services reference. The message data is emitted
/// separately by [`emit_data`], which binds those labels.
pub(crate) fn emit_services(code: &mut Code) -> RuntimeOffsets {
    let mut offsets = RuntimeOffsets {
        services: HashMap::new(),
        msg_blob_label: code.label(),
        msg_table_label: code.label(),
        crlf_label: code.label(),
    };
    let mut emit =
        |code: &mut Code, service: RuntimeService, body: fn(&mut Code, &RuntimeOffsets)| {
            offsets.services.insert(service, code.len() as u32);
            body(code, &offsets);
        };
    emit(code, RuntimeService::Init, |code, _| emit_init(code));
    emit(code, RuntimeService::Alloc, |code, _| emit_alloc(code));
    emit(code, RuntimeService::Free, |code, _| emit_free(code));
    emit(code, RuntimeService::MemLoad, |code, _| emit_mem_load(code));
    emit(code, RuntimeService::MemStore, |code, _| {
        emit_mem_store(code)
    });
    emit(code, RuntimeService::PrintInt, |code, r| {
        emit_print_int(code, r)
    });
    emit(code, RuntimeService::Exit, |code, _| emit_exit(code));
    emit(code, RuntimeService::Fail, emit_fail);
    emit(code, RuntimeService::WriteStdout, |code, _| {
        emit_write(code, true)
    });
    emit(code, RuntimeService::WriteStderr, |code, _| {
        emit_write(code, false)
    });
    offsets
}

/// Emits the message data into `code` (after the services): the
/// concatenated error messages, the error-index table (offset, length
/// pairs, one per error number), and the CRLF constant. Binds the labels
/// the services reference.
pub(crate) fn emit_data(code: &mut Code, offsets: &RuntimeOffsets) {
    // Error messages in number order (E-R01 first). Each message is the
    // full diagnostic line the runtime writes to stderr.
    let mut kinds = [
        RuntimeErrorKind::InitFailed,
        RuntimeErrorKind::OutOfMemory,
        RuntimeErrorKind::TableExhausted,
        RuntimeErrorKind::InvalidFree,
        RuntimeErrorKind::OutOfBounds,
        RuntimeErrorKind::Leak,
        RuntimeErrorKind::Misaligned,
        RuntimeErrorKind::InvalidSize,
    ];
    kinds.sort_by_key(|kind| kind.number());
    let messages = kinds
        .iter()
        .map(|kind| {
            format!(
                "mink: runtime error[{}]: {}\r\n",
                kind.code(),
                kind.message()
            )
        })
        .collect::<Vec<_>>();

    // The message blob, then the table of (offset, length) pairs indexed
    // by (error number - 1) * 16 bytes.
    let mut blob = Vec::new();
    let mut table = Vec::with_capacity(kinds.len() * 16);
    for message in &messages {
        table.extend_from_slice(&(blob.len() as u64).to_le_bytes());
        table.extend_from_slice(&(message.len() as u64).to_le_bytes());
        blob.extend_from_slice(message.as_bytes());
    }

    code.bind_label(offsets.msg_blob_label);
    code.bytes(&blob);
    code.bind_label(offsets.msg_table_label);
    code.bytes(&table);
    code.bind_label(offsets.crlf_label);
    code.bytes(b"\r\n");
}

/// The standard MINK-function prologue: `push rbp; mov rbp, rsp`.
fn prologue(code: &mut Code) {
    code.u8(0x55);
    code.bytes(&[0x48, 0x89, 0xE5]);
}

/// `mov rcx, error_number; call rt_fail` — a fatal error path. `rt_fail`
/// never returns.
fn fail(code: &mut Code, number: u32) {
    code.mov_r32_imm32(Reg::Rcx, number);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Fail));
}

// ---------------------------------------------------------------------------
// Services
// ---------------------------------------------------------------------------

/// `rt_init`: reset the bump cursor and the free list. The `.bss` state
/// is zero-initialized by the loader, so this is an explicit no-op that
/// keeps the runtime state deterministic even if the loader contract
/// changes; the cursor is an offset from the arena base (a block lives at
/// `arena + offset`), so it starts at zero.
fn emit_init(code: &mut Code) {
    prologue(code);
    code.mov_rip_imm32(PatchKind::Bss(BSS.cursor as u32), 0);
    code.mov_rip_imm32(PatchKind::Bss(BSS.free_head as u32), 0);
    code.leave_ret();
}

/// `rt_alloc(size) -> addr` (size at `[rbp + 16]`).
///
/// Validated bump allocation with LIFO free-list reuse and a bounded
/// liveness table: every returned block is 16-aligned and recorded in the
/// table as live, so later frees and accesses are checked against it.
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

    // Reuse the most recently freed block when the free list is nonempty.
    let bump = code.label();
    let record = code.label();
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(BSS.free_head as u32));
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x84, bump); // jz
    code.mov_r_mem(Reg::Rcx, Reg::Rax, 0); // next
    code.mov_rip_r(Reg::Rcx, PatchKind::Bss(BSS.free_head as u32));
    code.jmp_label(record);

    // Otherwise bump the cursor within the arena bounds.
    code.bind_label(bump);
    code.mov_r_rip(Reg::Rcx, PatchKind::Bss(BSS.cursor as u32));
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, -8);
    code.mov_rr(Reg::Rax, Reg::Rcx);
    code.add_rr(Reg::Rax, Reg::Rdx); // new offset
    code.cmp_r_imm32(Reg::Rax, HEAP_SIZE as u32);
    let oom = code.label();
    code.jcc_label(0x87, oom); // ja
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(BSS.cursor as u32));
    code.mov_rr(Reg::Rax, Reg::Rcx); // block offset
    code.lea_r_rip(Reg::Rcx, PatchKind::Bss(BSS.arena as u32));
    code.add_rr(Reg::Rax, Reg::Rcx); // absolute block address

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

/// `rt_free(ptr)` (ptr at `[rbp + 16]`).
///
/// The pointer must be the 16-aligned exact start of a live allocation;
/// anything else (a double free, a never-allocated or interior pointer,
/// `null`, or a misaligned address) is a structured runtime error.
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

/// `rt_mem_load(addr) -> word` (addr at `[rbp + 16]`).
///
/// The 8-byte word at `addr` must lie entirely inside a live allocation.
fn emit_mem_load(code: &mut Code) {
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
    code.jcc_label(0x84, next); // je — dead entry, skip
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

/// `rt_mem_store(addr, value)` (addr at `[rbp + 16]`, value at `[rbp + 24]`).
fn emit_mem_store(code: &mut Code) {
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
    code.jcc_label(0x84, next); // je — dead entry, skip
    code.mov_r_mem(Reg::R9, Reg::Rdx, 0); // start
    code.cmp_rr(Reg::Rax, Reg::R9);
    code.jcc_label(0x82, next); // jb
    code.mov_r_mem(Reg::R10, Reg::Rdx, 8); // size
    code.add_rr(Reg::R10, Reg::R9); // end
    code.cmp_rr(Reg::Rax, Reg::R10);
    code.jcc_label(0x83, next); // jae
    code.mov_rr(Reg::R11, Reg::Rax);
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

/// `rt_print_int(value)` (value at `[rbp + 16]`): decimal conversion into
/// the `.bss` print buffer, then a write of the digits (with a `-` sign
/// for negatives) and a CRLF to stdout.
fn emit_print_int(code: &mut Code, offsets: &RuntimeOffsets) {
    prologue(code);
    code.sub_rsp(16);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // save the value for the sign

    // Build the digits from the end of the buffer downward.
    code.lea_r_rip(Reg::R8, PatchKind::Bss(BSS.print_buf as u32));
    code.add_r_imm8(Reg::R8, 31);
    code.test_rr(Reg::Rax, Reg::Rax);
    let digits = code.label();
    let digit_loop = code.label();
    code.jcc_label(0x89, digits); // jns
    code.neg_r(Reg::Rax);
    code.bind_label(digits);
    code.bind_label(digit_loop);
    code.xor_rr32(Reg::Rdx, Reg::Rdx);
    code.mov_r32_imm32(Reg::Rcx, 10);
    code.div_r(Reg::Rcx); // unsigned rdx:rax / rcx
    code.add_r_imm8(Reg::Rdx, b'0');
    code.mov_mem_r8(Reg::R8, 0, Reg::Rdx);
    code.dec_r(Reg::R8);
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x85, digit_loop); // jnz

    // A negative value gets a leading '-'.
    let write = code.label();
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x89, write); // jns
    code.mov_mem_imm8(Reg::R8, 0, b'-');
    code.dec_r(Reg::R8);

    // Write the digits, then the CRLF.
    code.bind_label(write);
    code.lea_r_mem(Reg::Rcx, Reg::R8, 1); // buffer start
    code.lea_r_rip(Reg::Rax, PatchKind::Bss(BSS.print_buf as u32));
    code.add_r_imm8(Reg::Rax, 32);
    code.sub_rr(Reg::Rax, Reg::Rcx); // length
    code.mov_rr(Reg::Rdx, Reg::Rax);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::WriteStdout));

    code.lea_r_rip(Reg::Rcx, PatchKind::Label(offsets.crlf_label));
    code.mov_r32_imm32(Reg::Rdx, 2);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::WriteStdout));

    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_exit(code)` (code at `[rbp + 16]`): the leak-checked process exit.
/// Scans the liveness table; a live allocation is a leak (`E-R06`). Then
/// restores the process-entry stack pointer and returns, so the loader
/// turns the result into the exit code. Also invoked by the entry stub
/// with `main`'s result.
fn emit_exit(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, 16); // requested code

    let scan = code.label();
    let done = code.label();
    let leak = code.label();
    code.lea_r_rip(Reg::Rax, PatchKind::Bss(BSS.table as u32));
    code.lea_r_mem(Reg::Rdx, Reg::Rax, TABLE_BYTES);
    code.bind_label(scan);
    code.cmp_rr(Reg::Rax, Reg::Rdx);
    code.jcc_label(0x83, done); // jae
    code.cmp_mem_imm8(Reg::Rax, 16, 0);
    code.jcc_label(0x85, leak); // jne
    code.add_r_imm8(Reg::Rax, 24);
    code.jmp_label(scan);

    code.bind_label(leak);
    fail(code, 6); // E-R06

    code.bind_label(done);
    code.mov_rr(Reg::Rax, Reg::Rcx);
    code.mov_r_rip(Reg::Rsp, PatchKind::Bss(BSS.entry_rsp as u32));
    code.u8(0xC3); // ret
}

/// `rt_fail(rcx = error number)`: writes the structured diagnostic to
/// stderr and terminates with exit code `100 + number`. Never returns.
fn emit_fail(code: &mut Code, offsets: &RuntimeOffsets) {
    prologue(code);
    code.sub_rsp(8); // align (entry rsp ≡ 8 → now ≡ 0); [rbp-8] holds the number
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rcx);

    // Index the message table: entry = (number - 1) * 16.
    code.dec_r(Reg::Rcx);
    code.mov_rr(Reg::Rax, Reg::Rcx);
    code.shl_r_imm8(Reg::Rax, 4);
    code.lea_r_rip(Reg::Rcx, PatchKind::Label(offsets.msg_table_label));
    code.mov_rr(Reg::Rdx, Reg::Rcx);
    code.add_rr(Reg::Rdx, Reg::Rax); // entry
    code.mov_r_mem(Reg::Rax, Reg::Rdx, 0); // offset
    code.mov_r_mem(Reg::Rdx, Reg::Rdx, 8); // length
    code.lea_r_rip(Reg::Rcx, PatchKind::Label(offsets.msg_blob_label));
    code.add_rr(Reg::Rcx, Reg::Rax); // buffer
    code.call_patch(PatchKind::RuntimeService(RuntimeService::WriteStderr));

    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8); // error number
    code.add_r_imm8(Reg::Rax, 100);
    code.mov_r_rip(Reg::Rsp, PatchKind::Bss(BSS.entry_rsp as u32));
    code.u8(0xC3); // ret
}

/// The stdout/stderr write thunk: `write(rcx = buffer, rdx = length)`.
/// Fetches (and caches) the standard handle through `GetStdHandle`, then
/// writes through `WriteFile`. A missing console silently skips the write
/// — the process exit code remains authoritative.
fn emit_write(code: &mut Code, stdout: bool) {
    let handle_const: u32 = if stdout { 0xFFFF_FFF5 } else { 0xFFFF_FFF4 }; // -11 / -12
    let cache = if stdout {
        BSS.stdout_handle
    } else {
        BSS.stderr_handle
    };
    prologue(code);
    code.sub_rsp(16); // spills [rbp-8] = buffer, [rbp-16] = length
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rcx);
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rdx);

    let have = code.label();
    let done = code.label();
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(cache as u32));
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x85, have); // jnz
    code.sub_rsp(32); // shadow space for GetStdHandle
    code.mov_r32_imm32(Reg::Rcx, handle_const);
    code.call_rip(PatchKind::Iat(0)); // GetStdHandle
    code.add_rsp(32);
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(cache as u32));
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x84, done); // jz
    code.cmp_r_imm8(Reg::Rax, 0xFF); // -1: INVALID_HANDLE_VALUE
    code.jcc_label(0x84, done); // je

    code.bind_label(have);
    code.mov_rr(Reg::Rcx, Reg::Rax); // handle
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, -8); // buffer
    code.mov_r32_mem(Reg::R8, Reg::Rbp, -16); // length (DWORD)
    code.lea_r_rip(Reg::R9, PatchKind::Bss(BSS.bytes_written as u32));
    code.sub_rsp(48); // 32 shadow + 8 (5th arg) + 8 padding; rsp stays 16-aligned
    code.mov_mem_imm32(Reg::Rsp, 32, 0); // lpOverlapped = NULL
    code.call_rip(PatchKind::Iat(1)); // WriteFile
    code.add_rsp(48);

    code.bind_label(done);
    code.leave_ret();
}
