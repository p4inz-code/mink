# Session 275 — Current-Tree Release Acceptance Verification

Windows x86_64, 2026-11-13. Starting commit
`0aa07e43b626f8167750bcdcf72ccc35b29f857b`, equal to `origin/main` after
`git fetch origin`. This session changed no source file, test, manifest or
generated artifact. The only repository changes are documentation.

## 1. Objective and result

Establish whether the current tree produces a valid optimized release build and
passes the complete applicable release-mode suite, using the procedure recorded
at Session 213. **Both succeeded, and every executed suite passed.**

| Gate | Result |
| --- | --- |
| Clean release build | **PASS** — exit 0, 0 errors |
| All release test targets compiled | **PASS** — exit 0, 0 errors |
| Complete release-mode suite | **PASS** — 91 suites, 3,436 passed, 0 failed, 9 ignored, exit 0 |
| Unexpected failures | **none** |
| Ignored tests | 9, all intentional and documented in §7 |
| Source defects found | **none** |

## 2. Starting state

| Item | Value |
| --- | --- |
| Branch | `main` |
| Starting HEAD | `0aa07e43b626f8167750bcdcf72ccc35b29f857b` (exactly the requested checkpoint) |
| Remote `origin/main` | identical after `git fetch origin` |
| Tracked worktree | clean |
| Untracked user files | 4, preserved and never staged |
| Version metadata | `Cargo.toml` 1.0.5, `npm/mink/package.json` 1.0.5, unpublished |

The four user files are `MINK UI SETS OFFICIAL.zip`,
`freebuff-chat-2026-10-01T19-08-00.600Z.md`, `nl1.exe` and `nl1.mink`. They
were not read, modified, staged or removed.

## 3. Toolchain and configuration

| Item | Value |
| --- | --- |
| `rustc` | 1.97.1 (8bab26f4f 2026-07-14), commit `8bab26f4f68e0e26f0bb7960be334d5b520ea452` |
| `cargo` | 1.97.1 (c980f4866 2026-06-30) |
| LLVM | 22.1.6 |
| Host / target | `x86_64-pc-windows-msvc` |
| Static runtime | `.cargo/config.toml` sets `-C target-feature=+crt-static` |
| Optimization profile | Cargo's default `release` profile; `Cargo.toml` declares no custom profile |
| Dependencies | none (`[dependencies]` is intentionally empty) |

No compiler version, target, rustflag or profile setting was changed for this
run. The toolchain is the same one recorded at Sessions 212–213.

## 4. Clean release build

The build used a target directory that did not exist before the run
(`target/s275_release`), so no stale object could contribute.

```
CARGO_TARGET_DIR=target/s275_release cargo build --release
```

| Item | Value |
| --- | --- |
| Exit code | **0** |
| Duration | **20.765 s** wall (Cargo reported 20.72 s) |
| Errors | **0** |
| Warnings | **62** library warnings, no warnings suppressed or added |
| Binary | `target/s275_release/release/mink.exe` |
| Size | **4,242,944 bytes** |
| SHA-256 | `92bd263adcf11094ef89ef1743d84849fb2f6c22bbad7345adbbdee77d2da436` |
| Version check | `mink --version` → `mink 1.0.5` |
| Crash markers in log | none (no NTSTATUS, signal or "could not compile") |

Warning classes, all pre-existing categories rather than new failures:

- 55 dead-code warnings (`is never used`) — unused constants and functions in
  the frozen ELF/Linux backend and unused PE constants;
- 3 unused variables (`offsets`) in `src/backend/emit/runtime.rs`;
- 1 unused assignment (`else_terminated`) in `src/ownership/mod.rs:1678`;
- 2 missing-documentation warnings plus one multi-part diagnostic.

The `else_terminated` notice is in code added by the recent ownership work. It
is a warning, not a correctness failure, and was deliberately **not** fixed in
this acceptance session because it is unrelated to release verification.

**Artifact comparison caveat.** The binary is larger than the Session 213
reference (4,048,384 bytes, SHA-256 `87a1cae4…b6da23`), but the two are not
comparable: this tree contains struct patterns, `defer`, `defer move` and
element-level aggregate ownership, which the reference binary does not
implement. The difference is a different compiler, not a measured size
regression. The build was also faster than the 30.78 s recorded at Session 213;
run-to-run and cache variance plus a different source tree make that a
measurement, not an improvement claim.

## 5. Release test compilation

```
CARGO_TARGET_DIR=target/s275_release cargo test --release --no-run
```

| Item | Value |
| --- | --- |
| Exit code | **0** |
| Duration | **13.356 s** |
| Errors | 0 |
| Test targets | 88 integration files, the library unit harness and the binary unit harness |
| Result | every target compiled for release; none skipped or excluded |

Using one target directory for build and tests means the test harnesses resolve
`CARGO_BIN_EXE_mink` to the release compiler measured in §4, including the
`encoding_lib` and `math_lib` suites corrected at Session 212.

## 6. Complete release-mode suite

```
CARGO_TARGET_DIR=target/s275_release cargo test --release --no-fail-fast
```

| Metric | Value |
| --- | ---: |
| Cargo exit code | **0** |
| Wall duration | **101.707 s** |
| Suite result blocks | **91** (88 integration + 2 unit + doc-tests) |
| Passed | **3,436** |
| Failed | **0** |
| Ignored | **9** |
| Suites with a non-zero (failure) result | **0** |

The suite includes compiler front end and back end, parser hardening, semantic
analysis, diagnostics, ownership and move checking, `defer` and `defer move`,
struct patterns, aggregate ownership, runtime and resource lifecycle,
filesystem and 4 GiB sparse large-file handling, process and environment,
threads and async, TLS and HTTP, SQLite/JSON/CSV, the standard library
signature/documentation guards, the package manager, CLI, REPL and release
tests. Per-suite counts are in Appendix A.

Key subsystem rows:

| Suite | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| `defer` | 33 | 0 | 0 |
| `defer_move` | 74 | 0 | 0 |
| `defer_aggregate` | 40 | 0 | 0 |
| `struct_match` | 23 | 0 | 0 |
| `ownership` | 49 | 0 | 0 |
| `stdlib_ownership` | 11 | 0 | 0 |
| `resource_lifecycle` | 7 | 0 | 0 |
| `large_file` | 5 | 0 | 0 |
| `tls_lib` | 21 | 0 | 1 |
| `diagnostics` | 24 | 0 | 0 |
| `soak` | 1 | 0 | 8 |

## 7. Ignored tests

Nine tests are ignored by design; the count is unchanged from the debug
baseline and from Session 213.

| Suite | Test | Reason |
| --- | --- | --- |
| `soak` | `soak01_repeated_compiler_invocations` | bounded soak; run with `--ignored` |
| `soak` | `soak02_repeated_program_launches` | bounded soak; run with `--ignored` |
| `soak` | `soak03_collection_lifecycle` | bounded soak; run with `--ignored` |
| `soak` | `soak04_filesystem_cycles` | bounded soak; run with `--ignored` |
| `soak` | `soak05_process_cycles` | bounded soak; run with `--ignored` |
| `soak` | `soak06_error_paths` | bounded soak; run with `--ignored` |
| `soak` | `soak07_combined_workload` | bounded soak; run with `--ignored` |
| `soak` | `s207_extended_mixed_soak` | extended soak; run with `--ignored` (default 300 s, `MINK_SOAK_SECONDS`) |
| `tls_lib` | `s62_system_store_public_https` | requires public internet access; positive path run manually |

None is a failed or skipped test, and none was disabled in this session. The
`soak` suite's one non-ignored test passed; `tls_lib` executed its full local
21-test path.

## 8. Debug versus release

| Metric | Debug baseline (Session 274, `60c2160`) | Release (this session, `0aa07e4`) |
| --- | ---: | ---: |
| Suites | 91 | **91** |
| Passed | 3,436 | **3,436** |
| Failed | 0 | **0** |
| Ignored | 9 | **9** |
| Cargo exit code | 0 | **0** |

The two checkpoints differ only by the Session 274 documentation commit, so the
source compiled is identical and matching counts are the expected parity
result, not a coincidence. Debug and release totals are kept separate here.

## 9. Historical comparison

| Checkpoint | Source | Suites | Passed | Failed | Ignored |
| --- | --- | ---: | ---: | ---: | ---: |
| Session 213, 2026-11-08 | `f851947` | 85 | 3,262 | 0 | 9 |
| Session 274 debug, 2026-11-13 | `60c2160` | 91 | 3,436 | 0 | 9 |
| Session 275 release, 2026-11-13 | `0aa07e4` | **91** | **3,436** | **0** | **9** |

The additional suites come from the four post-package test files
(`struct_match`, `defer`, `defer_move`, `defer_aggregate`) and from the current
Cargo test layout, which reports the library harness, the binary harness and
doc-tests as separate results. The Session 213 record is preserved unchanged
and describes its own source; it is not restated as a current total.

## 10. Defects and source changes

- No release-mode test failed, so no defect was reproduced or fixed.
- No test, assertion, tolerance, filter or ignore attribute was changed.
- No source, manifest or configuration file was modified.
- The only observation is the unused-assignment warning noted in §4, which is
  not a release blocker and would be an unrelated code change.

## 11. Verdict

**Session 275: PASS.** The current tree at `0aa07e4` builds cleanly in release
mode, every release test target compiles, and the complete applicable suite
executes with 3,436 passed, 0 failed and 9 accounted-for ignores.

This verdict does **not** make MINK release-ready. M6 external developer
validation remains **OUTSTANDING**, M7 remains demand-gated, M8 is gated, the
Linux backend is frozen, and 1.0.5 remains unpublished with no tag or GitHub
release. No publication action was taken.

## 12. Evidence artifacts

All are git-ignored under `target/`:

| Path | Content |
| --- | --- |
| `target/s275_release_build.log` | full `cargo build --release` output |
| `target/s275_build_time.txt` | build wall time |
| `target/s275_release_testbuild.log` | `--no-run` compilation output |
| `target/s275_release_tests.log` | complete release test output |
| `target/s275_release_tests_time.txt` | suite wall time |
| `target/s275_suite_counts.csv` | per-suite counts used for Appendix A |

## Appendix A — per-suite release results

| Suite | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| `adversarial` | 94 | 0 | 0 |
| `aggregate` | 59 | 0 | 0 |
| `aggregate_returns` | 52 | 0 | 0 |
| `assert_lib` | 16 | 0 | 0 |
| `async_lib` | 23 | 0 | 0 |
| `backend` | 46 | 0 | 0 |
| `bin_main` | 0 | 0 | 0 |
| `bool_packing` | 22 | 0 | 0 |
| `capacity` | 7 | 0 | 0 |
| `cli` | 75 | 0 | 0 |
| `closures` | 59 | 0 | 0 |
| `collections` | 24 | 0 | 0 |
| `collections_lib` | 71 | 0 | 0 |
| `cross_subsystem` | 4 | 0 | 0 |
| `crypto_lib` | 18 | 0 | 0 |
| `csv_lib` | 14 | 0 | 0 |
| `defer` | 33 | 0 | 0 |
| `defer_aggregate` | 40 | 0 | 0 |
| `defer_move` | 74 | 0 | 0 |
| `diagnostics` | 24 | 0 | 0 |
| `discriminants` | 32 | 0 | 0 |
| `doc_tests` | 0 | 0 | 0 |
| `encoding_lib` | 76 | 0 | 0 |
| `enums` | 25 | 0 | 0 |
| `environment_lib` | 3 | 0 | 0 |
| `filesystem_lib` | 47 | 0 | 0 |
| `fs_read` | 6 | 0 | 0 |
| `function_annotations` | 60 | 0 | 0 |
| `generics` | 42 | 0 | 0 |
| `hashing_lib` | 29 | 0 | 0 |
| `hir` | 25 | 0 | 0 |
| `http_lib` | 68 | 0 | 0 |
| `httpserver_lib` | 10 | 0 | 0 |
| `if_expressions` | 9 | 0 | 0 |
| `json` | 68 | 0 | 0 |
| `large_file` | 5 | 0 | 0 |
| `let_annotations` | 82 | 0 | 0 |
| `lexer` | 50 | 0 | 0 |
| `lib_unit` | 121 | 0 | 0 |
| `logging_lib` | 13 | 0 | 0 |
| `loop_expressions` | 39 | 0 | 0 |
| `match_expressions` | 41 | 0 | 0 |
| `math_lib` | 106 | 0 | 0 |
| `mir` | 36 | 0 | 0 |
| `modules_check` | 7 | 0 | 0 |
| `network_lib` | 65 | 0 | 0 |
| `optimization` | 38 | 0 | 0 |
| `option_result` | 45 | 0 | 0 |
| `ownership` | 49 | 0 | 0 |
| `package_manager` | 24 | 0 | 0 |
| `packages` | 5 | 0 | 0 |
| `parser` | 116 | 0 | 0 |
| `parser_hardening` | 64 | 0 | 0 |
| `pattern_matching` | 45 | 0 | 0 |
| `process_lib` | 86 | 0 | 0 |
| `random_lib` | 15 | 0 | 0 |
| `re_lib` | 33 | 0 | 0 |
| `references` | 58 | 0 | 0 |
| `release` | 69 | 0 | 0 |
| `repl` | 15 | 0 | 0 |
| `resource_lifecycle` | 7 | 0 | 0 |
| `richer_patterns` | 90 | 0 | 0 |
| `runtime` | 28 | 0 | 0 |
| `s69_hardening` | 11 | 0 | 0 |
| `scalar_types` | 27 | 0 | 0 |
| `semantics` | 75 | 0 | 0 |
| `session100` | 14 | 0 | 0 |
| `session99` | 13 | 0 | 0 |
| `smoke` | 13 | 0 | 0 |
| `soak` | 1 | 0 | 8 |
| `source` | 12 | 0 | 0 |
| `sqlite_lib` | 18 | 0 | 0 |
| `stdlib_docs` | 1 | 0 | 0 |
| `stdlib_ownership` | 11 | 0 | 0 |
| `stdlib_signatures` | 1 | 0 | 0 |
| `strings` | 63 | 0 | 0 |
| `strings_lib` | 90 | 0 | 0 |
| `struct_destructure` | 28 | 0 | 0 |
| `struct_match` | 23 | 0 | 0 |
| `sum_types` | 44 | 0 | 0 |
| `test_runner` | 13 | 0 | 0 |
| `threads_lib` | 24 | 0 | 0 |
| `time_lib` | 29 | 0 | 0 |
| `tls_lib` | 21 | 0 | 1 |
| `try_operator` | 12 | 0 | 0 |
| `tuple_destructure` | 40 | 0 | 0 |
| `tuples` | 34 | 0 | 0 |
| `typecheck` | 163 | 0 | 0 |
| `windows_hardening` | 22 | 0 | 0 |
| `zip_lib` | 11 | 0 | 0 |
| `zlib_lib` | 15 | 0 | 0 |
| **Total (91 suites)** | **3436** | **0** | **9** |

