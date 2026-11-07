# Session 200 — Compiler Correctness and Diagnostics Audit

Windows x86_64, 2026-11-07. Starting HEAD
`d3c91f183d596cd5652de3ea5d85a78b908f5f07` (Session 199, local == origin/main
verified before any change).

## 1. Scope established before auditing

The front-end-to-back-end compiler pipeline: lexer, parser (including hard
cases), source spans/diagnostics, HIR, type checker, MIR, optimizer, and the
PE backend. Baseline evidence is the 482 tests in the eleven compiler-focused
suites, all green before any change:

| Suite | Tests |
| --- | --- |
| `lexer` | 50 |
| `parser` | 116 |
| `parser_hardening` | 64 |
| `source` | 12 |
| `hir` | 25 |
| `mir` | 36 |
| `optimization` | 38 |
| `semantics` | 75 |
| `backend` | 46 |
| `modules_check` | 7 |
| `smoke` | 13 |
| **Total** | **482** |

## 2. Method

Existing suites prove many individual rules but do not pin the *observable
contract* of a rejected program. The audit added `tests/diagnostics.rs`
(24 tests), which compiles each malformed program with the real `mink build`
CLI and asserts, for every case:

1. exit code **1** — a compile error, never a crash (`0xC0000005`) and never a
   silent success,
2. the expected `E-…` code,
3. a `file:line:` location,
4. no `panicked` / `internal compiler error` / `RUST_BACKTRACE` marker,
5. **byte-identical stderr on a second compile** (diagnostics are stable), and
6. empty stdout.

Coverage: 13 front-end/semantic codes (`E-P03`, `E-L02`, `E-L04`, `E-L05`,
`E-T01` ×2, `E-T04`, `E-T05`, `E-S01` ×2, `E-S02`, `E-S03`, `E-S05`), five
integer-range cases, two message-rendering guards, and three controls (valid
program, `i64::MAX`, `i64::MIN`).

Writing the battery immediately exposed two defects.

## 3. Defect 1 — out-of-range integer literals silently wrapped

### 3.1 Evidence (before the fix)

```
let x: Int = 9223372036854775808;     // prints -9223372036854775808, exit 0
let x: Int = 999999999999999999999999; // prints 2003764205206896639, exit 0
let x: Int = 0x1_0000_0000_0000_0000; // compiled, exit 0
match x { 9223372036854775808 => … }  // compiled; pairs i64::MIN
```

A literal the user can see in the source became a different number at run
time. That is a silent miscompile, not a language rule.

### 3.2 Root cause

The AST stores only the literal's **span**, so the value is decoded later from
source text in two places — `decode_int_literal` (type checker) and
`decode_int` (backend) — and both used `wrapping_mul`/`wrapping_add` followed
by `value as i64`, with no range check. Nothing in the pipeline ever asked
whether the literal fit in `Int`.

### 3.3 Fix (minimal, root-cause)

`Checker::check_int_literal` now decodes the text with **checked** `u64`
arithmetic and rejects any value above `i64::MAX` with the new diagnostic
**`E-T43`** (`TypeErrorKind::IntegerLiteralOverflow`, message
``integer literal `…` does not fit in `Int` (64-bit two's complement)``).

Two boundaries are preserved deliberately:

* `-9223372036854775808` is `Int`'s minimum and has no positive spelling, so
  exactly `2^63` is allowed **only** as the digits of a negated literal. The
  expression arm recognizes `Unary(Neg)` over an `Int` literal and types the
  operand without re-validating it; writing `-(9223372036854775808)` (a
  parenthesized expression) is rejected, as in Rust.
* Match patterns are validated recursively by `validate_pattern_literals`
  (plain literals, both range endpoints, or-alternatives, enum payloads,
  tuple elements and struct fields), so `9223372036854775808 =>` is `E-T43`
  while `-9223372036854775808 =>` still compiles and matches.

The backend decoder is intentionally unchanged: after this check, wrapping is
unreachable for accepted programs, and the backend is not a public entry
point. Adding a second range check there would duplicate the rule.

## 4. Defect 2 — aggregate diagnostics had a mangled prefix

### 4.1 Evidence (before the fix)

```
mink: error[E-T11]: `` is not a valid key type: array index `5` is out of bounds for an array of length 2
```

`E-T11` is an array-index error; the "not a valid key type" prefix belongs to
`E-T42` (invalid `Map`/`Set` key) only.

### 4.2 Root cause

`Display for TypeError` grouped every aggregate kind (`E-T07`…`E-T41`) with
`InvalidCollectionKey` in one match arm and formatted that arm with the
key-specific string, so all of those diagnostics gained the prefix and two
empty backticks.

### 4.3 Fix

Aggregate kinds now print their stored message unchanged; `InvalidCollectionKey`
keeps the key-specific format as its own arm. `d20` pins the exact `E-T11`
text and asserts the prefix is absent; `d21` guards against empty-backtick key
rendering.

## 5. Verification

* `tests/diagnostics.rs`: **24 passed / 0 failed** in debug and **24 passed /
  0 failed** in release.
* `i64::MIN` and `i64::MAX` round-trip through compile *and* execution with
  exact printed values; the `i64::MIN` pattern matches.
* Full regression after the shared type-checker change: **3,217 passed / 0
  failed / 7 ignored across 78 suites** (every target except `tls_lib`;
  `python3` on the audit machine is the Microsoft Store alias stub, so the TLS
  test server cannot start — reported as not run, not as a defect).
* Release build succeeds; `cargo build --release` reports 4,041,216 bytes for
  the CLI binary.

### 5.1 Environment note (not a source defect)

The first release rebuild of the session aborted with
`STATUS_ILLEGAL_INSTRUCTION (0xc000001d)` inside `rustc` while producing the
optimized library. Re-running the same build with `CARGO_INCREMENTAL=0`
succeeds repeatedly. The crash was in the incremental-compilation cache path
(the target directory had been written concurrently by two cargo processes),
not in MINK source; debug and release builds are both clean with incremental
compilation disabled.

## 6. Residual risks (recorded, not changed)

* Literal range errors are reported at the literal token; for a negated
  pattern the span covers only the digits, not the leading `-`.
* The backend's own literal decoder still wraps by design; any future entry
  point that bypasses type checking must apply the same rule.
* `tests/diagnostics.rs` pins the current wording of 24 diagnostics. Future
  wording changes are intentional breaking changes to that contract.

## 7. Reproduce

```
cargo test --test diagnostics
CARGO_INCREMENTAL=0 cargo test --release --test diagnostics
```

## 8. Verdict

**PASS — two confirmed compiler-correctness defects found and fixed**, each
with a permanent regression test in a new 24-case diagnostic battery; the
full regression, debug and release checks are green.
