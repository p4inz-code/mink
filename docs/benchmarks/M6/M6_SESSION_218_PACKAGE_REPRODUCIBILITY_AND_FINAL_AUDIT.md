# Session 218 — M6 Package Reproducibility and Final Internal Audit

Windows x86_64, 2026-11-08. Starting commit
`ebb3bc8e7e1e40345aebfbb3cbb65eb5553eea17` (Session 217, local `main` ==
`origin/main` verified before work began). Worktree clean except the four
pre-existing untracked user files, which were not staged or modified.

## 1. Scope and M6 boundary

Verify that an independent developer can follow
`docs/validation/M6_EXTERNAL/` against the exact 1.0.5 source/build without
private implementation knowledge, and correct the package only where the
evidence shows a mismatch.

This session does **not** complete M6. No independent tester has started the
protocol. The executing agent is not an independent developer, internal package
verification is not external acceptance, and no tester feedback or finding was
invented. The package is marked **READY FOR EXTERNAL EVALUATION**; M6 remains
**OUTSTANDING**.

## 2. Documents inspected

- `docs/validation/M6_EXTERNAL/README.md` (package index and instructions)
- `docs/validation/M6_EXTERNAL/EVALUATION_PROTOCOL.md` (T1–T18 and thresholds)
- `docs/validation/M6_EXTERNAL/VALIDATION_RECORD.md` (empty external record)
- The Session 217 documentation corrections in `docs/language/`, `docs/roadmap/`
  and `docs/brand/`.

## 3. Clean source-build path

The documented URL was checked first, then the build was run from an isolated
clone of the same commit.

| Check | Result |
| --- | --- |
| `git remote get-url origin` | `https://github.com/p4inz-code/mink` |
| `git ls-remote https://github.com/p4inz-code/mink.git HEAD` | `ebb3bc8e7e1e40345aebfbb3cbb65eb5553eea17` — equals local HEAD |
| `git clone --no-hardlinks . tmp/s218_clone` | exit 0; clone HEAD `ebb3bc8`; 0 changed paths |
| `rustc --version` / `cargo --version` | `1.97.1 (8bab26f4f 2026-07-14)` / `1.97.1`; LLVM 22.1.6, MSVC host |
| `cargo build --release` in the clone | **exit 0**, 21.38 s cargo time (22 s wall) |
| Build warnings / errors | **60 warnings** (`missing_docs` class), **0 errors** |
| `./target/release/mink.exe --version` | `mink 1.0.5`, exit 0 |
| Built binary size / SHA-256 | 4,048,384 bytes / `312f7bfd...c6c91a5` |
| Build-and-run a program from the clone root | `7`, exit 0 |

The clone was used instead of cloning over the network so the build could not
depend on a transient connection; `git ls-remote` confirms the documented
remote URL points at the same commit. No source, manifest or packaged-binary
file differs between the Session 213 build commit `f851947` and this HEAD
(empty `git diff --stat f851947..HEAD -- src stdlib Cargo.toml Cargo.lock
npm/mink/bin/mink.exe`).

## 4. Exact binary identity

| Binary | Version | Size | SHA-256 |
| --- | --- | ---: | --- |
| Package reference, `target/s213_clean/release/mink.exe` | `mink 1.0.5` | 4,048,384 | `87a1cae4946f121e292d1a516e6047ad287b6506c5fe52a16208717dc2b6da23` |
| Current `target/release/mink.exe` | `mink 1.0.5` | 4,048,384 | `8b3b28469ae4bc9d79f0f8bcf6ca63112ba9357dfef3f9ab2b71e1c49eccf4ee` |
| Session 218 clean clone build | `mink 1.0.5` | 4,048,384 | `312f7bfd3036c5e71575e34cef6190c72ee3b135a76db098f5e9f4dceb6c91a5` |
| `npm/mink/bin/mink.exe` (unpublished package payload) | `mink 1.0.5` | 4,028,416 | `c288115d8489571df2384953170e67da9bbcd45c3bedbf569db65714a2479401` |

The package's recorded size and SHA-256 **match the Session 213 reference
binary exactly**. Two other 1.0.5 builds are the same size and version but have
different hashes, so a checksum is an identity check for the supplied copy, not
a property of every 1.0.5 build. The package now records the reference path and
explains this distinction.

No separate coordinator-supplied path was present in the package. The exact
binary intended for evaluation is the one whose hash matches the package, and
that path is `target/s213_clean/release/mink.exe`. A supplied copy that does not
match must be stopped and reported, as the package already instructs.

## 5. Package instruction checks

| Check | Result |
| --- | --- |
| Path A layout: `mink.exe` plus `stdlib/` | 27 `.mink` modules copied; binary printed `mink 1.0.5`; checksum matched §4 |
| `./mink.exe help` | exit 0; command list present (8 command lines) |
| `./mink.exe explain` / `explain E-T01` | exit 0; 8 lines / `Error E-T01: Type Mismatch` |
| Quick start exactly as written (build from `hello/`, run `./hello.exe`) | build exit 0; printed `42`; run exit 0 |
| `mink check` / `mink run` on the same file | both exit 0; `run` printed `42` |
| Path B build from the clone root | exit 0; printed `7`; run exit 0 |
| `mink test` on a clean two-test file | 2 found, **2 passed, 0 failed**, exit 0 |
| `mink test` on internal `tests/json/test_limits.mink` | 4 found, 2 passed, 2 failed, exit 1 — that fixture allocates and never frees, so `E-R06` is the leak checker working; it is not a package acceptance test |
| All relative links in the three package documents | 10 checked, **0 missing** |
| `examples/*/main.mink` | **15** present, matching the package list |
| `benchmarks/m6/session176/01_hello.mink` … `10_imports.mink` | **10** present |
| `validation/*/main.mink`, `verify.py`, `python_ref.py` | **10 / 10 / 10** present |
| `stdlib/*.mink` | **27** present |
| Example build/run `examples/freq_count/main.mink` | build exit 0; no-argument run exit 0 |
| `examples/freq_count/main.exe --help` | **did not print help**; the program treated it as an argument. The package instruction was corrected to the no-argument form |

Package corrections made this session:

1. Added the **READY FOR EXTERNAL EVALUATION** status and the explicit statement
   that this is not external validation and M6 remains OUTSTANDING.
2. Recorded the reference binary path, the build-source commit, and the fact
   that normal rebuilds can be the same size/version with a different hash.
3. Changed the source-build estimate from `~31 s` to `~21-31 s` (Session 213:
   30.78 s; Session 218 clean clone: 21.38 s), with the same 60 warnings and
   0 errors.
4. Removed the misleading `--help` invocation for `examples/freq_count`.
5. Replaced the nonexistent `tests_example.mink` with a `your_tests.mink`
   placeholder and instructions to create the file.

No product capability, limitation, source file or test was changed.

## 6. Protocol consistency

| Check | Result |
| --- | --- |
| Task identifiers | T1–T18, **sequential, no gaps or duplicates** |
| Stated time boxes | 10, 10, 20, 20, 25, 25, 30, 25, 25, 25, 25, 30, 25, 30, 30, 30, 40, 30 min |
| Total time boxes | 455 min (7 h 35 min) |
| Required minimum set T1, T2, T3, T6, T8, T9, T17, T18 | 185 min (3 h 05 min) |
| Suggested effort | 4–6 h; Session 217 added the arithmetic showing the required subset fits |
| Per-task evidence fields | id, timing, outcome, expected vs actual, exit codes, errors, documentation used, severity, notes — all present |
| Coverage map | 8 criteria, including installation, documentation, compilation, stdlib, diagnostics, application experience, reproducible defects, limitations |
| Required minimum tasks in acceptance | the same 8 tasks listed in threshold 1 |
| Acceptance thresholds | 5 conditions, including no open S1, no open S2 in attempted tasks, fix and independent retest, documentation corrections, verbatim consent |
| Retest procedure | retry only the affected task after a fix; internal verification never substitutes |
| Reporting without implementation material | package §1 excludes `docs/implementation/`, `docs/benchmarks/` and `docs/audits/`; report template is self-contained in `DEBUGGING_AND_TESTING.md` §8 and package §10 |

The protocol is internally consistent. Its empty external record remains empty
because no tester has run it.

## 7. Tests and commands actually run

| Command or check | Count / result | Exit |
| --- | --- | --- |
| `cargo build --release` (clean clone) | 60 warnings, 0 errors, 21.38 s | 0 |
| Reference binary `--version` | `mink 1.0.5` | 0 |
| Package-layout quick start | `42` | 0 (build and run) |
| Path B build and run | `7` | 0 (build and run) |
| `mink test` clean two-test file | 2 passed, 0 failed, 2 total | 0 |
| `mink test` internal leak fixture | 2 passed, 2 failed, 4 total | 1 (expected for that fixture) |
| Link and path inventory | 10 links; 15/10/10/10/27 files | 0 missing |
| `git diff --check` before commit | no whitespace errors | 0 |

The full Rust regression suite was **not** run: the tree is source-identical to
Session 213, which already records 85 suites / 3,262 passed / 0 failed /
9 ignored in both debug and release. Running it again would not test the
package, and the package itself tells external testers not to run `cargo test`.

## 8. Gate result

| Gate item | Result |
| --- | --- |
| Package reproducibility from a clean clone | **PASS** |
| Exact reference binary version / size / SHA-256 | **PASS** — matches the package |
| Referenced paths, examples, tutorials, validation apps | **PASS** |
| 18-task protocol internal consistency | **PASS** |
| Package readable without implementation/audit material | **PASS** — exclusions and self-contained report template |
| Package status | **READY FOR EXTERNAL EVALUATION** |
| M6 external validation | **OUTSTANDING** — no independent tester |
| Publication | **NOT DONE** — 1.0.5 unpublished, no tag or release |

The package is ready to hand to a developer. Readiness to evaluate is not
validation, and this session does not move M6.

## 9. Commit

Commit message: `docs: verify the M6 package from a clean build`.
