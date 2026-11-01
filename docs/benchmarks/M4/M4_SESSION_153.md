# Session 153 — M4 W2 `netcheck`

**Date:** 2026-11-01
**Scope:** W2 contract, harness-owned local HTTP/1.1 fixture server, five
client implementations and the correctness gate. **No timing was measured and
no performance claim is made.**
**Result:** COMPLETE for W2 implementation and correctness. W2 gate PASS
(14 cases × 5 languages = 70/70; 8/8 scored cases in every language).

## 1. Contract

M1's `validation/e_network` exposes `http|https|tcp|udp` and prints
`status=`, `reason=`, `content-type=`, `content-length=` and `body=`. M4
narrows W2 to HTTP only and defines a different, smaller report. Where the
M4 text was silent, the resolution is recorded here and applied to all five
implementations before any was written.

### 1.1 Invocation and output

```text
netcheck <host> <port> <path>
```

| Outcome | stdout | stderr | Exit |
| --- | --- | --- | --- |
| Completed response, status < 400, body is a JSON object with an integral `value` | `STATUS <code>`, `BODY <decoded byte length>`, `VALUE <integer>` | — | 0 |
| Completed response, status >= 400 (for example 404) | `STATUS <code>`, `BODY <decoded byte length>`; `VALUE` only if the body qualifies | — | 0 |
| Completed response, status < 400, body is not JSON | — | `netcheck: malformed JSON` | 3 |
| Completed response, status < 400, valid JSON without an integral `value` | — | `netcheck: missing value` | 3 |
| Connect/read failure, or response bytes with no valid status line | — | `netcheck: connection failed` | 2 |
| Wrong argument count, unusable port, any `-`-prefixed argument | — | `netcheck: usage: netcheck <host> <port> <path>` or `netcheck: this tool has no options` | 1 |

`BODY` counts the **decoded** body: a chunked response reports the same
length as the equivalent Content-Length response.

### 1.2 Resolutions made before implementation

1. **The JSON field is `value`.** M4 says "extract one JSON field" without
   naming it. W2 fixes the member name to `value`, and the fixture server's
   JSON responses use it. The value must be an integral number; its decimal
   text is printed.
2. **`VALUE` is printed only for an integral `value`.** A 404 body that is
   HTML prints no `VALUE` line and still exits 0, which is the M4 rule that a
   completed 404 exchange is an application success.
3. **A 2xx body must carry the value.** This is the only way "malformed JSON"
   can be an error path at all. 4xx/5xx bodies are not JSON-checked.
4. **Errors print nothing on stdout.** The report is computed in full before
   anything is written, so exit 3 never leaves a partial `STATUS`/`BODY`
   pair behind. This makes the golden bytes unambiguous.
5. **No-status and transport failures share exit 2 and one message.** The M4
   text lists "connect/read failure **or** completed response with no status"
   under the same exit code. Each language reports these with a different
   exception type; one stable message keeps the observable contract equal and
   avoids comparing platform error vocabularies.
6. **10-second client timeout.** Not part of the contract and not exercised;
   it exists so a hung fixture cannot stall a run.

### 1.3 The eight scored cases

| # | Case | Server behavior | Expected | Exit |
| --- | --- | --- | --- | --- |
| 1 | `ok` | 200, Content-Length, JSON `{"value":42,...}` | `STATUS 200`, `BODY 31`, `VALUE 42` | 0 |
| 2 | `chunked` | 200, `Transfer-Encoding: chunked`, 3 chunks, same body | `STATUS 200`, `BODY 31`, `VALUE 42` | 0 |
| 3 | `notfound` | 404, `text/html` body | `STATUS 404`, `BODY 18` | 0 |
| 4 | `badjson` | 200, unparseable body | `malformed JSON` | 3 |
| 5 | `peer_close` | request read, connection closed with no bytes | `connection failed` | 2 |
| 6 | `refused` | nothing listening on a just-released port | `connection failed` | 2 |
| 7 | `no_status_line` | bytes with no HTTP status line | `connection failed` | 2 |
| 8 | `usage_too_few_args` | — | usage line | 1 |

Supporting cases (must pass, not in the eight-case budget): `value_missing`,
`empty_body`, `nonnumeric_port`, `zero_port`, `option_looking_arg`,
`extra_argument`.

## 2. Local fixture server

`benchmarks/m4/harness/server.py`, Python 3.11 standard library only.

- Binds `127.0.0.1` on an **ephemeral port**; the harness passes the port to
  every client, so no fixed port can collide and no orphan server survives.
- Runs in the harness process for the duration of a run and is stopped in a
  `finally` block. Each connection is handled in a daemon thread that is
  joined on stop.
- Endpoints: `/ok`, `/chunked`, `/notfound`, `/badjson`, `/novalue`, `/empty`,
  `/close`, `/garbage`; unknown paths return a 404.
- `closed_port` is obtained by binding and immediately releasing a port; it is
  used only for the connection-refused case.

### Deliberate fragmentation

The server never relies on one packet per response:

- every write is followed by a 20 ms pause (`FRAGMENT_PAUSE`);
- `/ok` sends the head, then the first 10 body bytes, then the remaining 21;
- `/chunked` sends the head, then each of the three chunks as a separate
  write (`a`, `a`, `b` hex sizes), then `0\r\n\r\n`;
- `/notfound` and `/badjson` send head and body separately.

This verifies that the MINK HTTP client and its chunked decoder accumulate
across receives instead of assuming a packet boundary, and it gives the other
four clients the same input shape.

## 3. Implementations

| Language | Path | Physical lines |
| --- | --- | --- |
| MINK 1.0.4 | `benchmarks/m4/apps/mink/w2_http/netcheck.mink` | 223 |
| Python 3.11.9 | `benchmarks/m4/apps/python/w2_http/netcheck.py` | 87 |
| Rust 1.97.1 | `benchmarks/m4/apps/rust/w2_http/src/main.rs` | 96 |
| Go 1.26.7 | `benchmarks/m4/apps/go/w2_http/main.go` | 104 |
| C# / .NET 10 | `benchmarks/m4/apps/csharp/w2_http/Program.cs` | 106 |

- **MINK** uses the bundled `http` library (`http_client_get`,
  `http_status_code`, `http_body`), the arena `json` library and its own
  status-line validator. No MINK source, compiler, runtime or stdlib file was
  changed.
- **Python** uses `http.client`, reads the decoded body, then `json.loads`.
- **Rust** uses `ureq` 2.12.1 with default features disabled (W2 is HTTP only,
  so no TLS stack is built) and `serde_json`.
- **Go** uses `net/http` and `encoding/json` with `UseNumber()`.
- **C#** uses `HttpClient` and `System.Text.Json`.

Every client reads the body to completion before printing, so the report is
atomic.

## 4. Dependencies

| Language | Dependency | Version | Direct | License | Restore |
| --- | --- | --- | --- | --- | --- |
| MINK | bundled `http`, `network`, `json`, `strings` | 1.0.4 | yes | Apache-2.0 (repository) | none |
| Python | `http.client`, `json` | 3.11.9 | yes | PSF-2.0 | none |
| Rust | `ureq` | 2.12.1 | yes | MIT OR Apache-2.0 | local cargo cache, `--offline` |
| Rust | `serde_json` | 1.0.151 | yes | MIT OR Apache-2.0 | local cargo cache, `--offline` |
| Go | `net/http`, `encoding/json` | 1.26.7 | yes | BSD-3-Clause | none |
| C# | `HttpClient`, `System.Text.Json` | .NET 10 | yes | MIT | in-box, no NuGet package |

### Rust transitive burden (measured, not hidden)

`cargo tree -e normal` reports 41 distinct crates in the W2 build graph (54
lines including repeated path annotations): `base64`, `displaydoc`,
`form_urlencoded`, `icu_collections`, `icu_locale_core`, `icu_normalizer`,
`icu_normalizer_data`, `icu_properties`, `icu_properties_data`,
`icu_provider`, `idna`, `idna_adapter`, `itoa`, `litemap`, `log`, `memchr`,
`once_cell`, `percent-encoding`, `potential_utf`, `proc-macro2`, `quote`,
`serde_core`, `serde_json`, `smallvec`, `stable_deref_trait`, `syn`,
`synstructure`, `tinystr`, `unicode-ident`, `ureq`, `url`, `utf8_iter`,
`writeable`, `yoke`, `yoke-derive`, `zerofrom`, `zerofrom-derive`,
`zerotrie`, `zerovec`, `zerovec-derive`, `zmij`.

The count above the two direct crates comes from `ureq` → `url` → ICU4X
(IDNA). Licenses are MIT/Apache-2.0 except the ICU4X crates, which are
`Unicode-3.0`. This is recorded as dependency burden evidence; it is not a
defect and was not worked around.

No crate was downloaded: every crate resolved from the existing local cargo
cache with `--offline`. The lockfile is committed at
`benchmarks/m4/apps/rust/w2_http/Cargo.lock`.

## 5. Correctness

```text
14 cases x 5 languages
  mink    14/14 passed, 0 failed
  python  14/14 passed, 0 failed
  rust    14/14 passed, 0 failed
  go      14/14 passed, 0 failed
  csharp  14/14 passed, 0 failed

scored cases: 8
  ok                  all five PASS
  chunked             all five PASS
  notfound            all five PASS
  badjson             all five PASS
  peer_close          all five PASS
  refused             all five PASS
  no_status_line      all five PASS
  usage_too_few_args  all five PASS

GATE: PASS
```

W1 was re-run after the harness gained W2 support: 115/115 still pass.

### Repeatability

Three consecutive W2 runs produced identical record streams
(SHA-256 `5117120c95288a7693ef5c053e1f52ea169ae190378e277ac301e2e9d9bea144`
each time), covering exit code and stdout/stderr hashes. The ephemeral port
changes between runs and is not part of any output.

## 6. Build commands and artifacts

| Language | Command | Artifact | Bytes |
| --- | --- | --- | --- |
| MINK | `target/release/mink.exe build <repo>/benchmarks/m4/.build/w2_http/mink/netcheck.mink` (cwd = repository root) | `netcheck.exe` | 235,008 |
| Python | none | `netcheck.py` | 2,159 |
| Rust | `cargo build --release --offline --manifest-path ... --target-dir ...` with `RUSTFLAGS=""` | `release/netcheck.exe` | 675,328 |
| Go | `go build -trimpath -buildvcs=false -o ... .` | `netcheck.exe` | 8,915,968 |
| C# | `dotnet publish -c Release -r win-x64 --self-contained false -o ...` | apphost 162,304 + `netcheck.dll` 8,704 + `.deps.json` 472 + `.runtimeconfig.json` 449 | 162,304 (+ 9,625) |

PE imports:

| Artifact | Imports |
| --- | --- |
| MINK | `kernel32.dll` |
| Rust | `WS2_32.dll`, `VCRUNTIME140.dll`, UCRT API sets, `KERNEL32.dll`, `ntdll.dll`, `bcryptprimitives.dll`, `api-ms-win-core-synch-l1-2-0.dll` |
| Go | `kernel32.dll` |
| C# apphost | `SHELL32.dll`, `ADVAPI32.dll`, `KERNEL32.dll`, `USER32.dll`, UCRT API sets (plus the .NET 10 runtime and sidecar files) |

The Rust binary now imports `WS2_32.dll` because it performs socket I/O; this
is expected artifact evidence, not a runtime measurement.

## 7. Failures and fixes

One application-level defect was found and fixed:

- **MINK `E-S10: use of moved value 'resp'`.** MINK user functions take
  ownership of `Str` parameters, so passing `resp` to the status-line helper
  moved it. Fixed by passing `rt_str_concat(resp, "")` and making the helper
  single-exit with `rt_str_free` before returning. This is the documented MINK
  ownership rule, not a compiler defect; no MINK file was changed.

Harness work in this session: the W2 fixture server, token resolution
(`{port}`, `{closed_port}`) and workload-aware validation. No expected output
was weakened.

## 8. Git state

| Item | Value |
| --- | --- |
| HEAD | `691ec38386bb37e204b834e61b0ff8de99a0ac65` (unchanged before commits) |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` (unchanged) |
| Modified tracked | 40 pre-existing Session 128–149 files, untouched |
| `git diff --check` | clean |
| New | `benchmarks/m4/` W2 tree, `docs/benchmarks/M4/M4_SESSION_153.md` |

No reset, clean, stash, rebase, amend, force, push, tag or publish was
performed. The W2 files are isolateable from the pre-existing work.

## 9. Limitations

- **No timing, memory or build-time result is recorded.** Correctness is not
  performance evidence.
- Loopback only; no public internet, TLS, redirects, cookies, keep-alive,
  HTTP/2 or large bodies. TLS is explicitly out of W2.
- MINK's HTTP client keeps `Connection: close` semantics, caps the body at
  4 MiB and reads at most 64 headers; none of the fixtures approach those
  limits.
- The 20 ms fragment pause is a deterministic separation mechanism, not a
  latency measurement; it must not be interpreted as network cost.
- Chunk extensions and trailer fields are not part of W2, and MINK ignores
  them.
- `BODY` is the decoded length. A client that returned a framing error instead
  of decoding would fail a case rather than silently report the wire size.
- The Rust dependency graph is large because of `ureq` → `url` → ICU4X; that
  is a real dependency burden and remains part of the build/distribution
  panel.
- W2 covers one request per process; connection reuse is not exercised.

## 10. Next step

Session 154 — W3 `dbshell`: inspect `validation/d_sqlite`, fix the
common-SQL-subset contract, implement the in-memory workload in all five
languages, and require every golden case to pass before W4.

W1 and W2 remain correctness-complete; no timing may start until W3–W5 are in
the same state.
