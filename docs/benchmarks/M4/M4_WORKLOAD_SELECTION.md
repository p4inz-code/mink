# M4 — Workload Selection

**Status:** selected in Session 150. W1, W2, W4 and W5 are implemented and
correctness-green in all five languages. W3 is green in MINK, Python and C#;
Rust and Go are blocked on SQLite drivers that are not available locally
(Session 154). No timing, memory or startup measurement has been collected.
**Rule:** every selected workload must have one functional contract that all
five languages can satisfy, a deterministic local fixture, byte-exact golden
output, and an explicit statement of what is and is not comparable.

## 1. Selection principles

1. **Reuse M1 before inventing.** M1 already has ten real applications with
   228 executable cases. Five are strong enough to become M4 workloads; the
   others are excluded or demoted to supporting probes for documented reasons.
2. **Compact but distinct.** Five workloads cover CLI/JSON/files, HTTP/JSON,
   a local relational store, concurrency, and an integrated utility. They do
   not duplicate one another.
3. **One contract, five implementations.** A workload that only MINK or only
   one competitor can express is not scored.
4. **No artificial microbenchmarks.** Each workload is a small but real tool
   with input, output, error paths and a practical purpose.
5. **Correctness before speed.** Every workload has golden output and error
   cases; timing is collected only after all five implementations pass.
6. **Limits are tested, not hidden.** Fixture sizes stay inside MINK's
   documented envelope for the scored run; separate boundary probes record
   where MINK stops working.

## 2. Candidate inventory from M1

| M1 app | Capability | Cases | M4 decision | Reason |
| --- | --- | --- | --- | --- |
| A1 `fileinspect` | File metadata | 20 | Support only | Covered by W5's walk; a metadata-only tool is too thin for a standalone workload. |
| A2 `treeops` | Recursive copy/move/filter | 18 | Support only | Filesystem mutation is covered by W5; separate copy semantics add little comparative evidence. |
| A3 `textgrep` | Regex line selection | 35 | **Excluded** | MINK's regex is a documented subset (no captures, inline flags or Unicode classes); a scored comparison would measure feature mismatch, not developer experience. |
| B `csvdata` | CSV select/sort/aggregate | 34 | Support / part of W5 | Standalone CSV is excluded because Rust/C# would need an extra CSV package or a hand-written writer; W5 uses CSV as one component. |
| C `jsontool` | JSON validate/extract/serialize | 43 | **W1** | Direct, practical, all five have JSON support; MINK's arena limit is tested separately. |
| D `dbshell` | SQL script + transactions | 10 | **W3** | Meaningful local-app category; narrowed to the common SQL subset because MINK's engine is not SQLite-compatible. |
| E `netcheck` | HTTP/HTTPS/TCP/UDP clients | 18 | **W2** (HTTP only) | HTTP is directly comparable; TLS trust-store setup and raw UDP/TCP are excluded from the scored contract. |
| F `jobrun` | Subprocess/env/time | 19 | **Excluded** | MINK's process module is Windows-only with bounded capture; comparing it would test platform semantics, not a shared workload. |
| G `poolrun` | Worker pool + pipeline | 18 | **W4** | Direct implementation contract; runtime panel is partial because Python's GIL and Go/C# scheduling differ from OS threads. |
| H `manifest` | JSON + walk + SHA-256 + CSV | 13 | **W5** | Best integrated utility; exercises multiple capabilities in one realistic tool. |

M1 was run against Python references only. M4 is a new evidence set; the M1
case counts are historical and are not reused as M4 measurements.

## 3. Selected workloads

### W1 — `jsontool`: CLI JSON inspection and transformation

**Source:** reduced from `validation/c_json/main.mink` (267 lines, 43 M1
cases). **Purpose:** load a JSON config/API payload, validate its shape, sum
an integer field over an array of records, and re-serialize it compactly.

| Item | Contract |
| --- | --- |
| CLI | `jsontool <command> <file>`; commands `validate`, `type`, `sum`, `compact` |
| Input | One UTF-8 JSON file; baseline fixtures ≤ 2,800 bytes |
| Success output | Canonical CRLF lines: `ok`; root type name; integer sum; compact JSON |
| Errors | exit 1 usage; 2 missing/non-file; 3 invalid JSON; 4 wrong root shape or missing field; short stderr diagnostic, no stdout |
| Error fixtures | missing file, directory, invalid JSON, trailing garbage, empty file, array where object is required |
| Baseline fixtures | compact object, 50-record array (~2.8 KiB), nested object, scalar roots, escaped string, raw UTF-8 value |
| Correctness oracle | Golden bytes from the specification, independently checked with a Python stdlib script; Python is not the sole judge |
| Dependencies | MINK `json`; Python `json`; Rust `serde_json`; Go `encoding/json`; C# `System.Text.Json` |
| Comparable | Yes, within the documented MINK JSON envelope; library API differences are recorded |
| Not comparable | MINK input above ~3.5 KiB is a separate boundary probe, not a scored case |

**Why it was selected:** JSON is the most common data interchange task in the
candidate set, every language has a reasonable path, and M1 already proved the
full application. W1 is deliberately smaller than M1's seven commands so the
five ports remain reviewable.

**Boundary probe (not scored):** a 4 KiB and 5 KiB document. MINK is expected
to fail with `E-R05` or a refused parse; the exact observed result is recorded
as a limitation, not compared as a performance result.

### W2 — `netcheck`: local HTTP client and JSON response handling

**Source:** narrowed from `validation/e_network/main.mink` (282 lines, 18 M1
cases). **Purpose:** call a local HTTP/1.1 endpoint, report status and body
length, extract one JSON field, and fail cleanly on refused/malformed
responses.

| Item | Contract |
| --- | --- |
| CLI | `netcheck <host> <port> <path>` |
| Fixture server | Harness-owned Python stdlib TCP server on `127.0.0.1`, ephemeral port, deterministic bytes |
| Endpoints | `/ok` (Content-Length), `/chunked` (three chunks), `/notfound` (404), `/badjson`, `/close` (peer closes with no response) |
| Success output | Canonical CRLF: `STATUS <code>`, `BODY <byte-length>`, `VALUE <field>` when the body is valid JSON |
| HTTP 404 | A completed HTTP exchange: print status/body, exit 0; it is not an application failure |
| Errors | exit 1 usage; 2 connect/read failure or completed response with no status; 3 malformed JSON; no stack trace |
| Dependencies | MINK `http` + `json`; Python `http.client`/`urllib` + `json`; Rust `ureq` + `serde_json`; Go `net/http` + `encoding/json`; C# `HttpClient` + `System.Text.Json` |
| Comparable | Yes for HTTP/1.1 framing and application-level error behavior |
| Not comparable | Underlying exception/Winsock text; TLS trust stores; HTTP timeouts (MINK's HTTP client still uses blocking reads; not part of W2) |

**Why it was selected:** HTTP is a core practical capability and M3 just
closed chunked decoding. The local server makes chunked, Content-Length, 404
and refused cases deterministic without internet access.

**Not scored:** HTTPS, redirects, keep-alive, cookies, public hosts, large
bodies, timeouts. These are documented MINK limits or separate concerns.

### W3 — `dbshell`: local relational workload (common SQL subset)

**Source:** reduced from `validation/d_sqlite/main.mink` (166 lines, 10 M1
cases). **Purpose:** create a table, insert rows, query counts/rows with
`WHERE`, update and delete rows, and exercise transactions against an
**in-memory** relational store.

| Item | Contract |
| --- | --- |
| CLI | `dbshell <script>`; each script is a small sequence of SQL statements |
| Store | In-memory for every language (`:memory:` where the library supports it) |
| SQL subset | `CREATE TABLE`, `INSERT`, `SELECT *`, `SELECT COUNT(*)`, `WHERE` with `= != <> < > <= >=` and `IS [NOT] NULL`, `UPDATE`, `DELETE`, `BEGIN`/`COMMIT`/`ROLLBACK` |
| Success output | Canonical CRLF: `OK` / `OK <n>` for statements, integer for `COUNT(*)`, tab-separated rows with `NULL` for null, matching the M1 renderer |
| Errors | exit 1 usage; 2 script missing/not a file; 3 statement failed (print the engine's message and stop) |
| Baseline | 100 rows, four columns; one `INSERT` per row plus 13 non-bulk statements (the 10-12 sketch predates the two-transaction shape) including a committed and a rolled-back transaction |
| Scale probe | 200 rows, plus a one-shot 500-row probe that **passes** at the final row shape (Session 154 §1.2). The earlier "500-row failure" expectation was not reproduced and is corrected here, not hidden |
| Correctness oracle | Golden per-statement output and final row/count checks; Python `sqlite3` is used to cross-check semantics, not as the only oracle |
| Dependencies | MINK `sqlite`; Python `sqlite3`; Rust `rusqlite` (bundled SQLite); Go `database/sql` + `modernc.org/sqlite`; C# `Microsoft.Data.Sqlite` |
| Comparable | **Partial.** The observable common-subset semantics are comparable; MINK is not a SQLite implementation |
| Not comparable | Engine internals, file format, durability/fsync, `ORDER BY`/`GROUP BY`/`JOIN`/`LIMIT`, indexes, query planning, file persistence |

**Why it was selected:** a local database is a standard developer workload,
and the common subset is enough to measure implementation effort, SQL error
handling and transaction correctness.

**Important verified limit:** `stdlib/sqlite.mink` exposes `sql_new`,
`sql_exec`, `sql_rows`, `sql_has_table`, `sql_tables`, `sql_quote`,
`sql_field` and `sql_free`; it has **no durable open/save API**. The database
is an in-memory `Str` blob. W3 therefore does not claim file persistence or
SQLite compatibility. The header documents roughly 220 four-column rows on
the default 1 MiB heap and O(N²) copying per statement; both are recorded as
limits.

### W4 — `poolrun`: worker pool and two-stage pipeline

**Source:** `validation/g_threads/main.mink` (289 lines, 18 M1 cases).
**Purpose:** split deterministic numeric work across workers, accumulate
under a lock, join in order, and run a two-stage pipeline through a shared
buffer.

| Item | Contract |
| --- | --- |
| CLI | `poolrun squares <n> <workers>` / `poolrun pipeline <n> <workers>` |
| Input | `n` positive integer; `workers` positive integer; baseline `n=10000`, workers 1/4/8/16 |
| Success output | Canonical CRLF: worker count, `n`, per-worker subtotals in join order, locked total, joined total, expected value |
| Errors | exit 1 usage (missing/extra/non-numeric/zero/unknown mode/option); exit 3 if a computed total differs from the closed form |
| Correctness oracle | Closed-form sum and deterministic join-order output |
| Dependencies | None beyond each language's standard threading/lock library |
| Comparable | **Partial.** Implementation contract and output are comparable; runtime scaling is not a pure language comparison |
| Not comparable | Python's GIL, Go goroutines (not OS threads), .NET thread-pool scheduling vs MINK/Python/Rust explicit OS threads; scheduling order is not compared, only join order |

**Why it was selected:** concurrency is a meaningful native-software category
and M1 already proved the MINK implementation. The runtime panel is explicitly
secondary to correctness and implementation effort.

### W5 — `manifest`: integrated filesystem + JSON + SHA-256 + CSV utility

**Source:** `validation/h_manifest/main.mink` (412 lines, 13 M1 cases).
**Purpose:** read a JSON config, recursively walk a directory, filter by
extension, hash files with SHA-256, and write a CSV manifest plus summary.

| Item | Contract |
| --- | --- |
| CLI | `manifest <config.json>` |
| Config | JSON object with `root`, `out`, `extensions` (array of strings) |
| Fixture | 100 files in nested directories (3 levels), ASCII path names, a space in one directory, empty/binary/text files, mixed-case extensions |
| Output | Report file `path,size,sha256,ext` with LF records; canonical CRLF summary on stdout: file/byte counts and first path; exact SHA-256 prefix per the M1 contract |
| Errors | 0 ok; 1 usage; 2 config missing/not a file; 3 invalid JSON/wrong shape/missing key/wrong type; 4 root missing or report unwritable; 5 no file matched |
| Scale probe | 400 files, one observation; not a timed sample |
| Correctness oracle | Golden report bytes and stdout; independent Python hashing/walk check |
| Dependencies | MINK `json` + `filesystem` + `hashing` + `csv`; Python `json` + `os` + `hashlib` + `csv`; Rust `serde_json` + `sha2` + `csv`; Go stdlib `encoding/json` + `path/filepath` + `crypto/sha256` + `encoding/csv`; C# `System.Text.Json` + `System.Security.Cryptography` + manual RFC 4180 writer |
| Comparable | Yes for ASCII paths and the defined report; CSV writer/library differences and filesystem error text are recorded |
| Not comparable | Unicode path names (MINK uses ANSI filesystem APIs), long paths, file timestamps/permissions, OS-specific traversal order |

**Why it was selected:** this is the strongest single "real utility" in M1:
it combines configuration, traversal, binary hashing and structured output.
It tests whether an integrated MINK program is simpler, not just whether a
single library call exists.

**Separate Unicode probe (not scored):** create `naïve café.txt` and
`日本語.txt` under the fixture tree, run all implementations, and record MINK's
ANSI path behavior. This is a documented MINK limitation, not a performance
comparison. Implemented in Session 156 as `benchmarks/m4/harness/probe_unicode.py`;
evidence in `results/raw/w5_manifest/unicode-probe.json`.

## 4. Workload coverage matrix

| Capability | W1 | W2 | W3 | W4 | W5 |
| --- | --- | --- | --- | --- | --- |
| CLI arguments / exit codes | ✓ | ✓ | ✓ | ✓ | ✓ |
| File read / traversal | ✓ | — | — | — | ✓ |
| JSON parse / serialize | ✓ | ✓ | — | — | ✓ |
| HTTP/1.1 | — | ✓ | — | — | — |
| SQL / transactions | — | — | ✓ | — | — |
| Threads / locks | — | — | — | ✓ | — |
| SHA-256 / binary data | — | — | — | — | ✓ |
| CSV output | — | — | — | — | ✓ |
| Error paths | ✓ | ✓ | ✓ | ✓ | ✓ |
| Native distribution | ✓ | ✓ | ✓ | ✓ | ✓ |

No selected workload requires a GUI, network server, TLS, package registry,
Unicode path handling or durable database. Those are separate capabilities or
documented limits.

## 5. Exclusions and why they are not scored

| Candidate | Decision | Reason |
| --- | --- | --- |
| A3 regex | Excluded | MINK's regex subset lacks captures, inline flags and Unicode classes; comparing it would measure missing features rather than the same task. |
| Standalone B CSV | Excluded | Python/Go have CSV in the standard library, Rust needs the `csv` crate and C# needs a package or a hand-written writer. W5 keeps CSV as one component with the dependency difference recorded. |
| F process automation | Excluded | MINK's process API is Windows-only with fixed capture windows and 4096-byte stdin; Python/Rust/Go/C# process models are not semantically comparable. |
| HTTPS | Not scored | MINK requires an explicit trust anchor; trust-store and certificate setup differ across languages. M2/M3 already validated MINK HTTPS separately. |
| Raw TCP/UDP | Not scored | W2 covers the practical HTTP client; UDP has no connection-oriented contract and M3 already validated its buffer parity. |
| HTTP timeouts | Not scored | MINK's HTTP client still uses blocking `net_recv`; Session 145 bounded raw sockets, not the HTTP helper. Requiring HTTP timeouts would test a missing helper, not the selected contract. |
| Durable SQLite | Not scored | MINK has no durable open/save API; W3 uses an in-memory common subset. |
| GUI / games / ML | Excluded | No MINK GUI/ML stack; not part of the defined native-tool class. |
| Large-scale stress | Separate probes | M1/M2 already measured allocator, JSON, HTTP and process ceilings. M4 records a few boundary probes but does not turn them into performance rankings. |

## 6. Comparability summary

| Workload | Correctness | Developer effort | Build/distribution | Runtime/resources |
| --- | --- | --- | --- | --- |
| W1 JSON | Direct | Direct | Direct (Python has no native exe) | Likely startup-dominated; mark if median <100 ms |
| W2 HTTP | Direct for HTTP framing | Direct | Direct | Direct for local loopback; network stack differences noted |
| W3 SQL | Partial (common subset) | Direct | Direct | Partial; MINK engine cost and in-memory limit noted |
| W4 concurrency | Direct output | Direct | Direct | Partial; GIL/goroutine/thread-pool differences |
| W5 manifest | Direct for ASCII paths | Direct | Direct | Direct for the defined fixture; boundary probes separate |

"Direct" means the same observable contract is measured. It does **not** mean
the implementations are internally equivalent.

## 7. Fixture and case budget

| Workload | Golden cases | Baseline size | Scale/boundary probe |
| --- | --- | --- | --- |
| W1 | 10 | 50 objects, ~2.8 KiB | 4 KiB / 5 KiB JSON (MINK limit) |
| W2 | 8 | 5 local endpoints | none (deterministic small bodies) |
| W3 | 10 | 100 rows, 4 columns | 200 rows; one-shot 500-row failure |
| W4 | 8 | 10,000 items, 1/4/8/16 workers | 999/7 non-divisible split |
| W5 | 10 | 100 files | 400 files; Unicode path probe |

Total scored cases: **46**. This is deliberately smaller than M1's 228: each
case must be implemented and reviewed five times, and the purpose is
comparison, not exhaustive validation.

## 8. Dependency policy per workload

Dependencies are pinned at implementation time with exact versions and
licenses. "Stdlib" means no package beyond the language's standard library or
in-box framework.

| Workload | MINK | Python 3.11 | Rust 1.97 | Go 1.26 | C# / .NET 10 |
| --- | --- | --- | --- | --- | --- |
| W1 | stdlib `json` | stdlib `json` | `serde_json` | stdlib `encoding/json` | in-box `System.Text.Json` |
| W2 | stdlib `http`, `json` | stdlib `http.client`, `json` | `ureq`, `serde_json` | stdlib `net/http`, `encoding/json` | in-box `HttpClient`, `System.Text.Json` |
| W3 | stdlib `sqlite` (in-memory) | stdlib `sqlite3` (`:memory:`) | `rusqlite` (bundled SQLite) | `modernc.org/sqlite` + `database/sql` | `Microsoft.Data.Sqlite` |
| W4 | stdlib `threads` | stdlib `threading` | stdlib `std::thread` + `Mutex` | stdlib goroutines + `sync` | in-box `Task`/`Thread` + lock |
| W5 | stdlib `json`, `filesystem`, `hashing`, `csv` | stdlib `json`, `os`, `hashlib`, `csv` | `serde_json`, `sha2`; manual CSV writer | stdlib `encoding/json`, `crypto/sha256`, `encoding/csv` | in-box `System.Text.Json`, `SHA256`; manual CSV writer |

The Rust and C# dependency counts are part of the measured dependency burden.
They are not "cheating" and must not be hidden; neither is MINK's bundled
stdlib.

## 9. Acceptance gate for the next session

A workload may enter timing only when:

- all five implementations build from a documented clean command;
- all golden cases pass byte-for-byte for all five;
- the error cases produce the specified exit codes and no stack traces;
- the fixture and oracle are deterministic and committed;
- dependency versions and licenses are recorded;
- no server/process is left running after the run;
- the harness records raw JSONL and a validation report.

If a language cannot satisfy the contract without changing the contract, the
workload is marked not comparable or failed; it is not silently reduced.
