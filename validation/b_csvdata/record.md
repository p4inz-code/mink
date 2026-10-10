# B — csvdata (CSV / data processing)

PROJECT: B CSV / data processing
PURPOSE: Read a CSV file and select / sort / aggregate / re-serialize it — the
"load the spreadsheet and crunch it" task a Python developer reaches for with
`csv.reader` + `sorted()` + a `dict` accumulator + `csv.writer`.
MINK SOURCE: `validation/b_csvdata/main.mink` (74 functions emitted)
PYTHON COMPARISON: `validation/b_csvdata/python_ref.py`
CROSS-CHECK HARNESS: `validation/b_csvdata/verify.py`
FIXTURES: `validation/b_csvdata/fixtures/make_fixtures.py` (4 files: RFC 4180
sales.csv with quoted commas and doubled quotes, notes.csv with an embedded
LF inside a quoted field, ragged.csv with a short row, header_only.csv)

BUILD: PASS — `mink build validation/b_csvdata/main.mink` → `main.exe`
(x86_64-windows-pe, no external toolchain, no runtime DLL).

EXECUTION: PASS — 34/34 cases produce byte-identical stdout, stderr, exit
codes **and** byte-identical `write` output files compared with the Python
reference (`python verify.py` → `34/34 cases match the Python reference`).

FUNCTIONAL CORRECTNESS: PASS — a hand-written RFC 4180 record splitter over
the raw bytes (CR / LF / CRLF terminators outside quotes are record
boundaries; a newline inside a quoted field is data), per-record decode with
the stdlib `csv_parse_row` and re-encode with `csv_write_row` (so the tool
exercises parse and write, not a byte copy), blank records skipped, the first
non-blank record defines the column count, `select` (substring on one column,
with quoted-comma and doubled-quote fields), stable `sort` by a column
(byte order, which equals Python's `str` order for UTF-8), `agg` (group by
one column, sum an integer column, a non-integer value contributes 0, print
`key:sum` per group in ascending key order), and `write` (re-serialize header
+ data rows).

ERROR HANDLING: PASS — distinct exit codes verified against Python: 1 usage
(no/one positional, missing/extra args, non-integer / negative column, any
`-`-prefixed argument, unknown command), 2 input missing / not a regular
file, 3 no data rows (header only, or nothing selected), 4 ragged row (field
count differs from the header) or column out of range, 5 output file not
writable (implemented, not exercised — the work dir is writable). Messages go
to stderr (`rt_stderr_write`), stdout stays clean.

MEMORY: PASS — the runtime leak check (`E-R06`, exit 106) is enabled; all 34
cases exit 0/1/2/3/4, never 106, including every failing path and every output
mode. One leak was found and fixed during validation (see CLASSIFICATION).

REALISTIC INPUT: PASS — quoted fields containing commas (`"Acme, Inc."`) and
doubled quotes (`"O'Neil ""Q"" Ltd"`), a quoted field containing an embedded
LF (`"first line\nsecond line"`), CRLF-terminated records, a ragged row, and
a header-only file.

REPEATED EXECUTION: PASS — `write` run twice plus the Python reference
produce byte-identical files (`cmp` → identical); `verify.py` has been run
repeatedly with identical results (34/34 each time).

STANDALONE EXE: PASS — `main.exe` runs with no interpreter and no runtime;
the only dependency is the Windows API.

WINDOWS: PASS.

MINK LIMITATION: see CLASSIFICATION; none of the findings blocked the
application.

CLASSIFICATION:
- B (defect, FIXED this session) — the emitter left the padding bytes of a
  partial trailing 8-byte chunk uninitialized when copying a sub-word value
  into a local slot (`copy_into_slot` / `copy_static_into_slot` in
  `src/backend/emit/x86_64.rs`). A `Bool` local occupies 1 byte, but a Bool is
  read back as a **whole word** when it is passed to a call, returned, or
  compared, so a stale high byte turned `false` into a nonzero (true) value.
  The B cross-check exercised exactly this shape (`str_contains` returns a
  `Bool`, `rt_map_has(...) == true`, `fs_is_file(...) == false` as branch
  conditions), so the emitted program took wrong branches. Fixed by zeroing
  the padding bytes of a partial last chunk (`let rem = size % 8; ... if rem
  != 0 { for k in rem..8 { mov r8, 0 } }`). Regression test:
  `tests/bool_packing.rs` `native_bool_field_passed_to_a_call_stays_canonical`
  (verified to FAIL without the fix and PASS with it).
- A (application defect, found & fixed during validation) — the first version
  had three issues caught by the cross-check and the leak checker:
  1. `print_row` was declared `-> Int` but its body ended in
     `rt_print_str(...)` (a `Unit` call) — corrected to `return 0;`;
  2. `let keys = rt_map_keys(agg);` was mutated by the insertion sort — needs
     `let mut`;
  3. the `agg` print loop copied the map key with `rt_str_concat(key, ":")`
     (a read-borrowing copy) but never freed the original `key` — added
     `rt_str_free(key);` after building the line.
  `verify.py` then reached 34/34.
- D (deliberate difference) — `rt_print_str` appends CRLF without translating
  embedded newlines; the Python reference re-encodes each row with
  `csv.writer(lineterminator="\r\n")` to match byte-for-byte (the re-serialized
  `write` file is CRLF-terminated, like Python's `csv` default; `od -c`
  confirms).
- D (deliberate difference) — `rt_argc`/`rt_argv` exclude the program name;
  argv[0] is the first user argument in both implementations.
- D (deliberate difference) — the ownership checker is flow-sensitive and
  branch-local (session 126) but **not path-sensitive**; application code
  funnels to a single exit and releases each owned string exactly once.
- D (deliberate difference) — `sort` uses a hand-written stable insertion sort
  over `str_cmp` (byte order) rather than Python's `sorted()`; byte order
  equals Python `str` ordering for UTF-8.
- C (architectural limitation) — no module-level `Vec` globals
  (`E-B03`); the field vector, record vector, key vector and row buffer are
  all function locals, and `Vec` parameters are passed by value and are **not**
  consumed (the caller frees them), so `cmd_write` receives the argv vector
  plus an index instead of the output path directly.
- C (architectural limitation) — no `clone` intrinsic; duplicate a `Str` with
  the borrowing intrinsic `rt_str_concat(s, "")`. `rt_map_insert` consumes its
  key, while `rt_map_get` / `rt_map_has` / `rt_map_keys` are read-borrowing,
  so the aggregator copies the key for the probe and then hands the original
  key to `rt_map_insert`.
- C (ergonomic trap, documented) — the read-borrowing intrinsics
  (`rt_print_str`, `rt_str_concat`, `rt_argv`, `rt_vec_get`, `rt_map_get`,
  `rt_map_has`, the `rt_fs_*` reads) do not free their arguments; inline
  temporaries leak and the run ends with `E-R06` (exit 106). Each temporary
  must be bound and released, or handed to a consuming stdlib wrapper.
- D (convention) — `fs_write` returns the number of bytes written and `-1` on
  failure (it is not one of the `0=ok, 1=error` fs mutators), so `cmd_write`
  checks `wrote < 0`.
- D (convention) — `str_parse_int` consumes its argument and `str_cmp`,
  `str_contains` consume theirs; copies are passed where the caller still
  needs the original (`rt_str_concat(..., "")`).

EVIDENCE:
- TEST — `validation/b_csvdata/verify.py`, 34 cases, 0 differences (stdout,
  stderr, exit code, output file tree).
- FIXTURE — `validation/b_csvdata/fixtures/make_fixtures.py` (4 files).
- OUTPUT — see the transcript below (`write` file checked with `od -c` and
  `cmp` against the Python reference).
- REGRESSION — the emitter defect is covered by `cargo test --test
  bool_packing` (59 passed, includes
  `native_bool_field_passed_to_a_call_stays_canonical`); the application leak
  fix is covered by the runtime leak checker (`E-R06`). The full test suite
  (73 integration targets + `--lib`, 3040 tests) was re-run green before
  starting B.

## Transcript

```
$ python validation/b_csvdata/fixtures/make_fixtures.py
fixtures: 4 files under .../validation/b_csvdata/fixtures

$ mink build validation/b_csvdata/main.mink
mink: build: 'validation/b_csvdata/main.mink' -> '.../main.exe'
(target: x86_64-windows-pe, 74 function(s), 0 binding(s))

$ python validation/b_csvdata/verify.py
ok   select_city (exit 0)
ok   select_quoted_comma (exit 0)
ok   select_quoted_quotes (exit 0)
ok   select_no_match (exit 3)
ok   select_notes_embedded_lf (exit 0)
ok   sort_customer (exit 0)
ok   sort_units_lexicographic (exit 0)
ok   sort_price_lexicographic (exit 0)
ok   sort_notes (exit 0)
ok   agg_city_units (exit 0)
ok   agg_city_price_nonint (exit 0)
ok   write_roundtrip (exit 0)
ok   write_notes (exit 0)
ok   write_header_only (exit 3)
ok   ragged_select (exit 4)
ok   ragged_sort (exit 4)
ok   ragged_write (exit 4)
ok   header_only_select (exit 3)
ok   header_only_sort (exit 3)
ok   header_only_agg (exit 3)
ok   missing_file (exit 2)
ok   dir_as_file (exit 2)
ok   unknown_command (exit 1)
ok   no_args (exit 1)
ok   one_positional (exit 1)
ok   select_missing_args (exit 1)
ok   select_extra_args (exit 1)
ok   col_not_int (exit 1)
ok   col_negative (exit 1)
ok   col_out_of_range (exit 4)
ok   agg_col_out_of_range (exit 4)
ok   option_looking_arg (exit 1)
ok   sort_missing_args (exit 1)
ok   agg_missing_args (exit 1)

34/34 cases match the Python reference

$ main.exe select fixtures/sales.csv 2 Berlin
1,"Acme, Inc.",Berlin,3,10.50
3,Globex,Berlin,1,99.90
5,Initech,Berlin,2,10.50
exit=0

$ main.exe select fixtures/sales.csv 1 "Acme, Inc."
1,"Acme, Inc.",Berlin,3,10.50
4,"Acme, Inc.",Paris,5,4.00
exit=0

$ main.exe agg fixtures/sales.csv 2 3
Berlin:6
Paris:13
exit=0

$ main.exe select fixtures/notes.csv 1 second
a,"first line
second line"
exit=0

$ main.exe select fixtures/ragged.csv 1 ok
csvdata: ragged row: field count differs from the header
exit=4

$ main.exe write fixtures/sales.csv work/demo/out.csv
exit=0

$ od -c work/demo/out.csv | tail -3
0000240   B   e   r   l   i   n   ,   2   ,   1   0   .   5   0  \r  \n
0000260   6   ,   U   m   b   r   e   l   l   a   ,   P   a   r   i   s
0000300   ,   1   ,   3   .   5   0  \r  \n

$ python python_ref.py write fixtures/sales.csv work/demo/out_py.csv
$ cmp work/demo/out.csv work/demo/out_py.csv && echo identical
identical
```

## Python comparison notes (concrete differences, no ranking)

1. Python's `csv.reader` handles quoting/terminators natively; MINK splits
   records with a hand-written RFC 4180 state machine because the app must
   decide record boundaries before `csv_parse_row` (which parses one record
   only). The reference uses `csv.reader`, so both agree on quoted commas,
   doubled quotes, and embedded newlines.
2. Python's `dict.get(key, 0)` accumulator maps to `rt_map_new` +
   `rt_map_has` + `rt_map_get` + `rt_map_insert` (the insert consumes the
   key), with the probe copies bound and freed because the map reads are
   read-borrowing.
3. Python's `sorted(rows, key=...)` maps to a hand-written stable insertion
   sort over `str_cmp`; byte order equals Python `str` ordering for ASCII/UTF-8.
4. Python's `csv.writer(lineterminator="\r\n")` maps to `csv_write_row` +
   `rt_print_str`, which appends CRLF — so no newline translation is needed
   and the re-serialized file matches Python's default dialect byte-for-byte.
5. Python's `try/except OSError` maps to status-code checks (`fs_write`
   returns bytes written or `-1`; the fs reads/mutators use `0=ok, 1=error`);
   there is no exception type.
6. Python's `len(argv)` / `sys.argv[1:]` maps to `rt_argc`/`rt_argv`, which
   exclude the program name; a leading `-` marks a usage error because the
   tool defines no options.
