# MINK Tester Handoff (M6)

**You are being asked to evaluate MINK 1.0.5 on Windows x86_64.**
Expected effort: **4–6 hours**, split across two or three sittings.
You must not have contributed to MINK. Nothing in this evaluation is coached:
the coordinator answers access questions only (where the binary is, how to
return your report), never language or API questions.

Read this page first. It is the short version of what to do. The full details
are in `README.md` (package), `EVALUATION_PROTOCOL.md` (tasks),
`SUBMISSION_TEMPLATE.md` (your report form), `FAQ.md` (troubleshooting) and
`ACCEPTANCE_CHECKLIST.md` (how your submission is judged).

## 1. What MINK is

MINK is a systems-oriented native programming language for Windows x86_64. It
compiles `.mink` source directly to a standalone `.exe` using its own lexer,
parser, type checker, optimizer and x86-64 code generator — there is no C
toolchain, VM or runtime to install. It has explicit ownership with compile-time
borrow checking, a tracked heap that is leak-checked at exit, and a 27-module
standard library (filesystem, processes, sockets, TLS, HTTP, SQLite, ZIP/DEFLATE,
threads, async). Generated executables import only Windows system libraries.

## 2. Supported platform and prerequisites

| Item | Requirement |
| --- | --- |
| OS | Windows 10 or 11, x86_64 only |
| Shell | Git Bash recommended; PowerShell works |
| Python | 3.8+ only if you choose to run the packaged tutorial verifier |
| Rust | Only for installation Path B (MSVC toolchain, verified with rustc 1.97.1) |
| Other | No account, license key or network access required; T13 may use the internet if available |

Linux and macOS are **not** supported. This is a documented limitation, not a
defect.

## 3. Exact setup and build

### Path A — supplied binary (recommended; no Rust)

```bash
mkdir -p /c/mink-eval && cd /c/mink-eval
# copy mink.exe here, and the 27 stdlib/*.mink files into ./stdlib/
sha256sum mink.exe
./mink.exe --version
./mink.exe help
```

The binary must report `mink 1.0.5`, be 4,048,384 bytes, and have SHA-256
`87a1cae4946f121e292d1a516e6047ad287b6506c5fe52a16208717dc2b6da23`.
If it differs, stop and report that instead of continuing — you were given the
wrong artifact.

MINK resolves modules from `<folder containing mink.exe>\stdlib` first, then
`<current directory>\stdlib`.

### Path B — build from source (Rust required)

```bash
git clone https://github.com/p4inz-code/mink.git
cd mink
rustc --version          # stable MSVC toolchain
cargo build --release    # ~22-31 s, 62 warnings, 0 errors
./target/release/mink.exe --version
```

Run programs from the repository root so `./stdlib` is found, or copy `stdlib/`
beside `target/release/mink.exe`. A self-built binary reports `mink 1.0.5` but
has a different checksum from the reference binary, which is expected. **Do not
use `npm install -g @p4inz-code/mink`**: the registry serves 1.0.4, which is not
the version under test and has known standard-library defects.

If you build from source, be aware that it also contains `struct` patterns,
`defer` and element-level aggregate ownership that the supplied 1.0.5 reference
binary does not implement. Use one path consistently and say which you used.

### First program (sanity check, ~2 min)

```bash
mkdir hello && cd hello
printf 'fn main() -> Int {\n    rt_print_int(42);\n    return 0;\n}\n' > hello.mink
../mink.exe build hello.mink
./hello.exe
```

Expected: `42`, exit code 0.

## 4. Required tasks

The protocol defines 18 tasks (T1–T18) with time boxes. You do **not** have to
finish all of them, and partial coverage with honest notes is more useful than a
rushed pass. These eight are **required** for acceptance — about 3 hours total:

| Task | Budget | What you must do |
| --- | ---: | --- |
| **T1** | 10 min | Install by §3; run `--version`, `help`, `explain`; record the checksum you measured |
| **T2** | 10 min | Build and run a program printing `42`; copy the `.exe` elsewhere and run it there |
| **T3** | 20 min | Write three deliberately broken programs (missing `;`, undefined fn, type mismatch); run `mink check` on each, then `mink explain` on each reported code |
| **T6** | 25 min | Struct with ≥2 fields; enum with ≥2 data-carrying variants plus one unit variant; consume it with `match`; observe a non-exhaustive `match` |
| **T8** | 25 min | List, map and set: insert, look up, update, iterate, remove; print a deterministic sorted summary |
| **T9** | 25 min | Create a directory; write, read, read-modify-write (there is no append mode), list and delete a file; then read a missing file |
| **T17** | 40 min | CLI tool: ≥2 subcommands, `--help`, required positional argument, one flag, stdin input via a pipe, meaningful exit codes (0 success, non-zero on bad usage and on processing failure) |
| **T18** | 30 min | Answer the seven experience questions in `EVALUATION_PROTOCOL.md` §3 T18 |

Optional tasks add confidence but cannot replace a missing required task:
T4 types, T5 ownership, T7 generics/modules, T10 JSON, T11 CSV, T12 SQLite,
T13 HTTP/HTTPS, T14 TCP, T15 concurrency, T16 multi-file project.

Ground rules, so your result is usable:

- Work in your own directory. **Do not read `docs/implementation/`,
  `docs/benchmarks/` or `docs/audits/`** — they describe how MINK was built and
  would invalidate the documentation-usability part of the evaluation.
- Use only the package documentation, the compiler's own diagnostics and
  `mink explain`.
- Before running anything, write down what you expect, then run it.
- Time-box each task. If the budget runs out, stop, record where you got stuck
  and move on. Being stuck is data, not failure.
- Keep a raw transcript. Do not tidy it up or delete failed attempts.

## 5. Expected results

Per-task expectations are stated in `EVALUATION_PROTOCOL.md` §3 and summarised
in `SUBMISSION_TEMPLATE.md` §2. The package-level expectations are:

| Area | Expected on a correct run |
| --- | --- |
| Install (T1, T2) | `mink 1.0.5`; command list includes `build`, `run`, `check`, `test`, `repl`; `explain` with no argument lists ~50 codes; `hello.exe` prints `42` and runs standalone anywhere |
| Diagnostics (T3) | Every broken program exits non-zero with an error code plus file and line; `mink explain <code>` matches the situation |
| Language (T6) | Program builds and runs; non-exhaustive `match` is rejected and, ideally, names the missing variant |
| Collections (T8) | Output identical across two runs; `rt_map_has` tests absence; `rt_map_get` on a missing key is a documented failure (`E-R11`, exit 111), not a sentinel |
| Filesystem (T9) | Contents round-trip exactly; a missing file is observable via `fs_exists` → `false` or `fs_file_size` → `-1`; `fs_read` returns an empty string for missing files |
| CLI (T17) | All paths behave as described; bad usage writes guidance to stderr and exits non-zero |
| Documentation (T18) | Honest subjective answers, including criticism |

Documented limitations that are **not** defects (see `README.md` §9): Windows-only,
1 GiB arena with a bounded live-allocation ceiling, ANSI paths only (no Unicode
paths, no `MAX_PATH`-exceeding paths), no Ctrl+C handler, no thread pool or
condition variables, SQLite is an in-memory engine with an opaque `Str` blob
(no file persistence), stdin is read-all rather than line-streamed, and the npm
package is 1.0.4 rather than the 1.0.5 under test.

## 6. How to report failures

For every problem — including confusing documentation — record:

1. `mink --version` output and the binary checksum you measured.
2. Windows version and architecture.
3. The smallest source file that reproduces it.
4. The exact command, stdout, stderr and exit code, verbatim.
5. What you expected, and the document and section you relied on.
6. Whether it reproduces on a second run and in a fresh directory.

Classification (compiler / runtime / standard-library / documentation /
usability / missing capability / intentional difference / environment) and
severity (S1 blocks installation or any program; S2 wrong result, crash or
documented behaviour that does not hold; S3 real time lost with a workaround;
S4 cosmetic) are helpful but optional — describe what happened and the
coordinator will triage it. Use the bug template in
`docs/language/DEBUGGING_AND_TESTING.md` §8 if you prefer.

**Do not silently work around a failure.** A failure you recorded is more
valuable than a workaround you did not mention.

## 7. Completing and returning the form

1. Copy `SUBMISSION_TEMPLATE.md` and fill it in as-is.
2. §1: environment, install path (A or B), version string, size and SHA-256,
   dates, and your consent to record the submission verbatim.
3. §2: one row per task T1–T18 — `attempted`, time spent, outcome
   (`completed` / `completed with friction` / `partially completed` / `blocked`
   / `not attempted`), expected vs actual, exit codes, documents used, finding
   ids. A "not attempted" row is honest data, never a pass.
4. §3: one block per finding you reproduced, with the details from §6.
5. §4: list every attachment so it can be matched to task ids.
6. §5: your T18 answers, in your own words.
7. §6 only if you stopped early: last completed task, exact blocker, and what
   you would need to continue.
8. Send the completed form and attachments to the coordinator who gave you the
   package. Do not open public issues and do not include credentials, personal
   paths or unrelated files.

Nothing you submit is published. Your name and contact details are optional;
only the information you agree to share is recorded, and criticism is preserved
word for word.
