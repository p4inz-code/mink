# Session 154 — M4 W3 `dbshell`: BLOCKED on dependencies

**Date:** 2026-11-01
**Status:** **BLOCKED — W3 is NOT implemented and NOT green.**
**Scope attempted:** inspect the M1 SQLite workload, lock the W3 contract,
audit the five-language SQL dependency set, and begin the ports.
**Result:** the contract and MINK feasibility evidence are established, but
Rust and Go have **no SQLite driver available locally**, and installing one
requires network authorization that this session does not have. Per the M4
stop rule, no package was installed and no substitute database architecture
was written.

Sessions 155 (W4), 156 (W5) and 157 (readiness audit) were **not started**
because W3 is not green.

## 1. M1 evidence inspected

Read without modification:

- `validation/d_sqlite/main.mink` — the M1 `dbshell`;
- `validation/d_sqlite/python_ref.py` — the `sqlite3` reference renderer;
- `validation/d_sqlite/record.md` — 10/10 cases, classification and limits;
- `validation/d_sqlite/verify.py` and the 5 committed scripts under
  `validation/d_sqlite/fixtures/`;
- `stdlib/sqlite.mink` — the actual engine and its public API.

### 1.1 What MINK's engine actually is

| Property | Verified fact |
| --- | --- |
| Storage | In-memory `Str` blob; `sql_new() -> Str`, `sql_exec(db, sql) -> (result, db)`, `sql_free(db)` |
| Persistence | **None.** No file open/save, no durability, no fsync |
| Supported statements | `CREATE TABLE` (INT/STR columns), `INSERT` (positional or column list), `SELECT *`, `SELECT COUNT(*)`, `WHERE`, `UPDATE`, `DELETE`, `BEGIN`/`COMMIT`/`ROLLBACK` |
| Not supported | `ORDER BY`, `GROUP BY`, `JOIN`, `LIMIT`, `ALTER TABLE`, indexes, aggregates other than `COUNT(*)`, non-`*` projections |
| Result rendering | `OK`, `OK <n>` for UPDATE/DELETE, bare integer for `COUNT(*)`, tab-separated rows each LF-terminated with `NULL` for a null cell, `ERROR: <stable text>` |
| Transactions | Snapshot prefix in the blob; rollback discards DDL as well as DML |
| Cost model | Every statement copies the whole database; total allocation is O(N²) over an append-only script |

## 2. W3 contract (semantics locked; one item left open)

The M4 contract is the M1 program reduced to the in-memory common subset:

```text
dbshell <script>
```

- One statement per line; blank lines and `--` comments are skipped; each
  executed statement is echoed as `sql> <statement>` followed by its result.
- stdout/stderr are UTF-8 with CRLF; result blocks keep their internal LF
  (the M1/MINK convention).
- Exit codes: 1 usage (argument count, any `-`-prefixed argument),
  2 script missing/not a regular file, 3 a statement failed (its `ERROR:`
  result is printed and the run stops).
- Comparison is limited to the observable common subset. W3 does **not**
  claim to measure SQLite file compatibility, durability, query planning,
  indexes, `JOIN`, `GROUP BY`, `ORDER BY`, advanced SQL or full SQLite
  compatibility.

### Resolution A — the baseline script shape

`M4_WORKLOAD_SELECTION.md` says "100 rows, four columns; 10-12 statements".
MINK's engine has no multi-row `INSERT` and no procedural construct, so a
100-row database cannot be built with 10-12 statements in MINK. The baseline
is therefore: `CREATE TABLE` + one `INSERT` per row + roughly 10-12
schema/select/update/delete/transaction statements. The "10-12" figure
describes the non-bulk statements, not the total. This matches the M1 engine
and is the only shape all five languages can share.

### Open item — the 500-row "failure probe"

The M4 document expects a one-shot 500-row probe to **fail** inside MINK's
documented ~220 four-column-row envelope. The measured MINK behaviour does
not reproduce that expectation (see §3). This must be settled before W3 is
implemented: either the probe is recorded as a scale observation, or the
probe needs a defined row width/allocator condition. It is not silently
reclassified here.

## 3. MINK feasibility probe (evidence only, not a benchmark result)

The existing M1 binary was run against generated scripts of the same shape as
the intended W3 baseline (4 columns: INT, STR name, INT, STR category).

| Rows | Statements | Exit | stdout lines | Final `COUNT(*)` |
| --- | --- | --- | --- | --- |
| 100 | 102 | 0 | 204 | 100 |
| 200 | 202 | 0 | 404 | 200 |
| 500 | 502 | 0 | 1,004 | 500 |

All three completed with no stderr and the correct count. No timing was
measured.

This contradicts two expectations recorded earlier:

1. `M4_WORKLOAD_SELECTION.md` expects a 500-row probe to fail ("500-row
   failure probe").
2. `stdlib/sqlite.mink` and the M1 record cite a ceiling of roughly 220
   four-column rows on the default heap.

Possible explanations are the compact row width used here (`item-001`,
`cat-0`), which keeps the copied blob small, and a conservative earlier
estimate. This is recorded as an observation, not as a MINK defect: no
engine misbehaviour was seen, and the run was clean at every size.

## 4. Dependency audit

| Language | Planned dependency (per M4) | Local availability | Evidence |
| --- | --- | --- | --- |
| MINK | bundled `sqlite` | **available** | `stdlib/sqlite.mink`, compiler 1.0.4 |
| Python | `sqlite3` stdlib | **available** | CPython 3.11.9 |
| Rust | `rusqlite` (bundled SQLite) | **ABSENT** | cargo cache has `libsqlite3-sys 0.30.1`, `cc`, `pkg-config`, `vcpkg`; no `rusqlite` crate; no git checkout; no vendored crate |
| Go | `database/sql` + `modernc.org/sqlite` | **ABSENT** | neither `GOMODCACHE` nor its download cache contains `modernc.org` or any SQLite driver (`mattn`, `glebarez`, …) |
| C# | `Microsoft.Data.Sqlite` | **available locally** | NuGet global cache has `Microsoft.Data.Sqlite 8.0.0` with the `SQLitePCLRaw 2.1.6` chain (`bundle_e_sqlite3`, `core`, `lib.e_sqlite3`, `provider.e_sqlite3`) |

No package was installed. No network restore was attempted. Nothing was
substituted for the missing drivers.

### Why no substitute was written

Writing a hand-rolled SQL engine in Rust or Go was rejected:

- it is a fundamentally different architecture from Python's `sqlite3` and
  C#'s `Microsoft.Data.Sqlite`, so the comparison would measure "writing a
  database" rather than "using one";
- it would inflate those ports' source size and developer effort for reasons
  unrelated to the workload;
- the M4 rules require the accepted standard package for the task and forbid
  disadvantages created by the benchmark.

## 5. Correctness matrix

| Workload | MINK | Python | Rust | Go | C# |
| --- | --- | --- | --- | --- | --- |
| W3 `dbshell` | not implemented | not implemented | **blocked** | **blocked** | not implemented |

No case was run. There is no W3 correctness result, and W3 is not complete.

## 6. Exact authorization needed to unblock

### Rust

Add to `benchmarks/m4/apps/rust/w3_sqlite/Cargo.toml`:

```toml
rusqlite = { version = "0.32", features = ["bundled"] }
```

and build **online** (drop `--offline`):

```text
cargo build --release --manifest-path benchmarks/m4/apps/rust/w3_sqlite/Cargo.toml
```

The exact patch version must be recorded from the resulting `Cargo.lock`.
`libsqlite3-sys 0.30.1` is already cached, but `rusqlite` itself must be
fetched from crates.io.

Additional risk to verify after authorization: `bundled` compiles SQLite's C
source through the `cc` crate, and `cl.exe` is **not on `PATH`**. Rust's MSVC
toolchain can link, so a Visual Studio C toolchain probably exists, but C
compiler discovery has not been proven. If no C compiler is found, Rust W3
needs a different authorized approach — it is not worked around here.

### Go

```text
cd benchmarks/m4/apps/go/w3_sqlite
go get modernc.org/sqlite
go mod tidy
```

The resolved module version must be pinned in `go.mod`/`go.sum` and recorded.
`modernc.org/sqlite` also pulls a large transitive module set (libc, mathutil,
memory, and platform packages); that burden must be recorded exactly as it was
for `ureq` in W2.

### C#

No authorization is required: `Microsoft.Data.Sqlite 8.0.0` and its
`SQLitePCLRaw 2.1.6` dependencies are already in the local NuGet cache. The
offline restore path still has to be verified when the port is written, and
the package must be pinned exactly (`8.0.0`).

## 7. Failures and fixes

None. No implementation was written and no MINK file was changed.

## 8. Git state

| Item | Value |
| --- | --- |
| HEAD | `441912ba065315656a6cdc994fdd2a7a93c580cc` |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` |
| Modified tracked | 40 pre-existing Session 128–149 files, untouched |
| `git diff --check` | clean |
| New in this session | this record only |
| Commit | isolated benchmark-docs commit; no source or dependency change |

No reset, clean, stash, rebase, amend, force, push, tag or publish was
performed. No MINK compiler, runtime or stdlib file was modified.

## 9. Limitations and consequences

- W3 cannot be marked complete, and Sessions 155-157 must not start: the
  readiness audit requires all five workloads green.
- The 100/200/500-row MINK observation above is **not** a performance result
  and must not be quoted as one.
- The `~220-row` figure in `stdlib/sqlite.mink` and the 500-row failure
  expectation are now in tension with measured behaviour; both must be
  reconciled in the documentation before W3 is scored.
- C#'s available package is 8.0.0, not the newest release; pinning an older
  major is a dependency-burden decision that must be recorded, not hidden.
- Rust `bundled` SQLite has an unresolved C-toolchain question (see §6).

## 10. Next step

Authorize the two dependency restores in §6 (or explicitly change the W3
dependency plan), then:

1. re-run the 500-row MINK probe with the final row shape and settle the
   probe's classification;
2. implement W3 in five languages and gate every case;
3. only then start Session 155 (W4).
