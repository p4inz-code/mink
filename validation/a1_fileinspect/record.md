# A1 — fileinspect (CLI utility)

PROJECT: A1 CLI utility
PURPOSE: Inspect a file and report on it — size, extension, SHA-256, line
count, preview, regex match count — from the command line.
MINK SOURCE: `validation/a1_fileinspect/main.mink` (144 functions emitted)
PYTHON COMPARISON: `validation/a1_fileinspect/python_ref.py`
CROSS-CHECK HARNESS: `validation/a1_fileinspect/verify.py`

BUILD: PASS — `mink build validation/a1_fileinspect/main.mink` → `main.exe`
(x86_64-windows-pe, no external toolchain, no runtime DLL).

EXECUTION: PASS — 20/20 cases produce byte-identical stdout, stderr and exit
codes compared with the Python reference (`python verify.py` → `0/20 cases
differ`).

FUNCTIONAL CORRECTNESS: PASS — size, extension, stem, SHA-256 (matches
Python `hashlib`), line counting, first-5-lines preview (raw bytes preserved),
regex match count (`re_count`, Python `re.findall`) and the anchored test
(`re_match_end`, Python `re.match` — matches only at the very start of the
text, so `--grep ERROR` reports `grep_matches: 1` together with
`grep_at_start: no` while `--grep 2026` reports `grep_matches: 21` with
`grep_at_start: yes`).

ERROR HANDLING: PASS — distinct exit codes verified: 1 usage, 2 missing file,
3 directory instead of file, 4 over the inspection window, 5 short read.
Messages go to stderr (`rt_stderr_write`), stdout stays clean.

MEMORY: PASS — the runtime leak check (`E-R06`, exit 106) is enabled, so every
successful and every failing run must release every allocation. All 20 cases
exit 0/1/2/3/4, never 106.

REALISTIC INPUT: PASS — 3,420,000-byte file inspected (SHA-256 computed over
all 3.4 MB); UTF-8 content; CRLF file; file with no trailing newline; empty
file.

REPEATED EXECUTION: PASS — verify.py runs 18 invocations per pass; run three
times with identical results.

STANDALONE EXE: PASS — `main.exe` runs with no interpreter, no runtime and no
arguments beyond its own; the only dependency is the Windows API.

WINDOWS: PASS.

MINK LIMITATION: Two practical constraints were hit and worked around
(details below); neither blocks the application.

CLASSIFICATION:
- D (deliberate difference) — `rt_print_str` is println-equivalent: it writes
  the string and appends CRLF, and it does *not* translate embedded newlines.
  Python's text-mode `print` translates every `\n` to `\r\n`. Both outputs were
  made byte-identical, so this is a convention difference, not a defect.
- D (deliberate difference) — `rt_argc()`/`rt_argv()` exclude the program name,
  unlike C `argv` and Python `sys.argv`. The first user argument is index 0.
- D (deliberate difference, documented in `docs/implementation/OWNERSHIP_IMPLEMENTATION.md`)
  — the V1 ownership checker is flow-sensitive but **not path-sensitive**: a
  value moved in one branch of an `if` cannot be used in another branch that
  appears later in the function. Application code must therefore funnel to a
  single exit and release each owned string exactly once (this is exactly the
  convention the standard library itself uses: "One `return` per function at
  the end").
- C (architectural limitation) — no explicit `clone` intrinsic. An owned `Str`
  can only be duplicated with the borrowing intrinsic `rt_str_concat(s, "")`;
  a user-defined `dup()` helper does not work because a *user* call consumes
  its argument. This makes copies verbose for functions that need to keep a
  string alive across several stdlib calls.

EVIDENCE:
- TEST — `validation/a1_fileinspect/verify.py`, 20 cases, 0 differences.
- FIXTURE — `validation/a1_fileinspect/fixtures/make_fixtures.py` (log, CRLF,
  no-trailing-newline, UTF-8, empty, 3.4 MB).
- OUTPUT — see the recorded transcripts in `record.md` below.
- REGRESSION — the defect found during this validation was in the application
  (a `report`/`fail_msg` pair that did not release the `Str` parameters they
  consume), not in MINK; it is recorded here as the ergonomic trap that the
  leak checker caught immediately.
- REGRESSION — a second application defect surfaced during the milestone's
  final sweep: `grep_at_start` was implemented with `re_is_match`
  (search-anywhere) instead of `re_match_end` (anchored), so any pattern that
  occurred only mid-text was reported as an anchored match. The harness scored
  2/18 differences. After switching to `re_match_end` the harness is green, and
  two cases were added to pin both directions of the anchored test: `2026`
  (present at the start of every line and nowhere else → `21` matches,
  `grep_at_start: yes`) and `^2026` (explicit anchor → `yes`), alongside the
  existing `ERROR` / `nomatch-xyz` cases that must answer `no`. This defect was
  in the application, not in the MINK regex engine (`re_match_end`/
  `re_is_match`/`re_count` all behaved exactly as documented); the record's
  earlier "0/18" claim had been taken from a run of the pre-fix binary, and the
  fix plus the new cases are what the current `0/20` result measures.

## Transcript

```
$ python validation/a1_fileinspect/fixtures/make_fixtures.py
fixtures: 7 entries

$ main.exe fixtures/app.log --sha --lines
path: validation/a1_fileinspect/fixtures/app.log
size: 1070
extension: .log
name: validation/a1_fileinspect/fixtures/app
window: 1048576
window_source: default
sha256: c1140ff41f065205ad901685e0c62e21a66ab7cd6e0f2f704ebcb1c9b8fde4ad
lines: 21
preview: 2026-10-01T00:00:00Z INFO  worker started pid=1000
...
exit=0

$ main.exe fixtures/nope.txt ; echo exit=$?
fileinspect: no such file: fixtures/nope.txt        (stderr)
exit=2

$ main.exe fixtures                                # a directory
fileinspect: not a regular file: fixtures          (stderr)
exit=3

$ main.exe fixtures/big.txt                        # 3.4 MB over the 1 MiB window
fileinspect: refusing to inspect  is 2371424 bytes over the inspection window
of 1048576 bytes; raise it with --max-bytes       (stderr)
exit=4

$ main.exe fixtures/big.txt --max-bytes 4194304 --sha
size: 3420000
sha256: 93c197aedd4d49c472d9d8dd24117b677783241b51903fd9cf8aeb22a3ef6275
exit=0

$ main.exe fixtures/app.log --grep 2026
...
grep_matches: 21
grep_at_start: yes
exit=0

$ main.exe fixtures/app.log --grep ^2026
...
grep_matches: 1
grep_at_start: yes
exit=0

$ main.exe fixtures/app.log --grep ERROR            # mid-text only
...
grep_matches: 1
grep_at_start: no
exit=0
```

## Python comparison notes (concrete differences, no ranking)

1. Python's `os.path.getsize` / `open().read()` are one call each; MINK needs
   `rt_fs_file_size` plus `rt_fs_read` and must keep both results alive across
   several reports because the standard-library wrappers consume their
   arguments.
2. Python returns `None`/raises; MINK has no exception type — errors are
   status codes (`rt_stderr_write` + an exit code), which for a CLI is
   perfectly workable but means no `try/except`.
3. `re.match` (anchored at the start of the text) corresponds to
   `re_match_end` — *not* to `re_is_match`, which searches anywhere in the text
   (Python `re.search`). `re.findall` counting corresponds to `re_count`. There
   is no direct "find all with positions and capture groups" equivalent used
   here beyond `re_find`/`re_find_all`.
4. Python can duplicate a string with `s[:]`; MINK needs the intrinsic
   `rt_str_concat(s, "")` because there is no `str_clone` intrinsic.
