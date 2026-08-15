# MINK Runtime Implementation

**Status:** Implemented (Sessions 12–13)
**Version:** 0.1.0

This document describes the first real MINK runtime and memory-model
foundation: a small, deterministic, embedded native runtime that every
generated executable carries. It deliberately does **not** implement a
garbage collector or threading; ownership/move semantics are enforced at
compile time by the ownership stage (session 15) before code generation,
not by the runtime (see `docs/implementation/OWNERSHIP_IMPLEMENTATION.md`)
— the memory model is established carefully now so later safety features
have a stable foundation.

## 1. Design boundaries

- **No garbage collector.** Memory is managed explicitly through the
  `rt_alloc` / `rt_free` intrinsics.
- **No unsafe Rust.** The compiler and the runtime emitter are written in
  safe Rust; the runtime itself is emitted machine code (assembled by the
  backend) plus a pure-Rust *reference* implementation used to specify and
  test the semantics.
- **Deterministic.** The heap is a fixed-size arena in the image's `.bss`
  (zero-filled by the loader), allocation is a validated bump allocator
  with LIFO free-list reuse, and every runtime error terminates with a
  stable exit code. Identical sources produce byte-identical images and
  identical runtime behavior.
- **No silent invalid memory operations.** Every access is validated
  against a bounded liveness table; violations are structured runtime
  errors (`E-R01+`), never silent corruption.

## 2. Memory model

- The heap is a fixed arena of [`HEAP_SIZE`] (1 MiB) inside `.bss`
  (`src/runtime/abi.rs` is the single source of truth for the layout).
- Blocks are 16-byte aligned; sizes are rounded up to 16.
- The bump cursor is an **offset from the arena base** (a block lives at
  `arena + offset`). `rt_init` resets the cursor and the free list.
- Every live allocation occupies one slot of a bounded liveness table
  ([`MAX_LIVE_ALLOCS`] = 256 entries of 24 bytes: start, size, live flag).
- `rt_free` requires the exact 16-aligned start of a *live* block; a freed
  block's first word becomes the free-list link (LIFO reuse).
- `rt_mem_load` / `rt_mem_store` require the 8-byte word to lie entirely
  inside a **live** allocation; dead entries are skipped, so use-after-free
  is an error (`E-R05`).
- The pure-Rust reference allocator (`src/runtime/allocator.rs`) mirrors
  the machine runtime exactly and is the executable specification of these
  semantics. Note: in the reference's offset model the first block lives at
  offset 0 (a valid block), whereas the machine runtime's blocks are always
  nonzero absolute addresses (`arena + offset`), so its null check never
  collides with a real block.

## 3. The runtime ABI

Generated code calls runtime services exactly like MINK functions
(`docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md` §ABI):

- arguments are pushed on the stack rightmost-first, one 64-bit word each;
  the callee reads argument `i` at `[rbp + 16 + 8*i]`;
- the result is returned in `rax`;
- the stack stays 16-byte aligned at every call — the **alignment padding
  for an odd number of argument words is pushed before the arguments**, so
  argument 1 is on top of the stack at the call (and thus at `[rbp + 16]`);
- `rbp`/`rsp` are the only callee-saved registers.

Runtime-internal helpers (error reporting, the stdout/stderr write thunks)
use a private register convention; the write thunks call the Win32
`GetStdHandle`/`WriteFile` imports through the image's `.idata` IAT.

## 4. Process lifecycle

The `.text` entry-point stub:

1. captures the process-entry stack pointer into `.bss` (`entry_rsp`);
2. aligns the stack and calls `rt_init`;
3. calls `main`;
4. pushes `main`'s result (with alignment padding) and calls `rt_exit`.

`rt_exit` scans the liveness table — a live allocation is a leak
(`E-R06`) — then restores `entry_rsp` and returns, so the loader turns
`main`'s result into the process exit code.

## 5. Runtime services

| Service            | Signature                         | Notes                                   |
| ------------------ | --------------------------------- | --------------------------------------- |
| `Init`             | `()`                              | reset cursor and free list              |
| `Alloc`            | `rt_alloc(size) -> addr`          | validated bump + LIFO reuse (`E-R02/03`)|
| `Free`             | `rt_free(ptr)`                    | exact live start required (`E-R04/07`)  |
| `MemLoad`          | `rt_mem_load(addr) -> word`       | 8-byte word inside a live block (`E-R05/07`) |
| `MemStore`         | `rt_mem_store(addr, value)`       | same validation                        |
| `StrAlloc`         | `rt_str_alloc(size) -> addr`      | heap blob `8 + size`, negative rejected (`E-R08`) |
| `StrFree`          | `rt_str_free(str)`                | heap blob only (`E-R05` otherwise)     |
| `StrLen`           | `rt_str_len(str) -> len`          | length prefix; validates the pointer   |
| `StrByte`          | `rt_str_byte(str, i) -> byte`     | bounds-checked index (`E-R09`)         |
| `StrSetByte`       | `rt_str_set_byte(str, i, v)`      | bounds-checked index (`E-R09`)         |
| `PrintStr`         | `rt_print_str(str)`               | bytes + CRLF to stdout                 |
| `PrintInt`         | `rt_print_int(value)`             | decimal digits + CRLF to stdout        |
| `Exit`             | `rt_exit(code)`                   | leak check, restore stack, return      |
| `Fail`             | `rt_fail(number)`                 | write diagnostic, exit `100 + number`  |
| `WriteStdout`      | `write(buf, len)`                 | cached handle through `WriteFile`      |
| `WriteStderr`      | `write(buf, len)`                 | cached handle through `WriteFile`      |

The intrinsics are predeclared as `SymbolKind::Intrinsic` module symbols
(`rt_alloc`, `rt_free`, `rt_mem_load`, `rt_mem_store`, `rt_str_alloc`,
`rt_str_free`, `rt_str_len`, `rt_str_byte`, `rt_str_set_byte`,
`rt_print_str`, `rt_exit`, `rt_print_int`), typed by the type checker,
threaded through HIR/MIR as module-item-style statics, and lowered by the
backend to `BInstKind::RuntimeCall` with a stable `RuntimeService` id.

String values are one word: the address of a length-prefixed UTF-8 byte
blob. String literals are immutable blobs in the image's `.text`
string-data region (bounds recorded into `.bss` at `rt_init`);
`rt_str_alloc` creates heap blobs through the validated allocator. The
string intrinsics validate their pointer argument first (`E-R05`) and
bounds-check every index (`E-R09`).

## 6. Runtime diagnostics

Runtime errors terminate with exit code `100 + number` after writing a
structured `mink: runtime error[E-R0N]: <message>` line to stderr:

| Code | Number | Meaning                                        |
| ---- | ------ | ---------------------------------------------- |
| E-R01| 1      | runtime initialization failure                 |
| E-R02| 2      | out of memory (arena exhausted)                |
| E-R03| 3      | liveness table exhausted                       |
| E-R04| 4      | invalid free (null, double, interior, unknown) |
| E-R05| 5      | out-of-bounds or use-after-free access         |
| E-R06| 6      | leak (live allocation at exit)                 |
| E-R07| 7      | misaligned pointer                             |
| E-R08| 8      | invalid allocation size                        |
| E-R09| 9      | string index out of range                      |
| E-R10| 10     | array index out of range                       |

`E-R10` is emitted by the generated code around every array index step
(`IndexLoad`, `IndexStore`, and index steps of `PlaceStore`): the index is
checked against the array's length and a negative index is rejected before
any memory is touched. Like `E-R09`, it terminates with exit code 110.

## 7. Image layout

Every image contains `.text` (stub, user functions, embedded runtime),
`.data` (module bindings, only when present), `.bss` (runtime state: entry
rsp, free list, cursor, cached handles, print buffer, heap arena, liveness
table), `.idata` (kernel32 import directory: `GetStdHandle`, `WriteFile`),
and `.reloc` (no-op base-relocation block for formal relocatability). The
import lookup and address tables are terminated by zero entries so the
loader never scans past them into string data.

## 8. Validation

- `src/runtime/verify.rs` checks the invariant state (table/free-list
  consistency, alignment, live ranges) in pure Rust.
- `src/runtime/*` unit tests cover allocation, lifetime, ABI constants,
  determinism, and adversarial cases.
- `tests/runtime.rs` builds and runs end-to-end MINK programs: heap round
  trips, LIFO reuse, every `E-R0N` path, `rt_print_int` output, and
  determinism of both the image bytes and the runtime behavior.

Full suite after session 16: **878 tests**, all passing (see
`NATIVE_BACKEND_IMPLEMENTATION.md` §13 for the breakdown). After session
17 (data-free enums, a single-word scalar with no runtime support
needed) the suite is **919 tests**.

## 9. Known limitations

- The runtime supports a single-threaded, single-heap model; no GC. Move
  semantics are a compile-time fiction enforced by the ownership stage
  (session 15) before code generation — the runtime itself performs no
  ownership checks (see `docs/implementation/OWNERSHIP_IMPLEMENTATION.md`).
- Strings are byte sequences without runtime UTF-8 validation; literals are
  immutable (no built-in concatenation). Strings and raw `rt_mem_*`
  pointers are distinct types and never mix.
- Struct and array values live entirely in stack slots and argument
  copies; they never touch the heap or the `rt_mem_*` intrinsics (which
  operate on raw 8-byte words at typed `Ptr<Int>` addresses). Aggregate
  size is bounded by `MAX_AGGREGATE_BYTES` (1 MiB) so every value fits
  the arena's addressing model.
- The arena is a fixed 1 MiB; exhaustion is a structured error.
