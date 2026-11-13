# MINK External Validation Package (M6)

**Package version:** 1.0.5 (repository build)
**Prepared:** 2026-11-08, Session 215
**Re-verified:** 2026-11-08, Session 237
**Inventory checked:** 2026-11-13, Session 274
**Audience:** a developer who did not build MINK and has no prior MINK knowledge
**Platform:** Windows x86_64
**Package status:** **READY FOR EXTERNAL EVALUATION** (internal reproducibility
check passed 2026-11-08, Session 218; instructions and task expectations
re-reviewed in Sessions 229–238). This is not external validation; M6 remains
OUTSTANDING.

This package exists so an independent developer can install MINK, learn it from
its own documentation, build practical programs, and report what actually
happened. Nothing here requires access to the MINK repository history, private
notes or the project owner.

## 1. What is in the package

| Item | Where |
| --- | --- |
| Compiler | `mink.exe` 1.0.5 supplied by the coordinator, or build it from source (§3) |
| Version and checksum | §4 |
| Installation | §3 and [`docs/language/WINDOWS_INSTALLATION.md`](../../language/WINDOWS_INSTALLATION.md) |
| Quick start | §5 and [`docs/language/LANGUAGE_BASICS.md`](../../language/LANGUAGE_BASICS.md) |
| Syntax reference | [`docs/language/CORE_GRAMMAR.md`](../../language/CORE_GRAMMAR.md) |
| Type system | [`docs/language/TYPE_SYSTEM.md`](../../language/TYPE_SYSTEM.md) |
| Standard library | [`docs/language/STANDARD_LIBRARY_REFERENCE.md`](../../language/STANDARD_LIBRARY_REFERENCE.md) |
| Errors and testing | [`docs/language/DEBUGGING_AND_TESTING.md`](../../language/DEBUGGING_AND_TESTING.md) |
| Small programs | `benchmarks/m6/session176/01_hello.mink` … `10_imports.mink` |
| Practical applications | `examples/` — the 15 programs supported by the 1.0.5 reference binary, each a `main.mink` (§1.2 explains the seven newer ones) |
| Validation-sized workloads | `validation/a1_fileinspect` … `validation/h_manifest` (10 programs) |
| Evaluation tasks and expected outcomes | [`EVALUATION_PROTOCOL.md`](EVALUATION_PROTOCOL.md) |
| Evidence record and report format | [`VALIDATION_RECORD.md`](VALIDATION_RECORD.md) |
| Per-task submission form | [`SUBMISSION_TEMPLATE.md`](SUBMISSION_TEMPLATE.md) |
| Troubleshooting | [`FAQ.md`](FAQ.md) |
| Acceptance and closure checklist | [`ACCEPTANCE_CHECKLIST.md`](ACCEPTANCE_CHECKLIST.md) |

### 1.1 Handoff manifest

The coordinator delivers either a clone of this repository at the commit under
test, or a folder with the same items:

- `mink.exe` (Path A; it must match the reference checksum in §4) or the source
  tree for Path B;
- `stdlib/` — the 27 `.mink` modules;
- `docs/language/` — the six reference pages linked in §1;
- `examples/` (15 programs), `validation/` (10 applications), and
  `benchmarks/m6/session176/` (the 10 tutorial programs `01_hello.mink` …
  `10_imports.mink`);
- this README, `EVALUATION_PROTOCOL.md`, `VALIDATION_RECORD.md`,
  `SUBMISSION_TEMPLATE.md`, `FAQ.md`, and `ACCEPTANCE_CHECKLIST.md`.

A folder containing only `mink.exe` is not sufficient: the documentation,
examples, and protocol are part of the evaluation. The coordinator keeps
`FINDINGS_TRIAGE.md` for handling returned reports.

### 1.2 Examples added after the reference package

The repository now contains 22 example directories. Seven use language work
added after the 1.0.5 reference binary was built and **require a newer source
build**, so they are not part of the Path A evaluation and must not be scored
against that binary: `struct_match_geometry` and `struct_match_dispatch` (struct
patterns in `match`, Session 239), and `defer_scope_order`, `defer_resource`,
`defer_move_resource`, `defer_move_multi` and `defer_move_pipeline` (`defer` and
`defer move`, Sessions 249–273). The 15-program package set above is unchanged.

The package is the repository itself; no separate archive is required. Do not
read `docs/implementation/`, `docs/benchmarks/` or `docs/audits/` while
evaluating: those describe how MINK was built, and using them would invalidate
the documentation-usability part of the evaluation.

## 2. What MINK is (one paragraph)

MINK is a systems-oriented native programming language for Windows x86_64. It
compiles `.mink` source directly to a standalone `.exe` with its own lexer,
parser, type checker, optimizer and x86-64 code generator. It has explicit
ownership with compile-time borrow checking, a tracked heap that is
leak-checked at exit, and a 27-module standard library covering filesystem,
processes, sockets, TLS, HTTP, SQLite, ZIP/DEFLATE, threads and async. Generated
executables import only Windows system libraries.

## 3. Installation

Two supported paths. Use the first one if the coordinator supplied a binary.

### Path A — supplied binary (no Rust required)

1. Create a working directory, for example `C:\mink-eval`.
2. Put `mink.exe` there.
3. Put the standard library beside it, in `C:\mink-eval\stdlib\` (the 27
   `.mink` files listed in §1.1). MINK resolves modules from
   `<folder containing mink.exe>\stdlib` first, then `<current directory>\stdlib`.
4. Verify the checksum against §4.
5. Open Git Bash in `C:\mink-eval` and run:

```bash
./mink.exe --version
./mink.exe help
```

Expected: `mink 1.0.5` and the command list from §5.

### Path B — build from source (Rust required)

Requires the MSVC Rust toolchain. The build was verified with rustc 1.97.1.

```bash
git clone https://github.com/p4inz-code/mink.git
cd mink
rustc --version          # must be a stable MSVC toolchain
cargo build --release    # ~21-31 s on the verification host, 60 warnings, 0 errors
./target/release/mink.exe --version
```

Run programs **from the repository root** so that `./stdlib` is found, or copy
`stdlib/` beside `target/release/mink.exe`. The `../mink.exe` in the §5 quick
start is the Path A layout; for Path B run `./target/release/mink.exe` from the
repository root.

Do not use `npm install -g @p4inz-code/mink` for this evaluation: the registry
still serves 1.0.4, which is not the version under evaluation and has known
standard-library defects. 1.0.5 is unpublished by policy.

## 4. Exact version information

| Item | Value |
| --- | --- |
| Compiler version | `mink 1.0.5` |
| Binary (release, Windows x86_64) | 4,048,384 bytes |
| SHA-256 | `87a1cae4946f121e292d1a516e6047ad287b6506c5fe52a16208717dc2b6da23` |
| Reference binary | `target/s213_clean/release/mink.exe` (Session 213 clean build, 2026-11-08; git-ignored) |
| Build source | compiler source commit `f851947`; the reference binary is unchanged through `1f6f79a`. `stdlib/` gained post-package fixes in `bf3fe81` (ASCII whitespace, Sessions 219–228), `18eae20` (npm bundle sync, Sessions 254–258) and `a02e6aa` (`json_str_val` borrow, Session 273); they affect compiled programs, not the compiler checksum. Source after `f851947` also contains struct patterns (239), `defer` (249–273) and element-level aggregate ownership (273), which the reference binary does not implement; a Path B source build therefore yields a newer compiler and must not be identified with the reference binary. |
| Target | `x86_64-windows-pe` |
| Build toolchain | rustc 1.97.1 (8bab26f4f 2026-07-14), LLVM 22.1.6 |
| Package state | **unpublished**; npm registry serves 1.0.4, not this version |
| Supported platform | Windows 10/11 x86_64 only |
| Linux / macOS | not supported (frozen / absent) |

Record the exact `mink --version` output and the checksum you observe in the
evidence record. If they differ from the table, stop and report it.

A normal `cargo build --release` can report the same version and produce the
same byte size with a different checksum. Session 218 measured
`312f7bfd3036c5e71575e34cef6190c72ee3b135a76db098f5e9f4dceb6c91a5` from a
clean clone and `8b3b28469ae4bc9d79f0f8bcf6ca63112ba9357dfef3f9ab2b71e1c49eccf4ee`
from `target/release`; both were 4,048,384 bytes and reported `mink 1.0.5`. The
checksum above identifies the exact reference binary. Compare the copy you were
given, not its filename.

Session 237 re-verified the source path at `1f6f79a`: `cargo build --release`
exited 0 in 21.36 s with 60 warnings and 0 errors, `mink --version` printed
`mink 1.0.5`, the binary was 4,048,384 bytes with SHA-256
`67450d40347029de902fb921242c42fa2a4281776f27f8ff84771a286e29717f` (a normal
self-build, expected to differ from the reference binary as explained above),
and a `mod filesystem;` program ran from the clone root. Path A's supplied
binary must match the reference checksum; a Path B self-build need not.

## 5. Quick start

```bash
mkdir hello && cd hello
printf 'fn main() -> Int {\n    rt_print_int(42);\n    return 0;\n}\n' > hello.mink
../mink.exe build hello.mink
./hello.exe
```

Expected: `42`, exit code 0. `build` writes `hello.exe` beside the source.
`run` compiles and runs in one step; `check` runs the front end only. For Path B,
replace `../mink.exe` with `./target/release/mink.exe` and run from the
repository root.

Programs are one or more `.mink` files. `fn main` is the entry point;
`main() -> Int` becomes the process exit code. Output is line-oriented: every
`rt_print_*` call writes one complete line ending in CRLF, so build a line as
one `Str` and print it once rather than concatenating two prints.

Useful commands:

```bash
mink build <file>       # compile to .exe
mink run <file>         # compile and execute
mink check <file>       # front end only, exit 1 on errors
mink test <file>        # run top-level fn test_* functions
mink explain E-T01      # explain an error code
mink explain            # list documented codes
mink repl               # interactive session
```

## 6. Where to start reading

Read in this order, and note where you had to guess:

1. `docs/language/WINDOWS_INSTALLATION.md` — install and first program.
2. `docs/language/LANGUAGE_BASICS.md` — runtime semantics, output, ownership,
   modules.
3. `docs/language/CORE_GRAMMAR.md` — syntax authority.
4. `docs/language/STANDARD_LIBRARY_REFERENCE.md` — module-by-module contracts,
   with EXEC / SOURCE / PARTIAL / UNKNOWN evidence markers.
5. `docs/language/DEBUGGING_AND_TESTING.md` — diagnostics, exit codes, bug
   report template.

The reference documents were audited against 1.0.4 and now name 1.0.5 as the
current repository build. Where a page records a 1.0.4-specific result, treat it
as the audit baseline and verify it as you go; report any statement that turns
out to be wrong for 1.0.5 as a documentation defect.

## 7. Example programs

| Example | Shows |
| --- | --- |
| `examples/freq_count` | filesystem read, map, sorting, formatted output |
| `examples/regex_tool` | regular expressions over file input |
| `examples/fs_util` | create, list, copy, remove |
| `examples/inventory_db` | SQLite schema, insert, query, update |
| `examples/http_client` | HTTP and HTTPS requests |
| `examples/tcp_client` | TCP connect, send, receive |
| `examples/threaded_work` | OS threads and shared state |
| `examples/async_tasks` | task scheduling |
| `examples/compress_util` | ZIP/DEFLATE |
| `examples/log_util`, `sysinfo`, `system_report`, `env_report` | logging, environment, system data |
| `examples/runtime_error_report` | runtime error reporting |
| `examples/proc_util` | child processes and exit codes |

Each is a single `main.mink`. Build from the repository root:

```bash
./target/release/mink.exe build examples/freq_count/main.mink
./examples/freq_count/main.exe
```

Not every example implements `--help`; these are small programs, not a uniform
CLI toolkit.

`benchmarks/m6/session176/01_hello.mink` … `10_imports.mink` are the shortest
progression from variables to imports; start there if the language is entirely
new. `validation/*/main.mink` are larger programs with a Python reference
implementation beside them (`python_ref.py`) and a comparison harness
(`verify.py`); they are useful after the tasks in the protocol.

## 8. Test instructions

MINK has built-in test discovery. Write a file containing top-level `fn test_*`
functions, then run:

```bash
mink test your_tests.mink   # runs fn test_* functions
```

A test function fails by returning a non-zero exit path through the assertion
helpers documented in `STANDARD_LIBRARY_REFERENCE.md` §21. For a program without
tests, verify behaviour by running it and comparing stdout, stderr and exit code
against an expectation you write down **before** running it.

Do not run the repository's Rust test suite (`cargo test`) for this evaluation;
that is internal verification and is not part of the external tasks.

## 9. Known limitations

These are documented, intentional or accepted, and are not defects to report
(mentioning them is still useful as feedback):

- Windows x86_64 only; Linux is frozen and macOS is absent.
- 1 GiB arena and a bounded live-allocation ceiling; exhaustion is reported as
  a failure, not silently handled.
- ANSI paths; Unicode filenames and paths beyond `MAX_PATH` are not supported.
- No console control-handler API (Ctrl+C handling), no thread pool, no
  condition variables, no dynamic thread priority.
- SQLite is an in-memory SQL engine (the database is an opaque `Str` blob),
  not a file-backed SQLite binding; it does not persist across processes.
  Correctness and practical workloads are claimed; performance parity with
  native SQLite is not.
- stdin is read-all rather than line-streamed; it is not a line-oriented
  terminal reader.
- Soak testing is bounded, not a long-duration guarantee.
- No claim of superiority over any other language.
- The published npm package is 1.0.4 and differs from 1.0.5; do not mix them.

## 10. Reporting bugs and feedback

Use the per-task form in [`SUBMISSION_TEMPLATE.md`](SUBMISSION_TEMPLATE.md) and
the bug template in `DEBUGGING_AND_TESTING.md` §8. Every report should contain:

1. `mink --version` output and the binary checksum.
2. Windows version and architecture.
3. The smallest source file that reproduces the problem.
4. The exact command, stdout, stderr and exit code.
5. What you expected and why (quote the document you relied on).
6. Whether it reproduces on a second run and in a fresh directory.

Classify each report as compiler defect, runtime defect, standard-library defect,
documentation defect, usability issue, missing capability, intentional language
difference, environment/test issue, or invalid report. Severity:

| Severity | Meaning |
| --- | --- |
| S1 | Blocks installation or any program from running |
| S2 | Wrong result, crash, corruption, or a documented behaviour that does not hold |
| S3 | Diagnostic or documentation problem that costs real time but has a workaround |
| S4 | Cosmetic or preference |

A report becomes a confirmed finding only after the coordinator reproduces it
against the reference binary; see [`FINDINGS_TRIAGE.md`](FINDINGS_TRIAGE.md).
Send reports to the coordinator who gave you this package. Do not open public
issues without the owner's agreement, and do not include credentials, personal
paths or unrelated files.

## 11. Privacy and consent

Only information you agree to share is recorded: your background in general
terms, your environment, your transcript excerpts, and your feedback. Your name
and contact details are optional. Nothing is published, and no account or
credential is required.
