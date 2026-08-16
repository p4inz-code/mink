# Session 24: Float, Char, and Null as native scalars

**Status:** complete (commit `d8f9d9d` + session 24 changes).

## 1. Scope

The native backend previously rejected `Float`, `Char`, and `Null` with
`E-B02`/`E-B03` even though the front end (lexer → parser → typechecker →
HIR → MIR) fully supported them: literals, typing rules, and constant
representations all existed. This session completes the milestone by
making all three first-class native types:

- **Float** — a 64-bit IEEE-754 double: SSE2 arithmetic (`+ - * / %`),
  six comparisons and equality, unary negation, constants in every
  literal notation (decimal, scientific), function parameters/returns,
  locals, struct fields, array elements, and exact decimal printing with
  17 significant digits (shortest round-trip for `f64`).
- **Char** — a single byte (layout `(1, 1)`), printable via the new
  `rt_print_char` intrinsic, passable through functions.
- **Null** — a word-sized unit-like value (layout `(8, 8)`), usable in
  locals and returns.

`main` may still not *return* any of these (`E-B09`): the entry stub
passes the result in `rax` to the exit service, so only `Int`, `Bool`,
and unit results can become exit codes.

## 2. Intrinsics

`src/runtime/intrinsics.rs` gains two entries with pinned argument types:

- `rt_print_float(Float)` → `PrintFloat`
- `rt_print_char(Char)` → `PrintChar`

Both are registered in the typechecker's intrinsic table
(`src/typecheck/checker.rs`) exactly like `rt_print_int`.

## 3. Backend IR (`src/backend/ir.rs`)

- `BType` gains `Float`, `Char`, `Null`.
- `BInstKind::Binary` gains a `ty: BType` field so the emitter can
  dispatch Float (SSE2) vs integer operations without re-deriving the
  operand type.
- `RuntimeService` gains `PrintFloat` and `PrintChar` variants.

## 4. Lowering (`src/backend/lower.rs`)

- `classify` maps `Float`/`Char`/`Null` to their `BType`s (previously
  `None`, which produced `E-B03`).
- `decode_float`/`decode_char` decode `MirConstantKind::Float`/`Char`
  into their bit patterns (doubles keep their IEEE-754 bits; chars are a
  single byte).
- `value_byte_size` returns 8 for `Float`/`Null`, 1 for `Char`.
- `runtime_service` maps the new intrinsic names to the new service
  variants.
- Statics: `Float`/`Char`/`Null` go through the normal single-word
  constant-image path (data region is normal byte order, unchanged).

## 5. Verification (`src/backend/verify.rs`)

- Binary operand/result rules now include `Float` (arithmetic and
  comparisons), keeping `Char`/`Null` restricted to equality (and
  `Char`'s `==`/`!=`), exactly matching the typechecker's operator
  table.
- Unary negation accepts `Int` or `Float` (float negation targets are
  `BType::Float`).
- `PrintFloat`/`PrintChar` are registered in the runtime-service arity
  and callable tables.

## 6. Emission (`src/backend/emit/x86_64.rs`)

- `emit_float_binary` loads both operands into `xmm0`/`xmm1` (constants
  via `movabs` + `movq xmmN, rax`, locals via `movsd xmmN, [rbp+disp]`),
  applies the SSE2 op, and stores the result to the target slot:
  - `Add`/`Sub`/`Mul`/`Div` → `addsd`/`subsd`/`mulsd`/`divsd`.
  - `Rem` → `x/y`, truncate toward zero by clearing the fractional
    mantissa bits (exponent field masked with `0x7FF`, shift count
    moved into `rcx` for `shl r8, cl`), then `x − y·trunc(x/y)`.
  - `Lt`/`Le`/`Gt`/`Ge`/`Eq`/`Ne` → `ucomisd` + the `setcc`/parity
    combination that matches IEEE-754 ordering (NaN-aware: ordered
    comparisons are false with NaN, `!=` is true).
- Unary negation of a float operand flips the sign bit
  (`movabs rax, 0x8000…`; `xor`).
- **Encoding fixes found while bringing the path up:** `movq xmmN, rax`
  requires `REX.W` (`66 48 0F 6E`) — without it the CPU treats the
  instruction as the 32-bit `movd`, silently truncating every constant;
  the ModRM `reg` field (bits 3–5) holds the xmm destination, not the
  low bits. `movq rax, xmmN` is the mirrored `66 48 0F 7E`. The
  `%` path also had two latent bugs (the exponent mask was `0x7F`
  instead of `0x7FF`, and the shift count never reached `cl`), and the
  division itself was missing before truncation.

## 7. Runtime services (`src/backend/emit/runtime.rs`)

- `rt_print_char`: writes the byte at `[rbp+16]` followed by CRLF,
  mirroring `rt_print_int`.
- `rt_print_float`: a from-scratch machine-code decimal conversion
  (Ryu-style exact algorithm, no libc):
  1. Decompose the bits: sign, exponent field, significand
     (`f = frac | 2^52` for normals, `k = exp − 1075`; subnormals get
     `k = −1074`). Non-finite values print `Inf`/`-Inf`/`NaN`
     (`-NaN` keeps the sign bit).
  2. Build the exact big integer `I = f·5^N` (or `f·2^k` for `k ≥ 0`)
     in `dtoa_words` (40 u64 words) by repeated wide multiplication
     with carry, tracking the word count.
  3. Extract the decimal digits LSD-first into `dtoa_digits` (400
     bytes) by repeated division by 10; compute `D_total` and the
     decimal exponent `E = D_total − 1 − N`.
  4. Round to 17 significant digits with round-half-even (sticky scan
     for any nonzero below the rounding digit, tie broken by the 17th
     digit's parity); a carry past the top bumps the exponent and prints
     `10^E`.
  5. Format: fixed notation for `−4 ≤ E < 17` (integer digits then a
     fraction with trailing zeros trimmed), scientific `d.dddde±XX`
     otherwise. Trailing-zero trimming scans from the low bound upward
     for the lowest nonzero digit (digits are stored least-significant
     first), comparing single bytes.
  6. Write the assembled buffer plus CRLF.
- The BSS layout (`src/runtime/abi.rs`) grows `dtoa_words` (80 bytes)
  and `dtoa_digits` (400 bytes) scratch regions.

## 8. Behavior verified

The printed form matches C's `%.17g` (and Rust's `{:e}` formatting for
large/small magnitudes):

```
1.5                0.1 + 0.2 → 0.30000000000000004
0.5                0.1 · 3   → 0.30000000000000004
2.5                1/3       → 0.33333333333333331
0.25               π         → 3.1415926535897931
100                0.7       → 0.69999999999999996
1e10               e         → 2.7182818284590451
10.0 % 3.0 = 1     -1.5e-300 → -1.5000000000000001e-300
1e-5    → 1.0000000000000001e-5
1e20    → 1e+20
1e308   → 1.7976931348623157e+308
5e-324  → 4.9406564584124654e-324
1e16    → 10000000000000000
Inf     NaN     -Inf     -0
```

Comparisons follow IEEE-754: `0.1 + 0.2 > 0.3` is true,
`0.1 + 0.2 == 0.3` is false.

## 9. Tests

`tests/scalar_types.rs` (27 tests): typechecking of float literals and
operators, int/float mixing rejection, backend lowering of float
binaries and char/null, `main`-return `E-B09`, native end-to-end printing
(basic values, arithmetic, remainder signs, negation/`-0`, scientific
notation, `Inf`/`NaN`, transcendentals, comparisons, function plumbing,
locals and mutation, struct fields and arrays, loops, char printing and
escapes, char through functions, null locals/returns, mixed scalars),
and byte-identical determinism. `tests/backend.rs` converts the stale
"Float/Char/Null are rejected" tests into positive ones (the remaining
rejection is `Range`, and `main` returning a non-exit-code type).

## 10. Test counts

1121 → **1148** (+27 in `tests/scalar_types.rs`; `tests/backend.rs`
net +5 with the converted tests).

## 11. Known limitations (unchanged)

- `main` may not return `Float`/`Char`/`Null` (or any aggregate) — `E-B09`.
- Tagged-union equality (`E-T30`), enum→`Int` conversion, struct/array
  destructuring/ranges/or-patterns, tuples, generics remain future
  milestones.
- `rt_print_float` prints 17 significant digits (exact round-trip), not
  the shortest representation Rust's `{}` uses.
