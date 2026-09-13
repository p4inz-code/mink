# Session 108 — S28 Directory listing / traversal (P1 closure)

**Predecessor:** Session 107 (L05 UTF-8 text layer) at `66e43639`.
**Scope:** Windows x86_64 Python-capability parity. Linux untouched (frozen).

## Capability

`os.listdir` / `os.scandir` / `os.walk` parity for Windows: enumerate the
entries of a directory through the language's own runtime and stdlib.

## Design (MINK-native)

A streaming trio rather than a collected list, mirroring `os.scandir()`:

| Layer | Surface |
|---|---|
| Runtime service | `RuntimeService::{FsDirOpen, FsDirNext, FsDirClose}` |
| Intrinsic | `rt_dir_open(path: Str) -> Int`, `rt_dir_next(handle: Int) -> Str`, `rt_dir_close(handle: Int) -> Int` |
| Stdlib | `fs_dir_open`, `fs_dir_next`, `fs_dir_close` |

- `rt_dir_open` appends `\*` to the path, calls `FindFirstFileA`, and returns
  an opaque handle to a heap block holding the Win32 find handle plus the
  `WIN32_FIND_DATAA` record. The handle is an `Int` because MINK's type system
  has no pointer/null comparison, so `handle == 0` must be expressible.
- `rt_dir_next` calls `FindNextFileA`, copies `cFileName` into a fresh `Str`,
  and skips the `FakeDirectoryEntries` `.` and `..` so the stream matches
  `os.scandir()`/`os.listdir()`.
- `rt_dir_close` calls `FindClose` and frees the handle block.
- `os.walk` is built by composing the trio with `path_join`; no separate
  runtime recursion is needed (proven by `d09`).

The handle is an **owned allocation**, so forgetting `fs_dir_close` is
reported by `rt_exit` as `E-R06` (leak) with exit code 106 — the same
ownership rule as every other MINK allocation. This is a deliberate
Python-capability-parity choice: Python's `os.scandir` leaks a file handle
if not closed; MINK makes the equivalent mistake a hard, deterministic error.

## Verification (native Windows PE)

Permanent regression: `tests/filesystem_lib.rs` `d01`–`d11`.

| Test | Covers |
|---|---|
| d01 | entry count in a fixture tree (dot entries skipped) |
| d02 | empty directory → 0 entries |
| d03 | missing directory → handle 0 |
| d04 | null handle: `next` returns empty, `close` returns -1 |
| d05 | trailing path separator |
| d06 | path containing a space |
| d07 | two interleaved enumerations stay independent |
| d08 | 300 open/enumerate/close cycles, no drift and no leak |
| d09 | one-level walk via `path_join` + nested enumeration |
| d10 | 45-entry directory |
| d11 | unclosed handle → `E-R06`, exit 106 |

Additional session evidence: `cargo build`, `cargo build --release`,
`cargo fmt --check`, `cargo clippy --all-targets` (no new findings),
`cargo test --test filesystem_lib` (45 passed),
`--test runtime` (24), `--test smoke` (13), `--test backend` (46),
`--test cli/modules_check/optimization/adversarial` (211), and the npm
mirror `npm/mink/stdlib/filesystem.mink` is byte-identical to `stdlib/`.

## Known limits (documented, not parity-blocking for S28)

- Entry order is filesystem order, not sorted.
- Names are ANSI (`FindFirstFileA`), sharing the FS layer's MAX_PATH/ANSI
  limits (see matrix W03).
- No per-entry metadata (see S31) and no glob (see S29); a recursive
  `os.walk` helper is buildable from the streaming trio and demonstrated.

## Matrix effect

- S28: MISSING → **VERIFIED** (`Blocks = N`, `Pri = -`).
- Aggregates recomputed from the rows: VERIFIED 73 → **74**,
  MISSING 93 → **92**, PARTIAL 37, total 232; P1 parity blockers 26 → **25**;
  rows requiring work 171 → **170**.
