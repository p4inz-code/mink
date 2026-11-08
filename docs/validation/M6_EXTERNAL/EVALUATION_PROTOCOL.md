# MINK External Evaluation Protocol

**Version under test:** MINK 1.0.5, Windows x86_64
**Prepared:** 2026-11-08 (Session 215)
**For:** an independent developer who did not build MINK

## 1. Purpose and rules

The purpose is to find out whether a developer who has never seen MINK can
install it, learn it from its documentation, and build practical software with
it — and to capture where that fails.

Rules for the coordinator:

- Give the tester the package and this protocol, and nothing else. Do not
  explain the language, do not demonstrate tasks, do not point at standard
  library functions.
- Answer only questions about **access** (where the binary is, how to send
  feedback). Defer language and API questions to the documentation.
- Do not edit the tester's code, do not fix their programs, do not watch over
  their shoulder.
- Do not ask leading questions ("wasn't the error message good?"). Ask what
  happened and what they expected.
- Record feedback verbatim, including criticism, in
  [`VALIDATION_RECORD.md`](VALIDATION_RECORD.md).
- A prepared or delivered package is not validation. Only completed, recorded
  tester work counts.

Rules for the tester:

- Work in your own directory. Do not read `docs/implementation/`,
  `docs/benchmarks/` or `docs/audits/`; they describe MINK's construction and
  would spoil the documentation and usability assessment.
- Use only the package documentation, the compiler's own diagnostics and
  `mink explain`.
- Time-box each task as stated. If a task is not finished in its budget, stop,
  record where you got stuck and move on. Being stuck is data, not failure.
- Before running anything, write down what you expect. Then run it.
- Keep a transcript (terminal output or a notes file). Do not clean it up.

Suggested total effort: **4–6 hours across two or three sittings**. The 18
stated time boxes sum to about 7.5 hours; the required minimum set (T1, T2, T3,
T6, T8, T9, T17, T18) sums to about 3 hours, so 4–6 hours covers the required
tasks plus several optional ones. It is not necessary to finish every task;
partial coverage with honest notes is more useful than a rushed pass.

## 2. What gets recorded per task

| Field | Value |
| --- | --- |
| Task id | T1–T18 |
| Started / finished | wall-clock times, or "abandoned at 20 min" |
| Outcome | completed / completed with friction / partially completed / blocked |
| Expected vs actual | one or two lines each |
| Exit codes | compiler and program |
| Errors seen | verbatim message and code |
| Documentation used | file and section |
| Severity | S1–S4 if a problem is reported |
| Notes | free text, in your own words |

## 3. Tasks

Each task states an outcome, not an implementation. Any correct program that
produces the stated behaviour passes.

### T1 — Installation and version (10 min)

Install MINK by the path in the package README and run `mink --version`,
`mink help` and `mink explain` (no argument).

**Expected:** `mink 1.0.5`; a command list including `build`, `run`, `check`,
`test`, `repl`; a non-empty list of documented error codes. Record the checksum
you measured.

### T2 — First program (10 min)

Compile and run a program that prints `42` and exits 0. Then copy the `.exe`
to a different directory and run it there.

**Expected:** prints `42`; exit 0; runs outside the source directory with no
runtime to install.

### T3 — Diagnostics and error codes (20 min)

Write three deliberately broken programs: (a) missing semicolon, (b) a call to
an undefined function, (c) an obvious type mismatch. Run `mink check` on each,
then `mink explain` on the code each one reports.

**Expected:** each is rejected with a non-zero exit; each message names a file
and line and carries an error code; `mink explain <code>` prints an explanation
that matches the situation. Note any message that does not say what to change.

### T4 — Type checking (20 min)

Write one program that must be accepted and one that must be rejected: store an
`Int` in a variable annotated `Str`, and pass a `Str` where the function
signature requires an `Int`.

**Expected:** the invalid program is rejected before producing an executable;
the valid one builds and runs. Record whether the rejection points at the
argument, the parameter or the whole call.

### T5 — Ownership and moves (25 min)

Write a program that moves a heap string into a function, tries to use the
original afterwards, and observe the result. Repeat with `&` and `&mut`
borrows, and with two simultaneous `&mut` borrows.

**Expected:** use-after-move and aliasing violations are compile-time errors,
not crashes; the corrected version compiles and, on exit, reports no leak.
Record how understandable each ownership message is.

### T6 — Structs, enums and pattern matching (25 min)

Define a struct with at least two fields, and an enum with at least two
data-carrying variants plus one unit variant. Write a function that consumes
the enum with `match` and returns a value derived from it.

**Expected:** builds and runs; a non-exhaustive `match` is rejected. Record
whether the exhaustiveness message names the missing variant.

### T7 — Functions, generics and modules (30 min)

Write a generic function used with two different concrete types, plus one call
that misuses it. Put the function in a second file and import it with `mod`.

**Expected:** both valid instantiations work; the misuse is rejected; the
multi-file build works from the entry file. Record what you had to discover
about file naming and module lookup.

### T8 — Collections (25 min)

Build a list, a map and a set; insert, look up, update, iterate and remove.
Print a deterministic summary (sorted keys, counts).

**Expected:** output is stable across two runs. Test absence with `rt_map_has`;
`rt_map_get` on a missing key is a documented runtime failure (`E-R11`, exit
111), not a sentinel value. If you exercise it, record the documented failure
rather than treating it as a crash. Record whether the reference made this
distinction obvious.

### T9 — Filesystem (25 min)

Create a directory, write a text file, read it back, overwrite it with
additional content (append by read-modify-write; there is no append mode), list
the directory, and delete the file. Then read a file that does not exist.

**Expected:** contents round-trip exactly and the overwrite contains the
intended combined text; a missing file is observable through `fs_exists`
(`false`) or `fs_file_size` (`-1`). `fs_read` returns an empty string for a
missing file, so it cannot distinguish missing from empty by itself; the
program exits non-zero only if you choose to make the missing file fatal.
Record whether deletion succeeded after the reads (no lock remained).

### T10 — JSON (25 min)

Serialize a structure with a string, an integer, a boolean, a nested object and
an array; write it to a file; read it back; extract two fields.

**Expected:** the round-trip preserves values and types. `json_parse` returns
`0` for malformed input; the API has no line/column reporting, so have your
program print the failure and exit non-zero if it treats it as fatal. Record
that absence of position information rather than expecting it.

### T11 — CSV (25 min)

Parse a CSV record containing a quoted field with a comma and a quoted field
with a newline; compute one aggregate (count or sum) over a numeric column. Pass
the whole record text to `csv_parse_row`; the module is record-oriented and does
not split a document into records for you.

**Expected:** the quoted fields are parsed as single values; the aggregate
matches a hand calculation. The embedded newline is preserved only when the
whole record is passed; splitting on physical lines first will fail and is a
documented caller constraint. Record any ambiguity in the documented API.

### T12 — SQLite (30 min)

Use `mod sqlite;` to create an in-memory database: create a table, insert
several rows, query with a filter, update one row, and report the final rows.
The "database" is an opaque `Str` blob threaded through every call, so pass
each call's returned blob to the next call. The engine is in-memory only; it is
not a file-backed SQLite binding and does not persist across process runs.

**Expected:** the queries reflect the mutations; a failing statement is
reported through the result text rather than silently continuing; the program
reports the documented in-memory limitation and does not claim file
persistence. Record whether the blob-threading ownership rule and the in-memory
scope were clear.

### T13 — HTTP and HTTPS (25 min)

Fetch a URL and print status and body length; then request a URL that returns
404. If the environment has no internet access, use a local server you start
yourself and say so.

**Expected:** status and body are reported accurately; 404 is observable without
crashing. Record whether TLS trust is explained anywhere you could find.

### T14 — TCP (30 min)

Start a TCP listener in one process and connect from a second process on
localhost; send a line each way and print what was received. Then connect to a
closed port.

**Expected:** both messages arrive intact; a closed port makes `net_connect`
return `-1` and leaves the reason in `net_last_error` (for example 10061,
connection refused). The program exits non-zero if you treat that as fatal; the
runtime does not exit for you. Record whether the accept/connect lifecycle was
clear.

### T15 — Concurrency (30 min)

Start several threads that each increment a shared counter under a lock, join
them, and print the total. Then deliberately run the same increments without
the lock and describe what you observed.

**Expected:** with the lock the total is exact on repeated runs; without it the
program is either rejected or produces a different total, and you can explain
which. Record whether the documentation states the memory model.

### T16 — Multi-file project (30 min)

Split a small program across three files: entry point, a utility module, and a
data-type module used by both. Build only the entry file.

**Expected:** the build resolves both modules; a deliberate typo in an imported
name is reported with the correct file and line. Record how long it took to
work out the module layout.

### T17 — CLI application (40 min)

Build a small command-line tool with at least two subcommands, `--help`, a
required positional argument, one flag, stdin input, and meaningful exit codes
(0 success, non-zero on bad usage and on processing failure). Test it through a
pipe.

**Expected:** all paths behave as described; invalid usage prints guidance to
stderr and exits non-zero. This is the capstone task: it should feel like
ordinary native-tool development. The reference should make argument parsing,
stderr and the read-all stdin model discoverable; record any remaining gap.

### T18 — Documentation and developer experience (30 min)

After the tasks, answer in your own words:

1. Time from starting the install to a running `hello.exe`.
2. Which document you actually used most, and which sections misled you.
3. The three most confusing things about the language.
4. The three best-supported things.
5. Whether you would choose MINK for a real Windows tool, and for what kind.
6. Overall rating 1–5 for installation, documentation, diagnostics, standard
   library, and developer experience.
7. The single change that would have saved you the most time.

## 4. Coverage map (protocol → M6 criteria)

| Criterion | Tasks |
| --- | --- |
| Independent installation success | T1, T2 |
| Documentation comprehension | T1–T18, especially T18 |
| Successful compilation of representative programs | T2, T6–T17 |
| Practical standard-library usability | T8–T15 |
| Compiler diagnostic quality | T3, T4, T5, T16 |
| Application development experience | T17, T18 |
| Reproducible defects | any task, reported per §5 |
| Remaining limitations | T18 and the package's known-limitations list |

## 5. Reporting and retest

For every problem: smallest reproducer, command, stdout, stderr, exit code,
expected behaviour, document relied on, classification (compiler / runtime /
standard-library / documentation / usability / missing capability / intentional
difference / environment / invalid) and severity S1–S4.

After a fix is shipped, the tester is asked to retry only the affected task and
to state whether the original problem is gone. Internal verification never
substitutes for that retest.

## 6. Acceptance thresholds for M6

M6 can be marked COMPLETE only when all of the following are true:

1. At least one independent developer completed T1, T2, T3, T6, T8, T9, T17 and
   T18 — installation, first program, diagnostics, data types, collections,
   filesystem, a CLI application, and the experience survey.
2. No open S1 defect, and no open S2 defect in a task that was attempted.
3. Any S2 defect that was found has been fixed and independently retested.
4. Documentation defects that blocked a task are corrected.
5. The tester's verbatim feedback is recorded and their consent covers its use.

M6 is PARTIAL when validation occurred but an S2 defect remains open or a
required task was not completed. M6 stays OUTSTANDING when no independent tester
has completed the protocol.

Internal tests, agent runs and repository audit results never count toward any
condition above. A prepared package is not validation.
