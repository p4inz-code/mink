# Session 167 — `hookbox` adversarial audit

**Date:** 2026-11-03
**Implementation commit:** `75c3b3b` (`benchmarks: implement and verify M5 hookbox`).
**Status:** **APPLICATION GREEN / CONTRACT PARTIAL.**
**Contract:** [`M5_APPLICATION_SELECTION.md`](M5_APPLICATION_SELECTION.md),
section 6, frozen in Session 163.
**Scope:** adversarial closure audit. No performance measurement, no MINK
source change, no `repoaudit` change.

The audit is not a replay of the happy path. Every frozen clause is mapped to
an executable check, the JSON boundary is probed explicitly, and deterministic
cases were repeated three times with a full-suite comparison.

## 1. Audit method

- Read the frozen section 6 line by line and enumerated every CLI, HTTP,
  filesystem, output and lifecycle clause.
- Added adversarial cases the Session 166 suite did not have: a request split
  across three TCP writes, a declared `Content-Length` over 8 KiB with a short
  body, a rejected body followed by a valid one, a 25-event startup window with
  ignored names present, and additional wrong methods (`OPTIONS`, `PATCH`).
- Ran the complete suite three times and compared the summaries and the
  per-case results.
- Re-ran the standalone executable from a temporary directory outside the
  repository.
- Manually inspected every ownership/error exit in `main.mink`.

## 2. CLI audit

| Check | Expected | Observed |
| --- | --- | --- |
| `--help` alone | exit 0, exact golden, stderr empty | PASS |
| no arguments | 1, stderr-only, no inbox created | PASS |
| unknown option `--wat` | 1 | PASS |
| `-h` | 1 (only `--help` is defined) | PASS |
| `--help` plus an argument | 1 | PASS |
| missing `--port` | 1 | PASS |
| missing `--inbox` | 1 | PASS |
| `--port`/`--inbox`/`--max-requests` without a value | 1 | PASS |
| duplicate `--port`/`--inbox`/`--max-requests` | 1 | PASS |
| stray positional | 1 | PASS |
| port 0, 65536, `abc`, `-1`, `" 80"`, `1_0`, `+80` | 1 | PASS |
| max-requests 1,000,001, `abc`, `-5`, `1.0` | 1 | PASS |
| inbox value beginning with `-` | 1 | PASS |
| `--max-requests 0` | accepted, runs until killed | PASS |
| `--max-requests` absent | accepted, runs until killed | PASS |
| port already in use | 3, stdout empty | PASS |
| empty inbox value | 1 | PASS |

Usage errors write `hookbox: <label>` then the usage line to stderr and never
create the inbox. Every one of these cases asserts empty stdout.

## 3. HTTP audit

| Area | Cases | Result |
| --- | --- | --- |
| `POST /hooks` valid object | compact, nested, whitespace-padded, duplicate keys | 201, exact event bytes, exact `{"seq":N}` |
| non-object JSON | array, scalar string | 400, no event |
| malformed JSON | truncated object, trailing junk, empty body, whitespace-only, missing `Content-Length` | 400, no event |
| size boundary | 4,096 / 4,097 / 8,192 / 8,193, declared 9,000 | 201 / 400 / 400 / 413 / 413 |
| request reassembly | one request over three TCP writes | 201, event bytes correct |
| rejected request | then a valid POST | valid POST gets seq 1, no sequence consumed |
| `GET /health` | empty inbox, 1, 2, 25, 26 events | 200, exact count |
| `GET /events` | empty, default 10, limit 1, limit 20, limit 05, 25 events, 25 pre-existing | exact NDJSON, ascending |
| bad limits | 0, 21, `abc`, empty, `1.5`, `-1`, ` 5`, extra parameter | 400 |
| unknown targets | `/nope`, `/`, `/hooks/`, `/health/`, `/events/`, `/health?x=1`, `POST /hooks?x=1`, `/events?limit=1&x=1` | 404 (last one 400) |
| wrong methods | GET/PUT/OPTIONS on /hooks; POST/DELETE/PATCH on /health; POST/HEAD on /events | 405 with the frozen body |
| content types | JSON responses / NDJSON | `application/json` / `application/x-ndjson` |
| response framing | status line, reason, `Content-Length`, `Connection: close` | exact |

Method precedence is explicit and tested: a wrong method on a known path is
405 before the query string is considered.

## 4. Filesystem audit

| Check | Result |
| --- | --- |
| inbox created one level below an existing parent | PASS |
| missing parent | exit 2, no parent created |
| inbox path is a file | exit 2 |
| unwritable inbox (directory on the probe name) | exit 2 |
| event naming | exactly `<6 digits>.json`, 25 consecutive events, no duplicate |
| sequence recovery | existing 3 and 7 -> next 8, count 3 |
| ignored names | `000000.json`, `1234567.json`, `000099.JSON`, `notes.txt`, `1.json` unchanged and not events |
| startup window | 25 existing + 1 new -> `/events?limit=20` returns 7..26 |
| event bytes | `{"seq":N,"body":<compact>}\n`, one LF, no CR |
| event JSON | compact re-serialization, member order and duplicate keys preserved |
| no stale events | rejected bodies and failures create no file |
| write failure | 500, exit 4, moved inbox left empty, no partial file |
| read failure during `/events` | 500, exit 4, no partial NDJSON |
| cleanup | startup probe removed after success and after failure |
| second run in one inbox | continues at 2, does not overwrite 1 |
| absolute inbox outside the working directory | created and used |

## 5. Output audit

- `listening port=<n>` is printed only after a successful bind, so a bind
  failure leaves stdout empty.
- `served=<n>` is printed only on a clean `--max-requests` completion; the
  write-failure and read-failure cases assert it is absent and stdout contains
  only the listening line.
- stdout is exactly two CRLF lines on success, with no bare LF.
- diagnostics are `hookbox: <label>` on stderr with CRLF, never on stdout.
- exit codes actually exercised in this audit: **0, 1, 2, 3, 4**.

## 6. Reliability audit

- 25 sequential events: sequential names, no duplicates, no gaps.
- A request rejected with 400 does not consume a sequence number.
- `max-requests` counts every accepted HTTP request, including 404s.
- The listener is closed before the process exits; a connect after exit is
  refused in every lifecycle case.
- No probe, temp or `.tmp` file remains after a clean run.
- Three full suite runs: identical 60/60 summaries, no flakes.
- The event-file write path removes a partial file before exiting 4.

## 7. JSON boundary audit (explicit)

| Body | Observed | Contract expectation | Classification |
| ---: | --- | --- | --- |
| 100–4,096-byte valid object | 201 + exact event | accepted | supported |
| 4,097-byte valid object | 400, no event | accepted (≤ 8,192) | **MINK stdlib limitation** |
| 8,192-byte valid object | 400, no event | accepted (≤ 8,192) | **MINK stdlib limitation** |
| 8,193-byte body | 413, no event | 413 | supported |

The 4,097–8,192 range is a genuine `stdlib/json.mink` arena ceiling. The
application resets the required write pointer before every parse
(`rt_mem_store(arena + 8191*8, 0)`), keeps the frozen 8,192-byte limit, and
does not modify the stdlib. It is recorded here, in the application README,
and in Session 166.

## 8. Defects found and actions

| Finding | Action |
| --- | --- |
| `GET /hooks?x=1` was asserted as 404 in the driver; the implementation correctly returns 405 (method first) | corrected the test, added the precedence rule to the README |
| 25-event case was budgeted 27 requests while sending 28 | corrected the budget; the refusal demonstrated `--max-requests` shutdown |
| fatal `/events` read failure exited 4 with no diagnostic | added `hookbox: cannot read event file` |
| temp-dir cleanup could race a server left running by a failed assertion | driver now kills active servers before removing the directory |
| Windows case-insensitive names: `000099.JSON` and `000099.json` are one file; only byte-exact lowercase names count as events | test probe moved off the live sequence; recorded as a Windows filesystem boundary, not a contract change |

No application logic defect was found in the audit itself beyond the missing
diagnostic. No MINK compiler/runtime/stdlib defect was found. `stdlib/json.mink`
was not modified.

## 9. Repeated-run and standalone evidence

- `python benchmarks/m5/hookbox/tests/driver.py --no-build` three times:
  `60 passed, 0 failed, 0 skipped, 60 checks` each time, exit 0.
- The suite's own determinism case runs the three-request scenario three times
  in fresh inboxes and byte-compares stdout (port normalized) and every event
  file.
- Standalone: the executable is copied to a temporary directory outside the
  repository and run with a working directory outside it; the event file is
  written under the working directory, not beside the executable. Absolute
  inbox paths are covered separately.

## 10. Contract classification

| Frozen area | Status |
| --- | --- |
| CLI, help, exit codes 0–4 | GREEN |
| `POST /hooks` for bodies ≤ 4,096 bytes | GREEN |
| `POST /hooks` for 4,097–8,192-byte valid bodies | **PARTIAL — MINK stdlib limit** |
| `GET /health` | GREEN |
| `GET /events` | GREEN |
| 404 / 405 / 413 / 500 behavior | GREEN |
| Event naming, bytes, recovery, determinism | GREEN |
| Inbox creation/writability/cleanup | GREEN |
| Standalone Windows executable | GREEN |

The correct overall label is therefore **APPLICATION GREEN / CONTRACT
PARTIAL**, not "COMPLETE".

## 11. Commit

Only `benchmarks/m5/hookbox/tests/driver.py`,
`benchmarks/m5/hookbox/README.md` and this record changed. No MINK source, no
`repoaudit` file and no unrelated pre-existing work was touched. No reset,
clean, stash, rebase, amend, force, push, tag or publish was used.
