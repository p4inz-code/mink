# Session 180 — `invsync` cold-start application

`invsync` is a small CSV inventory report CLI written from the Session 178
standard-library reference. It exists to test whether MINK's language and
library can be learned and used coherently, not to demonstrate every API.

## What it does

```text
invsync <input.csv>
```

Input is an RFC 4180 subset with a required header and one record per line:

```csv
sku,category,qty,price
bolt,fasteners,40,3
nut,fasteners,2,2
washer,hardware,0,1
screw,hardware,80,1
```

It validates the header, field count, integer parsing, non-negative quantities
and prices, and empty identifiers. Valid rows are aggregated by category with
`Map<Str, Int>`. On success it prints

```text
invsync: ok
rows=4 total_qty=122 total_value=204
```

and writes `report.txt` in the working directory:

```text
inventory report
rows=4 total_qty=122 total_value=204
category=fasteners qty=42
category=hardware qty=80
```

Category order is first-seen input order. `rt_map_keys` returns hash order, so
the report derives order from the input instead.

## Exit codes

| Code | Meaning |
| ---: | --- |
| 0 | Report written |
| 2 | Usage error (zero or more than one argument) |
| 3 | Invalid input (empty file, bad header, bad row, negative number) |
| 4 | Input file cannot be read |
| 5 | `report.txt` cannot be written |

No report is written on a failing path, and stdout is empty on failure. Errors
go to stderr.

## Modules exercised

`filesystem` (read/write/existence), `csv` (row parse/write), `strings` (trim,
split, integer parse), and built-in `Vec<Str>` / `Map<Str, Int>` handles. The
application also follows the ownership rules for consuming user calls,
collection element strings, and map keys.

## Build and run

```bash
# from the repository root (the repository compiler resolves stdlib from cwd)
./target/release/mink.exe build benchmarks/m6/session180/invsync.mink
cd some-disposable-dir
/path/to/invsync.exe good.csv
```

The installed npm compiler also builds it because it carries its own stdlib.

## Verifier

```bash
python benchmarks/m6/session180/verify.py
python benchmarks/m6/session180/verify.py --compiler <path-to-mink>
```

The verifier builds the application once in a temporary directory, then runs
12 cases in separate disposable working directories: normal operation,
header-only input, empty file, wrong field count, non-numeric quantity,
negative quantity, bad header, missing file, usage error, blocked report path,
aggregate/zero/CRLF boundary, and an extra positional argument. It checks exact
stdout, stderr, exit code and `report.txt`, and removes only its own temporary
directory.

## Cold-start findings

Recorded in `docs/benchmarks/M6/M6_CONSOLIDATED_SESSIONS_178_181.md`. The
short version: the reference made the API discoverable, but four issues cost
real time — `mod environment;` cannot be loaded, `process_stdout()` must not be
freed, `rt_map_keys` is hash-ordered, and a `Vec<Str>` accumulated inside
nested `if`/`else` blocks was corrupted (invalid free). The application avoids
all four rather than depending on them.
