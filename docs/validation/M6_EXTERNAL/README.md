# MINK External Validation Package (M6)

**Package version:** 1.0.5 (repository build)
**Prepared:** 2026-11-08, Session 215
**Audience:** a developer who did not build MINK and has no prior MINK knowledge
**Platform:** Windows x86_64

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
| Practical applications | `examples/` (15 programs, each a `main.mink`) |
| Validation-sized workloads | `validation/a1_fileinspect` … `validation/h_manifest` (10 programs) |
| Evaluation tasks and expected outcomes | [`EVALUATION_PROTOCOL.md`](EVALUATION_PROTOCOL.md) |
| Evidence record and report format | [`VALIDATION_RECORD.md`](VALIDATION_RECORD.md) |

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
   `.mink` files). MINK resolves modules from `<folder containing mink.exe>\stdlib`
   first, then `<current directory>\stdlib`.
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
cargo build --release    # ~31 s, 60 warnings, 0 errors
./target/release/mink.exe --version
```

Run programs **from the repository root** so that `./stdlib` is found, or copy
`stdlib/` beside `target/release/mink.exe`.

Do not use `npm install -g @p4inz-code/mink` for this evaluation: the registry
still serves 1.0.4, which is not the version under evaluation and has known
standard-library defects. 1.0.5 is unpublished by policy.

## 4. Exact version information

| Item | Value |
| --- | --- |
| Compiler version | `mink 1.0.5` |
| Binary (release, Windows x86_64) | 4,048,384 bytes |
| SHA-256 | `87a1cae4946f121e292d1a516e6047ad287b6506c5fe52a16208717dc2b6da23` |
| Target | `x86_64-windows-pe` |
| Build toolchain | rustc 1.97.1 (8bab26f4f 2026-07-14), LLVM 22.1.6 |
| Package state | **unpublished**; npm registry serves 1.0.4, not this version |
| Supported platform | Windows 10/11 x86_64 only |
| Linux / macOS | not supported (frozen / absent) |

Record the exact `mink --version` output and the checksum you observe in the
evidence record. If they differ from the table, stop and report it.

## 5. Quick start

```bash
mkdir hello && cd hello
printf 'fn main() -> Int {\n    rt_print_int(42);\n    return 0;\n}\n' > hello.mink
../mink.exe build hello.mink
./hello.exe
```

Expected: `42`, exit code 0. `build` writes `hello.exe` beside the source.
`run` compiles and runs in one step; `check` runs the front end only.

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

The reference documents were audited against 1.0.4. Where a page says
"Applies to 1.0.4", treat it as describing the same language surface as 1.0.5;
report any statement that turns out to be wrong for 1.0.5 as a documentation
defect.

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
./examples/freq_count/main.exe --help
```

`benchmarks/m6/session176/01_hello.mink` … `10_imports.mink` are the shortest
progression from variables to imports; start there if the language is entirely
new. `validation/*/main.mink` are larger programs with a Python reference
implementation beside them (`python_ref.py`) and a comparison harness
(`verify.py`); they are useful after the tasks in the protocol.

## 8. Test instructions

MINK has built-in test discovery:

```bash
mink test tests_example.mink   # runs fn test_* functions
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
- SQLite correctness and practical workloads are claimed; performance parity
  with native SQLite is not.
- Soak testing is bounded, not a long-duration guarantee.
- No claim of superiority over any other language.
- The published npm package is 1.0.4 and differs from 1.0.5; do not mix them.

## 10. Reporting bugs and feedback

Use the template in `DEBUGGING_AND_TESTING.md` §8. Every report should contain:

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

Send reports to the coordinator who gave you this package. Do not open public
issues without the owner's agreement, and do not include credentials, personal
paths or unrelated files.

## 11. Privacy and consent

Only information you agree to share is recorded: your background in general
terms, your environment, your transcript excerpts, and your feedback. Your name
and contact details are optional. Nothing is published, and no account or
credential is required.
