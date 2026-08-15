# MINK String + Memory Type Implementation

**Status:** Implemented (Session 13)
**Version:** 0.1.0

This document describes the first real memory-backed aggregate foundation
of the MINK language: the `Str` type, string literals as immutable byte
data, typed pointers (`Ptr<Int>` in the current language), and the
intrinsic operations that read and write them. It deliberately does **not**
introduce arbitrary raw-pointer syntax or generics/closures — those are
future sessions. (Structs/arrays arrived in session 14 and compile-time
move semantics for heap-owning values in session 15 — see
`docs/implementation/AGGREGATE_TYPES_IMPLEMENTATION.md` and
`docs/implementation/OWNERSHIP_IMPLEMENTATION.md`.)

## 1. Design boundaries

- **One pointer type.** The runtime memory model has exactly one pointer
  type today: `Ptr<Int>`, a single 64-bit word holding a byte address.
  `TypeKind::Ptr<T>` is generic in the type system, but the only instantiated
  element is `Int` (the word the raw `rt_mem_*` intrinsics read/write).
- **Strings are not pointers.** `Str` is a distinct type: a single word
  holding the address of a **length-prefixed UTF-8 byte blob**. The type
  checker forbids `Str` from flowing into the raw memory intrinsics and
  forbids pointers from flowing into the string intrinsics. No implicit
  conversions exist — the only coercion is the null-pointer constant.
- **Move semantics (session 15).** Explicit allocation and deallocation
  (`rt_alloc`/`rt_free`, `rt_str_alloc`/`rt_str_free`) is the whole
  runtime model; at compile time an Owned `Str` moves on transfer
  (use-after-move is `E-S10`), literals copy freely, and mutating an
  Immutable string is `E-S11` (see `OWNERSHIP_IMPLEMENTATION.md`).
- **No unsafe Rust.** Everything is safe Rust plus emitted machine code.
- **Deterministic.** Identical sources produce byte-identical images and
  identical runtime behavior; every runtime error has a stable code and
  exit status (`E-R09` for string index out of range).

## 2. The `Str` type

A `Str` value is **one word**: the address of a length-prefixed blob.

```
word:    [ length (8 bytes, little-endian) ] [ data byte 0 ] [ data byte 1 ] ...
         ^--- the Str value is the address of this word
```

- The length prefix is the blob's byte length (excluding the prefix).
- Data is raw UTF-8 bytes; the language does not validate UTF-8 well-formedness
  beyond the lexer's escape decoding — strings are byte sequences.
- **String literals** are immutable blobs emitted into the image's `.text`
  string-data region (see §6). The runtime records the region's bounds in
  `.bss` at `rt_init` so the string intrinsics can validate that a literal
  string's address lies inside the immutable region and read its length
  prefix from the image.
- **`rt_str_alloc(n)`** allocates a heap block of `8 + n` bytes (through
  the validated `rt_alloc` path, so all `E-R02/03/08` rules apply) and
  writes the length prefix `n`. Fresh bumps are zero-filled; free-list
  reuse retains old contents, exactly like word allocations.

## 3. Typed pointers

- `rt_alloc(n: Int) -> Ptr<Int>` returns a typed pointer.
- `rt_free(p: Ptr<Int>)`, `rt_mem_load(p: Ptr<Int>) -> Int`, and
  `rt_mem_store(p: Ptr<Int>, v: Int)` consume typed pointers. The runtime
  still validates alignment and bounds at every access (`E-R07`, `E-R05`).
- **Byte-addressed arithmetic:** `p + n`, `n + p`, `p - n` (with `n: Int`)
  advance the address by `n` bytes and produce `Ptr<Int>`. `p + q`,
  `p - q`, and `p + true` are invalid (`E-T02`). This is the session's
  minimal addition — pointer subtraction is deliberately not specified.
- **Equality:** `p == q` and `p != q` compare two pointers and produce
  `Bool`. Pointer/`Int` equality is rejected.
- **Null pointer constant:** the integer literal `0`, and only the literal
  `0`, is accepted in pointer-typed **argument positions** (intrinsic and
  user-function calls). A computed `Int` value is never a pointer, and
  `let p = 0` still types `p` as `Int`. The type checker plumbs the source
  map through so it can tell the literal `0` from any other expression.
- Strings never satisfy pointer positions and pointers never satisfy
  string positions; both are `E-T01` type mismatches.

## 4. Type system changes

- `TypeKind::Ptr(TypeId)` — canonical, interned, structurally unified
  (`Ptr<Int>` unifies with `Ptr<Int>`; display is `Ptr<Int>`).
- `TypeKind::Unit` — the result type of `rt_free`, `rt_str_*` mutators,
  `rt_print_*`, and `rt_exit`; the type checker already had a `Unit`
  notion for functions without results, now shared with intrinsic results.
- `IntrinsicType` gained `Ptr` and `Str` variants used in the intrinsic
  signature table (`src/runtime/intrinsics.rs`). The order of the
  intrinsic table is part of the runtime ABI and was extended in place:
  `rt_alloc`, `rt_free`, `rt_mem_load`, `rt_mem_store` (raw memory),
  `rt_str_alloc`, `rt_str_free`, `rt_str_len`, `rt_str_byte`,
  `rt_str_set_byte`, `rt_print_str` (strings), then the existing
  `rt_exit`, `rt_print_int`.

## 5. Intrinsic signatures

| Intrinsic        | Parameters               | Result   | Notes                              |
| ---------------- | ------------------------ | -------- | ---------------------------------- |
| `rt_alloc`       | `size: Int`              | `Ptr<Int>`| validated bump + LIFO reuse        |
| `rt_free`        | `p: Ptr<Int>`            | `Unit`   | exact live start (`E-R04/07`)      |
| `rt_mem_load`    | `p: Ptr<Int>`            | `Int`    | 8-byte word, validated (`E-R05/07`)|
| `rt_mem_store`   | `p: Ptr<Int>, v: Int`    | `Unit`   | same validation                    |
| `rt_str_alloc`   | `n: Int`                 | `Str`    | heap blob `8 + n`, zero-filled     |
| `rt_str_free`    | `s: Str`                 | `Unit`   | heap blob only (`E-R05` otherwise) |
| `rt_str_len`     | `s: Str`                 | `Int`    | length prefix                       |
| `rt_str_byte`    | `s: Str, i: Int`         | `Int`    | `i` in range (`E-R09`)             |
| `rt_str_set_byte`| `s: Str, i: Int, v: Int` | `Unit`   | `i` in range (`E-R09`)             |
| `rt_print_str`   | `s: Str`                 | `Unit`   | bytes + CRLF to stdout             |
| `rt_exit`        | `code: Int`              | `Unit`   | leak check, restore stack          |
| `rt_print_int`   | `v: Int`                 | `Unit`   | decimal digits + CRLF              |

`rt_str_len`, `rt_str_byte`, and `rt_str_set_byte` validate the string
pointer first: heap blobs must be the exact start of a live block
(`E-R05`); literal addresses must lie inside the recorded immutable
string-data region. A bad index is the new `E-R09`:

| Code  | Number | Meaning                                |
| ----- | ------ | -------------------------------------- |
| E-R09 | 9      | string index out of range              |

## 6. Backend and image changes

- `BType::Ptr` and `BType::Str` classify the two new word-sized values.
- `BProgram` gained a `strings: Vec<BString>` table; `BString` holds the
  decoded bytes and the exact source span of the literal.
- String literals lower to `BInstKind::LoadStr { target, string_index }`,
  which loads the blob's image address into a `Str` local.
- The emitter emits each blob in `.text` as a length prefix followed by
  the raw bytes, and records `str_data_start`/`str_data_end` label bounds.
  `rt_init` stores these absolute addresses into `.bss` (the string-data
  bounds fields added to the runtime ABI in `src/runtime/abi.rs`), so the
  string intrinsics can validate literal strings at runtime.
- The five string services (`StrAlloc`, `StrFree`, `StrLen`, `StrByte`,
  `StrSetByte`, `PrintStr`) are new embedded runtime routines following the
  existing register/service conventions. `StrAlloc` rejects sizes with the
  sign bit set (`E-R08`), matching the reference allocator's guard.
- The entry-point stub's result handling is unchanged; `main`'s result
  still becomes the process exit code.

## 7. Reference implementation

`src/runtime/allocator.rs` is the executable specification:

- `alloc_string(size)` — mirrors the machine `StrAlloc`, including the
  negative-size guard (`E-R08`), zero-fill on fresh bumps, and free-list
  reuse semantics.
- `string_len(s)` / `string_byte(s, i)` / `set_string_byte(s, i, v)` —
  mirror the machine routines; bad pointers are `E-R05`, bad indexes are
  `E-R09`. Block kinds are deliberately not tracked: the type checker is
  what keeps strings and word blocks apart, so a word block read as a
  string just reads its (zero-filled) first word as a length.
- The reference does not contain the image's literal string-data region
  (literals are not addresses in the pure-Rust model); the machine runtime
  additionally accepts literal addresses, documented at
  `docs/implementation/RUNTIME_IMPLEMENTATION.md`.

## 8. What is deliberately absent

- No explicit borrow syntax, lifetimes, or GC. Since session 15,
  compile-time move semantics apply to heap-owning values: an Owned `Str`
  moves on transfer (use-after-move is `E-S10`), literals copy freely,
  and mutating an Immutable string is `E-S11` — see
  `docs/implementation/OWNERSHIP_IMPLEMENTATION.md`.
- No arbitrary raw-pointer syntax (`&`, casts, address-of).
- No generics or closures. (Structs and arrays arrived in session 14 — see
  `docs/implementation/AGGREGATE_TYPES_IMPLEMENTATION.md`.)
- No string concatenation, slicing, or mutation of literals.
- No pointer subtraction (`p - p`), pointer/pointer arithmetic, or
  pointer/`Int` equality.

### 8.1 Pointer/aggregate interaction (session 14)

- Struct and array values are stored entirely in stack slots and argument
  copies; they never flow through the `rt_mem_*` intrinsics, which
  operate on raw 8-byte words at typed `Ptr<Int>` addresses. A struct or
  array value is never a `Ptr<Int>` and vice versa (`E-T01`).
- A struct **field** may be of type `Str` or `Ptr<Int>`: the field holds
  the single-word value exactly as a local would, and reading the field
  yields a value that can be passed to the string/pointer intrinsics or
  compared/arithmetic'd exactly like a local of the same type. Strings and
  pointers therefore compose with aggregates by value, with no special
  rules.
- Aggregate values are bounded by `MAX_AGGREGATE_BYTES` (1 MiB), so they
  always fit the arena's addressing model even though they never touch
  the heap.

## 9. Validation

- Unit tests in `src/runtime/allocator.rs` cover the length prefix,
  byte round trips, `E-R09` bounds, `E-R05` on freed/foreign blocks,
  the negative-size `E-R08` guard, and deterministic allocation.
- `tests/typecheck.rs` covers `Str`/`Ptr<Int>` typing, pointer arithmetic
  and equality, the null-pointer-`0` rule, unification across calls, and
  every rejection path.
- `tests/backend.rs` covers `LoadStr` lowering, exact-span preservation,
  escape/UTF-8/hex decoding, and `Ptr` local typing.
- `tests/runtime.rs` builds and runs end-to-end native binaries: literal
  printing (including escapes and UTF-8), `rt_str_alloc` round trips,
  zero-fill, and every string error path.

Full suite after session 16: **878 tests**, all passing (see
`NATIVE_BACKEND_IMPLEMENTATION.md` §13 for the breakdown).

## 10. Known limitations

- Strings are immutable values today (no built-in concatenation); the only
  way to build a string is `rt_str_alloc` + `rt_str_set_byte`.
- Only `Ptr<Int>` is instantiated; `TypeKind::Ptr<T>` exists for future
  sessions but no other element type can be produced by any intrinsic yet.
- UTF-8 well-formedness is not validated at runtime; strings are bytes.
- The arena is still a fixed 1 MiB; exhaustion is a structured error.
