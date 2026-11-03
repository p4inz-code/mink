# Session 166 — `hookbox` implementation

**Date:** 2026-11-03
**Status:** **IMPLEMENTATION GREEN / CONTRACT PARTIAL — 56/56 checks green.**
**Contract:** [`M5_APPLICATION_SELECTION.md`](M5_APPLICATION_SELECTION.md),
section 6 (`hookbox`), frozen in Session 163.
**Starting HEAD:** `bcd3fbd` (`benchmarks: close M5 repoaudit audit`).
**Scope:** implement and verify the second frozen M5 application. No
performance measurement, no MINK source change, no change to `repoaudit`.

## 1. What was built

| Path | Purpose |
| --- | --- |
| `benchmarks/m5/hookbox/app/main.mink` | the complete application (single file, single-exit ownership) |
| `benchmarks/m5/hookbox/fixtures/bodies/*.json` | eight committed request bodies (valid object, nested, array, scalar, invalid, empty, duplicate-key, whitespace-padded) |
| `benchmarks/m5/hookbox/expected/basic/*` | byte-exact event, NDJSON and stdout goldens |
| `benchmarks/m5/hookbox/expected/help.txt` | byte-exact `--help` golden |
| `benchmarks/m5/hookbox/tests/oracle.py` | independent contract oracle (own JSON reader/writer, event/response/stdout formats) |
| `benchmarks/m5/hookbox/tests/driver.py` | raw-TCP contract driver |
| `benchmarks/m5/hookbox/README.md` | usage, build/test, honest limitations |
| `benchmarks/m5/hookbox/.gitattributes` | `* -text`, so event/CRLF bytes survive checkout |

No MINK compiler, runtime or stdlib file was modified. The stdlib modules used
are read-only dependencies: `httpserver`, `json`, `filesystem`, `strings`
(plus `network` through `httpserver`).

## 2. Contract mapping

| # | Frozen requirement | Implementation | Evidence |
| ---: | --- | --- | --- |
| 1 | `--port` required, 1–65535 | manual argv scan; canonical decimal parse | 26 usage cases; `0`, `65536`, `abc`, `-1`, ` 80`, `1_0`, `+80` -> 1 |
| 2 | `--inbox` required, created one level, parent must exist | `prepare_inbox` | missing parent -> 2; created inbox used by every server case |
| 3 | `--max-requests` 1,000,000 max; 0/absent means until killed | argv scan; loop condition | `1000001` -> 1; `0` and absent stay running until killed |
| 4 | `--help` exits 0; unknown/duplicate/missing/extra -> 1 | `print_help`, `usage` | exact help golden; 26 usage cases |
| 5 | Bind `127.0.0.1` only | `http_server_listen("127.0.0.1", port)` | loopback requests only; port-in-use -> 3 |
| 6 | `POST /hooks`: JSON object, ≤8192 bytes, one event, `201 {"seq":N}` | route + `json_parse` + `json_to_str` | basic, nested, compacted and duplicate-key cases |
| 7 | `GET /health`: `200 {"status":"ok","count":N}` | route | empty inbox 0; after 1, 2 and 25 events |
| 8 | `GET /events?limit=N`: last N, NDJSON, ascending, default 10, 1–20 | `parse_limit` + bounded window | default 10, limit 1/20, 25-event window |
| 9 | Anything else 404; wrong method 405 | route precedence | 8 unknown targets; 6 wrong-method combinations |
| 10 | Event file `<seq:06d>.json`, sequence 1 or max+1 | `event_name`, `seq_from_name`, `sort_names` | 25 sequential events; recovery from 3/7 |
| 11 | Exact event bytes `{"seq":N,"body":<compact>}\n` | write path | oracle + committed goldens; one LF, no CR |
| 12 | No timestamps/headers/client data | event construction holds only seq + body | byte-identical repeated runs |
| 13 | Write failure -> 500 then exit 4 | write/cleanup path | reproduced by removing the inbox after startup |
| 14 | Missing/uncreatable inbox -> exit 2 | `prepare_inbox` | parent missing, file-instead-of-dir, blocked probe |
| 15 | stdout `listening port=<n>` / `served=<n>`, CRLF | `main` | exact byte assertions; two CRLF lines, no bare LF |
| 16 | Diagnostics `hookbox: <label>` on stderr, no stdout | `fail_msg`, `fail_path` | every exit-1/2/3/4 case asserts empty stdout + prefix |
| 17 | Exit codes 0–4 | `main` | 0, 1, 2, 3, 4 all exercised |
| 18 | Deterministic output | sorted recovery; no time/address data | three identical runs byte-identical; 3 full suite runs |
| 19 | Single connection at a time, Content-Length only, ≤64 KiB | stdlib server | documented boundary; no concurrency claimed |
| 20 | No listener left after exit | `net_close(listener)` | connect-after-exit fails in every lifecycle case |
| 21 | One standalone executable, no sidecars | PE build | 230,912-byte exe run from a temp dir outside the repo |

## 3. The JSON arena boundary (reproduced, not hidden)

Session 166's investigation finding was re-verified before the application was
written. The probe resets the persistent write pointer
(`rt_mem_store(arena + 8191 * 8, 0)`) before each parse and uses a 256 KiB
arena, exactly like the server:

| Body length | Result |
| ---: | --- |
| 100, 2,000, 4,000, 4,090, 4,095, 4,096 | parse + byte-exact round trip |
| 4,097, 4,100, 6,000, 8,190, 8,192 | refused (0) |
| sequential reuse: 300, 3,500, 300 | parse + round trip each time (reset works) |

The application therefore does exactly this before every `json_parse`:

```mink
rt_mem_store(arena + 8191 * 8, 0);
let v = json_parse(rt_str_concat(body, ""), arena);
```

Consequences, all tested:

- 4,096-byte valid object -> `201` and an exact event file;
- 4,097-byte valid object -> `400`, no event (MINK stdlib limit);
- 8,192-byte valid object -> `400`, no event (the frozen maximum cannot be
  accepted);
- 8,193-byte body -> `413` (the contract's over-limit status).

`stdlib/json.mink` was **not** modified. The contract is **not** weakened: the
application keeps the 8,192-byte limit and returns the contract's malformed-body
behavior for the range the stdlib refuses. This is why the status is
**IMPLEMENTATION GREEN / CONTRACT PARTIAL**, not COMPLETE.

## 4. Ambiguities resolved and recorded

The frozen text leaves a few response details open. The choices below are
documented in `README.md` and pinned by tests:

- JSON responses use `application/json`; `/events` uses
  `application/x-ndjson` (the only content type the contract names).
- Bodies for 400/413/500 are `{"error":"bad request"}`,
  `{"error":"payload too large"}` and `{"error":"storage failure"}`. The
  contract only pins the 404/405 bodies; those are exact-match tested.
- Method is checked before the query string: `GET /hooks?x=1` is 405, not 404.
  Only `/events` accepts a query, and only `limit=<digits>` (leading zeros
  accepted).
- `/health` `count` is the number of stored event files, not `max(seq)`; a
  numbering gap is preserved (recovery test 3, 7 -> count 3, next 8).
- Only exactly eleven-byte `<6 digits>.json` names are events; `1.json`,
  `1234567.json`, `000000.json` and non-`.json` files are ignored.
- A startup probe `<inbox>/.hookbox-probe` is written and removed to prove the
  inbox is writable; a directory on that name is the reproducible unwritable
  case.
- On an event write failure the application removes any partial file, sends
  `500`, writes a `hookbox:` diagnostic and exits 4 without printing `served=`.
  An event-file read failure during `/events` behaves the same way (500 then
  exit 4) and leaves no partial NDJSON body.
- A request beyond the stdlib's 64 KiB read cap is closed without a response.
  The frozen constraints already cap requests at 64 KiB.
- `--port`/`--max-requests` accept canonical digits only (no sign, spaces,
  underscores or leading `+`); `--inbox` rejects a value beginning with `-`.
- A sequence at or above 1,000,000 would produce a seven-digit name the frozen
  six-digit rule cannot represent. It is unreachable in any tested workload and
  is recorded rather than silently changing the naming rule.

## 5. Test evidence

`python benchmarks/m5/hookbox/tests/driver.py` -> **56 passed, 0 failed, 0
skipped**. Three consecutive runs produced the same summary and exit 0.

Groups (all byte-compared unless noted):

- CLI: exact `--help` golden and 26 usage cases covering unknown options,
  duplicates, missing values, stray positionals and every invalid port/max
  form.
- Startup: parent-missing inbox, inbox-as-file, blocked write probe, port
  already in use.
- Endpoints: valid object, nested object, compaction of a whitespace-padded
  body, duplicate-member preservation, array/scalar/invalid/empty/truncated/
  trailing-junk bodies, missing Content-Length, unknown paths, wrong methods,
  malformed limits, empty inbox views.
- JSON boundary: 4,096 / 4,097 / 8,192 / 8,193.
- Lifecycle: `served` counts 404s, max-requests 1, `--max-requests 0` and the
  absent option (killed after verification), exact two-line stdout, listener
  closed.
- Persistence: 25 sequential events with no duplicates, default-limit 10,
  limit 20, recovery from existing 3/7 with junk files present, second run in
  the same inbox continues at 2.
- Determinism: three identical runs produce identical stdout (port
  normalized) and identical event bytes.
- Failures: write failure (inbox removed after startup) -> 500 + exit 4 + no
  partial file; `/events` read failure -> 500 + exit 4; no probe/temp file
  left after a clean run.
- Distribution: copied executable run with a working directory outside the
  repository, plus an absolute inbox path.

Committed goldens (SHA-256):

| Golden | Bytes | SHA-256 |
| --- | ---: | --- |
| `expected/basic/000001.json` | 41 | `d79b8d0e57be81602d0cdf71931be4fbe447425a6593ca0e60ce1be3c7611e00` |
| `expected/basic/000002.json` | 72 | `d45a7755f775f228fe7c8a9e6751136b1256550ef69470b9df283858d4e993de` |
| `expected/basic/events_2.ndjson` | 113 | `b126babd82a81e6297748dd8bb07188ed016ca9ac456da6e64a00334ea9e9b11` |
| `expected/basic/stdout.txt` | 33 | `3ff38127bc67dbc20ae5299e14c63a7670bf1593b5574690f8ddc92868be917f` |
| `expected/help.txt` | 503 | `2045d42f73c0de1610aca74b2a1449a7dcfb2427bf3087371c97742e15e986dc` |

Event goldens contain exactly one LF and no CR; `stdout.txt` and `help.txt`
contain CRLF only. `.gitattributes` sets `* -text` so checkout cannot rewrite
them.

## 6. Standalone evidence

Build:

```bash
target/release/mink.exe build benchmarks/m5/hookbox/app/main.mink
```

Artifact: 230,912 bytes, SHA-256
`6ac524fae3282caa62bdb0138841295480545c147319d5b67d2d750d9e993c77`.

Run from `C:\Users\Admin\AppData\Local\Temp\hookbox_manual_jyTc` (outside the
repository, executable copied beside the working directory):

```text
exit: 0
stdout: b'listening port=56727\r\nserved=1\r\n'
stderr: b''
response: b'HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: 9\r\nConnection: close\r\n\r\n{"seq":1}'
event: b'{"seq":1,"body":{"a":1}}\n'
files: ['000001.json']
```

No repository-relative resource was needed and no sidecar file appeared. The
driver repeats this check automatically, including the "inbox is created in the
working directory, not next to the executable" assertion.

## 7. Defects found while implementing (application, not MINK)

| Defect | Fix |
| --- | --- |
| `prepare_inbox` freed `path` on the error path after assigning it to the state (E-S10 in MINK and a real double-release risk) | single-exit construction: `path` is moved into the returned state exactly once and the caller always releases it |
| `limit=1` was rejected because the parser required eight characters | corrected to seven (`"limit="` + one digit) |
| a fatal `/events` read failure exited 4 with no diagnostic | added `hookbox: cannot read event file` |
| driver raced temp-dir cleanup against a server left running by a failed assertion | active-server registry killed before the directory is removed |

All four were application/test defects. No MINK compiler, runtime or stdlib
defect was found in this session.

## 8. Files and commit

Only `benchmarks/m5/hookbox/**` and this record were staged. Pre-existing
modified/untracked M3 work was untouched. No reset, clean, stash, rebase,
amend, force, push, tag or publish was used. The commit hash is authoritative in
`git log`.

## 9. Final status

**IMPLEMENTATION GREEN / CONTRACT PARTIAL.**

Every frozen requirement is implemented and exercised; the single frozen
requirement that MINK cannot satisfy — accepting a valid JSON object of
4,097–8,192 bytes — is demonstrated with a boundary test and recorded here and
in the application README. Session 167 may now perform the adversarial audit.
