# Session 147 — M3 P7: HTTP/1.1 chunked response decoding

**Scope:** the remaining M3 HTTP item only — decode `Transfer-Encoding: chunked`
response bodies in the existing HTTP client. Windows x86_64 is the target;
Linux stays frozen. Version stayed **1.0.4**. No push, tag, publish, merge,
rebase, reset, clean, or amend.

## 1. Baseline

| Item | Value |
| --- | --- |
| HEAD | `6e88a7bcd62b0e853967d3625e125657f2ec4f2e` |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` |
| Version | `1.0.4` |
| Worktree | 39 modified tracked, 23 untracked (Sessions 128–146) |
| Full suite (Session 146) | 3,133 passed / 0 failed / 1 ignored, exit 0 |
| Compiler warnings | 59 (existing baseline) |

## 2. Existing HTTP body architecture

`http_client_get` dials with the Session 145 bounded connect/read helpers,
sends the request, and `http_recv_all` accumulates the response until the peer
closes:

- receive budget: `MAX_BODY() + 8 KiB` = 4,194,304 + 8,192 bytes for the whole
  response, headers included;
- `Content-Length` completeness is checked by `_http_incomplete`, which skips
  the check for any `Transfer-Encoding` response and for bodyless statuses;
- `http_last_error()` is per-thread state (Session 144), 0 on success, a
  positive Winsock code on a network failure, or a negative `HTTP_ERR_*` code;
- `http_body` found the end of the header block and returned **every byte after
  it**, with no transfer-coding interpretation.

## 3. Exact gap

The client delivered chunk framing as body data. A response

```
HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n
3\r\nabc\r\n0\r\n\r\n
```

produced a 12-byte body containing `3\r\nabc\r\n0\r\n\r\n` instead of `abc`.
The pre-fix `h49` test encoded this legacy behavior (`"A completed chunked
response keeps its V1 behavior"`), so the defect was previously asserted as a
contract rather than tested as a limitation.

## 4. Root cause

No transfer-coding decoder existed. `http_body` was a raw slice after the
header terminator, so chunk sizes, delimiters, the terminating zero chunk, and
trailers all reached callers. The receive path can already accumulate a whole
response across arbitrary TCP boundaries, so the missing piece was a stream
walk over those accumulated bytes, not a socket-level change.

## 5. Design decision

Decode in `stdlib/http.mink` at `http_body`, after the response has been
fully accumulated. This is the layer that already knows the header boundary
and owns the final `Str`; it adds no runtime primitive, intrinsic, IR op,
backend emitter change, import entry, or ABI surface. Because the input is the
complete byte sequence, fragmentation across `recv` calls is inherently
handled: the decoder never assumes a packet contains a header, size line,
payload, CRLF, zero chunk, or trailer.

Stdlib only (both copies kept byte-identical):

- `stdlib/http.mink`
- `npm/mink/stdlib/http.mink`

No runtime, compiler, parser, print, UDP, timeout, version, or metadata change.

## 6. Parser contract

`_http_encoding` classifies a response as chunked when a
`Transfer-Encoding` header value contains the whole token `chunked`,
case-insensitively. Token boundaries are checked so a longer word never
matches. `Transfer-Encoding` takes precedence over `Content-Length`, including
an inconsistent `Content-Length`.

`_http_chunked_decode` implements a bounded stream walk:

| Input | Behavior |
| --- | --- |
| Chunk size | One or more hexadecimal digits, upper or lower case |
| `;extension` after the size | Ignored through the CRLF; never becomes body bytes |
| Size-line CRLF | Required; a bare LF or missing CRLF is malformed |
| Payload | Exactly `size` bytes copied; CRLF after the payload required |
| Fragmentation | Irrelevant: the walk uses byte offsets over the full response |
| Zero chunk | Ends the body; trailer fields are consumed through the blank line and never appended |
| Trailer block | Must terminate with an empty CRLF line; otherwise malformed |
| Bytes after the terminal blank line | Ignored (connection-close client) |
| Empty decoded body | Owned empty `Str`, error 0 |
| Malformed size / CRLF / truncated frame / missing zero chunk | Empty body, `HTTP_ERR_RECV` (-5), unless a more specific error is already set |
| Decoded body over `MAX_BODY()` | Empty body, `HTTP_ERR_TOO_LARGE` (-4) |
| Non-chunked response | Unchanged: raw bytes after the header block |

The chunk-size accumulator stops multiplying once it exceeds `MAX_BODY()`, so
a long or absurd size line cannot overflow an `Int`. The temporary buffer is
sized from the **wire bytes actually received** (`end - start`), never from an
attacker-supplied chunk size, and the decoded length is checked cumulatively
against `MAX_BODY()` before any copy. The final `Str` is allocated at the exact
decoded length.

## 7. Body-limit behavior

- Nominal body limit: `MAX_BODY() = 4,194,304` bytes (4 MiB).
- The transport budget is `MAX_BODY() + 8 KiB` for the whole response
  (headers included), so a complete 4 MiB decoded body plus normal chunk
  framing fits.
- The limit applies to the **decoded** body, not wire bytes:
  - exactly 4 MiB decoded → accepted whole (`h72`);
  - one byte over → empty body + `HTTP_ERR_TOO_LARGE` (-4) (`h73`);
  - a chunk whose declared size alone exceeds the limit is rejected before
    allocation (`h64`).
- A response whose wire bytes exceed the receive budget remains
  `HTTP_ERR_TOO_LARGE` from `http_recv_all`, as before.

## 8. Malformed-input behavior

Malformed framing never returns partial body data as success. The body is an
owned empty `Str`; `http_last_error()` reports `HTTP_ERR_RECV` (-5), or keeps a
more specific existing error. `HTTP_ERR_TOO_LARGE` (-4) wins for any size or
cumulative-length violation. Winsock errors are positive and are never
rewritten by the decoder.

Covered cases: non-hex size, missing payload CRLF, EOF inside a payload, EOF
after a payload, missing zero chunk, missing trailer terminator, and a size
line that overflows the body limit.

## 9. Tests

`tests/http_lib.rs` (all local loopback, deterministic, no internet):

- `h49` rewritten from the retired "framed body" assertion to client-level
  decoding, including a `Transfer-Encoding: chunked` response with an
  inconsistent `Content-Length: 99` (TE wins; body is the decoded chunk).
- 19 new tests, `h55`–`h73`:

| Test | Coverage |
| --- | --- |
| h55 | H1 multiple chunks concatenated |
| h56 | H2 single chunk, upper-case hex, `;foo=bar` extension |
| h57 | H4 lower-case hex, case-insensitive header/value |
| h58 | H3 valid empty chunked body, error 0 |
| h59 | H10 trailers ignored, never appended |
| h60 | H11 malformed size → -5, empty |
| h61 | H12 missing payload CRLF → -5, empty |
| h62 | H13 premature EOF inside payload → -5, empty |
| h63 | H14 premature EOF after payload → -5, empty |
| h64 | H17 absurd 64-bit size bounded → -4, empty |
| h65 | H9 missing zero chunk → -5, empty |
| h66 | H18 Content-Length and close-delimited bodies unchanged |
| h67 | H5 size line fragmented across receives |
| h68 | H6 payload fragmented across receives |
| h69 | H7 payload CRLF fragmented across receives |
| h70 | H8 all framing fragmented at arbitrary boundaries |
| h71 | H20 repeated chunked requests on independent connections |
| h72 | H15 decoded body exactly 4 MiB accepted whole |
| h73 | H16 decoded body one byte over → -4, empty |

Every test that checks data verifies actual returned bytes and exact lengths.
The fragmentation tests split the header, size line, payload, CRLF, zero
chunk, and trailer terminator across separate writes.

Focused results:

| Suite | Result |
| --- | --- |
| `tests/http_lib` | **68 passed / 0 failed** (49 pre-existing + 19 new) |
| `tests/network_lib` | 65 / 0 |
| `tests/threads_lib` | 22 / 0 |
| `tests/httpserver_lib` | 10 / 0 |

## 10. Full verification

| Check | Result |
| --- | --- |
| `cargo test --no-fail-fast` | **3,152 passed / 0 failed / 1 ignored**, 77 binaries, exit 0 |
| Delta vs Session 146 | +19 (the new HTTP tests); no unrelated test changed |
| `cargo build --release` | exit 0; 59 existing warnings (unchanged) |
| Release chunked reproducer | exit **7**, `RELEASE len=9 err=0`, server `SERVER_OK` |
| `git diff --check` | exit 0 (clean) |
| Stdlib copies | `cmp` identical |

The release reproducer used the release `mink.exe` and a separate local Python
server that sent the response in 13 pieces, splitting the size line (`4` /
`\r` / `\n`), the payload (`ped` / `ia`), the payload CRLF, and the terminal
chunk. The release program verified length 9, error 0, and first byte `W`,
then exited 7.

Working-copy line endings on the two touched stdlib copies and the test file
were normalized to LF so `git diff --check` stays clean; this is
whitespace-only and does not affect behavior or the diff content.

## 11. Audit

- **Architecture:** stdlib-only change in the existing body accessor; no new
  abstraction, async runtime, socket ownership change, or shared state.
- **Windows/ABI:** no runtime, intrinsic, IR, emitter, import-table, BSS, or
  calling-convention change; no new PE import.
- **Body safety:** temporary buffer sized from received wire bytes (max
  `MAX_BODY + 8 KiB`), cumulative decoded cap checked before copy, exact final
  allocation, no terminator assumptions, embedded NUL preserved.
- **Integer safety:** size accumulation stops above `MAX_BODY`; all offsets
  are bounded by the received length and the cap before use; no negative
  length is passed to allocation.
- **Error safety:** malformed chunked responses never appear as successful
  body data; `HTTP_ERR_RECV` / `HTTP_ERR_TOO_LARGE` stay distinguishable;
  positive Winsock errors are preserved.
- **Ownership:** decoder returns the response unchanged and an owned result;
  `http_body` still frees the response exactly once and returns the result.
- **Regression:** non-chunked bodies unchanged; `Content-Length` completeness
  still skipped for transfer-coded responses; HTTP timeout behavior untouched.
- **Scope:** no UDP, timeout, parser/compiler, print, Linux, version, package,
  or branding change.

## 12. Known limitations

- Chunk extensions are accepted and ignored, not validated.
- Trailer fields are consumed and discarded (no trailer API).
- Only the `chunked` coding is decoded; `gzip`, `compress`, and other transfer
  codings are not decoded and never were.
- Bytes after the terminal trailer line are ignored; this client is
  connection-close only and has no keep-alive/pipelining.
- The whole-response receive budget is `MAX_BODY + 8 KiB`, so an extremely
  header-heavy response can hit the transport budget even if its decoded body
  alone would fit.
- Malformed responses return an empty body rather than any valid prefix.

## 13. Git state and commit decision

- HEAD remains `6e88a7bcd62b0e853967d3625e125657f2ec4f2e`; `origin/main`
  remains `b860837a8a6039f304a503e3d85728a643ab470b`; version `1.0.4`.
- Final status: 39 modified tracked files, 23 untracked files (same tree as
  Sessions 128–146). No commit was created.
- **Isolation is unsafe.** `stdlib/http.mink` (and its npm copy) plus
  `tests/http_lib.rs` also contain the uncommitted Session 144 thread-local
  HTTP error work, and the new `h49`/`h55`–`h73` tests depend on those
  helpers. File-level staging would absorb unrelated work; a hunk split would
  produce a commit whose tests reference helpers absent from HEAD. The
  verified work therefore stays in the working tree, uncommitted.

## 14. Remaining M3

1. `parse_block_expr` trailing-token recovery.
2. `rt_print_str` single-write line atomicity.
