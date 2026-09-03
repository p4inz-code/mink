# Session 93 — Linux HTTP Execution Verified

Date: 2026-09-03. Branch `main`, starting commit `7572a8a`. MINK v1.0.1.

## Scope

No package manager, REPL, debugger, formatter, Git automation, C ABI, DLL/SO,
Python/C++ interop work. This session's sole objective: **Linux HTTP =
EXECUTION VERIFIED** — a generated MINK Linux ELF performing real localhost
HTTP against a server that deliberately splits responses across many TCP
sends.

## Linux subsystem status matrix

| Subsystem                  | Previous (S92)         | Current (S93)                              | Verification level |
|----------------------------|------------------------|--------------------------------------------|--------------------|
| ELF backend                | STABLE                 | unchanged                                  | STABLE             |
| Core runtime / heap        | STABLE (1 MiB arena)   | arena 4 MiB; allocator first-fit reuse     | STABLE             |
| TCP networking             | STABLE                 | unchanged                                  | STABLE             |
| UDP                        | EXECUTION VERIFIED     | unchanged                                  | EXECUTION VERIFIED |
| env_get / env_has          | EXECUTION VERIFIED     | unchanged                                  | EXECUTION VERIFIED |
| env_set / env_remove       | STUB (deferred)        | unchanged                                  | STUB               |
| Crypto runtime subset      | EXECUTION VERIFIED     | unchanged                                  | EXECUTION VERIFIED |
| **HTTP (Linux)**           | **BLOCKED**            | request/response round-trip verified       | **EXECUTION VERIFIED** |
| Literal-free ownership     | E-R07 crash on free    | freeing image-region literal = safe no-op  | EXECUTION VERIFIED |
| http_recv_all ownership    | latent per-chunk leak  | exact-length result, single-exit frees     | EXECUTION VERIFIED |

## Findings and fixes (each root-caused before patching)

1. **Ownership blocker (architectural).** User-function `Str` parameters are
   Owned (callers pass by move, per the frozen model) and callees must free
   them — but `rt_str_free` on a literal crashed (E-R07 on Windows, silent
   exit on Linux). No stdlib function could therefore safely consume a heap
   string, and the whole `http_client_get` chain leaked on every platform.
   **Fix:** `rt_str_free`/`Free` of an immutable image-region string is now a
   safe no-op on both platforms (image data was never heap-allocated). Double
   frees, interior pointers, and non-live frees still fail E-R04. This is the
   minimal reusable primitive that makes consume-and-free implementable.
   Ownership doc rule updated accordingly.

2. **`http_client_get_with` was never executed.** Session 70 HTTP tests were
   parse-only, so the client path ran nowhere — not even on Windows. It
   hardcoded request-part lengths (`sm = 14`, `st = 24`) that do not match the
   actual literals (17 and 22 bytes), so the tail copy read past the literal
   (E-R09 on Linux). **Fix:** derive all lengths with `rt_str_len` from the
   literals (same pattern `http_client_post_with` already used).

3. **`http_recv_all` returned a capacity-sized string.** The accumulation
   buffer is `MAX_BODY()+8192` bytes, so `rt_str_len(response)` was ~1 MiB no
   matter how many bytes arrived; parsed bodies would have been garbage.
   **Fix:** after the receive loop, copy exactly `total` received bytes into an
   exact-length result and free the buffer — single textual frees, one exit.

4. **The 1 MiB receive buffer could not fit the 1 MiB heap arena.** `rt_str_alloc(MAX_BODY()+8192)` alone exceeded `HEAP_SIZE` (1,048,576), and the
   final trim copy needs roughly 2× the body live at once. **Fix:** grow the
   shared arena to 4 MiB (`src/runtime/abi.rs`); BSS layout is computed from
   the constant for both platforms. `heap_exhaustion_is_e_ro2` resized to
   match (2 × 2 MiB then E-R02).

5. **Allocator fragmentation (LIFO-top-only reuse).** The free list reused
   only the most recently freed block; small blocks freed after a big one
   (per-request request/response strings) permanently hid the big block, so
   each `http_recv_all` bump-allocated a fresh 1 MiB and the arena died after
   ~4 requests (E-R02) even though every block was freed. **Fix:** `Alloc`
   now walks the free list and reuses the first block large enough (skipped
   blocks are dropped only when a later fit is found; otherwise the list is
   preserved and the allocator bumps). Implemented identically in the Windows
   emitter, the Linux emitter, and the Rust reference model. This is the fix
   that lets repeated large receives run leak-free forever.

## Ownership model applied to the HTTP client path

- `http_send(request, host, port)` consumes and frees both parameters on every
  path (single textual exit; `rt_net_connect`/`rt_net_send` intrinsics borrow,
  so both stay live until the bottom free). Failure paths return `""` without
  leaking.
- `http_client_get_with`/`http_client_post_with` free their remaining string
  parameters (`path`, `body`) after `http_send` returns.
- `http_client_get`/`http_client_post` free `url` after the builder call.
- Parse/query helpers (`http_body`, `http_header`, `http_status_code`, …)
  remain read-only consumers of their parameters: they do not free, so V1
  callers must pass literals or parse inline within the owning function (the
  pattern used by the verified client below). Documented limitation.

## Execution evidence (WSL, generated MINK ELF)

Build: `mink build --target x86_64-linux-elf` of `network.mink + http.mink +
client`. Client performs 8 sequential HTTP exchanges against a deterministic
localhost python server (127.0.0.1, no public internet) that splits responses
across many sends, then exits 0. Exit 0 also proves the leak checker (E-R06)
never fired across 8 requests including error paths.

| Case                 | Server behaviour                                       | Client result |
|----------------------|--------------------------------------------------------|---------------|
| `/health`            | 200, body `ok`                                         | pass          |
| `/split`             | status + headers + body sent in 6 parts with delays    | body `hello world\n` byte-exact |
| `/slowhdr`           | every header line a separate send                      | body `hello`   |
| `/big`               | 19,998-byte body in 4 chunks (many `net_recv`)         | length + all-A prefix + `MINK-END` tail verified |
| `/empty`             | 200, Content-Length 0                                  | pass          |
| `/err404`            | 404 with body `not found`                              | status 404 + body |
| refused              | no listener on port                                    | empty response, no hang, no leak |
| `/close_early`       | closes mid-headers                                     | partial response, no hang, no crash |

Rerun-stable (multiple consecutive runs exit 0). Durable regression:
`linux_h01_http_execution_verified` in `tests/process_lib.rs` (embedded python
server + embedded client, runs through the Rust harness in WSL).

## HTTP capability matrix (current MINK API)

| Capability                | Status |
|---------------------------|--------|
| Request creation (GET/POST) | IMPLEMENTED (GET chain execution-verified; POST builder correct but not exercised end-to-end this session) |
| TCP connection            | EXECUTION VERIFIED |
| Request serialization     | EXECUTION VERIFIED |
| Send / receive            | EXECUTION VERIFIED |
| Status code / headers / body parsing helpers | WINDOWS VERIFIED (parse tests); helpers are read-only — see ownership note |
| Connection close semantics| EXECUTION VERIFIED (Connection: close) |
| Error handling (refused, premature close) | EXECUTION VERIFIED |
| Timeouts                  | MISSING (no socket timeout API) |
| Chunked transfer encoding | MISSING (Content-Length/EOF framing only) |
| TLS / HTTPS               | MISSING |
| HTTP/2 / keep-alive       | MISSING (HTTP/1.1, close per request) |
| Persistent connections    | MISSING |

## Quality gates

- `cargo fmt --check` — clean
- `cargo clippy --all-targets --all-features` — only pre-existing warnings in
  untouched files
- `cargo test` — full suite 2514 passed / 0 failed (includes 71-process_lib
  suite: 46 Linux WSL tests incl. new `linux_h01_http_execution_verified`)
- `cargo build` / `cargo build --release` — clean

## Remaining Linux gaps

- HTTP POST end-to-end execution (builder is length-correct by construction
  and shares the verified `http_send` path, but was not round-trip tested)
- Parse helpers cannot consume owned heap responses without leaking (V1
  move-model limitation; documented, with the inline-parse pattern as the
  supported usage)
- Socket timeouts, chunked encoding, TLS, keep-alive — not in the API
- env_set / env_remove still deferred stubs
- A real HTTP *project* (health checker over several endpoints) is embodied by
  the verified client; a fuller standalone example program is a natural next
  step

## Next recommended session

Session 94: HTTP POST execution verification + a standalone real-world MINK
project (e.g., a localhost service monitor or JSON-over-HTTP client using
`stdlib/json.mink`), then the Linux stability gate pass.
