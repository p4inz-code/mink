# hookbox — M5 Application 2

A local, loopback-only HTTP webhook inbox, implemented in MINK for Windows
x86_64. The frozen contract is section 6 of
[`docs/benchmarks/M5/M5_APPLICATION_SELECTION.md`](../../../../docs/benchmarks/M5/M5_APPLICATION_SELECTION.md).

## What it does

```text
hookbox --port <n> --inbox <dir> [--max-requests <n>]
hookbox --help
```

`--inbox` is created when missing (one level; the parent must exist). Event
files are `<inbox>/<seq:06d>.json`, sequence starting at 1 for an empty inbox
or `max(existing six-digit name) + 1`.

| Method/path | Behavior |
| --- | --- |
| `POST /hooks` | Body must be a JSON **object** (≤ 8,192 bytes). Stores one event and returns `201` with `{"seq":N}`. |
| `GET /health` | `200` with `{"status":"ok","count":N}`; N is the number of stored event files. |
| `GET /events?limit=N` | The last N events as `application/x-ndjson`, ascending. Default 10; N is 1–20. |
| Anything else | `404` with `{"error":"not found"}`. Wrong method on a known path is `405` with `{"error":"method not allowed"}`. |

Each event file is exactly `{"seq":N,"body":<compact JSON>}\n` (LF). No
timestamps, headers, client address or request id are stored.

stdout is exactly two CRLF lines — `listening port=<n>` at startup and
`served=<n>` on clean `--max-requests` completion. Diagnostics are
`hookbox: <label>` on stderr.

| Exit | Meaning |
| ---: | --- |
| 0 | Clean shutdown after `--max-requests` |
| 1 | Usage/option/argument error |
| 2 | Inbox cannot be created, is not a directory, or is not writable |
| 3 | `127.0.0.1:<port>` cannot be bound/listened on |
| 4 | Server I/O or event-file write/read failure |

Non-fatal request failures are HTTP statuses: `400` (bad/non-object JSON or a
bad `limit`), `404`, `405`, `413` (body over 8 KiB), `500` (storage failure;
the process then exits 4).

## Build

From the repository root:

```bash
target/release/mink.exe build benchmarks/m5/hookbox/app/main.mink
mkdir -p benchmarks/m5/hookbox/build
cp benchmarks/m5/hookbox/app/main.exe benchmarks/m5/hookbox/build/hookbox.exe
```

One native PE executable, no sidecar files, no runtime dependency beyond
Windows. Build outputs are ignored.

## Test

```bash
python benchmarks/m5/hookbox/tests/driver.py
```

`--no-build` reuses `build/hookbox.exe`; `--update-goldens` rewrites
`expected/` from the oracle; `--only <text>` runs a subset.

`tests/oracle.py` is an independent implementation of the contract (its own
JSON reader/writer, event format, response format and stdout format), not a
copy of the MINK code. `tests/driver.py` sends raw TCP requests, parses the
responses, compares event bytes against the oracle and the committed goldens,
and checks exit codes, stdout/stderr, cleanup, restart recovery and the
standalone executable.

The suite covers the committed body fixtures, byte-exact goldens, every exit
class, all four endpoints, unknown paths, wrong methods, missing/oversized
bodies, the JSON body boundary (4096 / 4097 / 8192 / 8193), limit boundaries,
sequence recovery with unrelated files present, two runs in one inbox, 25
events, three byte-identical repeated runs, write and read failures, cleanup,
and execution from outside the repository.

## JSON arena capacity

The frozen contract accepts bodies up to 8,192 bytes, and this application
accepts them (Sessions 174-175).

`stdlib/json.mink` exposes two entry points:

- `json_parse(input, arena)` keeps the historical 64 KiB geometry: 8,192 slots
  with the persistent write pointer in the final slot, which is why the old
  cutoff was around 4 KiB.
- `json_parse_in(input, arena, slots)` takes the caller's real slot count and
  stores the write pointer in slot `slots - 1`, so a larger arena can parse a
  larger document.

`hookbox` allocates 256 KiB (32,768 slots), passes that capacity to
`json_parse_in`, and resets the final slot before every parse. The 4,096 /
4,097 / 8,192 boundaries are permanent tests: valid objects of 4,096, 4,097
and 8,192 bytes return `201` with exact event bytes, and 8,193 bytes returns
`413`. The input bytes and parsed tree must fit below the final slot, and the
serializer writes after the tree; the 256 KiB arena accommodates an 8,192-byte
body with margin.

## Other recorded boundaries

- `count` is the number of stored event files, not `max(seq)`; a gap in the
  numbering is preserved.
- Only exactly eleven-byte `<6 digits>.json` names are events. `000000.json`,
  `1.json`, `1234567.json` and non-`.json` files are ignored. A sequence at or
  above 1,000,000 would produce a seven-digit name that the frozen six-digit
  rule cannot represent; the maximum request count makes that theoretical.
- Windows file names are case-insensitive. An existing `000026.JSON` is the
  same file as the next event `000026.json`: it is not recognized as an event
  (the name must be byte-exact lowercase) and the next event overwrites it.
  Fixtures avoid placing an upper-case-extension file on the live sequence.
- Methods match exactly (no `HEAD` support). Paths match exactly; only
  `/events` accepts a query string, and only `limit=<digits>` (leading zeros
  are accepted).
- Requests are Content-Length only, one connection at a time, no keep-alive or
  chunked requests. The stdlib reader caps one request at 64 KiB; a request
  beyond that cap is closed without a response (the frozen constraints say
  requests are ≤ 64 KiB).
- Startup writes and removes `<inbox>/.hookbox-probe` to prove the inbox is
  writable. A directory squatting on that name is the reproducible
  "unwritable" failure.
- On an event write failure the 500 is sent, any partial file is removed, and
  the process exits 4 without printing `served=`.
- No TLS, auth, CORS, redirects, cookies, HTTP/2, public binding, search,
  rotation, retention policy or database. No Ctrl+C handler (MINK has no
  console-control API); OS termination is not claimed as graceful.
- No Unicode path guarantee; fixtures and inbox paths are ASCII.
- No MINK compiler, runtime or stdlib source is modified by this application.
