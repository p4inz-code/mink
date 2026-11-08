# Session 213 — Full Release-Mode Verification

Windows x86_64, 2026-11-08. Starting commit
`f85194750c694c0a77acabe1945c5f9d11434391` (Session 212, local == `origin/main`
verified before any change). Toolchain: rustc 1.97.1 / LLVM 22.1.6, MSVC host,
Windows SDK 10.0.26100.0, `.cargo/config.toml` `crt-static` enabled.

## 1. Clean release build

| Item | Value |
| --- | --- |
| Command | `CARGO_TARGET_DIR=target/s213_clean cargo build --release` |
| Compiler | `rustc 1.97.1 (8bab26f4f 2026-07-14)` |
| Target dir | `target/s213_clean` — did not exist before the run |
| Exit code | **0** |
| Duration | **30.78 s** |
| Warnings | 60 library warnings (`missing_docs` class), 0 errors; no warnings suppressed or added |
| Binary | `target/s213_clean/release/mink.exe` |
| Size | **4,048,384 bytes** |
| SHA-256 | `87a1cae4946f121e292d1a516e6047ad287b6506c5fe52a16208717dc2b6da23` |
| Version check | `mink --version` -> `mink 1.0.5` |
| Crash markers in log | **0** (no NTSTATUS, no signal, no "could not compile") |

The same command in the default `target/` produced the same size
(4,048,384 bytes) in 31.42 s, replacing the 2026-11-07 artifact
(4,045,824 bytes). Sizes are current measurements, not historical ones.

## 2. Release test coverage

All 84 integration targets plus the library were compiled for release
(`cargo test --release --no-run`: 87 executables, exit 0) and executed with
`cargo test --release --no-fail-fast`.

| # | Required area | Release suites that executed |
| ---: | --- | --- |
| 1 | Compiler functionality | `backend`, `hir`, `mir`, `optimization`, `parser`, `parser_hardening`, `semantics`, `source`, `modules_check`, `adversarial`, `session99`, `session100`, `smoke` |
| 2 | Parser and semantic analysis | `parser`, `parser_hardening`, `semantics`, `if_expressions`, `match_expressions`, `pattern_matching`, `richer_patterns`, `discriminants` |
| 3 | Type checking | `typecheck`, `let_annotations`, `function_annotations`, `generics`, `sum_types`, `enums`, `option_result`, `try_operator`, `tuples`, `tuple_destructure`, `struct_destructure`, `closures`, `references`, `scalar_types`, `aggregate`, `aggregate_returns`, `loop_expressions` |
| 4 | Ownership and move checking | `ownership`, `stdlib_ownership`, `references` |
| 5 | Runtime behaviour | `runtime`, `assert_lib`, `bool_packing`, `windows_hardening` |
| 6 | Filesystem operations | `filesystem_lib`, `fs_read` |
| 7 | Large-file handling | `large_file` (sparse 4 GiB boundary) |
| 8 | Collections and generation retirement | `collections`, `collections_lib`, `capacity` |
| 9 | Memory allocation and cleanup | `runtime`, `resource_lifecycle`, `cross_subsystem`, `capacity`, `stdlib_ownership` |
| 10 | Standard library | `encoding_lib`, `hashing_lib`, `crypto_lib`, `math_lib`, `random_lib`, `strings_lib`, `time_lib`, `logging_lib`, `re_lib`, `zlib_lib`, `zip_lib`, `stdlib_docs`, `stdlib_signatures` |
| 11 | Networking and TLS | `network_lib`, `http_lib`, `httpserver_lib`, `tls_lib` (21 passed / 1 ignored) |
| 12 | Process operations | `process_lib`, `environment_lib`, `cli` |
| 13 | Concurrency | `threads_lib`, `async_lib` |
| 14 | SQLite and structured data | `sqlite_lib`, `json`, `csv_lib` |
| 15 | Error handling and diagnostics | `diagnostics` (24 cases), `cli`, `repl`, `release` |

## 3. Release regression result

```
CARGO_TARGET_DIR=target/s213_tests cargo test --release --no-fail-fast
```

| Metric | Debug (Session 211, same HEAD) | Release (this session) |
| --- | ---: | ---: |
| Suites (84 targets + lib) | 85 | **85** |
| Passed | 3,262 | **3,262** |
| Failed | 0 | **0** |
| Ignored | 9 | **9** |
| Cargo exit code | 0 | **0** |
| Suites with a non-zero result | 0 | **0** |

Every suite that ran in debug ran in release with the same counts. `tls_lib`
executed fully (21 passed / 1 ignored) because CPython is present as `python`.
The 9 ignores are the seven bounded/extended soaks, the opt-in public-internet
TLS control, and the shipped-1.0.1 compatibility note — unchanged from debug.
No test was skipped, filtered, weakened or given a tolerance to obtain this
result.

## 4. Stability verification

| Check | Result |
| --- | --- |
| Release soak suite, all 8 ignored tests, `MINK_SOAK_SECONDS=60` | **8 passed / 0 failed**, 60.11 s |
| Extended mixed soak (`s207_extended_mixed_soak`) | pass, bounded to 60 s |
| Release `resource_lifecycle` (handle-count sampling) | pass inside the full release run |
| Release `diagnostics` (exit code, code, file:line, stderr byte-identity) | 24/24 pass |
| Release `capacity` (arena ceiling, table exhaustion, generation retirement) | 7/7 pass |
| Repeated `tls_lib` runs | see §5 |
| Crashes, hangs, leaked processes, unexpected artifacts | none observed |

The soak run used the repository's own per-launch and wall-clock budgets; no
unbounded stress was started.

## 5. Harness correction and re-verification

Session 212 found that `tests/encoding_lib.rs` and `tests/math_lib.rs` hardcoded
`target/debug/mink.exe`. Both now use `CARGO_BIN_EXE_mink`, so a release run
tests the release compiler.

| Suite | Debug | Release |
| --- | --- | --- |
| `encoding_lib` | 75 passed / 0 failed | **75 passed / 0 failed** |
| `math_lib` | 106 passed / 0 failed | **106 passed / 0 failed** |

The full release suite was then **re-run after this change** so the reported
totals describe the final tree. `tls_lib` was additionally repeated to check the
nondeterministic TLS/port path: two repeat release runs each reported **21 passed
/ 0 failed / 1 ignored** in 4.16 s, matching the full-run counts.

## 6. Release readiness assessment

| Dimension | Verdict | Evidence |
| --- | --- | --- |
| Debug verification | PASS | 85 suites, 3,262 / 0 / 9 (Session 211, same tree) |
| Release verification | **PASS** | Clean build + full release suite, 3,262 / 0 / 9 |
| Runtime correctness | PASS | runtime, resource_lifecycle, capacity, cross_subsystem, diagnostics |
| Memory and ownership | PASS | ownership, stdlib_ownership, leak checker on every program run |
| Standard library | PASS | 13 lib suites plus signature and documentation guards |
| Windows platform stability | PASS for tested paths | soak 8/8, flat handle counts, no crashes |
| Known limitations | Recorded, unchanged | 1 GiB arena, `MAX_LIVE_ALLOCS`, ANSI paths, `MAX_PATH`, bounded soak |
| Documentation accuracy | Corrected | Sessions 211–213 records and roadmap status |
| M6 external validation | **OUTSTANDING** | requires a developer who did not build MINK |

## 7. What this verdict does and does not mean

- It **does** mean the release build is reproducible on this host, the release
  compiler executes the complete suite, and no source-side regression appears
  under optimization.
- It does **not** authorize publication: no npm publish, tag or GitHub release
  was performed, and 1.0.5 remains unpublished.
- It does **not** close M6. Internal verification is not external developer
  validation.
- It does **not** prove the earlier `rustc` crashes cannot recur. The host fault
  recorded in Session 212 was transient and non-reproducing; this session
  establishes a working baseline, not a hardware diagnosis.

## 8. Gate result

**A. PASS** — the required release build and the complete release-mode suite
executed successfully with recorded evidence.

Debug and release results are kept separate above. Release-mode verification is
no longer blocked. No publication, tag or release action was taken.
