# Session 99 — Windows Python Parity: Wave A Tranche 1

- Starting commit: `da2e3cc` (clean; Windows base COMPLETE/STABLE)
- Final commit: see git log
- Linux: FROZEN — no Linux implementation modified (Linux-side stubs below
  are compile-totality registrations only, explicitly marked)

## 1. Capabilities implemented and execution-verified

All of the following went through the full chain: source -> compiler ->
native PE -> Windows execution (and, where applicable, -> npm package ->
clean install -> public CLI -> standalone .exe).

| Capability | Intrinsic(s) | Windows mechanism | Tests |
|---|---|---|---|
| Environment get/has/set/remove | `rt_env_get/has/set/remove` | `GetEnvironmentVariableA` / `SetEnvironmentVariableA` | `tests/session99.rs` (4 env tests incl. free-list regression) |
| argv | `rt_argc`, `rt_argv(i)` | `GetCommandLineA` parse (lazy, quoted/empty/space rules) | `tests/session99.rs` |
| stdin read | `rt_stdin_read()` | ReadFile on stdin handle to EOF | `tests/session99.rs` |
| Sleep | `rt_sleep(ms)` | kernel32 `Sleep` | `tests/session99.rs` |
| stderr write | `rt_stderr_write(s) -> Int` | WriteFile on stderr handle | `tests/session99.rs` |
| float -> Str | `rt_str_from_float(f)` | write-redirect sink reusing the exact `rt_print_float` dtoa path | `tests/session99.rs` |
| str_format | `rt_str_format(fmt, a, b, c)` | two-pass scan: pass 1 measures, pass 2 fills; `{{`/`}}` escapes; missing args dropped | `tests/session99.rs` |
| stdlib in npm | — | `npm/mink/stdlib/*.mink` (16 modules) shipped in the package | clean-install proof below |
| module search path | — | `mod` resolution: source-dir sibling -> `<exe>/../stdlib` -> `cwd/stdlib` (`src/driver.rs` `resolve_module_path`) | `tests/modules_check.rs` + npm-layout build proof |

### Not implemented this session (classified honestly)

- **Runtime error-location metadata (matrix R06)**: DEFERRED. The runtime
  fault path is a fixed E-Rxx message table; adding per-function
  source-line metadata is a fresh emitter feature (~200+ lines) that did
  not fit safely in this session's budget after the env_set bug was
  root-caused. It is the explicit next-session task (see §8). R06 stays
  PARTIAL in the matrix with the deferral recorded.
- **USERPROFILE home discovery**: not needed by any implemented
  capability; deferred to the environment completion wave.

## 2. Root-cause: `rt_env_set` failure under free-list reuse (fixed)

**Symptom:** `rt_env_set("NAME", value)` returned -1
(`ERROR_INVALID_PARAMETER`, 87) only when the value's C-string landed on
a *reused* heap block; fresh-bump allocations always worked.

**Root cause:** `rt_to_cstr` wrote the NUL terminator with
`mov_mem_imm32(R9, 0, 0)` — an **8-byte zero store** — at `buf[len]`.
When `buf` sat at the end of its 16-byte block, the 4 extra zero bytes
spilled into the adjacent block, zeroing the first 4 bytes of the *name*
CStr ("MINK..." -> ""). With an empty name, `SetEnvironmentVariableA`
fails with error 87. Fresh-bump layouts masked it (the overrun landed in
unused zeroed BSS); free-list reuse made it hit a live neighbor.

**Fix:** `mov_mem_imm8(R9, 0, 0)` — a single-byte NUL. Verified against
the exact checkpoint repro plus variants (small->big and big->small free
order, literal and heap-built values). Note: the frozen Linux emitter
has the same pre-existing 8-byte-NUL pattern at `linux_runtime.rs`; it is
documented, not modified (Linux is frozen by policy).

## 3. Key implementation architecture

- **New intrinsics** registered in `src/runtime/intrinsics.rs`
  (stable order, appended); `RuntimeService` variants in
  `src/backend/ir.rs`; lowering in `src/backend/lower.rs`; new
  kernel32 IAT entries in `src/backend/emit/pe.rs` (Sleep,
  GetCommandLineA); BSS fields in `src/runtime/abi.rs` (stdin buffer,
  argv table, float sink + redirect flag, arg data).
- **float -> Str**: `rt_str_from_float` sets a BSS redirect flag and
  calls the existing `rt_print_float` dtoa; `emit_write` checks the flag
  and appends to a BSS sink instead of stdout, then the result is copied
  to an owned Str. Reuses the exact ~1000-line exact-dtoa path instead of
  duplicating it.
- **str_format**: exactly three `Str` substitution arguments (MINK V1
  contract); `{}` substituted, `{{`/`}}` literal, missing args dropped.
  Two-pass (measure then fill) so the result length is exact.
- **argv**: lazy `GetCommandLineA` parse at first `rt_argc`/`rt_argv`
  call; Windows quoting rules (quoted tokens may contain spaces, `""`
  -> empty argument); per-call owned `Str` results.
- **module search path**: `resolve_module_path` in `src/driver.rs` —
  sibling first (existing behavior preserved), then `<exe>/../stdlib`
  (npm layout: `node_modules/@p4inz-code/mink/stdlib`), then
  `cwd/stdlib` (dev checkout layout). No env vars, no config.

## 4. Durable regression tests

`tests/session99.rs` — 13 tests, all deterministic, leak checker enabled,
each compiling real MINK source and running the generated PE:

- env round trip; missing variable; **free-list reuse regression**
  (exact checkpoint repro); empty + spaced values
- argv count/order/spaces/empty; zero args
- stdin piped to EOF (exact bytes); empty stdin
- sleep zero + short
- stderr write separate from stdout (exact bytes both streams)
- float -> Str exact outputs (0, -7, 123456, 2.5)
- str_format substitution/escapes/missing-args; exact length + ownership

Plus `tests/windows_hardening.rs` env test rewritten from "stub pinned"
to "real behavior pinned" (missing -> empty; inherited var visible and
exact).

## 5. npm package + clean install

- `npm/mink/package.json` now ships `bin/mink.exe` **and** `stdlib`
  (all 16 `.mink` modules).
- Packed tarball contents verified (exe + package.json + 16 stdlib
  modules); tracked `npm/mink/bin/mink.exe` refreshed from the current
  release build.
- Clean install (fresh temp dir, no MINK anywhere): `npm install` of
  the packed tarball -> `mink`/`mink.cmd`/`mink.ps1` shims; `--version`,
  `-v`, `-V` all report `mink 1.0.1`.
- From a **separate** working directory with only `main.mink`:
  `mink check` + `mink build` on a `mod math;` program resolved the
  bundled stdlib automatically via `<exe>/../stdlib` — no include path,
  no manual copy, no env hacks. Generated exe ran correctly
  (`math_abs(-42)` -> 42, `math_pi()` -> `3.1415926535897931` via the
  new float->Str).

## 6. Proof application: `examples/env_report/`

`main.mink` exercises every new capability: argv (spaces preserved),
env get/set/remove round trip, stdin to EOF, `str_format`, float->Str,
stderr write, sleep, `rt_fs_write` of a text report — with every owned
string freed (exits 0 through the leak checker).

Verified through the full public chain:
1. repo CLI: check/build/run with piped stdin -> correct stdout/stderr,
   `env_report.txt` written, exit 0
2. clean npm-installed CLI (separate cwd): same result
3. standalone: `main.exe` copied to an empty directory with a space in
   its name, run with args + piped stdin -> identical output, exit 0,
   no missing-resource errors

## 7. Quality gates

- `cargo fmt --check`: clean
- `cargo clippy --all-targets`: 0 errors; 0 new warnings in changed
  files (pre-existing frozen-Linux/style warnings unchanged)
- `cargo build` / `cargo build --release`: clean
- `cargo test`: 2560 tests total (2547 baseline + 13 new). Full-run
  results: all deterministic tests pass. The two known Session 98
  timing-flaky loopback tests (`http_windows_split_server...`,
  `http_windows_large_body...`, sometimes `tcp_mink_client_checksum...`)
  intermittently fail only under full parallel load and pass in
  isolation (verified repeatedly); this is the documented pre-existing
  test-infra finding, not a Session 99 regression — the failing tests
  are TCP/HTTP loopback timing, unrelated to the env/argv/stdin emitters
  added here.

## 8. Parity matrix changes

Rows flipped in `docs/roadmap/WINDOWS_PYTHON_OFFICIAL_CAPABILITY_PARITY.md`:

- **L16** (format spec): MISSING -> PARTIAL (float->Str + str_format
  verified; Python-style format specs intentionally out of the minimal
  MINK contract, P2)
- **L67** (import search paths): PARTIAL -> VERIFIED
- **R09** (stderr write): MISSING -> VERIFIED
- **R10** (stdin): MISSING -> VERIFIED
- **R12** (argv): MISSING -> VERIFIED
- **R13** (env vars): PARTIAL -> VERIFIED
- **R23** (sleep): MISSING -> VERIFIED
- **S06** (numeric text): PARTIAL (formatting side now VERIFIED; parse
  remains Wave B)
- **S50** (os.environ): PARTIAL -> VERIFIED
- **S70** (sleep): MISSING -> VERIFIED
- **S74** (logging): MISSING (blocking dependency resolved; module to
  build)
- **W06** (env vars): PARTIAL -> VERIFIED
- **W08** (console stdin/stdout/stderr): PARTIAL -> VERIFIED
- **R06** (error location): stays PARTIAL; deferral recorded
- Distribution row: ships stdlib now
- Superseded-claims note: env no longer stubbed

## 9. Remaining gaps (accurate)

- R06 runtime error-location metadata: DEFERRED, next session
- Unicode (wide-char) env/argv/console: P2, wave H (W05)
- str_format: no format specs (intentional P2 difference)
- stdin: read-all only, no line streaming (P2)
- ANSI command line (P2)

## 10. Files changed

Source: `src/runtime/intrinsics.rs`, `src/backend/ir.rs`,
`src/backend/lower.rs`, `src/backend/emit/pe.rs`,
`src/backend/emit/runtime.rs`, `src/backend/emit/x86_64.rs`,
`src/backend/emit/linux_runtime.rs` (stub registrations only),
`src/runtime/abi.rs`, `src/driver.rs`
Tests: `tests/session99.rs` (new), `tests/windows_hardening.rs`
Package: `npm/mink/package.json`, `npm/mink/bin/mink.exe` (refreshed),
`npm/mink/stdlib/` (new, 16 modules)
Example: `examples/env_report/` (new)
Docs: `docs/roadmap/WINDOWS_PYTHON_OFFICIAL_CAPABILITY_PARITY.md`, this
file