# Session 192 — Strings, JSON and Arena Ownership Contract

Windows x86_64, 2026-11-06. Starting HEAD
`02c9f280e43bf9efd25d90b6b2150beb54dcbac7`.

## 1. Inventory of all ten tolerances

The previous note named three JSON tests. An inspection of every occurrence
found ten, in five chained-transformation tests, one nested-join test and four
JSON tests:

| Test | Strict result | Root cause |
| --- | --- | --- |
| `s57_chain_trim_upper` | pass | none — tolerance was unnecessary |
| `s58_chain_reverse_upper` | pass | none — tolerance was unnecessary |
| `s59_chain_join_sub` | pass | none — tolerance was unnecessary |
| `s60_chain_repeat_trim` | pass | none — tolerance was unnecessary |
| `s61_chain_replace_chain` | pass | none — tolerance was unnecessary |
| `s69_string_builder` | pass | none — tolerance was unnecessary |
| `s70_json_string_in_array` | **leak** | `r.1` (the `json_as_str` copy) never freed |
| `s71_json_uppercase_string` | pass | tolerance was unnecessary |
| `s72_json_object_keys_searched` | **leak** | `r.1` (the `json_obj_get_key` copy) never freed |
| `s73_json_array_string_count` | **leak** | per-iteration `r.1` never freed |

Seven tolerances were stale. The chained tests had been written when
intermediate strings leaked; every one of those functions now frees its input
(`str_trim`, `str_sub`, `str_to_upper`, `str_to_lower`, `str_reverse`,
`str_repeat`, `str_replace_all`), so the chains were already leak-free. Their
comments asserting "V1 has no RAII, chaining leaks the intermediate" were
false and have been corrected.

## 2. The contract, established from code

The three real leaks shared one incorrect assumption in the test comments:
*"s0 references arena memory; don't `rt_str_free` it."* That is not what the
implementation does.

**`json_as_str` copies.** `stdlib/json.mink` (the function at line 188):

```mink
fn json_as_str(arena: Ptr<Int>, offset: Int) -> Str {
    let mut len = _read(arena, offset + 1);
    let mut s = rt_str_alloc(len);
    while i < len { rt_str_set_byte(s, i, _read(arena, offset + 2 + i)); ... }
    return s;
}
```

`json_obj_get_key` and `json_num_str` do the same; `json_to_str` allocates its
own `Str`. None of them return an offset into the arena.

Answers to the six questions, with evidence:

| Question | Answer | Evidence |
| --- | --- | --- |
| Does the arena own the returned string's storage? | **No.** The arena stores the parsed document; the accessor allocates a separate block | `json_as_str` calls `rt_str_alloc(len)` then copies byte by byte |
| Borrowed view or heap string? | **Heap string** | same; proven by `s192_json_as_str_survives_arena_free` |
| Does `rt_str_free` accept an arena-backed string? | The situation cannot arise through the accessors. Freeing their result is required and safe | `s70`/`s72`/`s73` exit 0 once `r.1` is freed |
| Can the leak detector distinguish arena from heap? | **No, and it does not need to.** Both are ordinary live blocks in the same table; the arena is one block, each copied string another | `allocator.rs` `leaks()` over live entries; `s192_json_arena_not_freed_is_a_leak` and `s192_json_as_str_result_unfreed_is_a_leak` |
| When does arena cleanup occur? | Only when the caller runs `rt_free(arena)`; there is no implicit lifetime | `rt_free` is the inverse of `rt_alloc`; leak scan reports an unfreed arena |
| Caller responsibility | Free every accessor result with `rt_str_free`; free the arena with `rt_free`; order between them does not matter | copy semantics above; survival test frees the arena first |

The exit-time scan therefore does not need an "arena exception". The old
tolerance was hiding three genuinely unfreed heap blocks, not a semantic
limitation of the checker.

## 3. Fixes

- `s70`: `rt_str_free(r.1)` after the assertions; comment corrected to say the
  accessor copies.
- `s72`: `rt_str_free(r.1)`; `json_obj_get_key` noted as a heap copy.
- `s73`: `rt_str_free(r.1)` inside the enumeration loop, one allocation per
  element.
- The seven stale tolerances became `assert_eq!(code, 0, …)`; no output
  expectation changed. The stale chaining comments and notes were corrected.

## 4. New permanent contract tests

| Test | Proves |
| --- | --- |
| `s192_json_as_str_survives_arena_free` | the accessor result is an independent heap copy: length is readable before and after `rt_free(arena)` |
| `s192_json_as_str_result_unfreed_is_a_leak` | discarding the copy after freeing the arena still exits 106 — it is not arena-owned |
| `s192_json_arena_not_freed_is_a_leak` | freeing the copy does not free the arena: exits 106 until `rt_free(arena)` |

The strings harness captures stdout only; `E-R06` is written to stderr, so the
negative tests assert exit 106 plus the length that the program printed before
exiting.

## 5. Verification

| Check | Result |
| --- | --- |
| `cargo test --test strings_lib` | **88 passed, 0 failed** |
| Full `cargo test` | exit 0 — **3,183 passed, 0 failed, 1 ignored, 78 suites** |
| `rustfmt --check` on `tests/strings_lib.rs` | clean |
| `git diff --check` | exit 0 |
| `\|\| 106` / `== 106` in `strings_lib` | 0 |

Counts rose from 85 to 88 tests because of the three new contract tests; no
test was removed or renamed.

## 6. Documentation

`docs/language/STANDARD_LIBRARY_REFERENCE.md` §7 now states the arena-vs-copy
rule explicitly, and the §23 ownership cheat sheet gained rows for the JSON
accessors and the arena block.

## 7. Remaining gaps

No known ownership gap remains in `strings_lib`. The repository-wide question —
whether any other suite still hides a failure behind an exit-code tolerance, or
skips an assertion behind `if code == 0` — is Session 193's audit.

## 8. Exact next action

Session 193: rebuild the complete tolerance inventory across every test suite,
inspect each occurrence rather than counting matches, audit helpers shared
across suites, and close or explicitly justify every remaining exception.
