# SESSION 77 — Process Release Blocker Removal

## 1. Baseline

| Library | Before |
|---------|--------|
| Crypto | 18/18 ✅ |
| Random | 15/15 ✅ |
| Collections | 24/24 ✅ |
| Networking | 26/26 ✅ |
| HTTP | 35/35 ✅ |
| Filesystem | 33/33 ✅ |
| **Process** | **13/25** ❌ |

## 2. Failure Matrix

12 failures, all involving echo commands or stdout capture:

| Test | Description | Symptom |
|------|-------------|---------|
| p02 | echo returns zero | Exit code 1 instead of 0 |
| p05 | stdout captured | stdout_len = 0 |
| p06 | stdout has content | stdout_len = 0 |
| p08 | process_run_ok for echo | Returns false |
| p12 | stdout content match | E-R05 (string validation) |
| p13 | stdout len positive | stdout_len = 0 |
| p14 | repeated execution | stdout_len = 0 |
| p18 | direct rt_process_run echo | Exit code 1 |
| p19 | direct stdout_len | stdout_len = 0 |
| p21 | process with string ops | E-R05 (string validation) |
| p22 | many process runs | stdout_len = 0 |
| p23 | command with arguments | stdout_len = 0 |

## 3. Native Win32 Reference

Created minimal C reference implementations:
- `proc_test2.c`: Tests NULL vs inheritable security on CreatePipe
- `proc_test3.c`: Tests both security approaches

**Key finding**: Native Win32 with identical parameters returns exit code 0 for `cmd.exe /c echo hello`, confirming the bug is in the MINK emitter, not a Windows platform limitation.

## 4. Root Causes

### Root Cause 1: Non-inheritable pipe handles

`CreatePipe` was called with `NULL` security attributes, creating handles where `bInheritHandle = FALSE`. The MSDN documentation for `STARTF_USESTDHANDLES` explicitly states: "The handles must be inheritable."

With non-inheritable handles:
- The child process receives an invalid stdout/stderr handle via `STARTF_USESTDHANDLES`
- `cmd.exe` detects the invalid handle and returns exit code 1 for any output-producing command
- `ReadFile` on the pipe returns 0 bytes (child never wrote to the pipe)

### Root Cause 2: StrValidate missing BSS buffer check

`emit_str_validate` only recognized:
1. Live heap blocks (liveness table scan)
2. Image string-data region (str_data_start to str_data_end)

The stdout/stderr BSS capture buffers (`BSS.stdout_buf`, `BSS.stderr_buf`) were NOT recognized as valid string pointers. Any call to `rt_str_byte`, `rt_str_len`, or other string operations on captured process output triggered E-R05.

## 5. Fixes

### Fix 1: SECURITY_ATTRIBUTES + SetHandleInformation

```
Build SECURITY_ATTRIBUTES at [rbp-544]:
  nLength = 24
  lpSecurityDescriptor = NULL
  bInheritHandle = TRUE

CreatePipe(read, write, &SA, 0)    // handles now inheritable
SetHandleInformation(read, HANDLE_FLAG_INHERIT, 0)  // parent-side read = non-inheritable
```

This is the standard Windows pipe creation pattern:
1. Create pipes with inheritable handles (child gets write end via STARTUPINFOA)
2. Immediately remove inheritance from parent-side read handles (prevents child from inheriting them)

Frame extended from 512 to 544 bytes to accommodate the SA structure.

### Fix 2: StrValidate BSS buffer recognition

Added two `cmp_rr + je` checks before the image string-data bounds check:
```asm
lea rcx, [stdout_buf]
cmp rax, rcx
je valid
lea rcx, [stderr_buf]
cmp rax, rcx
je valid
```

Only applied in non-heap-only mode (i.e., `StrValidate`, not `StrValidateHeap`), preserving the mutation protection on BSS buffers.

## 6. Results After Fix

| Library | After | Status |
|---------|-------|--------|
| Crypto | 18/18 | ✅ |
| Random | 15/15 | ✅ |
| Collections | 24/24 | ✅ |
| Networking | 26/26 | ✅ |
| HTTP | 35/35 | ✅ |
| Filesystem | 33/33 | ✅ |
| **Process** | **25/25** | **✅ COMPLETE** |

## 7. Quality Gates

- ✅ `cargo fmt` clean
- ✅ `cargo clippy --all-targets` (0 errors)
- ✅ `cargo build` successful
- ✅ `cargo build --release` successful
- ✅ HEAD == origin/main
- ✅ Working tree clean
- ✅ No regressions

## 8. Security / Resource Audit

| Concern | Status |
|---------|--------|
| Handle leaks | ✅ All handles closed in cleanup path |
| Pipe handle inheritance | ✅ Write ends inheritable, read ends non-inheritable |
| Buffer overflow | ✅ ReadFile bounded to 4088 bytes |
| Stack corruption | ✅ Frame extended for SA, all calls properly aligned |
| Deadlock | ✅ WaitForSingleObject before ReadFile prevents pipe-fill deadlock |

## 9. Release Blocker Classification

**RELEASE BLOCKER REMOVED**

All ecosystem libraries pass:
- Strings ✅
- Math ✅
- Encoding ✅
- Hashing ✅
- JSON ✅
- Time ✅
- Random: 15/15 ✅
- Collections: 24/24 ✅
- Crypto: 18/18 ✅
- Environment ✅
- Networking: 26/26 ✅
- HTTP: 35/35 ✅
- Filesystem: 33/33 ✅
- Process: 25/25 ✅
