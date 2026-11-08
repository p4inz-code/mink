# M6 Evaluator FAQ and Troubleshooting

Every item below was verified against the repository build or is quoted from an
existing active document. If your experience differs, report it; do not assume
the FAQ is complete.

## Installation and paths

**`./mink.exe: No such file or directory`**
Run the command from the folder that contains `mink.exe`, and include `./` in
Git Bash. For Path B the binary is `./target/release/mink.exe` from the
repository root; the `../mink.exe` in the README quick start is the Path A
layout.

**`E-S01: module file 'filesystem.mink' not found`**
The repository build looks for `<folder containing mink.exe>\stdlib` first and
then `<current directory>\stdlib`. Run from the repository root, copy `stdlib/`
beside the compiler, or use the installed package layout. A local file named
`filesystem.mink` shadows the bundled module.

**A self-built binary has a different SHA-256 from §4**
That is expected. A normal `cargo build --release` can print the same version
and size with a different checksum. Path A's supplied binary must match the
reference checksum; a Path B self-build does not. Record the checksum you
measured either way.

## Output and basic language use

**Two prints appear on separate lines**
Every `rt_print_*` call writes one complete line with CRLF. Build the whole line
as one `Str` and print it once; do not concatenate two print calls expecting one
line.

**`rt_print_bool` is not found**
It is not an intrinsic. Print `1`/`0` with an `if`, or convert with
`rt_str_from_bool` and print the string. `rt_print_int` writes decimals;
`rt_print_str` writes strings.

## Command-line input

**How do I read arguments and stdin?**
`rt_argc()` counts the arguments after the executable; `rt_argv(0)` is the first
one. `rt_argv` returns an owned `Str` (free it), and an out-of-range index is
the runtime failure `E-R10`. `rt_stdin_read()` reads all input to EOF and
returns an owned `Str`; it is read-all, not line-streamed. `rt_stderr_write(s)`
writes to stderr. See
[`LANGUAGE_BASICS.md`](../../language/LANGUAGE_BASICS.md) §1.1 for a worked
example.

## Collections

**A map lookup exits 111 with `E-R11: map key not found`**
`rt_map_get` on an absent key is a documented runtime failure, not a sentinel.
Use `rt_map_has` first when absence is possible. The reference §5.2 states this.

**A program exits 106 with `E-R06: memory leak`**
An owned string, collection handle, directory handle or arena was not released.
Free `rt_str_concat`/`rt_str_from_*`/`fs_read` results, `rt_map_free`/
`rt_vec_free`/`rt_set_free`, and arenas with `rt_free`. Freeing a literal is a
no-op.

## Files, JSON, CSV and SQLite

**`fs_read` of a missing file returns an empty string**
That is the documented convention; it cannot distinguish missing from empty by
itself. Use `fs_exists` (`false`) or `fs_file_size` (`-1`).

**Can I append to a file?**
There is no append mode. `fs_write` replaces the contents; read the existing
text, concatenate the addition, and write the combined string back.

**`json_parse` returns 0 for malformed input**
Zero is the documented failure result. There is no line/column reporting in the
API. Have the program report the failure and choose its own exit code.

**A CSV field with an embedded newline breaks after splitting lines**
`csv_parse_row` is record-oriented. Pass the whole record text; it preserves a
newline inside quotes. Splitting the document on physical lines first is a
caller error, not a parser defect.

**SQLite data is gone in a second process**
The engine is in-memory: the database is an opaque `Str` blob threaded through
each call. It is not a file-backed SQLite binding. Do not expect persistence
across process runs; T12 tests the documented in-memory workflow.

## Diagnostics and tests

**`mink explain E-R06` says unknown error code**
`mink explain` covers the documented language/compiler/package codes (50 in this
build). Runtime codes such as `E-R04`, `E-R06` and `E-R11` are documented in
`DEBUGGING_AND_TESTING.md`, not in `mink explain`.

**`mink test` exits 1 and says no test functions were found**
That is expected for a file without top-level `fn test_*` functions. Add a test
function or use `mink run` for an ordinary program.

**`mink check --json` exits 1 even though it printed JSON**
The JSON is still valid; exit 1 means the checked program has errors. Read
`success` and the `errors` array.

## Networking

**A connection to a closed local port fails without a process error**
`net_connect` returns `-1` and `net_last_error` reports the Winsock reason (for
example 10061, connection refused). The runtime does not exit for you; treat the
failure explicitly if it is fatal for your program.

## Scope

Windows x86_64 only. Linux is frozen and unsupported; macOS is absent. Unicode
paths, paths beyond `MAX_PATH`, Ctrl+C handling, thread pools and condition
variables are not supported. See the package README §9 for the accepted
limitations.
