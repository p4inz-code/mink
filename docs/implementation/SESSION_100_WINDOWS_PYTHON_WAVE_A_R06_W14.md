# Session 100 — Windows Python Parity Wave A: R06 Error Locations + W14 Home Dir

## 1. Session identity

- Starting commit: `9ed9f52` (Session 99 final, clean tree)
- Final commit: `d490c86`
- MINK version: 1.0.1
- Target: Windows x86_64 only. **Linux remains FROZEN** — no Linux behavior modified.
- Baseline test count (Session 99): 2560 passed / 0 failed

## 2. Capabilities implemented

| # | Capability | Matrix row | Status | Evidence |
|---|---|---|---|---|
| 1 | Runtime error-location metadata | R06 | **VERIFIED** | `[test] tests/session100.rs` (9 tests), `[exec] examples/runtime_error_report/` |
| 2 | Home-directory discovery | W14 | **VERIFIED** | `[test] tests/session100.rs` (5 tests), native runs in 4 env scenarios |

## 3. R06 implementation architecture

Goal: when a runtime fault occurs, print the exact source file and line
instead of only the generic `mink: runtime error[E-Rxx]: msg` line.

Design (smallest correct mechanism):

1. **BSS `fail_loc` cell** (`src/runtime/abi.rs`): an 8-byte runtime cell.
   Generated code stores a location-table index into it immediately before
   every operation that can raise a runtime error (generated bounds checks
   and runtime-intrinsic calls). `0` means "no location recorded".
2. **Fail-site instrumentation** (`src/backend/emit/runtime.rs`,
   `src/backend/emit/x86_64.rs`): the PE emitter resolves each instrumented
   span against the compilation's `SourceMap` and records a
   `(file-path-offset, line)` entry. `emit()` now receives the
   `SourceMap` (`src/backend/emit/mod.rs`, `src/backend/mod.rs`) — the ELF
   path is untouched.
3. **Embedded location table**: file path strings + a compact per-site
   table live in the image's `.data`; they survive into the standalone
   `.exe` (no source files needed at run time).
4. **`rt_fail` printing** (`src/backend/emit/runtime.rs`): after printing
   the error line, reads `fail_loc`; when non-zero it prints
   `  at <file>:<line>`. Errors not tied to one user operation (e.g. the
   exit-time leak scan) clear the cell first, so the runtime never
   fabricates a location.
5. **Determinism contract**: the image embeds the source path exactly as
   given to the compiler (like debug info). Four byte-identical golden
   tests that previously compiled from two *different* unique temp paths
   now compile twice from the *same* fixed path (`tests/backend.rs` ×2,
   `tests/function_annotations.rs`, `tests/let_annotations.rs`,
   `tests/scalar_types.rs`).

Verified behaviors (native execution):

- Runtime fault reports exact file and line (E-R10 at `main.mink:20`).
- A failing function reports its **own** line, not the caller's line.
- The location survives: build → copy executable to empty directory → run
  (no source files, no cwd dependency).
- Paths containing spaces are reported intact.
- Leak-check failures print no location (correctly — no single site owns
  the leak).
- Success paths print nothing.
- Repeated execution is deterministic (same exit code, same stderr).

## 4. W14 implementation architecture

`rt_home_dir() -> Str` intrinsic (`src/runtime/intrinsics.rs`,
`src/backend/ir.rs` RuntimeService::HomeDir, `src/backend/lower.rs`),
emitted in `src/backend/emit/runtime.rs` (`emit_home_dir`):

- Resolution order: `USERPROFILE`; if unset/empty, `HOMEDRIVE` +
  `HOMEPATH`; if neither, an owned empty `Str`.
- Mirrors the verified `emit_env_get` query/fill pattern (raw size
  preserved across the `StrAlloc` call; exact-length result).

Bugs found and fixed during native verification (these were real defects
in the first draft, found only by executing generated PEs):

1. **Missing `test rax,rax`** after the `GetEnvironmentVariableA` fallback
   queries — the `je` relied on ZF left by kernel32 (undefined). With
   `HOMEDRIVE` missing, `rax=0` but ZF was stale → `StrAlloc(-1)` → E-R08.
2. **Dead copy loops** — `jmp` guards jumped *past* the loop condition, so
   the drive/path payload copies never executed → all-NUL results in the
   HOMEDRIVE/HOMEPATH branches.

Verified scenarios (native, via `env -u` / `rt_env_set`-driven tests):

- USERPROFILE set → returned value equals USERPROFILE.
- USERPROFILE missing, HOMEDRIVE+HOMEPATH set → concatenated correctly.
- HOMEDRIVE only → owned empty string (Windows semantics: drive alone is
  not a usable home; documented).
- Both missing → owned empty string, clean exit.
- Repeated calls → no leak, deterministic.

## 5. Tests added

`tests/session100.rs` — 14 tests (9 R06 + 5 W14), all native-execution
tests that build real MINK programs, run the generated `.exe`, and assert
stdout/stderr/exit codes:

- R06: `runtime_error_reports_file_and_line`,
  `called_function_reports_its_own_line`,
  `failing_access_reports_its_own_line_after_valid_ones`,
  `string_byte_error_reports_file_and_line`, `exit_leak_reports_no_location`,
  `success_path_reports_nothing`, `copied_executable_still_reports_location`,
  `source_path_with_spaces_reports_intact`, `repeated_execution_is_deterministic`
- W14: `home_dir_returns_userprofile`,
  `home_dir_falls_back_to_drive_plus_path`,
  `home_dir_drive_without_path_is_empty`,
  `home_dir_with_nothing_set_is_empty_and_clean`,
  `home_dir_repeated_calls_are_clean`

Test design notes: raw Rust strings (`r#"..."#`) avoid escape-layer bugs;
the W14 tests switch environment branches **mid-process** via
`rt_env_set`/`rt_env_remove` and restore the original `USERPROFILE`/
`HOMEDRIVE`/`HOMEPATH` values afterwards.

## 6. Proof application

`examples/runtime_error_report/` (`main.mink` + `README.md`), committed:

- A diagnostics example that intentionally triggers E-R10 (array index out
  of range) with the failing access on line 20 and the driving call in
  `main` on line 24.
- Native verification: `mink check` → `mink build` → one `main.exe` →
  `mink: runtime error[E-R10]: ...` + `  at <path>\main.mink:20`, exit 110.
- README documents build/run/standalone-copy proof and the location
  format contract (file + line; column intentionally not reported).

## 7. Quality gates

- `cargo fmt --check`: **clean**
- `cargo clippy --all-targets`: **0 errors, 0 new warnings** (lib baseline
  54 warnings confirmed identical to HEAD via stash comparison)
- Full suite: **2574 passed / 0 failed** (2560 baseline + 14 new). Run
  per-binary single-threaded; the two documented parallel-load loopback
  flakes (Session 98 finding) are avoided, all green.
- `cargo build`: clean · `cargo build --release`: **clean** (50 pre-existing
  lib warnings, unchanged)
- Git tree: clean after commit; scratch files (`tests/zz_elf_probe.rs`,
  generated `main.exe`) removed before commit.

## 8. Parity matrix changes

`docs/roadmap/WINDOWS_PYTHON_OFFICIAL_CAPABILITY_PARITY.md`:

- R06: PARTIAL → **VERIFIED** (test + exec refs; remaining limitation:
  no call stack — documented V1 contract).
- W14: MISSING → **VERIFIED** (test refs; APPDATA/known-folders via
  SHGetKnownFolderPath still MISSING — downgraded to P2, Wave H).
- Wave A completion row updated: "13 (+5 shared, all landed S99/S100)".

## 9. P0/P1/P2/P3 status

- P0: 0 · P1: 0 newly introduced · Wave A P1 count now 0.
- Remaining P2/P3: call-stack in error reports (P2, Wave G debugger), APPDATA
  known-folders (P2, Wave H), ANSI-only env/argv (P2/H), column in error
  locations (P3).

## 10. Linux freeze confirmation

No Linux implementation, tests, emitters, or behavior were modified. The
only shared-file change is `emit()` gaining a `SourceMap` parameter; the
ELF path ignores it and Linux emission output is byte-for-byte unchanged
(all Linux-independent shared tests green).

## 11. Files changed (Session 100)

- `src/runtime/abi.rs`, `src/backend/ir.rs`, `src/backend/lower.rs`,
  `src/backend/mod.rs`, `src/backend/emit/mod.rs`,
  `src/backend/emit/runtime.rs`, `src/backend/emit/x86_64.rs`,
  `src/runtime/intrinsics.rs`
- `tests/session100.rs` (new), `tests/backend.rs`,
  `tests/function_annotations.rs`, `tests/let_annotations.rs`,
  `tests/scalar_types.rs`
- `examples/runtime_error_report/` (new)
- `docs/roadmap/WINDOWS_PYTHON_OFFICIAL_CAPABILITY_PARITY.md`,
  `docs/implementation/SESSION_100_WINDOWS_PYTHON_WAVE_A_R06_W14.md`

## 12. Status

- **WINDOWS BASE = COMPLETE / STABLE**
- **WINDOWS PYTHON PARITY = PARTIAL** (Wave A fully landed; 43 P1 blockers
  remain in later waves)
- **LINUX = FROZEN** (untouched)

## 13. Recommended Session 101 objective

Wave B tranche 1 (core language/data): begin the Map/Set and full typed
Vec work (S01/S02) or the text/string utilities tranche (L08/L10/S36),
starting with the Session 98 plan's Wave B sequencing and its quick-win
list (temp dirs via GetTempPath, PATH resolution, error-text table
completion).