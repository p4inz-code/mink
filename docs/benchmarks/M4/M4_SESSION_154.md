# Session 154 (recovery) — M4 W3 `dbshell`

**Date:** 2026-11-01
**Status:** **PARTIAL — 3 of 5 implementations GREEN; Rust and Go BLOCKED on
unavailable SQLite drivers.**
**Supersedes:** the earlier Session 154 record, which stopped at the
dependency audit. This record keeps that audit and adds the contract
resolution, the final row shape, the 500-row evidence and the three passing
implementations. Nothing here is a performance result.

## 1. Contract (locked, not invented)

Source of truth: `validation/d_sqlite/main.mink`, `python_ref.py`, `record.md`,
`stdlib/sqlite.mink` and the M4 workload selection. The observable contract is
the M1 program on the in-memory common subset:

```text
dbshell <script>
```

- One statement per line; blank lines and `--` comments are skipped; each
  executed statement is echoed as `sql> <statement>` followed by its result.
- Result rendering: `OK`, `OK <n>` for `UPDATE`/`DELETE`, a bare integer for
  `COUNT(*)`, tab-separated rows with `NULL` for a null cell and an LF inside
  each row block. `stdout`/`stderr` lines themselves end with CRLF.
- Exit codes: 1 usage (argument count, any `-`-prefixed argument),
  2 script missing or not a regular file, 3 a statement failed (its `ERROR:`
  result is printed and the run stops).
- Comparison is limited to this subset. W3 does **not** claim SQLite file
  compatibility, durability, fsync, query planning, indexes, `JOIN`,
  `GROUP BY`, `ORDER BY`, advanced SQL or full SQLite compatibility.

### 1.1 Final fixture shape

`CREATE TABLE items (sku INT, name STR, qty INT, category STR)` — four
columns, then **one `INSERT` per row** (MINK's engine has no multi-row
`INSERT`), then **13 non-bulk statements**:

`COUNT(*)`, `SELECT * … WHERE category`, `UPDATE … WHERE sku`,
`SELECT * … WHERE sku`, `DELETE … WHERE sku`, `COUNT(*) … WHERE qty < 50`,
`BEGIN` + `INSERT` + `COMMIT`, `BEGIN` + `INSERT` + `ROLLBACK`, `COUNT(*)`.

Total for the baseline: 114 statements (1 schema + 100 rows + 13). The M4
sketch said "10-12 statements"; the actual non-bulk count is 13 because both
transactions and their verification counts are included. The sketch is
corrected here rather than dropping a transaction to hit the number. All five
languages run the identical script.

### 1.2 The 500-row probe, resolved

`probe_500.sql` is 502 statements (schema + 500 rows + one `COUNT(*)`). It
**passes** in MINK, Python and C#, byte-for-byte against the oracle:

| Case | Rows | Statements | Exit | stdout lines | Final `COUNT(*)` |
| --- | --- | --- | --- | --- | --- |
| `baseline_100` | 100 | 114 | 0 | 204 | 100 |
| `scale_200` | 200 | 214 | 0 | 404 | 200 |
| `probe_500` | 500 | 502 | 0 | 1,004 | 500 |

The earlier "the 500-row probe is expected to fail" expectation is **not
reproduced** at this row shape. This is recorded as an empirical correction to
the old assumption, not as a MINK defect: no engine misbehaviour appears, and
the runtime was not timed. The conservative "~220 four-column rows" note in
`stdlib/sqlite.mink` remains an unverified envelope estimate; M4 neither
confirms nor contradicts it beyond this fixture. `probe_500` is a supporting
(scale) case, never a scored one.

## 2. Implementations

| Language | Path | Store | Build |
| --- | --- | --- | --- |
| MINK | `benchmarks/m4/apps/mink/w3_sqlite/dbshell.mink` | bundled `sqlite` (in-memory `Str` blob) | built |
| Python | `benchmarks/m4/apps/python/w3_sqlite/dbshell.py` | `sqlite3` `:memory:` | none (source) |
| C# | `benchmarks/m4/apps/csharp/w3_sqlite/` | `Microsoft.Data.Sqlite` in-memory | built |
| Rust | `benchmarks/m4/apps/rust/w3_sqlite/` | `rusqlite` 0.32, `bundled` | **blocked** |
| Go | `benchmarks/m4/apps/go/w3_sqlite/` | `database/sql` + `modernc.org/sqlite` | **blocked** |

Fully written sources exist for Rust and Go; their manifests declare the
intended dependencies. They are **not** claimed green — neither has ever
compiled here.

## 3. Dependency audit

| Language | Dependency | Version / source | Local state |
| --- | --- | --- | --- |
| MINK | bundled `sqlite` | MINK 1.0.4 stdlib | available |
| Python | `sqlite3` | CPython 3.11.9 stdlib | available |
| Rust | `rusqlite` + `libsqlite3-sys` | `rusqlite 0.32`, crates.io | **`rusqlite` absent**; only `libsqlite3-sys 0.30.1` cached |
| Go | `modernc.org/sqlite` | module, proxy.golang.org | **absent** from `GOMODCACHE` and its download cache |
| C# | `Microsoft.Data.Sqlite` | 8.0.0 (NuGet cache), transitive `SQLitePCLRaw 2.1.6` chain | available, restores offline |

Recorded risk: the cached `SQLitePCLRaw.lib.e_sqlite3 2.1.6` raises NuGet
`NU1903` (high severity, GHSA-2m69-gcr7-jv3q). The warning is surfaced on
every C# build and is left visible rather than suppressed; upgrading the
package is not authorized and would need a network restore.

No package was installed. No engine was substituted or hand-written.

## 4. Correctness matrix

| Language | Cases passed | Scored passed | State |
| --- | --- | --- | --- |
| MINK | 13/13 | 10/10 | GREEN |
| Python | 13/13 | 10/10 | GREEN |
| C# | 13/13 | 10/10 | GREEN |
| Rust | 0/13 | 0/10 | **BLOCKED** (not built) |
| Go | 0/13 | 0/10 | **BLOCKED** (not built) |

The gate is `python benchmarks/m4/harness/validate.py --workload w3_sqlite`;
it correctly reports exit 1 for W3 and refuses measurement for the blocked
languages. Two consecutive full runs produced byte-identical JSONL records.

## 5. Exact authorization still required

**Rust** — fetch `rusqlite` (network), then build:

```text
cargo build --release --manifest-path benchmarks/m4/apps/rust/w3_sqlite/Cargo.toml
```

`bundled` compiles SQLite through the `cc` crate; `cl.exe` is not on `PATH`,
so a working MSVC C toolchain must be confirmed after the fetch.

**Go** — from `benchmarks/m4/apps/go/w3_sqlite`:

```text
go get modernc.org/sqlite
go mod tidy
```

then pin and record the resolved version plus its transitive module set.

## 6. Git state

| Item | Value |
| --- | --- |
| HEAD at session start | `b74f249` (benchmark docs only) |
| `origin/main` | unchanged |
| Pre-existing modified files | 40 tracked Session 128–149 files, untouched |
| New in this session | W3 apps, fixtures, oracle, expected outputs; harness changes |
| MINK source | **not modified** by this session |

No reset, clean, stash, rebase, amend, force, push, tag or publish was run.

## 7. Limitations

- W3 is not measurement-ready: two of five implementations have never built.
- The `~220-row` envelope and the "500-row failure" expectation are corrected
  in the M4 workload document; the runtime itself is unchanged.
- C# pins an older SQLite package because it is the only cached one.
- `probe_500` is a scale/boundary observation, not evidence of performance.
