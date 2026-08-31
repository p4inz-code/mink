# MINK Linux x86_64 Implementation Plan

**Status:** Implementation Blueprint (Session 83)
**Date:** August 31, 2026

---

## Overview

This plan details the dependency-ordered phases for adding Linux x86_64 support to MINK. Each phase builds on the previous one and can be independently verified.

---

## Phase A: Platform Abstraction Layer

**Prerequisites:** None
**Affected files:** `src/backend/emit/runtime.rs`, new `src/runtime/posix.rs` (conceptual)
**Expected outputs:** Abstract interface for runtime services
**Tests:** Existing Windows tests must continue to pass
**Failure risks:** Breaking existing Windows functionality
**Completion criteria:** All existing tests pass; abstraction interface defined

### Tasks
1. Define a `RuntimeService` trait/interface that both Windows and Linux implementations will satisfy
2. Refactor `emit/runtime.rs` to use the abstraction internally
3. Ensure all existing Windows intrinsics still work through the abstraction
4. Add documentation for the abstraction boundary

---

## Phase B: Runtime Portability

**Prerequisites:** Phase A
**Affected files:** `src/backend/emit/runtime.rs`, `src/runtime/`
**Expected outputs:** POSIX runtime service implementations
**Tests:** New unit tests for POSIX implementations
**Failure risks:** Incorrect syscall usage, ABI mismatches
**Completion criteria:** All POSIX runtime services implemented and tested

### Tasks
1. Implement POSIX file I/O (open, read, write, close, stat)
2. Implement POSIX process management (fork, exec, wait)
3. Implement POSIX environment (getenv, setenv)
4. Implement POSIX time (clock_gettime)
5. Implement POSIX random (getrandom)
6. Implement POSIX networking (socket, bind, listen, accept, connect, send, recv)
7. Implement POSIX crypto (getrandom for secure random)
8. Implement POSIX memory (mmap/munmap for allocation)
9. Implement POSIX print (write to stdout/stderr)

---

## Phase C: Linux Filesystem

**Prerequisites:** Phase B
**Affected files:** `src/backend/emit/runtime.rs`
**Expected outputs:** Working filesystem intrinsics on Linux
**Tests:** Filesystem integration tests on Linux
**Failure risks:** Path handling differences (case sensitivity, separators)
**Completion criteria:** All filesystem operations work on Linux

### Tasks
1. Implement rt_fs_read using open()+read()
2. Implement rt_fs_write using open()+write()
3. Implement rt_fs_exists using stat()
4. Implement rt_fs_file_size using stat()
5. Implement rt_fs_create_dir using mkdir()
6. Implement rt_fs_remove_dir using rmdir()
7. Implement rt_fs_remove_file using unlink()
8. Implement rt_fs_copy using copy_file_range() or read+write
9. Implement rt_fs_move using rename()
10. Implement rt_fs_get_cwd using getcwd()
11. Implement rt_fs_set_cwd using chdir()
12. Handle path separator differences (/ vs \)
13. Handle case sensitivity differences

---

## Phase D: Linux Process

**Prerequisites:** Phase B
**Affected files:** `src/backend/emit/runtime.rs`
**Expected outputs:** Working process intrinsics on Linux
**Tests:** Process integration tests on Linux
**Failure risks:** Pipe handling, signal behavior differences
**Completion criteria:** Process execution and output capture work on Linux

### Tasks
1. Implement rt_process_run using fork()+execvp()
2. Implement pipe creation for stdout/stderr capture
3. Implement rt_process_stdout using read() from pipe
4. Implement rt_process_stderr using read() from pipe
5. Implement rt_process_id using getpid()
6. Handle zombie process cleanup (waitpid)
7. Handle process exit code mapping

---

## Phase E: Linux Environment

**Prerequisites:** Phase B
**Affected files:** `src/backend/emit/runtime.rs`
**Expected outputs:** Working environment intrinsics on Linux
**Tests:** Environment integration tests on Linux
**Failure risks:** Environment variable inheritance differences
**Completion criteria:** All environment operations work on Linux

### Tasks
1. Implement rt_env_get using getenv()
2. Implement rt_env_set using setenv()
3. Implement rt_env_has using getenv() with NULL check
4. Implement rt_env_remove using unsetenv()
5. Handle empty string values
6. Handle very long values

---

## Phase F: Linux Networking

**Prerequisites:** Phase B
**Affected files:** `src/backend/emit/runtime.rs`
**Expected outputs:** Working networking on Linux
**Tests:** Networking integration tests on Linux
**Failure risks:** Socket API differences, address resolution
**Completion criteria:** TCP/UDP networking works on Linux

### Tasks
1. Implement rt_net_socket using socket() (no WSAStartup needed)
2. Implement rt_net_connect using connect()
3. Implement rt_net_bind using bind()
4. Implement rt_net_listen using listen()
5. Implement rt_net_accept using accept()
6. Implement rt_net_send using send()
7. Implement rt_net_recv using recv()
8. Implement rt_net_close using close()
9. Implement rt_net_shutdown using shutdown()
10. Implement rt_net_gethostname using gethostname()
11. Implement rt_net_htons using htons()
12. Simplify init/cleanup (no-op on Linux)

---

## Phase G: Linux Time/Random/Crypto

**Prerequisites:** Phase B
**Affected files:** `src/backend/emit/runtime.rs`
**Expected outputs:** Working time, random, and crypto on Linux
**Tests:** Time/random/crypto integration tests on Linux
**Failure risks:** Clock resolution, random quality
**Completion criteria:** Time, random, and crypto work on Linux

### Tasks
1. Implement rt_time_now using clock_gettime(CLOCK_REALTIME)
2. Implement rt_time_millis using clock_gettime(CLOCK_MONOTONIC)
3. Implement rt_time_ticks using clock_gettime(CLOCK_MONOTONIC)
4. Implement rt_time_freq using clock_getres()
5. Implement rt_random_seed (same xorshift64* algorithm)
6. Implement rt_random_next (same xorshift64* algorithm)
7. Implement rt_crypto_random_bytes using getrandom()
8. Implement rt_crypto_random_int using getrandom()
9. Implement rt_crypto_secure_zero using explicit_bzero()

---

## Phase H: ELF Executable Generation

**Prerequisites:** Phases A-G (all runtime services)
**Affected files:** New `src/backend/emit/elf.rs`, `src/backend/emit/mod.rs`
**Expected outputs:** Working ELF executable generation
**Tests:** ELF format validation, executable testing
**Failure risks:** ELF format errors, linker issues, ABI mismatches
**Completion criteria:** MINK programs compile to working Linux ELF executables

### Tasks
1. Implement ELF header generation (ET_EXEC, x86-64)
2. Implement program headers (LOAD segments)
3. Implement section headers (.text, .rodata, .data, .bss)
4. Implement dynamic linking section (libc dependency)
5. Implement PLT/GOT for dynamic function calls
6. Implement entry point (_start → __libc_start_main → main)
7. Implement x86-64 code generation for Linux calling convention (System V AMD64)
8. Handle red zone (128 bytes below RSP)
9. Handle stack alignment (16-byte before CALL)
10. Integrate with existing backend infrastructure
11. Register x86_64-linux-elf target in Target enum

---

## Phase I: Linux CLI Validation

**Prerequisites:** Phase H
**Affected files:** `src/cli.rs`, `src/backend/target.rs`
**Expected outputs:** Working CLI on Linux
**Tests:** CLI integration tests on Linux
**Failure risks:** Path handling, executable permissions
**Completion criteria:** mink check/build/run work on Linux

### Tasks
1. Update Target::native() to return x86_64-linux-elf on Linux
2. Update executable path generation for Linux (no .exe extension)
3. Test mink check on Linux
4. Test mink build on Linux
5. Test mink run on Linux
6. Test mink explain on Linux
7. Verify generated executables have correct permissions (+x)
8. Verify generated executables run without additional setup

---

## Phase J: Cross-Platform Integration Testing

**Prerequisites:** Phase I
**Affected files:** `tests/`, CI configuration
**Expected outputs:** Cross-platform test suite
**Tests:** All existing tests pass on both platforms
**Failure risks:** Platform-specific test assumptions
**Completion criteria:** Full test suite passes on Windows and Linux

### Tasks
1. Set up Linux CI runner (Ubuntu)
2. Port existing test suite to run on Linux
3. Add cross-platform comparison tests
4. Verify demo projects work on Linux
5. Verify npm package works on Linux
6. Add Linux-specific tests for POSIX features
7. Document platform differences
8. Update README for Linux support

---

## Dependencies Summary

```
Phase A (Abstraction)
    │
    ├──→ Phase B (Runtime Portability)
    │        │
    │        ├──→ Phase C (Filesystem)
    │        ├──→ Phase D (Process)
    │        ├──→ Phase E (Environment)
    │        ├──→ Phase F (Networking)
    │        └──→ Phase G (Time/Random/Crypto)
    │
    └──→ Phase H (ELF Generation)
              │
              └──→ Phase I (CLI Validation)
                       │
                       └──→ Phase J (Integration Testing)
```

## Estimated Effort

| Phase | Effort | Risk |
|-------|--------|------|
| A | 1-2 days | Low |
| B | 3-5 days | Medium |
| C | 1-2 days | Low |
| D | 1-2 days | Low |
| E | 0.5-1 day | Low |
| F | 1-2 days | Low |
| G | 1-2 days | Low |
| H | 5-10 days | High |
| I | 1-2 days | Low |
| J | 2-3 days | Medium |
| **Total** | **16-31 days** | |

## Critical Path

The critical path is: A → B → H → I → J

Phases C-G can be done in parallel after Phase B.

Phase H (ELF generation) is the highest-risk and highest-effort phase. It requires:
1. Understanding ELF format in detail
2. Implementing System V AMD64 calling convention
3. Dynamic linking with libc
4. Entry point setup

## Success Criteria

1. All existing Windows tests pass unchanged
2. New Linux tests pass on Ubuntu
3. Demo projects compile and run on Linux
4. Generated ELF executables are self-contained (only depend on libc)
5. npm package includes Linux binary
6. README documents Linux support
7. No regression in Windows functionality
