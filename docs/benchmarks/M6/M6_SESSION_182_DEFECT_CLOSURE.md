# M6 Session 182 - defect closure

**Date:** 2026-11-05
**Status:** **COMPLETE**
**Starting HEAD:** `be4d115d2f13c0f48ee31bb315f77c4e6c57b750`
**Branch:** `main`
**Upstream:** `origin/main` at `b860837a8a6039f304a503e3d85728a643ab470b`
**Starting worktree:** 63 entries (37 modified, 26 untracked)

This session investigated the four blockers recorded by Sessions 178-181. All
four now have a root cause, a reproduction and a verified disposition. No
compiler, runtime, Linux or release code was changed.

## A1. `stdlib/environment.mink` could not be loaded

**Original evidence.** `mod environment;` failed with `E-P01` pointing at
`stdlib/environment.mink:26:1`.

**Reproduction.** The pre-session file was extracted with
`git show HEAD:stdlib/environment.mink` into `tmp/s182/oldenv/` and loaded by a
one-line `main`. The build failed:

```text
mink: error[E-P01]: expected a top-level declaration (`fn`, `let`, or `const`)
  --> tmp/s182/oldenv/main.mink:3:1
```

**Root cause.** The module contained five top-level `extern fn` declarations.
The parser's top-level item set is `fn`, `let`, `const`, `mod`, `use`,
`struct`, `enum`; `extern` is not an item. Every other standard-library module
calls its runtime intrinsics directly without declaring them, because the
intrinsics are already predeclared by the compiler. `environment.mink` was the
only module using the unsupported form, so this was a defect in the module,
not in module loading.

**Resolution.** Option A (repair): the `extern` declarations were replaced by
ordinary wrapper functions over the predeclared intrinsics.

| Wrapper | Intrinsic |
| --- | --- |
| `env_get(name: Str) -> Str` | `rt_env_get` |
| `env_set(name: Str, value: Str) -> Int` | `rt_env_set` |
| `env_has(name: Str) -> Bool` | `rt_env_has` |
| `env_remove(name: Str) -> Int` | `rt_env_remove` |
| `env_cwd() -> Str` | `rt_fs_get_cwd` |

The intrinsics remain callable directly without the module, so existing code
is unaffected. The module now uses the unprefixed naming convention of the
rest of the standard library.

**Files changed.** `stdlib/environment.mink` (repaired),
`tests/environment_lib.rs` (new).

**Regression.** Two integration tests build a real program with
`mod environment;`, call every wrapper, free both returned strings, and assert
exit 0. Before the repair the build fails with `E-P01`; after it both pass.

**Before/after.** Build 1 -> 0. Run output `ok` / `cwd-ok`, exit 0. Regression
target: **2/2 passed**.

**Disposition.** RESOLVED. No remaining limitation for this module.

## A2. ZIP reader "leak"

**Original evidence.** A composite ZIP reader probe exited 106 with
`E-R06: memory leak`.

**Per-call matrix.** Each probe builds a two-entry archive, performs exactly
one reader operation, releases every owned return, and exits. `ignored` is the
negative control that deliberately drops the pass-back string.

| Probe | Repository 1.0.4 build/run | Installed 1.0.2 build/run |
| --- | --- | --- |
| `zip_read_count`, copy freed | 0 / **0** | 0 / **0** |
| `zip_read_count`, copy ignored | 0 / **106** | 0 / **106** |
| `zip_list`, vector freed | 0 / **0** | 0 / **0** |
| `zip_entry_name`, name freed | 0 / **0** | 0 / **0** |
| `zip_entry_size`, copy freed | 0 / **0** | 0 / **0** |
| `zip_entry_data`, payload freed | 0 / **0** | 0 / **0** |
| composite, every return freed | 0 / **0** | 0 / **0** |

**Root cause.** Three readers are pass-back functions: `zip_read_count`,
`zip_entry_size` and `zip_entry_is_dir` consume the archive and return a
*fresh copy* of it as the second tuple element. The Session 178-181 probes
bound that copy (`derr`, `size_back`, `dir_back`) and never freed it, so one
allocation survived to exit. The original `p_zip.mink` also reused `data`
after `zip_list` had consumed it, and `p_zip3.mink` called `zip_free` after
`zip_finish` had already freed the builder (`E-R04`). The composite probe
additionally passed a `Bool` to `rt_print_int` (`E-T01`), which stopped that
variant before any leak could be observed.

**Resolution.** No `stdlib/zip.mink` change. The documented ownership rules
are sufficient: free every returned `Str`, free the `Vec<Str>` from
`zip_list`, and never call `zip_free` after `zip_finish`. The composite and
per-call probes now exit 0 on both compilers.

**Files changed.** `tests/zip_lib.rs` (new regression
`s182_reader_owned_returns_are_all_released`).

**Regression.** The test frees all six owned returns in one program and
requires a clean exit; a missed release becomes `E-R06`/106. Zip target:
**11/11 passed**.

**Disposition.** NOT A PRODUCT DEFECT. Probe ownership error; documented and
pinned. No suppression of leak checking was introduced.

## A3. Nested `Vec<Str>` "corruption"

**Original evidence.** A `Vec<Str>` pushed once inside nested `if/else` was
reported with length 16; `rt_vec_free` then produced `E-R04`.

**Reproduction from the original file.** `tmp/s178/var/a.mink` frees `order`
inside the success branch (line 196), then prints `rt_vec_len(order)` at line
218 and frees it again at line 221. The value is read after its release and
then double-freed.

**Minimal reproduction.** `tmp/s182/v_vec_double_free.mink`:

```mink
fn main() -> Int {
    let mut v = rt_vec_new(2);
    v = rt_vec_push(v, "hello");
    rt_print_int(rt_vec_len(v));   // 1
    rt_vec_free(v);
    rt_print_int(rt_vec_len(v));   // stale: 48 on this run
    rt_vec_free(v);                // E-R04
    return 0;
}
```

Observed: `1`, then `48`, then
`E-R04: invalid free: the pointer is not the start of a live allocation`,
exit **104** on both compilers. The earlier value 16 was the allocator's
reused header for that particular heap state, not a vector element count.

**Corrected shape.** The same nested push with exactly one release prints `1`
and `fasteners`, exit 0. The compiler, MIR ownership lowering and runtime
vector are consistent; the defect was in the probe.

**Resolution.** No compiler or runtime change. The regression keeps the
nested-branch shape but releases exactly once, so a real lifecycle regression
would fail it.

**Files changed.** `tests/collections_lib.rs` (new regression
`s182_vec_str_nested_push_single_release`).

**Regression.** Collections target: **47/47 passed**.

**Disposition.** INCORRECT TEST ASSUMPTION. No product defect; the original
report is withdrawn with evidence.

## A4. Compiler 1.0.2 versus repository 1.0.4

**Evidence.**

| Item | Repository | Installed npm package |
| --- | --- | --- |
| `--version` | `mink 1.0.4` | `mink 1.0.2` |
| Binary SHA-256 | `d8f61348...7149f09` | `6b33b2f6...2447076` |
| Binary size | 4,028,416 bytes | 3,890,688 bytes |
| Bundled `strings.mink` | ownership fixes present | 127 diff lines absent, including the `rt_str_free` releases |
| Registry (`npm view`) | **1.0.4** | - |

**Minimal reproduction.** `str_trim(rt_str_concat("  hi  ", ""))`:

| Compiler | Exit |
| --- | --- |
| Repository 1.0.4 | **0** |
| Installed 1.0.2 | **106** (`E-R06`) |

`v_map_replace`, `v_split`, `v_parse` and `v_csv` were clean on both
compilers. The failure is specifically the pre-1.0.3 ownership fix in
`stdlib/strings.mink`: 1.0.2 consumes the heap argument but does not release
it.

**Application.** The Session 180 verifier is 12/12 with 1.0.4 (exit 0). With
1.0.2 it is 1/12: the normal case exits 106. The application source is
unchanged; the difference is the old bundled standard library.

**Resolution.** Documented limitation, no code or package change. 1.0.2 is
unsupported and predates the ownership fixes. The minimum verified compiler
for the M6 artifacts is **1.0.4**. The registry now serves 1.0.4, so a fresh
install receives a supported version; the local global install is stale.

**Disposition.** EXPLAINED VERSION LIMITATION. No backward-compatibility claim
is made.

## B. Integrated regression

| Check | Result |
| --- | --- |
| `cargo test --test environment_lib` | **2/2 passed** |
| `cargo test --test zip_lib` | **11/11 passed** |
| `cargo test --test collections_lib` | **47/47 passed** |
| Combined focused targets | **60 passed, 0 failed** |
| Session 180 verifier, repository 1.0.4 | **12/12 cases**, exit 0 |
| Session 180 verifier, installed 1.0.2 | **1/12**, normal case 106 |
| README complete MINK fences | **5/5 compile** |
| Reference complete MINK fences | **12/12 compile** |
| Local links under `docs/language/` and `docs/benchmarks/M6/` | **0 broken** |
| `git diff --check` | exit 0 |
| `rustfmt --check` on the three test files | exit 0 |

Normal, boundary and failure paths were exercised by the existing Session 180
12-case verifier and the three test targets. Leak and invalid-free behavior
was checked by the runtime itself (exit 106/104), not suppressed.

## C. Commits and preservation

| Commit | Contents |
| --- | --- |
| `ea63358` | environment module repair + `tests/environment_lib.rs` |
| `09f05cb` | zip reader and nested Vec lifecycle regressions |
| `e9c081d` | standard-library reference and debugging guide corrections |
| `02925a9` | README: two-line module/collection correction only |

The README commit was built from the HEAD blob plus the two corrections and
staged with `git update-index`, so the file's pre-existing 20-insertion /
12-deletion worktree changes were not committed. After the commit the working
README diff returned to exactly that pre-existing 20/12 diff.

Worktree after these commits: **63 entries**, the original set, with
`README.md` still modified by its pre-existing edits.

## D. Remaining limitations after Session 182

1. Collection use-after-free is still not diagnosed
   (`rt_map_free(m); rt_map_get(m, 1);` compiles and returns stale data). It is
   documented as an unsupported contract in `LANGUAGE_BASICS.md` and the
   standard-library reference.
2. Installed 1.0.2 remains unsupported; only 1.0.4 is verified.
3. The README's pre-existing published-state paragraph still says the registry
   serves 1.0.3 and that 1.0.4 is pending. `npm view` now reports 1.0.4. The
   paragraph belongs to the pre-existing uncommitted README work and was not
   rewritten.
4. Standard-library modules not executed in Session 178 remain source-only
   evidence.

## E. Duration and next action

- First Session 182 artifact: Unix `1793878655`.
- Last verification before writing this record: Unix `1793879062`.
- Measured artifact window: **407 seconds (6 min 47 s)**.
- Baseline reading and reasoning before the first probe were not instrumented;
  final record/ledger commits add a small amount after this measurement.

Next: **Session 183 - M6 final acceptance**, using the evidence above. Do not
advance to M7.
