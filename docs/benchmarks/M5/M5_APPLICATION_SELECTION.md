# M5 — Application Selection and Contract Freeze

**Status:** **FROZEN** (Session 163, 2026-11-02).
**Scope:** three complete Windows x86_64 CLI applications implemented in MINK.
**Rule:** M5 is an application-engineering evidence stage, not another
five-language benchmark suite. No performance measurement is part of M5.

## 1. Selection question

M4 established correctness, build, runtime, startup, memory and distribution
evidence for five compact workloads. M5 asks whether MINK's demonstrated
advantages survive when the deliverable is a complete, useful application with
real input validation, filesystem or network behavior, configuration, error
handling, tests and a standalone executable.

M5 is **not** intended to prove MINK is faster than every language, and it is
not intended to maximize application size. Each selected application must be
finishable to a production-quality standard within its frozen scope.

## 2. Evidence reviewed

| Source | Evidence used |
| --- | --- |
| M1 `docs/validation/VALIDATION_MILESTONE_1_REPORT.md` and capability matrix | 10 applications, 228 cases; ANSI filesystem, `MAX_LIVE_ALLOCS = 1024`, JSON arena ceiling, process capture windows, in-memory SQLite, no console control handler |
| M2 Sessions 133–138 | hostname/TCP networking, server workloads, 11,500+ echo requests, SQLite-backed service/restart, integrated file/automation workloads, large-scale ceilings |
| M3 Sessions 141–149 | `net_dial` error preservation, socket timeouts, UDP receive parity, HTTP chunked decoding, print atomicity, parser/MIR fixes |
| M4 final report and workload selection | 450/450 correctness; W3 SQL interpreter weakness; W5 Unicode limitation; build/artifact/startup/memory evidence; no composite score |
| M4 `M4_RESULTS_SUMMARY.json` | exact measured runtime/startup/memory/build/artifact values used as context, not re-measured in M5 |
| `stdlib/*.mink` | concrete APIs for `filesystem`, `json`, `csv`, `re`, `hashing`, `zip`, `httpserver`, `process`, `network`, `threads` |
| `tests/*.rs` | existing verification: `httpserver` 10 cases, `zip` 10 cases, `process` suite, JSON/CSV/regex suites |
| `validation/*/main.mink` | working ownership, module, argv, JSON-arena, filesystem-walk and CSV patterns |
| `docs/roadmap/IMPLEMENTATION_ROADMAP.md` | M5 is currently described as a seven-category shootout, with no `M5 NEXT` state to transition |
| repository search | **no `mink-scout` implementation or document exists in this tree**; the selection does not rely on it |

## 3. MINK capability envelope used for selection

The M5 applications must stay inside capabilities that are verified or
explicitly documented. They may not assume:

- Linux, macOS or non-x86_64 Windows;
- Unicode path names or non-ANSI argv (M5 fixtures use ASCII names);
- `MAX_LIVE_ALLOCS` above 1,024 simultaneous allocations;
- JSON input above the documented ~3.5 KiB arena budget;
- SQLite durability, file-backed databases or SQL beyond the common subset;
- HTTP keep-alive, chunked **requests**, TLS termination, or a console
  control handler;
- ZIP64, encryption, archive timestamps/permissions, or a streaming archive
  path;
- streaming child-process I/O (the process module captures bounded windows).

Each application must also have a deterministic or objectively testable
observable contract. The M5 applications deliberately avoid the unsupported
areas rather than building a failure demo.

## 4. Candidate evaluation

The candidate categories in the Session 163 brief were investigated against
the evidence above. This is a qualitative engineering screen, not a numeric
score.

| Candidate | Real usefulness | MINK coverage | Complexity | Testability | Windows relevance | Dependency burden | M4 duplication | Decision |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Project inventory / policy auditor | High: pre-commit/release hygiene | filesystem, JSON config, CSV, regex, diagnostics | Medium | High: deterministic reports | High | None beyond stdlib | Uses W5/W1 components, but adds policy workflow and multi-report output | **SELECTED — `repoaudit`** |
| Local webhook inbox | High: integration testing | HTTP server, JSON validation, filesystem, request routing | Medium | High: loopback + file artifacts | High | stdlib `httpserver` (pre-existing untracked module) | None: M4 W2 is client-only | **SELECTED — `hookbox`** |
| Release packager / archive verifier | High: reproducible release artifacts | ZIP create/read/extract, hashing, filesystem, CSV, JSON config | Medium | High: byte-identical zip + manifest | High | stdlib `zip`/`zlib` | Adds archive format/determinism; not a W5 duplicate | **SELECTED — `releasepack`** |
| Task/process orchestrator | High for build automation | process, filesystem, JSON | Medium-high | Medium: external commands vary | High | stdlib `process` | M4 excluded process; would add new evidence | **REJECTED for M5**: bounded capture, Windows-only process model and environment-dependent tests make a "complete" claim fragile; better as a later process-focused stage |
| Structured log query | Medium-high | filesystem, regex subset, CSV/JSON | Medium | High | Medium | stdlib | Overlaps `repoaudit` text/report profile | Rejected: not materially distinct from Application 1 |
| Dependency/manifest auditor | High | filesystem, JSON, regex | High | Medium | High | would need TOML/XML parsers | Overlaps M4 W5 | Rejected: no trusted TOML/XML parser; heuristics would weaken correctness |
| SQLite local service | Medium | filesystem, sqlite | Medium | High | Medium | stdlib `sqlite` | Duplicates W3 | Rejected: no durable DB, same in-memory subset |
| Asset/content validator | Medium | filesystem, hashing, CSV | Low-medium | High | Medium | stdlib | Overlaps W5 hashing | Rejected: too close to W5 and mostly a wrapper |
| HTTP endpoint monitor | Medium | http client, JSON | Low | High | Medium | stdlib | Duplicates W2 client | Rejected: adds no new capability profile |
| File watcher / daemon | High conceptually | none for notifications | High | Low | High | unsupported | None | Rejected: no watch or console-control API |
| Static-site builder | High | filesystem only | High | Medium | Medium | missing template/markdown layer | None | Rejected: would require unsupported language/tooling work |

The three selected applications cover three materially different profiles:

1. **offline project analysis** — traversal, text/regex analysis, policy,
   CSV/JSON reporting;
2. **local network service** — HTTP request parsing, routing, JSON validation,
   per-event persistence, lifecycle;
3. **release artifact engineering** — archive creation/verification,
   deterministic bytes, path safety, hashing.

No selected application is a JSON-only utility, a simple file hasher, a
synthetic thread benchmark, a database benchmark, trivial CRUD or a GUI.

## 5. Frozen Application 1 — `repoaudit`

### 1. Name

`repoaudit`.

### 2. Purpose

Audit an ASCII Windows project tree against a JSON policy: inventory included
source/text files, count physical lines and configured markers, apply size and
marker limits, and write deterministic CSV/JSON reports with a nonzero exit
when policy violations exist.

### 3. User story

A developer wants a fast, local pre-commit/release check that reports file
inventory, size, line and marker counts, and flags oversized files or files
with too many `TODO`/`FIXME` markers — without a package manager, network or
external tool.

### 4. CLI interface

```text
repoaudit <config.json>
repoaudit --help
```

- Exactly one positional config path, or `--help` (exit 0).
- Any other option, extra argument or missing argument is usage exit 1.
- The config's `out` directory must already exist.

### 5. Input/output contract

Config is a UTF-8 JSON object with exactly these required keys:

```json
{
  "root": "tree",
  "out": "out",
  "include_extensions": [".mink", ".md", ".txt"],
  "exclude_dirs": ["skip", "target"],
  "markers": ["TODO", "FIXME"],
  "max_file_bytes": 65536,
  "max_markers": 5
}
```

Validation rules:

- `root`, `out` are non-empty strings; paths are used relative to the current
  working directory.
- `include_extensions` has 1–8 entries, each matching `^\.[A-Za-z0-9]{1,8}$`;
  matching is case-insensitive.
- `exclude_dirs` has 0–8 entries, each a non-empty base name (no slash); a
  directory is excluded when its base name matches case-insensitively at any
  depth.
- `markers` has 1–4 entries, each matching `^[A-Za-z0-9_]{1,16}$`;
  matching is case-sensitive.
- `max_file_bytes` is an integer 1–1,048,576.
- `max_markers` is an integer 0–1,000.

Outputs (all overwritten atomically enough for this scope; ASCII paths):

```text
<out>/inventory.csv   path,bytes,lines,binary,todo,fixme
<out>/violations.csv  path,rule,detail
<out>/summary.json    compact object, fixed key order
```

- Inventory rows are sorted by path in byte order. Paths use `/` separators,
  relative to `root`.
- `lines`: 0 for an empty text file; otherwise `LF count + 1` when the last
  byte is not LF. Binary or size-skipped files use `-1`.
- `binary`: 1 when a NUL byte appears in the first 4,096 bytes; such files do
  not contribute marker counts.
- Marker counts are non-overlapping case-sensitive occurrences.
- Files larger than `max_file_bytes` are not read: `lines=-1`, `binary=0`,
  markers 0, and a `file_too_large` violation. `detail` is `limit=<n>`.
- A text file with `todo+fixme > max_markers` produces `too_many_markers`
  with `detail=count=<n>`.
- `summary.json` has exactly:
  `{"files":N,"bytes":N,"lines":N,"binary":N,"todo":N,"fixme":N,"violations":N}`.
- stdout is one CRLF line:
  `files=N bytes=N lines=N todo=N fixme=N violations=N`.
- stderr is reserved for diagnostics, prefixed `repoaudit: `, and no stdout is
  written before a fatal error.

### 6. Error/exit-code contract

| Exit | Meaning |
| ---: | --- |
| 0 | Audit completed; no violations |
| 1 | Usage error, unknown option, missing/extra argument |
| 2 | Config file missing or not a regular file |
| 3 | Config is not valid JSON, is not an object, or fails schema/type/range validation |
| 4 | `root` is missing or not a directory |
| 5 | `out` is missing/not a directory, or a report cannot be written |
| 6 | Reports written, but one or more policy violations were found |
| 7 | A directory/file could not be read during the scan |

### 7. Required features

- Manual argv parsing and `--help`.
- JSON config parse and full schema validation.
- Recursive ASCII directory walk with include-extension and exclude-directory
  filtering, deterministic byte-order sorting and `/` path normalization.
- Binary detection, line counting, marker counting and size policy.
- Three deterministic report artifacts plus the stdout summary.
- Explicit handling of every error class above; no stack traces.
- Repeated-run determinism and no leaked strings at exit.

### 8. Explicit non-features

- No `.gitignore` parsing, glob matching, symlink policy or hidden-file policy.
- No Unicode path guarantee; fixtures are ASCII. A separate unscored probe may
  record ANSI behavior.
- No hashing, archive creation, network, process execution, watch mode,
  auto-fix, HTML report or Markdown report.
- No configurable regex patterns; markers are literal ASCII tokens.
- No parallel scanning.

### 9. Test strategy

- Fixture tree: nested directories; an excluded `target`/`skip` directory;
  empty, CRLF, LF-no-final-newline, binary (NUL), oversized, extension-less,
  mixed-case-extension, and marker-heavy files; a path with a space.
- Independent Python oracle computes inventory/violations/summary and compares
  bytes; golden reports are committed.
- Positive, negative, boundary and malformed-config cases; missing root/out;
  unwritable out; unreadable file if reproducible; no-match tree.
- Three repeated runs compare report hashes; a separate ASCII/ANSI probe is
  unscored.
- Build from the documented command and run the executable from a directory
  outside the source tree.

### 10. Expected artifact/distribution model

One standalone `repoaudit.exe`, no sidecar files, no runtime dependency beyond
Windows. Reports are produced at run time in the user-selected `out` directory.

### 11. MINK-specific constraints

ASCII paths/argv; JSON config and summary stay within the documented arena;
fixture budget ≤ 40 files so the scan stays well below `MAX_LIVE_ALLOCS`; whole
files are read only when ≤ `max_file_bytes`; recursive depth is bounded by the
fixture (≤ 5). No MINK source modification is planned.

### 12. Equivalent comparison implementations

None. The Python verifier is an independent test oracle, not a shipped port.
M4 already provides five-language evidence; adding another full five-language
port set would duplicate M4 and is explicitly outside M5's purpose.

### 13. Definition of DONE

- Frozen CLI, config schema, outputs, exit codes and non-features implemented.
- All positive/negative/boundary tests green against the Python oracle.
- Three repeated runs produce byte-identical reports.
- Standalone executable builds and runs outside the source tree.
- Limitations and friction recorded in `M5_SESSION_164.md`.
- No MINK compiler/runtime/stdlib change is required; if one is, it is a
  separately documented defect, not a silent workaround.

## 6. Frozen Application 2 — `hookbox`

### 1. Name

`hookbox`.

### 2. Purpose

A local, loopback-only HTTP webhook inbox for development: accept JSON
webhook POSTs, validate and persist each event as its own numbered file, expose
health and recent-event endpoints, and shut down after a configured number of
requests for automated testing.

### 3. User story

A developer integrating a webhook sender needs a local endpoint that records
requests without a hosted service or database, and can inspect the most recent
events from the command line or another local tool.

### 4. CLI interface

```text
hookbox --port <n> --inbox <dir> [--max-requests <n>]
hookbox --help
```

- `--port` is required, integer 1–65535.
- `--inbox` is required; the directory is created if missing (one level; its
  parent must exist).
- `--max-requests` is optional, integer 1–1,000,000; `0`/absent means run
  until killed. The limit counts every accepted HTTP request.
- `--help` exits 0; any unknown option, duplicate option, missing value or
  extra positional is usage exit 1.

### 5. Input/output contract

Bind `127.0.0.1` only.

Endpoints:

| Method/path | Behavior |
| --- | --- |
| `POST /hooks` | Body must be a JSON **object**, ≤ 8,192 bytes. Writes one event file, returns `201` with `{"seq":N}`. |
| `GET /health` | Returns `200` with `{"status":"ok","count":N}`. |
| `GET /events?limit=N` | Returns up to the last N events as `application/x-ndjson`, ascending sequence. Default 10; valid 1–20. |
| Anything else | `404` with `{"error":"not found"}`; wrong method on a known path is `405` with `{"error":"method not allowed"}`. |

Event persistence:

- `<inbox>/<seq:06d>.json`, sequence starting at 1 for an empty inbox, or at
  `max(existing six-digit name)+1` when the inbox already contains numbered
  event files.
- Each file contains exactly `{"seq":N,"body":<compact JSON>}\n` (LF), where
  the body is the parsed object re-serialized compactly.
- No timestamps, client address, headers or request id are stored, so output
  is deterministic.
- A write failure returns `500` and exits 4; a missing/uncreatable inbox is a
  startup error 2.
- On startup stdout is `listening port=<n>`; on clean `--max-requests`
  completion stdout is `served=<n>`. Both CRLF.
- Diagnostics are `hookbox: <label>` on stderr with no stdout.

### 6. Error/exit-code contract

| Exit | Meaning |
| ---: | --- |
| 0 | Clean shutdown after `--max-requests` (or an orderly internal stop) |
| 1 | Usage/option/argument error |
| 2 | Inbox directory cannot be created, is not a directory, or is not writable |
| 3 | `127.0.0.1:<port>` cannot be bound/listened on |
| 4 | Server I/O or event-file write failure |

Per-request responses use 400 (malformed/non-object JSON or bad limit), 404,
405, 413 (body over 8 KiB) and 500 (storage failure); these are HTTP statuses,
not additional process exit codes.

### 7. Required features

- Manual argv parsing and `--help`.
- Loopback HTTP server lifecycle: bind, accept, read a complete Content-Length
  request, route, respond, close.
- JSON object validation and compact re-serialization with the stdlib parser.
- Deterministic numbered event files and sequence recovery across restarts.
- Health and recent-events endpoints with bounded response size.
- Explicit HTTP error responses and process exit codes.
- Clean termination after `--max-requests`; no orphan listener.

### 8. Explicit non-features

- No TLS, authentication, authorization, CORS, redirects, cookies, keep-alive,
  chunked requests, HTTP/2 or public-interface binding.
- No concurrent request handling: one connection is served at a time.
- No timestamps, headers/body size history, search, rotation, retention policy
  or database.
- No graceful Ctrl+C handler; the platform has no MINK console-control API.
  Termination by the OS is not claimed as graceful.
- No Unicode path guarantee; inbox paths are ASCII.

### 9. Test strategy

- Python loopback client sends exact requests; server runs with
  `--max-requests` so every test terminates deterministically.
- Cases: valid object, nested object, array/scalar/invalid/empty body,
  oversized body, missing Content-Length, unknown path, wrong method, bad
  limit, zero/limit boundary, health after events, `GET /events` ordering,
  restart sequence recovery, port already in use, missing inbox parent,
  unwritable inbox, repeated runs.
- Verify exact HTTP status/headers/body where deterministic, event file bytes,
  sequence numbers, stdout startup/served lines and exit code.
- No timing or concurrent-load claim.

### 10. Expected artifact/distribution model

One standalone `hookbox.exe`, no sidecar files. Event files are the only
runtime data and are written to the user-selected inbox directory.

### 11. MINK-specific constraints

Depends on the existing `stdlib/httpserver.mink` and `stdlib/json.mink`.
`httpserver.mink` is a **pre-existing untracked module** with 10 passing
library tests; it must not be modified by M5. If a defect blocks the contract,
the result is recorded as BLOCKED/PARTIAL rather than worked around silently.
Requests are Content-Length only and ≤ 64 KiB; event bodies are ≤ 8 KiB; the
server handles one connection at a time. ASCII paths only.

### 12. Equivalent comparison implementations

None; the Python client/verifier is a test harness, not a shipped equivalent.
M4 W2 already covers the HTTP-client comparison.

### 13. Definition of DONE

- Frozen endpoints, persistence format, limits, exit codes and non-features
  implemented.
- All positive/negative/boundary/restart tests green.
- Event files and responses are deterministic; no timestamps.
- Standalone executable builds and runs outside the source tree.
- No listener/process remains after each test.
- Limitations (single-threaded, no Ctrl+C, Content-Length only, untracked
  server module) are documented in `M5_SESSION_165.md`.

## 7. Frozen Application 3 — `releasepack`

### 1. Name

`releasepack`.

### 2. Purpose

Build, verify and extract deterministic Windows release ZIP archives from a
JSON configuration, with a sorted CSV manifest of sizes and SHA-256 digests and
safe path handling.

### 3. User story

A developer needs a reproducible release archive and checksum manifest without
installing an external zip tool, and wants to verify an archive against its
manifest or safely extract it before shipping.

### 4. CLI interface

```text
releasepack build <config.json>
releasepack verify <archive.zip> <manifest.csv>
releasepack extract <archive.zip> <dest-dir>
releasepack --help
```

- Exactly one of the three commands with the required positional count.
- `--help` exits 0; unknown command/option or wrong arity exits 1.

### 5. Input/output contract

Build config is a UTF-8 JSON object with exactly these required keys:

```json
{
  "root": "tree",
  "archive": "out/release.zip",
  "manifest": "out/manifest.csv",
  "include_extensions": [".txt", ".json"],
  "exclude_dirs": ["out", "skip"]
}
```

- `root` must be an existing directory; all paths are relative to the current
  working directory.
- `archive` and `manifest` parents must already exist; they must not be inside
  `root` (prevents self-inclusion).
- `include_extensions` is 1–8 entries matching `^\.[A-Za-z0-9]{1,8}$`,
  case-insensitive.
- `exclude_dirs` is 0–8 non-empty base names, matched case-insensitively at any
  depth.
- Compression is fixed at level 6; timestamps are the stdlib's fixed DOS epoch
  (1980-01-01), so the archive is byte-deterministic for identical inputs.
- Entries are sorted by relative path in byte order and use `/` separators.

Build outputs:

- ZIP archive at `archive`.
- CSV manifest at `manifest` with header `path,size,sha256` and LF records;
  `sha256` is the full 64 lowercase hex characters.
- stdout: `files=N bytes=N archive_bytes=N`, CRLF.
- Empty match set is exit 5 and writes no archive/manifest.

Verify:

- Reads the CSV manifest and the ZIP central directory; requires the same
  file set, sizes and SHA-256 values.
- stdout on success: `verified files=N`, CRLF.
- Mismatches print sorted `MISMATCH <path> <reason>` lines and exit 6.

Extract:

- Creates `dest-dir` if missing (one level; parent must exist).
- Uses the stdlib's validated extraction path; unsafe/absolute/traversal names
  fail with exit 6 and no partial tree is claimed.
- stdout: `extracted files=N`, CRLF.

### 6. Error/exit-code contract

| Exit | Meaning |
| ---: | --- |
| 0 | Command completed successfully |
| 1 | Usage/command/arity error |
| 2 | Config, archive or manifest missing / not a regular file |
| 3 | Config or manifest is not valid JSON/CSV or fails schema/type validation |
| 4 | Root/output/destination filesystem failure, or output inside root |
| 5 | Build matched no files |
| 6 | Corrupt/unsupported/unsafe archive, or manifest/archive mismatch |

All diagnostics are `releasepack: <label>` on stderr with no stdout before a
fatal error.

### 7. Required features

- Manual argv parsing and `--help`.
- JSON config parse/schema validation.
- Recursive ASCII walk with extension/exclude filtering and deterministic
  sorting.
- SHA-256 of every selected file.
- Deterministic ZIP creation with sorted entries and fixed metadata.
- CSV manifest creation with RFC-4180 escaping.
- ZIP verification against the manifest (names, sizes, hashes).
- Safe extraction using validated entry paths.
- Explicit error handling and no stack traces.

### 8. Explicit non-features

- No signing, encryption, ZIP64, multi-disk archives, comments, symlinks,
  permissions, timestamps, incremental archives, compression-level option or
  remote storage.
- No archive update-in-place; build creates a new archive.
- No JSON manifest; the manifest is CSV.
- No Unicode path guarantee; fixtures are ASCII. A separate unscored ANSI
  probe may record behavior.
- No hashing of directories; only file content.

### 9. Test strategy

- Fixture tree: nested directories, empty file, binary file, mixed-case
  extensions, excluded directory, a path with a space/comma, and enough entries
  to exercise sorting.
- Build twice and compare SHA-256 of both archive and manifest for
  byte-identical output; Python `zipfile` independently lists entries, checks
  sizes/CRCs and hashes; extract and compare the tree byte-for-byte.
- Negative cases: missing root/config, output inside root, empty match set,
  corrupt archive, truncated archive, unsafe traversal archive, manifest
  mismatch, missing manifest entry, extra archive entry, unwritable output.
- Repeated runs and execution from outside the source tree.

### 10. Expected artifact/distribution model

One standalone `releasepack.exe`, no sidecar files. The produced ZIP and CSV
manifest are release artifacts, not runtime dependencies.

### 11. MINK-specific constraints

ZIP is stored/DEFLATE only, no ZIP64, and archives must stay within the
documented heap envelope (target fixtures ≤ 200 KiB uncompressed). Entry names
are ASCII relative paths; timestamps are fixed. JSON config stays within the
arena budget. No MINK source modification is planned.

### 12. Equivalent comparison implementations

None. Python's `zipfile`/`hashlib` are used only as independent verification
oracles. M4 already provides the cross-language build/runtime comparison; M5
does not add another language port.

### 13. Definition of DONE

- Frozen CLI, config schema, archive/manifest format, exit codes and
  non-features implemented.
- Build is byte-deterministic across repeated runs and verified by Python.
- Verify detects corrupt, unsafe and mismatched archives with exit 6.
- Extract round-trips the fixture tree and rejects unsafe paths.
- Standalone executable builds and runs outside the source tree.
- Limitations and friction recorded in `M5_SESSION_166.md`.

## 8. Coverage added beyond M4

| Capability / evidence | M4 W1–W5 | M5 `repoaudit` | M5 `hookbox` | M5 `releasepack` |
| --- | --- | --- | --- | --- |
| JSON config + schema validation | W1/W5 input only | yes, full policy schema | CLI/config-free; JSON body validation | yes, build config validation |
| Recursive ASCII filesystem walk | W5 | yes, with excludes/sorting | inbox directory scan | yes, with excludes/sorting |
| Text/regex analysis | W1 JSON, W5 hashing | **yes, line/marker analysis** | no | no |
| CSV output | W5 | inventory + violations | no | manifest |
| JSON output | W1 | summary.json | HTTP JSON responses | no |
| HTTP **server** | no (W2 is client) | no | **yes: parse/route/respond** | no |
| Per-event persistence | no | reports only | **yes, numbered files** | archive output |
| ZIP create/read/extract | no | no | no | **yes** |
| SHA-256 | W5 | no | no | yes |
| Deterministic byte artifact | W5 report | reports | event/body format | **ZIP + manifest** |
| Policy/violation exit semantics | partial (W5 exit 5) | **yes, exit 6** | HTTP 4xx/5xx | verify exit 6 |
| Standalone Windows exe | yes | required | required | required |

No application requires a GUI, Linux/macOS, Unicode paths, durable SQLite,
TLS, process streaming or a package registry. None is a synthetic benchmark.

## 9. Cross-M5 test and oracle strategy

- Each application lives under `benchmarks/m5/<name>/` with:
  `main.mink` (and modules where justified), `fixtures/`, `verify.py`,
  `expected/` golden files where byte comparison is practical, and a README.
- `verify.py` is Python 3.11 standard library only. It is an **oracle and
  driver**, never a shipped competitor port.
- Deterministic fixtures are generated by a committed `make_fixtures.py` and
  assert their own counts.
- Every application is built with the existing `mink build` command and run
  from outside its source directory at least once.
- No timing, startup, memory or clean-build measurement is part of M5.
- Any ANSI/Unicode probe is unscored and reported separately, as in M4 W5.

## 10. Status vocabulary and M5 gate

| Status | Meaning |
| --- | --- |
| **GREEN** | Frozen contract implemented; positive/negative/boundary tests green; standalone artifact works outside the source tree |
| **PARTIAL** | Useful implementation exists but one or more frozen requirements are missing or unverified; limitations documented |
| **BLOCKED** | A MINK capability gap prevents a required part; no fake support and no contract weakening |
| **FAILED** | The implementation does not meet the contract for application-level or correctness reasons |

M5 is complete only when all three applications are GREEN, tests are green,
distribution is verified, limitations are documented, and the Session 167
audit passes. A PARTIAL/BLOCKED result is evidence, not something to relabel.

## 11. Risks and controls

| Risk | Control |
| --- | --- |
| `httpserver.mink` is pre-existing untracked work | use it read-only; if defective, classify honestly; never modify M3 work |
| JSON arena limits | keep configs/bodies/manifests small; reject oversized input before parsing |
| `MAX_LIVE_ALLOCS` | stream/scan files one at a time; cap fixtures and response limits |
| ANSI paths | ASCII fixtures; Unicode probe separate and unscored |
| ZIP heap cost | target ≤ 200 KiB uncompressed; fail loudly on archive errors |
| No Ctrl+C handler | `--max-requests` and orderly completion for tests; document OS termination |
| Scope creep | frozen non-features; no cross-language ports; no performance work |
| Hidden MINK defect | reproduce minimally; record; do not silently work around; fix only with a separate regression-backed change if permitted |

## 12. Roadmap note

The canonical roadmap currently describes `M5 — Competitive Workload
Shootout` and has no `M5 NEXT` status. It is also a pre-existing modified file
and M4's status line in it is historical. Session 163 therefore **does not
rewrite the roadmap**: there is no exact `M5 NEXT → M5 IN PROGRESS` transition
to apply. The M5 execution scope is frozen in this document and in
`M5_SESSION_163.md`; a roadmap editorial correction is left to a dedicated
roadmap session.
