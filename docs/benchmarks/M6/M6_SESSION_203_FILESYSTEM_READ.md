# Session 203 — Windows Filesystem Read Correctness

Windows x86_64, 2026-11-07. Starting commit `9cc6ad7` (Session 202, local ==
origin/main verified before any change).

## 1. Objective

Independently verify the Session 198 finding that `emit_fs_read` ignores the
BOOL and byte count returned by `ReadFile`, and correct it if the code still
behaves that way. Do not assume the earlier observation is current.

## 2. Files inspected

- `src/backend/emit/runtime.rs`: `emit_fs_read` (the whole emitter),
  `emit_fs_write` for the shared `.bss` bytes-written scratch, `emit_str_alloc`
  (length prefix and zeroing), `emit_free`/`emit_str_free` (ownership).
- `src/runtime/intrinsics.rs`: `rt_fs_read` declaration.
- `src/backend/lower.rs`, `src/backend/ir.rs`: service mapping and contract.
- `stdlib/filesystem.mink`: `fs_read` ownership wrapper.
- `tests/filesystem_lib.rs`, `tests/windows_hardening.rs`,
  `tests/resource_lifecycle.rs`, `tests/soak.rs`: existing filesystem tests.
- `docs/language/STANDARD_LIBRARY_REFERENCE.md` §6 and
  `docs/implementation/RUNTIME_IMPLEMENTATION.md`.
- Win32 `ReadFile`, `GetFileSize`, `LockFile` semantics.

## 3. Findings

The concern was **confirmed and still present**:

1. `emit_fs_read` sizes the result with `GetFileSize(handle, NULL)` and calls
   `StrAlloc(size)`, which writes the requested size into the string's length
   prefix.
2. `ReadFile(handle, result + 8, size, &bytes_written, NULL)` is called, and
   both its BOOL return value and the transferred-byte count are discarded.
3. The allocated buffer is returned regardless of the result. Its tail is
   uninitialised on a fresh bump and stale on free-list reuse, and the length
   prefix claims the full file size.

Reachability: Win32 byte-range locks are honoured by `ReadFile` even when the
file was opened with `FILE_SHARE_READ`, so a `ReadFile` can fail while
`GetFileSize` succeeds. A short read is also possible for device/redirector
handles and after concurrent truncation on filesystems that relax sharing
rules. The contract "read the entire file" must therefore distinguish
success, short transfer and failure.

Two secondary observations, not changed in this session:

- The byte count was written to the shared `.bss` slot `bytes_written`, also
  used by `emit_fs_write`. Using it for the returned length would have added a
  cross-thread length race, so the fix uses a per-call stack slot instead.
- `GetFileSize` is a 32-bit API. Files at or above 4 GiB are outside the
  supported range (the heap ceiling is 1 GiB); this is documented, not fixed.

## 4. Reproduction

`tests/fs_read.rs::fsr06_failed_read_returns_owned_empty_and_recovers` holds a
`LockFile` range over a 4,096-byte file and runs a MINK program that calls
`fs_read`, prints `rt_str_len`, frees the result and exits. With the old code:

```
thread 'fsr06_failed_read_returns_owned_empty_and_recovers' panicked at
tests\fs_read.rs:325:5:
assertion `left == right` failed: a failing ReadFile must return an owned
empty Str, not 4096 bytes over uninitialised memory (stderr="")
  left: Some(4096)
 right: Some(0)
```

After releasing the lock, the same executable prints `4096`. No race or
test-only runtime seam is involved: the lock is held for the whole first run.

## 5. Fix

`src/backend/emit/runtime.rs`, `emit_fs_read`:

- the spill frame grows from 32 to 64 bytes; `[rbp-40]` is the byte count,
  zeroed before the call because `ReadFile` writes a DWORD;
- `ReadFile` receives `lea r9, [rbp-40]` and the buffer address is loaded from
  `[rbp-32]` instead of relying on `rax` (which the zeroing clobbers);
- the BOOL is tested. On success the length prefix becomes the transferred
  byte count (64-bit load from the zeroed slot, so the high half is defined).
  On failure the prefix becomes 0, the documented "cannot read" result,
  matching the missing-file path; the block stays owned and freeable, so the
  caller's `rt_str_free` remains correct;
- the shared `BSS.bytes_written` scratch is no longer read by this path, so
  concurrent reads cannot cross lengths.

The `fs_read` wrapper and all other filesystem services are unchanged, and
there is no ABI change.

## 6. Tests added

`tests/fs_read.rs` (6 tests, all execute native Windows PE built by the real
CLI; each frees its result and exits with the leak check armed):

| Test | Asserts |
| --- | --- |
| `fsr01_small_file_read_exact` | 37-byte binary file: length and first/last bytes |
| `fsr02_empty_file_is_owned_empty` | empty file → length 0, freeable |
| `fsr03_large_file_read_exact` | 256 KiB pattern: length, checksum mod 251, first/last |
| `fsr04_repeated_reads_are_stable` | 100 reads of the same file, no mismatch |
| `fsr05_missing_file_is_owned_empty` | missing path → owned empty string |
| `fsr06_failed_read_returns_owned_empty_and_recovers` | locked range: length 0 (not 4096); after unlock the same exe reads 4096 |

## 7. Verification

| Check | Result |
| --- | --- |
| `cargo test --test fs_read` (debug) | **6 passed, 0 failed** — 0.04–0.05 s, repeated 4× |
| `cargo test --release --test fs_read` | **6 passed, 0 failed** (0.05 s) |
| Red check (old code, `fsr06` only) | **failed as expected**: reported 4096 over uninitialised memory |
| Full regression, every target except `tls_lib` | **80 suites, 3,224 passed, 0 failed, 7 ignored** |
| Baseline reconciliation | 3,218 + 6 new = 3,224; 79 + `fs_read` = 80 suites |
| `tls_lib` | not runnable here: `python3` is the Microsoft Store alias stub; environmental limit, unchanged |
| `rustfmt --check tests/fs_read.rs` | clean |
| `rustfmt --check src/backend/emit/runtime.rs` | one pre-existing diff at line 380 (net emitter, untouched); the changed `emit_fs_read` region is formatted |

The locked-read test is deterministic: the lock is taken before the process
starts and released only after it exits, so `ReadFile` cannot succeed during
the measured run.

## 8. Known limitations

- The failure path is exercised through a byte-range lock; the same fixed code
  path also covers short reads, which cannot be produced deterministically
  through the public API on a local NTFS file while MINK holds a
  `FILE_SHARE_READ` handle.
- `GetFileSize`'s 32-bit size and `INVALID_FILE_SIZE` are not separately
  diagnosed. A read of a ≥ 4 GiB file is outside the supported range and may
  report a small low-DWORD length; the 1 GiB heap ceiling rejects allocations
  above it with `E-R02` (102).
- No changelog file exists in this repository; the roadmap block, the
  readiness-audit addendum, the runtime document and this record are the
  change log.

## 9. Documentation

- `docs/language/STANDARD_LIBRARY_REFERENCE.md` §6: `fs_read` contract, exact
  byte-count semantics, failure result and 4 GiB limit.
- `docs/implementation/RUNTIME_IMPLEMENTATION.md` §5: filesystem-read
  subsection with the stack-slot byte count.
- `docs/roadmap/IMPLEMENTATION_ROADMAP.md`: dated Session 203 status.
- `docs/audits/SYSTEMS_READINESS_AUDIT.md`: dated Session 203 addendum.

## 10. Verdict

**PASS — confirmed defect fixed.** `fs_read` now returns exactly the bytes
`ReadFile` transferred, an owned empty string on failure, and never exposes
the uninitialised tail of the allocation. Six permanent tests cover exact
lengths, empty and missing files, repeated reads, and the deterministic
failure/recovery path; debug and release suites and the full regression are
green.
