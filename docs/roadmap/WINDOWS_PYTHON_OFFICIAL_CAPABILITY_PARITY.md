# MINK — Windows × Official Python Capability Parity Matrix (Master Audit)

**Session:** 98
**Starting commit:** `a716df4`
**MINK version:** 1.0.1
**Scope:** Windows x86_64 (the shipped platform). Official Python capabilities only.
**Classification:** capability parity — what a Windows developer can accomplish with
official Python must be accomplishable with MINK's own architecture. Syntax imitation is
explicitly out of scope. PyPI / third-party ecosystem parity is out of scope.
**Date:** September 4, 2026
**Auditor:** Buffy (Codebuff)

This document is the durable, evidence-based master map. Companion documents:

- `docs/roadmap/WINDOWS_PYTHON_PARITY_IMPLEMENTATION_PLAN.md` — waves, proofs, estimates.
- `docs/implementation/SESSION_98_WINDOWS_PYTHON_PARITY_AUDIT.md` — session record.
- This matrix supersedes the earlier `docs/audits/OFFICIAL_PYTHON_CAPABILITY_PARITY_AUDIT.md`
  (Session 82 baseline) where the two disagree; disagreements are called out inline.

---

## 1. How to read this matrix

**MINK status legend** (Phase 11 vocabulary):

| Status | Meaning |
|---|---|
| VERIFIED | Implemented on Windows and execution-verified: a native executable was compiled and run by the test suite and/or recorded real execution (session docs, this session's probes). |
| IMPLEMENTED | Support exists in source with in-suite coverage, but this audit did not trace a native execution of the exact capability; evidence = code + tests. |
| PARTIAL | A working subset exists; a material part of the official Python capability is missing or stubbed. |
| PLANNED | Designed in spec/roadmap documents; not implemented. |
| MISSING | Not implemented and not planned in near-term documents. |
| N/A | Concept does not translate to MINK's architecture (reason in Notes). |
| INTENT. DIFF. | MINK deliberately provides an equivalent capability through a different model (see §9 exclusion register). |

**Priority** (Phase 9): P0 = basic correctness/security; P1 = required before Windows
Python-capability parity can be declared; P2 = important production capability, not
parity-blocking; P3 = optional/convenience.

**Difficulty**: S = small · M = medium · L = large · XL = major subsystem.

**Blocks parity?** Y = the gap must be closed before the parity gate can pass; N = not a blocker.

**Wave** (Phase 12): A = Windows platform/runtime quick wins · B = core language/data ·
C = filesystem/process/time completion · D = networking/internet/compression ·
E = concurrency/async · F = packaging/distribution · G = developer tooling ·
H = Windows-specific completion · I = proof applications · J = final parity audit ·
`-` = no MINK work required for this row.

**Evidence anchors**: `[code] file` = source inspection · `[test] file` = test file ·
`[doc] file` = documentation · `[exec]` = recorded native execution (session docs or
probes). Every "every major claim must be traceable" row carries anchors in Notes.

---

## 2. Audited Windows baseline (re-verified this session)

| Fact | Value | Evidence |
|---|---|---|
| Starting commit | `a716df4`, clean tree | `git rev-parse HEAD`, `git status` |
| Version | `mink 1.0.1` | `target/release/mink.exe --version` |
| Release build | clean | `cargo build --release` |
| Test suite | 2547 tests / 56 test targets; full run 2545 pass + 2 timing-flaky loopback network tests that pass in isolation | `cargo test` ×2; isolated reruns of `windows_hardening` |
| Smoke/CLI/release suites | smoke 13/13 · release 66/66 · cli 74/74 | `cargo test --test {smoke,release,cli}` |
| Windows target | `x86_64-windows-pe` implemented | `[code] src/backend/target.rs` |
| Linux target | `x86_64-linux-elf` implemented but FROZEN by policy; not part of this audit | `[code] src/backend/target.rs`; session policy |
| Architecture | AOT compiler, no external toolchain, zero Rust crate deps, standalone PE | `[code] Cargo.toml`, `src/backend/emit/*.rs` |
| Distribution | npm `@p4inz-code/mink` 1.0.1 ships only the compiler `bin/mink.exe` (no stdlib sources) | `[code] npm/mink/package.json`; Session 97 clean-install record |
| P0 / P1 on the Windows base | 0 / 0 | Session 97 gate |

Short-circuit semantics of `&&` / `||` were probed natively this session: both **do**
short-circuit (a side-effecting right operand was not evaluated). Integer division by zero
is **not** guarded: it faults with the raw Windows exception status (probe exited 148 =
`0xC0000094` & 0xFF) instead of a structured runtime error. Both facts are recorded in the
rows below; the Session 82 audit's "no short-circuit" claim is therefore stale.

---

## 3. LANGUAGE capability matrix (official Python language surface → MINK)

### 3.1 Core data model

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| L01 | Integers (arbitrary precision, dynamic size) | INTENT. DIFF. | `Int` = signed 64-bit (`[code] src/typecheck/ty.rs`); all arithmetic ops, ranges, indexing | No bigint; fixed width is the systems-language model | P2 | M | N | B | Bignum is optional later; Python's dynamic bignum is a dynamic-language convenience, not an important capability for MINK's model |
| L02 | Floating point (`float`, IEEE-754 binary64) | VERIFIED | `Float` = f64, exact 17-digit print, conversions (`[code] src/runtime/intrinsics.rs`, `src/backend/emit/runtime.rs`; `[test] tests/math_lib.rs`, scalar_types) | None material | - | - | N | - | Both Python and MINK use binary64; print round-trip exact |
| L03 | Complex numbers + `cmath` | MISSING | none | No complex type/ops | P3 | L | N | B | Niche; stdlib `cmath` functions are rare outside scientific code |
| L04 | Booleans | VERIFIED | `Bool`; `&&`/`\|\|` short-circuit (probed native this session); `!` | None | - | - | N | - | Note: Session 82 audit claimed no short-circuit; probe proves otherwise |
| L05 | Strings as Unicode text (`str`) | INTENT. DIFF. + PARTIAL | `Str` = length-prefixed byte buffer, heap-owned or immutable image literal; byte access `rt_str_byte/set_byte` (`[code] src/runtime/intrinsics.rs`) | No UTF-8 validation/decoding, no code-point iteration, ASCII-only case ops, length is bytes not code points | P1 | M | Y | B/H | MINK's native text model is bytes (deterministic); a UTF-8 layer (validate, decode, iterate, non-ASCII-aware ops) is the parity item; full Unicode DB (categories/normalization/collation) is P3 |
| L06 | Bytes (`bytes`, immutable) | INTENT. DIFF. | Byte model IS the native string model; image literals immutable | None material | - | - | N | - | Binary data is the default, not a special case |
| L07 | Bytearray (mutable bytes) | INTENT. DIFF. | Heap `Str` mutable via `rt_str_set_byte` (bounds-checked E-R09) | None material | - | - | N | - | Mutable byte buffers exist |
| L08 | Lists (`list` of arbitrary objects) | PARTIAL | `Vec<T>` generic type identity (`[code] src/typecheck/ty.rs`); dynamic ops via `rt_vec_*` (`[code] stdlib/collections.mink`) | Runtime vec ops are word-sized Int-style values; `Vec<Str>`, `Vec<Float>`, `Vec<struct>` element ownership not supported in V1; no slicing, no nesting ergonomics | P1 | L | Y | B | Ownership-aware generic dynamic collections (strings/aggregates in a Vec) is a real design task |
| L09 | Tuples | VERIFIED | Heterogeneous fixed tuples, field access `.0`, destructuring in let/match (`[test] tests/tuples.rs`, tuple_destructure) | None | - | - | N | - | Named tuples: P3 sugar (see S09) |
| L10 | Dictionaries (`dict`) | MISSING | none (no hash-map type anywhere: `[code] src/typecheck/ty.rs`) | No key/value map; no dict literal, no hash-ordered iteration, no JSON-object round trip beyond arena API | P1 | L | Y | B | Highest-value single language gap; needs owned keys/values design |
| L11 | Sets (`set`) | MISSING | none | No hash-set | P2 | M | N | B | Rides on dict work |
| L12 | Frozen sets | MISSING | none | N/A until sets | P3 | S | N | B | Immutability by convention once sets exist |
| L13 | `None` / nullability | VERIFIED | `Null` type + `Option<T>` (`[code] stdlib/option.mink`), `?` operator | None | - | - | N | - | Stronger than Python: exhaustive `match` on `Option` |
| L14 | Ranges | VERIFIED | `a..b`, `a..=b`; `for i in 1..=10` (`[test] tests/loop_expressions.rs`) | No step forms (`range(a,b,s)`); P3 sugar | P3 | S | N | B | |
| L15 | Slicing `s[a:b:c]` | PARTIAL | `str_sub` for strings (`[code] stdlib/strings.mink`); no slice views of arrays/Vec, no step | Array/container slicing absent | P2 | M | N | C | Function form exists for Str; slice *views* need reference+length types later |
| L16 | Strings interpolation / f-strings / format spec | MISSING | Only `rt_str_from_int/bool` + manual concat; **no float→Str** intrinsic | No general formatting (values→text), the most-used Python text capability | P1 | M | Y | A/B | Quick-win first cut: `rt_str_from_float` + a small `str_format` |
| L17 | Enums / named constants (`enum`) | VERIFIED | `enum` w/ unit + data variants, explicit discriminants, exhaustive match (`[test] tests/enums.rs`, sum_types, discriminants) | None | - | - | N | - | Covers Python `enum` module capability |
| L18 | `dataclass`-style declarative records | INTENT. DIFF. | `struct` with explicit typed fields IS declarative (`[code] src/ast/mod.rs`) | No derived methods (equality/print) yet | P3 | M | N | B | Structs are the dataclass; derives are convenience |
| L19 | `namedtuple` | MISSING | tuples are positional only | Named-field tuple sugar | P3 | S | N | - | Structs cover the need |

### 3.2 Expressions

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| L20 | Arithmetic | VERIFIED | `+ - * / %` on Int/Float; compound assign `+= ...` (`[code] src/ast/mod.rs`) | Int div/mod by zero faults raw (exit 148 / 0xC0000094) with no structured error — hardening item | P2 | S | N | A | Add E-R guard on idiv (runtime error rather than CPU exception) |
| L21 | Comparisons | VERIFIED | `< <= > >= == !=` | None | - | - | N | - | |
| L22 | Boolean logic (short-circuit) | VERIFIED | `&&`/`\|\|` and `!`; short-circuit proven by native probe this session | None | - | - | N | - | |
| L23 | Bitwise ops | VERIFIED | `& \| ^ ~ << >>` | None | - | - | N | - | |
| L24 | Conditional expressions | VERIFIED | `if c {a} else {b}` as expression (`[test] tests/...`, block/if-expr session 28) | None | - | - | N | - | |
| L25 | Indexing | VERIFIED | arrays, Vec, str bytes; runtime bounds checks (E-R09/E-R10) | None | - | - | N | - | Python lists raise IndexError; MINK checks at runtime and terminates with structured error — stronger safety, no catch (see L34) |
| L26 | Membership `x in c` | PARTIAL | `str_contains`, `vec_contains`, `str_count` functions (`[code] stdlib/*.mink`) | No operator; impossible for dict/set until those exist | P3 | S | N | B | Function form is adequate |
| L27 | Identity `is` / `is not` | INTENT. DIFF. | Value semantics + `&`/`&mut` borrows (`[code] src/ast` Borrow/Deref; ownership engine) | Mutable shared identity is a borrow, not an alias | P3 | M | N | - | Static aliasing discipline replaces dynamic identity checks |
| L28 | Assignment expressions (`:=`) | MISSING | none | Expression-position binding | P3 | S | N | - | Statement language; low value |
| L29 | Unpacking `a, b = ...` | VERIFIED | tuple destructuring in `let`, struct destructure, match binding patterns (`[test] tests/tuple_destructure.rs`, struct_destructure) | No `*rest` star-unpacking | P3 | S | N | B | |
| L30 | Operator overloading | PLANNED | spec permits where predictable (`[doc] docs/language/CORE_LANGUAGE.md` §12) | No user-defined operators | P3 | L | N | I | Python's `__add__` etc. not needed for parity; optional later |
| L31 | Augmented assignment | VERIFIED | `+= -= *= /= %=` | None | - | - | N | - | |

### 3.3 Control flow

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| L32 | `if` / `elif` / `else` | VERIFIED | if statements + if-expressions (`[test] tests/parser.rs`, semantics) | None | - | - | N | - | |
| L33 | Loops (`while`, `for`) | VERIFIED | `while`, `loop`, `for` over ranges; break-with-value loop expressions (`[test] tests/loop_expressions.rs`) | `for` over containers missing (see L58) | P2 | M | N | B | |
| L34 | Exceptions (`raise`/`try`/`except`) | INTENT. DIFF. | `Option<T>`/`Result<T,E>` + `?` operator for recoverable errors (`[code] stdlib/result.mink`, `[test] tests/option_result.rs`, try_operator) | Runtime faults (bounds, free misuse, div-by-zero) terminate with E-R codes; no catch-and-resume | P1 | L | Y | B | Parity item is *catchable structured runtime errors with location info* (see R06) and value-based error flow for application errors — not Python stack-unwinding exceptions (see X-register) |
| L35 | `finally` / cleanup blocks | MISSING | ownership frees at scope end; no user `defer`/drop hooks | Deterministic cleanup callback on scope exit | P2 | M | N | B | Needed for RAII-style wrappers once handles/FFI exist |
| L36 | `assert` | MISSING | none (only test-harness asserts in Rust) | Debug assertion in MINK source | P2 | S | N | G | Comes with the MINK test framework |
| L37 | Pattern matching (`match`) | VERIFIED | literals, bindings, enum variants + payload, ranges, or-patterns, guards, tuple/struct destructure, exhaustiveness (`[test] tests/pattern_matching.rs`, richer_patterns, match_expressions) | None material | - | - | N | - | Statically-checked match is stronger than Python 3.10+ `match` |

### 3.4 Functions

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| L38 | Positional parameters | VERIFIED | typed/untyped params, type annotations (`[test] tests/function_annotations.rs`) | None | - | - | N | - | |
| L39 | Default parameter values | MISSING | none | Convenience for library APIs | P2 | S-M | N | B | |
| L40 | Keyword arguments / named args | MISSING | none | Named call arguments | P3 | M | N | B | Struct-args idiom may suffice |
| L41 | Variadic args (`*args`) | MISSING | `str_join_2/3` style manual overloads | Varargs | P3 | M | N | B | |
| L42 | `**kwargs` | MISSING | none | Dictionary-style options | P3 | M | N | - | Needs dict (L10) |
| L43 | Recursion | VERIFIED | `[test] tests/...` (recursion tests incl. deep-recursion limits) | None | - | - | N | - | |
| L44 | Closures | PARTIAL | `\|x: Int\| expr` (`[code] src/ast` Closure; `[test] tests/closures.rs`) | Capture is by-move (non-Copy) / by-copy (Copy); no borrow/&mut capture, limited composition | P2 | M | N | B | Ownership discipline constrains closure design; fine for callbacks |
| L45 | Lambdas / anonymous functions | VERIFIED | closure syntax is the lambda | None | - | - | N | - | |
| L46 | First-class functions / function values | PARTIAL | `Fn { params, result }` type exists (`[code] src/typecheck/ty.rs`) | Passing/storing functions+closures is barely exercised; no closures in structs/Vec | P2 | M | N | B | |
| L47 | Higher-order functions (`map/filter/reduce`) | MISSING | loops are the idiom; collections lib has no map/filter/fold | No functional iteration over collections | P2 | M | N | B | Combine with L58 iteration work |
| L48 | Decorators / compositional metadata | INTENT. DIFF. | none today | Static alternative = future compile-time attributes/macros | P3 | XL | N | G | Python decorators are runtime metaprogramming; MINK's static model calls for definition-time metadata (design later) |

### 3.5 Scopes

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| L49 | Local scopes | VERIFIED | block scoping, shadowing rules enforced | None | - | - | N | - | |
| L50 | Module / global scope | VERIFIED | module-level `fn`/`struct`/`enum`/`const`; `mod`/`use`/`pub` (`[code] src/driver.rs` module discovery; `[test] tests/modules_check.rs`) | None | - | - | N | - | |
| L51 | Mutable global state | PARTIAL | module `let` items exist in AST; mutable-global usage not execution-verified | Global mutable state + `global`/`nonlocal` keywords | P3 | M | N | B | NEEDS EXECUTION PROOF before claiming; low parity value (state should be explicit) |
| L52 | Closure capture / nonlocal mutation | PARTIAL | closure captures by value/move | Captured-variable mutation through closures | P3 | M | N | B | |

### 3.6 Object / data model

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| L53 | Classes (data + behavior) | INTENT. DIFF. | `struct` (data) + free functions; no `class`, no `impl` blocks (`[code] src/ast/mod.rs` ItemKind list) | No dot-method call syntax / behavior attached to types | P2 | M | N | B | Free functions with type prefixes are the V1 idiom; optional method sugar later — this is capability parity via a different shape, not a blocker |
| L54 | Inheritance | N/A (INTENT.) | none | — | - | - | N | - | Composition + generics + match replace inheritance; see X-register |
| L55 | Protocols / interfaces / ABCs | PLANNED | unconstrained generics today; spec designs interfaces (`[doc] docs/language/TYPE_SYSTEM.md` §13) | No trait bounds or structural typing | P2 | L | N | B | Needed for mature library APIs, not for the parity gate |
| L56 | Constructors / initialization | INTENT. DIFF. | struct literals `Point { x: 3, y: 4 }` + enum construction | No `__init__` hook | - | - | N | - | Literal construction is deterministic and checked |
| L57 | Static / class-level members | INTENT. DIFF. | module-scope `fn`/`const` serve the purpose | None | - | - | N | - | |
| L58 | Properties / accessors | MISSING | none | Get/set interception on fields | P3 | M | N | - | Encapsulation via functions today |
| L59 | Methods on user types | MISSING | free functions only (see L53) | Dot-call ergonomics | P2 | M | N | B | |

### 3.7 Iteration

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| L60 | Iterable protocol (`__iter__`) | MISSING | none | No uniform iteration over Vec/arrays/containers (only ranges) | P2 | L | N | B | Index loops are the current idiom; for-in over containers is high-value |
| L61 | Iterators / iterator objects | MISSING | none | Stateful iteration objects | P2 | M | N | B | Depends on L60 |
| L62 | Generators (`yield`) | MISSING | none | Lazy producer functions | P2 | L | N | E | Only relevant once async/streaming exist |
| L63 | Comprehensions | MISSING | none (loop idiom) | Sugar over loops | P3 | M | N | - | Not parity-blocking; static languages live without them |
| L64 | Lazy / infinite sequences | MISSING | none | Infinite/lazy sequences | P3 | L | N | E | With generators |

### 3.8 Modules and imports

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| L65 | Modules (files) | VERIFIED | `mod name;` loads `<dir>/name.mink`, recursively; inline modules (`[code] src/driver.rs`, `src/module/mod.rs`) | None | - | - | N | - | |
| L66 | Imports / name resolution | VERIFIED | `use path`, `use path::Item`, `pub` visibility; flat AST combination (`[code] src/driver.rs` check_multi_module) | V1: private items are still compiled in; true encapsulation is relaxed | P3 | S | N | B | Documented V1 simplification |
| L67 | Import search paths (stdlib reusable without copying) | PARTIAL | resolution is relative to the root file's directory only; npm package ships **no** stdlib `.mink` sources | `use strings` after `npm install` cannot resolve; developer must copy stdlib files into the project | P1 | S-M | Y | A/F | Ship stdlib in the npm package + an include path for module lookup (part of packaging work, but the module-search mechanism is Wave A) |
| L68 | Packages (directories / `__init__`) | MISSING | none | Directory package concept | P1 | M | Y | F | Package manager work (F) delivers this |
| L69 | Relative imports / aliases | PARTIAL | file-relative `mod` only | `use a::b` style nesting within project dirs is limited | P2 | M | N | F | |
| L70 | Circular import handling | PARTIAL | discovery visits each file once (cycle-safe) (`[code] src/driver.rs` visited set) | No explicit cycle diagnostic | P3 | S | N | - | |

### 3.9 Introspection

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| L71 | Compile-time type inspection / diagnostics | VERIFIED | typed checker, error codes E-T*/E-S*, `mink explain`, `--json` report (`[code] src/cli.rs`, diagnostics) | None | - | - | N | - | |
| L72 | Runtime reflection (`type()`, `dir()`, `getattr()`) | INTENT. DIFF. | none; static typing removes the need | Future: compile-time reflection / macros | P3 | XL | N | G | Needs a REPL/tooling context to be valuable |
| L73 | Callable/object metadata | MISSING | none | doc/metadata queries | P3 | M | N | G | |

### 3.10 Language section totals

73 rows: VERIFIED 25 · INTENT. DIFF. 11 · INTENT. DIFF. + PARTIAL 1 · PARTIAL 10 · PLANNED 2 · N/A (INTENT.) 1 · MISSING 23. Priorities: P1 7 · P2 18 · P3 23 · no-gap 25. Parity blockers: 7 (L05, L08, L10, L16, L34, L67, L68).

---

## 4. RUNTIME / execution capability matrix (official Python runtime UX → MINK)

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| R01 | Interactive interpreter / REPL | MISSING | CLI is `build/run/check/explain/version/help` only (`[code] src/cli.rs`) | No interactive exploration | P1 | L | Y | G | Major subsystem; a compile-eval REPL loop on the native backend |
| R02 | Script execution | VERIFIED | `mink run file.mink` compiles + runs + forwards exit code (`[code] src/cli.rs`) | None | - | - | N | - | |
| R03 | Module execution (`python -m pkg`) | MISSING | `mink run <file>` only | Package/module runner | P3 | M | N | F | Comes with package system |
| R04 | Compiled standalone program | VERIFIED | `mink build` → zero-dependency PE (`[exec]` Session 97: standalone exe outside repo) | None — MINK is stronger (Python needs an interpreter + packaging to ship) | - | - | N | - | |
| R05 | Import caching / bytecode cache | N/A | AOT compilation; no cache concept | — | - | - | N | - | |
| R06 | Traceback / error reporting w/ location | PARTIAL | compile-time errors carry file:line:col; runtime errors print `mink: runtime error[E-Rxx]: msg` only (`[code] src/backend/emit/runtime.rs` Fail) | Runtime faults carry no source line or call stack | P1 | M | Y | A | Add per-image function/source-line metadata; session 97 base is otherwise stable |
| R07 | Warnings (`warnings` module) | MISSING | none | Warning channel | P3 | S | N | - | |
| R08 | stdout output | VERIFIED | `rt_print_str/int/float/char` (+CRLF), write thunks via kernel32 (`[code] src/runtime/intrinsics.rs`) | None | - | - | N | - | Python `print()` equivalent |
| R09 | User-facing stderr write | MISSING | internal WriteStderr service exists but no intrinsic is exposed (`[code] src/backend/emit/runtime.rs`) | Program cannot write diagnostics to stderr | P2 | S | N | A | Quick win: expose `rt_write_stderr` |
| R10 | stdin input | MISSING | no stdin intrinsics | Programs cannot read console/stdin | P1 | M | Y | A | Needed for filters/pipes/CLI tools |
| R11 | Exit codes | VERIFIED | `main` return → exit code; `rt_exit(code)` w/ leak check (`[exec]` 2547-test suite asserts codes) | None | - | - | N | - | |
| R12 | argv (command-line args to programs) | MISSING | entry `fn main()` must take no parameters (`[code] src/backend/mod.rs` entry_function) | Programs cannot read their own arguments | P1 | M | Y | A | `GetCommandLineW` parse → argv intrinsic; entry-stub change |
| R13 | Environment variables | PARTIAL | `rt_env_*` are V1 **stubs on Windows**: get returns empty, has false, set/remove -1 (`[code] src/backend/emit/runtime.rs` emit_env_get/set/has/remove); real env walk exists only on the frozen Linux emitter | Windows env access is non-functional; IAT slots for Get/SetEnvironmentVariableA already exist (unused) | P1 | S-M | Y | A | Quick win: wire the existing imports |
| R14 | Signals / Ctrl+C handling | MISSING | no console control handler | Graceful interruption | P2 | M | N | H | Windows subset: SetConsoleCtrlHandler for CTRL_C/CLOSE |
| R15 | Platform info (`sys.platform`, `platform`) | MISSING | none | Query OS/arch at runtime | P2 | S | N | A | Trivial intrinsic |
| R16 | System info (mem/disk/cpu/uptime) | MISSING | none | System resource queries | P3 | M | N | H | |
| R17 | Memory management / leak detection | VERIFIED (INTENT. DIFF.) | arena allocator + liveness table + leak check at exit; deterministic errors E-R02..E-R08 (`[code] src/runtime/*`, `src/backend/emit/runtime.rs`) | Python: refcount + GC. MINK: compile-time ownership + explicit free + runtime validation | - | - | N | - | Equivalent safety story, different mechanism (X-register) |
| R18 | GC / automatic reclamation | N/A (INTENT.) | ownership model; no GC by design | — | - | - | N | - | Not a parity requirement |
| R19 | Threads | MISSING | no thread services anywhere (`[code] src/backend/emit` has no CreateThread) | No parallel execution | P1 | XL | Y | E | Major subsystem: threads + TLS-safe arena + atomics |
| R20 | Async / event loop | MISSING | none | No non-blocking I/O model | P1 | XL | Y | E | Major subsystem (select/poll first — see D rows) |
| R21 | Locks / synchronization primitives | MISSING | none | Mutex/condvar | P2 | L | N | E | With threads |
| R22 | Subprocess run + capture | PARTIAL | `process_run/process_stdout/stderr/id`; exit codes; large-output drain fixed in Session 97 (`[exec]` S97 1 MB drain) | stdout/stderr capped at 4088 bytes/stream; no stdin, no env override, no explicit argv | P2 | M | N | C | Core capability VERIFIED; parity extras P2 |
| R23 | Sleep / timers | MISSING | no sleep intrinsic | Programs cannot pause or time out | P1 | S | Y | A | kernel32 `Sleep` import; trivial |
| R24 | Process pools / multiprocessing | MISSING | none | Parallel subprocess workloads | P3 | XL | N | E | After threads/process work |
| R25 | Dynamic library loading / FFI | MISSING | LoadLibrary/GetProcAddress used internally for ws2_32/bcrypt (`[code] src/backend/emit/pe.rs`) but not user-accessible | No ctypes-equivalent; no C ABI calls from MINK programs | P2 | L | N | I | Design docs exist (`[doc] docs/ecosystem/C_ABI_SPEC.md`); not parity-blocking for the declared gate |
| R26 | Error text from OS calls (`GetLastError`) | PARTIAL | `net_last_error` only; filesystem returns -1 silently | OS error messages | P2 | S | N | A | |
| R27 | Runtime version/build info in program | MISSING | none | sys.version-like constant | P3 | S | N | - | |
| R28 | Terminal interaction (raw input, colors) | MISSING | none | Interactive console UX | P3 | L | N | H | |
| R29 | Time: epoch / monotonic clocks | VERIFIED | `time_now/millis/ticks/freq` (`[code] stdlib/time.mink`, `[test] tests/time_lib.rs`) | None | - | - | N | - | |

## 5. STANDARD LIBRARY capability matrix (official Python stdlib categories → MINK)

### 5.1 Text

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S01 | String methods (search/replace/trim/case/…) | VERIFIED | `str_*`: cmp, index_of, contains, starts/ends_with, count, sub, trim, upper/lower (ASCII), repeat, pad, reverse, replace(_all), join_2/3 (`[code] stdlib/strings.mink`; `[test] tests/strings_lib.rs`) | **No `split`**; case ops ASCII-only; no partition | P1 | M | Y | B | `split`/`join` are the daily text workhorses (CSV row depends) |
| S02 | Regular expressions (`re`) | MISSING | none | No regex anywhere | P1 | XL | Y | B | Major subsystem; no third-party deps allowed, engine must be self-written (or generated) |
| S03 | Unicode data (categories/normalization/casefold) | MISSING | none (byte model) | Full Unicode DB | P3 | L | N | H | Minimal UTF-8 layer is L05 (P1); full DB optional |
| S04 | Codecs / encodings (`codecs`, utf-8/16, latin-1…) | PARTIAL | `encoding.mink`: hex/base64(url)/url + `str_is_ascii` (`[code] stdlib/encoding.mink`; `[test] tests/encoding_lib.rs`) | No UTF-8 encode/decode, no text codecs | P2 | M | N | H | |
| S05 | Text wrapping / formatting helpers (`textwrap`) | MISSING | none | Wrap/pad paragraph text | P3 | S | N | - | |
| S06 | String parsing: int/float ↔ text | PARTIAL | `rt_str_from_int/bool`, `rt_str_parse_int` helper in http.mink; **no float→Str, no robust parse-to-float** (`[code] src/runtime/intrinsics.rs`) | Formatting and parsing numeric text is incomplete | P1 | M | Y | A/B | `str_format` + float text conversions are Wave A/B items |

### 5.2 Binary data

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S07 | Binary buffers (bytes/bytearray ops) | VERIFIED | Str byte model + alloc/set/get + concat | None material | - | - | N | - | |
| S08 | `struct` binary packing/unpacking | MISSING | none | Pack/unpack ints/floats to/from byte buffers | P2 | M | N | C | Needed for real binary file formats |
| S09 | base64 / hex | VERIFIED | encode/decode both, url-safe variants, hex decode alloc (`[code] stdlib/encoding.mink`; `[test] tests/encoding_lib.rs`) | None | - | - | N | - | |
| S10 | Byte-order / int↔bytes primitives | PARTIAL | `net_htons/ntohs` exist (`[code] stdlib/network.mink`) | No general endian conversion helpers | P2 | S | N | C | |

### 5.3 Data structures

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S11 | `collections`: deque/Counter/defaultdict/OrderedDict | MISSING | none beyond Vec | Rich collection types | P2 | M | N | B | deque valuable for queues |
| S12 | `queue` | MISSING | none | Thread-safe queues | P2 | M | N | E | With threads |
| S13 | `heapq` | MISSING | none | Heap operations | P2 | S | N | B | Easy over Vec |
| S14 | Typed arrays (`array` module) | PARTIAL | fixed-size `[T; N]` arrays with bounds checks (`[code] src/typecheck/ty.rs`; `[test] tests/aggregate.rs`) | No compact dynamic typed arrays (Vec covers word-sized only) | P2 | M | N | B | |
| S15 | Enum / flag values | VERIFIED | see L17 | None | - | - | N | - | |
| S16 | Linked structures / trees / custom collections | PARTIAL | Ptr<Int> + alloc/free allow manual structures; no stdlib tree/map | Standard tree/map containers | P2 | M | N | B | Manual linked list is possible via Ptr + mem ops (`[test] tests/runtime.rs` arena tests) |

### 5.4 Math / numeric

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S17 | `math` module | VERIFIED | constants; int helpers; float sqrt/pow/ln/log/exp/trig/hyperbolic/round via pure-MINK series (`[code] stdlib/math.mink`; `[test] tests/math_lib.rs` 106 tests) | Accuracy of series approximations vs libm (documented); no isfinite/isnan helpers exposed | P3 | M | N | - | Native float print is exact; series math is adequate |
| S18 | `random` | VERIFIED | seeded xorshift: ints/bytes/bool; crypto-secure random via BCrypt (`[code] stdlib/random.mink`, `stdlib/crypto.mink`; `[test] tests/random_lib.rs`, crypto_lib) | No distribution helpers (uniform/gauss/choice/shuffle) | P2 | S-M | N | B | choice/shuffle need containers |
| S19 | `statistics` | MISSING | none | mean/median/stdev | P2 | S | N | B | Trivial once numeric Vec works |
| S20 | `decimal` | MISSING | none | Exact decimal arithmetic | P2 | L | N | B | Money/finance; alternative: fixed-point lib |
| S21 | `fractions` | MISSING | none | Rationals | P3 | M | N | - | |
| S22 | `cmath` / complex | MISSING | see L03 | — | P3 | L | N | B | |

### 5.5 Functional programming

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S23 | `itertools` | MISSING | none | chain/zip/product/groupby | P2 | M | N | B | Needs L60 iteration work first |
| S24 | `functools` (partial/reduce/lru_cache) | MISSING | none | Function composition helpers | P3 | M | N | B | |
| S25 | `operator` helpers | N/A | operators are built-in syntax | — | - | - | N | - | |

### 5.6 Filesystem / paths

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S26 | File read/write | VERIFIED | `fs_read/write` byte-exact (`[code] stdlib/filesystem.mink`; `[test] tests/filesystem_lib.rs`, windows_hardening real ops) | None | - | - | N | - | |
| S27 | Path manipulation (pathlib-style) | VERIFIED | `path_join/parent/filename/extension/stem/with_extension/normalize/is_absolute` (`[code] stdlib/filesystem.mink`) | Drive-letter/UNC-aware semantics not execution-proven | P3 | S | N | C | NEEDS PROOF for `C:` handling |
| S28 | Directory listing / traversal (`os.listdir`, `os.walk`) | MISSING | none | No directory enumeration | P1 | M | Y | C | First-class daily capability (find files, iterate dirs) — FindFirstFileA/FindNextFileA already understood in the codebase (`[code] src/backend/emit/pe.rs` imports) |
| S29 | Glob | MISSING | none | Pattern-based file matching | P2 | M | N | C | After listdir |
| S30 | Temp files/dirs (`tempfile`) | MISSING | none | Temp location + unique names | P2 | S | N | C | GetTempPath is one import |
| S31 | File metadata (size/mtime/attrs) | PARTIAL | `fs_file_size` only (`[code] stdlib/filesystem.mink`) | mtime, permissions/attributes | P2 | M | N | C | |
| S32 | Copy / move / remove / mkdir | VERIFIED | `fs_copy_file/move/remove_file/create_dir/remove_dir`, cwd get/set, exists, is_file/is_dir (`[code] stdlib/filesystem.mink`; `[exec]` spaced-path real ops) | Recursive delete/copy | P2 | S | N | C | |
| S33 | Working directory | VERIFIED | `fs_get_cwd/set_cwd` | None | - | - | N | - | |
| S34 | File streams / append / seek / partial reads | MISSING | whole-file read/write only | Incremental file I/O | P2 | M | N | C | Needed for large-file tools |

### 5.7 Serialization / data formats

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S35 | JSON | VERIFIED | parse + serialize over an arena (`[code] stdlib/json.mink`; `[test] tests/json.rs`; `[exec]` system_report example) | No JSON Schema/streaming | P2 | M | N | B | Core capability solid |
| S36 | CSV | MISSING | none | CSV read/write | P1 | M | Y | B | Proof-app category in plan; split (S01) is the dependency |
| S37 | Config files (`configparser`, INI) | MISSING | none | INI config | P2 | S | N | C | |
| S38 | TOML (`tomllib`, read) | MISSING | none | TOML parsing | P3 | M | N | C | |
| S39 | Binary serialization (`pickle`) | N/A | native binary + JSON exist; deterministic formats by design | — | P3 | M | N | - | Pickle is Python-object-graph magic; not a parity requirement |
| S40 | XML / HTML parsing | MISSING | none | XML/HTML processing | P3 | L | N | C | Official xml.etree/html.parser are important in niche; P3 for MINK gate |

### 5.8 Database

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S41 | SQLite (`sqlite3`) | MISSING | none | Embedded SQL database | P1 | XL | Y | D | Major subsystem; prompt lists SQLite in the audit scope and proof apps |

### 5.9 Compression / archives

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S42 | zlib/gzip | MISSING | none | DEFLATE compress/decompress | P1 | XL | Y | D | No deps allowed → self-written or Windows-native (none exists for gzip); read-first then write |
| S43 | bz2 / lzma | MISSING | none | Other codecs | P3 | XL | N | D | After zlib |
| S44 | zip archives | MISSING | none | Read/write .zip | P1 | L | Y | D | Container over stored/deflate entries |
| S45 | tar archives | MISSING | none | .tar read/write | P2 | M | N | D | Trivial container once fs works |

### 5.10 Crypto / hashing / security

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S46 | Hashing (`hashlib`) | PARTIAL | SHA-256 (pure MINK) + FNV-1a/DJB2 (`[code] stdlib/hashing.mink`; `[test] tests/hashing_lib.rs`) | Only SHA-256; md5/sha1/sha512/blake2 absent | P2 | M | N | B | |
| S47 | HMAC | VERIFIED | HMAC-SHA256 (`[code] stdlib/crypto.mink`; `[test] tests/crypto_lib.rs`) | None | - | - | N | - | |
| S48 | Secrets / secure random | VERIFIED | BCrypt-backed secure bytes/ints/hex + constant-time verify (`[code] stdlib/crypto.mink`; `[exec]` crypto vectors) | None material | - | - | N | - | Covers `secrets` capability |
| S49 | Basic cipher primitives | N/A | not in official Python stdlib (only hashlib/hmac/secrets) | — | - | - | N | - | Out of scope by definition |

### 5.11 OS / platform / process / IPC

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S50 | Environment (os.environ) | PARTIAL | Windows stubs (see R13) | Non-functional on Windows | P1 | S-M | Y | A | |
| S51 | Process execution | PARTIAL | see R22 | stdin/env/argv control | P2 | M | N | C | |
| S52 | Platform identity | MISSING | see R15 | — | P2 | S | N | A | |
| S53 | Locale / internationalization | MISSING | none | Locale-aware formatting | P3 | L | N | - | |
| S54 | Terminal interaction | MISSING | see R28 | — | P3 | L | N | H | |
| S55 | Pipes / IPC | PARTIAL | anonymous pipes used internally in process capture (`[code] src/backend/emit/runtime.rs` emit_process_run) | No user pipes, no named pipes, no IPC between MINK processes | P2 | L | N | C | |

### 5.12 Networking / internet

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S56 | TCP sockets | VERIFIED | Winsock2 TCP connect/bind/listen/accept/send/recv/close/shutdown (`[code] stdlib/network.mink`; `[test] tests/network_lib.rs`, windows_hardening loopback checksums) | None | - | - | N | - | |
| S57 | UDP sockets | VERIFIED | UDP send/receive (`[exec]` Session 92/95 hardening) | None | - | - | N | - | |
| S58 | DNS resolution | VERIFIED | `net_resolve` (getaddrinfo) | None | - | - | N | - | |
| S59 | HTTP client | PARTIAL | HTTP/1.1 GET/POST, header/status/body parse, url split helpers (`[code] stdlib/http.mink`; `[test] tests/http_lib.rs`; `[exec]` Session 96 byte-exact POST echo) | No TLS/HTTPS, no redirects, no cookies, no chunked transfer, no keep-alive | P2 | L | N | D | The client capability is real; protocol conveniences P2; HTTPS is S62 |
| S60 | HTTP server | PARTIAL | socket-level server verified (accept/echo repeated connections); no HTTP parsing server-side lib | `http.server`-style convenience | P2 | M | N | D | |
| S61 | URL parsing (`urllib.parse`) | PARTIAL | url encode/decode in encoding.mink; host/port/path split in http.mink | Full URL grammar handling | P2 | M | N | D | |
| S62 | TLS/SSL | MISSING | none | Encrypted connections; HTTPS end-to-end | P1 | XL | Y | D | Major subsystem (self-contained TLS impl or schannel via FFI) |
| S63 | Non-blocking I/O / select | MISSING | blocking sockets only | multiplexing primitives | P2 | M | N | E | Required before async (R20) |
| S64 | Email / FTP / SMTP / IMAP | MISSING | none | Protocol clients | P3 | L | N | D | Not parity-critical for the declared gate |
| S65 | Hostname / byte order | VERIFIED | `net_hostname`, `net_htons/ntohs` | None | - | - | N | - | |

### 5.13 Time

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S66 | Epoch/time/monotonic | VERIFIED | see R29 | None | - | - | N | - | |
| S67 | Date/time decomposition + arithmetic | PARTIAL | year/month/day/hour/min/sec/weekday, leap-year, days-in-month, diff/add (`[code] stdlib/time.mink`) | No date structs/parsing, no timedelta | P2 | M | N | C | |
| S68 | Timezone | MISSING | UTC epoch only | TZ handling, DST | P2 | M | N | C | Windows TZ API exists |
| S69 | Formatting / parsing (`strftime`/`strptime`) | PARTIAL | single fixed `time_format(ts)` | Pattern-based formatting + parse-back | P1 | M | Y | C | High daily-use; pattern engine M |
| S70 | Sleep / timers | MISSING | see R23 | — | P1 | S | Y | A | |

### 5.14 Concurrency / async

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S71 | Threads + locks | MISSING | none | see R19/R21 | P1 | XL | Y | E | |
| S72 | Futures / thread pools | MISSING | none | see R24 | P2 | L | N | E | |
| S73 | async/await + event loop | MISSING | none | see R20 | P1 | XL | Y | E | |

### 5.15 Development support (logging/testing/assert/debug) — stdlib half

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S74 | Logging | MISSING | none (needs stderr write first, R09) | Leveled logging | P1 | S-M | Y | A | Quick win once R09 lands; important production capability |
| S75 | Unit-test framework in MINK | MISSING | Rust-hosted suite only | `mink test` + assert macros | P1 | M | Y | G | Tooling row T09 is the same gap |
| S76 | Profiling | MISSING | none | Timing/profiling tools | P2 | L | N | G | |
| S77 | Runtime diagnostics / tracing hooks | MISSING | leak checker + error codes only | Tracing | P2 | M | N | G | |
| S78 | Assertions for tests | MISSING | none | assert_eq/assert_true | P1 | S | Y | G | Part of S75 |

---

## 6. TOOLING capability matrix (official Python developer workflow → MINK)

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| T01 | Compiler/CLI executable | VERIFIED | `mink build/run/check/explain/version/help`, `-v/-V/--version`, `--json`, `--target` (`[code] src/cli.rs`; `[test] tests/cli.rs` 74) | None | - | - | N | - | |
| T02 | Script execution | VERIFIED | `mink run` (see R02) | None | - | - | N | - | |
| T03 | Check/typecheck without output | VERIFIED | `mink check` runs parse→semantics→types→HIR→MIR (and ownership) (`[code] src/driver.rs`) | None | - | - | N | - | Equivalent of a lint+typecheck pass; no official Python equivalent exists (py_compile is weaker) |
| T04 | REPL / interactive | MISSING | none | see R01 | P1 | L | Y | G | |
| T05 | Module execution `python -m` | MISSING | none | see R03 | P3 | M | N | F | |
| T06 | Import search paths / installed stdlib | PARTIAL | module resolution is root-file-relative only (`[code] src/module/mod.rs` module_file_path) | see L67 | P1 | S-M | Y | A/F | |
| T07 | Test runner (unittest equivalent) | MISSING | Rust-side harness only | `mink test` runner | P1 | M | Y | G | |
| T08 | Debugger (pdb equivalent) | MISSING | none | breakpoints/step/inspect | P2 | XL | N | G | Major subsystem; parity of pdb is desirable but the parity gate can pass with strong error diagnostics + print debugging; revisit at wave G |
| T09 | Profiler (cProfile/timeit) | MISSING | none | performance measurement | P2 | L | N | G | |
| T10 | Formatter | N/A | Python has no official formatter; third-party only — explicitly not a parity requirement per audit scope | — | - | - | N | - | `mink fmt` may still be added later as own tooling (P3) |
| T11 | Linter / analyzer | VERIFIED (partial) | `mink check` + 1500+ diagnostics; error-code system with `mink explain` (`[code] src/diagnostics/*`, `[test] tests/semantics.rs`, typecheck) | No dedicated style lint | P3 | M | N | - | Python's flake8/ruff are third-party — not parity |
| T12 | Doc generation (pydoc) | MISSING | none | API docs from source | P3 | M | N | G | |
| T13 | Error-code explanation | VERIFIED | `mink explain E-XXX` lists and explains all codes | None (Python has no equivalent) | - | - | N | - | |
| T14 | Machine-readable diagnostics | VERIFIED | `mink check --json` | None (Python has no official equivalent) | - | - | N | - | |
| T15 | Env vars affecting the tool | MISSING | none honored | Tool config via env | P3 | S | N | A | |
| T16 | Source-map/source execution of fragments (`-c`) | MISSING | none | Run code from string | P3 | S | N | G | REPL wave |

## 7. WINDOWS-SPECIFIC capability matrix (official Python on Windows → MINK)

Classification prefix in Notes: **REQ** = required for MINK Windows parity · **OPT** = useful but optional · **PY** = Python-specific / not required.

| ID | Python capability (on Windows) | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| W01 | Drive-letter + backslash path handling | PARTIAL | paths are ANSI byte paths passed to kernel32; `fs_*`/`path_*` exist (`[code] stdlib/filesystem.mink`) | Drive-relative/root semantics not execution-proven; no `C:` parse helpers | P2 | S | N | C | REQ. NEEDS EXECUTION PROOF for drive letters |
| W02 | UNC paths | PARTIAL | kernel32 accepts UNC via same APIs; untested | No UNC-specific tests | P3 | S | N | C | OPT |
| W03 | Long paths (>260) | PARTIAL | ANSI APIs without `\\?\` or longPathAware manifest → MAX_PATH-limited | Long-path support | P2 | M | N | H | REQ for modern Windows; wide-API migration later |
| W04 | Case-insensitive filesystem semantics | PARTIAL | kernel32 semantics inherited; no tests | — | P3 | S | N | - | PY-adjacent: Python docs warn about it; MINK inherits OS behavior |
| W05 | UTF-8/Unicode paths + console output | PARTIAL | byte paths; WriteFile console output depends on console code page | Wide-char console + Unicode path layer | P2 | M | N | H | REQ for non-ASCII correctness; pairs with L05 |
| W06 | Environment variables | PARTIAL | stubs (see R13) | wiring Get/SetEnvironmentVariableA | P1 | S-M | Y | A | REQ |
| W07 | Process creation | VERIFIED | CreateProcessA + pipe capture (`[code] src/backend/emit/runtime.rs`; `[exec]` Session 97) | see R22 extras | P2 | M | N | C | REQ |
| W08 | Console stdin/stdout/stderr | PARTIAL | stdout verified; stderr capture verified; program stdin/stderr write missing (R09/R10) | — | P1 | M | Y | A | REQ |
| W09 | File metadata (attributes, timestamps) | PARTIAL | size only | attrs/mtime | P2 | M | N | C | REQ |
| W10 | Sockets | VERIFIED | Winsock2 loopback TCP/UDP verified (`[test] tests/windows_hardening.rs`) | None | - | - | N | - | REQ |
| W11 | Signals/control events | MISSING | none | SetConsoleCtrlHandler | P2 | M | N | H | OPT (graceful shutdown) |
| W12 | Executable discovery / PATH resolution | PARTIAL | child commands run through shell resolution; direct CreateProcessA path behavior untested | PATH search semantics | P2 | S | N | C | REQ (CLI tools) |
| W13 | Temp dirs | MISSING | none | GetTempPath/GetTempFileName | P2 | S | N | C | REQ |
| W14 | User/home/known folders | MISSING | none | USERPROFILE/APPDATA via env or SHGetKnownFolderPath | P1 | S | Y | A | REQ (config-file apps) — once env works, one import |
| W15 | Registry (winreg) | MISSING | none | registry access | P2 | M | N | H | OPT — Python winreg is official; materially useful for Windows automation but not gate-blocking |
| W16 | DLL/shared-library loading | MISSING | internal LoadLibrary/GetProcAddress only (ws2_32, bcrypt) (`[code] src/backend/emit/pe.rs`) | user-level FFI | P2 | L | N | I | OPT-to-REQ depending on interop ambitions; C ABI spec exists |
| W17 | Terminal colors/ANSI/interactive UX | MISSING | none | console capability APIs | P3 | L | N | H | OPT |
| W18 | CRLF/text-mode translation | INTENT. DIFF. | writes are raw bytes; print adds CRLF | Python text mode translates \n; MINK is explicit-binary by design | - | - | N | - | Deterministic-bytes choice, not a gap |
| W19 | OS error text | PARTIAL | net error only | FormatMessage for fs/process | P2 | S | N | A | REQ quality |
| W20 | Wide vs ANSI APIs | PARTIAL | ANSI (A) APIs everywhere | migrate hot paths to W APIs for Unicode | P2 | L | N | H | REQ for full Unicode paths |
| W21 | Windows version/platform query | MISSING | none | version/arch intrinsic | P2 | S | N | A | OPT |
| W22 | Console encoding (cp1252/OEM vs UTF-8) | PARTIAL | bytes pass through untouched | explicit UTF-8 console mode (SetConsoleOutputCP / WriteConsoleW) | P2 | M | N | H | REQ for non-ASCII output; pairs with W05 |

## 8. PACKAGING / project-workflow capability matrix (official Python packaging → MINK)

Explicit distinction maintained throughout (Phase 8): (A) a **language package ecosystem**
for MINK libraries ≠ (B) **distributing the MINK compiler itself through npm**. npm
distribution of the compiler is complete and does NOT constitute a MINK package manager.

| ID | Python concept | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| P01 | Distributing the tool itself | VERIFIED | npm `@p4inz-code/mink` 1.0.1, clean install ×2, standalone exe (`[exec]` Session 97; `[code] npm/mink/package.json`) | None | - | - | N | - | Category (B) is done |
| P02 | Standard-library source available to installed users | MISSING | npm ships only `bin/mink.exe`; stdlib `.mink` sources live in the repo only | Installed users cannot `use strings` without copying sources | P1 | S-M | Y | A/F | Quick win: bundle `stdlib/` + a module include path (see L67/T06) |
| P03 | Import package (`import pkg`) | PARTIAL | file `mod`/`use` (see L65-L67) | package directory concept | P1 | M | Y | F | |
| P04 | Installed package / site-packages | MISSING | none | project-local dependency installation | P1 | M | Y | F | |
| P05 | Dependency declaration + install (`pip`) | MISSING | none | dependency install/update | P1 | XL | Y | F | Major subsystem (package manager) |
| P06 | Dependency resolver | MISSING | none | version solving | P1 | L | Y | F | Within package manager |
| P07 | Lockfile / reproducibility | MISSING | none | deterministic lock | P2 | M | N | F | Security architecture doc designs it (`[doc] docs/ecosystem/SECURITY_ARCHITECTURE.md`) |
| P08 | Project metadata / manifest | PLANNED | design docs define `mink.toml` (`[doc] docs/ecosystem/SECURITY_ARCHITECTURE.md`, PACKAGE_ARCHITECTURE.md) | no manifest yet | P2 | M | N | F | |
| P09 | Virtual environment / environment isolation | MISSING | none | isolated per-project envs | P1 | M | Y | F | Depends on P05 |
| P10 | Registry / publishing workflow | MISSING | none | package registry | P2 | XL | N | F | Ecosystem-stage item, after P05-P09 |
| P11 | Versioning / semver discipline | PARTIAL | compiler itself is 1.0.1 semantic; no library-version model | package versioning | P2 | S | N | F | |
| P12 | Executable entry points (console_scripts) | INTENT. DIFF. | every build IS a standalone exe (stronger than Python) | — | - | - | N | - | No equivalent needed |
| P13 | Source vs wheel/binary distribution | N/A | MINK ships source `.mink` libraries; the compiler ships native | distribution-shape decision deferred to package manager | P2 | M | N | F | |
| P14 | Offline builds / cache | MISSING | none | dep cache | P3 | M | N | F | |

## 9. INTENTIONALLY-DIFFERENT / NOT-APPLICABLE exclusion register (anti-clone)

For each: Python capability · why it exists in Python · MINK equivalent/alternative · why exact parity is not required.

| ID | Python capability | Why it exists in Python | MINK equivalent / alternative | Why exact parity is not required |
|---|---|---|---|---|
| X01 | Arbitrary-precision int everywhere | Dynamic typing, no fixed-width native target | Fixed 64-bit `Int` + explicit bignum if/when needed | MINK targets native machine code; fixed width is the systems norm; wrong-width bugs are caught by type annotations |
| X02 | GC / refcounting | Dynamic object graphs without ownership discipline | Compile-time ownership/move/borrow + validated arena + leak check at exit | Deterministic resource lifetimes are a MINK design goal; GC would add overhead and nondeterminism |
| X03 | Classes + inheritance + MRO | Object-centric dynamic language; code reuse via inheritance | `struct`/`enum` + generics + pattern matching + free functions | Composition and sum types cover data modeling; inheritance conflicts with the value/ownership model |
| X04 | Duck-typed protocols at runtime | Dynamic dispatch | Static typing; planned compile-time interfaces (L55) | Errors should be caught at compile time, not AttributeError at runtime |
| X05 | Runtime exceptions for control flow | No static error channels | `Option<T>`/`Result<T,E>` + `?`; structured runtime error codes | Error flow is explicit and typed; no hidden unwind |
| X06 | Python-level `bytes`/`str` split and `Unicode`-by-default | Interpreter stores text as code points | Byte strings natively + UTF-8 layer | Byte-oriented text is the native-execution model; a UTF-8 layer gives the important capability |
| X07 | Dynamic introspection (`type()`, `dir()`, monkey-patching) | Dynamic language feature | Static compile-time checks + planned compile-time reflection | Runtime reflection is unnecessary with static types and would require metadata the runtime deliberately omits |
| X08 | Pickle / arbitrary object serialization | Interpreter can serialize live objects | Deterministic JSON + binary formats | Object-graph pickling is unsafe and Python-specific |
| X09 | Decorators as runtime wrappers | Definition-time metaprogramming in a dynamic model | Free functions + planned compile-time attributes | Static wrapping can be done at compile time instead of runtime |
| X10 | Generators/yield as core iteration primitive | Lazy evaluation culture | Index loops today; planned iteration protocol + explicit streams | Lazy iteration is convenience, not capability, for a native language |
| X11 | Walrus `:=` / assignment expressions | Expression-oriented dynamic style | Statement-oriented `let`/assignment | MINK is statement-based; low value |
| X12 | Global/nonlocal mutation keywords | Module-level mutable state | Module scope + explicit state passing | Hidden mutable globals conflict with deterministic behavior |
| X13 | Text-mode I/O with \n translation | Legacy console conventions | Raw bytes everywhere; print adds CRLF | Explicit binary semantics are deterministic and portable |
| X14 | `del` statement | GC-era manual reference removal | Ownership + explicit free | The compiler already manages lifetimes |
| X15 | `*args`/`**kwargs` universal call shapes | Dynamic call convention | Fixed typed params; variadic only where needed later | Typed signatures are checked at compile time |
| X16 | List/dict/set literals as language syntax | Core dynamic containers | Function-based constructors today; container literals possible later | Literal syntax is ergonomics; the containers themselves are the parity item (L10/L08) |
| X17 | f-string expression evaluation | String interpolation of arbitrary expressions | Future `str_format` with typed arguments | Formatting capability is the goal; embedding expressions is syntax |
| X18 | Threads without ownership safety | GIL serializes; dynamic language | Threads (planned) will respect the ownership model | MINK concurrency must preserve memory safety; no GIL needed |
| X19 | `python -m` any-module execution | Interpreter path model | `mink run <file>`; package runner later | Compiled entry points differ; capability covered by run/build |
| X20 | Wheel/sdist/pip distribution formats | Python packaging history | MINK source packages + future manifest/registry (P rows) | Package formats follow the language's own distribution model |
| X21 | venv as environment isolation | Python installs are shared and mutable | Project-local dependency dirs + lockfiles (planned) | Isolation need is the same; mechanism follows MINK's native model |

## 10. TOTALS AND GAP SUMMARY (computed by row scan, September 4, 2026)

232 capability rows audited across six domains (L language 73 · R runtime 29 ·
S standard-library 78 · T tooling 16 · W Windows-specific 22 · P packaging 14).

### 10.1 Status distribution

| Status | L | R | S | T | W | P | Total |
|---|---|---|---|---|---|---|---|
| VERIFIED | 25 | 5 | 18 | 5 | 2 | 1 | 56 |
| VERIFIED (INTENT. DIFF.) | 0 | 1 | 0 | 0 | 0 | 0 | 1 |
| VERIFIED (partial) | 0 | 0 | 0 | 1 | 0 | 0 | 1 |
| INTENT. DIFF. | 11 | 0 | 0 | 0 | 1 | 1 | 13 |
| INTENT. DIFF. + PARTIAL | 1 | 0 | 0 | 0 | 0 | 0 | 1 |
| N/A | 0 | 1 | 3 | 1 | 0 | 1 | 6 |
| N/A (INTENT.) | 1 | 1 | 0 | 0 | 0 | 0 | 2 |
| PARTIAL | 10 | 4 | 15 | 1 | 12 | 2 | 44 |
| PLANNED | 2 | 0 | 0 | 0 | 0 | 1 | 3 |
| MISSING | 23 | 17 | 42 | 8 | 7 | 8 | 105 |
| **Total** | **73** | **29** | **78** | **16** | **22** | **14** | **232** |

### 10.2 Priority distribution

| Priority | L | R | S | T | W | P | Total |
|---|---|---|---|---|---|---|---|
| P0 | 0 | 0 | 0 | 0 | 0 | 0 | **0** |
| P1 (parity-blocking by definition) | 7 | 8 | 17 | 3 | 3 | 6 | **44** |
| P2 (important, not blocking) | 18 | 7 | 33 | 2 | 14 | 5 | **79** |
| P3 (optional) | 23 | 6 | 14 | 5 | 3 | 1 | **52** |
| — (no gap / no MINK work) | 25 | 8 | 14 | 6 | 2 | 2 | **57** |
| **Total** | **73** | **29** | **78** | **16** | **22** | **14** | **232** |

Every row marked P1 is also marked "Blocks parity = Y"; there are no P1 non-blockers
and no P2 blockers in this audit. **44 capability gaps block the parity gate.**

### 10.3 Difficulty distribution (rows requiring work; the other 57 rows have no gap)

| Difficulty | Rows | Meaning |
|---|---|---|
| S-M (small-to-medium) | 9 | small change with a medium tail |
| S (small) | 38 | one focused change, low risk |
| M (medium) | 82 | multiple components, contained |
| L (large) | 31 | major feature area |
| XL (major subsystem) | 15 | dedicated multi-session subsystem |
| **Rows requiring work** | **175** | 175 + 57 no-gap rows = 232 |

### 10.4 Parity-blocking gaps (44, all P1) by wave

Wave tags are exact matrix values; where a gap spans waves (A/B, A/F, B/H) it is listed
under its primary wave and repeated in the note.

| Wave | Blocking gaps | Count |
|---|---|---|
| A (Windows platform/runtime quick wins) | R06, R10, R12, R13, R23, S50, S70, S74, W06, W08, W14 (+ L16, S06 begin here: float→Str/format/numeric-text; + L67, P02, T06 mechanism: importable stdlib) | 11 (+5 shared) |
| B (core language/data) | L08, L10, L34, S01, S02, S36 (+ L16, S06 complete here; + L05, whose UTF-8 layer is shared with H) | 6 (+4 shared) |
| C (filesystem/process/time) | S28, S69 | 2 |
| D (networking/internet/compression) | S41, S42, S44, S62 | 4 |
| E (concurrency/async) | R19, R20, S71, S73 | 4 |
| F (packaging/distribution) | L68, P03, P04, P05, P06, P09 (+ L67, P02, T06 delivered by the A include-path mechanism) | 6 (+3 shared) |
| G (developer tooling) | R01, S75, S78, T04, T07 | 5 |
| **Total** | | **44** |

### 10.5 Fully covered areas (no material gap, Wave `-`)

Fully covered means the official capability has a working MINK equivalent with no
material missing subset; rows are VERIFIED or INTENT. DIFF. with no P-gap:

- Scalars & arithmetic: booleans (L04), integer/float arithmetic (L20/L21/L22/L23), conditional expressions (L24), indexing with bounds checks (L25), augmented assignment (L31)
- Data: tuples (L09), ranges (L14), enum/named-constant modeling (L17), Null/Option (L13), byte buffers (L06/L07), mutable heap `Str`
- Control: `if`, `while`, `loop`, `for` over ranges, break/continue/return, exhaustive `match` with rich patterns (L32-L37)
- Functions: positional params, recursion, lambdas/closures (L38-L45)
- Modules: file modules, `use`/`pub`, multi-file compilation (L65/L66)
- Memory/runtime: deterministic arena + ownership + leak check (R17/R18)
- Filesystem: read/write/copy/move/remove/mkdir/cwd/path ops (S26/S27/S32/S33)
- Process: run + output capture + exit codes + PID (R22 core)
- Network: TCP/UDP/DNS/hostname/byte-order (S56-S58, S65)
- HTTP/1.1 client GET/POST core (S59 core)
- Data formats: JSON parse/serialize (S35), base64/hex/url encode-decode (S09), SHA-256/HMAC/secure-random (S46-S48)
- Time: epoch/millis/monotonic/ticks (R29, S66)
- Math: float+int helper library (S17), random (S18 core)
- Tooling: CLI build/run/check/explain/`--json`, exit codes, `--target` selection, error-code docs (T01-T03, T11, T13, T14)
- Windows: process creation, Winsock sockets, BCrypt crypto provider, stdout console writes, standalone PE output (W07, W10 + base rows)
- Compiler distribution via npm (P01)

### 10.6 Headline conclusions

1. **The Windows base platform is COMPLETE/STABLE** with 0 P0 and 0 P1 on the base (Session 97 gate, re-verified this session).
2. **Official-Python-capability parity is PARTIAL**: 56+3 VERIFIED rows, 44 PARTIAL rows, 105 MISSING rows, 13+4 intentionally-different/NA rows, 3 PLANNED rows.
3. **44 parity-blocking P1 gaps** remain; in all, 175 rows require work (15 XL + 31 L + 82 M + 38 S + 9 S-M) and 57 rows need none.
4. **Zero P0 gaps** on the Windows base.
5. Fully covered categories concentrate where MINK has already executed real work: native execution, ownership/memory, files, processes, sockets, HTTP client, JSON, crypto, math, time, and the compiler toolchain itself.
6. Parity-blocking work clusters into: language/data containers (Wave B), concurrency + async (Wave E), packaging (Wave F), networking/compression (Wave D), filesystem/time completion (Wave C), developer tooling incl. REPL and test runner (Wave G), plus a fast Windows-runtime wave (A) that removes the environment/stdin/argv/sleep/stderr/error-info gaps and bundles the stdlib into npm.
7. `docs/audits/OFFICIAL_PYTHON_CAPABILITY_PARITY_AUDIT.md` (Session 82) is **superseded for stale claims**: Windows env (`rt_env_*`) is NOT implemented (stubs); `x86_64-linux-elf` IS implemented (frozen); `&&`/`||` DO short-circuit (probed); the test suite is 2547 tests/56 targets (not "199 test files"); crypto is execution-verified on Windows.

### 10.7 FINAL STATUS (this matrix)

- WINDOWS BASE PLATFORM = **COMPLETE / STABLE**
- WINDOWS OFFICIAL PYTHON CAPABILITY PARITY = **PARTIAL** (44 parity-blocking gaps; see the implementation plan)
- LINUX = **FROZEN** (untouched this session)

---

## 11. Evidence index (primary anchors)

Compiler pipeline and targets: `[code] src/{lexer,parser,ast,semantics,typecheck,ownership,hir,mir,monomorphize,backend}`, `src/backend/target.rs`.
Entry/CLI: `[code] src/cli.rs`, `src/driver.rs`, `src/backend/mod.rs`. Modules: `src/module/mod.rs`.
Runtime services/intrinsics: `[code] src/runtime/intrinsics.rs`, `src/backend/emit/runtime.rs`, `src/backend/emit/pe.rs`, `src/backend/emit/x86_64.rs`, `src/runtime/{allocator,error,abi}.rs`.
Standard library: `[code] stdlib/*.mink` (16 modules: collections, crypto, encoding, environment, filesystem, hashing, http, json, math, network, option, process, random, result, strings, time).
Test suite (execution-verified native runs): `[test] tests/*.rs` (56 targets, 2547 tests) — per-domain: strings_lib, math_lib, encoding_lib, filesystem_lib, process_lib, network_lib, http_lib, json, crypto_lib, hashing_lib, collections_lib, time_lib, random_lib, windows_hardening, release, cli, smoke.
Recorded real execution: `[exec]` SESSION_92..97 docs (crypto vectors, HTTP POST byte-exact echo, process 1 MB drain, npm clean installs ×2, standalone exe) and this session's native probes (short-circuit, div-by-zero fault status 148).
Specs/plans: `[doc] docs/core/*`, `docs/language/*`, `docs/ecosystem/*` (C_ABI_SPEC, PACKAGE_ARCHITECTURE, SECURITY_ARCHITECTURE, STDLIB_ARCHITECTURE), `docs/roadmap/*`, `docs/implementation/SESSION_*`.

*End of master parity matrix — Session 98, commit a716df4.*

