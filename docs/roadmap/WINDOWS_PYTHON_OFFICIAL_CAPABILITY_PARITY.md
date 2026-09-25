# MINK — Windows × Official Python Capability Parity Matrix (Master Audit)

**Session:** 118 (final Windows completion audit; matrix reconciled through Sessions 116–117, and §2/§3.10/§10/§11 recomputed **from the 232 rows** rather than from prose totals)
**Starting commit:** `568dc04` (Session 106 close) → `48ed337` (Session 107 start) → `c3b8091` (Session 107 push) → `9abd6f9` (Session 116 close) → `86642b4` (Session 117 close; final-audit start)
**MINK version:** 1.0.2
**Scope:** Windows x86_64 (the shipped platform). Official Python capabilities only.
**Classification:** capability parity — what a Windows developer can accomplish with
official Python must be accomplishable with MINK's own architecture. Syntax imitation is
explicitly out of scope. PyPI / third-party ecosystem parity is out of scope.
**Date:** September 22, 2026
**Auditor:** Buffy (Codebuff)

> **Session 107 reconciliation note.** Earlier sessions flipped individual rows to
> VERIFIED/EXECUTION VERIFIED but did not re-derive the aggregate counts, and some rows
> kept `P1`/`Blocks = Y` after their own evidence showed the capability delivered. Section
> 10's totals are now computed **from the rows** (see the per-row anchors), and the rows
> whose flags contradicted their own evidence were corrected with the evidence named
> inline. The session's own tranche (L05) is recorded with native execution evidence.

> **Documentation audit note (refreshed by the systems-readiness audit).** This matrix remains the
> durable, evidence-based map of Windows × official-Python capability parity. The current
> full regression is **2 957 passed · 3 ignored · 0 failed** (Session 119 recorded 2 946;
> eleven permanent regression tests have landed since — six from the docs reconciliation,
> five from the systems-readiness audit). The §2 baseline row and the §11
> evidence index carry the same figure. Windows x86_64 is the completed, frozen baseline
> (P0 = 0, P1 = 0) and Linux remains FROZEN. The public npm registry serves
> **`@p4inz-code/mink@1.0.2`** as `latest`; the published tarball's compiler binary and
> standard library are byte-identical to the copies bundled at `npm/mink/`.

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
| Starting commit | `86642b4` (Session 117 close), clean tree | `git rev-parse HEAD`, `git status` |
| Version | `mink 1.0.2` | `target/release/mink.exe --version` |
| Release build | clean | `cargo build --release` |
| Test suite | 72 test units (71 integration targets + lib); current regression: **2 957 passed, 3 ignored, 0 failures** across every unit (Session 119 recorded 2 946; six regression tests added by the docs reconciliation, five by the systems-readiness audit) | `cargo test` per target group + repeated stress runs; see the Session 119 report |
| Smoke/CLI/release suites | smoke 13/13 · release 68/68 · cli 74/74 (+1 ignored) | `cargo test --test {smoke,release,cli}` |
| `windows_hardening` | 18/18 in **both** parallel and `--test-threads=1` modes, deterministically (Session 118 harness fix; was parallel-hanging before it) | `cargo test --test windows_hardening` × 6 parallel runs |
| Windows target | `x86_64-windows-pe` implemented | `[code] src/backend/target.rs` |
| Linux target | `x86_64-linux-elf` implemented but FROZEN by policy; not part of this audit | `[code] src/backend/target.rs`; session policy |
| Architecture | AOT compiler, no external toolchain, zero Rust crate deps, standalone PE | `[code] Cargo.toml`, `src/backend/emit/*.rs` |
| Distribution | npm `@p4inz-code/mink` 1.0.2 ships compiler `bin/mink.exe` + bundled `stdlib/` (all **26** `.mink` modules, `tls` included); `mod` resolves via `<exe>/../stdlib` with no config | `[code] npm/mink/package.json`, `src/driver.rs` resolve_module_path; `[exec]` Session 118 packaged-CLI audit (tarball packed and installed in a clean external directory whose path contains spaces and Unicode; bundled `mod` import, project init, dependency install and environment workflow all run from there) |
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
| L05 | Strings as Unicode text (`str`) | INTENT. DIFF. + PARTIAL | `Str` = length-prefixed byte buffer, heap-owned or immutable image literal; **UTF-8 code-point layer** in `stdlib/encoding.mink`: `utf8_validate` (strict — rejects overlong C0/C1, 3-byte overlong, UTF-16 surrogates, > U+10FFFF, stray/truncated bytes), `utf8_decode`, `utf8_encode`, `utf8_char_count`, `utf8_char_at`, `utf8_byte_index`, `utf8_slice` (`[code] stdlib/encoding.mink`, `npm/mink/stdlib/encoding.mink`; `[test] tests/encoding_lib.rs` 18 strict exit-0 s107_utf8_* tests; `[exec]` Session 107 native PE probes: valid 2/3/4-byte, overlong, surrogate and boundary validation, decode/encode round trip, code-point indexing/slicing, leak-free on heap input) | No non-ASCII case ops (`str_to_upper`/`str_to_lower` are ASCII) and no Unicode DB (categories/normalization/collation) — the latter is S03 (P3); the native `rt_str_len` remains a byte length by design | P2 | M | N | B/H | **P1 blocker CLOSED in Session 107**: validation, decoding, encoding and code-point iteration/length/slicing now exist and are native-verified. Two latent defects fixed on the way: `utf8_validate`/`utf8_char_count` leaked their heap `Str` argument (E-R06), and `utf8_validate` accepted overlong 2-byte forms and surrogates. Byte-oriented storage remains the native model (INTENT. DIFF.) |
| L06 | Bytes (`bytes`, immutable) | INTENT. DIFF. | Byte model IS the native string model; image literals immutable | None material | - | - | N | - | Binary data is the default, not a special case |
| L07 | Bytearray (mutable bytes) | INTENT. DIFF. | Heap `Str` mutable via `rt_str_set_byte` (bounds-checked E-R09) | None material | - | - | N | - | Mutable byte buffers exist |
| L08 | Lists (`list` of arbitrary objects) | EXECUTION VERIFIED | `Vec<T>` generic type identity (`[code] src/typecheck/ty.rs`); dynamic ops via `rt_vec_*` (`[code] stdlib/collections.mink`); runtime ops on multi-word element types now supported (Str, Float, struct, multi-struct, enum) — generic result typing via `rt_vec_get`/`rt_vec_pop` hidden return slot with chunked layout; element ownership/lifecycle verified for Vec<Str>, Vec<struct>, Vec<struct-with-Str>, Vec<Float> (`[code] src/runtime/intrinsics.rs src/backend/emit/runtime.rs src/backend/ir.rs`; `[test] tests/collections_lib.rs` s107_chunked_return_slot_struct_fields, s107_vec_remove_r8_clobber_no_double_free, s107_vec_str_lifecycle, s107_map_keys_values_set_elements_arity; `[exec]` Session 107 native probes: probe_vecstr, vecstr_growth, vecstr_ownership, vec_typed, vec_cross, vec_struct_only, vec_struct_heap, vec_struct_get, vec_struct_remove, vec_struct_combo, vec_struct_str, vec_float_push, dict_with_vec, map_of_vec, stress_vec_multiple_types, stress_vec_regression, stress_small_ints) | Multi-word return-slot memcpy was writing in linear order but the hidden slot uses chunked layout — FIXED (Session 107); VecRemove shift-loop bound (R8) was clobbered by inner memcpy reload — FIXED (Session 107, latent double-free for elem_size ≥ 9 and len ≥ 9); Vec<Str>/<Float>/<struct> element push/insert/shift/get/pop/remove ownership now verified across types; Vec<Float> comparison path has a pre-existing unreported live-block leak (not a Session 107 regression; not product-parity-blocking for Vec itself); no slicing, no append/extend convenience, no filter/map (P3) | P2 | L | N | B | Ownership-aware generic dynamic collections (strings/aggregates in a Vec) is the real design task — now DEMONSTRATED WORKING for Str/Float/struct/multi-struct; Float comparison leak is a separate pre-existing item to triage. Session 107: stale `P1` cleared (the row is execution-verified; remaining items are P3 ergonomics) |
| L09 | Tuples | VERIFIED | Heterogeneous fixed tuples, field access `.0`, destructuring in let/match (`[test] tests/tuples.rs`, tuple_destructure) | None | - | - | N | - | Named tuples: P3 sugar (see S09) |
| L10 | Dictionaries (`dict`) | VERIFIED | `Map<Int,T>` runtime type; `rt_map_new/insert/get/has/remove/len/free`; open-addressing hash table with probing; growth/rebuild verified; string keys via literal and heap; collision chains verified; ownership/leak-free verified (`[code] src/runtime/intrinsics.rs`, `src/backend/emit/runtime.rs`; `[test] tests/collections_lib.rs` s106_map_*; `[exec]` Session 106 probes: map_basic, map_growth, map_ownership, map_strkeys, map_collisions, stress_collections) | No dict literal syntax (function-based `rt_map_new`); no hash-ordered iteration; `Map<Int,T>` only in V1 (generic value types limited to word-sized) | P2 | M | N | B | Root-cause fixes in Session 106: occupied-flag rewrite in rebuild, map_get probe-advance fix, stale-bucket clearing on allocator reuse |
| L11 | Sets (`set`) | VERIFIED | `Set<T>` runtime type; `rt_set_new/insert/has/remove/len/free`; same hash-table architecture as Map; growth/rebuild/collision/ownership verified (`[code] src/runtime/intrinsics.rs`, `src/backend/emit/runtime.rs`; `[test] tests/collections_lib.rs` s106_set_*; `[exec]` Session 106 probes: set_basic, set_growth, set_growth2, set_ownership, set_strvals, set_collisions, stress_collections) | Set<T> element type limited to word-sized in V1 (same constraint as Map keys) | P2 | M | N | B | Same root-cause fixes as Map in Session 106 |
| L12 | Frozen sets | VERIFIED | Use `Set<T>` and do not mutate; ownership makes mutation explicit | None (convention over enforced immutability) | P3 | S | N | B | MINK's ownership model means a set not passed as `&mut` is effectively frozen |
| L13 | `None` / nullability | VERIFIED | `Null` type + `Option<T>` (`[code] stdlib/option.mink`), `?` operator | None | - | - | N | - | Stronger than Python: exhaustive `match` on `Option` |
| L14 | Ranges | VERIFIED | `a..b`, `a..=b`; `for i in 1..=10` (`[test] tests/loop_expressions.rs`) | No step forms (`range(a,b,s)`); P3 sugar | P3 | S | N | B | |
| L15 | Slicing `s[a:b:c]` | PARTIAL | `str_sub` for strings (`[code] stdlib/strings.mink`); no slice views of arrays/Vec, no step | Array/container slicing absent | P2 | M | N | C | Function form exists for Str; slice *views* need reference+length types later |
| L16 | Strings interpolation / f-strings / format spec | PARTIAL | `rt_str_from_float` (exact dtoa, same path as `rt_print_float`) + `rt_str_format` (up to 3 `{}` substitutions, `{{`/`}}` escapes, missing args dropped) (`[code] src/runtime/intrinsics.rs`; `[test] tests/session99.rs`; `[exec]` env_report example) | No Python-style format specs (`:02d`, `.2f`), no f-string syntax | P2 | M | N | - | Minimal MINK-native formatting contract is INTENTIONALLY DIFFERENT from Python's format language |
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
| L34 | Exceptions (`raise`/`try`/`except`) | INTENT. DIFF. | `Option<T>`/`Result<T,E>` + `?` operator for recoverable errors (`[code] stdlib/result.mink`, `[test] tests/option_result.rs`, try_operator) | Runtime faults (bounds, free misuse, div-by-zero) terminate with E-R codes; no catch-and-resume | P2 | L | N | B | Reclassified P1→P2 Session 111: MINK's value-based error model (`Result<T,E>`/`Option<T>`) is the intentional design (INTENT. DIFF.) — same pattern as L01/L06/L07/L18/L27/L53. Parity item (catchable errors with location info) is covered by R06 (VERIFIED). The `?` operator and pattern matching on generic enums are V1 sugar limitations, not P1 parity gaps |
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
| L67 | Import search paths (stdlib reusable without copying) | VERIFIED | `mod name;` resolves in order: source dir sibling → bundled `stdlib` next to the installed `mink` exe → `cwd/stdlib` (`[code] src/driver.rs` resolve_module_path); npm ships `stdlib/*.mink` | None for V1 | - | - | N | - | Clean-install proof: `mod math;` program built via installed CLI from a separate cwd with no config (`[exec]` Session 99) |
| L68 | Packages (directories / `__init__`) | VERIFIED | A directory package resolves from `mod name;` to `name/mod.mink` (the package root — MINK's `__init__` equivalent); the package's own `mod sub;` declarations resolve inside the package directory, and nested packages (`name/sub/mod.mink`) compose; a flat `name.mink` wins when both exist (`[code] src/driver.rs` `resolve_module_path` + declared-name discovery; `[test] tests/packages.rs` l68_*; fixtures under `tests/packages/`; `[exec]` native PE runs) | No manifest/registry/version model (that is P05-P08); discovery is filesystem-based | - | M | N | F | Delivered this session. Closing it exposed and fixed a pre-existing multi-module defect: declaration lookups were keyed by byte offset alone, so two modules whose names shared an offset collapsed onto one symbol (see P03) |
| L69 | Relative imports / aliases | PARTIAL | file-relative `mod` only | `use a::b` style nesting within project dirs is limited | P2 | M | N | F | |
| L70 | Circular import handling | PARTIAL | discovery visits each file once (cycle-safe) (`[code] src/driver.rs` visited set) | No explicit cycle diagnostic | P3 | S | N | - | |

### 3.9 Introspection

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| L71 | Compile-time type inspection / diagnostics | VERIFIED | typed checker, error codes E-T*/E-S*, `mink explain`, `--json` report (`[code] src/cli.rs`, diagnostics) | None | - | - | N | - | |
| L72 | Runtime reflection (`type()`, `dir()`, `getattr()`) | INTENT. DIFF. | none; static typing removes the need | Future: compile-time reflection / macros | P3 | XL | N | G | Needs a REPL/tooling context to be valuable |
| L73 | Callable/object metadata | MISSING | none | doc/metadata queries | P3 | M | N | G | |

### 3.10 Language section totals

73 rows: EXECUTION VERIFIED 1 · VERIFIED 30 · INTENT. DIFF. 11 · INTENT. DIFF. + PARTIAL 1 · PARTIAL 9 · PLANNED 2 · N/A (INTENT.) 1 · MISSING 18. Priorities: P1 0 · P2 23 · P3 23 · no-gap 27. **Parity blockers (Blocks = Y): 0.**

*Session 118 correction:* this line still carried the Session 107-era totals and named **L34 (exceptions) and L68 (packages) as live `Blocks = Y` blockers**, contradicting both the rows and §10: L34 is INTENT. DIFF./P2/N (reclassified in Session 111) and L68 is VERIFIED with `Blocks = N` (Session 109). The figures above are recomputed directly from the 73 language rows by the Session 118 scan (see §10). L05 closed in Session 107, L08 in Session 106/107, and the remaining L-row items are P2/P3.

---

## 4. RUNTIME / execution capability matrix (official Python runtime UX → MINK)

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| R01 | Interactive interpreter / REPL | VERIFIED | `mink repl [path]` runs an interactive compile-eval session: declarations (`fn`/`struct`/`enum`/`use`/`mod`/`const`) accumulate, bare expressions and statements are compiled and executed through `driver::build` against the accumulated declarations, with `:help`/`:show`/`:clear`/`:quit` (`[code] src/cli.rs` `run_repl`/`repl_eval`; `[test] tests/repl.rs` r01-r14: startup, EOF exit, bare int/string expressions, declaration+call, multiline declarations, compile-error survival, help/clear/show, unknown command, initial-file load, 10x repeated evaluation; `[exec]` native PE runs) | Bare expressions print via `rt_print_int` then `rt_print_str` (no runtime reflection); each evaluation is a full native build+run cycle, so no value bindings persist between lines (declarations only) | - | L | N | G | Delivered this session. Interactive execution in MINK's own architecture: a compile-eval loop on the native backend, not a bytecode/interpreter trampoline |
| R02 | Script execution | VERIFIED | `mink run file.mink` compiles + runs + forwards exit code (`[code] src/cli.rs`) | None | - | - | N | - | |
| R03 | Module execution (`python -m pkg`) | MISSING | `mink run <file>` only | Package/module runner | P3 | M | N | F | Comes with package system |
| R04 | Compiled standalone program | VERIFIED | `mink build` → zero-dependency PE (`[exec]` Session 97: standalone exe outside repo) | None — MINK is stronger (Python needs an interpreter + packaging to ship) | - | - | N | - | |
| R05 | Import caching / bytecode cache | N/A | AOT compilation; no cache concept | — | - | - | N | - | |
| R06 | Traceback / error reporting w/ location | VERIFIED | compile-time errors carry file:line:col; runtime faults now print `mink: runtime error[E-Rxx]: msg (file:line)` — per-function fail-site location table embedded in the image (BSS `fail_loc` cell + patch-time span resolution; `[code] src/backend/emit/runtime.rs`, `src/backend/emit/x86_64.rs`, `src/runtime/abi.rs`) | Runtime faults carry no call stack (V1 contract: location yes, stack no) | P2 | M | N | A | IMPLEMENTED Session 100: `[test] tests/session100.rs` (9 tests incl. exact file/line), `[exec] examples/runtime_error_report/main.mink` (E-R10 at main.mink:20, exit 110); determinism tests pin same-path builds (image embeds path as given). Session 107: the row was already VERIFIED; the stale `P1`/`Blocks = Y` flags are cleared (the remaining item is the P3 call-stack extension) |
| R07 | Warnings (`warnings` module) | MISSING | none | Warning channel | P3 | S | N | - | |
| R08 | stdout output | VERIFIED | `rt_print_str/int/float/char` (+CRLF), write thunks via kernel32 (`[code] src/runtime/intrinsics.rs`) | None | - | - | N | - | Python `print()` equivalent |
| R09 | User-facing stderr write | VERIFIED | `rt_stderr_write(Str) -> Int` writes exact bytes to stderr (`[code] src/runtime/intrinsics.rs`; `[test] tests/session99.rs` stderr_write_is_separate_from_stdout; `[exec]` env_report example) | Writes are synchronous; stdout/stderr interleaving order not guaranteed | P3 | S | N | - | |
| R10 | stdin input | VERIFIED | `rt_stdin_read() -> Str` reads all piped input to EOF (owned string; empty on no input) (`[code] src/runtime/intrinsics.rs`; `[test] tests/session99.rs`; `[exec]` env_report example with piped stdin) | Read-all only; no line-by-line streaming, no binary reads | P2 | M | N | - | Text/binary boundary documented in SESSION_99 record |
| R11 | Exit codes | VERIFIED | `main` return → exit code; `rt_exit(code)` w/ leak check (`[exec]` 2547-test suite asserts codes) | None | - | - | N | - | |
| R12 | argv (command-line args to programs) | VERIFIED | `rt_argc() -> Int`, `rt_argv(i) -> Str` parse the ANSI command line (order, spaces, empty args preserved; count excludes the exe name) (`[code] src/backend/emit/runtime.rs`; `[test] tests/session99.rs`; `[exec]` env_report example) | ANSI-only command line; Unicode args P2 (W05/H) | P2 | M | N | - | |
| R13 | Environment variables | VERIFIED | `rt_env_get/has/set/remove` wired to GetEnvironmentVariableA/SetEnvironmentVariableA; empty value sets an empty-valued var (deletion only via `rt_env_remove`); free-list reuse regression fixed (ToCstr NUL overrun) (`[code] src/backend/emit/runtime.rs`; `[test] tests/session99.rs` 4 env tests; `[exec]` env_report example) | ANSI-only names/values; Unicode env P2 (W05/H) | P2 | S-M | N | - | |
| R14 | Signals / Ctrl+C handling | MISSING | no console control handler | Graceful interruption | P2 | M | N | H | Windows subset: SetConsoleCtrlHandler for CTRL_C/CLOSE |
| R15 | Platform info (`sys.platform`, `platform`) | MISSING | none | Query OS/arch at runtime | P2 | S | N | A | Trivial intrinsic |
| R16 | System info (mem/disk/cpu/uptime) | MISSING | none | System resource queries | P3 | M | N | H | |
| R17 | Memory management / leak detection | VERIFIED (INTENT. DIFF.) | arena allocator + liveness table + leak check at exit; deterministic errors E-R02..E-R08 (`[code] src/runtime/*`, `src/backend/emit/runtime.rs`) | Python: refcount + GC. MINK: compile-time ownership + explicit free + runtime validation | - | - | N | - | Equivalent safety story, different mechanism (X-register) |
| R18 | GC / automatic reclamation | N/A (INTENT.) | ownership model; no GC by design | — | - | - | N | - | Not a parity requirement |
| R19 | Threads | VERIFIED | `rt_thread_spawn(fn, arg: Ptr<Int>) -> Int`, `rt_thread_join(handle) -> Int`, `rt_thread_id() -> Int`, plus `rt_ptr_to_int`/`rt_int_to_ptr` so a shared-context pointer can be published in a `Ptr<Int>` block; real OS threads via `CreateThread` and a trampoline that bridges the Windows x64 ABI to the MINK stack ABI (`[code] src/backend/emit/runtime.rs`, `src/backend/emit/pe.rs` KERNEL32 imports, `[code] stdlib/threads.mink`; `[test] tests/threads_lib.rs` r19_*: spawn/join result, live per-thread ids, 200 sequential spawn/joins, 8 concurrent threads, parallel allocation integrity, string interaction, pointer-cast round trip, the shipped `threads` stdlib module, and the `examples/threaded_work` proof app run twice; `[exec]` native PE runs) | No thread-local storage, no thread cancellation, no per-thread heaps — every thread shares the runtime arena, which is guarded by a global spin lock | - | XL | N | E | Delivered in Session 112. The thread control block is a MINK-heap allocation owned by `rt_thread_join`, so a spawn without a join is a leak (E-R06) |
| R20 | Async / event loop | VERIFIED | A task is `fn(arg: Int) -> Int` running on its own OS thread, tracked by the process-global task loop: `rt_task_spawn(fn, arg) -> Int` (handle), `rt_task_await(handle) -> Int` (wait, collect, return the result word), `rt_task_run() -> Int` (collect every outstanding task, returning how many), `rt_task_pending() -> Int` and `rt_task_stop()`; `rt_task_spawn0(fn)` is the zero-argument form the language surface uses (`[code] src/backend/emit/runtime.rs` (`emit_task_spawn`/`emit_task_await`/`emit_task_run`, loop struct in `.bss`), `src/backend/ir.rs`, `src/runtime/abi.rs`, `src/runtime/error.rs` (`E-R13`), `[code] stdlib/tasks.mink` + `npm/mink/stdlib/tasks.mink`; `[test] tests/async_lib.rs` r20_* 14 native-PE tests: handle accounting, `task_run` drain, zero tasks, 200 spawn/await cycles, 64 concurrent tasks, `task_stop`, uncollected-task leak (E-R06), double collect (E-R05), await of a non-task allocation (E-R13), concurrency proof (spawner prints before the awaiting task finishes), 4×1000 locked shared increments = exactly 4000, tasks allocating/freeing strings, and 5 identical stress runs; `[exec]` native PE runs) | Tasks carry one word in and one word out (the thread ABI), so string/aggregate results travel as a context pointer; collection must not use two collectors for the same task (the second collect is a stable error, E-R05/E-R13, not a crash); no I/O-readiness integration yet (S63's non-blocking sockets are usable from a task, but there is no readiness-driven wakeup) | - | XL | N | E | Delivered in Session 114. The task control block is a MINK-heap allocation owned by whoever collects it, so a spawn that is never awaited and never drained is a leak (E-R06) rather than a silent leak. The loop list and count are guarded by a test-and-set spin lock, so a task may collect its own children (verified) |
| R21 | Locks / synchronization primitives | VERIFIED | `rt_mutex_new() -> Int` (an 8-byte lock word in the MINK heap), `rt_mutex_lock(word)`, `rt_mutex_unlock(word)`, `rt_mutex_free(word)`, plus the runtime's own global test-and-set spin lock that guards the allocator and the raw memory accessors (`[code] src/backend/emit/runtime.rs`, `src/backend/emit/x86_64.rs` `xchg_r_mem`/`xchg_rip_r`; `[test] tests/threads_lib.rs` s71_*: an occupancy-counter mutual-exclusion probe, 4x5000 locked increments giving exactly 20000 twice, an unlocked control, 200 new/lock/free cycles, and re-acquisition after join; `[exec]` native PE runs) | Non-reentrant (a second lock from the same thread deadlocks, like Python's `Lock`); no condition variables, no owner tracking, no timed acquire | - | L | N | E | Delivered in Session 112 |
| R22 | Subprocess run + capture | PARTIAL | `process_run/process_stdout/stderr/id`; exit codes; large-output drain fixed in Session 97 (`[exec]` S97 1 MB drain) | stdout/stderr capped at 4088 bytes/stream; no stdin, no env override, no explicit argv | P2 | M | N | C | Core capability VERIFIED; parity extras P2 |
| R23 | Sleep / timers | VERIFIED | `rt_sleep(ms)` via kernel32 Sleep; bounded non-spinning wait (`[test] tests/session99.rs` sleep_zero_and_short_returns_cleanly) | No timer/callback API | P3 | S | N | - | |
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
| S01 | String methods (search/replace/trim/case/…) | EXECUTION VERIFIED | `str_*`: cmp, index_of, contains, starts/ends_with, count, sub, trim, upper/lower (ASCII), repeat, pad, reverse, replace(_all), join_2/3, **split** (`[code] stdlib/strings.mink`; `[test] tests/strings_lib.rs`; `[exec]` Session 107 native probes: split_basic, split_empty, split_single delim, split_long, split_stress, join_basic, join_strings, join_repeat, join_empty, join_vec_str, stress_split_join) | Case ops ASCII-only; no partition (P3); no unicode-aware split (L05) | P2 | M | N | B | `split`/`join` are the daily text workhorses (CSV row depends) — split now implemented; join generalized from join_2/3. Session 107: the P1 blocker is closed (remaining items are P3 partition and L05 Unicode-aware split), so the flag is cleared |
| S02 | Regular expressions (`re`) | VERIFIED | `stdlib/re.mink`: a self-contained backtracking engine (no deps) supporting literals, `.`, `^`/`$`, `\d\D\w\W\s\S`, classes with ranges/negation, groups `()`/`(?:)`, alternation `\|`, greedy and lazy `* + ? {m} {m,} {m,n}`, with `re_is_match`, `re_search`, `re_match_end`, `re_full_match`, `re_find`, `re_count`, `re_find_all`, `re_split`, `re_replace`, `re_replace_first` (`[code] stdlib/re.mink`, `npm/mink/stdlib/re.mink`; `[test] tests/re_lib.rs` s02_* 33 tests: leftmost/no-match, empty pattern/text, anchors, full-match, spans, classes/ranges/negation, shorthand classes, quantifiers incl. braces and lazy, alternation/groups, escaped metacharacters, byte semantics over UTF-8, count/find_all/split/replace, malformed patterns, a bounded catastrophic-backtracking probe, 5000-byte input, 200-cycle leak-free ownership run; `[exec]` native PE runs) | No capture-group extraction, no backreferences, no Unicode classes, no inline flags; ASCII/byte semantics (`. ` matches one byte, `\w` is ASCII); matching bounded by a step budget so pathological patterns fail fast | - | XL | N | B | Delivered this session. Architecture mirrors the JSON arena design (pattern + subject pre-read into `Ptr<Int>` arenas) so recursion is not constrained by the V1 Str user-call ownership contract |
| S03 | Unicode data (categories/normalization/casefold) | MISSING | none (byte model) | Full Unicode DB | P3 | L | N | H | Minimal UTF-8 layer is L05 (P2); full DB optional |
| S04 | Codecs / encodings (`codecs`, utf-8/16, latin-1…) | PARTIAL | `encoding.mink`: hex/base64(url)/url + `str_is_ascii` + **UTF-8** `utf8_validate`/`utf8_decode`/`utf8_encode`/`utf8_char_count`/`utf8_char_at`/`utf8_byte_index`/`utf8_slice` (`[code] stdlib/encoding.mink`; `[test] tests/encoding_lib.rs` incl. s107_utf8_*) | UTF-8 is covered (Session 107); no UTF-16/latin-1 or other text codecs | P2 | M | N | H | |
| S05 | Text wrapping / formatting helpers (`textwrap`) | MISSING | none | Wrap/pad paragraph text | P3 | S | N | - | |
| S06 | String parsing: int/float ↔ text | VERIFIED | Formatting: `rt_str_from_int/bool/float` + `rt_str_format` (Session 99). Parsing: `str_parse_int` (sign, ASCII whitespace, `_` digit separators, i64 overflow rejection) and `str_parse_float` (sign, fraction, `e`/`E` exponent with sign) returning `(value, ok)` tuples (`[code] stdlib/strings.mink`, `npm/mink/stdlib/strings.mink`; `[test] tests/strings_lib.rs` p01-p12: basics, whitespace, separators, malformed inputs, i64 max/min boundaries, exponents, saturating extreme exponents, precision, 200-heap-string ownership run; `[exec]` native PE runs) | No `inf`/`nan` keywords and no base-prefixed parsing (`int(s, 16)`); float assembly is arithmetic-based, not bit-exact `strtod`; the `ok` flag replaces Python's ValueError (MINK has no exceptions) | - | M | N | B | Closed in Session 108. Python's `int()`/`float()` raise; MINK returns `(value, ok)` to match its existing tuple-result idiom |

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
| S11 | `collections`: deque/Counter/defaultdict/OrderedDict | PARTIAL | `Map<Int,T>` provides Counter/defaultdict-like functionality; `Set<T>` provides set operations; no deque/OrderedDict | deque, OrderedDict, named-tuple variants | P2 | M | N | B | Map/Set cover the two highest-value named collection types |
| S12 | `queue` | MISSING | none | Thread-safe queues | P2 | M | N | E | With threads |
| S13 | `heapq` | MISSING | none | Heap operations | P2 | S | N | B | Easy over Vec |
| S14 | Typed arrays (`array` module) | PARTIAL | fixed-size `[T; N]` arrays with bounds checks (`[code] src/typecheck/ty.rs`; `[test] tests/aggregate.rs`) | No compact dynamic typed arrays (Vec covers word-sized only) | P2 | M | N | B | |
| S15 | Enum / flag values | VERIFIED | see L17 | None | - | - | N | - | |
| S16 | Linked structures / trees / custom collections | VERIFIED | Map/Set provide hash-based containers; `Ptr<Int>` + alloc/free for custom structures; JSON arena for tree-like data | No stdlib tree/bag/deque types | P2 | M | N | B | Map/Set cover the core hash-based collection need |

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
| S28 | Directory listing / traversal (`os.listdir`, `os.walk`) | VERIFIED | `fs_dir_open`/`fs_dir_next`/`fs_dir_close` streaming enumeration over `rt_dir_open`/`rt_dir_next`/`rt_dir_close` (Win32 `FindFirstFileA`/`FindNextFileA`/`FindClose`); `.`/`..` are skipped so the stream matches `os.scandir()` (`[code] stdlib/filesystem.mink`, `src/backend/emit/runtime.rs`, `src/backend/ir.rs`, `src/runtime/intrinsics.rs`; `[test] tests/filesystem_lib.rs` d01-d11: entry counts, empty dir, missing dir = 0, null handle = inert, trailing separator, spaced path, interleaved handles, 300 open/enumerate/close cycles, one-level walk, 45-entry dir, unclosed handle = E-R06 leak; `[exec]` native PE runs) | No recursion helper (`os.walk`-style) in the stdlib — build it from the streaming trio (proven in d09); no sorted order; ANSI names (shares the FS layer's MAX_PATH/ANSI limits, see W03); no per-entry metadata (S31); no glob (S29) | - | M | N | C | `os.scandir()`/`os.listdir()` parity delivered in Session 108. The handle is an owned allocation, so an unclosed enumeration is reported as a leak like any other MINK allocation |
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
| S36 | CSV | VERIFIED | `stdlib/csv.mink`: `csv_parse_row` (RFC 4180 subset — quoted fields, `""` escapes, CR/LF/CRLF terminators, empty fields, quotes kept literal mid-field), `csv_write_row` (quotes and doubles only when needed), `csv_escape`, `csv_field_count`, `csv_round_trip` (`[code] stdlib/csv.mink`, `npm/mink/stdlib/csv.mink`; `[test] tests/csv_lib.rs` c01-c14: plain/quoted/escaped/empty fields, all terminators, empty input, write quoting, round trip, 100-field row, embedded newline inside quotes, 100 parse/write cycles; `[exec]` native PE runs) | Records are `Vec<Str>` rather than nested lists (MINK has no nested collection in this layer), so a document is parsed record by record; no dialect options (delimiter/quote char), no `DictReader`; no type inference/marshalling | - | M | N | B | Closed in Session 108 — the `str_split` (S01) dependency was already resolved. The write path sizes its buffer from the quoted length, so embedded quotes cannot overflow it (regression covered by c08/c10) |
| S37 | Config files (`configparser`, INI) | MISSING | none | INI config | P2 | S | N | C | |
| S38 | TOML (`tomllib`, read) | MISSING | none | TOML parsing | P3 | M | N | C | |
| S39 | Binary serialization (`pickle`) | N/A | native binary + JSON exist; deterministic formats by design | — | P3 | M | N | - | Pickle is Python-object-graph magic; not a parity requirement |
| S40 | XML / HTML parsing | MISSING | none | XML/HTML processing | P3 | L | N | C | Official xml.etree/html.parser are important in niche; P3 for MINK gate |

### 5.8 Database

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S41 | SQLite (`sqlite3`) | VERIFIED | `[code] stdlib/sqlite.mink` — self-contained embedded SQL engine over a packed word arena: `CREATE TABLE` (INT/STR), `INSERT` (positional and named-column), `SELECT *`/`SELECT COUNT(*)` with `WHERE col OP value` (`= != <> < > <= >=`) and `col IS [NOT] NULL` joined by `AND`/`OR`, `UPDATE`, `DELETE`, `BEGIN`/`COMMIT`/`ROLLBACK`, `sql_tables`/`sql_rows`/`sql_has_table`/`sql_in_transaction`/`sql_quote`/`sql_field`; `[test] tests/sqlite_lib.rs` (16 native-PE tests); `[exec]` `mink run examples/inventory_db/main.mink` native proof application; `[doc] docs/implementation/SESSION_111_WINDOWS_SQLITE.md` | Embedded SQL database with the core `sqlite3` capability; NULL is a distinct one-byte marker so it never collides with the empty string; malformed/hostile SQL is rejected with a stable error string; the only measured limit is the runtime's fixed 1 MiB heap (about 220 four-column rows for an append-only workload, since each statement returns a fresh blob) — documented in the module header, not a leak (clean exit, no E-R06) | - | XL | N | D | Major subsystem closed in Session 111 |

### 5.9 Compression / archives

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S42 | zlib/gzip | VERIFIED | `stdlib/zlib.mink` (bundled at `npm/mink/stdlib/zlib.mink`) — a self-contained DEFLATE codec with raw, zlib and gzip entry points: `deflate_raw`/`inflate_raw`, `zlib_compress`/`zlib_decompress`, `gzip_compress`/`gzip_decompress`, `zlib_crc32`, `zlib_adler32`, over stored, fixed-Huffman and dynamic-Huffman blocks with a 256 KB sliding window and level 0/1/6/9 (`[test] tests/zlib_lib.rs` s42_* 15 tests: empty and tiny inputs, repetitive and binary payloads, all three wrappers round-tripped, CPython-compressed streams inflated, level/wrapper interactions, malformed/truncated/corrupt/bad-checksum/bad-block-type rejection, deterministic repeated use, 400+ cycle ownership run; `[exec]` native PE runs; `[exec]` Session 110 cross-validation: 77 vectors verified bidirectionally against CPython 3.11 `zlib` — CPython inflates MINK output and MINK inflates CPython output, with a byte-identical match for `zlib.compress(b"hello", 6)` and 4500→40 bytes on repetitive input) | No bz2/lzma (S43, P3); no streaming/incremental API and no multi-member gzip concatenation; decompression is bounded by the fixed 4 MiB runtime heap, so payloads needing a larger resident window are out of range by construction; fixed-window LZ77 with no optimal parsing, so ratios trail CPython's on non-repetitive data | - | M | N | D | Session 110. Huffman/LZ77 tables are folded into the input-sized arena so every request is input-sized; residual heap growth is caller-side (non-splitting first-fit allocator), measured and documented in the module and test comments. Root-caused on the way: `_z_fill` never advanced its scan index after a separator (infinite loop in every entry point), and the free order in the compression path stranded a 128 KiB skipped block per call. |
| S43 | bz2 / lzma | MISSING | none | Other codecs | P3 | XL | N | D | After zlib |
| S44 | zip archives | VERIFIED | `stdlib/zip.mink` (bundled at `npm/mink/stdlib/zip.mink`) — a self-contained PKZIP reader/writer over the verified DEFLATE codec (`[code] stdlib/zip.mink`, `npm/mink/stdlib/zip.mink`): archive creation (`zip_new`/`zip_add`/`zip_add_stored`/`zip_add_dir`/`zip_finish`) and reading (`zip_read_count`/`zip_list`/`zip_entry_name`/`zip_entry_data`/`zip_entry_size`/`zip_entry_is_dir`/`zip_entry_data_by_name`), with extraction to disk (`zip_extract_all`/`zip_extract_entry`) including recursive directory creation; CRC-32 verification on decompression; entry capacity and pool doubling for growth; a path-safety predicate rejecting traversal, absolute paths, drive-qualified paths, UNC, NTFS alternate data streams and `..` segments; `[test] tests/zip_lib.rs` s44_* 10 tests: empty archive, single stored entry, deflate+stored+dir multi-entry, entry data round trip, 20-cycle leak-free build loop, deterministic golden bytes, path-safety predicates, empty payload, binary payload, 30-entry capacity-growth stress; `[exec]` native PE runs; `[exec]` Session 111 CPython 3.11 `zipfile` interop: 87 cross-validation checks pass — MINK-written archive read by CPython (7 entries, all names/contents/methods correct), CPython-written archive read by MINK (5 entries, names/lengths/CRC-32 match), MINK extracts CPython archive (all bytes match), all 10 hostile archive names rejected with -4 (traversal, absolute, drive, UNC, ADS, `..`), all 4 malformed inputs rejected, leak-free throughout | No bz2/lzma (S43, P3); no per-entry timestamps preserved (written as fixed DOS epoch 1980-01-01); non-ASCII entry *names* on extracted files are reinterpreted through the ANSI Win32 API (documented MINK limitation, matrix W22 family); no multi-disk, no ZIP64, no data descriptor streaming; the archive byte format is ZIP 2.0 (MS-DOS version) | - | L | N | D | Session 111. Two growth-copy defects were fixed in `_zp_grow` (records overwritten by pool copy due to an incorrect base calculation). Also closed during this session: a multi-module P0 defect where expression types and name resolutions were keyed by byte offset only, so any two-module program could mis-type expressions — fixed in `src/typecheck/mod.rs` (`expr_type`/`expr_type_exact` sorted/looked up by `(file, start)`) and `src/semantics/mod.rs` (`resolve`/`resolutions`/`binding_aliases`/`errors` keyed by `(file, start)`). |
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
| S50 | Environment (os.environ) | VERIFIED | see R13 — real get/set/has/remove on Windows | ANSI-only | P2 | S-M | N | - | |
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
| S59 | HTTP client | PARTIAL | HTTP/1.1 GET/POST, header/status/body parse, url split helpers (`[code] stdlib/http.mink`; `[test] tests/http_lib.rs`; `[exec]` Session 96 byte-exact POST echo); HTTPS GET now exists via S62's `stdlib/tls.mink` | No redirects, no cookies, no chunked transfer, no keep-alive, no POST-over-TLS helper | P2 | L | N | D | The client capability is real; protocol conveniences P2; TLS/HTTPS delivered by S62 (Session 117) |
| S60 | HTTP server | PARTIAL | socket-level server verified (accept/echo repeated connections); no HTTP parsing server-side lib | `http.server`-style convenience | P2 | M | N | D | |
| S61 | URL parsing (`urllib.parse`) | PARTIAL | url encode/decode in encoding.mink; host/port/path split in http.mink | Full URL grammar handling | P2 | M | N | D | |
| S62 | TLS/SSL | VERIFIED | A TLS 1.2/1.3 client and an HTTPS client on Windows Schannel, written in MINK source over a new general FFI layer. `rt_sys_load_lib`/`rt_sys_get_proc`/`rt_sys_call` (a Win64 call with up to 12 word arguments and an explicitly 16-byte-aligned stack) plus the unvalidated foreign reads `rt_sys_load64`/`rt_sys_load32` reach the OS crypto stack (`[code] src/backend/emit/runtime.rs`, `src/backend/ir.rs`, `src/runtime/intrinsics.rs`); `rt_net_getaddrinfo` now really resolves with Winsock `getaddrinfo` (it was a V1 pass-through), so a name connects as its dotted quad while the original name stays available for checking. Trust is never skipped: Schannel runs manual credential validation and MINK makes the trust decision itself with crypt32 — chain build (`CertGetCertificateChain`) + `CERT_CHAIN_POLICY_SSL` (serverAuth EKU, certificate time validity, matching host name) + anchor match by SHA-1 thumbprint against the Windows system store or a pinned PEM CA (`[code] stdlib/tls.mink` + npm mirror: `tls_connect`/`tls_status`/`tls_err`/`tls_cipher`/`tls_send`/`tls_recv`/`tls_close`, `https_client_get(_with)`, error codes 1001-1008). A measured platform defect: any `CertGetCertificateChain` call makes crypt32 deadlock in its DLL-detach handler, so `rt_exit`/`rt_fail` now terminate through `kernel32!TerminateProcess` after the leak scan (no return through foreign detach handlers; `[code] src/backend/emit/runtime.rs`). `[test] tests/tls_lib.rs` 20 tests — 19 native-PE plus an opt-in public-endpoint one: pinned-CA handshake, HTTPS GET in both forms, SAN matching (second entry, wildcard, IP literal), expired certificate, wrong extended key usage, untrusted root, system store does not accept a private CA, wrong host name, connection refused, missing/malformed CA, malformed URL, invalid/closed handle operations, binary echo with partial reads, 300 KB transfer, repeated connections with no leak; deterministic local server `tests/tls/server.py` with committed fixtures from `tests/tls/make_certs.py`; `[exec]` every test compiles and runs a real Windows PE through the CLI | Client only (no TLS server); no client certificates; no ALPN/HTTP-2, redirects, cookies or keep-alive; `https_client_get*` does GET only (POST over TLS has no helper yet); TLS 1.0/1.1 not targeted | - | XL | N | D | Delivered in Session 117. The FFI layer is general (any DLL entry point, any 12-word signature) and is the mechanism a future Windows service binding can reuse; it is deliberately raw — the caller owns pointer validity, which is why it is a stdlib building block rather than a user-facing convention |
| S63 | Non-blocking I/O / select | VERIFIED | `net_set_nonblocking`, `net_poll` (over `WSAPoll`), `net_wait_readable`, `net_wait_writable`, `net_can_read`, `net_can_write`; poll entries are 16-byte `WSAPOLLFD` records mapped to a `Ptr<Int>` array (`[code] stdlib/network.mink`, `src/backend/emit/runtime.rs`; `[test] tests/network_lib.rs`; `[exec]` native PE probes: poll timeout, readiness after connect/accept, non-blocking accept/recv with `net_would_block`) | Synchronous fallback only (no `select`/`poll` in the Python stdlib sense) | - | M | N | E | Delivered in Session 113 — prerequisite for R20 |
| S64 | Email / FTP / SMTP / IMAP | MISSING | none | Protocol clients | P3 | L | N | D | Not parity-critical for the declared gate |
| S65 | Hostname / byte order | VERIFIED | `net_hostname`, `net_htons/ntohs` | None | - | - | N | - | |

### 5.13 Time

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S66 | Epoch/time/monotonic | VERIFIED | see R29 | None | - | - | N | - | |
| S67 | Date/time decomposition + arithmetic | PARTIAL | year/month/day/hour/min/sec/weekday, leap-year, days-in-month, diff/add (`[code] stdlib/time.mink`) | No date structs/parsing, no timedelta | P2 | M | N | C | |
| S68 | Timezone | MISSING | UTC epoch only | TZ handling, DST | P2 | M | N | C | Windows TZ API exists |
| S69 | Formatting / parsing (`strftime`/`strptime`) | VERIFIED | `stdlib/time.mink`: `time_strftime(ts, fmt)` renders any strftime-style pattern (year/month/day/hour/min/sec/weekday/month names + %F/%T/%%); `time_strptime(s, fmt)` parses numeric directives + %F/%T/%% back to epoch seconds with full validation; `time_epoch(y,mo,d,h,mi,s)` for direct epoch construction (`[code] stdlib/time.mink`, `npm/mink/stdlib/time.mink`; `[test] tests/time_lib.rs` s69_strftime_y_m_d..s69_ownership_repeated_strftime: 13 tests covering %Y-%m-%d, %H:%M:%S, %F, %T, epoch zero, round-trip parse+format, invalid input rejection, weekday names (%A/%a), month names (%B/%b), literal %%), ownership 200-call run; `[exec]` native PE runs | Name directives (%A/%a/%B/%b) formatting-only (no parse-back), no locale support | - | M | N | C | Delivered in Session 108 |
| S70 | Sleep / timers | VERIFIED | see R23 — `rt_sleep(ms)` | None | - | - | N | - | |

### 5.14 Concurrency / async

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S71 | Threads + locks | VERIFIED | see R19 (threads) and R21 (locks): `stdlib/threads.mink` ships `thread_spawn`/`thread_join`/`thread_self` and `lock_new`/`lock_acquire`/`lock_release`/`lock_free` over the verified runtime services (`[code] stdlib/threads.mink`, `npm/mink/stdlib/threads.mink`; `[test] tests/threads_lib.rs` r19_stdlib_threads_and_locks_module + s71_*; `[exec]` `examples/threaded_work/main.mink`: 4 threads, lock-guarded accumulation, closed-form check, deterministic across runs) | See the R19/R21 gaps (no thread-local storage, no cancellation, non-reentrant lock, no condition variables) | - | XL | N | E | Delivered in Session 112 |
| S72 | Futures / thread pools | MISSING | none | see R24 | P2 | L | N | E | |
| S73 | async/await + event loop | VERIFIED | The language surface over the verified R20 runtime: `async fn name(args) -> T { body }` declares the task body and an ordinary handle producer (`name(args) -> Int`), and `await expr` collects a handle (lowering to `rt_task_await`). The desugar is a front-end transformation (`src/parser/mod.rs`: `parse_async_fn`, `await` prefix in `parse_unary`, `pub`-qualified async fns via `parse_pub_items`) over the same call/ownership/codegen path, so awaits compose with loops, branches and module boundaries (`[code] src/parser/mod.rs`, `src/parser/error.rs` (`E-P30`/`E-P31`/`E-P32`), `[code] stdlib/tasks.mink`; `[test] tests/async_lib.rs` s73_* 8 native-PE tests: direct-call and handle awaits, zero-parameter async fn, three distinct async fns awaited out of spawn order, nested async (a task awaiting its own child), awaits inside loops and branches, cross-module async (pub and non-pub), and the two rejected forms (two parameters E-P31, `async` without `fn` E-P30); `[exec]` `examples/async_tasks/main.mink` — four async workers over 0..1000, lock-guarded accumulation, closed-form check, identical output across runs) | An async fn takes zero or one parameter and yields one word (the task ABI); results wider than a word travel through a context pointer. `async` is a declaration modifier (no async closures/lambdas). Awaits must be collected by one owning thread per task | - | XL | N | E | Delivered in Session 114 with R20. The task body really runs on its own OS thread — there is no synchronous stand-in; `await` blocks only the awaiting thread, so the spawner's own work and every other task keep making progress (asserted by `r20_tasks_run_concurrently_with_the_spawner`) |

### 5.15 Development support (logging/testing/assert/debug) — stdlib half

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| S74 | Logging | VERIFIED | `stdlib/logging.mink`: levels DEBUG/INFO/WARNING/ERROR/CRITICAL (Python numeric values), threshold configuration (`log_set_level`/`log_set_level_name`/`log_get_level`), `log_enabled`, one `LEVEL: message\n` record per call to stderr, level-name lookup both ways (`log_level_name`/`log_level_from_name`) (`[code] stdlib/logging.mink`, `npm/mink/stdlib/logging.mink`; `[test] tests/logging_lib.rs` l01-l13: default threshold, filtering, exact stderr bytes, name round trips, boundaries incl. empty + 4000-byte messages, 200-call and 50-heap-message ownership runs; `[exec]` native PE runs, `examples/log_util/main.mink` via `mod logging;`) | No handlers/formatters/filters hierarchy, no file rotation, no timestamps (a `strftime`-formatted prefix waits on S69), no `%`-style record formatting | - | S-M | N | A | Delivered in Session 108. The threshold lives in the process environment (`MINK_LOG_LEVEL`) because MINK has no mutable globals; `log_set_level` writes it and `log_get_level` reads it (default INFO). No long-lived state, no unsound globals |
| S75 | Unit-test framework in MINK | VERIFIED | `mink test` CLI discovers `fn test_*()` functions, generates per-test wrappers, builds+runs each, reports PASS/FAIL per test + summary counts; `stdlib/assert.mink` provides assertion macros (`[code] src/cli.rs` run_test_command, `stdlib/assert.mink`; `[test] tests/test_runner.rs` t01-t33: discovery, no-tests diagnostic, pass/fail reporting, assertion integration, compile error reporting, main-stripping, parameter-skipping, single-test; `[test] tests/assert_lib.rs` a01-a30; `[exec]` native PE runs via `mink test`) | No test filtering/selection, no timeout, no parallel execution, no coverage | - | M | N | G | Delivered in Session 108. Depends on S78 (assertions) and the `mink test` CLI command |
| S76 | Profiling | MISSING | none | Timing/profiling tools | P2 | L | N | G | |
| S77 | Runtime diagnostics / tracing hooks | MISSING | leak checker + error codes only | Tracing | P2 | M | N | G | |
| S78 | Assertions for tests | VERIFIED | `stdlib/assert.mink`: `assert_true(cond)`, `assert_false(cond)`, `assert_eq_int(a,b)`, `assert_ne_int(a,b)`, `assert_eq_str(a,b)`, `assert_ne_str(a,b)`, `assert_eq_float(a,b)` — on failure: diagnostic to stderr, exit 1; on success: silent (`[code] stdlib/assert.mink`, `npm/mink/stdlib/assert.mink`; `[test] tests/assert_lib.rs` a01-a30: 16 tests covering pass/fail paths for all assertion functions, zero/int64-extremes/empty-string boundaries, diagnostic content validation, 200-call ownership run; `[exec]` native PE runs) | No deep-equality or struct comparison (use `assert_eq_int`/`assert_eq_str` field-by-field); no `assert_ne_float`; no custom failure messages | - | S | N | G | Delivered in Session 108. Foundation for S75/T07 test framework |

---

## 6. TOOLING capability matrix (official Python developer workflow → MINK)

| ID | Python capability | MINK status | MINK equivalent / evidence | Gap | Pri | Diff | Blocks | Wave | Notes |
|---|---|---|---|---|---|---|---|---|---|
| T01 | Compiler/CLI executable | VERIFIED | `mink build/run/check/explain/version/help`, `-v/-V/--version`, `--json`, `--target` (`[code] src/cli.rs`; `[test] tests/cli.rs` 74) | None | - | - | N | - | |
| T02 | Script execution | VERIFIED | `mink run` (see R02) | None | - | - | N | - | |
| T03 | Check/typecheck without output | VERIFIED | `mink check` runs parse→semantics→types→HIR→MIR (and ownership) (`[code] src/driver.rs`) | None | - | - | N | - | Equivalent of a lint+typecheck pass; no official Python equivalent exists (py_compile is weaker) |
| T04 | REPL / interactive | VERIFIED | `mink repl [path]` — the interactive compile-eval session (see R01); an optional path pre-loads that file's declarations into the session (`[code] src/cli.rs` `run_repl`; `[test] tests/repl.rs` r13) | No persistent value bindings across lines (declarations only); no history/readline tab completion | - | L | N | G | Delivered this session |
| T05 | Module execution `python -m` | MISSING | none | see R03 | P3 | M | N | F | |
| T06 | Import search paths / installed stdlib | VERIFIED | `mod name;` searches the declaring file's directory, then `<exe>/../stdlib`, then `cwd/stdlib` (`[code] src/driver.rs` resolve_module_path; `[test] tests/modules_check.rs`; `[exec]` Session 99 clean install and Session 107 probes importing `encoding` through `cwd/stdlib`) | None for V1 | - | - | N | - | **Reclassified in Session 107**: the stale evidence cited `src/module/mod.rs`; the delivered mechanism is `src/driver.rs` resolve_module_path (see L67 VERIFIED) |
| T07 | Test runner (unittest equivalent) | VERIFIED | `mink test` CLI command discovers and runs `fn test_*()` functions with per-test build+run isolation; pass/fail output, failure list, summary line; compile errors reported inline (`[code] src/cli.rs` run_test_command + strip_main_fn; `[test] tests/test_runner.rs` t01-t33: discovery, no-tests diagnostic, pass/fail, assertions, compile error, main stripping, parameter skip; `[exec]` native PE runs) | No test filtering (`--test-name`), no timeout, no parallel execution | - | M | N | G | Delivered in Session 108. Paired with S75 (test framework) |
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
| W06 | Environment variables | VERIFIED | see R13 — wired | ANSI-only | P2 | S-M | N | - | REQ |
| W07 | Process creation | VERIFIED | CreateProcessA + pipe capture (`[code] src/backend/emit/runtime.rs`; `[exec]` Session 97) | see R22 extras | P2 | M | N | C | REQ |
| W08 | Console stdin/stdout/stderr | VERIFIED | stdout verified; program stdin (`rt_stdin_read`) and stderr write (`rt_stderr_write`) added (R09/R10) (`[test] tests/session99.rs`) | Unicode console text (W05) | P2 | M | N | H | REQ |
| W09 | File metadata (attributes, timestamps) | PARTIAL | size only | attrs/mtime | P2 | M | N | C | REQ |
| W10 | Sockets | VERIFIED | Winsock2 loopback TCP/UDP verified (`[test] tests/windows_hardening.rs`) | None | - | - | N | - | REQ |
| W11 | Signals/control events | MISSING | none | SetConsoleCtrlHandler | P2 | M | N | H | OPT (graceful shutdown) |
| W12 | Executable discovery / PATH resolution | PARTIAL | child commands run through shell resolution; direct CreateProcessA path behavior untested | PATH search semantics | P2 | S | N | C | REQ (CLI tools) |
| W13 | Temp dirs | MISSING | none | GetTempPath/GetTempFileName | P2 | S | N | C | REQ |
| W14 | User/home/known folders | VERIFIED | `rt_home_dir` intrinsic: USERPROFILE, falling back to HOMEDRIVE+HOMEPATH (`[code] src/backend/emit/runtime.rs` emit_home_dir) | APPDATA/known-folders via SHGetKnownFolderPath still MISSING (P2, Wave H) | P2 | S | N | A | IMPLEMENTED Session 100: `[test] tests/session100.rs` (5 tests: USERPROFILE, both fallback branches, missing-vars, path correctness). Session 107: the row was already VERIFIED; the stale `P1`/`Blocks = Y` flags are cleared |
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
| P01 | Distributing the tool itself | VERIFIED | npm `@p4inz-code/mink` 1.0.2 is `latest` on the registry, clean install ×2, standalone exe (`[exec]` Session 97; `[code] npm/mink/package.json`) | None | - | - | N | - | Category (B) is done |
| P02 | Standard-library source available to installed users | VERIFIED | npm `@p4inz-code/mink` ships `npm/mink/stdlib/*.mink` (all 26 modules) beside `bin/mink.exe`, and `mod name;` resolves it via `<exe>/../stdlib` with no config (`[code] npm/mink/package.json` `files`, `src/driver.rs` resolve_module_path; `[test] tests/release.rs` s107_npm_stdlib_bundle_matches_repo_stdlib — bundle byte-identical to `stdlib/`; `[exec]` Session 99 clean-install stdlib import) | None for V1 | - | - | N | - | **Reclassified in Session 107**: the row still said MISSING although the bundled stdlib landed in Session 99 (see the §2 baseline row). Session 107 also synced the drifted copy (`str_split`/`str_join`, the UTF-8 layer) and added the permanent drift guard |
| P03 | Import package (`import pkg`) | VERIFIED | `mod pkg;` imports a directory package (see L68) and `use pkg::item;` names its items; two or more sibling modules now resolve to distinct symbols (`[code] src/hir/lower.rs`, `src/typecheck/checker.rs`, `src/ownership/mod.rs` — declaration maps keyed by `(file, offset)`; `[test] tests/packages.rs` l68_two_sibling_modules_resolve_distinct_symbols; fixture `tests/packages/siblings/`; `[exec]` native PE run) | No namespace isolation between packages (V1 flattens all items); no package versioning | - | M | N | F | Delivered this session, including the multi-module symbol-collision defect fix |
| P04 | Installed package / site-packages | VERIFIED | A **packages directory** is MINK's site-packages: `<project>/.mink/packages` (or `<project>/.mink/envs/<name>/packages` when an environment is active). `mink install` copies each resolved package to `<packages>/<name>/` and records `name`/`version`/`source`/`sha256:` content hash in `<packages>/.installed`; the compiler resolves `mod name;` against that directory (site-packages), then each path dependency, the declaring file's own directory and the bundled stdlib (`[code] src/package/{install,mod}.rs`, `src/driver.rs` `resolve_module_path` + `module_roots`; `[test] tests/package_manager.rs` p04_* and p05_install_then_run_imports_the_installed_package (native PE prints a value from the installed package), p04_spaces_and_unicode_in_the_project_path, p04_an_empty_project_installs_nothing_and_succeeds, p04_init_reports_an_existing_project; `[exec]` native PE runs) | No registry-provided binary packages (§6); installs are source copies, not symlink farms | - | M | N | F | Delivered in Session 116 |
| P05 | Dependency declaration + install (`pip`) | VERIFIED | `mink add <name> --version <req>` / `--path <dir>` declares and installs, `mink remove` (`uninstall`) undeclares and prunes what is no longer required, `mink install [--check]` resolves+installs+writes the lock, `mink update` re-resolves (`[code] src/package/mod.rs` (`add_dependency`/`remove_dependency`/`install_project`), `src/cli.rs` (`init`/`add`/`remove`/`install`/`update`); `[code] src/package/manifest.rs` reads and edits `mink.toml`); `[test] tests/package_manager.rs` p05_*: end-to-end add→install→**run** for a versioned and for a path dependency, remove-uninstalls-the-leaves, update reconciles a removed dependency, 24 packages install deterministically; `[exec]` native PE runs) | Dependencies come from local sources (`<name>/<version>/` directories) and paths; the network registry of §18 is P10, so `pip install <url>` has no counterpart | - | XL | N | F | Delivered in Session 116. A failed `add` never edits the manifest (the edited manifest is resolved before it is written) |
| P06 | Dependency resolver | VERIFIED | A fixed-point resolver over `BTreeMap`s (so the result never depends on enumeration order): candidates come from every configured source plus path-pinned dependencies, the **highest** version satisfying every requirement wins, and a lockfile can pin exact versions. Reports no-candidate (`E-PKG05`), conflict with both requirement chains (`E-PKG06`), non-convergence (`E-PKG07`) and cycles (`E-PKG08`) (`[code] src/package/resolver.rs`, `src/package/version.rs` (`^`/`~`/`=`/ranges/`*` with the documented bare-default), `[test] tests/package_manager.rs` p06_*: highest-compatible + update moves it, transitive closure with the lock recording the edge, both conflict sides named, cycle reported, missing package reported without touching the manifest; `[test] src/package/resolver.rs` 13 unit tests incl. determinism across runs and pin fallback) | No features/optional/default dependencies (§9), no target-specific dependencies (§8), no version-preference heuristics beyond highest-compatible | - | L | N | F | Delivered in Session 116 |
| P07 | Lockfile / reproducibility | VERIFIED | `mink.lock` in the documented §5 format (`[[package]]` with `name`, `version`, `source`, `checksum`, `dependencies`). `install` prefers the locked versions while they still satisfy the manifest and reports a stale lock as `E-PKG14`; `update` re-resolves and rewrites it; rendering is byte-stable for a resolution (`[code] src/package/lock.rs`, `[test] tests/package_manager.rs` p07_*: byte-identical lock across repeated installs, sha256 content check catches tampering and a plain install repairs it, stale lock reported and reconciled, exact requirement honored across updates; `[test] src/package/lock.rs` unit tests) | Checksums cover the installed source tree, not a signed archive (signing is §17, V2+) | - | M | N | F | Delivered in Session 116 |
| P08 | Project metadata / manifest | VERIFIED | `mink.toml` per §3, read and edited by a dependency-free TOML-subset reader (tables, quoted strings, inline tables, string arrays, comments), preserving every section it does not act on; `mink init` writes a starter manifest (`[code] src/package/manifest.rs`, `[test] src/package/manifest.rs` unit tests + `tests/package_manager.rs` p08_*: malformed version reported with its location, whitespace/Unicode paths, missing path dependency reported, version+path together rejected) | Fields outside `name`/`version`/`dependencies`/`sources` are preserved but unused (no `edition`, `license`, `features` behaviour); a duplicate table is a reported error, but full TOML (dates, floats, nested tables) is not read | - | M | N | F | Delivered in Session 116 |
| P09 | Virtual environment / environment isolation | VERIFIED | `mink env new/use/list/remove`: an environment is `<project>/.mink/envs/<name>/packages` plus a `.mink/active-env` pointer; creating one activates it, removing the active one clears the pointer, and installs land in — and module resolution reads — only the active environment (`[code] src/package/env.rs`, `src/cli.rs`, `src/driver.rs`); `[test] tests/package_manager.rs` p09_*: a fresh environment does not inherit the project's packages, install fills the active environment while the project's copy stays byte-identical, tampering inside the environment is detected there, `env remove` restores the project, unknown/invalid names report `E-PKG13`; `[test] src/package/env.rs` 7 unit tests; `[exec]` native PE runs in the project and inside the environment) | Isolation is directory-level (no interpreter/compiler shim, no activation shell script); an environment installs the project's dependencies rather than having its own manifest | - | M | N | F | Delivered in Session 116. Depends on P05 |
| P10 | Registry / publishing workflow | MISSING | none | package registry | P2 | XL | N | F | Ecosystem-stage item, after P05-P09 |
| P11 | Versioning / semver discipline | PARTIAL | compiler itself is 1.0.2 semantic; no library-version model | package versioning | P2 | S | N | F | |
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
| VERIFIED | 30 | 15 | 35 | 8 | 5 | 9 | 102 |
| VERIFIED (INTENT. DIFF.) | 0 | 1 | 0 | 0 | 0 | 0 | 1 |
| VERIFIED (partial) | 0 | 0 | 0 | 1 | 0 | 0 | 1 |
| EXECUTION VERIFIED | 1 | 0 | 1 | 0 | 0 | 0 | 2 |
| INTENT. DIFF. | 11 | 0 | 0 | 0 | 1 | 1 | 13 |
| INTENT. DIFF. + PARTIAL | 1 | 0 | 0 | 0 | 0 | 0 | 1 |
| N/A | 0 | 1 | 3 | 1 | 0 | 1 | 6 |
| N/A (INTENT.) | 1 | 1 | 0 | 0 | 0 | 0 | 2 |
| PARTIAL | 9 | 2 | 12 | 0 | 10 | 1 | 34 |
| PLANNED | 2 | 0 | 0 | 0 | 0 | 0 | 2 |
| MISSING | 18 | 9 | 27 | 6 | 6 | 2 | 68 |
| **Total** | **73** | **29** | **78** | **16** | **22** | **14** | **232** |

*(Session 117 recomputation: produced by scanning the 232 capability rows directly
rather than by adjusting the previous table; every column and row sums to 232, and the
`P1` set equals the `Blocks = Y` set exactly: 0 rows.)*

### 10.2 Priority distribution

| Priority | L | R | S | T | W | P | Total |
|---|---|---|---|---|---|---|---|
| P0 | 0 | 0 | 0 | 0 | 0 | 0 | **0** |
| P1 (parity-blocking by definition) | 0 | 0 | 0 | 0 | 0 | 0 | **0** |
| P2 (important, not blocking) | 23 | 9 | 34 | 2 | 17 | 3 | **88** |
| P3 (optional) | 23 | 8 | 14 | 5 | 3 | 1 | **54** |
| — (no gap / no MINK work) | 27 | 12 | 30 | 9 | 2 | 10 | **90** |
| **Total** | **73** | **29** | **78** | **16** | **22** | **14** | **232** |

Every row marked P1 is also marked "Blocks parity = Y"; there are no P1 non-blockers
and no P2 blockers in this audit. **0 capability gaps block the parity gate** — the
parity-blocking set is empty. Session 116 closed P04–P09 and Session 117 closed S62.

Session 107 cleared the flags that contradicted a row's own evidence (R06 and W14 were
already VERIFIED; P02's bundled stdlib landed in Session 99; T06's search path is the
`src/driver.rs` mechanism behind the VERIFIED L67), so P1 equals the `Blocks = Y` set
exactly. Session 112 removed the last four stale `Blocks = Y` flags on rows that were
already VERIFIED (S41, S75, S78, T07 — S41 also carried a stale `P1`), so the invariant
holds again after the R19/R21/S71 closures. Session 114 closed R20 and S73, Session 116
closed P04–P09, and Session 117 closed S62 (each row's flags were cleared as the row
became VERIFIED), leaving the parity-blocking set **empty**.

### 10.3 Difficulty distribution (rows requiring work; the other 61 rows carry no work)

| Difficulty | Rows | Meaning |
|---|---|---|
| S-M (small-to-medium) | 6 | small change with a medium tail |
| S (small) | 37 | one focused change, low risk |
| M (medium) | 84 | multiple components, contained |
| L (large) | 30 | major feature area |
| XL (major subsystem) | 14 | dedicated multi-session subsystem |
| **Rows requiring work** | **171** | 171 + 61 no-work (`—`) rows = 232 |

### 10.4 Parity-blocking gaps (0) by wave

Wave tags are the exact matrix values; a gap that spans waves (B/H) is listed under its
primary wave. The rows delivered across Sessions 99–117
(R06, R10, R12, R13, R23, S50, S70, W06, W08, W14, L16, L67, P02, T06, L05, L08, S01,
S06, S28, S36, S42, S44, S41, S74, S69, S75, S78, T07, R01, T04, S02, L68, P03, S71,
R19, R21, R20, S73, S63, P04, P05, P06, P07, P08, P09, S62) no longer appear. Every wave
is now empty.

| Wave | Blocking gaps | Count |
|---|---|---|
| A (Windows platform/runtime quick wins) | (none) | 0 |
| B (core language/data) | (none) | 0 |
| C (filesystem/process/time) | (none) | 0 |
| D (networking/internet/compression) | (none) | 0 |
| E (concurrency/async) | (none) | 0 |
| F (packaging/distribution) | (none) | 0 |
| G (developer tooling) | (none) | 0 |
| **Total** | | **0** |

### 10.5 Fully covered areas (no material gap, Wave `-`)

Fully covered means the official capability has a working MINK equivalent with no
material missing subset; rows are VERIFIED or INTENT. DIFF. with no P-gap:

- Scalars & arithmetic: booleans (L04), integer/float arithmetic (L20/L21/L22/L23), conditional expressions (L24), indexing with bounds checks (L25), augmented assignment (L31)
- Data: tuples (L09), ranges (L14), enum/named-constant modeling (L17), Null/Option (L13), byte buffers (L06/L07), mutable heap `Str`
- Control: `if`, `while`, `loop`, `for` over ranges, break/continue/return, exhaustive `match` with rich patterns (L32-L37)
- Functions: positional params, recursion, lambdas/closures (L38-L45)
- Modules: file modules, `use`/`pub`, multi-file compilation (L65/L66)
- Memory/runtime: deterministic arena + ownership + leak check (R17/R18)
- Filesystem: read/write/copy/move/remove/mkdir/cwd/path ops, directory enumeration/traversal (S26/S27/S28/S32/S33)
- Process: run + output capture + exit codes + PID (R22 core)
- Network: TCP/UDP/DNS/hostname/byte-order (S56-S58, S65)
- HTTP/1.1 client GET/POST core (S59 core)
- Data formats: JSON parse/serialize (S35), CSV read/write (S36), base64/hex/url encode-decode (S09), SHA-256/HMAC/secure-random (S46-S48)
- Time: epoch/millis/monotonic/ticks (R29, S66)
- Math: float+int helper library (S17), random (S18 core)
- Tooling: CLI build/run/check/explain/`--json`, exit codes, `--target` selection, error-code docs (T01-T03, T11, T13, T14)
- Windows: process creation, Winsock sockets, BCrypt crypto provider, stdout console writes, standalone PE output (W07, W10 + base rows)
- Compiler distribution via npm (P01)
- Concurrency: native threads (R19), locks/spin primitives (R21) and the `threads` stdlib module (S71), with a lock-guarded allocation path; async tasks and their event loop (R20) with the `async fn`/`await` language surface (S73) and the `tasks` stdlib module

### 10.6 Headline conclusions

1. **The Windows base platform is COMPLETE/STABLE** with 0 P0 and 0 P1 on the base (Session 97 gate, re-verified this session).
2. **Official-Python-capability parity passes the parity gate**: 106 execution-verified rows (VERIFIED 102 + EXECUTION VERIFIED 2 + the two partial-VERIFIED variants), 34 PARTIAL rows, 68 MISSING rows, 14 intentionally-different rows (13 INTENT. DIFF. + 1 INTENT. DIFF. + PARTIAL), 8 N/A rows (6 N/A + 2 N/A (INTENT.)), 2 PLANNED rows. No remaining gap is parity-blocking (all P2/P3).
3. **0 parity-blocking P1 gaps** (Wave A 0 · B 0 · C 0 · D 0 · E 0 · F 0 · G 0); in all, 171 rows require work and 61 carry none (the counts are recomputed from the rows in §10.1/§10.2/§10.3).
4. **Zero P0 gaps** on the Windows base.
5. Fully covered categories concentrate where MINK has already executed real work: native execution, ownership/memory, files, processes, sockets, HTTP client, JSON, crypto, math, time, and the compiler toolchain itself.
6. Parity-blocking work is closed: packaging (Wave F — P04–P09 in Session 116, with P07/P08 delivered alongside) and TLS (Wave D — S62 in Session 117, after S42 zlib/gzip, S41 SQLite and S44 zip). Wave E closed in Session 114 (R19/S71 threads + locks in Session 112, R20/S73 async + await now VERIFIED). Waves A, B and C are fully closed (S74 in Session 108; L34 reclassified P2 in Session 111 — MINK's value-based error model is the intentional design); the shared A/F include-path mechanism landed across Sessions 99–107.
7. `docs/audits/OFFICIAL_PYTHON_CAPABILITY_PARITY_AUDIT.md` (Session 82) is **superseded for stale claims**: Windows env (`rt_env_*`) was stubbed at audit time but is implemented since Session 99; `x86_64-linux-elf` IS implemented (frozen); `&&`/`||` DO short-circuit (probed); crypto is execution-verified on Windows.

### 10.7 FINAL STATUS (this matrix)

- WINDOWS BASE PLATFORM = **COMPLETE / STABLE**
- WINDOWS OFFICIAL PYTHON CAPABILITY PARITY = **PASSES THE PARITY GATE** (0 P0, 0 P1;
  the remaining P2/P3 rows are non-blocking roadmap work)
- LINUX = **FROZEN** (untouched this session)

**Session 117 updates:** **S62** (TLS/SSL) **P1 blocker CLOSED** — the last one. A TLS 1.2/1.3 client and an HTTPS client on Windows Schannel live in `stdlib/tls.mink` (plus the npm mirror), driven by a new general FFI layer (`rt_sys_load_lib`/`rt_sys_get_proc`/`rt_sys_call` with the Win64 register/stack convention and 16-byte-aligned call sites, plus the unvalidated foreign reads `rt_sys_load64`/`rt_sys_load32`). Trust is never skipped: Schannel runs manual credential validation, and MINK performs the trust decision itself with crypt32 (chain build + `CERT_CHAIN_POLICY_SSL` + root-anchor thumbprint match against the system store or a pinned PEM CA). `rt_net_getaddrinfo` was upgraded from its V1 pass-through to a real Winsock `getaddrinfo` resolution, and `rt_exit`/`rt_fail` now terminate through `kernel32!TerminateProcess` after the leak scan, because a measured crypt32 DLL-detach deadlock makes any `CertGetCertificateChain` caller hang at process exit. New permanent evidence: `tests/tls_lib.rs` (20 tests: 19 native-PE + 1 opt-in public endpoint; pinned-CA handshake, HTTPS GET, SAN/wildcard/IP-literal matching, expired certificate, wrong EKU, untrusted root, own-store rejection, wrong host name, connection refused, malformed/missing CA, malformed URL, invalid/closed handles, binary echo with partial reads, 300 KB transfer, repeated no-leak connections; deterministic local server + committed fixture certs) and `tests/network_lib.rs`'s resolver test now asserts the real resolution contract. Parity-blocking set: 1 → 0. The Windows parity gate is reached: **0 P0 · 0 P1**.

**Session 116 updates:** **P04/P05/P06/P07/P08/P09** (site-packages, dependency declaration + install, resolver, lockfile, manifest, virtual environments) **P1 blockers CLOSED**, emptying Wave F. MINK's packaging subsystem is `mink.toml` + `mink.lock`, a deterministic fixed-point resolver over `BTreeMap`s (highest compatible version wins; reported no-candidate/conflict/non-convergence/cycle), content-hashed installs into a packages directory the compiler resolves as site-packages, and directory-level environment isolation via `mink env new/use/list/remove`. `[test] tests/package_manager.rs` covers the whole workflow end to end (including native PE runs of installed packages, deterministic 24-package installs, tamper detection, stale locks and clean-project workflows) with unit tests for the manifest, version, resolver, lock and env modules; `docs/ecosystem/PACKAGE_ARCHITECTURE.md` §26 records the V1 implementation. Parity-blocking set: 5 → 1.

**Session 114 updates:** **R20** (async / event loop) and **S73** (async/await + event loop) **P1 blockers CLOSED**, emptying Wave E. The task loop is the asyncio-equivalent in MINK's own architecture: a task is `fn(arg: Int) -> Int` on its own OS thread, and the process-global loop tracks uncollected tasks (`rt_task_spawn`/`rt_task_spawn0`, `rt_task_await`, `rt_task_run`, `rt_task_pending`, `rt_task_stop`). The language surface is real syntax — `async fn name(args) -> T { body }` plus `await expr` — implemented as a front-end desugaring over the verified runtime (`src/parser/mod.rs`), so awaits compose with loops, branches and module boundaries and there is no synchronous stand-in. New permanent evidence: `tests/async_lib.rs` (23 native-PE tests: handle accounting, drain, zero tasks, 200 spawn/await cycles, 64 concurrent tasks, `task_stop`, the E-R06 uncollected-task leak, the E-R05 double-collect and E-R13 non-task-handle errors, a concurrency-ordering proof, 4×1000 locked increments giving exactly 4000, tasks allocating/freeing strings, 5 identical stress runs, the language-surface cases incl. nested async and cross-module async, the two rejected forms, the `tasks` stdlib module and the shipped example twice), `examples/async_tasks/main.mink` and `stdlib/tasks.mink` (+ npm mirror). Two defects were found and fixed while verifying: an awaited handle is now read through the runtime's validated word load (so a stale or bogus handle is E-R05/E-R13 instead of a fault), and `pub async fn` in a module is parsed as two `pub` items instead of being silently dropped. Parity-blocking set: 7 → 5 (S62, P04, P05, P06, P09).

**Session 112 updates:** **R19** (threads), **R21** (locks/synchronization primitives) and **S71** (threads + locks) **P1 blockers CLOSED**. Real OS threads via `CreateThread` and a hand-written trampoline that bridges the Windows x64 ABI into the MINK stack ABI (`rt_thread_spawn`/`rt_thread_join`/`rt_thread_id`), plus `rt_ptr_to_int`/`rt_int_to_ptr` so a shared-context pointer can be published in a `Ptr<Int>` block; `stdlib/threads.mink` ships `thread_spawn`/`thread_join`/`thread_self` and `lock_new`/`lock_acquire`/`lock_release`/`lock_free`. Because every thread shares the runtime arena, the allocator and the raw memory accessors are now guarded by a self-contained test-and-set spin lock; `rt_fail` reports a failed `CreateThread` as `E-R12`. New permanent evidence: `tests/threads_lib.rs` (17 native-PE tests: spawn/join results, live thread ids, 200 sequential spawn/joins, 8 concurrent threads, parallel allocation integrity, string interaction, pointer-cast round trip, an occupancy-counter mutual-exclusion probe, 4x5000 locked increments giving exactly 20000 twice, an unlocked control that must not be exact, 200 lock new/free cycles, and re-acquisition after join), plus `examples/threaded_work/main.mink` (4 threads, lock-guarded accumulation, closed-form check, deterministic across runs) and the `threads` stdlib module test. Four stale `Blocks = Y` flags on already-VERIFIED rows (S41, S75, S78, T07 — S41 also a stale `P1`) were cleared, and the whole §10 aggregate block was recomputed from the rows. Parity-blocking set: 9 → 7 (R20, S62, S73, P04, P05, P06, P09).

**Session 106 updates:** L10 (dict) MISSING → VERIFIED; L11 (set) MISSING → VERIFIED; L12 (frozen set) MISSING → VERIFIED; S11 (named collections) MISSING → PARTIAL; S16 (custom collections) PARTIAL → VERIFIED. Four root-cause bugs fixed in Map/Set rebuild/lookup/string-free. Permanent regression coverage added.

**Session 108 updates:** four P1 blockers **CLOSED** — **S28** (directory listing / traversal), **S74** (logging), **S06** (string parsing) and **S36** (CSV) — taking the parity-blocking set from 26 to 22, emptying Wave A and reducing Wave B to L34/S02.

- **S36** adds `stdlib/csv.mink`: `csv_parse_row` (RFC 4180 subset with quoted fields, `""` escapes and CR/LF/CRLF terminators), `csv_write_row`/`csv_escape` (quote-and-double only when needed), `csv_field_count` and `csv_round_trip`. Coverage: `tests/csv_lib.rs` c01–c14. A buffer-sizing defect found while testing (embedded quotes were not counted against the quoted length) was fixed with permanent coverage.

- **S06** adds `str_parse_int`/`str_parse_float` to `stdlib/strings.mink`: `(value, ok)` tuple results replacing Python's `int()`/`float()` exceptions, with sign/whitespace/`_`-separator handling, fraction/exponent parsing, i64 boundary rejection, and saturating extreme exponents. Coverage: `tests/strings_lib.rs` p01–p12.

- **S28** adds `fs_dir_open`/`fs_dir_next`/`fs_dir_close` over new Windows runtime services (`rt_dir_open`/`rt_dir_next`/`rt_dir_close`, `FindFirstFileA`/`FindNextFileA`/`FindClose`), with `.`/`..` filtered so the stream matches `os.scandir()`. The handle is an owned allocation, so an unclosed enumeration is a leak (E-R06), consistent with MINK's ownership model. Regression coverage: `tests/filesystem_lib.rs` d01–d11.
- **S74** adds `stdlib/logging.mink` (levels, threshold, one `LEVEL: message` record per call on stderr, level-name lookup both ways, `log_enabled`). The threshold lives in `MINK_LOG_LEVEL` because MINK has no mutable globals. Regression coverage: `tests/logging_lib.rs` l01–l13, plus `examples/log_util/main.mink` built and run through `mod logging;`.

Every count in §10 was recomputed from the rows after both closures.

**Session 107 updates:** L05 (Unicode text) **P1 blocker CLOSED** — the UTF-8 code-point layer (`utf8_validate`/`decode`/`encode`/`char_count`/`char_at`/`byte_index`/`slice`) was added, native-verified and leak-checked, fixing two latent defects on the way (heap-argument leak; acceptance of overlong 2-byte forms and surrogates). S01 (string methods) and L08 (lists) `P1` flags cleared after their Session 106/107 closures. Row flags reconciled against each row's own evidence for R06, W14, P02 and T06, and every count in §10 recomputed **from the rows** (stale aggregate tables and five rows with a missing trailing pipe were corrected). The npm stdlib bundle was re-synced (`str_split`/`str_join` plus the UTF-8 layer) with a permanent drift guard.

**Session 111 updates:** **S44** (ZIP) and **S41** (SQLite) **P1 blockers CLOSED**. `stdlib/zip.mink` reads and writes real ZIP archives on top of the Session 110 DEFLATE codec, cross-validated bidirectionally against CPython's `zipfile`, with mandatory path-traversal safety; `stdlib/sqlite.mink` is a self-contained embedded SQL engine (see the S41 row). Two compiler defects were found and fixed on the way, both the same class: expression types and semantic resolutions were keyed by source offset alone, so any program that combined two modules could resolve one module's expression to another module's at the same offset (`src/typecheck/mod.rs`, `src/semantics/mod.rs`). Parity-blocking set: 12 → 9 (S44, L34 reclassified to P2 as an intentional difference already represented by `Result`/`Option`, and S41).

**Session 110 updates:** **S42** (zlib/gzip) **P1 blocker CLOSED** — `stdlib/zlib.mink` is a self-contained DEFLATE codec (raw/zlib/gzip over stored, fixed-Huffman and dynamic-Huffman blocks, with CRC-32 and Adler-32), native-PE verified and cross-validated bidirectionally against CPython 3.11 `zlib` across 77 vectors. Parity-blocking set: 13 → 12. Two real defects were found and fixed on the way: `_z_fill` never advanced its scan index after a separator (an infinite loop reachable from every public entry point) and the compression free order stranded a 128 KiB skipped block per call; the codec now folds its static tables into the input-sized arena so every request is input-sized.

**Session 111 updates:** **S44** (zip archives) **P1 blocker CLOSED** — `stdlib/zip.mink` is a self-contained PKZIP reader/writer over the verified DEFLATE codec, native-PE verified and cross-validated bidirectionally against CPython 3.11 `zipfile` across 87 checks (archive creation, multi-entry with stored/deflate/dir entries, CPython reads MINK archives correctly, MINK reads CPython archives correctly, MINK extracts CPython archives with byte-correct content, 10 hostile archive names rejected, 4 malformed inputs rejected, 30-entry capacity-growth stress, deterministic golden bytes, leak-free). A multi-module P0 defect was also found and fixed: `expr_type` and `resolve` lookups in `src/typecheck/mod.rs` and `src/semantics/mod.rs` were keyed by byte offset only, so two-module programs could mis-type expressions — fixed by keying all span lookups by `(SourceId, offset)`. Parity-blocking set: 12 → 11.

**Session 109 updates:** five P1 blockers **CLOSED** (L68, P03, R01, T04, S02), one latent multi-module defect fixed. **L68/P03** (directory packages + package import) — `mod name;` resolves to `name/mod.mink`, nested packages compose, and the multi-module symbol-collision defect (declaration maps keyed by byte offset alone, so sibling modules at the same offset collapsed onto one symbol) was root-caused and fixed across HIR/typecheck/ownership (`tests/packages.rs`, fixtures under `tests/packages/`). **R01/T04** (interactive REPL) — `mink repl` delivers a compile-eval session on the native backend, with an initial-file preload and `:help`/`:show`/`:clear`/`:quit` commands (`tests/repl.rs` r01-r14, native PE runs through the real CLI). **S02** (regular expressions) — `stdlib/re.mink` delivers a self-contained backtracking engine with search/match/full-match/find/find_all/count/split/replace and a bounded step budget (`tests/re_lib.rs` s02_* 33 tests). Parity-blocking set: 18 → 13; every count in §10 was recomputed from the rows at the end of Session 109.

---

## 11. Evidence index (primary anchors)

Compiler pipeline and targets: `[code] src/{lexer,parser,ast,semantics,typecheck,ownership,hir,mir,monomorphize,backend}`, `src/backend/target.rs`.
Entry/CLI: `[code] src/cli.rs`, `src/driver.rs`, `src/backend/mod.rs`. Modules: `src/module/mod.rs`.
Runtime services/intrinsics: `[code] src/runtime/intrinsics.rs`, `src/backend/emit/runtime.rs`, `src/backend/emit/pe.rs`, `src/backend/emit/x86_64.rs`, `src/runtime/{allocator,error,abi}.rs`.
Standard library: `[code] stdlib/*.mink` (26 modules: assert, collections, crypto, csv, encoding, environment, filesystem, hashing, http, json, logging, math, network, option, process, random, re, result, sqlite, strings, tasks, threads, time, tls, zip, zlib — each mirrored into the npm bundle, with `tests/release.rs` guarding the mirror byte-for-byte).
Test suite (execution-verified native runs): `[test] tests/*.rs` (71 integration targets + the library unit tests; **2 957 passed · 3 ignored · 0 failed** — 2 946 in the Session 119 public-acceptance audit, 2 941 as of Session 118, plus the regression tests added since, including the five hashing heap-input regressions from the systems-readiness audit) — per-domain: strings_lib, math_lib, encoding_lib, filesystem_lib, logging_lib, csv_lib, process_lib, network_lib, http_lib, json, crypto_lib, hashing_lib, collections_lib, time_lib, random_lib, sqlite_lib, zip_lib, zlib_lib, re_lib, threads_lib, async_lib, tls_lib, packages, package_manager, repl, test_runner, windows_hardening, release, cli, smoke.
Recorded real execution: `[exec]` SESSION_92..97 docs (crypto vectors, HTTP POST byte-exact echo, process 1 MB drain, npm clean installs ×2, standalone exe) and this session's native probes (short-circuit, div-by-zero fault status 148).
Specs/plans: `[doc] docs/core/*`, `docs/language/*`, `docs/ecosystem/*` (C_ABI_SPEC, PACKAGE_ARCHITECTURE, SECURITY_ARCHITECTURE, STDLIB_ARCHITECTURE), `docs/roadmap/*`, `docs/implementation/SESSION_*`.

*End of master parity matrix — Session 98 baseline, commit `a716df4`; reconciliation headers, §2/§3.10/§10 and §11 recomputed by the Session 118 final Windows completion audit at commit `86642b4`.*

