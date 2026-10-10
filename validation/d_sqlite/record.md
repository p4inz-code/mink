# D — dbshell (SQLite-backed application)

PROJECT: D SQLite / database
PURPOSE: Run a script of SQL statements against an embedded relational store
and report each result — the "create a table, record rows, query, update and
transaction" task a Python developer reaches for with the `sqlite3` module.
MINK SOURCE: `validation/d_sqlite/main.mink` (139 functions emitted)
PYTHON COMPARISON: `validation/d_sqlite/python_ref.py` (the real `sqlite3`
module, re-rendered in the MINK engine's result format)
CROSS-CHECK HARNESS: `validation/d_sqlite/verify.py`
FIXTURES: `validation/d_sqlite/fixtures/make_fixtures.py` (5 scripts: the full
workload ending in an error, the same without the error, a CRLF copy, a
comments-only script, an empty script)

BUILD: PASS — `mink build validation/d_sqlite/main.mink` → `main.exe`
(x86_64-windows-pe, no external toolchain, no runtime DLL).

EXECUTION: PASS — 10/10 cases produce byte-identical stdout, stderr and exit
codes compared with the Python `sqlite3` reference (`python verify.py` →
`10/10 cases match the Python reference`). The full workload's transcript
covers 26 statements: schema, inserts (including a missing column → NULL),
`SELECT *`, `SELECT COUNT(*)`, `WHERE` on an integer, a range and a string,
`UPDATE`, `DELETE`, a committed transaction, a rolled-back transaction, and
the first failing statement that stops the run.

FUNCTIONAL CORRECTNESS: PASS — `CREATE TABLE`, `INSERT` (positional and with a
column list), `SELECT *`, `SELECT COUNT(*)`, `WHERE` (with `=`, `<`, string
equality and NULL handling), `UPDATE`, `DELETE`, and explicit
`BEGIN`/`COMMIT`/`ROLLBACK`. SQL NULL semantics are honoured: `WHERE qty < 50`
excludes the row whose `qty` is NULL, exactly like `sqlite3`. Transactions are
all-or-nothing including DDL — `ROLLBACK` after a `CREATE TABLE` discards the
table, so the later insert reports `no such table`.

ERROR HANDLING: PASS — distinct exit codes verified against Python: 1 usage
(no/extra arguments or any option), 2 script missing / not a regular file, 3 a
statement failed (its `ERROR:` result is printed and the run stops). Messages
go to stderr (`rt_stderr_write`), stdout stays clean.

MEMORY: PASS — the runtime leak check (`E-R06`, exit 106) is enabled; all 10
cases exit 0/1/2/3, never 106. One leak was found and fixed during validation
(see CLASSIFICATION).

REALISTIC INPUT: PASS — a 26-statement inventory workload with an injected
error, a clean variant, a CRLF variant (the runner trims CR so line endings do
not matter), a comments-only script and an empty script.

REPEATED EXECUTION: PASS — repeated runs of the same script produce identical
bytes; `verify.py` has been run repeatedly with identical results (10/10 each
time).

STANDALONE EXE: PASS — `main.exe` runs with no interpreter and no runtime;
the only dependency is the Windows API.

WINDOWS: PASS.

MINK LIMITATION: see CLASSIFICATION; none of the findings blocked the
application.

CLASSIFICATION:
- C (architectural limitation, the biggest one for this category) — the
  database is an **in-memory text blob**: `sql_new() -> Str` and every
  `sql_exec`/`sql_*` call consumes the blob and hands the updated one back.
  There is no file-backed database, no `open`/`close`, and no persistence
  across runs — an app must rebuild the database each time. This is the
  Python-parity gap for this category (Python has `sqlite3.connect("file.db")`).
- C (architectural limitation) — the SQL engine is a deliberate subset:
  `CREATE TABLE`, `INSERT`, `SELECT *` / `SELECT COUNT(*)`, `WHERE`,
  `UPDATE`, `DELETE`, `BEGIN`/`COMMIT`/`ROLLBACK`. There is no `ORDER BY`,
  `GROUP BY`, `JOIN`, `LIMIT`, `ALTER TABLE`, secondary index, or aggregate
  other than `COUNT(*)`; a non-`*`/`COUNT(*)` select is rejected with
  `ERROR: only SELECT * / SELECT COUNT(*) is supported`.
- C (ergonomic trap, documented, sharper than elsewhere) — `rt_str_concat` is
  read-borrowing and does **not** free its arguments, so a *nested* call like
  `rt_str_concat("sql> ", rt_str_concat(line, ""))` leaks the inner
  temporary and the run ends with `E-R06` (exit 106). Every allocated
  argument that is not handed to a consuming callee must be bound and freed.
- C (ergonomic trap, documented) — a user function's `Str` parameter is
  consuming and must be freed by the callee (`has_prefix` frees both of its
  arguments; callers pass copies).
- D (deliberate difference) — the result rendering is the stdlib engine's
  (`OK`, `OK <n>`, a bare integer for `COUNT(*)`, tab-separated rows each LF
  terminated with `NULL` for a null cell, `ERROR: ...`). The Python reference
  re-renders the real `sqlite3` rows in exactly this format so the transcripts
  are byte-identical; this proves the *semantics* match, not that sqlite3
  prints the same text.
- D (harness normalization, documented) — the Python reference maps
  `sqlite3` error text onto the MINK engine's canonical message for the one
  error the workload exercises (`no such table`). It is not a claim that
  arbitrary engine error strings match.
- D (deliberate difference) — `rt_print_str` appends CRLF without translating
  embedded newlines, so a `SELECT *` block (which already ends each row with
  LF) is followed by an extra CRLF; the Python reference writes raw bytes +
  `\r\n` to match.
- D (deliberate difference) — `rt_argc`/`rt_argv` exclude the program name;
  a leading `-` marks a usage error because the tool defines no options.
- D (semantics verified) — NULL comparisons follow SQL three-valued logic in
  both implementations (`NULL < 50` is not true, so the row is excluded), and
  a rolled-back transaction discards DDL as well as DML.
- A (application defect, found & fixed during validation) — the first version
  echoed each statement with a nested `rt_str_concat`, leaking the inner
  statement copy on every line (`E-R06`, exit 106). Fixed by binding the copy
  (`stmt_copy`) and releasing it before the outer concatenation. After the
  fix the leak checker is silent on every case. No MINK compiler/runtime
  defect was found in D — the engine, its subset and its transactions behaved
  as documented.

EVIDENCE:
- TEST — `validation/d_sqlite/verify.py`, 10 cases, 0 differences (stdout,
  stderr, exit code).
- FIXTURE — `validation/d_sqlite/fixtures/make_fixtures.py` (5 scripts).
- OUTPUT — see the transcript below.
- REGRESSION — the leak fix is application-level; the runtime leak checker
  (`E-R06`) is the regression net and every case passes it. The engine itself
  is covered by `cargo test --test sqlite_lib` (16 passed) and the full suite
  (73 integration targets + `--lib`, 3040 tests) was re-run green before
  starting D.

## Transcript

```
$ python validation/d_sqlite/fixtures/make_fixtures.py
fixtures: 5 files under .../validation/d_sqlite/fixtures

$ mink build validation/d_sqlite/main.mink
mink: build: 'validation/d_sqlite/main.mink' -> '.../main.exe'
(target: x86_64-windows-pe, 139 function(s), 0 binding(s))

$ python validation/d_sqlite/verify.py
ok   inventory_stops_on_error (exit 3)
ok   clean_workload (exit 0)
ok   crlf_workload (exit 0)
ok   comments_only (exit 0)
ok   empty_script (exit 0)
ok   missing_file (exit 2)
ok   dir_as_file (exit 2)
ok   no_args (exit 1)
ok   two_positionals (exit 1)
ok   option_looking_arg (exit 1)

10/10 cases match the Python reference

$ main.exe fixtures/inventory.sql        (excerpt)
sql> CREATE TABLE items (sku INT, name STR, qty INT)
OK
sql> INSERT INTO items VALUES (1001, 'bolt', 240)
OK
sql> INSERT INTO items (sku, name) VALUES (1003, 'washer')
OK
sql> SELECT * FROM items
1001	bolt	240
1002	nut	12
1003	washer	NULL

sql> SELECT COUNT(*) FROM items WHERE qty = 12
1
sql> SELECT * FROM items WHERE qty < 50
1002	nut	12

sql> UPDATE items SET qty = 0 WHERE sku = 1001
OK 1
sql> DELETE FROM items WHERE sku = 1002
OK 1
sql> BEGIN
OK
sql> INSERT INTO items VALUES (1004, 'screw', 80)
OK
sql> COMMIT
OK
sql> SELECT COUNT(*) FROM items
3
sql> BEGIN
OK
sql> INSERT INTO items VALUES (1005, 'ghost', 5)
OK
sql> ROLLBACK
OK
sql> SELECT COUNT(*) FROM items
3
sql> SELECT * FROM missing
ERROR: no such table
exit=3
```

## Python comparison notes (concrete differences, no ranking)

1. Python's `sqlite3.connect(":memory:")` maps to `sql_new()` — the MINK
   database is a returned `Str` blob threaded through every call rather than a
   connection object, and it cannot be backed by a file.
2. Python's `conn.commit()` / autocommit maps to explicit `BEGIN`/`COMMIT`/
   `ROLLBACK` statements; the reference sets `isolation_level = None` so its
   transaction boundaries match the script exactly.
3. Python's cursor rows (tuples with Python values) are re-rendered as the
   MINK engine's tab-separated text with `NULL` for a null cell, so a
   `SELECT *` transcript is comparable; the row order is insertion (rowid)
   order in both engines.
4. Python's `cursor.rowcount` supplies the `OK <n>` count for `UPDATE`/
   `DELETE`; `CREATE`/`INSERT`/`BEGIN`/`COMMIT`/`ROLLBACK` print a bare `OK`
   because that is what the MINK engine reports.
5. Python's `sqlite3.Error` maps to a status result string starting with
   `ERROR:` (there is no exception type); the app stops at the first one.
6. Python's `del`/`gc` hide memory management; in MINK every returned string
   (including each `sql_exec` result) is freed explicitly and the `E-R06`
   leak checker enforces it.
