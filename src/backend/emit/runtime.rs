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
use crate::runtime::abi::{BSS, HEAP_SIZE, LIVE_TABLE_BYTES, MAX_LIVE_ALLOCS};
use crate::runtime::error::RuntimeErrorKind;

/// The liveness table size in bytes.
const TABLE_BYTES: i32 = (MAX_LIVE_ALLOCS as u32 * 24) as i32;

// --- Networking BSS (Session 67) ---
const NET_INIT_FLAG: u32 = BSS.wsa_initialized as u32;
const NET_DLL_HANDLE: u32 = BSS.ws2_dll_handle as u32;
const NET_FUNC_TABLE: u32 = BSS.net_func_table as u32;
const NET_RECV_BUF: u32 = BSS.recv_buf as u32;
// --- Crypto BSS (Session 71) ---
const CRYPTO_DLL: u32 = BSS.bcrypt_dll_handle as u32;
const CRYPTO_TABLE: u32 = BSS.crypto_func_table as u32;
const RNG_STATE: u32 = BSS.rng_state as u32;
const ENV_STORAGE: u32 = BSS.env_storage as u32;

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
    emit(code, RuntimeService::IntToFloat, |code, _| {
        emit_int_to_float(code)
    });
    emit(code, RuntimeService::FloatToInt, |code, _| {
        emit_float_to_int(code)
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
    emit(code, RuntimeService::VecSet, |code, _| emit_vec_set(code));
    emit(code, RuntimeService::VecPop, |code, _| emit_vec_pop(code));
    emit(code, RuntimeService::VecRemove, |code, _| {
        emit_vec_remove(code)
    });
    emit(code, RuntimeService::StrConcat, |code, _| {
        emit_str_concat(code)
    });
    emit(code, RuntimeService::StrEq, |code, _| emit_str_eq(code));
    emit(code, RuntimeService::StrFromInt, |code, _| {
        emit_str_from_int(code)
    });
    emit(code, RuntimeService::StrFromBool, |code, _| {
        emit_str_from_bool(code)
    });
    // --- Networking (Session 67) ---
    emit(code, RuntimeService::NetWsaStartup, |code, _| {
        emit_net_wsa_startup(code)
    });
    emit(code, RuntimeService::NetWsaCleanup, |code, _| {
        emit_net_wsa_cleanup(code)
    });
    emit(code, RuntimeService::NetWsaLastError, |code, _| {
        emit_net_wsa_last_error(code)
    });
    emit(code, RuntimeService::NetSocket, |code, _| {
        emit_net_socket(code)
    });
    emit(code, RuntimeService::NetConnect, |code, _| {
        emit_net_connect(code)
    });
    emit(code, RuntimeService::NetBind, |code, _| emit_net_bind(code));
    emit(code, RuntimeService::NetListen, |code, _| {
        emit_net_listen(code)
    });
    emit(code, RuntimeService::NetAccept, |code, _| {
        emit_net_accept(code)
    });
    emit(code, RuntimeService::NetSend, |code, _| emit_net_send(code));
    emit(code, RuntimeService::NetRecv, |code, _| emit_net_recv(code));
    emit(code, RuntimeService::NetClose, |code, _| {
        emit_net_close(code)
    });
    emit(code, RuntimeService::NetShutdown, |code, _| {
        emit_net_shutdown(code)
    });
    emit(code, RuntimeService::NetGetAddrInfo, |code, _| {
        emit_net_get_addr_info(code)
    });
    emit(code, RuntimeService::NetFreeAddrInfo, |code, _| {
        emit_net_free_addr_info(code)
    });
    emit(code, RuntimeService::NetGetHostName, |code, _| {
        emit_net_get_host_name(code)
    });
    emit(code, RuntimeService::NetHtons, |code, _| {
        emit_net_htons(code)
    });
    // --- Crypto (Session 71) ---
    emit(code, RuntimeService::CryptoInit, |code, _| {
        emit_crypto_init(code)
    });
    emit(code, RuntimeService::CryptoRandomBytes, |code, _| {
        emit_crypto_random_bytes(code)
    });
    emit(code, RuntimeService::CryptoRandomInt, |code, _| {
        emit_crypto_random_int(code)
    });
    emit(code, RuntimeService::CryptoSecureZero, |code, _| {
        emit_crypto_secure_zero(code)
    });
    // --- Time (Session 72) ---
    emit(code, RuntimeService::TimeNow, |code, _| emit_time_now(code));
    emit(code, RuntimeService::TimeMillis, |code, _| {
        emit_time_millis(code)
    });
    emit(code, RuntimeService::TimeTicks, |code, _| {
        emit_time_ticks(code)
    });
    emit(code, RuntimeService::TimeFreq, |code, _| {
        emit_time_freq(code)
    });
    emit(code, RuntimeService::TimeFiletime, |code, _| {
        emit_time_filetime(code)
    });
    emit(code, RuntimeService::TimeFiletimeHigh, |code, _| {
        emit_time_filetime_high(code)
    });
    // --- Process (Session 72) ---
    emit(code, RuntimeService::ProcessId, |code, _| {
        emit_process_id(code)
    });
    emit(code, RuntimeService::ProcessRun, |code, _| {
        emit_process_run(code)
    });
    emit(code, RuntimeService::ProcessStdout, |code, _| {
        emit_process_stdout(code)
    });
    emit(code, RuntimeService::ProcessStderr, |code, _| {
        emit_process_stderr(code)
    });
    emit(code, RuntimeService::ProcessStdoutLen, |code, _| {
        emit_process_stdout_len(code)
    });
    emit(code, RuntimeService::ProcessStderrLen, |code, _| {
        emit_process_stderr_len(code)
    });
    // --- Random (Session 73) ---
    emit(code, RuntimeService::RandomSeed, |code, _| {
        emit_random_seed(code)
    });
    emit(code, RuntimeService::RandomNext, |code, _| {
        emit_random_next(code)
    });
    // --- Environment (Session 73) ---
    emit(code, RuntimeService::EnvGet, |code, _| emit_env_get(code));
    emit(code, RuntimeService::EnvSet, |code, _| emit_env_set(code));
    emit(code, RuntimeService::EnvHas, |code, _| emit_env_has(code));
    emit(code, RuntimeService::EnvRemove, |code, _| {
        emit_env_remove(code)
    });
    // --- Filesystem (Session 56) ---
    emit(code, RuntimeService::FsRead, |code, _| emit_fs_read(code));
    emit(code, RuntimeService::FsWrite, |code, _| emit_fs_write(code));
    emit(code, RuntimeService::FsExists, |code, _| {
        emit_fs_exists(code)
    });
    emit(code, RuntimeService::FsFileSize, |code, _| {
        emit_fs_file_size(code)
    });
    emit(code, RuntimeService::FsCreateDir, |code, _| {
        emit_fs_create_dir(code)
    });
    emit(code, RuntimeService::FsRemoveDir, |code, _| {
        emit_fs_remove_dir(code)
    });
    emit(code, RuntimeService::FsRemoveFile, |code, _| {
        emit_fs_remove_file(code)
    });
    emit(code, RuntimeService::FsCopy, |code, _| emit_fs_copy(code));
    emit(code, RuntimeService::FsMove, |code, _| emit_fs_move(code));
    emit(code, RuntimeService::FsGetCwd, |code, _| {
        emit_fs_get_cwd(code)
    });
    emit(code, RuntimeService::FsSetCwd, |code, _| {
        emit_fs_set_cwd(code)
    });
    emit(code, RuntimeService::ToCstr, |code, _| emit_to_cstr(code));
    emit(code, RuntimeService::FreeCstr, |code, _| {
        emit_free_cstr(code)
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
    // Initialize RNG state (non-zero seed for xorshift64*)
    code.movabs(Reg::Rax, 1u64);
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(RNG_STATE));
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
/// `rt_int_to_float(n: Int) -> Float`: convert integer to float.
fn emit_int_to_float(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // n
    // cvtsi2sd xmm0, [rsp] — need to put on stack first
    code.sub_rsp(8);
    code.mov_mem_r(Reg::Rsp, 0, Reg::Rax);
    // F2 REX.W 0F 2A 04 24 = cvtsi2sd xmm0, [rsp]
    code.bytes(&[0xF2, 0x48, 0x0F, 0x2A, 0x04, 0x24]);
    // Store xmm0 back to stack, return as Int bits
    // F2 0F 11 04 24 = movsd [rsp], xmm0
    code.bytes(&[0xF2, 0x0F, 0x11, 0x04, 0x24]);
    code.mov_r_mem(Reg::Rax, Reg::Rsp, 0);
    code.add_rsp(8);
    code.leave_ret();
}

/// `rt_float_to_int(f: Float) -> Int`: truncate float to integer.
fn emit_float_to_int(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // bits
    code.sub_rsp(8);
    code.mov_mem_r(Reg::Rsp, 0, Reg::Rax);
    // F2 0F 10 04 24 = movsd xmm0, [rsp]
    code.bytes(&[0xF2, 0x0F, 0x10, 0x04, 0x24]);
    code.add_rsp(8);
    // F2 REX.W 0F 2C C0 = cvttsd2si rax, xmm0
    code.bytes(&[0xF2, 0x48, 0x0F, 0x2C, 0xC0]);
    code.leave_ret();
}

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

/// `rt_vec_set(data, index, value)` (data at [rbp+16], index at [rbp+24], value at [rbp+32]).
/// Returns the data pointer (for chaining).
fn emit_vec_set(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);
    // rax = data ptr — save it for return value.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // spill data ptr
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
    // Store: data + 16 + index * 8 = value.
    code.mov_rr(Reg::Rdx, Reg::Rcx);
    code.shl_r_imm8(Reg::Rdx, 3); // index * 8
    code.add_r_imm8(Reg::Rdx, 16); // + 16
    code.mov_r_mem(Reg::R10, Reg::Rbp, 32); // value
    code.add_rr(Reg::Rax, Reg::Rdx); // data + offset
    code.mov_mem_r(Reg::Rax, 0, Reg::R10); // store element
    // Return data pointer.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.add_rsp(8);
    code.leave_ret();

    code.bind_label(oob);
    fail(code, 10); // E-R10 (array index out of range)
}

/// `rt_vec_pop(data) -> Int`: Pop last element. Returns the popped value.
fn emit_vec_pop(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // data ptr
    code.mov_r_mem(Reg::Rcx, Reg::Rax, 8); // length
    // Empty check
    let empty = code.label();
    code.test_rr(Reg::Rcx, Reg::Rcx);
    code.jcc_label(0x84, empty); // jz
    // Decrement length
    code.sub_r_imm32(Reg::Rcx, 1);
    code.mov_mem_r(Reg::Rax, 8, Reg::Rcx);
    // Load last element: data + 16 + (length-1) * 8
    code.shl_r_imm8(Reg::Rcx, 3);
    code.add_r_imm8(Reg::Rcx, 16);
    code.add_rr(Reg::Rax, Reg::Rcx);
    code.mov_r_mem(Reg::Rax, Reg::Rax, 0);
    code.leave_ret();
    code.bind_label(empty);
    fail(code, 10); // E-R10
}

/// `rt_vec_remove(data, index) -> Int`: Remove element at index, shift remaining.
fn emit_vec_remove(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16); // [rbp-8]=data ptr, [rbp-16]=saved value
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // data ptr
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, 24); // index
    // Bounds check
    code.test_rr(Reg::Rcx, Reg::Rcx);
    let oob = code.label();
    code.jcc_label(0x88, oob);
    code.mov_r_mem(Reg::Rdx, Reg::Rax, 8); // length
    code.cmp_rr(Reg::Rcx, Reg::Rdx);
    code.jcc_label(0x8D, oob);
    // Save element value: data + 16 + index * 8
    code.mov_rr(Reg::R10, Reg::Rcx);
    code.shl_r_imm8(Reg::R10, 3);
    code.add_r_imm8(Reg::R10, 16);
    code.add_rr(Reg::Rax, Reg::R10);
    code.mov_r_mem(Reg::Rax, Reg::Rax, 0); // load value
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax); // save to stack
    // Shift loop: copy element[i+1] to element[i]
    code.mov_rr(Reg::R10, Reg::Rcx); // i = index
    code.sub_r_imm32(Reg::Rdx, 1); // length - 1
    let shift_loop = code.label();
    let shift_done = code.label();
    code.bind_label(shift_loop);
    code.cmp_rr(Reg::R10, Reg::Rdx);
    code.jcc_label(0x8D, shift_done); // jge
    // Compute src_addr = data + 16 + (i+1)*8
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8); // data ptr
    code.mov_rr(Reg::R9, Reg::R10);
    code.shl_r_imm8(Reg::R9, 3); // i*8
    code.add_r_imm8(Reg::R9, 24); // +24 = 16 + (i+1)*8 offset from data
    code.add_rr(Reg::R9, Reg::Rax); // src_addr = data + 24 + i*8
    code.mov_r_mem(Reg::R11, Reg::R9, 0); // R11 = *src_addr
    // Compute dst_addr = data + 16 + i*8
    code.mov_rr(Reg::R9, Reg::R10);
    code.shl_r_imm8(Reg::R9, 3); // i*8
    code.add_r_imm8(Reg::R9, 16); // +16
    code.add_rr(Reg::R9, Reg::Rax); // dst_addr
    code.mov_mem_r(Reg::R9, 0, Reg::R11); // *dst_addr = src value
    code.add_r_imm8(Reg::R10, 1);
    code.jmp_label(shift_loop);
    code.bind_label(shift_done);
    // Decrement length
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.mov_r_mem(Reg::Rcx, Reg::Rax, 8);
    code.sub_r_imm32(Reg::Rcx, 1);
    code.mov_mem_r(Reg::Rax, 8, Reg::Rcx);
    // Return data pointer (caller reassigns v = result)
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.add_rsp(16);
    code.leave_ret();
    code.bind_label(oob);
    fail(code, 10);
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

// ---------------------------------------------------------------------------
// String operations (Session 44)
// ---------------------------------------------------------------------------

/// `rt_str_concat(a, b) -> Str`
/// (a at [rbp + 16], b at [rbp + 24]): allocate a new string containing
/// the bytes of `a` followed by the bytes of `b`.
///
/// Stack layout:
/// - `[rbp-8]`  = saved a pointer
/// - `[rbp-16]` = saved b pointer
/// - `[rbp-24]` = len_a
/// - `[rbp-32]` = len_b
/// - `[rbp-40]` = new string data pointer
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

    // total = len_a + len_b (check overflow).
    code.movabs(Reg::R10, i64::MAX as u64);
    code.cmp_rr(Reg::R8, Reg::R10);
    let overflow = code.label();
    code.jcc_label(0x87, overflow); // ja (unsigned >)
    code.sub_rr(Reg::R10, Reg::R8); // R10 = MAX - len_a
    code.cmp_rr(Reg::R9, Reg::R10);
    code.jcc_label(0x87, overflow); // ja
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
    code.add_r_imm8(Reg::R10, 8); // a+8 (data start)
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

    code.bind_label(overflow);
    fail(code, 2); // E-R02 (out of memory / overflow)
}

/// `rt_str_eq(a, b) -> Bool`
/// (a at [rbp + 16], b at [rbp + 24]): byte-for-byte comparison.
///
/// Returns 1 (true) when both strings have equal length and content,
/// 0 (false) otherwise.
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
    // rax = b data ptr
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
    // rcx = a+8 (data start of a)
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    code.add_r_imm8(Reg::Rcx, 8);
    // rdx = b+8 (data start of b)
    code.lea_r_mem(Reg::Rdx, Reg::Rax, 8);
    // r8 = len_a (loop counter, decrementing)
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

/// `rt_str_from_int(value) -> Str`
/// (value at [rbp + 16]): decimal representation of the signed integer.
///
/// Stack layout:
/// - `[rbp-8]`  = saved length (across Alloc call)
/// - `[rbp-16]` = saved data start pointer from print_buf
///
/// Strategy: use the 32-byte print_buf as scratch to build digits
/// backwards (same approach as `emit_print_int`), then allocate a string
/// of the correct size and copy. The minus sign is written AFTER all
/// digits so it appears first when reading left-to-right.
fn emit_str_from_int(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);

    // rax = value.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);

    // Cursor starts at print_buf + 31.
    code.lea_r_rip(Reg::R8, PatchKind::Bss(BSS.print_buf as u32));
    code.add_r_imm8(Reg::R8, 31);

    // Handle zero: write '0', decrement cursor, done.
    code.test_rr(Reg::Rax, Reg::Rax);
    let non_zero = code.label();
    code.jcc_label(0x85, non_zero); // jnz
    code.mov_mem_imm8(Reg::R8, 0, b'0');
    code.dec_r(Reg::R8); // cursor now before '0'
    let build_done = code.label();
    code.jmp_label(build_done);

    code.bind_label(non_zero);
    // If negative, negate for digit extraction.
    code.test_rr(Reg::Rax, Reg::Rax);
    let digit_loop = code.label();
    code.jcc_label(0x89, digit_loop); // jns (not negative)
    code.neg_r(Reg::Rax);

    // Digit extraction loop: RAX = |value|, R8 = cursor (writing backwards).
    code.bind_label(digit_loop);
    code.xor_rr32(Reg::Rdx, Reg::Rdx);
    code.mov_r32_imm32(Reg::Rcx, 10);
    code.div_r(Reg::Rcx); // RDX:RAX / 10 -> RAX=quot, RDX=rem
    code.add_r_imm8(Reg::Rdx, b'0');
    code.mov_mem_r8(Reg::R8, 0, Reg::Rdx);
    code.dec_r(Reg::R8);
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x85, digit_loop); // jnz

    // After the digit loop, write '-' for negative values.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // reload original value
    code.test_rr(Reg::Rax, Reg::Rax);
    let after_sign = code.label();
    code.jcc_label(0x89, after_sign); // jns
    code.mov_mem_imm8(Reg::R8, 0, b'-');
    code.dec_r(Reg::R8);
    code.bind_label(after_sign);

    code.bind_label(build_done);
    // R8 points to one byte before the first character.
    // Data start = R8 + 1. Length = (print_buf + 31) - R8.
    code.lea_r_mem(Reg::R9, Reg::R8, 1); // data start
    code.lea_r_rip(Reg::R10, PatchKind::Bss(BSS.print_buf as u32));
    code.add_r_imm8(Reg::R10, 31);
    code.sub_rr(Reg::R10, Reg::R8); // R10 = length (use R8, not R9)

    // Save length and data start before alloc (caller-saved).
    code.mov_mem_r(Reg::Rbp, -8, Reg::R10); // save length
    code.mov_mem_r(Reg::Rbp, -16, Reg::R9); // save data start

    // Allocate 8 + length bytes.
    code.mov_rr(Reg::Rax, Reg::R10);
    code.add_r_imm8(Reg::Rax, 8);
    code.sub_rsp(8);
    code.u8(0x50); // push rax
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Alloc));
    code.add_rsp(16);
    // RAX = new string data pointer.

    // Reload length and data start.
    code.mov_r_mem(Reg::R10, Reg::Rbp, -8); // length
    code.mov_r_mem(Reg::R9, Reg::Rbp, -16); // data start

    // Write length prefix: [rax] = length.
    code.mov_mem_r(Reg::Rax, 0, Reg::R10);

    // Save block start (rax) for the return value.
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax); // overwrite data start slot

    // Copy digits: dst = new+8, src = R9, count = R10.
    code.add_r_imm8(Reg::Rax, 8); // data start of new string
    code.mov_r32_imm32(Reg::R11, 0); // i = 0
    let copy_loop = code.label();
    code.bind_label(copy_loop);
    code.cmp_rr(Reg::R11, Reg::R10);
    let copy_done = code.label();
    code.jcc_label(0x8D, copy_done); // jge
    code.movzx_byte(Reg::R12, Reg::R9, 0); // src[i]
    code.mov_mem_r8(Reg::Rax, 0, Reg::R12); // dst[i]
    code.add_r_imm8(Reg::R9, 1);
    code.add_r_imm8(Reg::Rax, 1);
    code.add_r_imm8(Reg::R11, 1);
    code.jmp_label(copy_loop);

    code.bind_label(copy_done);
    // Return saved block start pointer.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16);
    code.leave_ret();
}

/// `rt_str_from_bool(value) -> Str`
/// (value at [rbp + 16]): produce `"true"` or `"false"`.
///
/// Strategy: allocate a 4-byte or 5-byte string via `rt_str_alloc`, then
/// write each byte directly to the data region (offset +8 from the block
/// start). This avoids repeated calls to `rt_str_set_byte` and keeps the
/// stack management simple.
fn emit_str_from_bool(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8); // [rbp-8] = saved string ptr

    // rax = value.
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.test_rr(Reg::Rax, Reg::Rax);
    let emit_false = code.label();
    code.jcc_label(0x84, emit_false); // je (false)

    // true: allocate 4 bytes.
    code.sub_rsp(8);
    code.mov_r32_imm32(Reg::Rax, 4);
    code.u8(0x50); // push rax
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    // rax = new string ptr. Write 't','r','u','e' at data offsets 8-11.
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // save
    code.mov_mem_imm8(Reg::Rax, 8, b't');
    code.mov_mem_imm8(Reg::Rax, 9, b'r');
    code.mov_mem_imm8(Reg::Rax, 10, b'u');
    code.mov_mem_imm8(Reg::Rax, 11, b'e');
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8); // return ptr
    code.add_rsp(8);
    code.leave_ret();

    code.bind_label(emit_false);
    // false: allocate 5 bytes.
    code.sub_rsp(8);
    code.mov_r32_imm32(Reg::Rax, 5);
    code.u8(0x50); // push rax
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    // rax = new string ptr. Write 'f','a','l','s','e' at offsets 8-12.
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // save
    code.mov_mem_imm8(Reg::Rax, 8, b'f');
    code.mov_mem_imm8(Reg::Rax, 9, b'a');
    code.mov_mem_imm8(Reg::Rax, 10, b'l');
    code.mov_mem_imm8(Reg::Rax, 11, b's');
    code.mov_mem_imm8(Reg::Rax, 12, b'e');
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8); // return ptr
    code.add_rsp(8);
    code.leave_ret();
}
// ===========================================================================
// Networking services (Session 67)
// ===========================================================================

// BSS constants for networking (defined at file top as NET_*)

/// `rt_net_wsa_startup() -> Int`: Load ws2_32.dll, resolve function pointers, call WSAStartup.
/// Returns 0 on success, -1 on error.
fn emit_net_wsa_startup(code: &mut Code) {
    prologue(code);
    // Check if already initialized
    let already = code.label();
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_INIT_FLAG));
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x85, already); // jnz

    // Load ws2_32.dll if not already loaded
    let have_dll = code.label();
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_DLL_HANDLE));
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x85, have_dll); // jnz

    // Write "ws2_32.dll\0" to temp area in recv_buf
    // NET_RECV_BUF is used as temp during init (safe: no active sockets yet)
    let dll_name_off = NET_RECV_BUF;
    code.movabs(Reg::Rax, u64::from_le_bytes(*b"ws2_32.d"));
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(dll_name_off));
    code.movabs(Reg::Rax, 0x0000_0000_0000_6C6Cu64); // "ll\0..."
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(dll_name_off + 8));
    // LoadLibraryA(&dll_name)
    code.lea_r_rip(Reg::Rcx, PatchKind::Bss(dll_name_off));
    code.sub_rsp(32);
    code.call_rip(PatchKind::Iat(32)); // LoadLibraryA
    code.add_rsp(32);
    code.test_rr(Reg::Rax, Reg::Rax);
    let dll_fail = code.label();
    code.jcc_label(0x84, dll_fail); // jz: LoadLibraryA failed
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(NET_DLL_HANDLE));

    code.bind_label(have_dll);

    // Resolve 15 function pointers from ws2_32.dll
    // Names go into recv_buf temp area (offsets 0..320, 20 bytes each)
    // Function pointers go into NET_FUNC_TABLE (17 * 8 = 136 bytes)
    // We'll resolve 15 unique functions, then copy htons to ntohs slot

    let names: [&[u8]; 16] = [
        b"WSAStartup\0",
        b"WSACleanup\0",
        b"WSAGetLastError",
        b"WSASocketA\0\0",
        b"connect\0\0\0",
        b"bind\0\0\0\0",
        b"listen\0\0\0\0",
        b"accept\0\0\0\0",
        b"send\0\0\0\0\0",
        b"recv\0\0\0\0\0",
        b"closesocket\0",
        b"shutdown\0\0\0",
        b"getaddrinfo\0",
        b"freeaddrinfo\0",
        b"gethostname\0",
        b"htons\0\0\0\0",
    ];

    let name_size: u32 = 20;
    // Write each name and resolve it
    for (i, name_bytes) in names.iter().enumerate() {
        let name_off = NET_RECV_BUF + (i as u32) * name_size;
        // Write name in 8-byte chunks
        for chunk_off in (0..name_bytes.len()).step_by(8) {
            let mut buf = [0u8; 8];
            let end = (chunk_off + 8).min(name_bytes.len());
            buf[..end - chunk_off].copy_from_slice(&name_bytes[chunk_off..end]);
            code.movabs(Reg::Rax, u64::from_le_bytes(buf));
            code.mov_rip_r(Reg::Rax, PatchKind::Bss(name_off + chunk_off as u32));
        }
        // GetProcAddress(dll_handle, &name)
        code.mov_r_rip(Reg::Rcx, PatchKind::Bss(NET_DLL_HANDLE));
        code.lea_r_rip(Reg::Rdx, PatchKind::Bss(name_off));
        code.sub_rsp(32);
        code.call_rip(PatchKind::Iat(33)); // GetProcAddress
        code.add_rsp(32);
        // Store result
        let slot = NET_FUNC_TABLE + (i as u32) * 8;
        code.mov_rip_r(Reg::Rax, PatchKind::Bss(slot));
    }

    // Slot 16 (ntohs) = copy from slot 15 (htons) — they're the same function
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 15 * 8));
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 16 * 8));

    // Call WSAStartup(MAKEWORD(2,2), &wsadata)
    // WSADATA is 408 bytes, stored at NET_RECV_BUF as temp
    // Already cleared (zero-init BSS)
    let wsadata_off = NET_RECV_BUF;
    code.lea_r_rip(Reg::Rdx, PatchKind::Bss(wsadata_off)); // lpWSAData
    code.mov_r32_imm32(Reg::Rcx, 0x0202); // MAKEWORD(2,2)
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE)); // WSAStartup ptr
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);

    // Check result (WSAStartup returns 0 on success, non-zero on error)
    code.test_rr(Reg::Rax, Reg::Rax);
    let wsa_fail = code.label();
    code.jcc_label(0x85, wsa_fail); // jnz: jump on error (non-zero return)

    // Mark initialized
    code.movabs(Reg::Rax, 1);
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(NET_INIT_FLAG));

    code.bind_label(already);
    code.xor_rr32(Reg::Rax, Reg::Rax); // return 0
    code.leave_ret();

    // WSAStartup failed
    code.bind_label(wsa_fail);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64); // -1
    code.leave_ret();

    // LoadLibraryA failed
    code.bind_label(dll_fail);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFEu64); // -2
    code.leave_ret();
}

/// `rt_net_wsa_cleanup() -> Int`: WSACleanup. Returns 0 on success.
fn emit_net_wsa_cleanup(code: &mut Code) {
    prologue(code);
    let skip = code.label();
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_INIT_FLAG));
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x84, skip); // jz: not initialized
    // WSACleanup()
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 8)); // slot 1 = WSACleanup
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(NET_INIT_FLAG)); // clear flag
    code.bind_label(skip);
    code.xor_rr32(Reg::Rax, Reg::Rax); // return 0
    code.leave_ret();
}

/// `rt_net_wsa_last_error() -> Str`: Return WSAGetLastError as a string.
fn emit_net_wsa_last_error(code: &mut Code) {
    prologue(code);
    // WSAGetLastError() -> int (intrinsic returns Int)
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 16)); // slot 2
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    // Return the integer error code directly
    code.leave_ret();
}

/// `rt_net_socket(af, ty, proto) -> Int`: WSASocketA. Returns socket handle.
fn emit_net_socket(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::R10, Reg::Rbp, 16); // af
    code.mov_r_mem(Reg::R11, Reg::Rbp, 24); // type
    code.mov_r_mem(Reg::R12, Reg::Rbp, 32); // protocol
    // WSASocketA(af, type, protocol, NULL, 0, 0)
    code.mov_rr(Reg::Rcx, Reg::R10); // af
    code.mov_rr(Reg::Rdx, Reg::R11); // type
    code.mov_rr(Reg::R8, Reg::R12); // protocol
    code.xor_rr32(Reg::R9, Reg::R9); // lpProtocolInfo = NULL
    // Stack args: g = 0, flags = 0
    code.sub_rsp(48); // 32 shadow + 16 for 2 stack args
    code.mov_mem_imm32(Reg::Rsp, 32, 0); // g = 0
    code.mov_mem_imm32(Reg::Rsp, 40, 0); // flags = 0
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 24)); // WSASocketA
    code.call_rax();
    code.add_rsp(48);
    code.leave_ret();
}

/// `rt_net_connect(sock, addr, port) -> Int`: Connect to IPv4 address.
/// addr is a Str containing "x.x.x.x". Returns 0 on success, -1 on error.
fn emit_net_connect(code: &mut Code) {
    prologue(code);
    code.sub_rsp(64); // [rbp-16]=sock, [rbp-24]=addr ptr, [rbp-32]=port, [rbp-40..56]=sockaddr_in
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 24);
    code.mov_mem_r(Reg::Rbp, -24, Reg::Rax);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 32);
    code.mov_mem_r(Reg::Rbp, -32, Reg::Rax);
    // Zero-init sockaddr_in
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -40, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -48, Reg::Rax);
    // AF_INET = 2
    code.mov_r32_imm32(Reg::Rax, 2);
    code.mov_mem_r(Reg::Rbp, -40, Reg::Rax);
    // Parse IP: build sin_addr as single 32-bit value in R8
    code.xor_rr32(Reg::R8, Reg::R8); // accumulator = 0
    code.mov_r_mem(Reg::R10, Reg::Rbp, -24);
    code.add_r_imm8(Reg::R10, 8); // skip len prefix
    for octet in 0..4u32 {
        let done_label = code.label();
        code.xor_rr32(Reg::R9, Reg::R9); // octet value = 0
        let digit_loop = code.label();
        code.bind_label(digit_loop);
        code.movzx_byte(Reg::R11, Reg::R10, 0); // R11 = byte
        code.test_rr(Reg::R11, Reg::R11);
        code.jcc_label(0x84, done_label);
        code.cmp_r_imm8(Reg::R11, 0x2E);
        code.jcc_label(0x84, done_label);
        code.sub_r_imm32(Reg::R11, '0' as u32); // R11 = digit
        // R9 = R9 * 10 + R11
        code.mov_r32_imm32(Reg::Rax, 10);
        code.mul_r(Reg::R9); // RAX = 10 * R9
        code.mov_rr(Reg::R9, Reg::Rax);
        code.add_rr(Reg::R9, Reg::R11);
        code.add_r_imm8(Reg::R10, 1);
        code.jmp_label(digit_loop);
        code.bind_label(done_label);
        code.add_r_imm8(Reg::R10, 1);
        // Shift octet into correct position for network byte order
        // (little-endian store: low byte = first octet)
        if octet > 0 {
            code.shl_r_imm8(Reg::R9, (octet * 8) as u8);
        }
        code.add_rr(Reg::R8, Reg::R9);
    }
    // Save sin_addr before htons (R8 is volatile, clobbered by call)
    code.mov_mem_r(Reg::Rbp, -8, Reg::R8);
    // htons(port)
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -32);
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 15 * 8));
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    // RAX = htons result. Restore sin_addr from spill slot.
    code.mov_r_mem(Reg::R8, Reg::Rbp, -8);
    // Store sin_port then sin_addr
    code.mov_mem_r(Reg::Rbp, -38, Reg::Rax); // sin_port
    code.mov_mem_r(Reg::Rbp, -36, Reg::R8); // sin_addr
    // connect(sock, &addr, 16)
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -16);
    code.lea_r_mem(Reg::Rdx, Reg::Rbp, -40);
    code.movabs(Reg::R8, 16);
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 32));
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x84, ok);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_bind(sock, addr, port) -> Int`: Bind to IPv4 address.
fn emit_net_bind(code: &mut Code) {
    prologue(code);
    code.sub_rsp(64);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax); // sock
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 24);
    code.mov_mem_r(Reg::Rbp, -24, Reg::Rax); // addr ptr
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 32);
    code.mov_mem_r(Reg::Rbp, -32, Reg::Rax); // port

    // Zero sockaddr_in
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -40, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -48, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -56, Reg::Rax);
    code.mov_r32_imm32(Reg::Rax, 2);
    code.mov_mem_r(Reg::Rbp, -40, Reg::Rax); // AF_INET

    // Parse IP: build sin_addr as single 32-bit value in R8
    code.xor_rr32(Reg::R8, Reg::R8); // accumulator = 0
    code.mov_r_mem(Reg::R10, Reg::Rbp, -24);
    code.add_r_imm8(Reg::R10, 8); // skip len prefix
    for octet in 0..4u32 {
        let done_label = code.label();
        code.xor_rr32(Reg::R9, Reg::R9); // octet value = 0
        let digit_loop = code.label();
        code.bind_label(digit_loop);
        code.movzx_byte(Reg::R11, Reg::R10, 0); // R11 = byte
        code.test_rr(Reg::R11, Reg::R11);
        code.jcc_label(0x84, done_label);
        code.cmp_r_imm8(Reg::R11, 0x2E);
        code.jcc_label(0x84, done_label);
        code.sub_r_imm32(Reg::R11, '0' as u32); // R11 = digit
        code.mov_r32_imm32(Reg::Rax, 10);
        code.mul_r(Reg::R9); // RAX = 10 * R9
        code.mov_rr(Reg::R9, Reg::Rax);
        code.add_rr(Reg::R9, Reg::R11);
        code.add_r_imm8(Reg::R10, 1);
        code.jmp_label(digit_loop);
        code.bind_label(done_label);
        code.add_r_imm8(Reg::R10, 1);
        // Shift octet into correct position for network byte order
        if octet > 0 {
            code.shl_r_imm8(Reg::R9, (octet * 8) as u8);
        }
        code.add_rr(Reg::R8, Reg::R9);
    }
    // Save sin_addr before htons (R8 is volatile, clobbered by call)
    code.mov_mem_r(Reg::Rbp, -8, Reg::R8);
    // htons(port)
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -32);
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 15 * 8));
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    // RAX = htons result. Restore sin_addr from spill slot.
    code.mov_r_mem(Reg::R8, Reg::Rbp, -8);
    // Store sin_port then sin_addr
    code.mov_mem_r(Reg::Rbp, -38, Reg::Rax); // sin_port
    code.mov_mem_r(Reg::Rbp, -36, Reg::R8); // sin_addr

    // bind(sock, &addr, 16)
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -16);
    code.lea_r_mem(Reg::Rdx, Reg::Rbp, -40);
    code.movabs(Reg::R8, 16);
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 40)); // bind
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x84, ok); // jz: jump if zero (success)
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_listen(sock, backlog) -> Int`: Start listening.
fn emit_net_listen(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, 16); // sock
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, 24); // backlog
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 48)); // listen
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x84, ok);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_accept(sock) -> Int`: Accept connection. Returns new socket or -1.
fn emit_net_accept(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, 16); // sock
    code.xor_rr32(Reg::Rdx, Reg::Rdx); // addr = NULL
    code.xor_rr32(Reg::R8, Reg::R8); // addrlen = NULL
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 56)); // accept
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    // accept returns INVALID_SOCKET (-1) on error
    code.cmp_r_imm32(Reg::Rax, 0xFFFF_FFFFu32);
    let ok = code.label();
    code.jcc_label(0x85, ok); // jne: valid socket
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.leave_ret();
}

/// `rt_net_send(sock, data) -> Int`: Send data. Returns bytes sent or -1.
fn emit_net_send(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, 16); // sock
    // data is a Str: [len:8][bytes...] at [rbp+24]
    code.mov_r_mem(Reg::R10, Reg::Rbp, 24); // data ptr
    code.lea_r_mem(Reg::Rdx, Reg::R10, 8); // data+8 = byte array
    code.mov_r_mem(Reg::R8, Reg::R10, 0); // length (first 8 bytes of Str)
    code.xor_rr32(Reg::R9, Reg::R9); // flags = 0
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 64)); // send
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    code.cdqe(); // sign-extend EAX to RAX (send returns int)
    // send returns bytes sent on success, SOCKET_ERROR on failure
    code.cmp_r_imm32(Reg::Rax, 0xFFFF_FFFFu32);
    let ok = code.label();
    code.jcc_label(0x85, ok);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.leave_ret();
}

/// `rt_net_recv(sock, maxlen) -> Str`: Receive data into recv_buf, return as Str.
fn emit_net_recv(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16); // [rbp-8] = bytes received, [rbp-16] = saved string ptr
    code.mov_r_mem(Reg::R10, Reg::Rbp, 16); // sock
    code.mov_r_mem(Reg::R11, Reg::Rbp, 24); // maxlen
    // recv(sock, recv_buf+8, maxlen, 0)
    code.mov_rr(Reg::Rcx, Reg::R10); // sock
    code.lea_r_rip(Reg::Rdx, PatchKind::Bss(NET_RECV_BUF + 8)); // buffer (skip length prefix)
    code.mov_rr(Reg::R8, Reg::R11); // maxlen
    code.xor_rr32(Reg::R9, Reg::R9); // flags = 0
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 72)); // recv
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    code.cdqe(); // sign-extend EAX to RAX (recv returns int)
    // RAX = bytes received, 0 on close, SOCKET_ERROR (-1) on error
    // Store result
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);
    // Check for SOCKET_ERROR
    code.cmp_r_imm32(Reg::Rax, 0xFFFF_FFFFu32);
    let is_ok = code.label();
    let done = code.label();
    code.jcc_label(0x85, is_ok); // jne: not SOCKET_ERROR
    // Error: length = 0
    code.movabs(Reg::Rax, 0);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax);
    code.bind_label(is_ok);
    // Check for negative or zero length
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x8E, done); // jle: zero or negative → return empty
    // Positive length: allocate Str and copy data
    // StrAlloc(length) - RAX already has the recv byte count
    code.sub_rsp(8); // alignment padding
    code.u8(0x50); // push rax (= length) as stack arg
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16); // pop arg + alignment padding
    // RAX = allocated string ptr. Save it.
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax);
    // Copy recv_buf+8 -> string+8
    code.lea_r_rip(Reg::R10, PatchKind::Bss(NET_RECV_BUF + 8)); // source
    code.add_r_imm8(Reg::Rax, 8); // skip length prefix
    code.mov_rr(Reg::R8, Reg::Rax); // dest
    code.mov_r_mem(Reg::R9, Reg::Rbp, -8); // count
    let copy_loop = code.label();
    let copy_done = code.label();
    code.bind_label(copy_loop);
    code.test_rr(Reg::R9, Reg::R9);
    code.jcc_label(0x84, copy_done);
    code.movzx_byte(Reg::Rax, Reg::R10, 0);
    code.mov_mem_r8(Reg::R8, 0, Reg::Rax);
    code.add_r_imm8(Reg::R10, 1);
    code.add_r_imm8(Reg::R8, 1);
    code.sub_r_imm32(Reg::R9, 1);
    code.jmp_label(copy_loop);
    code.bind_label(copy_done);
    // Return saved string ptr
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16);
    code.leave_ret();
    // Empty/error path: return allocated empty string
    code.bind_label(done);
    code.sub_rsp(8); // alignment padding
    code.xor_rr32(Reg::Rax, Reg::Rax); // RAX = 0
    code.u8(0x50); // push rax (= 0) as stack arg
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16); // pop arg + alignment padding
    // RAX = empty string ptr (length prefix = 0)
    code.leave_ret();
}

/// `rt_net_close(sock) -> Int`: Close socket. Returns 0 on success.
fn emit_net_close(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, 16); // sock
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 80)); // closesocket
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    // closesocket returns 0 on success
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x84, ok);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_shutdown(sock, how) -> Int`: Shutdown socket.
fn emit_net_shutdown(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, 16); // sock
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, 24); // how
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 88)); // shutdown
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x84, ok);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_getaddrinfo(host, port) -> Str`: V1 returns host as-is.
fn emit_net_get_addr_info(code: &mut Code) {
    prologue(code);
    // V1: just return the host string as-is (no allocation needed)
    // Parameters: [rbp+16]=host ptr, [rbp+24]=port
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // host ptr
    code.leave_ret();
}

/// `rt_net_freeaddrinfo()`: V1 no-op.
fn emit_net_free_addr_info(code: &mut Code) {
    prologue(code);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_gethostname() -> Str`: Get local hostname.
/// Allocates a heap Str via StrAlloc and copies the hostname into it.
fn emit_net_get_host_name(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16); // [rbp-8] = length, [rbp-16] = saved string ptr
    // Use BSS recv_buf as temp buffer: gethostname(NET_RECV_BUF, 255)
    code.lea_r_rip(Reg::Rcx, PatchKind::Bss(NET_RECV_BUF));
    code.movabs(Reg::Rdx, 255);
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 112)); // gethostname
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    // Scan for null terminator to compute length
    code.lea_r_rip(Reg::R10, PatchKind::Bss(NET_RECV_BUF));
    code.xor_rr32(Reg::R8, Reg::R8);
    let scan = code.label();
    let scan_done = code.label();
    code.bind_label(scan);
    code.cmp_r_imm32(Reg::R8, 255);
    code.jcc_label(0x83, scan_done);
    code.movzx_byte(Reg::R9, Reg::R10, 0);
    code.test_rr(Reg::R9, Reg::R9);
    code.jcc_label(0x84, scan_done);
    code.add_r_imm8(Reg::R10, 1);
    code.add_r_imm8(Reg::R8, 1);
    code.jmp_label(scan);
    code.bind_label(scan_done);
    // R8 = hostname length. Store in spill slot.
    code.mov_mem_r(Reg::Rbp, -8, Reg::R8);
    // Allocate a Str via StrAlloc(length)
    code.sub_rsp(8); // alignment padding
    code.mov_rr(Reg::Rax, Reg::R8); // RAX = length (for stack arg)
    code.u8(0x50); // push length as stack arg
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16); // pop arg + alignment padding
    // RAX = allocated string ptr. Save it.
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax);
    // Copy hostname from NET_RECV_BUF to string+8
    code.lea_r_rip(Reg::R10, PatchKind::Bss(NET_RECV_BUF)); // source
    code.add_r_imm8(Reg::Rax, 8); // skip length prefix
    code.mov_rr(Reg::R8, Reg::Rax); // dest
    code.mov_r_mem(Reg::R9, Reg::Rbp, -8); // count
    let copy_loop = code.label();
    let copy_done = code.label();
    code.bind_label(copy_loop);
    code.test_rr(Reg::R9, Reg::R9);
    code.jcc_label(0x84, copy_done);
    code.movzx_byte(Reg::Rax, Reg::R10, 0);
    code.mov_mem_r8(Reg::R8, 0, Reg::Rax);
    code.add_r_imm8(Reg::R10, 1);
    code.add_r_imm8(Reg::R8, 1);
    code.sub_r_imm32(Reg::R9, 1);
    code.jmp_label(copy_loop);
    code.bind_label(copy_done);
    // Return saved string ptr
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16);
    code.leave_ret();
}

/// `rt_net_htons(value) -> Int`: Host to network byte order.
fn emit_net_htons(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, 16); // value
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(NET_FUNC_TABLE + 120)); // htons
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    code.leave_ret();
}

// ===========================================================================
// Time services (Session 72)
// ===========================================================================

/// `rt_time_now() -> Int`: Return current Unix timestamp (seconds since epoch).
/// Uses GetSystemTimeAsFileTime, converts from 100-ns intervals since 1601.
fn emit_time_now(code: &mut Code) {
    prologue(code);
    // FILETIME is 8 bytes on stack
    code.sub_rsp(8);
    // GetSystemTimeAsFileTime(&ft)
    code.lea_r_mem(Reg::Rcx, Reg::Rbp, -8);
    code.call_rip(PatchKind::Iat(24)); // GET_SYSTEM_TIME_AS_FILE_TIME
    // Load ft into RAX (100-ns intervals since 1601-01-01)
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    // Convert: Unix timestamp = (ft - 116444736000000000) / 10000000
    // 116444736000000000 = 0x019DB1DED53E8000
    code.movabs(Reg::Rdx, 0x019DB1DED53E8000u64);
    code.sub_rr(Reg::Rax, Reg::Rdx);
    // Divide by 10000000
    code.movabs(Reg::Rdx, 0);
    code.movabs(Reg::R10, 10_000_000u64);
    code.div_r(Reg::R10); // RAX = RAX / 10000000
    code.add_rsp(8);
    code.leave_ret();
}

/// `rt_time_millis() -> Int`: Return milliseconds since boot.
/// Uses GetTickCount64.
fn emit_time_millis(code: &mut Code) {
    prologue(code);
    code.call_rip(PatchKind::Iat(25)); // GET_TICK_COUNT_64
    code.leave_ret();
}

/// `rt_time_ticks() -> Int`: Return QueryPerformanceCounter value.
/// Stub: returns GetTickCount64 (close enough for V1).
fn emit_time_ticks(code: &mut Code) {
    prologue(code);
    code.call_rip(PatchKind::Iat(25)); // GET_TICK_COUNT_64
    code.leave_ret();
}

/// `rt_time_freq() -> Int`: Return QueryPerformanceFrequency.
/// Stub: returns 1000 (millisecond resolution).
fn emit_time_freq(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, 1000);
    code.leave_ret();
}

/// `rt_time_filetime() -> Int`: Return low 32 bits of FILETIME.
fn emit_time_filetime(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);
    code.lea_r_mem(Reg::Rcx, Reg::Rbp, -8);
    code.call_rip(PatchKind::Iat(24));
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.add_rsp(8);
    code.leave_ret();
}

/// `rt_time_filetime_high() -> Int`: Return high 32 bits of FILETIME.
fn emit_time_filetime_high(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);
    code.lea_r_mem(Reg::Rcx, Reg::Rbp, -8);
    code.call_rip(PatchKind::Iat(24));
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -4); // high DWORD at offset +4
    code.add_rsp(8);
    code.leave_ret();
}

// ===========================================================================
// Process services (Session 72)
// ===========================================================================

/// `rt_process_id() -> Int`: Return current process ID.
fn emit_process_id(code: &mut Code) {
    prologue(code);
    code.call_rip(PatchKind::Iat(22)); // GET_CURRENT_PROCESS_ID
    code.leave_ret();
}

/// `rt_process_run(cmd) -> Int`: V1 stub — returns -1.
fn emit_process_run(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
}

/// `rt_process_stdout() -> Str`: V1 stub — returns empty string.
fn emit_process_stdout(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.u8(0x50);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.leave_ret();
}

/// `rt_process_stderr() -> Str`: V1 stub — returns empty string.
fn emit_process_stderr(code: &mut Code) {
    emit_process_stdout(code);
}

/// `rt_process_stdout_len() -> Int`: V1 stub — returns 0.
fn emit_process_stdout_len(code: &mut Code) {
    prologue(code);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_process_stderr_len() -> Int`: V1 stub — returns 0.
fn emit_process_stderr_len(code: &mut Code) {
    emit_process_stdout_len(code);
}

// ===========================================================================
// Random services (Session 73)
// ===========================================================================

/// `rt_random_seed(seed)`: Seed the xorshift64* PRNG.
fn emit_random_seed(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // seed
    // xorshift64* requires nonzero state: map 0 -> 1.
    code.test_rr(Reg::Rax, Reg::Rax);
    let nonzero = code.label();
    code.jcc_label(0x85, nonzero); // jnz
    code.movabs(Reg::Rax, 1u64);
    code.bind_label(nonzero);
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(RNG_STATE));
    code.leave_ret();
}

/// `rt_random_next() -> Int`: Return next xorshift64* value.
fn emit_random_next(code: &mut Code) {
    prologue(code);
    // x ^= x >> 12; x ^= x << 25; x ^= x >> 27; return x * 2685821657736338717
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(RNG_STATE));
    // x ^= x >> 12
    code.mov_rr(Reg::Rcx, Reg::Rax);
    code.shr_r_imm8(Reg::Rcx, 12);
    code.xor_rr(Reg::Rax, Reg::Rcx);
    // x ^= x << 25
    code.mov_rr(Reg::Rcx, Reg::Rax);
    code.shl_r_imm8(Reg::Rcx, 25);
    code.xor_rr(Reg::Rax, Reg::Rcx);
    // x ^= x >> 27
    code.mov_rr(Reg::Rcx, Reg::Rax);
    code.shr_r_imm8(Reg::Rcx, 27);
    code.xor_rr(Reg::Rax, Reg::Rcx);
    // x *= multiplier (2685821657736338717)
    // mul_r uses RDX:RAX = RAX * r/m, clobbers RDX
    code.movabs(Reg::Rdx, 2685821657736338717u64);
    code.mul_r(Reg::Rdx);
    // Store back to RNG_STATE
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(RNG_STATE));
    code.leave_ret();
}

// ===========================================================================
// Environment services (Session 73)

// ===========================================================================

// NOTE: Environment stubs return empty/error values.
// A real implementation would use GetEnvironmentVariableA.

/// `rt_env_get(key) -> Str`: V1 stub — return empty string.
fn emit_env_get(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.u8(0x50);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.leave_ret();
}

/// `rt_env_set(key, value) -> Int`: V1 stub — return -1.
fn emit_env_set(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
}

/// `rt_env_has(key) -> Bool`: V1 stub — return false (0).
fn emit_env_has(code: &mut Code) {
    prologue(code);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_env_remove(key) -> Int`: V1 stub — return -1.
fn emit_env_remove(code: &mut Code) {
    emit_env_set(code);
}

// ===========================================================================
// Crypto services (Session 71)
// ===========================================================================

/// `rt_crypto_init() -> Int`: Load bcrypt.dll, resolve BCryptGenRandom.
/// Returns 0 on success, -1 on error.
fn emit_crypto_init(code: &mut Code) {
    prologue(code);
    let already = code.label();
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(CRYPTO_DLL));
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x85, already); // jnz: already loaded

    // Write "bcrypt.dll\0" to temp area
    let dll_name = NET_RECV_BUF; // reuse temp area (crypto init happens before any recv)
    // "bcrypt.d" = 0x642E747079726362 in LE
    code.movabs(Reg::Rax, u64::from_le_bytes(*b"bcrypt.d"));
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(dll_name));
    // "ll\0..." = 0x000000006C6C in LE
    code.movabs(Reg::Rax, u64::from_le_bytes(*b"ll\0\0\0\0\0\0"));
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(dll_name + 8));

    // LoadLibraryA("bcrypt.dll")
    code.lea_r_rip(Reg::Rcx, PatchKind::Bss(dll_name));
    code.sub_rsp(32);
    code.call_rip(PatchKind::Iat(32)); // LoadLibraryA
    code.add_rsp(32);
    code.test_rr(Reg::Rax, Reg::Rax);
    let fail = code.label();
    code.jcc_label(0x84, fail);
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(CRYPTO_DLL));

    // Resolve BCryptGenRandom
    let func_name = NET_RECV_BUF + 32;
    code.movabs(Reg::Rax, u64::from_le_bytes(*b"BCryptGe"));
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(func_name));
    code.movabs(Reg::Rax, u64::from_le_bytes(*b"nRandom\0"));
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(func_name + 8));

    code.mov_r_rip(Reg::Rcx, PatchKind::Bss(CRYPTO_DLL));
    code.lea_r_rip(Reg::Rdx, PatchKind::Bss(func_name));
    code.sub_rsp(32);
    code.call_rip(PatchKind::Iat(33)); // GetProcAddress
    code.add_rsp(32);
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x84, fail);
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(CRYPTO_TABLE));

    // Return 0 (success)
    code.bind_label(already);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
    code.bind_label(fail);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
}

/// `rt_crypto_random_bytes(buf, len) -> Int`: Fill buf with len secure random bytes.
/// Returns 0 on success, -1 on error.
fn emit_crypto_random_bytes(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);
    code.mov_r_mem(Reg::R10, Reg::Rbp, 16); // buf ptr
    code.mov_r_mem(Reg::R11, Reg::Rbp, 24); // len
    // BCryptGenRandom(NULL, buf+8, len, BCRYPT_USE_SYSTEM_PREFERRED_RNG=0x00000002)
    code.xor_rr32(Reg::Rcx, Reg::Rcx); // hAlgorithm = NULL
    code.lea_r_mem(Reg::Rdx, Reg::R10, 8); // buf+8 = data area
    code.mov_rr(Reg::R8, Reg::R11); // length
    code.movabs(Reg::R9, 2); // flags = BCRYPT_USE_SYSTEM_PREFERRED_RNG
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(CRYPTO_TABLE));
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    // NTSTATUS: 0 = success
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x84, ok);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    // Set string length
    code.mov_r_mem(Reg::R10, Reg::Rbp, 24);
    code.mov_r_mem(Reg::R11, Reg::Rbp, 16);
    code.mov_mem_r(Reg::R11, 0, Reg::R10);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_crypto_random_int() -> Int`: Return a cryptographically secure 64-bit random integer.
fn emit_crypto_random_int(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);
    // Use [rbp-8] as the 8-byte temp buffer
    code.mov_mem_imm32(Reg::Rbp, -8, 0);
    // BCryptGenRandom(NULL, &buf, 8, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
    code.xor_rr32(Reg::Rcx, Reg::Rcx);
    code.lea_r_mem(Reg::Rdx, Reg::Rbp, -8);
    code.movabs(Reg::R8, 8);
    code.movabs(Reg::R9, 2);
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(CRYPTO_TABLE));
    code.sub_rsp(32);
    code.call_rax();
    code.add_rsp(32);
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x84, ok);
    code.add_rsp(16);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
    code.bind_label(ok);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.add_rsp(16);
    code.leave_ret();
}

/// `rt_crypto_secure_zero(ptr, len)`: Securely zero memory.
fn emit_crypto_secure_zero(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::R10, Reg::Rbp, 16); // ptr
    code.mov_r_mem(Reg::R11, Reg::Rbp, 24); // len
    let loop_start = code.label();
    let loop_done = code.label();
    code.bind_label(loop_start);
    code.test_rr(Reg::R11, Reg::R11);
    code.jcc_label(0x84, loop_done);
    code.mov_mem_imm32(Reg::R10, 0, 0);
    code.add_r_imm8(Reg::R10, 1);
    code.sub_r_imm32(Reg::R11, 1);
    code.jmp_label(loop_start);
    code.bind_label(loop_done);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

// ===========================================================================
// Filesystem stubs (Session 56 — prevent compiler panics, real Win32 calls TBD)
// ===========================================================================

/// `rt_to_cstr(s: Str) -> Ptr<Int>`: allocate a null-terminated copy of s.
/// The buffer is allocated on the runtime heap and must be freed with
/// `rt_free_cstr`.
fn emit_to_cstr(code: &mut Code) {
    prologue(code);
    // s at [rbp+16]
    code.sub_rsp(8);
    code.mov_r_mem(Reg::R10, Reg::Rbp, 16); // s ptr
    // len = [s]
    code.mov_r_mem(Reg::Rcx, Reg::R10, 0);
    // alloc(len + 1)
    code.lea_r_rip(Reg::Rax, PatchKind::Bss(0)); // placeholder
    code.mov_rr(Reg::Rcx, Reg::Rcx);
    code.add_r_imm8(Reg::Rcx, 1);
    code.u8(0x50); // push rcx (size)
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Alloc));
    code.add_rsp(16);
    // Rax = allocated buffer
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // save buf
    // copy loop: for i in 0..len { buf[i] = s[16+i] }
    code.mov_r_mem(Reg::R10, Reg::Rbp, 16); // s ptr
    code.mov_r_mem(Reg::R11, Reg::R10, 0); // len
    code.mov_r_mem(Reg::R9, Reg::Rbp, -8); // buf
    code.mov_rr(Reg::R10, Reg::Rax); // R10 = buf cursor
    let loop_start = code.label();
    let loop_done = code.label();
    code.bind_label(loop_start);
    code.test_rr(Reg::R11, Reg::R11);
    code.jcc_label(0x84, loop_done); // jz done
    code.mov_r_mem(Reg::Rax, Reg::R10, 0); // load byte from buf...
    // Actually: load from s+16+offset, store to buf+offset
    // Simpler: use a byte loop with movzx
    // R8 = s+16 (data start)
    code.mov_r_mem(Reg::R8, Reg::Rbp, 16);
    code.add_r_imm8(Reg::R8, 16);
    // Compute offset = original_len - R11
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, 16);
    code.mov_r_mem(Reg::Rdx, Reg::Rdx, 0); // original len
    code.mov_rr(Reg::Rax, Reg::Rdx);
    code.sub_rr(Reg::Rax, Reg::R11); // offset
    // Load byte from s[16+offset]
    code.add_rr(Reg::R8, Reg::Rax);
    code.movzx_byte(Reg::Rdx, Reg::R8, 0);
    // Store byte to buf[offset]
    code.mov_r_mem(Reg::R9, Reg::Rbp, -8);
    code.add_rr(Reg::R9, Reg::Rax);
    code.mov_mem_r8(Reg::R9, 0, Reg::Rdx);
    code.sub_r_imm32(Reg::R11, 1);
    code.jmp_label(loop_start);
    code.bind_label(loop_done);
    // null terminate
    code.mov_r_mem(Reg::R9, Reg::Rbp, -8);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_r_mem(Reg::Rax, Reg::Rax, 0); // len
    code.add_rr(Reg::R9, Reg::Rax);
    code.mov_mem_imm32(Reg::R9, 0, 0); // buf[len] = 0
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.leave_ret();
}

/// `rt_free_cstr(p: Ptr<Int>)`: free a buffer allocated by `rt_to_cstr`.
fn emit_free_cstr(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // ptr
    code.u8(0x50); // push rax
    code.call_patch(PatchKind::RuntimeService(RuntimeService::Free));
    code.add_rsp(8);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_fs_exists(path: Str) -> Bool`: stub — return false (0).
fn emit_fs_exists(code: &mut Code) {
    prologue(code);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_fs_file_size(path: Str) -> Int`: stub — return -1.
fn emit_fs_file_size(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
}

/// `rt_fs_read(path: Str) -> Str`: stub — return empty string.
fn emit_fs_read(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.u8(0x50);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.leave_ret();
}

/// `rt_fs_write(path: Str, data: Str) -> Int`: stub — return -1.
fn emit_fs_write(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
}

/// `rt_fs_create_dir(path: Str) -> Int`: stub — return -1.
fn emit_fs_create_dir(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
}

/// `rt_fs_remove_dir(path: Str) -> Int`: stub — return -1.
fn emit_fs_remove_dir(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
}

/// `rt_fs_remove_file(path: Str) -> Int`: stub — return -1.
fn emit_fs_remove_file(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
}

/// `rt_fs_copy(src: Str, dst: Str) -> Int`: stub — return -1.
fn emit_fs_copy(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
}

/// `rt_fs_move(src: Str, dst: Str) -> Int`: stub — return -1.
fn emit_fs_move(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
}

/// `rt_fs_get_cwd() -> Str`: stub — return empty string.
fn emit_fs_get_cwd(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.u8(0x50);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.leave_ret();
}

/// `rt_fs_set_cwd(path: Str) -> Int`: stub — return -1.
fn emit_fs_set_cwd(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
}
