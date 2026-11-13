# Session 276 — Release acceptance automation and reproducibility

**Date:** 2026-11-13
**Starting HEAD:** `1b2f3249b51a7fab708fb4288c45edd797e34e93`
**Branch:** `main`
**Target:** `x86_64-pc-windows-msvc`
**Type:** Engineering infrastructure and release verification

## 1. Mission

Session 275 proved that the current tree builds and passes its complete release
regression, but the proof depended on hand-run commands and hand-recorded evidence. This
session replaces that manual procedure with a repository-native acceptance entry point
that runs the same authoritative stages, captures evidence, and decides the verdict from
the results rather than from its own exit status.

The goal is a trustworthy procedure, not a new build system. No compiler, test, target,
optimization or configuration was changed.

## 2. Audit of existing automation

The repository had no release acceptance runner and no CI workflow:

- `.github/` contains only `FUNDING.yml`.
- `scripts/` and `tools/` did not exist.
- PowerShell exists only for Windows shell integration:
  `npm/mink/windows/register-mink-filetype.ps1` and
  `validation/windows_filetype/verify_filetype_association.ps1`.
- Verification tooling is otherwise Python: `validation/**/verify.py`,
  `benchmarks/m4/harness/*.py`, `benchmarks/m5/**/tests/*.py`.

Python was therefore chosen for the runner, matching the existing verification
convention. A PowerShell wrapper would have duplicated the parsing and reporting logic
without adding repository-native value.

The authoritative commands are unchanged from Sessions 213 and 275:

```
cargo build --release
cargo test --release --no-run
cargo test --release --no-fail-fast
```

## 3. What was added

| File | Purpose |
| --- | --- |
| `tools/release_acceptance.py` | Staged runner, Cargo output parser, verdict logic, JSON and Markdown reports. |
| `tools/test_release_acceptance.py` | 11 failure-injection tests using a fake `cargo`; does not build MINK. |
| `docs/validation/RELEASE_ACCEPTANCE.md` | Usage, prerequisites, outputs, verdicts, reproduction and limitations. |
| `docs/benchmarks/M6/M6_SESSION_276_RELEASE_ACCEPTANCE_AUTOMATION.md` | This record. |

`README.md` now points to the workflow, and the roadmap carries a dated Session 276
status. Session 275's record is untouched.

## 4. Architecture

One entry point, four stages, fresh build tree:

1. **Environment** — rustc and cargo versions, host and LLVM, branch, HEAD, worktree
   status, `.cargo/config.toml`, `[profile.release]`, dependency count. Missing
   `cargo`, `rustc` or `git` produces `BLOCKED` before any stage runs.
2. **Build** — `cargo build --release` with `CARGO_TARGET_DIR` set to
   `target/acceptance-work/<run-id>/cargo-target`. The directory is created for the run,
   so no previous artifact can satisfy the build.
3. **Test compilation** — `cargo test --release --no-run`, captured separately.
4. **Regression** — `cargo test --release --no-fail-fast`, parsed into per-suite counts.

Each stage records its exact command, exit code, UTC start and finish, duration and a
merged stdout/stderr log. If the build fails, the later stages are recorded as
`skipped`; if test compilation fails, the regression is skipped. No stage is counted as
passed because it did not run.

The parser recognises `Running unittests src\lib.rs`, `Running tests\<name>.rs` and
`Doc-tests mink`, then attaches the following `test result:` block to that suite. It
collects ignored and failed test names, and flags a suite whose `Running` line has no
result block as unexecuted. An ignored test and a test that never ran are therefore
reported differently.

## 5. Verdicts and exit codes

| Verdict | Rule | Exit |
| --- | --- | ---: |
| `PASS` | All stages succeeded, suites parsed, no failures, artifact present, tracked tree clean. | 0 |
| `FAIL` | Any stage failed or any test failed. | failing stage's code, commonly 101 |
| `BLOCKED` | Required tool or repository prerequisite unavailable. | 2 |
| `PARTIAL` | Execution completed but evidence is incomplete: missing artifact, no parsed suites, unexecuted suite, tracked modifications. | 3 |

If a failing stage exits 0, 2 or 3, the runner reports 1 so a failure cannot be mistaken
for `PASS`, `BLOCKED` or `PARTIAL`. Untracked files are recorded but do not decide the
verdict; tracked modifications produce `PARTIAL` unless `--allow-dirty` is passed.

Reports go to `target/acceptance/<session>/<run-id>/` and are never overwritten: a
collision appends `-2`, `-3`, and so on. Build output stays under
`target/acceptance-work/<run-id>/`. Both trees are git-ignored.

## 6. Real acceptance run

Executed through the new entry point on the starting tree:

```
python tools/release_acceptance.py --session 276 --label baseline
```

Run id `20261113T072925Z-1b2f324-baseline`.

| Item | Result |
| --- | --- |
| Verdict | **PASS** (runner exit 0) |
| Commit | `1b2f3249b51a7fab708fb4288c45edd797e34e93` |
| rustc / cargo | 1.97.1 / 1.97.1 |
| Host / LLVM | `x86_64-pc-windows-msvc` / 22.1.6 |
| Cargo config | `.cargo/config.toml` (`+crt-static`), default release profile, 0 dependencies |
| Build | exit 0, 29.960 s |
| Test compilation | exit 0, 15.602 s |
| Regression | exit 0, 116.286 s |
| Artifact | 4,242,944 bytes, SHA-256 `b6918dfad1c84c832b6d6780a40f37a7b3268a4f4d3404a263a09f9b60b177ea`, `mink 1.0.5` |
| Suites | 91 |
| Passed / failed / ignored | **3,436 / 0 / 9** |
| Unexecuted suites | none |
| Failures | none |
| Limitations | six untracked paths (four preserved user files plus the two new tool files) |

The nine ignored tests are the eight bounded/extended soaks in `tests/soak.rs` and
`tls_lib::s62_system_store_public_https`. The counts match the Session 275 release run
and the Session 274 debug baseline exactly.

The build was 29.960 s against Session 275's 20.765 s. That is host variance on a
different target path, not a regression, and no performance claim is made.

### Artifact hash versus Session 275

Session 275 recorded 4,242,944 bytes and SHA-256
`92bd263adcf11094ef89ef1743d84849fb2f6c22bbad7345adbbdee77d2da436`. This run has the
same size and version but SHA-256 `b6918dfa…b177ea`. The Cargo target directory name is
embedded in the PE/PDB path, so a clean build in a differently named directory produces
different bytes. This confirms the existing M6 note that a source build is not
bit-reproducible across target paths; the workflow records the hash as evidence and does
not compare hashes between runs. Session 275's figures remain its own record.

## 7. Automation self-tests

`python tools/test_release_acceptance.py` — **11 tests, 11 passed**, about 3.4 s. The
tests inject a fake `cargo`, so they neither build nor test MINK.

| Test | Check |
| --- | --- |
| `test_success_generates_reports_and_pass` | PASS, exit 0, JSON/Markdown/logs created, size and SHA-256 match the artifact, four parsed suites. |
| `test_build_failure_propagates_and_still_reports` | Build exit 42 becomes runner exit 42, verdict FAIL, later stages skipped, report still written. |
| `test_test_failure_is_distinguished_from_ignored` | One failed test, one ignored test: verdict FAIL, exit 101, names kept separate. |
| `test_ignored_test_alone_does_not_fail` | Ignored count 1 with zero failures still PASS. |
| `test_missing_tool_is_blocked_and_reported` | Missing cargo: BLOCKED, exit 2, empty stage list, report written. |
| `test_missing_binary_is_partial` | Build exits 0 without an artifact: PARTIAL, exit 3. |
| `test_unparsable_test_output_is_partial` | Exit 0 with no result blocks: PARTIAL, zero suites. |
| `test_test_compile_failure_skips_regression` | `--no-run` exit 101: FAIL, regression skipped. |
| `test_repeated_runs_preserve_previous_reports` | Two runs create distinct directories; the first report is byte-identical afterwards. |
| `test_paths_with_spaces` | Output directory and fake cargo path contain spaces; PASS and reports written. |
| `test_tracked_changes_mark_partial` | A modified tracked file yields PARTIAL, exit 3. |

## 8. Safety

- No MINK source, test, expectation, compiler setting or profile changed.
- Failure injection uses a fake `cargo` in a temporary directory, not a damaged checkout.
- Reports and build output are inside git-ignored `target/`; no binary or log is staged.
- Staging and commit are explicit; the four preserved user files are untouched.
- No npm publication, tag or GitHub release.

## 9. Verdict

**Session 276: PASS.** A documented entry point exists, performs a fresh release build,
runs the complete release regression, captures artifact metadata, writes valid JSON and
Markdown reports, propagates failures, handles missing prerequisites, preserves earlier
reports, passes its own positive and negative tests, and its documentation is updated.

MINK publication readiness is unchanged: **NOT READY**. M6 external developer validation
remains **OUTSTANDING**, M7 is demand-gated, M8 gated, Linux frozen, and 1.0.5 remains
unpublished.
