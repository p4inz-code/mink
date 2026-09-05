# MINK Environment Report (Windows example)

A small, self-contained MINK application that exercises the Session 99
"Wave A tranche 1" capabilities: command-line arguments, environment
variables, stdin, `str_format`, float-to-string conversion, stderr
writes, sleep, and filesystem output.

It is the official Session 99 proof application: the same source builds
through the public `mink` CLI from a clean npm installation and produces
one standalone Windows executable.

## What it demonstrates

- Command-line arguments: `rt_argc` / `rt_argv` (order, spaces, empty
  arguments preserved)
- Environment: `rt_env_get` (owned result), `rt_env_set` /
  `rt_env_remove` round trip, missing-variable handling
- stdin: `rt_stdin_read` reads all piped input to EOF and returns an
  owned string (empty when nothing is piped)
- Formatting: `rt_str_format` with exactly three `{}` substitutions
  (missing arguments are dropped, `{{` / `}}` escape braces)
- Float conversion: `rt_str_from_float` (same exact dtoa path as
  `rt_print_float`)
- stderr: `rt_stderr_write` writes exact bytes to a separate stream
- Sleep: `rt_sleep` with a millisecond duration
- Filesystem: `rt_fs_write` writes a small text report
- Ownership: every returned string is freed exactly once; the program
  exits 0 through the leak checker

## Requirements

- MINK >= 1.0.1 (Windows x86_64) with the Session 99 runtime services
- Installed via `npm install -g mink` (the package bundles the standard
  library, so `mod math;` etc. resolve automatically)

## Build

```text
mink check main.mink
mink build main.mink
```

This produces one standalone executable: `main.exe`.

## Run

stdin must be piped (the program reads stdin to EOF):

```text
echo "piped input" | main.exe firstarg "two words"
```

Expected stdout:

```text
2
firstarg
two words
env_report: path_len=<PATH length> value_len=16 stdin_len=12
2.5
3.1415926535897931
0
0
<N bytes written>
```

stderr carries the diagnostics line; `env_report.txt` is written to the
current directory containing the argv report.

Exit code is 0.

## Standard-library modules

The npm package ships `stdlib/*.mink` next to the compiler executable.
`mod` declarations resolve in this order: the source file's directory,
the bundled `stdlib` next to the installed `mink` executable, then a
`stdlib` directory in the current working directory. No include-path
configuration is required for normal use.

## Produce a standalone .exe

```text
mink build main.mink
```

`main.exe` is a single self-contained executable. Copy it to any
directory — even without MINK, Rust, Node.js, or npm — and run it there.

## Ownership note

Every `rt_str_*` function that returns a `Str` returns a new owned
string which the caller must release with `rt_str_free`. The example
follows this rule for every returned string, including the argv strings
replaced by `rt_argv`, so it exits cleanly through the leak checker.