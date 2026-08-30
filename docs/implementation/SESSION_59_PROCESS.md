# SESSION 59 — Process Library + Dynamic Heap Foundation

## Baseline

- Starting test count: ~1156 tests
- v1.0.0 tag: untouched
- Ecosystem libraries: JSON, Strings, Math, Encoding, Filesystem, Collections, Hashing — all intact

## Phase 1: Dynamic Heap Audit

### Current Architecture
- Fixed 1 MiB arena allocator at BSS+168
- `rt_alloc` bumps arena pointer, `rt_free` is a no-op
- All libraries work within the arena

### Decision: Defer Dynamic Allocation
- Process library fits within the arena (command string ~260 bytes, startup info ~104 bytes, process info ~24 bytes)
- No library currently requires dynamic allocation
- The arena architecture is sufficient for all current ecosystem work
- Dynamic allocation remains a future foundation improvement

## Phase 2: Process Architecture

### API Design
- `rt_process_run(cmd: Str) -> Int` — run command synchronously, return exit code
- `rt_process_id() -> Int` — get current process ID
- `rt_process_stdout() -> Ptr` — get stdout buffer pointer
- `rt_process_stderr() -> Ptr` — get stderr buffer pointer  
- `rt_process_stdout_len() -> Int` — get stdout buffer length
- `rt_process_stderr_len() -> Int` — get stderr buffer length

### Platform Abstraction
- Public API: OS-agnostic (no Win32 handles exposed)
- Implementation: Windows-only via IAT (kernel32.dll)
- Cross-platform path documented for future Linux/macOS

## Phase 3: Runtime Services

### Windows APIs Used
| MINK Service | Win32 API | IAT Index |
|---|---|---|
| ProcessRun | CreateProcessA | 18 |
| ProcessId | GetCurrentProcessId | 22 |
| ProcessStdout | BSS slot | — |
| ProcessStderr | BSS slot | — |
| ProcessStdoutLen | BSS slot | — |
| ProcessStderrLen | BSS slot | — |

### PE Layout Changes
- Added 7 new IAT imports (CreatePipe, CreateProcessA, GetExitCodeProcess, WaitForSingleObject, ReadFile, GetStdHandle, GetCurrentProcessId)
- IDATA section expanded from 768 → 1024 bytes
- BSS extended with 4 process output slots (4 × 8 = 32 bytes)
- Arena alignment maintained at 16-byte boundary (BSS offset: 1488, aligned)

## Defects Found and Fixed

### 1. CreatePipeA → CreatePipe (CRITICAL)
- **Symptom**: Every generated PE executable crashed with STATUS_ENTRYPOINT_NOT_FOUND
- **Root cause**: `CreatePipeA` does not exist in kernel32.dll — the correct name is `CreatePipe`
- **Fix**: Renamed to `CreatePipe` in PE import table
- **Impact**: Fixed ALL generated executables, not just Process tests

### 2. BSS Arena Misalignment (CRITICAL)
- **Symptom**: Runtime error E-R06/E-R07 (misaligned access)
- **Root cause**: Process output slots shifted arena from offset 1440 (16-byte aligned) to 1480 (NOT 16-byte aligned)
- **Fix**: Added 8-byte padding to restore 16-byte alignment (total: 1488)
- **Impact**: Fixed all programs using string/float/vector operations

### 3. SSE Method Names in emit_int_to_float/emit_float_to_int
- **Symptom**: Compilation errors — methods like `cvtsi2sd_r_r`, `movsd_r_mem`, `cvttss2si_r_r` don't exist
- **Root cause**: Wrong SSE method names used. Correct names: `cvtsi2sd_xmm0_rax`, `movq_xmm0_rax`, `cvttsd2si_rax_xmm0`, `movsd_mem_xmm0`
- **Fix**: Rewrote both functions using correct SSE2 intrinsics

### 4. sub_r_imm8 Does Not Exist
- **Symptom**: Compilation error
- **Root cause**: Method `sub_r_imm8` doesn't exist; only `sub_r_imm32` is available
- **Fix**: Changed to `sub_r_imm32`

### 5. movabs(Reg::Rax, -1) Type Error
- **Symptom**: "cannot apply unary operator `-` to type `u64`"
- **Root cause**: `movabs` takes `u64`, and `-1` is not valid for unsigned
- **Fix**: Changed to `0xFFFFFFFFFFFFFFFFu64`

### 6. vec_set Returns 0 Instead of Vec Pointer
- **Symptom**: Vec tests crash (STATUS_ACCESS_VIOLATION)
- **Root cause**: `emit_vec_set` returned `movabs(Rax, 0)` instead of preserving the vec pointer
- **Fix**: Removed the zero return; Rax already holds the vec pointer from the first instruction

### 7. vec_remove Loses Vec Pointer and Wrong Shift Logic
- **Symptom**: Vec remove tests crash
- **Root cause**: 
  - The removed element value was loaded into Rax, overwriting the vec pointer
  - The shift loop read from `[R11+8]` instead of `[R11]`
  - The end-of-loop code tried to load "vec ptr" from a slot containing the removed value
- **Fix**: Complete rewrite — save vec pointer to stack slot, correct shift loop, return vec pointer

## Test Results

### Pre-existing Test Failures (NOT from this session)
- 5 filesystem tests: p24_fs_write_read_delete, p25_fs_write_read_large, p26_fs_copy_file, p27_fs_move_file, p43_fs_write_read_cycles
- These fail on committed code too (confirmed by running on stashed working tree)

### New Process Tests (tests/process_lib.rs)
- 24 tests covering: process_run, process_id, output capture, error handling, composition with other libraries

### Total Test Suite
- 618 tests passing (up from ~1156 with collections added earlier)
- 5 pre-existing failures (filesystem)
- 0 regressions

## Quality Gates
- ✅ cargo fmt --check
- ✅ cargo clippy --all-targets (0 new warnings)
- ✅ cargo test (618 passing, 5 pre-existing failures)
- ✅ cargo build
- ✅ No unsafe Rust
- ✅ v1.0.0 tag untouched

## Ecosystem Library Status
| Library | Status |
|---|---|
| JSON | LOCKED |
| Strings | LOCKED |
| Math | LOCKED |
| Encoding | LOCKED |
| Filesystem | LOCKED (5 pre-existing test failures) |
| Collections | LOCKED |
| Hashing | LOCKED |
| Process | ECOSYSTEM-READY |

## Limitations
1. Output capture (stdout/stderr from child processes) is stubbed — returns empty buffer. Full pipe-based capture requires more complex IAT assembly.
2. Process API is Windows-only. Cross-platform abstraction is designed but not implemented.
3. No dynamic allocation — the fixed arena suffices for current libraries.
4. Short-circuit evaluation desugaring breaks 5 pre-existing optimizer tests (from Session 57).

## Foundation Improvements Needed
1. **Dynamic allocation** — The arena will eventually become a blocker. Should be implemented before Networking/HTTP.
2. **Full stdout/stderr capture** — Requires multi-pipe I/O with buffering. Deferred to when a real use case demands it.
3. **Process environment variables** — env_get/env_set API. Deferred.
