# Session 111 — Windows SQLite (S41)

**Parity blocker closed:** S41 `SQLite (sqlite3)` — MISSING → **VERIFIED**
**Parity-blocking set:** 12 → 9 (S44, L34 reclassification, S41)
**Linux:** frozen; no shared Rust runtime code was touched by this unit.

## What landed

- `stdlib/sqlite.mink` — a self-contained embedded SQL engine written in MINK
  over the runtime primitives. No FFI, no external `sqlite3` dependency, no new
  intrinsics.
- `tests/sqlite_lib.rs` — 16 permanent tests; each prepends the module, compiles
  a real Windows PE through the `mink build` CLI and executes it.
- `examples/inventory_db/main.mink` — a small end-to-end proof application.
- `npm/mink/stdlib/sqlite.mink` — bundle sync.

## Architecture

The database is one `Str` blob of newline-terminated records:

```
T <1> name <1> ncols <1> c1:TYPE,c2:TYPE    table schema
R <1> name <1> v1 <1> v2 ...                row (one field per column)
```

- A **NULL** column is a one-byte field holding byte `0`. Because string
  literals containing control bytes are rejected, byte `0` can never occur in a
  real value, so NULL and the empty string stay distinguishable — the one
  semantic wart most naive text encodings fall into.
- A **transaction** is an optional `#<n>\n<snapshot>` prefix in front of the
  records. The length prefix keeps the snapshot unambiguous even though it
  contains newlines. `BEGIN` writes it, `COMMIT` strips it, `ROLLBACK` restores
  it, and record reading starts after it.
- `sql_exec` copies the database and statement text into a word arena and runs
  the whole statement from there. That is deliberate: MINK consumes a `Str`
  argument at a user-function boundary, while `Ptr<Int>` is copied, so a
  `Ptr<Int>` arena is what lets the scanner revisit the same text as often as it
  needs without ownership pass-backs.
- Text is packed **eight bytes per word** in both the arena and the builders.
  The first version stored one byte per word (eight times the text size); the
  fixed 1 MiB runtime heap made that the difference between ~80 and ~220
  appended rows.

## SQL subset

`CREATE TABLE t (col INT|STR, ...)` · `INSERT INTO t VALUES (...)` ·
`INSERT INTO t (cols) VALUES (...)` · `SELECT * FROM t [WHERE ...]` ·
`SELECT COUNT(*) FROM t [WHERE ...]` · `UPDATE t SET col = v [WHERE ...]` ·
`DELETE FROM t [WHERE ...]` · `BEGIN` / `COMMIT` / `ROLLBACK`.
Conditions are `col OP value` (`= != <> < > <= >=`) or `col IS [NOT] NULL`,
joined with `AND` / `OR`. Numbers compare numerically and strings
lexicographically; `''` collapses to a single quote inside literals.

## Defects found and fixed during the unit

1. **Byte-indexed versus word-indexed arena access.** The first draft mixed the
   two, so record offsets were read as word indices. Caught immediately by
   native execution; the accessors now go through one `_db`/`_sq` pair.
2. **Statements read the transaction prefix as data.** `_run` scanned the whole
   blob, so inside a transaction every statement folded the snapshot back into
   its own body and row counts doubled. Fixed with an explicit body-start field
   (`_b0`) that every scanner honours; verified by the commit/rollback test.
3. **Column-list `INSERT` conflated "absent" with "unknown".** Columns not named
   by the list must become NULL, and a name that is not a column must be an
   error. The first version treated both as an error; now every listed name is
   validated first and unlisted columns decode to NULL.
4. **Quoted-literal comparison did not collapse `''`.** Values were stored
   collapsed but compared against the raw source range, so
   `WHERE a = 'O''Brien'` never matched. Comparison now builds the decoded
   literal.
5. **Missing closing parenthesis was accepted.** `INSERT INTO t VALUES (1` is
   now rejected (`ERROR: expected ')' after values`).
6. **Eight-fold memory amplification.** Arena and builders stored one byte per
   word; packing to eight bytes per word raised the sustained append workload
   from ~80 to ~220 four-column rows.

## Verification

`cargo test --test sqlite_lib` — **16/16 pass** at HEAD. Each test builds and runs
a real Windows PE; a clean exit is also the ownership/leak proof (leaks exit 106
with `E-R06`).

Covered: create/insert/select/count; all six comparison operators; NULL versus
empty string; `IS NULL`/`IS NOT NULL`; negative and 53-bit integers; `''`
escaping and embedded spaces; named-column inserts including partial column
lists; update/delete with and without `WHERE`; commit and rollback; twenty-three
malformed statements each pinned to its exact error string; duplicate tables;
missing parentheses; control-byte literals; an empty database and every accessor;
300 insert/select/update/delete cycles in one process; a 200-row order workload
with aggregation, a bulk update, a transactional bulk delete and rollback; and
UTF-8 byte sequences round-tripping through values and `WHERE` comparisons.

`mink run examples/inventory_db/main.mink` exercises the public path end to end
and exits 0.

## Measured limit

`sql_exec` returns a fresh database blob, so each statement copies the whole
database and an append-only workload allocates O(N²) bytes in total. On the
runtime's fixed 1 MiB heap a four-column table sustains **220 appended rows**;
230 reports `E-R02` (out of memory). This is the runtime allocator's property
(fixed arena, non-coalescing first-fit — the same characteristic documented in
`stdlib/zlib.mink`), not a leak in this module: no live allocation remains at
exit. The bound is recorded in the module header and in the test-suite header so
it cannot silently regress.

## Matrix

S41 row updated to VERIFIED with code/test/exec/doc evidence; §10.1/§10.2/§10.3/
§10.4 and the headline counts recomputed from the rows (VERIFIED 88 → 89,
MISSING 81 → 80, P1 10 → 9, rows requiring work 156 → 155).
