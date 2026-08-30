# SESSION 53 — STRINGS LIBRARY + FOUNDATION HARDENING

## Objective
Build the second official MINK ecosystem library: Strings. Harden foundation for future libraries.

## Library Selected: Strings
**Score**: 8.7/10 — highest among candidates (Math 6.6, Collections 6.8, Filesystem 6.7, Encoding 7.0).
Rationale: Every library, JSON, and AI agent needs string manipulation. Universally depended upon.

---

## Files Changed

| File | Change |
|------|--------|
| `stdlib/strings.mink` | **New** — 559 lines, 30 functions |
| `tests/strings_lib.rs` | **New** — 73 integration tests |
| `tests/json/` | Test helper files |

## API Implemented

### Search (return `(result, Str)`)
- `str_index_of(s, sub) -> (Int, Str)` — first occurrence
- `str_last_index_of(s, sub) -> (Int, Str)` — last occurrence
- `str_contains(s, sub) -> (Bool, Str)` — substring check
- `str_starts_with(s, prefix) -> (Bool, Str)` — prefix check
- `str_ends_with(s, suffix) -> (Bool, Str)` — suffix check
- `str_count(s, sub) -> (Int, Str)` — count non-overlapping occurrences
- `str_char_at(s, index) -> (Int, Str)` — byte at position

### Validation (return `(Bool, Str)`)
- `str_is_numeric(s) -> (Bool, Str)`
- `str_is_alpha(s) -> (Bool, Str)`
- `str_is_alphanumeric(s) -> (Bool, Str)`

### Comparison
- `str_cmp(a, b) -> Int` — lexicographic: -1, 0, or 1

### Transformation (consume input, return new Str)
- `str_sub(s, start, end) -> Str` — substring
- `str_trim(s) -> Str` — trim whitespace both ends
- `str_trim_start(s) -> Str` — trim leading whitespace
- `str_trim_end(s) -> Str` — trim trailing whitespace
- `str_to_upper(s) -> Str` — ASCII uppercase
- `str_to_lower(s) -> Str` — ASCII lowercase
- `str_reverse(s) -> Str` — reverse bytes
- `str_repeat(s, count) -> Str` — repeat N times
- `str_pad_left(s, target_len, pad_byte) -> Str` — left-pad
- `str_pad_right(s, target_len, pad_byte) -> Str` — right-pad

### Composition
- `str_replace(s, old, new) -> Str` — replace first occurrence
- `str_replace_all(s, old, new) -> Str` — replace all occurrences
- `str_join_2(a, b) -> Str` — concatenate two strings
- `str_join_3(a, b, c) -> Str` — concatenate three strings

---

## Architecture Chosen

**Single-return pattern** — every function has exactly ONE return statement at the end. No conditional returns with Str values.

**Why**: MINK V1 has no NLL (Non-Lexical Lifetimes). If `s` appears in ANY `return` statement (even inside an `if` branch), the type checker marks it as moved for the entire function. The single-return pattern avoids this by computing results into `mut` variables and returning once.

**Rejected patterns**:
- Multiple `return (result, s)` in if/else branches — fails type checker
- Conditional `return` with Str — marks value as moved globally

---

## V1 Compiler Limitations Discovered

### 1. No NLL (Non-Lexical Lifetimes)
The type checker does not track conditional ownership. Once a variable appears in a `return` expression in any branch, it is marked as moved for the entire function, even if the branch is not taken.

**Impact**: All library functions must use single-return pattern with `mut` result variables.

### 2. No RAII / Drop / Deferred Free
MINK V1 has no destructors. Strings allocated by transformation functions cannot be automatically freed when they go out of scope.

**Impact**: Chained transformations like `str_to_upper(str_trim(s))` leak the intermediate `str_trim` result. The caller must manage all allocations explicitly.

### 3. `rt_str_free` Crashes on Literals
`rt_str_free` on a string literal causes E-R07 (misaligned access) because literals are not heap-allocated.

**Impact**: Library functions cannot free their Str parameters — they don't know if the input is heap-allocated or a literal. The caller must manage free lifetimes.

### 4. User Functions Consume Str Parameters
When a user-defined function takes `Str`, the parameter is consumed. The caller cannot use the original variable afterward.

**Impact**: Search functions return `(result, Str)` tuples so the caller can recover the original string. After `str_index_of(s, sub)`, use `r.1` instead of `s`.

---

## Root-Cause Analysis for Defects Found

### Defect 1: str_trim / str_trim_end crash (E-R09)
- **Symptom**: Trim functions hit string index out of bounds
- **Root cause**: Loop logic `if !cond { end = 0; }` set end to 0 instead of stopping. Also, `end - 1` could go negative.
- **Fix**: Rewrote with `done` flag pattern that cleanly terminates loops
- **Regression test**: s34, s35, s36, s37 tests

### Defect 2: str_join_3 memory leak (exit 106)
- **Symptom**: `str_join_3` leaked the intermediate `rt_str_concat(a, b)` result
- **Root cause**: Nested `rt_str_concat(rt_str_concat(a, b), c)` created an intermediate string that was never freed
- **Fix**: Store intermediate in variable, use it, then `rt_str_free` it before returning

### Defect 3: str_sub negative allocation
- **Symptom**: `str_sub(s, 4, 1)` caused E-R08 (invalid allocation size)
- **Root cause**: No guard for `start >= end` — computed negative `sublen`
- **Fix**: Added `if end < start { end = start; }` guard

### Defect 4: str_replace consumed `old` parameter
- **Symptom**: `str_replace` failed to compile because `old` was consumed by `str_index_of` before reading `oldlen`
- **Root cause**: Read `oldlen` AFTER calling `str_index_of(s, old)` which consumed `old`
- **Fix**: Moved `rt_str_len(old)` and `rt_str_len(new)` BEFORE the `str_index_of` call

---

## Test Results

| Category | Tests |
|----------|-------|
| Search | 11 |
| Validation | 8 |
| Comparison | 4 |
| Transformation | 16 |
| Replace | 5 |
| Join | 2 |
| Chaining | 6 |
| Ownership | 5 |
| Practical | 3 |
| JSON Integration | 4 |
| Edge cases | 9 |
| **Total** | **73** |

**All 73 tests pass. 0 failures. 0 regressions.**

---

## Quality Gates

| Gate | Result |
|------|--------|
| `cargo fmt --check` | ✅ Pass |
| `cargo clippy --all-targets` | ✅ Pass (1 pre-existing warning) |
| `cargo test` | ✅ All pass (0 failures across all binaries) |
| `cargo build` | ✅ Pass |
| `cargo build --release` | ✅ Pass |
| Unsafe Rust | ✅ None |
| v1.0.0 untouched | ✅ Verified |

---

## 10-Persona Audit Results

| Persona | Findings | Classification |
|---------|----------|----------------|
| Compiler Engineer | No-NLL forces single-return | B |
| Type-System Engineer | Inconsistent Str consumption (intrinsics vs user fns) | B |
| Ownership Engineer | Chained transforms leak intermediates | B |
| Runtime Engineer | rt_str_free on literals crashes (correct behavior) | E |
| Backend Engineer | No issues | E |
| Library/API Designer | (result, Str) pattern is verbose | C |
| Security Engineer | All bounds checked, no unsafe | E |
| Performance Engineer | O(n*m) replace_all, acceptable for V1 | C |
| AI Developer | Ownership model non-obvious for AI codegen | C |
| External Developer | Needs documentation for ownership gotchas | C |

**Summary**: 0 A, 3 B, 5 C, 2 E. All B findings are V1 compiler limitations, not strings library defects.

---

## Performance Notes

- All search/iteration functions: O(n × m) where n = string length, m = pattern length
- `str_replace_all`: Two-pass O(n × m) per replacement
- `str_count`: O(n × m) single pass
- No premature optimization — all operations are practical for typical string sizes
- Future optimization: single-pass `str_replace_all`, Boyer-Moore for large patterns

---

## Security Notes

- All index operations bounds-checked (E-R09)
- No buffer overflows
- No integer overflow (V1 Int model)
- No unsafe Rust
- All allocations validated (E-R02/E-R03/E-R08)
- No recursion in any function — all iterative

---

## Remaining Limitations (V1)

1. **No NLL** — single-return pattern required
2. **No RAII** — chained transforms leak intermediates
3. **ASCII only** — no Unicode case conversion, no Unicode-aware trimming
4. **No regex** — pattern matching is simple substring search
5. **No `str_split`** — not implemented yet (could be added)
6. **No `str_contains_char`** — byte-level only
7. **No `format!` macro** — str_join is manual

---

## Recommendation for Session 54

**Strings is production-ready as a V1 library with documented limitations.** LOCK Strings as the second official MINK ecosystem library.

Session 54 should begin **Math** (basic numeric utilities) or **Encoding** (hex, base64) as the next library, per the dependency graph. Both are foundational and depend only on what's already available.

The foundation improvements that would unlock the most future libraries:
1. **NLL (Non-Lexical Lifetimes)** — eliminates single-return pattern requirement
2. **RAII/Drop** — eliminates memory leak in chained operations
3. **Heap string cloning** — enables safe function composition
