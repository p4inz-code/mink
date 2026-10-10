# F — jobrun (process automation)

PROJECT: F process automation / time
PURPOSE: Drive child processes, stamp timestamps and manage environment
variables — the "automation script" task a Python developer reaches for with
`subprocess`, `time.strftime`, `os.environ` and `sys.argv`.
MINK SOURCE: `validation/f_process/main.mink` (82 functions emitted)
PYTHON COMPARISON: `validation/f_process/python_ref.py` (`subprocess`, `time`,
`os.environ`)
CROSS-CHECK HARNESS: `validation/f_process/verify.py`

BUILD: PASS — `mink build validation/f_process/main.mink` → `main.exe`
(x86_64-windows-pe, no external toolchain, no runtime DLL).

EXECUTION: PASS — 19/19 cases produce byte-identical stdout, stderr and exit
codes compared with the Python reference (`python verify.py` → `19/19 cases
match the Python reference`).

FUNCTIONAL CORRECTNESS: PASS — `argv` prints the whole user argument vector
(including a quoted argument with a space and a non-ASCII argument), `run`
launches a child through the stdlib `process` module and reports its exit
code, stdout length/content and stderr length, `runin` pipes an input string
to the child's stdin (`cmd /c sort` sorts it), `stamp` formats UTC timestamps
through `time_strftime` (epoch 0, 1e9, 1.7e9), and `env` reads, sets, reads
back and removes a variable through the `rt_env_*` intrinsics.

ERROR HANDLING: PASS — exit 1 for usage errors (no arguments, unknown
command, any option, missing/extra arguments, non-numeric epoch); messages go
to stderr (`rt_stderr_write`), stdout stays clean. A child's non-zero exit
code (3) is reported as data, not as the tool's own failure.

MEMORY: PASS — the runtime leak check (`E-R06`, exit 106) is enabled; all 19
cases exit 0/1, never 106. Two leak sites were found and fixed during
validation (see CLASSIFICATION).

REALISTIC INPUT: PASS — a real `cmd.exe` child, a pipeline (`echo one|findstr
one`), a stdin-driven sort, three timestamps, a Unicode argument, and a
set/read/remove environment round trip.

REPEATED EXECUTION: PASS — repeated runs produce identical bytes; `verify.py`
has been run repeatedly with identical results (19/19 each time).

STANDALONE EXE: PASS — `main.exe` runs with no interpreter and no runtime;
the only dependency is the Windows API.

WINDOWS: PASS.

MINK LIMITATION: see CLASSIFICATION; none of the findings blocked the
application.

CLASSIFICATION:
- C (architectural limitation, real parity gap) — `rt_argv` delivers the
  **ANSI** command line: a non-ASCII argument arrives as system-codepage
  bytes (CP1252 `0xE9` for `é`), while Python's `sys.argv` round-trips
  Unicode through the wide command line. The reference mirrors the ANSI
  conversion (`str.encode("mbcs")`) so the transcripts stay byte-identical;
  an application that must accept Unicode arguments has no UTF-8 argv path.
- C (architectural limitation) — stdout/stderr capture is a fixed
  runtime-owned window: each `process_run` overwrites it, and exceeding the
  window fails the program with `E-R02` instead of truncating silently. There
  is no streaming or interactive child I/O.
- C (architectural limitation) — stdin input is capped at one pipe buffer
  (4096 bytes) by design (`process_run_input` documents the deadlock reason);
  longer input is truncated there.
- C (architectural limitation) — the process module is Windows-only
  (`CreateProcessA`); the header documents the future POSIX plan. There is no
  signal/terminate/pid-of-child API, and a failed launch is reported as an
  exit code rather than distinguishing "could not start" from "exited 1".
- C (ergonomic trap, documented) — a nested `rt_str_concat` leaks the inner
  temporary when that temporary is itself an allocation (the intrinsic is
  read-borrowing): `rt_str_concat("arg[", rt_str_from_int(i))` leaked one
  string per argument.
- C (ergonomic trap, documented, new here) — the `rt_env_*` intrinsics do
  **not** consume their arguments, unlike every stdlib wrapper: copies of the
  name/value passed to `rt_env_set`/`rt_env_remove` must be bound and freed by
  hand.
- D (deliberate difference) — `rt_print_str` appends CRLF without translating
  embedded newlines, so a child's captured stdout (which already ends in
  CRLF) is followed by an extra CRLF; the Python reference writes raw bytes +
  `\r\n` to match.
- D (deliberate difference) — `rt_argc`/`rt_argv` exclude the program name;
  the reference mirrors `sys.argv[1:]`, and a leading `-` marks a usage error
  because the tool defines no options.
- D (deliberate difference) — `time_strftime` is UTC (no local-time variant);
  the reference uses `time.gmtime`. The directives exercised (`%Y %m %d %H %M
  %S %Z`-free formats) match Python's `strftime`.
- D (convention) — `process_run` consumes the command string and returns the
  child's exit code; captured stdout/stderr are runtime windows that must be
  printed but **never** freed (`stdlib/process.mink` documents this), unlike
  every heap `Str`.
- D (semantics verified) — environment names are case-insensitive on Windows;
  both implementations use the same casing in the workload.
- A (application defect, found & fixed during validation) — two leak sites,
  both caught by the leak checker (`E-R06`, exit 106):
  1. `cmd_argv` built the argument label with
     `rt_str_concat("arg[", rt_str_from_int(i))`, leaking the converted index
     on every argument; fixed by binding the Int→Str conversion first;
  2. `cmd_env` passed `rt_str_concat(name, "")` / `rt_str_concat("probe-value",
     "")` straight into `rt_env_set`/`rt_env_remove`, which do not consume
     them; fixed by binding and releasing each copy.
  `verify.py` then reached 19/19. No MINK compiler/runtime defect was found in
  F — the process runtime, the capture windows and the time/environment
  modules behaved as documented.

EVIDENCE:
- TEST — `validation/f_process/verify.py`, 19 cases, 0 differences (stdout,
  stderr, exit code).
- OUTPUT — see the transcript below.
- REGRESSION — the leak fixes are application-level; the runtime leak checker
  (`E-R06`) is the regression net and every case passes it. The process
  runtime is covered by `cargo test --test process_lib` (86 passed) and
  `time_lib` (29 passed); the full suite (73 integration targets + `--lib`,
  3040 tests) was re-run green before starting F.

## Transcript

```
$ mink build validation/f_process/main.mink
mink: build: 'validation/f_process/main.mink' -> '.../main.exe'
(target: x86_64-windows-pe, 82 function(s), 0 binding(s))

$ python validation/f_process/verify.py
ok   argv_basic (exit 0)
ok   argv_empty (exit 0)
ok   argv_unicode (exit 0)
ok   run_echo (exit 0)
ok   run_exit3 (exit 0)
ok   run_pipe (exit 0)
ok   runin_sort (exit 0)
ok   runin_empty (exit 0)
ok   stamp_epoch0 (exit 0)
ok   stamp_iso (exit 0)
ok   stamp_2023 (exit 0)
ok   env_probe (exit 0)
ok   no_args (exit 1)
ok   unknown_command (exit 1)
ok   option_looking_arg (exit 1)
ok   run_missing_args (exit 1)
ok   run_extra_args (exit 1)
ok   stamp_bad_epoch (exit 1)
ok   env_extra_args (exit 1)

19/19 cases match the Python reference

$ main.exe argv one "two words" 3
argc=4
arg[0]=argv
arg[1]=one
arg[2]=two words
arg[3]=3
exit=0

$ main.exe run "cmd /c echo hello"
exit=0
stdout-len=7
stdout=hello

stderr-len=0
stderr=
exit=0

$ main.exe run "cmd /c exit 3"
exit=3
stdout-len=0
stdout=
stderr-len=0
stderr=
exit=0

$ main.exe runin "cmd /c sort" $'b\na\n'
exit=0
stdout-len=4
stdout=a
b

stderr-len=0
stderr=
exit=0

$ main.exe stamp 1000000000 "%Y-%m-%dT%H:%M:%SZ"
time=2001-09-09T01:46:40Z
exit=0

$ main.exe env MINK_F_VALIDATION_PROBE
value=(unset)
has=false
after-set=probe-value
after-remove-has=false
exit=0
```

## Python comparison notes (concrete differences, no ranking)

1. Python's `subprocess.run(cmd, shell=False, capture_output=True)` maps to
   `process_run` + `process_stdout`/`process_stdout_len` +
   `process_stderr`/`process_stderr_len`; the MINK runtime passes the command
   line to `CreateProcessA` directly, so the reference does not use
   `shell=True` either.
2. Python's `input=`/`stdin=PIPE` maps to `process_run_input`, with the
   documented one-pipe-buffer (4096-byte) limit.
3. Python's `time.strftime(fmt, time.gmtime(ts))` maps to
   `time_strftime(ts, fmt)`; both are UTC and agree on the exercised
   directives.
4. Python's `os.environ` maps to `rt_env_get`/`rt_env_set`/`rt_env_has`/
   `rt_env_remove`; there is no wrapper module, so the app calls the
   intrinsics and manages the copies itself. An empty value is printed as
   `(unset)`, mirroring the `rt_str_len == 0` test.
5. Python's `sys.argv[1:]` maps to `rt_argc`/`rt_argv` — except that MINK's
   argv is ANSI (see CLASSIFICATION), which the reference mirrors with
   `mbcs`.
6. Python's `subprocess.CompletedProcess.returncode` is data (printed as
   `exit=`), not the tool's own exit status; the tool exits 0 whenever the
   subprocess step itself ran, and 1 only for its own usage errors.
