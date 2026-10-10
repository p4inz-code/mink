# A3 — textgrep (text-processing, grep-like tool)

PROJECT: A3 text processing
PURPOSE: Select lines of a file with a regular expression — the "find the
lines that matter" task a Python developer reaches for with `re.search` +
`open()`: regex matching, `-v/--invert`, `-c/--count`, `-n/--line-number`,
`-i/--ignore-case`, `-m/--max-count`, and writing the report to a file with
`-o/--output`.
MINK SOURCE: `validation/a3_textgrep/main.mink` (93 functions emitted)
PYTHON COMPARISON: `validation/a3_textgrep/python_ref.py`
CROSS-CHECK HARNESS: `validation/a3_textgrep/verify.py`
FIXTURES: `validation/a3_textgrep/fixtures/make_fixtures.py` (7 files, 535
bytes: log with a blank line, word-anchored plain text, CRLF, no trailing LF,
empty, only-newlines, UTF-8 multibyte)

BUILD: PASS — `mink build validation/a3_textgrep/main.mink` → `main.exe`
(x86_64-windows-pe, no external toolchain, no runtime DLL).

EXECUTION: PASS — 35/35 cases produce byte-identical stdout, stderr, exit
codes **and** byte-identical `--output` files compared with the Python
reference (`python verify.py` → `35/35 cases match the Python reference`).

FUNCTIONAL CORRECTNESS: PASS — byte-oriented regex search via the stdlib `re`
engine (`re_is_match`, the MINK-native Python-`re` subset), `^`/`$` anchors
evaluated per line, character classes and `+`/`*` quantifiers, invert, count,
line numbers, `--max-count` stop, `--ignore-case` (ASCII folding), LF line
splitting with the trailing-LF rule, CRLF input preserved, UTF-8 bytes passed
through, empty pattern matching every line.

ERROR HANDLING: PASS — distinct exit codes verified against Python: 1 usage
(no positionals, one positional, too many positionals, missing/invalid/zero
`-m` value), 2 input missing / not a regular file, 3 nothing selected, 4
output file not writable (implemented, not exercised — the work dir is
writable). Messages go to stderr (`rt_stderr_write`), stdout stays clean.

MEMORY: PASS — the runtime leak check (`E-R06`, exit 106) is enabled; all 35
cases exit 0/1/2/3, never 106, including every failing path and every output
mode. Two leaks were found and fixed during validation (see CLASSIFICATION).

REALISTIC INPUT: PASS — log/plain/CRLF/no-trailing-newline/empty/
only-newlines/UTF-8 fixtures; empty pattern selects all 8 `app.log` lines;
`^INFO` selects 3 of 6 `plain.txt` lines; `done$` anchors on the last line.

REPEATED EXECUTION: PASS — the same command run twice produces identical
bytes (`cmp` on two redirected runs); `verify.py` has been run repeatedly
with identical results (35/35 each time).

STANDALONE EXE: PASS — `main.exe` runs with no interpreter and no runtime;
the only dependency is the Windows API.

WINDOWS: PASS.

MINK LIMITATION: see CLASSIFICATION; none of the findings blocked the
application.

CLASSIFICATION:
- D (deliberate difference) — `rt_print_str` appends CRLF without translating
  embedded newlines; the Python reference writes raw bytes + `\r\n` to match.
  The `--output` file deliberately uses LF only, so the file artifact is
  newline-transparent for other tools; Python mirrors that byte-for-byte.
- D (deliberate difference) — `rt_argc`/`rt_argv` exclude the program name;
  argv[0] is the first user argument in both implementations.
- D (deliberate difference, now improved) — the ownership checker is
  flow-sensitive but **not path-sensitive**; application code funnels to a
  single exit and releases each owned string exactly once.
- D (deliberate difference) — `-i/--ignore-case` is defined as lower-casing
  the pattern and the subject with `str_to_lower` / `bytes.lower` (ASCII)
  instead of Python `re.IGNORECASE`. Consequence, documented in the source:
  uppercase escapes (`\D`, `\W`, `\S`) fold to their lowercase cousins, so
  `-i` is only meaningful for literal/class patterns.
- C (architectural limitation) — the `re` engine has no `(?i)`/`(?s)` inline
  flags, no capture-group extraction, no backreferences, no Unicode classes;
  it is byte-oriented and bounded by a step budget. `re_is_match` is the
  Python `re.search` equivalent; `re_full_match`/`re_match_end` cover the
  anchored forms.
- C (architectural limitation) — no module-level `Vec` globals; the selected
  lines and the split line vector are function locals with untyped `out`
  threading not needed here (single pass).
- C (ergonomic trap, documented) — the read-borrowing intrinsics do not free
  their arguments (`rt_print_str`, `rt_str_concat`, `rt_argv`, and the
  `rt_fs_*` read calls), so inline temporaries leak and the run ends with
  `E-R06` (exit 106). Each temporary must be bound and released (or handed to
  a stdlib wrapper that consumes it).
- A (application defect, found & fixed during validation) — three leaks in
  the first version, all caught by the leak checker (`E-R06`, exit 106):
  1. `rt_print_str(rt_str_concat(...))` — the printed copy was never freed;
  2. `render(..., numbered = 1)` did `rt_str_concat(head, line)` but only
     freed `head`, leaking the original `line` (rt_str_concat copies, it does
     not consume);
  3. `fs_write(rt_str_concat(rt_argv(out_index), ""), ...)` leaked the
     `rt_argv` result, and the same inline call was repeated in the failure
     message path.
  Fixed by binding/releasing every temporary; `verify.py` then went from
  13/35 to 35/35. No MINK defect was found in A3 — the engine, the `re`
  subset and the ownership analysis behaved as documented.
- D (convention) — `fs_write` returns the number of bytes written and `-1`
  on failure (it is not one of the `0=ok, 1=error` fs mutators), so the
  output path checks `wrote < 0`.
- D (convention) — `str_parse_int` consumes its argument; the parsed
  `(value, ok)` is validated before use (`--max-count` must be positive).

EVIDENCE:
- TEST — `validation/a3_textgrep/verify.py`, 35 cases, 0 differences
  (stdout, stderr, exit code, `--output` file tree).
- FIXTURE — `validation/a3_textgrep/fixtures/make_fixtures.py` (7 files).
- OUTPUT — see the transcript below (`-o` file checked with `od -c`).
- REGRESSION — the leak fixes are application-level; the runtime leak checker
  (`E-R06`) is the regression net and every case passes it. The full test
  suite (73 integration targets + `--lib`) was re-run green before starting
  A3 (see SESSION_127 notes).

## Transcript

```
$ python validation/a3_textgrep/fixtures/make_fixtures.py
fixtures: 7 files, 535 bytes under .../validation/a3_textgrep/fixtures

$ mink build validation/a3_textgrep/main.mink
mink: build: 'validation/a3_textgrep/main.mink' -> '.../main.exe'
(target: x86_64-windows-pe, 93 function(s), 0 binding(s))

$ python validation/a3_textgrep/verify.py
ok   basic_error (exit 0)
ok   match_anywhere (exit 0)
ok   anchored_caret (exit 0)
ok   anchored_dollar (exit 0)
ok   comment_anchor (exit 0)
ok   numbered (exit 0)
ok   count (exit 0)
ok   count_long (exit 0)
ok   invert (exit 0)
ok   ignore_case (exit 0)
ok   ignore_case_class (exit 0)
ok   max_count_2 (exit 0)
ok   max_count_long (exit 0)
ok   regex_class_digits (exit 0)
ok   regex_dotstar (exit 0)
ok   empty_pattern (exit 0)
ok   no_match (exit 3)
ok   missing_file (exit 2)
ok   dir_as_file (exit 2)
ok   no_args (exit 1)
ok   one_positional (exit 1)
ok   too_many_positionals (exit 1)
ok   max_missing_value (exit 1)
ok   max_invalid (exit 1)
ok   max_zero (exit 1)
ok   output_file (exit 0)
ok   output_count (exit 0)
ok   output_no_match (exit 3)
ok   no_trailing_newline (exit 0)
ok   crlf_error (exit 0)
ok   empty_file (exit 3)
ok   only_newlines_empty_pattern (exit 0)
ok   only_newlines_anchored (exit 3)
ok   unicode_lines (exit 0)
ok   unicode_error (exit 0)

35/35 cases match the Python reference

$ main.exe ERROR fixtures/app.log
2026-10-01T00:00:04Z ERROR failed to fetch rows code=42
2026-10-01T00:00:05Z ERROR failed again code=43
exit=0

$ main.exe -n -i error fixtures/app.log
5:2026-10-01T00:00:04Z ERROR failed to fetch rows code=42
7:2026-10-01T00:00:05Z ERROR failed again code=43
exit=0

$ main.exe -c INFO fixtures/plain.txt
3
exit=0

$ main.exe zzz fixtures/app.log
exit=3

$ main.exe -n "^INFO" fixtures/plain.txt -o work/demo/out.txt
exit=0

$ od -c work/demo/out.txt
0000000   1   :   I   N   F   O       s   t   a   r   t   i   n   g  \n
0000020   2   :   I   N   F   O       s   t   i   l   l       g   o   i
0000040   n   g  \n   6   :   I   N   F   O       d   o   n   e  \n

$ main.exe INFO fixtures/plain.txt > r1.txt ; main.exe INFO fixtures/plain.txt > r2.txt
$ cmp r1.txt r2.txt && echo identical
identical
```

## Python comparison notes (concrete differences, no ranking)

1. Python's `re.search(pattern, line)` maps to `re_is_match(pattern, line)`
   (the stdlib passes the subject back so the caller keeps it); Python's
   `re.match` maps to `re_match_end`/`re_full_match`. There is no capture
   group extraction, so nothing like `m.group(1)` exists.
2. Python's `str.splitlines()` / `for line in f` maps to an explicit
   `str_split(content, "\n")` in the app plus the trailing-LF rule (a
   trailing newline terminates the last line and must not yield an extra
   empty line); the reference mirrors that rule with
   `content.split(b"\n")[:-1]` when the last part is empty.
3. Python's `re.IGNORECASE` maps to ASCII lower-casing of both the pattern
   and the subject (`str_to_lower` / `bytes.lower`) because the engine has no
   inline flags; the reference deliberately lowers bytes so the outputs stay
   identical.
4. Python's `try/except OSError` maps to status-code checks; `fs_write`
   reports bytes written or `-1`, so a write failure is `wrote < 0` (unlike
   the `0=ok, 1=error` fs mutators).
5. CRLF input is preserved: splitting on LF leaves the `\r` at the end of the
   line, matching Python's binary read + split on `b"\n"`.
6. The reference compiles the pattern with Python `re` (including patterns the
   MINK subset does not support); the 35 validated cases stay inside the
   documented MINK subset, and the escaping fallback is only a harness
   robustness measure, not an exercised path.
