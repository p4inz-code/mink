# SESSIONS 75 + 76 — FILESYSTEM COMPLETION + PROCESS PROGRESS

**Date:** August 30, 2026
**Status:** Checkpoint — FILESYSTEM COMPLETE, PROCESS PARTIAL (13/25)

---

## SESSION 75 — FILESYSTEM COMPLETION ✅

### Baseline
- Filesystem: 24/33 (9 failing)
- Root causes identified and fixed

### Root Causes Fixed

#### Bug 1: `emit_to_cstr` string offset
- **Problem:** `to_cstr` copied string data from `s+16` but MINK string format is `[8-byte length][data]`, so data starts at `s+8`.
- **Fix:** Changed offset from `s+16` to `s+8` in `emit_to_cstr`.
- **Impact:** Every filesystem CStr was garbage. All 9 failures partially caused by this.

#### Bug 2: Stack alignment in filesystem emitters
- **Problem:** Several filesystem emitters allocated non-16-aligned amounts before Win32 API calls.
- **Fix:** Aligned all sub_rsp allocations to 16-byte boundaries.
- **Impact:** Prevented access violations in Win32 API call paths.

#### Bug 3: `emit_free_cstr` clobbered RAX
- **Problem:** `emit_free_cstr` called `Free` which returns 0 in RAX, destroying the caller's return value.
- **Fix:** All filesystem functions now save the return value to a spill slot before calling `free_cstr`, then reload after.
- **Impact:** All filesystem functions that used CStr (exists, file_size, copy, move, etc.) returned 0.

#### Bug 4: `emit_fs_exists` comparison logic
- **Problem:** `cmp_r_imm32` uses REX.W which sign-extends the immediate, comparing against `0xFFFFFFFFFFFFFFFF` instead of DWORD `-1`.
- **Fix:** Replaced with CreateFileA approach using `TEST EAX, EAX` + `JS` to check sign bit (INVALID_HANDLE has high bit set).
- **Impact:** `fs_exists` always returned 1 for nonexistent files.

#### Bug 5: Win32 BOOL convention
- **Problem:** CopyFileA, MoveFileA, CreateDirectoryA, RemoveDirectoryA return nonzero=TRUE=success, but MINK tests expect 0=success.
- **Fix:** Added TEST EAX/EAX; JNZ success; INC EAX; JMP done; success: XOR EAX,EAX pattern to invert BOOL results.
- **Impact:** 3 tests expected 0 for success but got 1.

#### Bug 6: CreateFileA for directories
- **Problem:** `fs_exists` couldn't detect directories because CreateFileA requires `FILE_FLAG_BACKUP_SEMANTICS` (0x02000000) in dwFlagsAndAttributes for directory handles.
- **Fix:** Set dwFlagsAndAttributes to 0x02000000 in the CreateFileA call.
- **Impact:** `p28_fs_create_remove_dir` failed.

### Result
- **Filesystem: 33/33 ✅**

### Quality Gates
- cargo fmt: ✅
- cargo clippy: ✅ (0 errors)
- cargo build: ✅
- cargo build --release: ✅

---

## SESSION 76 — PROCESS PROGRESS (13/25)

### Baseline
- Process: 9/25 (16 failing)

### Bugs Fixed

#### Bug 1: IAT indices missing for process APIs
- **Problem:** `IAT_CREATE_PIPE_A`, `IAT_CREATE_PROCESS_A`, `IAT_WAIT_FOR_SINGLE_OBJECT`, `IAT_GET_EXIT_CODE_PROCESS` constants were missing.
- **Fix:** Added all missing IAT constants mapped to correct KERNEL32_IMPORTS indices.

#### Bug 2: Accessor functions used wrong `mov` direction
- **Problem:** `emit_process_stdout_len` and `emit_process_stderr_len` used `mov_rip_r` (STORE to BSS) instead of `mov_r_rip` (LOAD from BSS).
- **Fix:** Changed to `mov_r_rip` for loading values from BSS.

#### Bug 3: BSS stores used wrong direction in process_run
- **Problem:** 12+ instances of `mov_rip_r` (STORE) used where `lea_r_rip` (load address) or `mov_r_rip` (load value) was needed in `emit_process_run`.
- **Fix:** Changed all to correct direction: `lea_r_rip` for getting BSS slot addresses, `mov_r_rip` for loading values.

#### Bug 4: Alloc call pushed wrong value
- **Problem:** `emit_process_run` called Alloc with `push rax` but RAX held the CStr pointer, not the size. Alloc received a huge pointer value as the size.
- **Fix:** Computed `len + 12` in RAX, then pushed the correct size.

#### Bug 5: STARTUPINFOA hStdOutput/hStdError offsets wrong
- **Problem:** hStdOutput was at offset +80 (hStdInput's position) and hStdError at +88 (hStdOutput's position). Should be +88 and +96.
- **Fix:** Corrected to offsets 88 and 96.

#### Bug 6: Missing space in command prefix
- **Problem:** "cmd.exe /c" had no space before the command. Was "cmd.exe /cecho" instead of "cmd.exe /c echo".
- **Fix:** Changed prefix from 10 bytes to 11 bytes with space at position 10.

#### Bug 7: CreateProcessA lpApplicationName not zeroed
- **Problem:** `xor_rr32(Reg::Rcx, Reg::Rax)` with RAX=0 does `RCX = RCX ^ 0 = RCX` (unchanged!). RCX contained stale data from CreatePipeA.
- **Fix:** Changed to `xor_rr32(Reg::Rcx, Reg::Rcx)` to actually zero RCX.

#### Bug 8: Exit code slot overlap
- **Problem:** Exit code at `[rbp-52]` overlapped with stderr_write handle at `[rbp-48]`. `mov_r_mem` loaded 8 bytes, picking up handle data in upper4 bytes.
- **Fix:** Moved exit code to `[rbp-56]` and zeroed it before GetExitCodeProcess.

### Remaining Process Failures (12)

**Root cause under investigation:** `cmd.exe /c echo hello` returns exit code 1 (instead of 0) when executed through our CreateProcessA with piped stdout. Verified:
- Python `subprocess.run(["cmd.exe", "/c", "echo", "hello"])` returns exit code 0 ✅
- PowerShell `Start-Process cmd.exe '/c echo hello'` returns exit code 0 ✅
- Our MINK `process_run("echo hello")` returns exit code 1 ❌
- stdout capture reads 0 bytes ❌

The 12 failing tests are ALL ones that use `echo` or expect stdout content:
- p02, p05, p06, p08, p12, p13, p14, p18, p19, p21, p22, p23

The 13 passing tests work correctly:
- p01 (pid positive), p03 (invalid nonzero), p04 (empty no crash), p07 (stderr no crash), p09 (invalid false), p10 (valid nonempty), p11 (invalid empty), p15 (exit 42), p16 (multiple exit codes), p17 (direct pid), p20 (no output), p24 (exit7 preserved), p25 (exit 200 preserved)

**Hypothesis:** The ReadFile call after pipe creation may be interfering with the child process. ReadFile on a pipe can block if data isn't available yet, potentially causing timing issues with WaitForSingleObject. When read is placed before wait, ReadFile blocks until the child writes data. When read is after wait, the child has already exited but ReadFile returns 0 bytes (pipe may be in a bad state).

### Current Ecosystem Matrix

| Library | Passing | Total | Status |
|---------|---------|-------|--------|
| Strings | — | — | ✅ (used by others) |
| Math | — | — | ✅ |
| Encoding | — | — | ✅ |
| Hashing | — | — | ✅ |
| JSON | — | — | ✅ |
| Time | — | — | ✅ |
| Random | 15 | 15 | ✅ |
| Collections | 24 | 24 | ✅ |
| Crypto | 18 | 18 | ✅ |
| Environment | — | — | ✅ |
| Networking | 26 | 26 | ✅ |
| HTTP | 35 | 35 | ✅ |
| **Filesystem** | **33** | **33** | **✅ COMPLETE** |
| **Process** | **13** | **25** | **⚠️ PARTIAL** |

### Quality Gates
- cargo fmt: ✅
- cargo clippy: ✅ (0 errors, 39 warnings)
- cargo build: ✅
- cargo build --release: ✅

### Known Limitations
1. **Process stdout capture:** ReadFile returns 0 bytes when reading from pipe after WaitForSingleObject. Need to investigate pipe state after child exit.
2. **Echo exit code:** `cmd.exe /c echo hello` returns 1 through our CreateProcessA but 0 through Python/PowerShell. May be related to stdin handle state.
3. **Process V1 limitations:** Windows-only, bounded 4088-byte capture, no timeout, no stdin support, cmd.exe routing.

---

*Generated with Codebuff 🤖*
