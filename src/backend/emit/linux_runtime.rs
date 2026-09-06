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
//! - ProcessId, ProcessRun (fork+execve+wait4 with pipe capture),
//!   ProcessStdout, ProcessStderr, ProcessStdoutLen, ProcessStderrLen
//!
//! Stub services (emit a trap for unimplemented subsystems):
//! - Vec*, Time*, Random*, Env*, Net*, Crypto*

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
const SYS_PIPE: u64 = 22;
const SYS_DUP2: u64 = 33;
const SYS_GETPID: u64 = 39;
const SYS_FORK: u64 = 57;
const SYS_EXECVE: u64 = 59;
const SYS_WAIT4: u64 = 61;
const SYS_GETTIMEOFDAY: u64 = 96;
const SYS_GETRANDOM: u64 = 318;
const SYS_NANOSLEEP: u64 = 35;
const SYS_CLOCK_GETTIME: u64 = 228;
const CLOCK_REALTIME: i32 = 0;
const CLOCK_MONOTONIC: i32 = 1;

// ===========================================================================
// Linux networking syscall numbers
// ===========================================================================

const SYS_SOCKET: u64 = 41;
const SYS_SETSOCKOPT: u64 = 54;
const SYS_CONNECT: u64 = 42;
const SYS_ACCEPT: u64 = 43;
const SYS_SENDTO: u64 = 44;
const SYS_RECVFROM: u64 = 45;
const SYS_SHUTDOWN: u64 = 48;
const SYS_BIND: u64 = 49;
const SYS_LISTEN: u64 = 50;
const SYS_UNAME: u64 = 63;

// AF_INET=2, SOCK_STREAM=1, SOCK_DGRAM=2, IPPROTO_TCP=6, IPPROTO_UDP=17
const AF_INET: u64 = 2;
const SOCK_STREAM: u64 = 1;
const IPPROTO_TCP: u64 = 6;

/// Offset of recv_buf in BSS (reuse Windows net_func_table slot).
const NET_RECV_BUF_BSS: u32 = crate::runtime::abi::BSS.recv_buf as u32;

/// recv_buf size (bytes). Must hold hostname + receive data.
const RECV_BUF_SIZE: u32 = 4096;

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
    // Note: entry_rsp was already saved by the x86_64 entry stub before
    // calling Init. Do NOT overwrite it here.
    prologue(code);
    // Reset bump cursor and free list
    code.mov_rip_imm32(PatchKind::Bss(BSS.cursor as u32), 0);
    code.mov_rip_imm32(PatchKind::Bss(BSS.free_head as u32), 0);
    // Record immutable string-data bounds in BSS for StrValidate
    code.lea_r_rip(Reg::Rax, PatchKind::Label(offsets.str_data_start_label));
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(BSS.str_data_start as u32));
    code.lea_r_rip(Reg::Rax, PatchKind::Label(offsets.str_data_end_label));
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(BSS.str_data_end as u32));
    // Initialize RNG seed via getrandom(2) syscall
    // int getrandom(void *buf, size_t count, unsigned int flags)
    code.sub_rsp(16); // scratch buffer for random bytes
    code.mov_rr(Reg::Rdi, Reg::Rsp); // buf
    code.movabs(Reg::Rsi, 8u64); // count = 8
    code.xor_rr32(Reg::Rdx, Reg::Rdx); // flags = 0 (blocking)
    code.movabs(Reg::Rax, SYS_GETRANDOM);
    code.syscall();
    // rax = bytes read (8 on success, negative on error)
    code.test_rr(Reg::Rax, Reg::Rax);
    let rng_fallback = code.label();
    code.jcc_label(0x8E, rng_fallback); // jle = rax <= 0 => error
    // Success: load random bytes as seed
    code.mov_r_mem(Reg::Rax, Reg::Rsp, 0);
    code.add_rsp(16);
    let rng_done = code.label();
    code.jmp_label(rng_done);
    code.bind_label(rng_fallback);
    code.add_rsp(16);
    code.movabs(Reg::Rax, 1u64); // fallback seed
    code.bind_label(rng_done);
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(BSS.rng_state as u32));
    // Capture environment pointer from the initial kernel stack.
    // entry_rsp is the REAL kernel RSP saved by the entry stub (points to argc).
    // Layout: [rsp+0]=argc, [rsp+8]=argv[0],..., [rsp+8*argc]=argv[argc-1],
    //   [rsp+8*(argc+1)]=NULL, [rsp+8*(argc+2)]=envp[0],...
    code.mov_r_rip(Reg::Rcx, PatchKind::Bss(BSS.entry_rsp as u32));
    code.mov_r_mem(Reg::Rax, Reg::Rcx, 0); // argc
    code.lea_r_mem(Reg::Rdx, Reg::Rax, 2); // argc + 2
    code.shl_r_imm8(Reg::Rdx, 3); // 8 * (argc + 2)
    code.add_rr(Reg::Rdx, Reg::Rcx); // envp = entry_rsp + 8*(argc+2)
    code.mov_rip_r(Reg::Rdx, PatchKind::Bss(BSS.env_ptr as u32));
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

    // Walk the LIFO free list from the head and reuse the FIRST block
    // large enough for this allocation. Small blocks freed after a large
    // one must not permanently hide it (repeated large buffers would
    // otherwise exhaust the arena although every block was freed).
    // Skipped blocks are dropped only when a later block is reused;
    // when nothing fits the whole list is preserved and we bump.
    let bump = code.label();
    let record = code.label();
    let walk = code.label();
    let reuse = code.label();
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(BSS.free_head as u32));
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x84, bump); // jz (empty free list)
    code.bind_label(walk);
    // Read the saved size from [block+8] and the next link from [block].
    code.mov_r_mem(Reg::Rdx, Reg::Rax, 8); // Rdx = old_size
    code.mov_r_mem(Reg::Rcx, Reg::Rax, 0); // Rcx = next block
    code.cmp_r_mem(Reg::Rdx, Reg::Rbp, -8); // old_size vs needed
    code.jcc_label(0x8D, reuse); // jge — large enough, reuse this block
    code.mov_rr(Reg::Rax, Reg::Rcx); // advance to the next block
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x85, walk); // jnz — keep walking
    code.jmp_label(bump); // walked off the end: nothing fits
    code.bind_label(reuse);
    // Unlink the reused block: free_head = its next link.
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
    // Freeing an immutable image literal is a safe no-op: image string
    // data is immortal and was never heap-allocated, so there is nothing
    // to release. This lets consuming stdlib functions free their Owned
    // Str parameters unconditionally (callers may legitimately pass
    // literals, which the checker cannot distinguish at runtime).
    // Heap misuse below is still fully detected: double frees and free
    // of non-live pointers fall through this check and fail E-R04.
    let not_image = code.label();
    code.mov_r_rip(Reg::Rcx, PatchKind::Bss(BSS.str_data_start as u32));
    code.cmp_rr(Reg::Rax, Reg::Rcx);
    code.jcc_label(0x82, not_image); // jb — below the image region
    code.mov_r_rip(Reg::Rdx, PatchKind::Bss(BSS.str_data_end as u32));
    code.cmp_rr(Reg::Rax, Reg::Rdx);
    code.jcc_label(0x83, not_image); // jae — at/above the image region
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret(); // inside the image region: no-op success
    code.bind_label(not_image);

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
    // Found a live allocation — leak! Fail with the catalogued E-R06
    // number (6), matching the Windows runtime. The enum discriminant
    // is not the error number.
    code.movabs(Reg::Rcx, RuntimeErrorKind::Leak.number());
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
// Process services
// ===========================================================================

/// `rt_process_id() -> Int`: Return current process ID via getpid().
fn emit_process_id(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, SYS_GETPID);
    code.syscall();
    code.leave_ret();
}
/// `rt_process_run(cmd: Str) -> Int`: Run command via /bin/sh -c, capture
/// stdout/stderr into BSS buffers, return child exit code.
///
/// Uses fork/execve/wait4 with pipe-based output capture.
/// MINK Str is [u64 len][bytes]; converted to NUL-terminated CStr for
/// execve. Child argv: ["/bin/sh", "-c", cmd, NULL].
///
/// Stack frame (112 bytes, keeps RSP 16-byte aligned after push rbp):
///   [rbp-8]   = cmd_cstr ptr
///   [rbp-16]  = stdout pipe fds (int[2]: read at -16, write at -12)
///   [rbp-24]  = stderr pipe fds (int[2]: read at -24, write at -20)
///   [rbp-32]  = child pid
///   [rbp-40]  = wait4 status
///   [rbp-48]  = exit code
///   [rbp-56]  = "/bin/sh\0" (8 bytes)
///   [rbp-64]  = "-c\0" (8 bytes)
///   [rbp-112] = argv[0] = &"/bin/sh" (at rbp-56) -- array grows UPWARD
///   [rbp-104] = argv[1] = &"-c" (at rbp-64)
///   [rbp-96]  = argv[2] = cmd_cstr
///   [rbp-88]  = argv[3] = NULL
fn emit_process_run(code: &mut Code) {
    prologue(code);
    // [rbp+16] = cmd Str ptr

    // Allocate frame: 112 bytes (112%16==0, so after push rbp: RSP 16-aligned)
    code.sub_rsp(112);

    // --- Convert cmd Str to NUL-terminated C string ---
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    to_cstr(code, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // [rbp-8] = cmd_cstr

    // --- Set up "/bin/sh\0" and "-c\0" on stack (BELOW argv to avoid overlap) ---
    // "/bin/sh\0" = 0x0068732F6E69622F little-endian
    code.movabs(Reg::Rax, 0x0068732F6E69622Fu64);
    code.mov_mem_r(Reg::Rbp, -56, Reg::Rax);
    // "-c\0" = 0x000000000000632D little-endian
    code.movabs(Reg::Rax, 0x000000000000632Du64);
    code.mov_mem_r(Reg::Rbp, -64, Reg::Rax);

    // --- Set up argv array growing UPWARD from rbp-112 ---
    // execve reads argv[0] at rsi, argv[1] at rsi+8, etc.
    code.lea_r_mem(Reg::Rax, Reg::Rbp, -56); // &"/bin/sh"
    code.mov_mem_r(Reg::Rbp, -112, Reg::Rax); // argv[0]
    code.lea_r_mem(Reg::Rax, Reg::Rbp, -64); // &"-c"
    code.mov_mem_r(Reg::Rbp, -104, Reg::Rax); // argv[1]
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8); // cmd_cstr
    code.mov_mem_r(Reg::Rbp, -96, Reg::Rax); // argv[2]
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -88, Reg::Rax); // argv[3] = NULL

    // --- Create stdout pipe ---
    code.lea_r_mem(Reg::Rdi, Reg::Rbp, -16); // &stdout_fds
    code.movabs(Reg::Rax, SYS_PIPE);
    code.syscall();
    code.test_rr(Reg::Rax, Reg::Rax);
    let pipe1_err = code.label();
    code.jcc_label(0x85, pipe1_err); // jnz = error

    // --- Create stderr pipe ---
    code.lea_r_mem(Reg::Rdi, Reg::Rbp, -24); // &stderr_fds
    code.movabs(Reg::Rax, SYS_PIPE);
    code.syscall();
    code.test_rr(Reg::Rax, Reg::Rax);
    let pipe2_err = code.label();
    code.jcc_label(0x85, pipe2_err); // jnz = error

    // --- fork() ---
    code.movabs(Reg::Rax, SYS_FORK);
    code.syscall();
    code.mov_mem_r(Reg::Rbp, -32, Reg::Rax); // save pid

    // Check fork error (negative)
    code.test_rr(Reg::Rax, Reg::Rax);
    let fork_err = code.label();
    code.jcc_label(0x88, fork_err); // js = negative = error

    // Check if child (pid == 0)
    code.test_rr(Reg::Rax, Reg::Rax);
    let parent = code.label();
    code.jcc_label(0x85, parent); // jnz = parent

    // ================================================================
    // CHILD PROCESS
    // ================================================================

    // Close read ends of both pipes
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -16); // stdout_read
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -24); // stderr_read
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();

    // dup2(stdout_write, 1) - redirect stdout to pipe
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -12); // stdout_write
    code.mov_r32_imm32(Reg::Rsi, 1);
    code.movabs(Reg::Rax, SYS_DUP2);
    code.syscall();

    // dup2(stderr_write, 2) - redirect stderr to pipe
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -20); // stderr_write
    code.mov_r32_imm32(Reg::Rsi, 2);
    code.movabs(Reg::Rax, SYS_DUP2);
    code.syscall();

    // Close original write ends (now duplicated to 1 and 2)
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -12);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -20);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();

    // execve("/bin/sh", argv, NULL)
    code.lea_r_mem(Reg::Rdi, Reg::Rbp, -56); // rdi = &"/bin/sh" (filename)
    code.lea_r_mem(Reg::Rsi, Reg::Rbp, -112); // rsi = argv pointer (rbp-112)
    code.xor_rr32(Reg::Rdx, Reg::Rdx); // envp = NULL (inherit parent env)
    code.movabs(Reg::Rax, SYS_EXECVE);
    code.syscall();
    // If execve returns, it failed. _exit(127).
    code.mov_r32_imm32(Reg::Rdi, 127);
    code.movabs(Reg::Rax, SYS_EXIT);
    code.syscall();
    code.int3(); // unreachable

    // ================================================================
    // PARENT PROCESS
    // ================================================================
    code.bind_label(parent);

    // Close write ends of both pipes (parent doesn't write)
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -12); // stdout_write
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -20); // stderr_write
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();

    // Read stdout from pipe into BSS stdout_buf+8 (max 4088 bytes)
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -16); // stdout_read fd
    code.lea_r_rip(Reg::Rsi, PatchKind::Bss(BSS.stdout_buf as u32 + 8));
    code.movabs(Reg::Rdx, 4088u64);
    code.movabs(Reg::Rax, SYS_READ);
    code.syscall();
    // rax = bytes read; if negative, treat as 0
    code.test_rr(Reg::Rax, Reg::Rax);
    let stdout_ok = code.label();
    code.jcc_label(0x89, stdout_ok); // jns
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.bind_label(stdout_ok);
    // Store stdout length in BSS
    code.lea_r_rip(Reg::R10, PatchKind::Bss(BSS.stdout_buf as u32));
    code.mov_mem_r(Reg::R10, 0, Reg::Rax); // [stdout_buf] = len
    code.lea_r_rip(Reg::R10, PatchKind::Bss(BSS.proc_stdout_len as u32));
    code.mov_mem_r(Reg::R10, 0, Reg::Rax);
    code.lea_r_rip(Reg::Rax, PatchKind::Bss(BSS.stdout_buf as u32));
    code.lea_r_rip(Reg::R10, PatchKind::Bss(BSS.proc_stdout_ptr as u32));
    code.mov_mem_r(Reg::R10, 0, Reg::Rax);

    // Read stderr from pipe into BSS stderr_buf+8 (max 4088 bytes)
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -24); // stderr_read fd
    code.lea_r_rip(Reg::Rsi, PatchKind::Bss(BSS.stderr_buf as u32 + 8));
    code.movabs(Reg::Rdx, 4088u64);
    code.movabs(Reg::Rax, SYS_READ);
    code.syscall();
    code.test_rr(Reg::Rax, Reg::Rax);
    let stderr_ok = code.label();
    code.jcc_label(0x89, stderr_ok); // jns
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.bind_label(stderr_ok);
    code.lea_r_rip(Reg::R10, PatchKind::Bss(BSS.stderr_buf as u32));
    code.mov_mem_r(Reg::R10, 0, Reg::Rax);
    code.lea_r_rip(Reg::R10, PatchKind::Bss(BSS.proc_stderr_len as u32));
    code.mov_mem_r(Reg::R10, 0, Reg::Rax);
    code.lea_r_rip(Reg::Rax, PatchKind::Bss(BSS.stderr_buf as u32));
    code.lea_r_rip(Reg::R10, PatchKind::Bss(BSS.proc_stderr_ptr as u32));
    code.mov_mem_r(Reg::R10, 0, Reg::Rax);

    // wait4(child_pid, &status, 0, NULL)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -32); // child pid
    code.lea_r_mem(Reg::Rsi, Reg::Rbp, -40); // &status
    code.xor_rr32(Reg::Rdx, Reg::Rdx); // options = 0
    code.xor_rr32(Reg::R10, Reg::R10); // rusage = NULL
    code.movabs(Reg::Rax, SYS_WAIT4);
    code.syscall();

    // Extract exit code: (status >> 8) & 0xFF
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -40);
    code.shr_r_imm8(Reg::Rax, 8);
    code.and_r_imm8(Reg::Rax, 0xFF);
    code.mov_mem_r(Reg::Rbp, -48, Reg::Rax); // save exit_code

    // Close read ends
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -16);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -24);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();

    // Free cmd CStr
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);

    // Return exit code
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -48);
    code.leave_ret();

    // ================================================================
    // ERROR PATHS
    // ================================================================

    // Second pipe failed: close first pipe fds, fall through to pipe1_err
    code.bind_label(pipe2_err);
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -16);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -12);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();

    // First pipe failed or cleanup after pipe2_err
    code.bind_label(pipe1_err);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.movabs(Reg::Rax, 0xFFFFFFFFFFFFFFFFu64); // return -1
    code.leave_ret();

    // Fork failed: close all pipe fds
    code.bind_label(fork_err);
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -16);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -12);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -24);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();
    code.mov_r32_mem(Reg::Rdi, Reg::Rbp, -20);
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.movabs(Reg::Rax, 0xFFFFFFFFFFFFFFFFu64);
    code.leave_ret();
}

/// `rt_process_stdout() -> Str`: Return BSS stdout buffer as MINK Str.
fn emit_process_stdout(code: &mut Code) {
    prologue(code);
    code.lea_r_rip(Reg::Rax, PatchKind::Bss(BSS.stdout_buf as u32));
    code.leave_ret();
}

/// `rt_process_stderr() -> Str`: Return BSS stderr buffer as MINK Str.
fn emit_process_stderr(code: &mut Code) {
    prologue(code);
    code.lea_r_rip(Reg::Rax, PatchKind::Bss(BSS.stderr_buf as u32));
    code.leave_ret();
}

/// `rt_process_stdout_len() -> Int`: Return captured stdout length.
fn emit_process_stdout_len(code: &mut Code) {
    prologue(code);
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(BSS.proc_stdout_len as u32));
    code.leave_ret();
}

/// `rt_process_stderr_len() -> Int`: Return captured stderr length.
fn emit_process_stderr_len(code: &mut Code) {
    prologue(code);
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(BSS.proc_stderr_len as u32));
    code.leave_ret();
}

// ===========================================================================
// Time services (Linux)
// ===========================================================================

/// `rt_time_now() -> Int`: Return current Unix timestamp (seconds since 1970).
/// Uses clock_gettime(CLOCK_REALTIME, &ts).
fn emit_time_now(code: &mut Code) {
    prologue(code);
    // Allocate timespec on stack: [rbp-8]=tv_sec, [rbp-16]=tv_nsec
    code.sub_rsp(16);
    // clock_gettime(CLOCK_REALTIME=0, &ts)
    code.movabs(Reg::Rdi, CLOCK_REALTIME as u64);
    code.lea_r_mem(Reg::Rsi, Reg::Rbp, -16);
    code.movabs(Reg::Rax, SYS_CLOCK_GETTIME);
    code.syscall();
    // Return tv_sec (at rbp-16, first 8 bytes)
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16);
    code.add_rsp(16);
    code.leave_ret();
}

/// `rt_time_millis() -> Int`: Return milliseconds since boot.
/// Uses clock_gettime(CLOCK_MONOTONIC, &ts).
fn emit_time_millis(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);
    // clock_gettime(CLOCK_MONOTONIC=1, &ts)
    code.movabs(Reg::Rdi, CLOCK_MONOTONIC as u64);
    code.lea_r_mem(Reg::Rsi, Reg::Rbp, -16);
    code.movabs(Reg::Rax, SYS_CLOCK_GETTIME);
    code.syscall();
    // Return tv_sec * 1000 + tv_nsec / 1000000
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16); // tv_sec
    code.movabs(Reg::R10, 1000u64);
    code.mul_r(Reg::R10); // RDX:RAX = tv_sec * 1000
    code.mov_rr(Reg::R8, Reg::Rax); // save low bits
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8); // tv_nsec
    code.movabs(Reg::R10, 1000000u64);
    code.xor_rr32(Reg::Rdx, Reg::Rdx);
    code.div_r(Reg::R10); // RAX = tv_nsec / 1000000
    code.add_rr(Reg::Rax, Reg::R8); // total millis
    code.add_rsp(16);
    code.leave_ret();
}

/// `rt_time_ticks() -> Int`: Return high-resolution ticks.
/// Uses clock_gettime(CLOCK_MONOTONIC, &ts) and returns nanoseconds.
fn emit_time_ticks(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);
    code.movabs(Reg::Rdi, CLOCK_MONOTONIC as u64);
    code.lea_r_mem(Reg::Rsi, Reg::Rbp, -16);
    code.movabs(Reg::Rax, SYS_CLOCK_GETTIME);
    code.syscall();
    // Return tv_sec * 1_000_000_000 + tv_nsec (nanoseconds since boot)
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16); // tv_sec
    code.movabs(Reg::R10, 1000000000u64);
    code.mul_r(Reg::R10); // RDX:RAX = tv_sec * 1e9
    code.mov_rr(Reg::R8, Reg::Rax);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8); // tv_nsec
    code.add_rr(Reg::Rax, Reg::R8); // total nanoseconds
    code.add_rsp(16);
    code.leave_ret();
}

/// `rt_time_freq() -> Int`: Return frequency of ticks.
/// Returns 1_000_000_000 (nanoseconds) to match ticks.
fn emit_time_freq(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, 1_000_000_000u64);
    code.leave_ret();
}

/// `rt_time_filetime() -> Int`: Return low 32 bits of nanoseconds since boot.
fn emit_time_filetime(code: &mut Code) {
    prologue(code);
    // Allocate 24 bytes: [rbp-16]=timespec, [rbp-24]=nanosecond result
    code.sub_rsp(24);
    code.movabs(Reg::Rdi, CLOCK_MONOTONIC as u64);
    code.lea_r_mem(Reg::Rsi, Reg::Rbp, -16);
    code.movabs(Reg::Rax, SYS_CLOCK_GETTIME);
    code.syscall();
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16); // tv_sec
    code.movabs(Reg::R10, 1000000000u64);
    code.mul_r(Reg::R10);
    code.mov_rr(Reg::R8, Reg::Rax);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8); // tv_nsec
    code.add_rr(Reg::Rax, Reg::R8);
    // Store 64-bit result, read low 32 bits as u32
    code.mov_mem_r(Reg::Rbp, -24, Reg::Rax);
    code.mov_r32_mem(Reg::Rax, Reg::Rbp, -24); // zero-extended 32-bit load
    code.add_rsp(24);
    code.leave_ret();
}

/// `rt_time_filetime_high() -> Int`: Return high 32 bits of nanoseconds since boot.
fn emit_time_filetime_high(code: &mut Code) {
    prologue(code);
    code.sub_rsp(24);
    code.movabs(Reg::Rdi, CLOCK_MONOTONIC as u64);
    code.lea_r_mem(Reg::Rsi, Reg::Rbp, -16);
    code.movabs(Reg::Rax, SYS_CLOCK_GETTIME);
    code.syscall();
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16);
    code.movabs(Reg::R10, 1000000000u64);
    code.mul_r(Reg::R10);
    code.mov_rr(Reg::R8, Reg::Rax);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -8);
    code.add_rr(Reg::Rax, Reg::R8);
    // Store 64-bit result, read high 32 bits
    code.mov_mem_r(Reg::Rbp, -24, Reg::Rax);
    code.mov_r32_mem(Reg::Rax, Reg::Rbp, -20); // high DWORD at offset +4
    code.add_rsp(24);
    code.leave_ret();
}

// ===========================================================================
// Random services (Linux)
// ===========================================================================

/// `rt_random_seed(seed)`: Seed the xorshift64* PRNG.
fn emit_random_seed(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // seed
    // xorshift64* requires nonzero state
    code.test_rr(Reg::Rax, Reg::Rax);
    let nonzero = code.label();
    code.jcc_label(0x85, nonzero); // jnz
    code.movabs(Reg::Rax, 1u64);
    code.bind_label(nonzero);
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(BSS.rng_state as u32));
    code.leave_ret();
}

/// `rt_random_next() -> Int`: Return next xorshift64* value.
fn emit_random_next(code: &mut Code) {
    prologue(code);
    // x ^= x >> 12; x ^= x << 25; x ^= x >> 27; return x * multiplier
    code.mov_r_rip(Reg::Rax, PatchKind::Bss(BSS.rng_state as u32));
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
    // x *= 2685821657736338717
    code.movabs(Reg::Rdx, 2685821657736338717u64);
    code.mul_r(Reg::Rdx);
    // Store back to state and return
    code.mov_rip_r(Reg::Rax, PatchKind::Bss(BSS.rng_state as u32));
    code.leave_ret();
}

// ===========================================================================
// Environment services (Linux)
// ===========================================================================

/// `rt_env_get(name: Str) -> Str`: Get environment variable value.
/// Uses the process environment pointer captured at init.
fn emit_env_get(code: &mut Code) {
    prologue(code);
    code.sub_rsp(40); // [rbp-8]=cstr_name, [rbp-16]=value_start, [rbp-24]=result, [rbp-32]=val_len
    // Convert name Str to C string
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    to_cstr(code, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // cstr_name
    // Walk envp array looking for "NAME=VALUE"
    code.mov_r_rip(Reg::Rcx, PatchKind::Bss(BSS.env_ptr as u32));
    let scan = code.label();
    let not_found = code.label();
    code.bind_label(scan);
    code.mov_r_mem(Reg::Rdx, Reg::Rcx, 0); // *envp = entry ptr
    code.test_rr(Reg::Rdx, Reg::Rdx);
    code.jcc_label(0x84, not_found); // jz = NULL terminator
    // Compare: entry starts with name + '='
    // Load cstr_name pointer
    code.mov_r_mem(Reg::R8, Reg::Rbp, -8);
    let cmp_loop = code.label();
    let name_done = code.label();
    let try_next = code.label();
    code.bind_label(cmp_loop);
    code.movzx_byte(Reg::R9, Reg::R8, 0); // name byte
    code.movzx_byte(Reg::R10, Reg::Rdx, 0); // entry byte
    // If name byte is 0, we matched the full name -> check for '=' in entry
    code.test_rr(Reg::R9, Reg::R9);
    code.jcc_label(0x84, name_done); // jz = name exhausted
    // If entry byte is 0 or doesn't match, try next
    code.test_rr(Reg::R10, Reg::R10);
    code.jcc_label(0x84, try_next); // jz = entry exhausted
    code.cmp_rr(Reg::R9, Reg::R10);
    code.jcc_label(0x85, try_next); // jne
    code.add_r_imm8(Reg::R8, 1);
    code.add_r_imm8(Reg::Rdx, 1);
    code.jmp_label(cmp_loop);
    code.bind_label(name_done);
    // Name matched fully; check entry has '=' next
    code.movzx_byte(Reg::R10, Reg::Rdx, 0);
    code.cmp_r_imm8(Reg::R10, b'=');
    code.jcc_label(0x85, try_next); // jne = no '=' -> not a match
    // Found! entry+strlen(name)+1 is the value
    code.mov_r_mem(Reg::R8, Reg::Rbp, -8);
    code.add_r_imm8(Reg::Rdx, 1); // skip '='
    // Compute value length by scanning for NUL
    code.mov_rr(Reg::R9, Reg::Rdx); // start of value
    let len_loop = code.label();
    code.bind_label(len_loop);
    code.movzx_byte(Reg::R10, Reg::R9, 0);
    code.test_rr(Reg::R10, Reg::R10);
    let len_done = code.label();
    code.jcc_label(0x84, len_done);
    code.add_r_imm8(Reg::R9, 1);
    code.jmp_label(len_loop);
    code.bind_label(len_done);
    code.sub_rr(Reg::R9, Reg::Rdx); // length of value
    code.mov_mem_r(Reg::Rbp, -32, Reg::R9); // save value length across StrAlloc
    // Save the value-start pointer too: the StrAlloc service call below
    // may clobber caller-saved registers (rdx is volatile), and the copy
    // loop reads the source bytes from it.
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rdx); // save value start
    // Allocate MINK Str: 8 + len bytes
    code.mov_rr(Reg::Rax, Reg::R9);
    code.add_r_imm8(Reg::Rax, 8);
    code.sub_rsp(8);
    code.u8(0x50); // push rax (size)
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.mov_mem_r(Reg::Rbp, -24, Reg::Rax); // save result ptr
    // Restore value length and value start, write length prefix
    code.mov_r_mem(Reg::R9, Reg::Rbp, -32);
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, -16);
    code.mov_mem_r(Reg::Rax, 0, Reg::R9);
    // Copy value bytes
    code.add_r_imm8(Reg::Rax, 8); // data start
    code.mov_rr(Reg::R11, Reg::R9); // count
    let copy_loop = code.label();
    code.bind_label(copy_loop);
    code.test_rr(Reg::R11, Reg::R11);
    let copy_done = code.label();
    code.jcc_label(0x84, copy_done);
    code.movzx_byte(Reg::R10, Reg::Rdx, 0);
    code.mov_mem_r8(Reg::Rax, 0, Reg::R10);
    code.add_r_imm8(Reg::Rax, 1);
    code.add_r_imm8(Reg::Rdx, 1);
    code.sub_r_imm8(Reg::R11, 1);
    code.jmp_label(copy_loop);
    code.bind_label(copy_done);
    // Free name CStr and return result
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -24);
    code.add_rsp(40);
    code.leave_ret();
    code.bind_label(try_next);
    code.add_r_imm8(Reg::Rcx, 8); // next envp entry
    code.jmp_label(scan);
    code.bind_label(not_found);
    // Variable not found: free name CStr, return empty string
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.sub_rsp(8);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.u8(0x50); // push 0
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.add_rsp(40);
    code.leave_ret();
}

/// `rt_env_has(name: Str) -> Bool`: Check if environment variable exists.
fn emit_env_has(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);
    // Convert name Str to C string
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    to_cstr(code, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // cstr_name
    // Walk envp
    code.mov_r_rip(Reg::Rcx, PatchKind::Bss(BSS.env_ptr as u32));
    let scan = code.label();
    let not_found = code.label();
    code.bind_label(scan);
    code.mov_r_mem(Reg::Rdx, Reg::Rcx, 0);
    code.test_rr(Reg::Rdx, Reg::Rdx);
    code.jcc_label(0x84, not_found);
    code.mov_r_mem(Reg::R8, Reg::Rbp, -8);
    let cmp_loop = code.label();
    code.bind_label(cmp_loop);
    code.movzx_byte(Reg::R9, Reg::R8, 0);
    code.movzx_byte(Reg::R10, Reg::Rdx, 0);
    code.test_rr(Reg::R9, Reg::R9);
    let name_done = code.label();
    code.jcc_label(0x84, name_done);
    code.test_rr(Reg::R10, Reg::R10);
    let try_next = code.label();
    code.jcc_label(0x84, try_next);
    code.cmp_rr(Reg::R9, Reg::R10);
    code.jcc_label(0x85, try_next);
    code.add_r_imm8(Reg::R8, 1);
    code.add_r_imm8(Reg::Rdx, 1);
    code.jmp_label(cmp_loop);
    code.bind_label(name_done);
    code.movzx_byte(Reg::R10, Reg::Rdx, 0);
    code.cmp_r_imm8(Reg::R10, b'=');
    code.jcc_label(0x85, try_next);
    // Found
    // Found. Free the name CStr FIRST: free_cstr clobbers rax, and the
    // true result must survive to the return.
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.movabs(Reg::Rax, 1u64); // true
    code.add_rsp(8);
    code.leave_ret();
    code.bind_label(try_next);
    code.add_r_imm8(Reg::Rcx, 8);
    code.jmp_label(scan);
    code.bind_label(not_found);
    code.mov_r_mem(Reg::Rcx, Reg::Rbp, -8);
    free_cstr(code, Reg::Rcx);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.add_rsp(8);
    code.leave_ret();
}

/// `rt_env_set(name: Str, value: Str) -> Int`: Set environment variable.
/// Returns 0 on success, -1 on failure.
/// Linux does not provide a simple setenv syscall from user space without libc,
/// so we use a helper: write "NAME=VALUE\0" to env_storage and update env_ptr.
/// This is a simplified V1 implementation.
fn emit_env_set(code: &mut Code) {
    prologue(code);
    // V1: return -1 (not fully supported without libc)
    code.movabs(Reg::Rax, 0xFFFFFFFFFFFFFFFFu64);
    code.leave_ret();
}

/// `rt_env_remove(name: Str) -> Int`: Remove environment variable.
fn emit_env_remove(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, 0xFFFFFFFFFFFFFFFFu64);
    code.leave_ret();
}

// --- Session 99 stub helpers (Linux frozen; compile-totality only) ---

/// Unit stub: returns immediately with no value.
fn emit_linux_stub_unit(code: &mut Code) {
    prologue(code);
    code.leave_ret();
}

/// Stub returning 0.
fn emit_linux_stub_zero(code: &mut Code) {
    prologue(code);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// Stub returning -1.
fn emit_linux_stub_minus_one(code: &mut Code) {
    prologue(code);
    code.movabs(Reg::Rax, 0xFFFF_FFFF_FFFF_FFFFu64);
    code.leave_ret();
}

/// Stub returning an owned empty Str.
fn emit_linux_stub_empty_str(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.u8(0x50);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.leave_ret();
}

// ===========================================================================
// Networking services (Linux)
// ===========================================================================

/// Helper: parse an IPv4 dotted-decimal string into a 32-bit network-byte-order address.
/// Input: R10 = pointer to MINK Str (8-byte length prefix + data).
/// Output: RAX = sin_addr in network byte order.
/// Clobbers: RCX, RDX, R8, R9, R10, R11, RAX.
fn net_parse_ip_addr(code: &mut Code) {
    // R8 = accumulator (network byte order)
    code.xor_rr32(Reg::R8, Reg::R8);
    // R10 = data start (skip 8-byte length prefix)
    code.mov_r_mem(Reg::Rax, Reg::R10, 0); // len
    code.add_r_imm8(Reg::R10, 8);
    // Compute end pointer for bounds checking
    code.mov_rr(Reg::R11, Reg::R10);
    code.add_rr(Reg::R11, Reg::Rax); // R11 = end_ptr
    for octet in 0..4u32 {
        let done_label = code.label();
        code.xor_rr32(Reg::R9, Reg::R9); // octet value = 0
        // Bounds check
        code.cmp_rr(Reg::R10, Reg::R11);
        code.jcc_label(0x83, done_label); // jae
        let digit_loop = code.label();
        code.bind_label(digit_loop);
        code.movzx_byte(Reg::Rcx, Reg::R10, 0);
        code.test_rr(Reg::Rcx, Reg::Rcx);
        code.jcc_label(0x84, done_label); // NUL
        code.cmp_r_imm8(Reg::Rcx, 0x2E);
        code.jcc_label(0x84, done_label); // '.'
        code.sub_r_imm32(Reg::Rcx, '0' as u32);
        code.mov_r32_imm32(Reg::Rax, 10);
        code.mul_r(Reg::R9);
        code.mov_rr(Reg::R9, Reg::Rax);
        code.add_rr(Reg::R9, Reg::Rcx);
        code.add_r_imm8(Reg::R10, 1);
        code.cmp_rr(Reg::R10, Reg::R11);
        code.jcc_label(0x83, done_label); // jae
        code.jmp_label(digit_loop);
        code.bind_label(done_label);
        code.add_r_imm8(Reg::R10, 1); // skip '.' or NUL
        // Build network byte order: octet 0 goes to bits [7:0], etc.
        if octet > 0 {
            code.shl_r_imm8(Reg::R9, (octet * 8) as u8);
        }
        code.add_rr(Reg::R8, Reg::R9);
    }
    code.mov_rr(Reg::Rax, Reg::R8); // result in RAX
}

/// `rt_net_wsa_startup() -> Int`: No-op on Linux (always returns 0).
fn emit_net_wsa_startup(code: &mut Code) {
    prologue(code);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_wsa_cleanup() -> Int`: No-op on Linux (always returns 0).
fn emit_net_wsa_cleanup(code: &mut Code) {
    prologue(code);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_wsa_last_error() -> Int`: Returns 0 on Linux.
fn emit_net_wsa_last_error(code: &mut Code) {
    prologue(code);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_socket(af, ty, proto) -> Int`: Create a socket via socket(2).
/// Returns file descriptor on success, -1 on error.
fn emit_net_socket(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, 16); // af
    code.mov_r_mem(Reg::Rsi, Reg::Rbp, 24); // type
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, 32); // protocol
    code.movabs(Reg::Rax, SYS_SOCKET);
    code.syscall();
    // socket returns fd >= 0 on success, negative errno on error
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x89, ok); // jns (non-negative = success)
    code.movabs(Reg::Rax, 0xFFFFFFFFFFFFFFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.leave_ret();
}

/// `rt_net_connect(sock, addr, port) -> Int`: Connect TCP socket.
/// addr is a MINK Str containing "x.x.x.x". Returns 0 on success, -1 on error.
fn emit_net_connect(code: &mut Code) {
    prologue(code);
    // [rbp-16]=sock, [rbp-24]=addr ptr, [rbp-40..56]=sockaddr_in
    code.sub_rsp(56);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax); // sock
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 24);
    code.mov_mem_r(Reg::Rbp, -24, Reg::Rax); // addr ptr
    // Store port at [rbp-56] before writing sockaddr_in
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 32);
    code.mov_mem_r(Reg::Rbp, -56, Reg::Rax);
    // Zero-init sockaddr_in (16 bytes at [rbp-40])
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -40, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -32, Reg::Rax);
    // sin_family = AF_INET (2)
    code.movabs(Reg::Rax, AF_INET);
    code.mov_mem_r(Reg::Rbp, -40, Reg::Rax);
    // htons(port) — store BEFORE sin_addr to avoid 8-byte overwrite
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -56);
    code.xchg_al_ah(); // swap low 2 bytes = htons
    code.mov_mem_r(Reg::Rbp, -38, Reg::Rax); // sin_port at offset 2
    // Parse IP address string -> sin_addr
    code.mov_r_mem(Reg::R10, Reg::Rbp, -24);
    net_parse_ip_addr(code);
    // RAX = sin_addr in network byte order
    code.mov_mem_r(Reg::Rbp, -36, Reg::Rax); // sin_addr at offset 4
    // connect(sock, &sockaddr_in, 16)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -16);
    code.lea_r_mem(Reg::Rsi, Reg::Rbp, -40);
    code.movabs(Reg::Rdx, 16);
    code.movabs(Reg::Rax, SYS_CONNECT);
    code.syscall();
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x84, ok); // jz: success
    code.movabs(Reg::Rax, 0xFFFFFFFFFFFFFFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_bind(sock, addr, port) -> Int`: Bind TCP socket.
fn emit_net_bind(code: &mut Code) {
    prologue(code);
    code.sub_rsp(56);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax); // sock
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 24);
    code.mov_mem_r(Reg::Rbp, -24, Reg::Rax); // addr ptr
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 32);
    code.mov_mem_r(Reg::Rbp, -56, Reg::Rax); // port
    // Zero-init sockaddr_in
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -40, Reg::Rax);
    code.mov_mem_r(Reg::Rbp, -32, Reg::Rax);
    code.movabs(Reg::Rax, AF_INET);
    code.mov_mem_r(Reg::Rbp, -40, Reg::Rax);
    // htons(port) — store BEFORE sin_addr to avoid 8-byte overwrite
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -56);
    code.xchg_al_ah(); // swap low 2 bytes = htons
    code.mov_mem_r(Reg::Rbp, -38, Reg::Rax); // sin_port at offset 2
    // Parse IP address -> sin_addr
    code.mov_r_mem(Reg::R10, Reg::Rbp, -24);
    net_parse_ip_addr(code);
    code.mov_mem_r(Reg::Rbp, -36, Reg::Rax); // sin_addr at offset 4
    // setsockopt(sock, SOL_SOCKET, SO_REUSEADDR, &one, 4): allow quick
    // rebinds so restarts and test reruns do not hit EADDRINUSE from a
    // peer in TIME_WAIT. Best-effort — a failure is ignored.
    code.mov_mem_imm32(Reg::Rbp, -8, 1); // optval = 1
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -16); // sock
    code.movabs(Reg::Rsi, 1); // SOL_SOCKET
    code.movabs(Reg::Rdx, 2); // SO_REUSEADDR
    code.lea_r_mem(Reg::R10, Reg::Rbp, -8); // optval
    code.movabs(Reg::R8, 4); // optlen
    code.movabs(Reg::Rax, SYS_SETSOCKOPT);
    code.syscall();
    // bind(sock, &addr, 16)
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -16);
    code.lea_r_mem(Reg::Rsi, Reg::Rbp, -40);
    code.movabs(Reg::Rdx, 16);
    code.movabs(Reg::Rax, SYS_BIND);
    code.syscall();
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x84, ok);
    code.movabs(Reg::Rax, 0xFFFFFFFFFFFFFFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_listen(sock, backlog) -> Int`: Start listening.
fn emit_net_listen(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, 16); // sock
    code.mov_r_mem(Reg::Rsi, Reg::Rbp, 24); // backlog
    code.movabs(Reg::Rax, SYS_LISTEN);
    code.syscall();
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x84, ok);
    code.movabs(Reg::Rax, 0xFFFFFFFFFFFFFFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_accept(sock) -> Int`: Accept connection. Returns new fd or -1.
fn emit_net_accept(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, 16); // sock
    code.xor_rr32(Reg::Rsi, Reg::Rsi); // addr = NULL
    code.xor_rr32(Reg::Rdx, Reg::Rdx); // addrlen = NULL
    code.movabs(Reg::Rax, SYS_ACCEPT);
    code.syscall();
    // accept returns fd >= 0 on success, negative errno on error
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x89, ok); // jns: non-negative = success
    code.movabs(Reg::Rax, 0xFFFFFFFFFFFFFFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.leave_ret();
}

/// `rt_net_send(sock, data) -> Int`: Send data. Returns bytes sent or -1.
/// sendto syscall: rdi=fd, rsi=buf, rdx=len, r10=flags, r8=dest_addr, r9=addrlen
fn emit_net_send(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, 16); // sock
    // data is a Str: [len:8][bytes...] at [rbp+24]
    code.mov_r_mem(Reg::R10, Reg::Rbp, 24); // data ptr (temp)
    code.lea_r_mem(Reg::Rsi, Reg::R10, 8); // data+8 = byte array
    code.mov_r_mem(Reg::Rdx, Reg::R10, 0); // length (first 8 bytes of Str)
    code.xor_rr32(Reg::R10, Reg::R10); // flags = 0 (must use r10 for sendto)
    code.xor_rr32(Reg::R8, Reg::R8); // dest_addr = NULL
    code.xor_rr32(Reg::R9, Reg::R9); // addrlen = 0
    code.movabs(Reg::Rax, SYS_SENDTO);
    code.syscall();
    // sendto returns bytes sent on success, negative errno on error
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x89, ok); // jns: non-negative = success
    code.movabs(Reg::Rax, 0xFFFFFFFFFFFFFFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.leave_ret();
}

/// `rt_net_recv(sock, maxlen) -> Str`: Receive data into BSS buffer, return as Str.
/// recvfrom syscall: rdi=fd, rsi=buf, rdx=len, r10=flags, r8=src_addr, r9=addrlen
fn emit_net_recv(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16); // [rbp-8]=byte_count, [rbp-16]=saved_str_ptr
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, 16); // sock
    // Receive into BSS recv_buf+8 (skip 8-byte length prefix)
    code.lea_r_rip(Reg::Rsi, PatchKind::Bss(NET_RECV_BUF_BSS + 8));
    code.mov_r_mem(Reg::Rdx, Reg::Rbp, 24); // maxlen
    code.xor_rr32(Reg::R10, Reg::R10); // flags = 0 (must use r10 for recvfrom)
    code.xor_rr32(Reg::R8, Reg::R8); // src_addr = NULL
    code.xor_rr32(Reg::R9, Reg::R9); // addrlen = NULL
    code.movabs(Reg::Rax, SYS_RECVFROM);
    code.syscall();
    code.mov_mem_r(Reg::Rbp, -8, Reg::Rax); // save byte count
    // Check for error (negative)
    code.test_rr(Reg::Rax, Reg::Rax);
    let is_ok = code.label();
    code.jcc_label(0x89, is_ok); // jns: non-negative = ok
    // Error: return empty string
    code.sub_rsp(8);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.u8(0x50); // push 0
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.leave_ret();
    code.bind_label(is_ok);
    // Check for zero-length (connection closed)
    code.test_rr(Reg::Rax, Reg::Rax);
    let positive = code.label();
    code.jcc_label(0x8F, positive); // jg: positive length
    // Zero: return empty string
    code.sub_rsp(8);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.u8(0x50);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.leave_ret();
    code.bind_label(positive);
    // Allocate MINK Str(length) and copy data
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, -8); // length
    code.sub_rsp(8);
    code.u8(0x50); // push length
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    // RAX = allocated Str ptr. Save it.
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax);
    // Copy: BSS recv_buf+8 -> Str+8, for length bytes
    code.lea_r_rip(Reg::R10, PatchKind::Bss(NET_RECV_BUF_BSS + 8));
    code.add_r_imm8(Reg::Rax, 8); // Str data start
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
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16);
    code.leave_ret();
}

/// `rt_net_close(sock) -> Int`: Close socket. Returns 0 on success.
fn emit_net_close(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, 16); // sock
    code.movabs(Reg::Rax, SYS_CLOSE);
    code.syscall();
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x84, ok);
    code.movabs(Reg::Rax, 0xFFFFFFFFFFFFFFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_shutdown(sock, how) -> Int`: Shutdown socket.
fn emit_net_shutdown(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rdi, Reg::Rbp, 16); // sock
    code.mov_r_mem(Reg::Rsi, Reg::Rbp, 24); // how
    code.movabs(Reg::Rax, SYS_SHUTDOWN);
    code.syscall();
    code.test_rr(Reg::Rax, Reg::Rax);
    let ok = code.label();
    code.jcc_label(0x84, ok);
    code.movabs(Reg::Rax, 0xFFFFFFFFFFFFFFFFu64);
    code.leave_ret();
    code.bind_label(ok);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_getaddrinfo(host, port) -> Str`: V1 — returns host string as-is.
fn emit_net_get_addr_info(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // host ptr
    code.leave_ret();
}

/// `rt_net_freeaddrinfo()`: V1 no-op.
fn emit_net_free_addr_info(code: &mut Code) {
    prologue(code);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_net_gethostname() -> Str`: Get local hostname via uname(2).
fn emit_net_get_host_name(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16); // [rbp-8] = length, [rbp-16] = saved string ptr
    // uname struct on stack: 65 * 5 = 325 bytes for 5 fields (sysname, nodename, ...)
    // We only need nodename at offset 65. Use BSS recv_buf as temp.
    code.lea_r_rip(Reg::Rdi, PatchKind::Bss(NET_RECV_BUF_BSS));
    code.movabs(Reg::Rax, SYS_UNAME);
    code.syscall();
    // nodename is at offset 65 in utsname (each field is 65 bytes)
    code.lea_r_rip(Reg::R10, PatchKind::Bss(NET_RECV_BUF_BSS + 65));
    // Scan for null terminator to compute length
    code.xor_rr32(Reg::R8, Reg::R8); // length counter
    let scan = code.label();
    let scan_done = code.label();
    code.bind_label(scan);
    code.cmp_r_imm32(Reg::R8, 63);
    code.jcc_label(0x83, scan_done);
    code.movzx_byte(Reg::R9, Reg::R10, 0);
    code.test_rr(Reg::R9, Reg::R9);
    code.jcc_label(0x84, scan_done);
    code.add_r_imm8(Reg::R10, 1);
    code.add_r_imm8(Reg::R8, 1);
    code.jmp_label(scan);
    code.bind_label(scan_done);
    code.mov_mem_r(Reg::Rbp, -8, Reg::R8); // save length
    // Allocate Str(length)
    code.mov_rr(Reg::Rdi, Reg::R8);
    code.sub_rsp(8);
    code.u8(0x50);
    code.call_patch(PatchKind::RuntimeService(RuntimeService::StrAlloc));
    code.add_rsp(16);
    code.mov_mem_r(Reg::Rbp, -16, Reg::Rax); // save string ptr
    // Copy: BSS recv_buf+65 -> Str+8, for length bytes
    code.lea_r_rip(Reg::R10, PatchKind::Bss(NET_RECV_BUF_BSS + 65));
    code.add_r_imm8(Reg::Rax, 8);
    code.mov_rr(Reg::R8, Reg::Rax);
    code.mov_r_mem(Reg::R9, Reg::Rbp, -8);
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
    code.mov_r_mem(Reg::Rax, Reg::Rbp, -16);
    code.leave_ret();
}

/// `rt_net_htons(value) -> Int`: Host-to-network byte order (16-bit).
fn emit_net_htons(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16); // value
    code.xchg_al_ah(); // swap low 2 bytes = htons
    code.leave_ret();
}

// ===========================================================================
// Crypto services (Linux) — kernel getrandom(2), no external libraries
// ===========================================================================

/// `rt_crypto_init() -> Int`: No provider initialization is required on
/// Linux — getrandom(2) is a direct syscall backed by the kernel CSPRNG.
/// Always returns 0 (success).
fn emit_crypto_init(code: &mut Code) {
    prologue(code);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_crypto_random_bytes(buf, len) -> Int`: Fill `buf` with `len` secure
/// random bytes via getrandom(2) (flags = 0: the kernel CSPRNG). Loops on
/// short reads so the whole buffer is filled; returns 0 on success and -1
/// on error. On success the string length prefix is written, matching the
/// Windows (BCryptGenRandom) contract.
fn emit_crypto_random_bytes(code: &mut Code) {
    prologue(code);
    // r8 = data pointer (buf + 8, past the length prefix); r9 = remaining.
    // Both survive the syscall (the kernel only clobbers rcx/r11).
    code.mov_r_mem(Reg::R8, Reg::Rbp, 16); // buf
    code.add_r_imm8(Reg::R8, 8); // skip the 8-byte length prefix
    code.mov_r_mem(Reg::R9, Reg::Rbp, 24); // len
    let loop_start = code.label();
    let done = code.label();
    let error = code.label();
    code.bind_label(loop_start);
    code.test_rr(Reg::R9, Reg::R9);
    code.jcc_label(0x8E, done); // jle — buffer filled
    code.mov_rr(Reg::Rdi, Reg::R8); // buf
    code.mov_rr(Reg::Rsi, Reg::R9); // count
    code.xor_rr32(Reg::Rdx, Reg::Rdx); // flags = 0
    code.movabs(Reg::Rax, SYS_GETRANDOM);
    code.syscall();
    // rax = bytes read (positive) or a negative errno
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x8E, error); // jle — error (incl. EINTR): fail
    code.add_rr(Reg::R8, Reg::Rax); // advance the data pointer
    code.sub_rr(Reg::R9, Reg::Rax); // decrement the remaining count
    code.jmp_label(loop_start);
    code.bind_label(error);
    code.movabs(Reg::Rax, 0xFFFFFFFFFFFFFFFFu64);
    code.leave_ret();
    code.bind_label(done);
    // Set the string length prefix (Windows parity)
    code.mov_r_mem(Reg::R11, Reg::Rbp, 16); // buf
    code.mov_r_mem(Reg::R10, Reg::Rbp, 24); // len
    code.mov_mem_r(Reg::R11, 0, Reg::R10);
    code.xor_rr32(Reg::Rax, Reg::Rax);
    code.leave_ret();
}

/// `rt_crypto_random_int() -> Int`: Return 8 secure random bytes from
/// getrandom(2) as a 64-bit integer. On error, returns 0 (Windows parity).
fn emit_crypto_random_int(code: &mut Code) {
    prologue(code);
    code.sub_rsp(8); // 8-byte scratch buffer at [rsp]
    let ok = code.label();
    code.mov_rr(Reg::Rdi, Reg::Rsp); // buf
    code.movabs(Reg::Rsi, 8u64); // count = 8
    code.xor_rr32(Reg::Rdx, Reg::Rdx); // flags = 0
    code.movabs(Reg::Rax, SYS_GETRANDOM);
    code.syscall();
    code.test_rr(Reg::Rax, Reg::Rax);
    code.jcc_label(0x8F, ok); // jg — 8 bytes read
    code.xor_rr32(Reg::Rax, Reg::Rax); // error: return 0
    code.leave_ret();
    code.bind_label(ok);
    code.mov_r_mem(Reg::Rax, Reg::Rsp, 0);
    code.leave_ret();
}

/// `rt_crypto_secure_zero(ptr, len)`: Securely zero `len` bytes at `ptr`.
/// A byte-by-byte loop (the write cannot be optimized away because the
/// machine runtime is hand-emitted assembly). Unlike the Windows version
/// this stores exactly `len` bytes — the Windows implementation writes
/// 32-bit zeros while advancing by one byte, which can touch up to three
/// bytes past the requested region.
fn emit_crypto_secure_zero(code: &mut Code) {
    prologue(code);
    code.mov_r_mem(Reg::R10, Reg::Rbp, 16); // ptr
    code.mov_r_mem(Reg::R11, Reg::Rbp, 24); // len
    code.xor_rr32(Reg::Rax, Reg::Rax); // zero source
    let loop_start = code.label();
    let done = code.label();
    code.bind_label(loop_start);
    code.test_rr(Reg::R11, Reg::R11);
    code.jcc_label(0x84, done); // je
    code.mov_mem_r8(Reg::R10, 0, Reg::Rax); // byte [ptr] = 0
    code.add_r_imm8(Reg::R10, 1);
    code.sub_r_imm32(Reg::R11, 1);
    code.jmp_label(loop_start);
    code.bind_label(done);
    code.xor_rr32(Reg::Rax, Reg::Rax);
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

    // --- Map/Set services (stub; Linux is FROZEN) ---
    emit!(RuntimeService::MapNew, emit_stub_int);
    emit!(RuntimeService::MapInsert, emit_stub_int);
    emit!(RuntimeService::MapGet, emit_stub_int);
    emit!(RuntimeService::MapHas, emit_stub_int);
    emit!(RuntimeService::MapRemove, emit_stub_int);
    emit!(RuntimeService::MapLen, emit_stub_int);
    emit!(RuntimeService::MapFree, emit_stub_int);
    emit!(RuntimeService::MapKeys, emit_stub_int);
    emit!(RuntimeService::MapValues, emit_stub_int);
    emit!(RuntimeService::SetNew, emit_stub_int);
    emit!(RuntimeService::SetInsert, emit_stub_int);
    emit!(RuntimeService::SetHas, emit_stub_int);
    emit!(RuntimeService::SetRemove, emit_stub_int);
    emit!(RuntimeService::SetLen, emit_stub_int);
    emit!(RuntimeService::SetFree, emit_stub_int);
    emit!(RuntimeService::SetElements, emit_stub_int);

    // --- Internal collection helpers (stub; Linux is FROZEN) ---
    emit!(RuntimeService::CollFreeValue, emit_stub_int);
    emit!(RuntimeService::CollCloneValue, emit_stub_int);
    emit!(RuntimeService::CollHash, emit_stub_int);
    emit!(RuntimeService::CollKeyEq, emit_stub_int);
    emit!(RuntimeService::MapRebuild, emit_stub_int);
    emit!(RuntimeService::SetRebuild, emit_stub_int);

    // --- Process services ---
    emit!(RuntimeService::ProcessId, emit_process_id);
    emit!(RuntimeService::ProcessRun, emit_process_run);
    emit!(RuntimeService::ProcessStdout, emit_process_stdout);
    emit!(RuntimeService::ProcessStderr, emit_process_stderr);
    emit!(RuntimeService::ProcessStdoutLen, emit_process_stdout_len);
    emit!(RuntimeService::ProcessStderrLen, emit_process_stderr_len);

    // --- Time services ---
    emit!(RuntimeService::TimeNow, emit_time_now);
    emit!(RuntimeService::TimeMillis, emit_time_millis);
    emit!(RuntimeService::TimeTicks, emit_time_ticks);
    emit!(RuntimeService::TimeFreq, emit_time_freq);
    emit!(RuntimeService::TimeFiletime, emit_time_filetime);
    emit!(RuntimeService::TimeFiletimeHigh, emit_time_filetime_high);

    // --- Random services ---
    emit!(RuntimeService::RandomSeed, emit_random_seed);
    emit!(RuntimeService::RandomNext, emit_random_next);

    // --- Environment services ---
    emit!(RuntimeService::EnvGet, emit_env_get);
    emit!(RuntimeService::EnvSet, emit_env_set);
    emit!(RuntimeService::EnvHas, emit_env_has);
    emit!(RuntimeService::EnvRemove, emit_env_remove);

    // --- Session 99 services (Linux frozen: deterministic stubs so the
    // shared service table stays total; no Linux program calls these) ---
    emit!(RuntimeService::Sleep, |code: &mut Code| {
        emit_linux_stub_unit(code)
    });
    emit!(RuntimeService::StderrWrite, |code: &mut Code| {
        emit_linux_stub_minus_one(code)
    });
    emit!(RuntimeService::StdinRead, |code: &mut Code| {
        emit_linux_stub_empty_str(code)
    });
    emit!(RuntimeService::Argc, |code: &mut Code| {
        emit_linux_stub_zero(code)
    });
    emit!(RuntimeService::Argv, |code: &mut Code| {
        emit_linux_stub_empty_str(code)
    });
    emit!(RuntimeService::ArgvParse, |code: &mut Code| {
        emit_linux_stub_unit(code)
    });
    emit!(RuntimeService::StrFromFloat, |code: &mut Code| {
        emit_linux_stub_empty_str(code)
    });
    emit!(RuntimeService::StrFormat, |code: &mut Code| {
        emit_linux_stub_empty_str(code)
    });

    // --- Network services (Linux) ---
    emit!(RuntimeService::NetWsaStartup, emit_net_wsa_startup);
    emit!(RuntimeService::NetWsaCleanup, emit_net_wsa_cleanup);
    emit!(RuntimeService::NetWsaLastError, emit_net_wsa_last_error);
    emit!(RuntimeService::NetSocket, emit_net_socket);
    emit!(RuntimeService::NetConnect, emit_net_connect);
    emit!(RuntimeService::NetBind, emit_net_bind);
    emit!(RuntimeService::NetListen, emit_net_listen);
    emit!(RuntimeService::NetAccept, emit_net_accept);
    emit!(RuntimeService::NetSend, emit_net_send);
    emit!(RuntimeService::NetRecv, emit_net_recv);
    emit!(RuntimeService::NetClose, emit_net_close);
    emit!(RuntimeService::NetShutdown, emit_net_shutdown);
    emit!(RuntimeService::NetGetAddrInfo, emit_net_get_addr_info);
    emit!(RuntimeService::NetFreeAddrInfo, emit_net_free_addr_info);
    emit!(RuntimeService::NetGetHostName, emit_net_get_host_name);
    emit!(RuntimeService::NetHtons, emit_net_htons);

    // --- Crypto services (Linux: kernel getrandom(2)) ---
    emit!(RuntimeService::CryptoInit, emit_crypto_init);
    emit!(RuntimeService::CryptoRandomBytes, emit_crypto_random_bytes);
    emit!(RuntimeService::CryptoRandomInt, emit_crypto_random_int);
    emit!(RuntimeService::CryptoSecureZero, emit_crypto_secure_zero);

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
