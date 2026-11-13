# Release acceptance workflow

`tools/release_acceptance.py` is the repository-native entry point for MINK release
acceptance. It runs the authoritative release build, release test compilation and the
full release regression in a fresh Cargo target directory, captures the evidence, and
computes the verdict itself. A successful script exit is never treated as acceptance on
its own: the verdict comes from the stage results and the parsed test output.

## Prerequisites

- Windows x86_64 with the `x86_64-pc-windows-msvc` Rust toolchain (the toolchain used by
  Session 275 is rustc/cargo 1.97.1).
- Python 3.9 or newer on `PATH`.
- Git on `PATH`, with the working tree in the repository to verify.
- No network access is required; MINK has no Cargo dependencies.

Run from the repository root:

```bash
python tools/release_acceptance.py --session 276
```

Useful options:

| Option | Meaning |
| --- | --- |
| `--session ID` | Report directory and title label (default `unlabelled`). |
| `--label NAME` | Optional suffix on the run id, for example `baseline` or `retry`. |
| `--repo PATH` | Repository to verify (default: the checkout containing the script). |
| `--out PATH` | Report root (default: `<repo>/target/acceptance`). |
| `--timeout SECONDS` | Per-stage timeout (default 1800). |
| `--allow-dirty` | Do not treat tracked worktree changes as `PARTIAL`. |

## Stages

Each stage runs with `stdout` and `stderr` merged into one chronological log, so Cargo's
`Running ...` lines and libtest's result lines stay in order.

1. **Environment**: records rustc/cargo versions, host, LLVM, branch, HEAD, worktree
   status, `.cargo/config.toml`, the `[profile.release]` section (or the absence of one)
   and the dependency count. Missing `cargo`, `rustc` or `git` produces `BLOCKED`.
2. **Build**: `cargo build --release` with `CARGO_TARGET_DIR` pointing at a new
   `target/acceptance-work/<run-id>/cargo-target`. The directory did not exist before the
   run, so a previous artifact cannot satisfy the check.
3. **Test compilation**: `cargo test --release --no-run`, reported separately from test
   execution.
4. **Regression**: `cargo test --release --no-fail-fast`, parsed per result block.

If the build fails, the later stages are recorded as `skipped`, not as passed. If test
compilation fails, the regression is skipped.

## Outputs

Reports are written under `<out>/<session>/<run-id>/`:

```
report.json       machine-readable evidence (schema_version 1)
report.md         human-readable report with per-suite results
environment.txt   short key=value environment snapshot
logs/build.log
logs/test-compile.log
logs/tests.log
```

The run id is `YYYYMMDDTHHMMSSZ-<short-head>[-label]`, for example
`20261113T072925Z-1b2f324-baseline`. If that directory exists, `-2`, `-3`, ... is
appended. Earlier reports are never overwritten or deleted. The `target/` tree is
git-ignored, so reports and build output are not committed.

`report.json` contains `schema_version`, `session`, `label`, `run_id`, `generated_at`,
`verdict`, `exit_code`, `repository`, `toolchain`, `configuration`, `stages`, `artifact`,
`tests` and `reproduction`. Timestamps are ISO 8601 UTC with second precision
(`YYYY-MM-DDTHH:MM:SSZ`). `artifact` records the binary path, size, SHA-256, modification
time and `mink --version` output. `tests.totals` records suite, pass, fail, ignore and
unexecuted counts plus every ignored test name.

## Verdicts and exit codes

| Verdict | Meaning | Runner exit |
| --- | --- | ---: |
| `PASS` | All three stages succeeded, at least one suite result was parsed, no test failed, the artifact exists, and the tracked tree is clean. | 0 |
| `FAIL` | A build, test-compilation or regression stage failed, or a test failed. | the failing stage's exit code (commonly 101) |
| `BLOCKED` | A required tool or repository prerequisite is unavailable, so nothing ran. | 2 |
| `PARTIAL` | Execution completed but acceptance evidence is incomplete: missing artifact, no parsed suites, a suite started without a result, or tracked modifications. | 3 |

A failure whose stage exit code is 0, 2 or 3 is reported as exit code 1 so it cannot be
confused with `PASS`, `BLOCKED` or `PARTIAL`.

## Ignored tests and unexecuted tests

An **ignored** test is reported by libtest as `test <name> ... ignored` and counted in
that suite's ignored total. Ignored tests are intentional here: eight bounded/extended
soaks in `tests/soak.rs` and the opt-in public-internet TLS control
`tls_lib::s62_system_store_public_https`. They do not fail acceptance, but they are never
counted as passed.

An **unexecuted** suite is a target whose `Running ...` line has no following
`test result:` block, for example because the process aborted. It is listed in
`tests.totals.unexecuted_suites` and changes the verdict to `PARTIAL` unless a stage
failure already makes it `FAIL`. A stage that was not reached is recorded as `skipped`,
not as zero tests.

## Reproducing a failed stage

1. Open `report.md` and read the failing stage row and its log.
2. Copy the matching command from the `## Reproduction` section (or run it yourself with
   `CARGO_TARGET_DIR=<report configuration.target_dir>`).
3. For a single failing suite, run the narrow command, for example
   `cargo test --release --test adversarial`; then rerun the full workflow to confirm the
   complete regression.
4. To include the intentional ignores deliberately, add `-- --ignored`; this is not part
   of normal acceptance and must be recorded separately.

Do not edit tests or expectations to turn a failure into a pass.

## Validating the automation itself

```bash
python tools/test_release_acceptance.py
```

The suite uses a fake `cargo` and never builds MINK. It covers a successful run,
build-failure propagation, test-failure propagation, missing-tool handling, missing
binary, unparsable output, test-compilation failure, ignored-versus-failed separation,
report preservation across repeated runs, paths containing spaces, and tracked-change
detection. Session 276 records 11 passing tests.

## Limitations

- The SHA-256 identifies one built file, not the source. Rust embeds the target/PDB path,
  so two clean builds of the same commit in differently named target directories have
  the same size and version but different hashes. Session 275 and Session 276 show this;
  acceptance therefore records the hash instead of comparing it across runs.
- The runner verifies a build from the current worktree; it does not prove that the
  worktree matches a remote branch, and it does not fetch.
- Ignored soaks are counted, not executed. Stability soaks remain a separate run.
- Untracked files (including historical user files) are recorded as a limitation but do
  not decide the verdict. Tracked modifications do.
- Only Windows x86_64 is exercised; the frozen Linux backend is not.
- The workflow does not publish, tag or upload anything.
