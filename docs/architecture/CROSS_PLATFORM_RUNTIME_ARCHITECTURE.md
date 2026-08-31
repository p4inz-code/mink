# MINK Cross-Platform Runtime Architecture

**Status:** Design Document (Session 83)
**Date:** August 31, 2026

---

## 1. Goals

1. Enable MINK to produce native executables on Linux (x86_64) while preserving the existing Windows x64 implementation
2. Share the compiler pipeline (lexer → parser → AST → semantic analysis → type checking → ownership → HIR → MIR → optimization) across all platforms
3. Abstract platform-specific runtime services behind a clean boundary
4. Maintain zero external dependencies for the compiler itself
5. Keep the generated executables self-contained (no runtime DLLs/shared libraries required)

## 2. Non-Goals

1. macOS support (future, not in scope)
2. ARM64 support (future, not in scope)
3. Cross-compilation (compile on Windows for Linux or vice versa)
4. Runtime compatibility between Windows and Linux executables
5. Changing the MINK language syntax or semantics for cross-platform support

## 3. Current Windows Architecture

```
┌─────────────────────────────────────────────────────┐
│                    MINK Source                        │
└──────────────────────┬──────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────┐
│              Compiler Pipeline (Platform-Neutral)     │
│  Lexer → Parser → AST → Semantic → TypeCheck →       │
│  Ownership → HIR → MIR → Optimize                    │
└──────────────────────┬──────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────┐
│           Backend (Target-Specific)                   │
│  ┌────────────────────────────────────────────────┐  │
│  │ x86_64-windows-pe                              │  │
│  │  • PE/COFF image format                        │  │
│  │  • kernel32.dll imports (IAT)                   │  │
│  │  • x86-64 machine code                         │  │
│  │  • Runtime services (emit/runtime.rs)           │  │
│  └────────────────────────────────────────────────┘  │
└──────────────────────┬──────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────┐
│           Runtime Services (Windows)                  │
│  • Print: kernel32 GetStdHandle/WriteFile            │
│  • Exit: kernel32 ExitProcess                        │
│  • Alloc: kernel32 VirtualAlloc/VirtualFree          │
│  • FS: kernel32 CreateFileA/ReadFile/WriteFile       │
│  • Process: kernel32 CreateProcessA                  │
│  • Time: kernel32 GetSystemTimeAsFileTime            │
│  • Env: kernel32 GetEnvironmentVariableA             │
│  • Net: ws2_32.dll (loaded at runtime)               │
│  • Crypto: bcryptprimitives.dll (loaded at runtime)  │
└─────────────────────────────────────────────────────┘
```

### Key Windows-Specific Components

| Component | File | Windows API |
|-----------|------|-------------|
| PE image format | `backend/emit/pe.rs` | N/A (format) |
| x86-64 code gen | `backend/emit/x86_64.rs` | N/A (ISA) |
| Kernel32 imports | `backend/emit/pe.rs` | kernel32.dll IAT |
| Print/Exit | `backend/emit/runtime.rs` | GetStdHandle, WriteFile, ExitProcess |
| Memory alloc | `backend/emit/runtime.rs` | VirtualAlloc, VirtualFree |
| Filesystem | `backend/emit/runtime.rs` | CreateFileA, ReadFile, WriteFile |
| Process | `backend/emit/runtime.rs` | CreateProcessA |
| Time | `backend/emit/runtime.rs` | GetSystemTimeAsFileTime |
| Environment | `backend/emit/runtime.rs` | GetEnvironmentVariableA |
| Networking | `backend/emit/runtime.rs` | ws2_32.dll (dynamic load) |
| Crypto/Random | `backend/emit/runtime.rs` | bcryptprimitives.dll (dynamic load) |

## 4. Platform-Neutral Compiler Boundary

The following components are already platform-neutral and require NO changes for Linux support:

- **Lexer** (`src/lexer/`) — tokenizes source text
- **Parser** (`src/parser/`) — produces AST
- **AST** (`src/ast/`) — abstract syntax tree
- **Semantic Analysis** (`src/semantics/`) — name resolution, scope analysis
- **Type Checking** (`src/typecheck/`) — type inference, unification
- **Ownership Analysis** (`src/ownership/`) — move/borrow checking
- **HIR** (`src/hir/`) — high-level IR
- **MIR** (`src/mir/`) — mid-level IR + optimization
- **Monomorphization** (`src/monomorphize/`) — generic instantiation
- **Module System** (`src/module/`) — multi-file compilation
- **Diagnostics** (`src/diagnostics/`) — error reporting
- **Source Infrastructure** (`src/source/`) — file loading, spans

## 5. Runtime Boundary

The runtime boundary is the set of intrinsic functions (`rt_*`) that the compiler emits calls to. These are the ONLY interface between generated code and the OS.

### Current Runtime Intrinsics

| Category | Intrinsics | Windows Implementation |
|----------|-----------|----------------------|
| Memory | rt_alloc, rt_free, rt_mem_load, rt_mem_store | VirtualAlloc/VirtualFree |
| String | rt_str_alloc, rt_str_free, rt_str_len, rt_str_byte, rt_str_set_byte, rt_str_concat, rt_str_eq, rt_str_from_int, rt_str_from_bool | HeapAlloc/HeapFree |
| I/O | rt_print_str, rt_print_int, rt_print_float, rt_print_char | GetStdHandle/WriteFile |
| Exit | rt_exit | ExitProcess |
| Vec | rt_vec_new, rt_vec_push, rt_vec_get, rt_vec_set, rt_vec_len, rt_vec_free, rt_vec_pop, rt_vec_remove | HeapAlloc/HeapFree |
| Numeric | rt_int_to_float, rt_float_to_int | SSE2 instructions |
| Filesystem | rt_fs_read, rt_fs_write, rt_fs_exists, rt_fs_file_size, rt_fs_create_dir, rt_fs_remove_dir, rt_fs_remove_file, rt_fs_copy, rt_fs_move, rt_fs_get_cwd, rt_fs_set_cwd | kernel32 file APIs |
| Process | rt_process_run, rt_process_stdout, rt_process_stderr, rt_process_stdout_len, rt_process_stderr_len, rt_process_id | CreateProcessA |
| Time | rt_time_now, rt_time_millis, rt_time_ticks, rt_time_freq | GetSystemTimeAsFileTime, QueryPerformanceCounter |
| Random | rt_random_seed, rt_random_next | xorshift64* (pure algorithm) |
| Environment | rt_env_get, rt_env_set, rt_env_has, rt_env_remove | GetEnvironmentVariableA, SetEnvironmentVariableA |
| Network | rt_net_wsa_startup, rt_net_wsa_cleanup, rt_net_wsa_last_error, rt_net_socket, rt_net_connect, rt_net_bind, rt_net_listen, rt_net_accept, rt_net_send, rt_net_recv, rt_net_close, rt_net_shutdown, rt_net_getaddrinfo, rt_net_freeaddrinfo, rt_net_gethostname, rt_net_htons | ws2_32.dll (Winsock2) |
| Crypto | rt_crypto_init, rt_crypto_random_bytes, rt_crypto_random_int, rt_crypto_secure_zero | bcryptprimitives.dll (BCryptGenRandom) |
| C FFI | rt_to_cstr, rt_free_cstr | N/A (conversion helpers) |

## 6. Platform Abstraction Boundary

For Linux support, the following abstraction is required:

```
┌─────────────────────────────────────────────────────┐
│           Compiler Pipeline (unchanged)              │
└──────────────────────┬──────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────┐
│           Backend (target-specific)                   │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
│  │x86_64-win-pe│  │x86_64-linux │  │ (future)    │  │
│  │  (existing) │  │    -elf     │  │             │  │
│  └─────────────┘  └─────────────┘  └─────────────┘  │
└──────────────────────┬──────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────┐
│     Runtime Services (platform-specific)              │
│  ┌─────────────┐  ┌─────────────┐                   │
│  │  Windows    │  │   POSIX/    │                   │
│  │  (existing) │  │   Linux     │                   │
│  └─────────────┘  └─────────────┘                   │
└─────────────────────────────────────────────────────┘
```

## 7. Windows Backend (Existing)

The Windows backend generates PE (Portable Executable) images with:
- DOS header + PE signature
- COFF header (x86-64, machine 0x8664)
- Optional header (PE32+)
- Section headers (.text, .rdata, .data, .bss, .idata, .reloc)
- Import Address Table (kernel32.dll)
- x86-64 machine code
- Runtime service implementations (embedded in .text)

**Key files:**
- `src/backend/emit/pe.rs` — PE image format
- `src/backend/emit/x86_64.rs` — x86-64 code generation
- `src/backend/emit/runtime.rs` — Windows runtime services

## 8. POSIX/Linux Backend (Planned)

The Linux backend will generate ELF (Executable and Linkable Format) images with:
- ELF header (x86-64, ET_EXEC)
- Program headers (LOAD segments)
- Section headers (.text, .rodata, .data, .bss, .symtab)
- Dynamic linking (libc, libpthread)
- x86-64 machine code (same ISA as Windows)
- Runtime service implementations (POSIX syscalls/libc)

**Key differences from Windows:**
- ELF format instead of PE
- Linux syscalls instead of WinAPI
- libc dependency (printf, malloc, fork/exec, etc.)
- No IAT (uses .dynamic/.plt/.got for dynamic linking)

## 9. Linux x86_64 Executable Strategy

### Option A: libc-linked (Recommended for V1)
- Link against glibc/musl
- Use libc functions for runtime services
- Simpler implementation, proven approach
- Requires libc at runtime (standard on all Linux distros)

### Option B: Static syscalls (Future optimization)
- Use raw Linux syscalls (no libc dependency)
- Fully self-contained executables
- More complex, requires syscall number knowledge
- Better for embedded/containerized environments

**Recommendation:** Start with Option A (libc-linked) for rapid Linux support. Option B can be added later as an optimization.

## 10. ELF Requirements

For libc-linked ELF executables:

1. **ELF Header:** 64 bytes, identifies x86-64, ET_EXEC, entry point
2. **Program Headers:** LOAD segments for code (.text) and data (.data, .bss)
3. **Section Headers:** For debugging/symbol resolution
4. **Dynamic Section:** libc/libpthread dependency declarations
5. **PLT/GOT:** For dynamic function resolution
6. **Entry Point:** _start → __libc_start_main → main

## 11. Filesystem Abstraction

| Operation | Windows API | Linux Syscall/libc |
|-----------|------------|-------------------|
| Open/Read | CreateFileA + ReadFile | open() + read() |
| Write | CreateFileA + WriteFile | open() + write() |
| Exists | GetFileAttributesA | stat() |
| File Size | GetFileSize | stat() |
| Create Dir | CreateDirectoryA | mkdir() |
| Remove Dir | RemoveDirectoryA | rmdir() |
| Remove File | DeleteFileA | unlink() |
| Copy | CopyFileA | copy_file_range() or read+write |
| Move | MoveFileA | rename() |
| Get CWD | GetCurrentDirectoryA | getcwd() |
| Set CWD | SetCurrentDirectoryA | chdir() |

## 12. Process Abstraction

| Operation | Windows API | Linux Syscall/libc |
|-----------|------------|-------------------|
| Run command | CreateProcessA | fork() + execvp() |
| Get stdout | ReadFile from pipe | read() from pipe |
| Get stderr | ReadFile from pipe | read() from pipe |
| Get PID | GetCurrentProcessId | getpid() |

## 13. Environment Abstraction

| Operation | Windows API | Linux Syscall/libc |
|-----------|------------|-------------------|
| Get var | GetEnvironmentVariableA | getenv() |
| Set var | SetEnvironmentVariableA | setenv() |
| Has var | GetEnvironmentVariableA (check len) | getenv() (check NULL) |
| Remove var | SetEnvironmentVariableA("") | unsetenv() |

## 14. Networking Abstraction

| Operation | Windows API | Linux API |
|-----------|------------|-----------|
| Init | WSAStartup (ws2_32.dll) | N/A ( sockets always available) |
| Cleanup | WSACleanup | N/A |
| Create socket | socket() via ws2_32 | socket() |
| Connect | connect() via ws2_32 | connect() |
| Bind | bind() via ws2_32 | bind() |
| Listen | listen() via ws2_32 | listen() |
| Accept | accept() via ws2_32 | accept() |
| Send | send() via ws2_32 | send() |
| Recv | recv() via ws2_32 | recv() |
| Close | closesocket() via ws2_32 | close() |
| Shutdown | shutdown() via ws2_32 | shutdown() |
| Get hostname | gethostname() via ws2_32 | gethostname() |
| Byte order | htons() via ws2_32 | htons() |

**Note:** Linux sockets are always available (no WSAStartup needed). The networking abstraction can be simplified significantly on Linux.

## 15. Time Abstraction

| Operation | Windows API | Linux API |
|-----------|------------|-----------|
| Wall clock | GetSystemTimeAsFileTime | clock_gettime(CLOCK_REALTIME) |
| Monotonic | QueryPerformanceCounter | clock_gettime(CLOCK_MONOTONIC) |
| Frequency | QueryPerformanceFrequency | clock_getres() |

## 16. Random Abstraction

| Operation | Windows API | Linux API |
|-----------|------------|-----------|
| Secure random | BCryptGenRandom | getrandom() or /dev/urandom |
| PRNG seed | xorshift64* state | Same (pure algorithm) |
| PRNG next | xorshift64* | Same (pure algorithm) |

## 17. Crypto Abstraction

| Operation | Windows API | Linux API |
|-----------|------------|-----------|
| Init | LoadLibrary(bcryptprimitives.dll) | N/A |
| Secure random | BCryptGenRandom | getrandom() |
| Secure zero | SecureZeroMemory | explicit_bzero() |

## 18. Error Model

The error model is platform-neutral:
- Runtime errors use structured codes (E-R01 through E-R10)
- Error messages are embedded in the executable
- Exit codes are standardized (0=success, non-zero=error)
- No platform-specific error reporting required

## 19. ABI Compatibility

- **x86-64 calling convention:** Windows uses Microsoft x64 (RCX, RDX, R8, R9); Linux uses System V AMD64 (RDI, RSI, RDX, RCX, R8, R9)
- **Stack alignment:** Both require 16-byte alignment before CALL
- **Red zone:** Linux has 128-byte red zone; Windows does not
- **Return value:** Both use RAX for integer returns

## 20. Testing Strategy

1. **Unit tests:** Existing Rust test suite (62 unit tests + integration tests)
2. **Cross-platform tests:** Run same .mink source on both Windows and Linux
3. **Runtime tests:** Verify all intrinsics produce identical results
4. **Binary tests:** Verify ELF format correctness with readelf/objdump
5. **Integration tests:** Build and run demo projects on both platforms

## 21. CI Strategy

1. **Windows CI:** Existing GitHub Actions on Windows runners
2. **Linux CI:** Add Ubuntu runners for Linux builds
3. **Cross-validation:** Run same test suite on both platforms
4. **Release builds:** Produce both .exe (Windows) and ELF (Linux) artifacts

## 22. Migration Strategy

1. **Phase 1:** Abstract runtime services behind a trait/interface
2. **Phase 2:** Implement Linux runtime services using POSIX/libc
3. **Phase 3:** Implement ELF image generation
4. **Phase 4:** Cross-platform integration testing
5. **Phase 5:** Release Linux binary alongside Windows binary

## 23. Risks

1. **ABI differences:** Calling convention changes require careful code generation
2. **libc dependency:** Linux executables will depend on libc (acceptable for V1)
3. **Testing complexity:** Need CI runners for both platforms
4. **Feature parity:** Some Windows features may not have direct Linux equivalents
5. **Performance:** libc calls may be slightly slower than raw syscalls

## 24. Explicit Unresolved Questions

1. Should Linux support static linking (musl) or dynamic linking (glibc)?
2. Should the ELF backend support both static and dynamic linking?
3. How should the module system handle platform-specific stdlib files?
4. Should the npm package include Linux binaries or use a separate package?
5. What is the minimum glibc version to target?
