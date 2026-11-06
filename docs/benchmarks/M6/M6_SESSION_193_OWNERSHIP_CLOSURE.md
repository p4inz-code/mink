# Session 193 — Cross-Library Ownership Audit and Enforcement Closure

Windows x86_64, 2026-11-06. Starting HEAD
`2496b752f39598380343660df08ad96fbd43429d`.

## 1. How the inventory was rebuilt

Searches over every `tests/*.rs` file, with each hit inspected rather than
counted:

- `|| 106` and `== 106`
- `assert!(… code … || …)` and other multi-code assertions
- `code == N || …` tolerance shapes
- `if code == 0` conditional-assertion gates
- every helper that takes or asserts an exit status (`assert_success`,
  `assert_ok`, `assert_clean`, `run_ok`, …)

The three remaining `|| 106` textual matches are prose in the Session 191/192
records' tests: `tests/collections_lib.rs:1430`, `tests/filesystem_lib.rs:1057`,
`tests/encoding_lib.rs:590`. No executable tolerance remains.

## 2. Complete accounting: 96 → 0

| Suite | Tolerances before | Removed by | Now |
| --- | ---: | --- | ---: |
| `encoding_lib` | 36 | Session 190 | 0 |
| `hashing_lib` | 24 | Session 190 | 0 |
| `crypto_lib` | 18 | Session 190 | 0 |
| `math_lib` | 4 | Session 190 | 0 |
| `random_lib` | 1 | Session 190 | 0 |
| `time_lib` | 1 | Session 190 | 0 |
| `collections_lib` | 1 | Session 191 | 0 |
| `filesystem_lib` | 1 | Session 191 | 0 |
| `strings_lib` | 10 | Session 192 | 0 |
| **total** | **96** | | **0** |

The counts are the number of `== 106` assertions, not the number of tests they
gated: Session 191's single collections tolerance covered 21 tests and its
filesystem tolerance covered 24. The affected-test counts are 21 + 24 in 191
and 10 in 192; the 84 tolerances removed in 190 were distributed across 272
tests in six suites.

## 3. Remaining 106 assertions are the opposite of tolerances

These deliberately require the leak checker to fire. Removing them would make
the suites weaker:

| Site | What it pins |
| --- | --- |
| `async_lib.rs:197` | a spawned task that is never awaited leaks |
| `collections_lib.rs:1441` | an unreleased `rt_vec_new` handle leaks |
| `filesystem_lib.rs:1051` | an unclosed directory handle leaks |
| `filesystem_lib.rs:1071` | an unreleased `rt_fs_read` result leaks |
| `process_lib.rs:1246`, `1348` | unfreed `process_stdout` / `env_get` values leak |
| `runtime.rs:128` | a live allocation at exit is `E-R06` |
| `session100.rs:263` | the `E-R06` exit code itself |
| `stdlib_ownership.rs:437` | a taken ownership guard exits while the value is live |
| `strings_lib.rs:1481`, `1508` | the JSON accessor copy and the arena are separate blocks |

## 4. Correctness assertions are independent of exit status

Every shared harness now requires success before any value assertion:

| Helper | Check |
| --- | --- |
| `encoding_lib::assert_int_op` / `assert_hex_enc` / … | `assert!(code == 0)` |
| `collections_lib::assert_success` | `assert_eq!(code, 0)` |
| `filesystem_lib::assert_success` | `assert_eq!(code, 0)` |
| `random_lib::assert_success`, `time_lib::assert_success` | `assert_eq!(code, 0)` |
| `async_lib::assert_ok`, `re_lib::assert_ok`, `threads_lib::assert_ok`, `zlib_lib::assert_ok`, `zip_lib::assert_ok` | `assert_eq!(code, 0)` |
| `logging_lib::assert_clean`, `httpserver_lib::assert_clean`, `stdlib_ownership::assert_clean` | exit 0 (some also require empty stderr) |

`encoding_lib` and `math_lib` still contain 40 `if code == 0 { … }` blocks
around their value assertions. They are **vestigial, not gates**: an
`assert!(code == 0, …)` precedes every one, so a non-zero exit panics before
the condition is evaluated. A line-order check over both files confirmed none
is reachable without that preceding assert. Removing 40 wrappers would be pure
churn and was not done.

No helper converts a failure into exit 0, and no test was removed or renamed to
reduce failures.

## 5. Ownership model (as established and tested in 190–192)

| Category | Contract | Release |
| --- | --- | --- |
| Heap `Str` | returned by `rt_str_alloc`, `rt_str_concat`, `rt_str_from_*`, `str_*` transforms, `fs_read`, `json_as_str`/`json_obj_get_key`/`json_num_str`/`json_to_str` | `rt_str_free` exactly once |
| Heap block | `rt_alloc` (JSON arena, thread/lock state) | `rt_free` exactly once |
| Collection handle | `rt_vec_new` / `rt_map_new` / `rt_set_new` | `rt_*_free`, which also releases element strings |
| Borrowed | `rt_print_str`, `rt_str_len`, `rt_str_byte`, `rt_str_eq`; string literals are immortal | never freed |
| User `Str` parameter | consumed (moved) by the callee | callee |
| JSON accessors | **copy** the arena bytes into a new heap `Str` | caller frees each; results outlive `rt_free(arena)` |
| JSON arena | one `rt_alloc` block owned by the caller | `rt_free(arena)`; no implicit lifetime |

The exit scan sees ordinary live blocks; it has no arena exception, and none is
needed because the accessors are copies.

## 6. Regression tests added during the closure

| Session | Tests |
| --- | --- |
| 191 | `s191_unfreed_vec_is_a_leak`, `s191_vec_free_releases_element_strings`, `p46_unfreed_fs_read_result_is_a_leak` |
| 192 | `s192_json_as_str_survives_arena_free`, `s192_json_as_str_result_unfreed_is_a_leak`, `s192_json_arena_not_freed_is_a_leak` |

Session 193 added no test: it is an audit, and its findings are covered by the
above plus the existing per-suite leak tests. It did correct one stale comment
in `encoding_lib` that still claimed the UTF-8 tests tolerated 106.

## 7. Verification — current run, not historical

| Check | Result |
| --- | --- |
| Full `cargo test` | exit 0 — **3,183 passed, 0 failed, 1 ignored, 78 suites** |
| `collections_lib` | 49 passed |
| `filesystem_lib` | 47 passed |
| `strings_lib` | 88 passed |
| `crypto_lib` | 18 passed |
| `encoding_lib` | 75 passed |
| `hashing_lib` | 29 passed |
| `math_lib` | 106 passed |
| `random_lib` | 15 passed |
| `time_lib` | 29 passed |
| `stdlib_ownership` | 11 passed |
| `json` | 68 passed |
| `runtime` | 28 passed |
| Release build | `cargo build --release` exit 0, `target/release/mink.exe` 4,028,416 bytes |
| `invsync` (Session 180 verifier) | **12/12 PASS**, exit 0 |
| `rustfmt --check` on changed files | clean |
| `git diff --check` | exit 0 |
| Remaining exit-106 tolerances | **0** |

The ignored test is pre-existing and unchanged.

## 8. Known limitations

- No path-sensitive ownership: a value freed inside a branch is treated as
  moved for the rest of the function. The `stdlib_ownership` guard test pins the
  behaviour.
- A collection used after `rt_*_free` is not diagnosed; the handle is dead but
  the runtime does not track it.
- `encoding_lib`/`math_lib` keep the vestigial `if code == 0` wrappers
  (documented above).
- Leak enforcement is proven through process exit codes; `E-R06` text is
  asserted only where the harness captures stderr.
- Windows x86_64 only. Linux is frozen and not supported; macOS is not
  supported.

## 9. Deferred work

- **M6 external developer validation** — requires a developer who did not build
  MINK; cannot be closed by repository work.
- **M7 ecosystem/tooling** — gated on demonstrated demand by the roadmap.
- Linux parity — explicitly out of scope.

## 10. Is standard-library leak enforcement complete?

Yes. Every suite that compiles and runs MINK programs now requires exit 0; all
96 historical tolerances are gone, not text-replaced: each removed tolerance
was followed by a concrete ownership fix or a correction of a stale assumption,
and the three suites left open by Session 190 are strict and green under a
tested contract. The two negative test families (unfreed-vector, unfreed-JSON)
prove the checker still fires, so "zero tolerances" is a measured state rather
than a search-count artifact.
