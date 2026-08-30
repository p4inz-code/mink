# SESSION 70 — HTTP LIBRARY V1

## Overview

Session 70 implemented the MINK HTTP library V1, providing HTTP/1.1 client functionality over TCP sockets. This is the first high-level networking library built on top of the Session 68 networking foundation.

## Public API

### URL Parsing
- `http_url_host(url: Str) -> Str` — Extract host from URL
- `http_url_port(url: Str) -> Int` — Extract port (default: 80)
- `http_url_path(url: Str) -> Str` — Extract path (default: "/")

### Request Construction
- `http_request(method: Str, path: Str) -> Str` — Create request line
- `http_get(path: Str) -> Str` — Create GET request line
- `http_add_header(request: Str, name: Str, value: Str) -> Str` — Add header
- `http_finalize_request(request: Str) -> Str` — Add empty line terminator

### Response Parsing
- `http_status_code(response: Str) -> Int` — Parse status code
- `http_status_text(response: Str) -> Str` — Parse reason phrase
- `http_header(response: Str, name: Str) -> Str` — Find header (case-insensitive)
- `http_content_length(response: Str) -> Int` — Get Content-Length (-1 if missing)
- `http_body(response: Str) -> Str` — Extract body after headers

### Client
- `http_send(request: Str, host: Str, port: Int) -> Str` — Send raw request
- `http_client_get_with(host: Str, port: Int, path: Str) -> Str` — GET with explicit host
- `http_client_post_with(host: Str, port: Int, path: Str, body: Str) -> Str` — POST with explicit host
- `http_client_get(url: Str) -> Str` — GET from full URL
- `http_client_post(url: Str, body: Str) -> Str` — POST from full URL

### Utility
- `http_parse_int_from_bytes(s: Str, start: Int, end: Int) -> Int` — Parse digits from byte range
- `http_sub_bytes(s: Str, start: Int, end: Int) -> Str` — Extract substring
- `rt_str_from_char(b: Int) -> Str` — Single byte to string
- `rt_str_parse_int(s: Str) -> Int` — String to integer

## Implementation

### String Memory Management
All string building uses `rt_str_alloc` + `rt_str_set_byte` to avoid the `rt_str_concat` memory leak. This pattern:
1. Allocates a buffer of the exact size needed
2. Copies bytes using only runtime intrinsics (non-consuming)
3. Returns the buffer without any intermediate allocations

### HTTP Parser
- Byte-by-byte state machine
- Case-insensitive header matching via `http_to_lower` / `http_byte_eq_ci`
- Content-Length parsed inline (no allocation)
- Status code parsed from fixed position (byte 9-12 of response)

### Client
- TCP socket per request (no connection reuse)
- Full response received before returning (http_recv_all)
- Socket closed after each request (Connection: close)

## Test Coverage (35 tests)

### URL Parsing (9 tests)
- Simple host, host with HTTP prefix, host with port and path
- Default port, custom port, port with HTTP prefix
- Simple path, root path, path with HTTP prefix

### Request Construction (4 tests)
- GET request line, custom method, single header, multiple headers

### Response Parsing (7 tests)
- Status code 200/404, status text, header extraction, case-insensitive headers

### Header Extraction (5 tests)
- Content-Length, Content-Type, case-insensitive, not found, multiple headers

### Body Extraction (5 tests)
- Simple body, empty body, multiline body, Content-Length function, Content-Length missing

### Edge Cases (5 tests)
- Status 201, status 500, colon in header value, empty header value, port/path parsing

## Security Audit

### A/B Findings (Fixed)
None — all security concerns are C/D level for V1.

### C-Level Findings (Documented)
1. **CRLF injection**: User-provided header names/values are not sanitized for `\r\n`. V1 limitation.
2. **No request body size limit**: POST body can be up to 1 MB (MAX_BODY).
3. **Integer overflow in Content-Length**: Malicious Content-Length could cause overflow in parser. Mitigated by MINK's 64-bit integers.
4. **No timeout**: Long-running requests block indefinitely. Documented as future work.

### E-Level Findings
- Buffer overflow in `http_recv_all`: Correctly bounded by `max_size` allocation.

## Performance

### Characteristics
- O(n) header lookup (linear scan)
- O(n) body extraction (linear scan for \r\n\r\n)
- One allocation per string operation
- No intermediate string copies

### Trade-offs
- Simple implementation over performance
- No header caching or indexing
- Acceptable for V1 usage patterns

## Limitations

1. **No chunked transfer encoding** — Server must send Content-Length
2. **No HTTPS/TLS** — Requires separate TLS library
3. **No connection pooling** — New TCP connection per request
4. **No timeout support** — Blocks until completion
5. **No query string parsing** — URL parsed for host/port/path only
6. **No streaming** — Full response must be received before processing
7. **No redirect following** — Client returns raw response

## Ecosystem Status

HTTP is **ECOSYSTEM-READY** for V1 scope:
- 35/35 tests pass
- No ACCESS_VIOLATION
- No ignored correctness tests
- No unsafe Rust
- Security audit complete (no A/B findings)
- Documentation complete
- Networking dependency stable (26/26)

## Future Work

1. **Chunked transfer encoding** support
2. **HTTPS/TLS** integration
3. **Connection pooling** and keep-alive
4. **Timeout** support
5. **Streaming** responses
6. **Redirect** following
7. **Cookie** handling
8. **Form data** encoding
9. **JSON** request/response bodies
10. **WebSocket** upgrade support
