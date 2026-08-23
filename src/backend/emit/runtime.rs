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
/// the services reference (bound by [`emit_data`] and the emitter's string
/// data).
#[derive(Debug, Clone)]
pub(crate) struct RuntimeOffsets {
    services: HashMap<RuntimeService, u32>,
    /// Label of the error-message blob in `.text`.
    pub(crate) msg_blob_label: u32,
    /// Label of the error-index table in `.text`.
    pub(crate) msg_table_label: u32,
    /// Label of the `\r\n` constant in `.text`.
    pub(crate) crlf_label: u32,
    /// Label bounding the start of the image's immutable string-data
    /// region (bound by the emitter after the runtime data).
    pub(crate) str_data_start: u32,
    /// Label bounding one past the end of that region.
    pub(crate) str_data_end: u32,
}

impl RuntimeOffsets {
    /// The absolute `.text` offset of a service.
    pub(crate) fn of(&self, service: RuntimeService) -> u32 {
        self.services[&service]
    }
}

/// Emits every runtime service into `code`, returning their offsets and
/// the data labels the services reference. The message data is emitted
/// separately by [`emit_data`], and the string data by the caller, which
/// binds those labels. `str_data_start` and `str_data_end` are the labels
/// bounding the image's immutable string-data region; `rt_init` records
/// their addresses so the string intrinsics can validate literal strings.
pub(crate) fn emit_services(
    code: &mut Code,
    str_data_start: u32,
    str_data_end: u32,
) -> RuntimeOffsets {
    let mut offsets = RuntimeOffsets {
        services: HashMap::new(),
        msg_blob_label: code.label(),
        msg_table_label: code.label(),
        crlf_label: code.label(),
        str_data_start,
        str_data_end,
    };
    let mut emit =
        |code: &mut Code, service: RuntimeService, body: fn(&mut Code, &RuntimeOffsets)| {
            offsets.services.insert(service, code.len() as u32);
            body(code, &offsets);
        };
    emit(code, RuntimeService::Init, emit_init);
    emit(code, RuntimeService::Alloc, |code, _| emit_alloc(code));
    emit(code, RuntimeService::Free, |code, _| emit_free(code));
    emit(code, RuntimeService::MemLoad, |code, _| emit_mem_load(code));
    emit(code, RuntimeService::MemStore, |code, _| {
        emit_mem_store(code)
    });
    emit(code, RuntimeService::StrAlloc, |code, _| {
        emit_str_alloc(code)
    });
    emit(code, RuntimeService::StrFree, |code, _| emit_str_free(code));
    emit(code, RuntimeService::StrLen, |code, _| emit_str_len(code));
    emit(code, RuntimeService::StrByte, |code, _| emit_str_byte(code));
    emit(code, RuntimeService::StrSetByte, |code, _| {
        emit_str_set_byte(code)
    });
    emit(code, RuntimeService::PrintStr, |code, r| {
        emit_print_str(code, r)
    });
    emit(code, RuntimeService::PrintInt, |code, r| {
        emit_print_int(code, r)
    });
    emit(code, RuntimeService::PrintFloat, |code, r| {
        emit_print_float(code, r);
    });
    emit(code, RuntimeService::PrintChar, |code, r| {
        emit_print_char(code, r)
    });
    emit(code, RuntimeService::Exit, |code, _| emit_exit(code));
    emit(code, RuntimeService::Fail, emit_fail);
    emit(code, RuntimeService::WriteStdout, |code, _| {
        emit_write(code, true)
    });
    emit(code, RuntimeService::WriteStderr, |code, _| {
        emit_write(code, false)
    });
    emit(code, RuntimeService::StrValidate, |code, _| {
        emit_str_validate(code, false)
    });
    emit(code, RuntimeService::StrValidateHeap, |code, _| {
        emit_str_validate(code, true)
    });
    emit(code, RuntimeService::VecNew, |code, _| emit_vec_new(code));
    emit(code, RuntimeService::VecPush, |code, _| emit_vec_push(code));
    emit(code, RuntimeService::VecGet, |code, _| emit_vec_get(code));
    emit(code, RuntimeService::VecLen, |code, _| emit_vec_len(code));
    emit(code, RuntimeService::VecFree, |code, _| emit_vec_free(code));
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
        RuntimeErrorKind::StringIndexOutOfRange,
        RuntimeErrorKind::ArrayIndexOutOfRange,
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
/// never returns. Shared with the user-function emitter for generated
/// bounds checks (array indexing, `E-R10`).
pub(crate) fn fail(code: &mut Code, number: u32) {
    code.mov_r32_imm32(Reg::Rcx, number);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Fail));
}

// ---------------------------------------------------------------------------
// Services
// ---------------------------------------------------------------------------

/// `rt_init`: reset the bump cursor and the free list, and record the
/// bounds of the image's immutable string-data region. The `.bss` state is
/// zero-initialized by the loader, so the resets are explicit no-ops that
/// keep the runtime state deterministic even if the loader contract
/// changes; the cursor is an offset from the arena base (a block lives at
/// `arena + offset`), so it starts at zero. The string-data bounds are
/// absolute addresses (positions within `.text`), read by the string
/// intrinsics to validate literal strings.
fn emit_init(code: &mut Code, offsets: &RuntimeOffsets) {
    prologue(code);
    code.mov_rip_imm32(PatchKind::Bss(BSS.cursor as u32), 0);
    code.mov_rip_imm32(PatchKind::Bss(BSS.free_head as u32), 0);
    code.lea_r_rip(Reg::Rax, PatchKind::Label(offsets.str_data_start));
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(BSS.str_data_start as u32));
    code.lea_r_rip(Reg::Rax, PatchKind::Label(offsets.str_data_end));
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(BSS.str_data_end as u32));
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

/// `rt_str_alloc(size) -> Str` (size at `[rbp + 16]`): allocate a
/// length-prefixed string blob of `size` bytes through the regular
/// allocator (`8 + size` bytes) and write the length prefix at the block
/// start. The block's data bytes are zero on fresh bumps and retain old
/// contents on free-list reuse, matching `rt_alloc`. A negative size is
/// `E-R08`; the allocator reports exhaustion (`E-R02`/`E-R03`) itself.
fn emit_str_alloc(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16); // [rbp-8] holds the size
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.test_rr(Reg::Rax, Reg::Rax);
    let bad_size = code.label();
    code.jcc_label(0x8C, bad_size); // js
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);

    // Allocate 8 + size bytes through `rt_alloc` (the argument is pushed
    // with alignment padding, so the stack stays 16-aligned at the call).
    code.add_r_imm8(Reg::Rax, 8);
    code.sub_rsp(8);
    code.u8(0x50); // push rax
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Alloc));
    code.add_rsp(16);

    // Write the length prefix: [rax] = size.
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, -8);
    code.mov_mem_r(Reg::Rax, 0, Reg::Rdx);
    code.leave_ret();

    code.bind_label(bad_size);
    fail(code, 8); // E-R08
}

/// `rt_str_free(s)` (s at `[rbp + 16]`): deallocate the string blob at `s`.
/// The block must be the exact start of a live allocation (`E-R04`/`E-R07`
/// otherwise, matching `rt_free`), so freeing an immutable literal is a
/// structured error.
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

/// `rt_str_len(s) -> Int` (s at `[rbp + 16]`): the byte length of a
/// validated string (`E-R05` when `s` is neither a live heap string nor an
/// image literal).
fn emit_str_len(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrValidate));
    code.mov_r_mem(Reg::Rax, Reg::Rax, 0); // len = [s]
    code.leave_ret();
}

/// `rt_str_byte(s, index) -> Int` (s at `[rbp + 16]`, index at `[rbp + 24]`):
/// the byte of a validated string at `index`, bounds-checked against the
/// length prefix (`E-R09` out of range, `E-R05` for an invalid string).
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

/// `rt_str_set_byte(s, index, value)` (s at `[rbp + 16]`, index at
/// `[rbp + 24]`, value at `[rbp + 32]`): write the low byte of `value` at
/// `index` of a *heap* string (immutable image literals are rejected with
/// `E-R05`; an out-of-range index is `E-R09`).
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

/// `rt_print_str(s)` (s at `[rbp + 16]`): write the bytes of a validated
/// string plus a CRLF to stdout.
fn emit_print_str(code: &mut Code, offsets: &RuntimeOffsets) {
    prologue(code);
    code.sub_rsp(16);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrValidate));
    code.mov_r_mem(Reg::R8, Reg::Rax, 0); // len
    code.lea_r_mem(Reg::Rcx, Reg::Rax, 8); // data base
    code.mov_rr(Reg::Rdx, Reg::R8); // length
    code.call_patch(PatchKind::RuntimeService(RuntimeService::WriteStdout));
    code.lea_r_rip(Reg::Rcx, PatchKind::Label(offsets.crlf_label));
    code.mov_r32_imm32(Reg::Rdx, 2);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::WriteStdout));
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// Internal string-pointer validator: `rt_str_validate(rax = s) -> s`, or
/// `E-R05`. With `heap_only`, only a live heap-block start is accepted
/// (used by `rt_str_set_byte`, which cannot write immutable image data);
/// otherwise a pointer into the image's string-data region is accepted too
/// (used by every read). Uses the private register convention: `rax` in
/// and out; `rcx`/`rdx`/`r8`/`r9` are scratch. Never calls out (the `E-R05`
/// path goes through `rt_fail`), so its frame needs no extra alignment.
fn emit_str_validate(code: &mut Code, heap_only: bool) {
    prologue(code);
    code.sub_rsp(16);

    // Scan the liveness table for a live entry whose start equals `s`.
    let scan = code.label();
    let next = code.label();
    let not_heap = code.label();
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
        code.mov_r_rip(Reg::Rcx, PatchKind::Bss(BSS.str_data_start as u32));
        code.mov_r_rip(Reg::Rdx, PatchKind::Bss(BSS.str_data_end as u32));
        code.cmp_rr(Reg::Rax, Reg::Rcx);
        code.jcc_label(0x82, fail5); // jb
        code.cmp_rr(Reg::Rax, Reg::Rdx);
        code.jcc_label(0x83, fail5); // jae
        code.leave_ret();
    }

    code.bind_label(fail5);
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

/// `rt_print_char(value)` (value at `[rbp + 16]`): write the single byte
/// of the character plus a CRLF to stdout. The char model is byte-sized
/// (layout `(1, 1)`), so the low byte of the value word is the character.
fn emit_print_char(code: &mut Code, offsets: &RuntimeOffsets) {
    prologue(code);
    code.lea_r_rip(Reg::R8, PatchKind::Bss(BSS.print_buf as u32));
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_mem_r8(Reg::R8, 0, Reg::Rax);
    code.mov_rr(Reg::Rcx, Reg::R8);
    code.mov_r32_imm32(Reg::Rdx, 1);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::WriteStdout));
    code.lea_r_rip(Reg::Rcx, PatchKind::Label(offsets.crlf_label));
    code.mov_r32_imm32(Reg::Rdx, 2);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::WriteStdout));
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_print_float(value)` (value at `[rbp + 16]`): the deterministic
/// decimal representation of the double, then a CRLF.
///
/// Format (17 significant digits, round-half-even, `%g`-style):
/// - `NaN`, `Inf`/`-Inf` for non-finite values;
/// - fixed notation when `-4 <= E < 17` (e.g. `0.015`, `123456.25`);
/// - otherwise scientific: `d[.dddd]e+XX` / `e-XX`, the exponent sign
///   always present (e.g. `1e+308`, `5e-324`, `1e-5`);
/// - trailing zeros in the fractional part are trimmed (`2.5`, `1e+17`);
/// - negative zero prints `-0`.
///
/// The digits are exact: the value `f * 2^k` is expanded into the exact
/// integer `f * 5^N` (N = `-k` for `k < 0`, `f << k` for `k >= 0`), the
/// full decimal expansion (up to 767 digits) is extracted by repeated
/// division by 10 into `dtoa_digits`, and the 17 significant digits are
/// rounded half-even against the remaining digits. 17 digits guarantee a
/// round trip through `decode_float` back to the same double.
///
/// Scratch: `dtoa_words` (40 u64 words) holds the big integer, built by
/// repeated multiplication by 5 (or doubling for `k >= 0`); `dtoa_digits`
/// holds the expansion digits, one byte each. `print_buf` is the output
/// assembly area (the format never exceeds 24 characters).
fn emit_print_float(code: &mut Code, offsets: &RuntimeOffsets) {
    prologue(code);
    code.sub_rsp(48);
    // Spills: [rbp-8] sign flag, [rbp-16] bits, [rbp-24] digit count,
    // [rbp-32] decimal exponent E, [rbp-40] N (digits shifted right by
    // N decimal places), [rbp-48] unused. Registers: r12 = words base,
    // r13 = digits base, r14 = big-int word count, r15 = output offset.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax);

    // --- sign flag and magnitude ---
    code.mov_rr(Reg::Rcx, Reg::Rax);
    code.shr_r_imm8(Reg::Rcx, 63);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rcx);
    code.movabs(Reg::Rdx, 0x7FFF_FFFF_FFFF_FFFF);
    code.and_rr(Reg::Rax, Reg::Rdx);

    // --- non-finite values: NaN / Inf ---
    code.mov_rr(Reg::Rcx, Reg::Rax);
    code.shr_r_imm8(Reg::Rcx, 52);
    code.movabs(Reg::Rdx, 0x7FF);
    code.and_rr(Reg::Rcx, Reg::Rdx);
    let normal = code.label();
    let nan = code.label();
    let special = code.label();
    let special_write = code.label();
    code.cmp_r_imm32(Reg::Rcx, 0x7FF);
    code.jcc_label(0x85, normal); // jne: finite
    code.movabs(Reg::Rdx, 0xF_FFFF_FFFF_FFFF);
    code.mov_rr(Reg::Rcx, Reg::Rax);
    code.and_rr(Reg::Rcx, Reg::Rdx);
    code.test_rr(Reg::Rcx, Reg::Rcx);
    code.jcc_label(0x85, nan); // jnz: NaN
    // Inf at [buf .. buf+3).
    code.lea_r_rip(Reg::R8, PatchKind::Bss(BSS.print_buf as u32));
    code.mov_mem_imm8(Reg::R8, 0, b'I');
    code.mov_mem_imm8(Reg::R8, 1, b'n');
    code.mov_mem_imm8(Reg::R8, 2, b'f');
    code.mov_r32_imm32(Reg::Rdx, 3);
    code.jmp_label(special);
    // NaN at [buf .. buf+3).
    code.bind_label(nan);
    code.lea_r_rip(Reg::R8, PatchKind::Bss(BSS.print_buf as u32));
    code.mov_mem_imm8(Reg::R8, 0, b'N');
    code.mov_mem_imm8(Reg::R8, 1, b'a');
    code.mov_mem_imm8(Reg::R8, 2, b'N');
    code.mov_r32_imm32(Reg::Rdx, 3);
    code.bind_label(special);
    // Prepend '-' when the sign is set (shift the three chars right).
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    code.test_rr(Reg::Rcx, Reg::Rcx);
    code.jcc_label(0x84, special_write); // jz
    code.movzx_byte(Reg::Rax, Reg::R8, 2);
    code.mov_mem_r8(Reg::R8, 3, Reg::Rax);
    code.movzx_byte(Reg::Rax, Reg::R8, 1);
    code.mov_mem_r8(Reg::R8, 2, Reg::Rax);
    code.movzx_byte(Reg::Rax, Reg::R8, 0);
    code.mov_mem_r8(Reg::R8, 1, Reg::Rax);
    code.mov_mem_imm8(Reg::R8, 0, b'-');
    code.add_r_imm8(Reg::Rdx, 1);
    code.bind_label(special_write);
    code.mov_rr(Reg::Rcx, Reg::R8);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::WriteStdout));
    code.lea_r_rip(Reg::Rcx, PatchKind::Label(offsets.crlf_label));
    code.mov_r32_imm32(Reg::Rdx, 2);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::WriteStdout));
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();

    // --- finite values ---
    // r15 = output offset into print_buf (0, or 1 with '-' at [0]).
    code.bind_label(normal);
    let sign_done = code.label();
    code.xor_rr32(Reg::R15, Reg::R15);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    code.test_rr(Reg::Rcx, Reg::Rcx);
    code.jcc_label(0x84, sign_done); // jz
    code.lea_r_rip(Reg::R8, PatchKind::Bss(BSS.print_buf as u32));
    code.mov_mem_imm8(Reg::R8, 0, b'-');
    code.mov_r32_imm32(Reg::R15, 1);
    code.bind_label(sign_done);

    // Decompose: f = frac | 2^52 (normal) or frac (subnormal);
    // k = exp_field - 1075 (normal) or -1074 (subnormal); value = f * 2^k.
    // The exponent field is recomputed here (the sign handling above
    // clobbered `rcx`, which held it after the NaN check).
    code.lea_r_rip(Reg::R12, PatchKind::Bss(BSS.dtoa_words as u32));
    code.lea_r_rip(Reg::R13, PatchKind::Bss(BSS.dtoa_digits as u32));
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -16); // bits
    code.shr_r_imm8(Reg::Rcx, 52);
    code.movabs(Reg::Rdx, 0x7FF);
    code.and_rr(Reg::Rcx, Reg::Rdx); // exp_field
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16); // bits
    code.movabs(Reg::Rdx, 0xF_FFFF_FFFF_FFFF);
    code.and_rr(Reg::Rax, Reg::Rdx); // frac
    code.movabs(Reg::Rdx, 1);
    code.shl_r_imm8(Reg::Rdx, 52); // 2^52
    let subnormal = code.label();
    let have_k = code.label();
    code.test_rr(Reg::Rcx, Reg::Rcx); // rcx = exp_field
    code.jcc_label(0x84, subnormal); // jz
    code.or_rr(Reg::Rax, Reg::Rdx); // f |= 2^52
    code.movabs(Reg::Rdx, 1075);
    code.sub_rr(Reg::Rcx, Reg::Rdx); // k = exp - 1075
    code.jmp_label(have_k);
    code.bind_label(subnormal);
    code.movabs(Reg::Rcx, (-1074i64) as u64); // k = -1074
    code.bind_label(have_k);
    // f in rax, k in rcx. A zero significand prints 0/-0 directly.
    let print_zero = code.label();
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x84, print_zero); // jz
    code.mov_mem_r(Reg::R12, 0, Reg::Rax); // words[0] = f
    code.mov_r32_imm32(Reg::R14, 1); // len = 1
    // Build I: k < 0 → I = f * 5^N (N = -k, multiplier 5);
    // k >= 0 → I = f << k (multiplier 2, count = k); N = 0.
    let k_nonneg = code.label();
    let mul_outer = code.label();
    code.test_rr(Reg::Rcx, Reg::Rcx);
    code.jcc_label(0x89, k_nonneg); // jns
    code.neg_r(Reg::Rcx);
    code.mov_mem_r(Reg::Rbp, -40, Reg::Rcx); // N = -k
    code.mov_r32_imm32(Reg::Rsi, 5);
    code.mov_rr(Reg::Rdi, Reg::Rcx); // count
    code.jmp_label(mul_outer);
    code.bind_label(k_nonneg);
    code.mov_mem_imm32(Reg::Rbp, -40, 0); // N = 0
    code.mov_r32_imm32(Reg::Rsi, 2);
    code.mov_rr(Reg::Rdi, Reg::Rcx); // count = k
    // Multiply the big int (len words at r12) by rsi, rdi times.
    let mul_inner = code.label();
    let mul_grow = code.label();
    let mul_no_grow = code.label();
    let mul_done = code.label();
    code.bind_label(mul_outer);
    code.test_rr(Reg::Rdi, Reg::Rdi);
    code.jcc_label(0x84, mul_done); // jz
    code.xor_rr32(Reg::Rcx, Reg::Rcx); // carry = 0
    code.mov_rr(Reg::R8, Reg::R12); // word pointer
    code.mov_rr(Reg::R9, Reg::R14); // words left
    code.bind_label(mul_inner);
    code.test_rr(Reg::R9, Reg::R9);
    code.jcc_label(0x84, mul_grow); // jz: pass done
    code.mov_r_mem(Reg::Rax, Reg::R8, 0);
    code.mul_r(Reg::Rsi); // rdx:rax = word * multiplier
    code.add_rr(Reg::Rax, Reg::Rcx); // + carry_in
    code.bytes(&[0x48, 0x83, 0xD2, 0x00]); // adc rdx, 0: fold the carry
    code.mov_mem_r(Reg::R8, 0, Reg::Rax);
    code.mov_rr(Reg::Rcx, Reg::Rdx); // carry_out
    code.add_r_imm8(Reg::R8, 8);
    code.dec_r(Reg::R9);
    code.jmp_label(mul_inner);
    code.bind_label(mul_grow);
    code.test_rr(Reg::Rcx, Reg::Rcx);
    code.jcc_label(0x84, mul_no_grow); // jz: no new word
    code.mov_mem_r(Reg::R8, 0, Reg::Rcx); // append carry word
    code.add_r_imm8(Reg::R14, 1);
    code.bind_label(mul_no_grow);
    code.dec_r(Reg::Rdi);
    code.jmp_label(mul_outer);
    code.bind_label(mul_done);

    // --- exact decimal expansion ---
    // Repeatedly divide the big int by 10, storing one digit (the
    // remainder) per pass into digits at r11, until the number is zero.
    let digit_outer = code.label();
    let digit_inner = code.label();
    let digit_shrink = code.label();
    let digit_shrink_done = code.label();
    let digits_done = code.label();
    code.mov_rr(Reg::R11, Reg::R13); // digit write pointer
    code.bind_label(digit_outer);
    code.test_rr(Reg::R14, Reg::R14);
    code.jcc_label(0x84, digits_done); // jz: number is zero
    code.mov_rr(Reg::R8, Reg::R14);
    code.shl_r_imm8(Reg::R8, 3);
    code.add_rr(Reg::R8, Reg::R12);
    code.sub_r_imm32(Reg::R8, 8); // top word pointer
    code.xor_rr32(Reg::Rdx, Reg::Rdx); // carry = 0
    code.mov_r32_imm32(Reg::Rcx, 10);
    code.bind_label(digit_inner);
    code.mov_r_mem(Reg::Rax, Reg::R8, 0);
    code.div_r(Reg::Rcx); // rdx:rax / 10
    code.mov_mem_r(Reg::R8, 0, Reg::Rax);
    code.sub_r_imm32(Reg::R8, 8);
    code.cmp_rr(Reg::R8, Reg::R12);
    code.jcc_label(0x83, digit_inner); // jae: more words below
    code.mov_mem_r8(Reg::R11, 0, Reg::Rdx); // digits[D_total] = digit
    code.add_r_imm8(Reg::R11, 1);
    // Shrink len to the highest nonzero word (0 when the number is 0).
    code.mov_rr(Reg::R9, Reg::R14);
    code.bind_label(digit_shrink);
    code.test_rr(Reg::R9, Reg::R9);
    code.jcc_label(0x84, digit_shrink_done); // jz: all zero
    code.mov_rr(Reg::Rax, Reg::R9);
    code.shl_r_imm8(Reg::Rax, 3);
    code.add_rr(Reg::Rax, Reg::R12);
    code.sub_r_imm32(Reg::Rax, 8);
    code.mov_r_mem(Reg::Rcx, Reg::Rax, 0);
    code.test_rr(Reg::Rcx, Reg::Rcx);
    code.jcc_label(0x85, digit_shrink_done); // jnz: top word nonzero
    code.dec_r(Reg::R9);
    code.jmp_label(digit_shrink);
    code.bind_label(digit_shrink_done);
    code.mov_rr(Reg::R14, Reg::R9);
    code.jmp_label(digit_outer);
    code.bind_label(digits_done);
    // D_total = r11 - r13; E = D_total - 1 - N.
    code.mov_rr(Reg::Rax, Reg::R11);
    code.sub_rr(Reg::Rax, Reg::R13);
    code.mov_mem_r(Reg::Rbp, -24, Reg::Rax);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -24);
    code.dec_r(Reg::Rax);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -40); // N
    code.sub_rr(Reg::Rax, Reg::Rcx);
    code.mov_mem_r(Reg::Rbp, -32, Reg::Rax); // E

    // --- round the 17 significant digits (half-even) ---
    // Rounding digit index = D_total - 18 (absent below zero); sticky is
    // any nonzero digit below it; the tie checks the 17th digit's parity.
    let no_round = code.label();
    let round_up = code.label();
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -24);
    code.sub_r_imm32(Reg::Rax, 18);
    code.jcc_label(0x88, no_round); // js: D_total < 18
    code.mov_rr(Reg::R8, Reg::R13);
    code.add_rr(Reg::R8, Reg::Rax); // round digit pointer
    code.mov_rr(Reg::R9, Reg::R8);
    code.add_r_imm8(Reg::R9, 1); // 17th digit pointer
    let sticky_scan = code.label();
    let sticky_set = code.label();
    let sticky_done = code.label();
    code.xor_rr32(Reg::R10, Reg::R10); // sticky = 0
    code.mov_rr(Reg::Rax, Reg::R13); // scan pointer
    code.bind_label(sticky_scan);
    code.cmp_rr(Reg::Rax, Reg::R8);
    code.jcc_label(0x83, sticky_done); // jae: scanned everything
    code.cmp_mem_imm8(Reg::Rax, 0, 0);
    code.jcc_label(0x85, sticky_set); // jnz: nonzero below
    code.add_r_imm8(Reg::Rax, 1);
    code.jmp_label(sticky_scan);
    code.bind_label(sticky_set);
    code.mov_r32_imm32(Reg::R10, 1);
    code.bind_label(sticky_done);
    code.movzx_byte(Reg::Rax, Reg::R8, 0); // round digit
    code.cmp_r_imm8(Reg::Rax, 5);
    code.jcc_label(0x87, round_up); // ja
    code.jcc_label(0x82, no_round); // jb
    // == 5: round up iff sticky || the 17th digit is odd.
    code.test_rr(Reg::R10, Reg::R10);
    code.jcc_label(0x85, round_up);
    code.movzx_byte(Reg::Rax, Reg::R9, 0);
    code.test_r_imm32(Reg::Rax, 1);
    code.jcc_label(0x85, round_up); // jnz: odd
    code.jmp_label(no_round);
    // Increment the 17-digit significand (least significant first),
    // propagating the carry; leaving the top is an exponent bump.
    let inc_loop = code.label();
    let inc_no_carry = code.label();
    let inc_done = code.label();
    let overflow = code.label();
    code.bind_label(round_up);
    code.mov_rr(Reg::Rax, Reg::R9); // start at the 17th digit
    code.mov_r32_imm32(Reg::Rdx, 1); // carry
    code.bind_label(inc_loop);
    code.movzx_byte(Reg::Rcx, Reg::Rax, 0);
    code.add_rr(Reg::Rcx, Reg::Rdx); // sum
    code.xor_rr32(Reg::Rdx, Reg::Rdx);
    code.cmp_r_imm8(Reg::Rcx, 10);
    code.jcc_label(0x82, inc_no_carry); // jb: no carry
    code.mov_r32_imm32(Reg::Rdx, 1);
    code.sub_r_imm32(Reg::Rcx, 10);
    code.bind_label(inc_no_carry);
    code.mov_mem_r8(Reg::Rax, 0, Reg::Rcx);
    code.mov_rr(Reg::Rsi, Reg::R13);
    code.add_rr(Reg::Rsi, Reg::R11);
    code.sub_rr(Reg::Rsi, Reg::R13); // top ptr = r13 + D_total - 1
    code.dec_r(Reg::Rsi);
    code.cmp_rr(Reg::Rax, Reg::Rsi);
    code.jcc_label(0x83, inc_done); // jae: top processed
    code.add_r_imm8(Reg::Rax, 1);
    code.jmp_label(inc_loop);
    code.bind_label(inc_done);
    code.test_rr(Reg::Rdx, Reg::Rdx);
    code.jcc_label(0x84, no_round); // jz: carried inside
    // Carried past the top: the value is now exactly 10^(E+1); the
    // rounded digits are all zero, so write 10^E directly.
    code.bind_label(overflow);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -32);
    code.add_r_imm8(Reg::Rax, 1);
    code.mov_mem_r(Reg::Rbp, -32, Reg::Rax);
    let write_pow10 = code.label();
    code.jmp_label(write_pow10);

    // --- formatting ---
    // The output is assembled at print_buf[out ..]; r9 is the write
    // pointer, r15 the offset, r13 the digits, D_total/E/N in spills.
    let format_dispatch = code.label();
    let scientific = code.label();
    let fixed_neg = code.label();
    code.bind_label(no_round);
    code.bind_label(format_dispatch);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -32);
    code.cmp_r_imm32(Reg::Rax, (-4i32) as u32);
    code.jcc_label(0x8C, scientific); // jl: E < -4
    code.cmp_r_imm32(Reg::Rax, 17);
    code.jcc_label(0x8D, scientific); // jge: E >= 17
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x88, fixed_neg); // js: -4 <= E < 0
    // Fixed notation, E >= 0: integer digits then a trimmed fraction.
    code.lea_r_rip(Reg::R9, PatchKind::Bss(BSS.print_buf as u32));
    code.add_rr(Reg::R9, Reg::R15);
    code.mov_rr(Reg::R8, Reg::R13);
    code.add_rr(Reg::R8, Reg::R11);
    code.sub_rr(Reg::R8, Reg::R13); // digits top ptr
    code.dec_r(Reg::R8);
    let int_loop = code.label();
    let finish = code.label();
    code.mov_rr(Reg::R10, Reg::Rax); // count = E + 1
    code.add_r_imm8(Reg::R10, 1);
    code.bind_label(int_loop);
    code.movzx_byte(Reg::Rax, Reg::R8, 0);
    code.add_r_imm8(Reg::Rax, b'0');
    code.mov_mem_r8(Reg::R9, 0, Reg::Rax);
    code.add_r_imm8(Reg::R9, 1);
    code.dec_r(Reg::R8);
    code.dec_r(Reg::R10);
    code.jcc_label(0x85, int_loop); // jnz
    // Fraction: digits at indices N-1 down to N+E-16, trailing zeros
    // trimmed; absent when the range is empty or all zero.
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -40); // N
    code.mov_rr(Reg::Rax, Reg::R13);
    code.add_rr(Reg::Rax, Reg::Rcx); // digits + N
    code.mov_rr(Reg::Rcx, Reg::Rax);
    code.dec_r(Reg::Rcx); // high ptr
    code.mov_rr(Reg::Rdx, Reg::R11);
    code.sub_r_imm32(Reg::Rdx, 17); // low ptr = r13 + D_total - 17
    let frac_low_ok = code.label();
    code.cmp_rr(Reg::Rdx, Reg::R13);
    code.jcc_label(0x83, frac_low_ok);
    code.mov_rr(Reg::Rdx, Reg::R13);
    code.bind_label(frac_low_ok);
    code.cmp_rr(Reg::Rcx, Reg::Rdx);
    code.jcc_label(0x82, finish); // jb: empty fraction
    // Trim trailing zeros: digits are stored least-significant first, so
    // the last digit to write is the LOWEST nonzero, found by scanning
    // from the low bound upward.
    let frac_trim = code.label();
    let frac_found = code.label();
    code.mov_rr(Reg::R8, Reg::Rdx); // scan from the low bound up
    code.bind_label(frac_trim);
    code.cmp_mem8_imm8(Reg::R8, 0, 0);
    code.jcc_label(0x85, frac_found); // jnz: lowest nonzero
    code.add_r_imm8(Reg::R8, 1);
    code.cmp_rr(Reg::R8, Reg::Rcx);
    code.jcc_label(0x86, frac_trim); // jbe: keep scanning (incl. high bound)
    code.jmp_label(finish); // all zero: no fraction
    code.bind_label(frac_found);
    code.mov_rr(Reg::R11, Reg::R8); // stop ptr
    code.mov_rr(Reg::R8, Reg::Rcx); // restart at high
    code.mov_mem_imm8(Reg::R9, 0, b'.');
    code.add_r_imm8(Reg::R9, 1);
    let frac_write = code.label();
    code.bind_label(frac_write);
    code.movzx_byte(Reg::Rax, Reg::R8, 0);
    code.add_r_imm8(Reg::Rax, b'0');
    code.mov_mem_r8(Reg::R9, 0, Reg::Rax);
    code.add_r_imm8(Reg::R9, 1);
    code.cmp_rr(Reg::R8, Reg::R11);
    code.jcc_label(0x86, finish); // jbe: stop reached
    code.dec_r(Reg::R8);
    code.jmp_label(frac_write);
    // Fixed notation, -4 <= E < 0: 0.00…digits.
    code.bind_label(fixed_neg);
    code.lea_r_rip(Reg::R9, PatchKind::Bss(BSS.print_buf as u32));
    code.add_rr(Reg::R9, Reg::R15);
    code.mov_mem_imm8(Reg::R9, 0, b'0');
    code.add_r_imm8(Reg::R9, 1);
    code.mov_mem_imm8(Reg::R9, 0, b'.');
    code.add_r_imm8(Reg::R9, 1);
    code.neg_r(Reg::Rax); // zeros count = -E - 1
    code.dec_r(Reg::Rax);
    let fneg_zero_loop = code.label();
    let fneg_digits = code.label();
    code.bind_label(fneg_zero_loop);
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x84, fneg_digits); // jz
    code.mov_mem_imm8(Reg::R9, 0, b'0');
    code.add_r_imm8(Reg::R9, 1);
    code.dec_r(Reg::Rax);
    code.jmp_label(fneg_zero_loop);
    code.bind_label(fneg_digits);
    code.mov_rr(Reg::R8, Reg::R13);
    code.add_rr(Reg::R8, Reg::R11);
    code.sub_rr(Reg::R8, Reg::R13);
    code.dec_r(Reg::R8); // high ptr
    code.mov_rr(Reg::Rcx, Reg::R13);
    code.add_rr(Reg::Rcx, Reg::R11);
    code.sub_rr(Reg::Rcx, Reg::R13);
    code.sub_r_imm32(Reg::Rcx, 17);
    let fneg_low_ok = code.label();
    code.cmp_rr(Reg::Rcx, Reg::R13);
    code.jcc_label(0x83, fneg_low_ok);
    code.mov_rr(Reg::Rcx, Reg::R13);
    code.bind_label(fneg_low_ok);
    // Trim trailing zeros: scan from the low bound up for the lowest
    // nonzero digit, then write from the high bound down to it.
    code.mov_rr(Reg::R10, Reg::Rcx); // scan from the low bound up
    let fneg_trim = code.label();
    let fneg_found = code.label();
    code.bind_label(fneg_trim);
    code.cmp_mem8_imm8(Reg::R10, 0, 0);
    code.jcc_label(0x85, fneg_found); // jnz: lowest nonzero
    code.add_r_imm8(Reg::R10, 1);
    code.cmp_rr(Reg::R10, Reg::R8);
    code.jcc_label(0x86, fneg_trim); // jbe: keep scanning (incl. high bound)
    code.jmp_label(finish); // all zero (unreachable for f != 0)
    code.bind_label(fneg_found);
    code.mov_rr(Reg::R11, Reg::R10);
    let fneg_write = code.label();
    code.bind_label(fneg_write);
    code.movzx_byte(Reg::Rax, Reg::R8, 0);
    code.add_r_imm8(Reg::Rax, b'0');
    code.mov_mem_r8(Reg::R9, 0, Reg::Rax);
    code.add_r_imm8(Reg::R9, 1);
    code.cmp_rr(Reg::R8, Reg::R11);
    code.jcc_label(0x86, finish);
    code.dec_r(Reg::R8);
    code.jmp_label(fneg_write);
    // Scientific: d[.dddd]e±XX.
    code.bind_label(scientific);
    code.lea_r_rip(Reg::R9, PatchKind::Bss(BSS.print_buf as u32));
    code.add_rr(Reg::R9, Reg::R15);
    code.mov_rr(Reg::R8, Reg::R13);
    code.add_rr(Reg::R8, Reg::R11);
    code.sub_rr(Reg::R8, Reg::R13);
    code.dec_r(Reg::R8); // leading digit ptr
    code.movzx_byte(Reg::Rax, Reg::R8, 0);
    code.add_r_imm8(Reg::Rax, b'0');
    code.mov_mem_r8(Reg::R9, 0, Reg::Rax);
    code.add_r_imm8(Reg::R9, 1);
    code.dec_r(Reg::R8);
    code.mov_rr(Reg::Rcx, Reg::R13);
    code.add_rr(Reg::Rcx, Reg::R11);
    code.sub_rr(Reg::Rcx, Reg::R13);
    code.sub_r_imm32(Reg::Rcx, 17);
    let sci_low_ok = code.label();
    code.cmp_rr(Reg::Rcx, Reg::R13);
    code.jcc_label(0x83, sci_low_ok);
    code.mov_rr(Reg::Rcx, Reg::R13);
    code.bind_label(sci_low_ok);
    // Trim trailing zeros: scan from the low bound up for the lowest
    // nonzero digit, then write from the fraction top down to it.
    code.mov_rr(Reg::R10, Reg::Rcx); // scan from the low bound up
    let sci_trim = code.label();
    let sci_found = code.label();
    let sci_exp = code.label();
    code.bind_label(sci_trim);
    code.cmp_mem8_imm8(Reg::R10, 0, 0);
    code.jcc_label(0x85, sci_found);
    code.add_r_imm8(Reg::R10, 1);
    code.cmp_rr(Reg::R10, Reg::R8);
    code.jcc_label(0x86, sci_trim); // jbe: keep scanning (incl. high bound)
    code.jmp_label(sci_exp); // all zero: no fraction
    code.bind_label(sci_found);
    code.mov_rr(Reg::R11, Reg::R10); // stop ptr (r8 is still the fraction top)
    code.mov_mem_imm8(Reg::R9, 0, b'.');
    code.add_r_imm8(Reg::R9, 1);
    let sci_write = code.label();
    code.bind_label(sci_write);
    code.movzx_byte(Reg::Rax, Reg::R8, 0);
    code.add_r_imm8(Reg::Rax, b'0');
    code.mov_mem_r8(Reg::R9, 0, Reg::Rax);
    code.add_r_imm8(Reg::R9, 1);
    code.cmp_rr(Reg::R8, Reg::R11);
    code.jcc_label(0x86, sci_exp);
    code.dec_r(Reg::R8);
    code.jmp_label(sci_write);
    // Exponent: 'e' + sign + unpadded digits of |E|.
    let sci_pos = code.label();
    let sci_exp_digits = code.label();
    let sci_no_h = code.label();
    let sci_ones = code.label();
    let sci_tens_written = code.label();
    code.bind_label(sci_exp);
    code.mov_mem_imm8(Reg::R9, 0, b'e');
    code.add_r_imm8(Reg::R9, 1);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -32);
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x89, sci_pos); // jns: positive — write '+'
    code.mov_mem_imm8(Reg::R9, 0, b'-');
    code.add_r_imm8(Reg::R9, 1);
    code.neg_r(Reg::Rax);
    code.jmp_label(sci_exp_digits);
    code.bind_label(sci_pos);
    code.mov_mem_imm8(Reg::R9, 0, b'+');
    code.add_r_imm8(Reg::R9, 1);
    code.bind_label(sci_exp_digits);
    code.mov_r32_imm32(Reg::Rcx, 100);
    code.xor_rr32(Reg::Rdx, Reg::Rdx);
    code.div_r(Reg::Rcx); // rax = E/100, rdx = E%100
    code.mov_rr(Reg::R10, Reg::Rax); // hundreds
    code.mov_rr(Reg::R11, Reg::Rdx); // remainder
    code.test_rr(Reg::R10, Reg::R10);
    code.jcc_label(0x84, sci_no_h);
    code.add_r_imm8(Reg::R10, b'0');
    code.mov_mem_r8(Reg::R9, 0, Reg::R10);
    code.add_r_imm8(Reg::R9, 1);
    code.bind_label(sci_no_h);
    code.mov_rr(Reg::Rax, Reg::R11);
    code.mov_r32_imm32(Reg::Rcx, 10);
    code.xor_rr32(Reg::Rdx, Reg::Rdx);
    code.div_r(Reg::Rcx); // rax = tens, rdx = ones
    code.mov_rr(Reg::R11, Reg::Rdx);
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x85, sci_tens_written);
    code.test_rr(Reg::R10, Reg::R10); // hmm — r10 = hundreds (nonzero iff written)
    code.jcc_label(0x84, sci_ones); // jz: no hundreds written
    code.mov_mem_imm8(Reg::R9, 0, b'0');
    code.add_r_imm8(Reg::R9, 1);
    code.jmp_label(sci_ones);
    code.bind_label(sci_tens_written);
    code.add_r_imm8(Reg::Rax, b'0');
    code.mov_mem_r8(Reg::R9, 0, Reg::Rax);
    code.add_r_imm8(Reg::R9, 1);
    code.bind_label(sci_ones);
    code.mov_rr(Reg::Rax, Reg::R11);
    code.add_r_imm8(Reg::Rax, b'0');
    code.mov_mem_r8(Reg::R9, 0, Reg::Rax);
    code.add_r_imm8(Reg::R9, 1);
    code.jmp_label(finish);
    // The value is exactly 10^E (a rounding overflow): 1 + zeros.
    code.bind_label(write_pow10);
    code.lea_r_rip(Reg::R9, PatchKind::Bss(BSS.print_buf as u32));
    code.add_rr(Reg::R9, Reg::R15);
    code.mov_mem_imm8(Reg::R9, 0, b'1');
    code.add_r_imm8(Reg::R9, 1);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -32);
    let pow_sci = code.label();
    let pow_neg = code.label();
    code.cmp_r_imm32(Reg::Rax, 17);
    code.jcc_label(0x8D, pow_sci); // jge
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x88, pow_neg); // js
    let pow_zero_loop = code.label();
    code.bind_label(pow_zero_loop);
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x84, finish); // jz
    code.mov_mem_imm8(Reg::R9, 0, b'0');
    code.add_r_imm8(Reg::R9, 1);
    code.dec_r(Reg::Rax);
    code.jmp_label(pow_zero_loop);
    code.bind_label(pow_neg);
    code.mov_mem_imm8(Reg::R9, 0, b'0');
    code.add_r_imm8(Reg::R9, 1);
    code.mov_mem_imm8(Reg::R9, 0, b'.');
    code.add_r_imm8(Reg::R9, 1);
    code.neg_r(Reg::Rax);
    code.dec_r(Reg::Rax);
    let pow_neg_zero_loop = code.label();
    let pow_neg_one = code.label();
    code.bind_label(pow_neg_zero_loop);
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x84, pow_neg_one);
    code.mov_mem_imm8(Reg::R9, 0, b'0');
    code.add_r_imm8(Reg::R9, 1);
    code.dec_r(Reg::Rax);
    code.jmp_label(pow_neg_zero_loop);
    code.bind_label(pow_neg_one);
    code.mov_mem_imm8(Reg::R9, 0, b'1');
    code.add_r_imm8(Reg::R9, 1);
    code.jmp_label(finish);
    code.bind_label(pow_sci);
    code.mov_mem_imm8(Reg::R9, 0, b'e');
    code.add_r_imm8(Reg::R9, 1);
    code.mov_mem_imm8(Reg::R9, 0, b'+');
    code.add_r_imm8(Reg::R9, 1);
    code.jmp_label(sci_exp_digits); // E' >= 17: |E'| is 2-3 digits

    // --- write the assembled output and CRLF ---
    code.bind_label(finish);
    code.lea_r_rip(Reg::Rax, PatchKind::Bss(BSS.print_buf as u32));
    code.mov_rr(Reg::Rcx, Reg::Rax);
    code.mov_rr(Reg::Rdx, Reg::R9);
    code.sub_rr(Reg::Rdx, Reg::Rax);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::WriteStdout));
    code.lea_r_rip(Reg::Rcx, PatchKind::Label(offsets.crlf_label));
    code.mov_r32_imm32(Reg::Rdx, 2);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::WriteStdout));
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();

    // Zero value (including -0.0): "0" at the output offset.
    code.bind_label(print_zero);
    code.lea_r_rip(Reg::R9, PatchKind::Bss(BSS.print_buf as u32));
    code.add_rr(Reg::R9, Reg::R15);
    code.mov_mem_imm8(Reg::R9, 0, b'0');
    code.add_r_imm8(Reg::R9, 1);
    code.jmp_label(finish);
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

// ---------------------------------------------------------------------------
// Vec services (Session 41)
// ---------------------------------------------------------------------------

// Buffer layout: [capacity: 8 bytes][length: 8 bytes][element_0][element_1]...
// Each element is one word (8 bytes). Total allocation = (2 + capacity) * 8.

/// `rt_vec_new(capacity) -> data_ptr` (capacity at [rbp + 16]).
///
/// Allocates a zero-initialized Vec buffer with the given capacity.
/// Returns a pointer to the buffer. The buffer stores capacity at
/// offset 0, length (initially 0) at offset 8, and elements starting
/// at offset 16.
fn emit_vec_new(code: &mut Code) {
    prologue(code);
    // rsp = rbp - 8.
    // Need to call rt_alloc(size) where size = (2 + capacity) * 8.
    // We need rsp to be 16-byte aligned at the call site.
    code.sub_rsp(24); // rsp = rbp - 32 (16-byte aligned)

    // rax = capacity from [rbp + 16].
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    // Validate: capacity must be > 0.
    code.test_rr(Reg::Rax, Reg::Rax);
    let bad = code.label();
    code.jcc_label(0x8E, bad); // jle

    // rax = (2 + capacity) * 8.
    code.add_r_imm8(Reg::Rax, 2);
    code.shl_r_imm8(Reg::Rax, 3); // rax *= 8

    // Place size argument on the stack for rt_alloc.
    // Convention: sub_rsp(8) for padding (1 arg = odd), then store arg at [rsp].
    code.sub_rsp(8);
    code.mov_mem_r(Reg::Rsp, 0, Reg::Rax);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Alloc));
    code.add_rsp(16); // clean up: 8 (pad) + 8 (arg)
    // rax = allocated buffer pointer.

    // Initialize header: [rax+0] = capacity, [rax+8] = 0 (length).
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, 16); // capacity
    code.mov_mem_r(Reg::Rax, 0, Reg::Rcx);
    code.mov_mem_imm32(Reg::Rax, 8, 0); // length = 0

    code.add_rsp(24);
    code.leave_ret();

    code.bind_label(bad);
    fail(code, 8); // E-R08 (invalid size)
}

/// `rt_vec_push(data, value) -> data_ptr`
/// (data at [rbp + 16], value at [rbp + 24]).
///
/// Pushes `value` onto the end of the Vec buffer. If the buffer is
/// full (length == capacity), reallocates with double capacity, copies
/// elements, and frees the old buffer. Returns the (possibly new) data
/// pointer.
///
/// Stack layout (after sub_rsp(32)):
///   [rbp-8]  = data ptr (updated if reallocated)
///   [rbp-16] = value to push
///   [rbp-24] = old data ptr (for free after realloc)
///   [rbp-32] = unused (padding)
fn emit_vec_push(code: &mut Code) {
    prologue(code);
    code.sub_rsp(32);
    // Save args to spill slots.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // data ptr
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 24); // value
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax);

    let no_realloc = code.label();
    let store_value = code.label();

    // Check if realloc needed: length == capacity?
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8); // data ptr
    code.mov_r_mem(Reg::Rcx, Reg::Rax, 0); // capacity
    code.mov_r_mem(Reg::Rdx, Reg::Rax, 8); // length
    code.cmp_rr(Reg::Rdx, Reg::Rcx);
    code.jcc_label(0x8C, no_realloc); // jl (length < capacity)

    // --- Reallocation ---
    // Save old data ptr.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.mov_mem_r(Reg::Rbp, -24, Reg::Rax);

    // new_size = (2 + capacity * 2) * 8.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8); // data ptr
    code.mov_r_mem(Reg::Rcx, Reg::Rax, 0); // capacity
    code.shl_r_imm8(Reg::Rcx, 1); // capacity * 2
    code.add_r_imm8(Reg::Rcx, 2); // + 2
    code.shl_r_imm8(Reg::Rcx, 3); // * 8

    // Call rt_alloc(new_size).
    code.sub_rsp(24);
    code.sub_rsp(8);
    code.mov_mem_r(Reg::Rsp, 0, Reg::Rcx);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Alloc));
    code.add_rsp(16);
    code.add_rsp(24);
    // rax = new data ptr.
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);

    // Copy header: new_cap = old_cap * 2, length = old length.
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8); // new data ptr
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, -24); // old data ptr
    code.mov_r_mem(Reg::Rax, Reg::Rdx, 0); // old capacity
    code.shl_r_imm8(Reg::Rax, 1); // new cap = old cap * 2
    code.mov_mem_r(Reg::Rcx, 0, Reg::Rax);
    code.mov_r_mem(Reg::Rax, Reg::Rdx, 8); // old length
    code.mov_mem_r(Reg::Rcx, 8, Reg::Rax);

    // Copy elements loop: i = 0..length.
    // R8 = i, Rax = length, Rdx = old ptr, Rcx = new ptr.
    code.test_rr(Reg::Rax, Reg::Rax);
    let loop_exit = code.label();
    code.jcc_label(0x84, loop_exit); // jz (length == 0)
    code.mov_r32_imm32(Reg::R8, 0);
    let loop_top = code.label();
    code.bind_label(loop_top);
    code.cmp_rr(Reg::R8, Reg::Rax);
    code.jcc_label(0x8D, loop_exit); // jge
    // offset = 16 + i * 8.
    code.mov_rr(Reg::R9, Reg::R8);
    code.shl_r_imm8(Reg::R9, 3);
    code.add_r_imm8(Reg::R9, 16);
    // src = old + offset.
    code.mov_rr(Reg::R10, Reg::Rdx);
    code.add_rr(Reg::R10, Reg::R9);
    code.mov_r_mem(Reg::R11, Reg::R10, 0);
    // dst = new + offset.
    code.mov_rr(Reg::R10, Reg::Rcx);
    code.add_rr(Reg::R10, Reg::R9);
    code.mov_mem_r(Reg::R10, 0, Reg::R11);
    code.add_r_imm8(Reg::R8, 1);
    code.jmp_label(loop_top);
    code.bind_label(loop_exit);

    // Free old buffer.
    code.sub_rsp(24);
    code.sub_rsp(8);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -24);
    code.mov_mem_r(Reg::Rsp, 0, Reg::Rax);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Free));
    code.add_rsp(16);
    code.add_rsp(24);
    code.jmp_label(store_value);

    // --- No reallocation ---
    code.bind_label(no_realloc);
    code.jmp_label(store_value);

    // Store value and increment length.
    code.bind_label(store_value);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8); // data ptr (possibly new)
    code.mov_r_mem(Reg::Rcx, Reg::Rax, 8); // length
    // offset = 16 + length * 8.
    code.mov_rr(Reg::Rdx, Reg::Rcx);
    code.shl_r_imm8(Reg::Rdx, 3);
    code.add_r_imm8(Reg::Rdx, 16);
    // Store value at data + offset.
    code.mov_rr(Reg::R10, Reg::Rax);
    code.add_rr(Reg::R10, Reg::Rdx);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16); // value
    code.mov_mem_r(Reg::R10, 0, Reg::Rax);
    // Increment length.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8); // data ptr
    code.mov_r_mem(Reg::Rcx, Reg::Rax, 8); // length
    code.add_r_imm8(Reg::Rcx, 1);
    code.mov_mem_r(Reg::Rax, 8, Reg::Rcx);
    // Return data ptr.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.leave_ret();
}

/// `rt_vec_get(data, index) -> Int`
/// (data at [rbp + 16], index at [rbp + 24]).
///
/// Bounds-checked element access. Returns the element at the given
/// index, or triggers E-R10 (array index out of range) if invalid.
fn emit_vec_get(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);
    // rax = data ptr.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    // rcx = index.
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, 24);
    // Bounds check: index < 0 -> error.
    code.test_rr(Reg::Rcx, Reg::Rcx);
    let oob = code.label();
    code.jcc_label(0x88, oob); // js (negative)
    // rdx = length from [data+8].
    code.mov_r_mem(Reg::Rdx, Reg::Rax, 8);
    // index >= length -> error.
    code.cmp_rr(Reg::Rcx, Reg::Rdx);
    code.jcc_label(0x8D, oob); // jge
    // rax = data + 16 + index * 8.
    code.mov_rr(Reg::Rdx, Reg::Rcx);
    code.shl_r_imm8(Reg::Rdx, 3); // index * 8
    code.add_r_imm8(Reg::Rdx, 16); // + 16
    code.add_rr(Reg::Rax, Reg::Rdx); // data + offset
    code.mov_r_mem(Reg::Rax, Reg::Rax, 0); // load element
    code.add_rsp(8);
    code.leave_ret();

    code.bind_label(oob);
    fail(code, 10); // E-R10 (array index out of range)
}

/// `rt_vec_len(data) -> Int` (data at [rbp + 16]).
///
/// Returns the current length of the Vec.
fn emit_vec_len(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // data ptr
    code.mov_r_mem(Reg::Rax, Reg::Rax, 8); // length from [data+8]
    code.leave_ret();
}

/// `rt_vec_free(data)` (data at [rbp + 16]).
///
/// Frees the Vec buffer by calling rt_free on the data pointer.
fn emit_vec_free(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_mem_r(Reg::Rsp, 0, Reg::Rax);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Free));
    code.add_rsp(8);
    code.leave_ret();
}
