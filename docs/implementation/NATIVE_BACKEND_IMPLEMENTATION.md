# MINK — Native Backend Implementation

**Status:** Implementation
**Version:** 0.1.0
**Sessions:** 11 — Native Backend / Code Generation Foundation, 13 —
String + Memory Types (strings and typed pointers)

## 1. Purpose

The native backend is the code-generation stage of the compiler: it consumes
the **optimized** MIR produced by session 10 and produces an executable
machine image, closing the pipeline:

    Source → Lexer → Parser → AST → Semantic Analysis → Type Analysis
        → HIR → MIR → Optimization → Native Backend → Executable

The first milestone deliberately implements a **small, genuinely working
native subset** rather than broad fake support: a self-contained x86-64
code generator that assembles a complete Windows PE executable with **no
external toolchain** (no C compiler, assembler, or linker). The success
criterion for the milestone is

> MINK source → … → Optimization → Native Backend → executable →
> successful execution.

which is validated by an end-to-end test that builds a program and runs the
generated binary.

Like every other stage, the backend is:

- **target-independent up to the emission boundary** — MIR is lowered into a
  portable instruction representation that any target consumes, and each
  `Target` supplies an emitter;
- **deterministic** — items, functions, blocks, statements, and errors are
  produced in source order, so identical input always yields an identical
  image (verified by `emission_is_deterministic`);
- **never silently wrong** — every construct outside the native subset is
  rejected with a structured `E-B01+` diagnostic instead of being
  miscompiled, and every independent problem is reported in source order;
- **defensive** — a verifier checks the lowered program's structural
  integrity before emission, so malformed hand-built or mutated
  instructions fail cleanly (`E-B07`) instead of panicking;
- **safe Rust** — no `unsafe` anywhere in the backend.

## 2. Module Layout

    src/backend/
        mod.rs     Public API: lower, verify, compile, Target, errors
        error.rs   Backend error model (E-B01 … E-B12)
        target.rs  Target selection and validation
        ir.rs      Portable backend instruction representation (B* types)
        lower.rs   Optimized MIR → backend instructions
        verify.rs  Structural verifier for lowered programs
        emit/
            mod.rs    Emitter dispatch and EmittedImage
            x86_64.rs x86-64 code generator (the first native target)
            runtime.rs Machine-code runtime: init, heap, intrinsic services
            pe.rs     PE container builder

    src/runtime/
        mod.rs     Runtime crate root and diagnostics overview
        abi.rs     Fixed calling convention and heap/table ABI between
                   generated code and the machine-code runtime
        error.rs   Structured runtime errors (E-R01 … E-R06)
        layout.rs  Explicit memory-layout model for future data types
        allocator.rs Reference heap allocator (bump + free list) used as the
                   authoritative spec for the machine-code runtime and by
                   `tests/runtime.rs`
        verify.rs  Reference invariant checker (allocation, lifetime, ABI)
        intrinsics.rs  Intrinsic symbol table shared with the front end

The public entry point is `backend::compile(program, sources, target)`,
which finds and validates the entry function, lowers, verifies, and emits:

    optimized MIR → lowering → backend instructions → verification
        → emission → machine image

## 3. The Target Boundary

`Target` (`src/backend/target.rs`) is an instruction-set/OS/format triple:

- `x86_64-windows-pe` — **implemented**: 64-bit x86-64 Windows PE image.
  Chosen first because the build environment is x86-64 Windows and the
  target needs no external toolchain.
- `x86_64-linux-elf` — recognized, not implemented (`E-B11`).
- `aarch64-linux-elf` — recognized, not implemented (`E-B11`).

`--target <name>` selects explicitly; unrecognized names are `E-B12`.
`Target::native()` currently selects `x86_64-windows-pe` on every host (the
first milestone implements one target).

## 4. The Backend Instruction Representation

`ir.rs` defines the portable program the emitters consume:

- `BProgram { functions, statics }` — functions in source order, module
  bindings in source order;
- `BFunction { name, symbol, params, locals, blocks, result, span }` —
  params and locals are typed `BLocal`s (name, symbol, `BType`, mutability,
  span); temporaries created during lowering are unnamed locals;
- `BType` — `Int` (64-bit), `Bool` (`0`/`1`), `Range` (a two-word value:
  the normalized exclusive end and the iteration cursor), `Ptr` (a typed
  pointer, one word), `Str` (a string, one word: the address of a
  length-prefixed UTF-8 blob), `Enum` (an enum value, one word holding the
  variant's discriminant), and `Unit` (a function that produces no
  value);
- `BInst` — instructions with exact source spans:
  `LoadConst`, `LoadLocal`, `StoreLocal`, `LoadStatic`, `StoreStatic`,
  `LoadStr`, `Binary`, `Unary`, `Call`, `RangeInit`, `RangeNext`,
  `RangeFinished`;
- `BProgram` additionally carries `strings: Vec<BString>` — decoded literal
  blobs (bytes + exact source span) emitted into the image's immutable
  string-data region;
- `BTerminator` — `Return` (with optional value), `Jump`, `Branch`
  (condition + then/else blocks);
- `BStatic` — module bindings with decoded constant values
  (`mutable`, `ty`, `value`, `span`).

## 5. Lowering (optimized MIR → instructions)

`lower.rs` walks the optimized `MirProgram` once. It never re-runs name
resolution, type checking, or MIR analysis — it only consumes the answers
MIR already carries (classified types, locals, operands, control flow).

**Supported subset.** Types `Int`, `Bool`, `Range<Int>`, `Ptr<Int>`, `Str`,
user-declared `Struct`s, and `Array`s; integer and boolean literals
**decoded from the source text** (the backend is the first stage to decode
literal values: decimal, `0x` hex, and `_` separators, plus `true`/`false`);
string literals decoded into byte blobs (escape, UTF-8, and `\xNN` hex
escapes resolved from the source text, with the exact literal span
preserved on `BString`); local loads and stores; module-binding loads and
stores; byte-addressed pointer arithmetic (`p + n`, `n + p`, `p - n`);
arithmetic (`+ - * / %`), shifts (`<< >>`), bitwise (`& ^ | ~`),
comparisons and equality (`< <= > >= == !=`), logical (`&& || !`),
negation (`-`), range construction and iteration (`for` loops via
`RangeInit`/`RangeNext`/`RangeFinished`), direct function calls, and
`if`/`else`, `while`, `for`, `loop`, `break`, `continue`, `return`;
struct/array literals (field/element stores into a materialized temp),
member/index reads and writes (`FieldLoad`/`FieldStore`,
`IndexLoad`/`IndexStore`), and deep place stores (`PlaceStore`) with
runtime bounds checks (`E-R10`) on every index step.

Aggregate layout is resolved during lowering through the same
`struct_layout`/`array_layout` engine the type checker used, so backend
and typechecker cannot disagree about offsets. Aggregate values in stack
slots follow the downward value-image convention (byte `b` of a value
lives at `slot_word0 - b`); copies and argument marshalling are word-wise
with byte-wise unaligned tails. Aggregate **returns** and aggregate
**module statics** are rejected (`E-B03`); aggregate **arguments** are
supported.

**Rejected constructs** (structured errors, never miscompilation):

| Construct | Code |
|---|---|
| Function values | `E-B01` |
| Float / char / `null` literals | `E-B02` |
| `Float`, `Char`, `Null`, unresolved inference, `Range` in a single-word position (function result, static, operand), aggregate returns / aggregate statics | `E-B03` |
| Unsupported assignment targets | `E-B04` |
| Module bindings initialized by non-constant expressions | `E-B05` |
| Calls whose callee is not a module-level function | `E-B06` |

Lowering is **defensive**: statements that touch a value whose type was
already rejected are skipped (their root cause was already reported), so
one problem never cascades into a swarm of diagnostics. Every independent
problem is reported in deterministic source order, and any error keeps the
program from reaching an emitter.

**Decoding.** Literal values are decoded from the source text through the
source map at the literal's span (`decode_constant`). A missing source
text is a `DecodeError` (`E-B10`), only reachable on malformed hand-built
MIR or a missing source map.

## 6. Verification

`verify.rs` checks the lowered program's structural integrity before
emission: every local reference resolves, every block reference is in
range, blocks are ordered (`block[i].id == i`), every block has exactly one
terminator, terminators reference valid blocks, and operand types are
consistent with their instructions. Violations are `E-B07`, reported as
structured errors rather than panics. Lowering always produces valid
programs; the verifier defends the pipeline (and downstream tooling)
against malformed hand-built or mutated instructions.

## 7. Emission: x86-64 Windows PE

`emit/` dispatches on the selected `Target`. For `x86_64-windows-pe`:

- **`x86_64.rs`** is a register-based code generator. Each function's
  locals live on the stack in a fixed frame; instruction selection is
  direct (one backend instruction → one small sequence). Booleans are
  `0`/`1`; comparisons materialize a `0`/`1` via `setcc`; shifts are
  arithmetic; `&&`/`||` are already lowered to branches in MIR, so the
  emitter only sees comparisons, arithmetic, and jumps. Calls follow the
  Windows x64 calling convention (register args, stack return slots),
  with the callee's result moved into the caller's target slot.
  `LoadStr` emits the length-prefixed blob's image address into the
  target slot; the blob's data bytes live in `.text` between the
  `str_data_start`/`str_data_end` label bounds recorded into `.bss` by
  `rt_init`.
- **`pe.rs`** assembles the sections (`.text` containing user code, the
  embedded runtime, and the immutable string-data region; `.data` when
  there are module bindings; `.reloc`) into a complete PE image with
  correct headers, section table, RVAs, and file padding. Entry point RVA
  is `0x1000` (the start of `.text`).
- The `main` function's integer/boolean result becomes the process exit
  code; a unit `main` exits `0`.

**Determinism.** Emission walks functions, blocks, and instructions in
their stable source order and produces byte-identical images for identical
input.

## 8. The Entry Point

`compile` validates the entry function before emitting:

- no module-level `fn main()` → `E-B08` (`NoEntryPoint`);
- `main` takes parameters, or its result is a `Range` → `E-B09`
  (`InvalidEntryPoint`; the entry result must be an integer, a boolean, or
  nothing).

## 9. Driver and CLI Integration

`driver::build` runs the full pipeline (`check` through optimization, then
`backend::compile`) and writes the image to `<stem>.exe` next to the source
(or `--output`). `mink check` is unchanged: it still stops after
optimization. The CLI:

- `mink build <path> [--target <name>]` — exit 0 with a success message
  (`mink: build: '<in>' -> '<out>' (target: <name>, N function(s), M
  binding(s))`); exit 1 for front-end errors, backend errors (printed with
  their `E-B01+` codes and spans), an unrecognized target name, or
  output-write failures.

## 10. Error Codes

`E-B01` UnsupportedRvalue · `E-B02` UnsupportedConstant · `E-B03`
UnsupportedType · `E-B04` UnsupportedPlace · `E-B05` UnsupportedStatic ·
`E-B06` UnsupportedCallee · `E-B07` InvalidBackendIr · `E-B08`
NoEntryPoint · `E-B09` InvalidEntryPoint · `E-B10` DecodeError · `E-B11`
UnsupportedTarget · `E-B12` InvalidTarget.

Every error carries its stable code, a human-readable message, and the
exact source span of the rejected construct. The codes continue the
established stable ranges (`E-L*` lexical, `E-P*` syntax, `E-S*` semantic,
`E-T*` type, `E-H*` HIR, `E-M*` MIR).

## 11. Runtime ABI

The backend links every generated image against a small machine-code
runtime (`src/backend/emit/runtime.rs`) whose behavior is specified by the
safe-Rust reference implementation (`src/runtime/`) and documented in
docs/implementation/RUNTIME_IMPLEMENTATION.md. The ABI is fixed and
documented in `src/runtime/abi.rs`:

- **Entry stub** — the PE entry point saves the loader stack, calls
  `rt_init`, calls `main`, pushes `main`'s result, calls `rt_exit`, and
  returns to the loader; the loader-visible exit code is `main`'s result.
- **Calling convention** — stack-based: arguments are pushed right-to-left
  with 16-byte alignment (padding first when the argument count is odd, so
  argument 1 always sits at `[rbp+16]`), the callee saves `rbp`/`rsp`, and
  the return value is left in `rax`. Results of `unit`-typed calls are
  ignored.
- **Heap services** — `rt_alloc(size) → ptr` returns a zero-initialized
  block from the bump arena (or the free list) and traps with `E-R02` on
  exhaustion; `rt_free(ptr)` returns the block to the free list and traps
  with `E-R03`/`E-R04` on invalid pointers; `rt_mem_load`/`rt_mem_store` trap with
  `E-R05` when accessing a freed or never-allocated block. All memory is
  zero-initialized, so behavior is deterministic and leak-safe for every
  supported program.
- **String services** — `rt_str_alloc(size)` allocates a heap blob of
  `8 + size` bytes (negative sizes trap `E-R08`) with a length prefix;
  `rt_str_len`/`rt_str_byte`/`rt_str_set_byte` validate the pointer
  (`E-R05`) and bounds-check every index (`E-R09`); `rt_str_free` returns
  heap blobs to the free list; `rt_print_str` writes the blob's bytes plus
  CRLF to stdout. Literal strings are validated against the immutable
  string-data bounds recorded by `rt_init`.
- **Services** — `rt_print_int` writes an integer plus newline to stdout;
  `rt_exit(code)` traps with `E-R06` when a live block is leaked.
- **Runtime table** — a `.bss` region holds the entry stack slot, heap
  cursor/free-list head, arena base, and print buffer, addressed by
  RIP-relative loads/stores resolved at emission time.

## 12. Known Limitations

- Only the first native target (`x86_64-windows-pe`) is implemented;
  `x86_64-linux-elf` and `aarch64-linux-elf` are recognized but rejected
  (`E-B11`).
- No floating point, characters, `null`, or function values — all
  rejected with structured errors. Strings are byte sequences (no runtime
  UTF-8 validation, no concatenation, literals immutable);
  `TypeKind::Ptr<T>` exists in the type system but only `Ptr<Int>` is
  instantiable today. Structs and arrays are supported as values (member/
  index access, place mutation, arguments), but aggregate returns and
  aggregate module statics are rejected (`E-B03`).
- No debug info, no symbol tables beyond what the PE format requires, and
  no optimizations in the backend itself (the MIR pipeline already
  optimized the input).
- Source mapping is preserved on every instruction for future diagnostics
  and debug info, but no consumer exists yet.
- The image targets Windows only; `Target::native()` is the first target on
  every host until more targets land.

## 13. Quality Gates

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo build
    git diff --check

Full suite after session 17: **919 tests** (61 CLI + 50 lexer + 98 parser +
62 parser hardening + 75 semantics + 12 source + 163 typecheck + 25 HIR +
34 MIR + 38 optimization + 55 lib unit + 46 backend + 24 runtime
end-to-end + 59 aggregate + 41 ownership + 51 references + 25 enums),
all passing. After session 18 (pattern matching) the suite is **963 tests**
(+44 in `tests/pattern_matching.rs`; see
`PATTERN_MATCHING_IMPLEMENTATION.md` §6).
The backend tests
(`tests/backend.rs`) cover program structure and determinism,
functions/locals/instructions, constant decoding, string decoding
(escapes, UTF-8, hex), `LoadStr` lowering with exact spans, pointer locals
and arithmetic, arithmetic, comparisons and logical operators, calls,
module bindings, range iteration, every rejected construct with its code
and span, multi-error reporting, verifier checks on malformed
instructions, PE image structure, emission determinism, and the CLI
end-to-end build (build + run the generated binary). The runtime tests
(`tests/runtime.rs`) build and run native binaries that allocate, store,
load, and free heap blocks and string blobs, print integers and strings,
trap with the documented `E-R01+` exit codes on invalid memory operations,
and leak-check on exit. The aggregate tests (`tests/aggregate.rs`) cover
struct/array parsing, typing, layout determinism, and native execution
(see `docs/implementation/AGGREGATE_TYPES_IMPLEMENTATION.md` §8).
