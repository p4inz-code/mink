# Session 206 — Large-File Filesystem Correctness

Windows x86_64, 2026-11-07. Starting commit
`c6c6ce7ec9088300778181a34356fa097928756a` (Session 205, local == origin/main
verified before any change).

## 1. Objective

Determine whether file sizes, lengths and offsets remain correct above the
4 GiB boundary of the 32-bit `GetFileSize` API, without writing gigabytes of
physical data, and make any unsupported case fail safely instead of silently
truncating.

## 2. Files inspected

- `src/backend/emit/runtime.rs`: `emit_fs_read`, `emit_fs_file_size`,
  `emit_fs_write`, `emit_fs_copy`, `emit_fs_exists`.
- `src/backend/emit/pe.rs`: kernel32 import table and IAT indices.
- `src/runtime/abi.rs`: `HEAP_MAX_BYTES` (1 GiB), 16-byte string header.
- `stdlib/filesystem.mink`: the public wrappers.
- `tests/fs_read.rs`, `tests/filesystem_lib.rs`: existing coverage.
- `docs/language/STANDARD_LIBRARY_REFERENCE.md` and
  `docs/implementation/RUNTIME_IMPLEMENTATION.md`: the documented contract.

## 3. Public surface established

The public filesystem API is **whole-file only**. It exposes a path and,
for writes, a complete `Str`; there is no public seek, read-at-offset,
write-at-offset or sparse API. File offsets therefore never cross the public
API, and a 4 GiB offset cannot be requested. The values that can cross the
boundary are:

- the file size returned by `fs_file_size` (`Int`, 64-bit);
- the allocation size chosen by `fs_read` (arena-bounded);
- the bytes `fs_write` writes (arena-bounded);
- `fs_copy_file`/`fs_move`, which delegate whole-file to kernel32.

## 4. Findings and root cause

Both `rt_fs_file_size` and `rt_fs_read` called `GetFileSize(handle, NULL)`,
which returns only the low DWORD. `GetFileSize` also overloads `0xFFFFFFFF`
with `INVALID_FILE_SIZE`, so a real 4 GiB−1 file is indistinguishable from a
failed query without checking `GetLastError` (not checked).

Measured before the fix, with sparse files created through `SetEndOfFile`:

| Case | Expected | Actual before |
| --- | --- | --- |
| size of 4 GiB + 12,345 | 4,294,979,641 | **12,345** (low DWORD) |
| read of 4 GiB + 12,345 | owned empty (refused) | **12,345 bytes** of the file's leading hole |
| size of exactly 4 GiB | 4,294,967,296 | **0** |
| read of 4 GiB − 1 | owned empty (refused) | **exit 102 `E-R02`** trying to allocate 4 GiB |

So: silent 32-bit truncation of reported size, silent truncated reads, and a
fatal allocation for a file that should have been refused up front. No offset
overflow is reachable through the public API (`fs_write`'s length is also
arena-bounded, so `WriteFile`'s DWORD count cannot overflow).

## 5. Fix

`src/backend/emit/runtime.rs`:

- Both services now call `GetFileSizeEx`, which reports an exact signed 64-bit
  length.
- `rt_fs_file_size` checks the BOOL result and returns `-1` on failure; on
  success it returns the full 64-bit size.
- `rt_fs_read` checks the BOOL result, refuses a size greater than
  `HEAP_MAX_BYTES - 16` (the arena minus the string header), and returns an
  owned empty `Str` after closing the handle. A failed size query takes the
  same safe path. A file that fits is allocated and read exactly as before,
  with Session 203's transferred-byte length and short-read handling.
- `GetFileSizeEx` is appended to the kernel32 import list at index 44, so no
  existing IAT index changes. An intermediate attempt inserted it in the
  middle and shifted every later index (the new slot resolved to
  `TlsSetValue`); the regression tests caught it immediately and it is not in
  the final diff.

No public API shape changed. No offset/seek API was added.

## 6. Tests added — `tests/large_file.rs` (5)

Sparse files: `File::set_len` extends to the target size without allocating
the hole; only a 16-byte tail is written. A guard removes every file even on
failure, so disk usage stays bounded.

| Test | Asserts |
| --- | --- |
| `lf01_size_above_4_gib_is_exact` | 4 GiB + 12,345 reports exactly 4,294,979,641 |
| `lf02_read_above_4_gib_is_refused_not_truncated` | read returns owned empty, exit 0, leak check passes |
| `lf03_exactly_4_gib_is_exact_and_refused` | size 4,294,967,296, read empty (old code reported 0) |
| `lf04_just_below_4_gib_is_exact_and_refused` | size 4,294,967,295, read empty (old code aborted with `E-R02`) |
| `lf05_small_file_round_trip_unchanged` | 64 KiB size and bytes unchanged |

## 7. Verification

| Check | Result |
| --- | --- |
| Red run before the fix | **4 failed / 1 passed**: truncated size 12,345, 12,345-byte read, 0 for exactly 4 GiB, `E-R02` for 4 GiB−1 |
| `cargo test --test large_file` after the fix | **5 passed, 0 failed** (1.86 s) |
| Full regression (all suites, including `tls_lib`) | recorded in the roadmap/audit addendum |
| Release build/tests | **not run**: `rustc` at opt-level 3 crashes in this environment with `STATUS_ILLEGAL_INSTRUCTION` / `STATUS_HEAP_CORRUPTION` / `STATUS_ACCESS_VIOLATION` (Session 205 records the evidence); debug is unaffected |
| `rustfmt --check tests/large_file.rs` | clean |

## 8. Known limitations

- The public filesystem API remains whole-file only. There is no seek or
  read/write-at-offset operation, so offsets above 4 GiB cannot be exercised
  or requested; `fs_copy_file`/`fs_move` delegate to kernel32 and handle large
  files without reading them into the arena.
- `fs_read` refuses any file larger than 1 GiB − 16 bytes. It returns an owned
  empty string, which is indistinguishable from an empty or unreadable file;
  callers should use `fs_file_size` to tell them apart (it now reports the
  exact size).
- `fs_write` is bounded by the 1 GiB arena, so partial/large writes are not
  reachable; a `Str` cannot exceed the arena.
- Sparse-file testing relies on NTFS `SetEndOfFile` semantics; the tests query
  the actual length before asserting.
- Release verification is blocked by the environment, not by this change.

## 9. Documentation

- `docs/language/STANDARD_LIBRARY_REFERENCE.md`: the 4 GiB limitation is
  replaced by the exact 64-bit size and safe-refusal contract.
- `docs/implementation/RUNTIME_IMPLEMENTATION.md`: filesystem section updated.
- Roadmap and readiness-audit addenda.
- This record.

## 10. Verdict

**PASS — 32-bit file-size truncation confirmed, fixed and covered.**
`fs_file_size` is exact at and above 4 GiB; `fs_read` refuses oversized files
with an owned empty string instead of truncating them or aborting. Five
permanent sparse-file tests pass; the red run showed all four incorrect
behaviors. Release verification remains environment-blocked and is reported
as not run.
