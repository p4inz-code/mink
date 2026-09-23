# Session 119 — Final public-facing Windows acceptance audit

**Starting commit:** `bd9aaa9` (Session 118 close; `HEAD == origin/main`, clean tree)
**Ending commit:** the commit carrying this report
**Defects found and root-caused:** 4 (`mink init` in a non-identifier directory; `mink test`
and `mink repl <file>` module resolution; a malformed missing-module diagnostic; a garbled
no-candidate resolver message)
**Documentation corrected:** the README's command list, standard-library table, environment
status, clippy gate and two "Future" rows, plus one stale stdlib comment
**Linux:** frozen and untouched. No ELF/backend source was modified.

## 1. What this session was

The last audit before the Windows checkpoint is frozen, but with a different purpose from
Session 118's parity audit: treat the **README and public documentation as a contract**,
prove every testable claim, verify the **packaged distribution** rather than only the
repository, and drive the whole workflow from clean external environments. No features were
added, no roadmap work was started, and no unrelated code was optimised.

Nothing reported by Session 118 was assumed. `HEAD == origin/main` (`bd9aaa9`), the tree was
clean, and every number below was re-derived.

## 2. Repository and release integrity

| Check | Result |
|---|---|
| `git rev-parse HEAD` | `bd9aaa9` |
| `git rev-parse origin/main` (after `git fetch`) | `bd9aaa9` — equal |
| `git status` | clean |
| `Cargo.toml` / `npm/mink/package.json` / README badge / README expected output / `mink --version` | all `1.0.1` — consistent |
| Rust toolchain used | 1.97.1 (manifest requires 1.85) |
| Node / npm used | 24.14.0 / 11.9.0 |

## 3. README claims audited

Every externally testable claim in the README was executed, not read.

| Claim | Result |
|---|---|
| `mink --version` prints `mink 1.0.1` | **confirmed** |
| `mink -V` / `-v` / `version` also print the version | **confirmed** |
| `mink help` / `--help` / no arguments print usage | **confirmed** |
| `mink build --help` reports an unknown option (documented as such) | **confirmed**, exit 1 |
| First Program: `mink run hello.mink` prints `42` | **confirmed** |
| `mink build hello.mink` creates `hello.exe`; `./hello.exe` prints `42` | **confirmed** |
| Sum 1..10 prints `55` and exits `1` (both via `run` and via the standalone `.exe`) | **confirmed** |
| Strings and heap example prints `hi!` then `done` | **confirmed** |
| Structs/enums/matching example prints `5` | **confirmed** |
| `mink check hello.mink` succeeds | **confirmed** |
| `mink explain E-T01` explains a type mismatch; bare `mink explain` lists 50 codes | **confirmed** |
| "Static CRT — no external DLL dependencies"; standalone PE runs on a target with no compiler | **confirmed** by parsing the emitted PE: machine `0x8664`, PE32+, console subsystem, exactly one import (`kernel32.dll`, 40 functions), and it runs with `PATH` reduced to `C:\Windows\System32` in a directory containing nothing else |
| Bundled stdlib is imported with `use` and needs no configuration | **confirmed** through the packaged compiler |
| Supported platform is Windows x64; Linux/macOS are not supported | accurate as a *support* statement (the CLI additionally exposes a frozen `x86_64-linux-elf` emission target) |

## 4. Claims that were false — and the fix

Each stale README claim was resolved against the implementation. The implementation was
never weakened to satisfy documentation.

1. **`mink init` failed in any directory whose name is not a legal package name.** In
   `…/mink proj π` it aborted with `E-PKG03: invalid package name`. `parse_init` had a
   comment claiming the derived name was "lowercased and sanitised", but it only lowercased.
   Added `package::manifest::package_name_from_directory` (lowercase, `-`/`_` preserved,
   every other character a separator, separators collapsed and trimmed, `project` as the
   fallback) and used it for the *derived* default only — an explicit `--name` is still
   validated exactly as typed. The pre-existing test for spacey paths had passed
   `--name spaced`, which is what hid this.

2. **`mink test` and `mink repl <file>` could not resolve project-local modules.** Both
   commands generated their source into the system temp directory, and module resolution is
   relative to the *source file's* directory — so a test file or session file containing
   `mod helper;` looked for `%TEMP%\helper.mink` and failed with "module file … not found",
   as did any installed package. Generated sources are now written beside the file they are
   derived from (`generated_source_dir`), with a pid/counter-qualified name, and the existing
   cleanup still removes them. A bare `mink repl` with no file keeps using the temp
   directory.

3. **A missing `mod` file produced a nonsensical diagnostic.** It read
   ``cannot find name `module file 'nope.mink' not found` in this scope`` — a *file* error
   rendered through the unresolved-*identifier* template. `SemanticError::module_not_found`
   now carries its own message (keeping the `E-S01` code, whose documented causes include a
   failed module import), so it reads `module file 'nope.mink' not found`.

4. **A no-candidate resolver error read as a broken sentence.**
   `no version of 'gone' satisfies 'ok' wants '1.0.0'` — `describe()` renders
   `'from' wants 'req'`, which is correct after "conflicting requirements for 'x':" but not
   after "satisfies". `describe_requirements()` renders `'req' required by 'from'` for that
   arm only, giving `no version of 'gone' satisfies '1.0.0' required by 'ok'`.

Documentation corrections (no implementation change):

- the Commands table now lists `mink test` and `mink repl`, and the package commands
  (`init`/`add`/`remove`/`install`/`update`/`env`) are documented in their own table;
- the standard-library table documented 13 of the 26 shipped modules; the 12 missing ones
  were added and each was checked against its module header;
- the environment library was documented as "V1 stubs — API exists but not yet wired to the
  Windows API"; `rt_env_get/set/has/remove` have been wired to
  `GetEnvironmentVariableA`/`SetEnvironmentVariableA` since Session 99, so the claim was
  corrected in both places it appeared (and confirmed by execution: `rt_env_get("PATH")`
  returns the real value);
- **Package manager** and **Concurrency / threading** were listed as "Future" while both are
  implemented; they now read "Available";
- the documented clippy gate was `cargo clippy --all-targets -- -D warnings`, which **fails**
  (exit 101, 85 errors against the existing 82-library/2-test warning baseline). The README
  now documents the gate the project actually keeps: warnings are not zero yet and no change
  may add one. Fixing the baseline itself is out of this audit's scope;
- `stdlib/assert.mink`'s header advertised `assert_eq`/`assert_ne`, which do not exist; the
  real names are `assert_true`, `assert_false`, `assert_eq_int`, `assert_ne_int`,
  `assert_eq_str`, `assert_ne_str`, `assert_eq_float`. The comment now names them (this is
  also what corrected the first draft of the README's `assert` row).

The `npm/mink/bin/mink.exe` bundle and the bundled `stdlib/` were refreshed from this
session's source, so the packaged distribution matches the repository (`diff -r stdlib
npm/mink/stdlib` is empty).

## 5. Clean external installation and the packaged distribution

Every workflow below ran outside the repository, in a path containing **spaces and
Unicode** (`…/Temp/mink audit 测试`, `…/Temp/mink global π`, …), using the tarball produced
by `npm pack`.

1. `npm pack` → **28 files** (compiler + package.json + 26 stdlib modules); bundled `stdlib/`
   byte-identical to `stdlib/`.
2. `npm install -g --prefix "…/mink global π" <tarball>` → installs the package and the
   `mink`/`mink.cmd`/`mink.ps1` shims; `mink --version` → `mink 1.0.1` through the shim.
3. Bundled stdlib resolution: `mod logging; use logging::log_info;` compiled and ran against
   the packaged `stdlib/` with no configuration or copying.
4. Project workflow: `mink init` (with the corrected derived name), `mink add helper --path
   ../helper`, a `mink.lock` carrying a content checksum, `mink install --check`, then
   `mink run` importing the installed package and printing `42`.
5. Environments: `mink env new/use/list/remove`, install into the environment's packages
   directory, and a run inside the environment using the environment's copy.
6. `mink test` on a file importing a sibling module and `mink repl <file>` calling it —
   both previously failing, both now passing — plus `mink test` with `mod assert;`.
7. Shipped examples run through the packaged compiler: `freq_count` (collections), `log_util`
   (logging), `threaded_work` (threads + locks; "MATCH" against the closed-form total),
   `async_tasks` (tasks), `inventory_db` (SQLite), `fs_util`, `proc_util`, `env_report`,
   `compress_util` (zlib/gzip: a 20 000-byte binary survives a compress→decompress round trip
   `cmp`-identical), `runtime_error_report` (E-R10, exit 110 as designed), `regex_tool`,
   `tcp_client` (against a local echo server), `http_client` (against a local HTTP/1.1
   server).
8. Standalone output: `mink build` then run in an otherwise empty directory.

## 6. TLS / HTTPS security, re-verified from the packaged compiler

Trust was never bypassed to make anything pass. Through the packaged compiler against public
endpoints:

| Endpoint | Result |
|---|---|
| `https://example.com/` (public CA, system store) | **ACCEPTED** — real `HTTP/1.1 200 OK`, 868 bytes |
| `https://expired.badssl.com/` | **REJECTED** |
| `https://wrong.host.badssl.com/` (hostname mismatch) | **REJECTED** |
| `https://self-signed.badssl.com/` (untrusted root) | **REJECTED** |
| `https://untrusted-root.badssl.com/` | **REJECTED** |

The opt-in `tests/tls_lib.rs::s62_system_store_public_https` also passes when run explicitly
(`cargo test --test tls_lib -- --ignored`).

## 7. Failure modes

| Input | Behaviour |
|---|---|
| missing source file | `failed to read 'missing.mink'`, exit 1 |
| directory as source | `'adir' is a directory, not a MINK source file`, exit 1 |
| unknown command | `unknown command 'bogus'` + "Run 'mink help' for usage." |
| missing path argument | names the command and its usage |
| malformed source | `error[E-P02]: expected an identifier` with `--> file:line:col` |
| missing module | `error[E-S01]: module file 'nope.mink' not found` (fixed this session) |
| duplicate `mod` of the same file | accepted (de-duplicated); the file is compiled once |
| invalid package name / version in `mink.toml` | `E-PKG03` naming the offending value |
| missing dependency | `E-PKG05` naming the requirement and the requester (fixed this session) |
| no `mink.toml` when one is required | `E-PKG01: cannot read '.\mink.toml'` |
| unknown error code | `unknown error code 'E-NOPE'` + pointer to `mink explain` |

## 8. Full regression (re-derived)

The suite was run with the final source, in grouped invocations (one cargo invocation at a
time, as Session 118 requires):

| Group | Result |
|---|---|
| Group 1 — lib unit tests + adversarial … generics (20 units) | 888 passed · 2 ignored · 0 failed |
| Group 2 — hashing_lib … packages (18 units) | 698 passed · 0 ignored · 0 failed |
| Group 3 — parser … smoke (17 units) | 742 passed · 0 ignored · 0 failed |
| Group 4 — source … zlib_lib (17 units) | 618 passed · 1 ignored · 0 failed |
| Doc tests | 0 |
| **Total** | **2 946 passed · 3 ignored · 0 failed** |

This reconciles exactly with Session 118's 2 941: this session added **5** permanent
regression tests — `package::manifest::directory_names_derive_valid_package_names`,
`modules_check::missing_module_file_message_names_the_file`,
`package_manager::p04_init_derives_a_legal_name_in_any_directory`,
`test_runner::t13_test_command_resolves_a_sibling_module`,
`repl::r15_repl_resolves_a_sibling_module`. No test was removed, renamed or ignored.

## 9. Parity matrix

The 232 rows were re-parsed mechanically (whole-cell splitting, so `\|`-escaped rows parse
correctly): **232 rows**, no duplicates, all 10 cells.

| Check | Re-derived | Documented | Agreement |
|---|---|---|---|
| Rows by domain | L 73 · R 29 · S 78 · T 16 · W 22 · P 14 | same | yes |
| Status | VERIFIED 102 · MISSING 68 · PARTIAL 34 · INTENT. DIFF. 13 · N/A 6 · EXECUTION VERIFIED 2 · PLANNED 2 · N/A (INTENT.) 2 · INTENT. DIFF.+PARTIAL 1 · VERIFIED (INTENT. DIFF.) 1 · VERIFIED (partial) 1 = **232** | §10.1 identical | yes |
| Priority | **P0 0 · P1 0** · P2 88 · P3 54 · no-gap 90 = **232** | §10.2 identical | yes |
| Difficulty | S-M 6 · S 37 · M 84 · L 30 · XL 14 = 171 work rows + 61 no-work = **232** | §10.3 identical | yes |
| `Blocks = Y` | **0** (all 232 are `N`) | `P1` set empty | yes |
| Wave blockers | 0 in every wave | §10.4 identical | yes |

The only live figures that had gone stale were the test-suite totals in the matrix's §2
baseline table and its evidence index; both are updated to 2 946 with a pointer to this
report.

## 10. Build quality

- `cargo fmt --check` — clean (one formatting fix was needed in this session's own new code).
- `cargo clippy --all-targets` — **82 library warnings + 2 test warnings**, identical to the
  documented baseline; **0 new** warnings from this session's changes.
- Debug, release and test builds — clean; `cargo build --release` exits 0.
- Windows PE smoke: every example, test and standalone binary executed as a real PE.

## 11. Infrastructure limitations (not product behaviour)

1. **`LNK1104: cannot open file …deps\<target>.exe`.** This machine has Windows Defender
   (`MsMpEng.exe`) and `SearchIndexer.exe` running, so freshly linked test binaries are locked
   while being written. It is transient and per-target, and a retry always succeeds:
   `cargo test --no-run -j 1` needed 5 attempts, each failing on a different target
   (`struct_destructure`, `strings_lib`, `source`, `sqlite_lib`) and then linking everything.
   `cargo test -j 1` per group, with a retry, completed the whole suite. No orphaned
   processes remained at any point (`tasklist` showed no `mink`/`cargo`/`rustc` processes).
   Security was **not** disabled or reconfigured to work around this.
2. One 600-second wall-clock limit killed the first full-suite invocation mid-target. The
   grouping above exists so no single invocation is at risk of being cut off; the killed
   run's completed results were discarded and everything was re-run.

## 12. Acceptance decision

- P0 = 0 · P1 = 0 (matrix re-derived from the 232 rows)
- No known Windows product regression; the full suite is 2 946 passed / 3 ignored / 0 failed
- No broken README command or example; all four defects that made documented workflows fail
  are fixed with permanent regression coverage
- Documentation matches reality (commands, stdlib, environment, gates, versions)
- The packaged distribution matches the repository and works outside it, in paths with
  spaces and Unicode
- TLS trust is enforced and re-verified against public endpoints; concurrency stress is
  deterministic; native PE verification covers runtime, compiler and platform behaviour
- `HEAD == origin/main`; working tree clean; Linux untouched

**The Windows checkpoint is accepted and frozen at this commit.** The remaining matrix rows
are P2/P3 non-blocking roadmap work; Linux stays frozen.
