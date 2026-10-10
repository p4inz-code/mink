# A2 — treeops (file automation tool)

PROJECT: A2 file automation
PURPOSE: Recursively copy/move a filtered subset of a source tree into a
destination tree, preserving relative structure, with a deterministic sorted
report, extension filters, dry-run and limit — the "sync a folder" task a
Python developer reaches for with `os.walk` + `shutil.copy2`.
MINK SOURCE: `validation/a2_treeops/main.mink` (69 functions emitted)
PYTHON COMPARISON: `validation/a2_treeops/python_ref.py`
CROSS-CHECK HARNESS: `validation/a2_treeops/verify.py`
FIXTURES: `validation/a2_treeops/fixtures/make_fixtures.py` (18 files, 4 nesting
levels, spaces in names, mixed case, empty file, binary files, dotfile dir)

BUILD: PASS — `mink build validation/a2_treeops/main.mink` → `main.exe`
(x86_64-windows-pe, no external toolchain, no runtime DLL).

EXECUTION: PASS — 18/18 cases produce byte-identical stdout, stderr, exit
codes **and** byte-identical destination/source directory trees compared with
the Python reference (`python verify.py` → `18/18 cases match`).

FUNCTIONAL CORRECTNESS: PASS — recursive walk (4 levels deep), extension
filter (case-insensitive, comma list), copy and move modes, dry-run,
post-sort `--limit` (deterministic because directory enumeration order is
unspecified), relative-path preservation including `with space/read me.txt`,
sorted report lines byte-identical to Python's `str` ordering.

ERROR HANDLING: PASS — distinct exit codes verified against Python: 1 usage
(no args, missing option value, too many positionals), 2 source missing /
not a directory, 3 destination not creatable, 4 nothing matched. Messages go
to stderr (`rt_stderr_write`), stdout stays clean.

MEMORY: PASS — the runtime leak check (`E-R06`, exit 106) is enabled; all 18
cases exit 0/1/2/3/4, never 106, including repeat execution (run twice) and
every failing path.

REALISTIC INPUT: PASS — 18-file tree with nested directories, file names
containing spaces, mixed-case extensions, an empty file, binary `.png`/`.bin`
payloads, plus a source-tree prefix (`src_subdir_prefix`) that must be
stripped correctly when building relative targets.

REPEATED EXECUTION: PASS — `repeat_execution` runs the same copy twice and
compares both runs plus both resulting trees; existing destination is accepted
(`fs_create_dir` returns 1 when the directory already exists — not an error).

STANDALONE EXE: PASS — `main.exe` runs with no interpreter and no runtime;
the only dependency is the Windows API.

WINDOWS: PASS.

MINK LIMITATION: see CLASSIFICATION; none of the findings blocked the
application.

CLASSIFICATION:
- D (deliberate difference) — `rt_print_str` appends CRLF without translating
  embedded newlines; the Python reference writes raw bytes + `\r\n` to match.
- D (deliberate difference) — `rt_argc`/`rt_argv` exclude the program name;
  argv[0] is the first user argument in both implementations.
- D (deliberate difference, now improved) — the ownership checker was
  flow-sensitive but **not path-sensitive**; application code funnels to a
  single exit and releases each owned string exactly once.
- B (defect, FIXED this session) — the ownership analysis was branch-blind:
  state was carried straight through `if/else` and `match` arms, so
  `rt_str_free(x)` on an early-exit path in one arm caused false
  `E-S10: use of moved value` in sibling arms. Fixed in
  `src/ownership/mod.rs` by making `walk_if`/`walk_if_match` branch-local with
  a conservative join (`snapshot_flow`/`restore_flow`/`join_flow`).
  Regression tests: `tests/ownership.rs` — 6 new tests
  (`branch_state_is_isolated_between_if_arms`,
  `branch_state_is_isolated_in_loops_and_nested_if`,
  `branch_join_still_reports_a_move_after_the_if`,
  `branch_join_reports_a_move_from_the_else_arm`,
  `match_arms_do_not_see_each_others_moves`,
  `native_branch_local_move_program_runs`); 49/49 ownership tests pass.
- B (defect, FIXED this session) — `rt_fs_set_cwd`'s Windows emitter returned
  the raw Win32 BOOL (1 = success, 0 = failure), i.e. success and failure were
  swapped relative to its documented `0=ok` contract and relative to every
  other fs mutator. Fixed in `src/backend/emit/runtime.rs`
  (`emit_fs_set_cwd` now inverts the BOOL like `emit_fs_create_dir`);
  `src/backend/ir.rs` doc comments corrected to the actual shared convention
  `0=ok, 1=error` (the emitters always returned 1 on error, never -1).
  Regression test: `tests/filesystem_lib.rs`
  `p42b_fs_mutators_return_zero_on_success_one_on_error` (46/46 pass).
- C (architectural limitation) — no module-level `Vec` globals:
  `let mut items = rt_vec_new(8);` at module scope fails with
  `E-B03 (module bindings of type Vec<Str> are not supported)`. Vectors must
  be function locals, threaded through untyped `out` parameters.
- C (architectural limitation) — no `clone` intrinsic; duplicate a `Str` with
  the borrowing intrinsic `rt_str_concat(s, "")`.
- C (ergonomic trap, documented) — `rt_fs_*` read intrinsics
  (`rt_fs_file_size`, `rt_fs_exists`, ...) do **not** free their arguments;
  binding-and-freeing is required or the run ends with `E-R06` (exit 106).
  The stdlib wrappers (`fs_read`, `fs_copy_file`, `fs_move`, `fs_create_dir`,
  ...) do consume their arguments and are leak-free.
- A (application defect, found & fixed during this validation) — the first
  version only called `fs_create_dir(dst)` once, so nested targets
  (`dst/a/b/c/...`) failed to copy (CopyFileA does not create directories)
  and the error was invisible because it checked `moved == -1` while the
  runtime reports `1`. Fixed with a recursive `make_dirs` (mkdir -p) helper
  and `moved != 0`; verify.py then went from 8/18 to 18/18.
- D (convention) — sorting `Vec<Str>` in place needs an explicit insertion
  sort built on `str_cmp` (which consumes both args, so pass
  `rt_str_concat(..., "")` copies); byte order equals Python `str` order for
  UTF-8.

EVIDENCE:
- TEST — `validation/a2_treeops/verify.py`, 18 cases, 0 differences
  (stdout, stderr, exit code, destination tree, source tree).
- FIXTURE — `validation/a2_treeops/fixtures/make_fixtures.py` (18 files).
- REGRESSION — compiler defect + fix covered by `cargo test --test ownership`
  (49 passed) and `cargo test --test filesystem_lib` (46 passed, includes the
  new `p42b` test); `cargo test --lib` 121 passed; `windows_hardening` 13 +
  `smoke` 22 + `process_lib` 86 all passed after the runtime change.

## Transcript

```
$ python validation/a2_treeops/fixtures/make_fixtures.py
fixtures: 18 files under .../validation/a2_treeops/fixtures/src

$ mink build validation/a2_treeops/main.mink
mink: build: 'validation/a2_treeops/main.mink' -> '.../main.exe'
(target: x86_64-windows-pe, 69 function(s), 5 binding(s))

$ python validation/a2_treeops/verify.py
ok   default_copy (exit 0)
ok   repeat_execution (exit 0)
ok   dry_run (exit 0)
ok   move (exit 0)
ok   ext_png_bin (exit 0)
ok   no_match (exit 4)
ok   limit_3 (exit 0)
ok   missing_src (exit 2)
ok   src_is_file (exit 2)
ok   no_args (exit 1)
ok   too_many_positionals (exit 1)
ok   ext_missing_value (exit 1)
ok   limit_missing_value (exit 1)
ok   limit_invalid_value (exit 0)
ok   move_and_dry_run (exit 0)
ok   empty_src (exit 4)
ok   dst_is_file (exit 3)
ok   src_subdir_prefix (exit 0)

18/18 cases match the Python reference

$ main.exe --ext .log,.txt,.json work/demo/src work/demo/dst   (manual)
processed a/b/c/d/four_levels.log 9
... (11 lines)
matched: 11
bytes: 164
skipped: 7
errors: 0
exit=0

$ main.exe --ext .xyz work/demo/src work/demo/dst2             (no match)
matched: 0 / bytes: 0 / skipped: 18 / errors: 0
exit=4
```

## Python comparison notes (concrete differences, no ranking)

1. Python's `os.walk` yields entries in OS order; MINK's `fs_dir_next` order
   is unspecified, so the app collects matches into a `Vec<Str>` and sorts
   them (insertion sort over `str_cmp`) to get Python's deterministic
   `list.sort()` behaviour.
2. Python's `os.makedirs(parent, exist_ok=True)` maps to a hand-written
   recursive `make_dirs` because the stdlib has no `mkdir -p` (zip.mink has a
   private `_zp_mkdir_p`, not exported).
3. Python removes an existing destination before `os.rename`;
   `MoveFileA` fails if the target exists, while `CopyFileA` overwrites
   (`bFailIfExists = FALSE`) — so repeat execution works in copy mode only,
   matching the reference (the reference does the same).
4. Python's `try/except OSError` maps to status-code checks
   (`rt_fs_*` return `1` on error); there is no exception type.
5. `fs_create_dir` returns **1 when the directory already exists** — the app
   must treat that as success, exactly like `os.makedirs(exist_ok=True)`.
