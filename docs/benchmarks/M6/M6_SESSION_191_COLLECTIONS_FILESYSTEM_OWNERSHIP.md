# Session 191 — Collections and Filesystem Ownership Enforcement

Windows x86_64, 2026-11-06. Starting HEAD
`59cd32731f3eab78c60bd041c4e26d893ae03b14`.

## 1. Baseline

Both suites were green only because one shared helper per suite accepted
`code == 0 || code == 106`:

- `tests/collections_lib.rs:59` — gates **21** of 47 tests
- `tests/filesystem_lib.rs:75` — gates **24** of 46 tests

Changing each helper to `assert_eq!(code, 0, …)` reproduced the reported
failures exactly:

| Suite | Passed | Failed | Total |
| --- | ---: | ---: | ---: |
| `collections_lib` | 26 | 21 | 47 |
| `filesystem_lib` | 22 | 24 | 46 |

Every failure was `E-R06` (exit 106) with correct stdout — the probes computed
the right answers and then leaked.

## 2. Ownership traces

Traced against `stdlib/collections.mink`, `stdlib/filesystem.mink`,
`src/runtime/intrinsics.rs`, `src/backend/ir.rs` and the existing passing tests
(`s106_*`, `d01`–`d11`), not inferred from names:

| Value | Contract | Evidence |
| --- | --- | --- |
| `rt_vec_new` handle | owned by caller; leaked unless `rt_vec_free` | `via.rs` docs; `s106_*` tests free before exit |
| `rt_vec_free(v)` | releases the backing buffer **and element strings** | `docs/language/STANDARD_LIBRARY_REFERENCE.md:274`; verified by new `s191_vec_free_releases_element_strings` |
| `rt_vec_push/set/remove` | consumes `v`, returns the (possibly reallocated) handle | `v = rt_vec_push(v, …)` in every passing test |
| `rt_vec_pop` | returns the element value, not an owned handle | `v07`/`v20` use it as an `Int` |
| `path_join(a, b)` | consumes both `Str` args (frees them) and returns an owned `Str` | `filesystem.mink` ends with `rt_str_free(a); rt_str_free(b); return result;` |
| `path_parent/filename/extension/stem/with_extension` | return owned `Str` | each allocates a result and frees its argument |
| `rt_fs_read`, `rt_fs_get_cwd` | return owned `Str`; intrinsic does not consume args | `p24`/`p42`; leak scanner counts the result |
| `rt_str_from_int` | returns owned `Str` | `p24`/`p43`/`p45` |

**The fix is not uniform.** In `p34` and `p44` an intermediate `path_join`
result is passed straight into the next `path_join`, which frees it. A blanket
"free every binding" edit would have double-freed those two. Only the final
result is freed there, with an inline comment recording why.

Otherwise all 45 failures were the same defect: the probe discarded an owned
value. That matches the previous session's "mechanical" observation, with the
two consumed-intermediate exceptions above.

## 3. Fixes

- 21 collections probes now call `rt_vec_free(v)` before `rt_exit(0)`.
- 24 filesystem probes now release `path_*` results, `rt_str_from_int` results,
  `rt_fs_read` results and `rt_fs_get_cwd` results.
- Both `assert_success` helpers require `code == 0`. No wrapper, no altered exit
  code, no runtime check weakened.
- No expected output changed. All value assertions are untouched and now run
  unconditionally after a strict success check.

## 4. New permanent regression tests

| Test | Asserts |
| --- | --- |
| `collections_lib::s191_unfreed_vec_is_a_leak` | discarding a `rt_vec_new` handle exits **106** with `E-R06` |
| `collections_lib::s191_vec_free_releases_element_strings` | a vec holding a heap `Str` exits **0** after `rt_vec_free`, proving element strings are released |
| `filesystem_lib::p46_unfreed_fs_read_result_is_a_leak` | discarding an `rt_fs_read` result exits **106** with `E-R06` |

The two negative tests prove the tolerances were hiding real leaks; the
positive test distinguishes "freed the buffer" from "freed the elements".

## 5. Verification

| Check | Result |
| --- | --- |
| `cargo test --test collections_lib` | **49 passed, 0 failed** |
| `cargo test --test filesystem_lib` | **47 passed, 0 failed** |
| Full `cargo test` | exit 0 — **3,180 passed, 0 failed, 1 ignored, 78 suites** |
| `rustfmt --check` on both files | clean |
| `git diff --check` | exit 0 |
| Remaining `\|\| 106` / `== 106` | 0 outside explanatory comments; `d11` keeps its deliberate `assert_eq!(code, 106)` for the unclosed-handle negative test |

The total rose from 3,177 to 3,180 because of the three new tests.

## 6. Remaining ownership work

`strings_lib` still holds **10** `== 106` tolerances, including the arena-owned
JSON results (`s70`, `s72`, `s73`). Those are not the same defect class and are
Session 192's subject; no change was made to them here.

## 7. Status

Collections and filesystem are strictly enforced. Every previously tolerated
failure was a caller-side leak, now fixed and covered by both a positive and a
negative test. The suites' correctness assertions were never skipped behind an
exit-code guard.

## 8. Exact next action

Session 192: inventory all ten `strings_lib` tolerances, establish whether
`json_as_str` / `str_index_of` return borrowed arena views or heap strings, and
resolve the arena lifetime contract before removing any tolerance.
