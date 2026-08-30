# SESSION 69 — HTTP FOUNDATION + ARCHITECTURE

## Overview

Session 69 established the architectural foundation for MINK's HTTP library V1. The work was combined with Session 70 into a single implementation run due to the complementary nature of the tasks.

## Architecture Decisions

### Protocol Scope
- **HTTP/1.1** with `Connection: close` semantics (no keep-alive in V1)
- **Content-Length** for body transfer (no chunked transfer encoding in V1)
- **No HTTPS/TLS** — TLS is a separate library to be implemented later

### Request Representation
- Request line + headers + body serialized into a single contiguous string
- Built using `rt_str_alloc` + `rt_str_set_byte` to avoid string memory leaks
- Header format: `Name: Value\r\n`

### Response Parsing
- Byte-by-byte parsing using `rt_str_byte` (non-consuming intrinsic)
- Case-insensitive header name matching
- Status code parsed directly from bytes (no allocation)
- Body extracted after `\r\n\r\n` separator

### Ownership Model
- MINK strings are moved (consumed) by user functions with `Str` parameters
- Runtime intrinsics (`rt_str_byte`, `rt_str_len`, `rt_str_set_byte`) do NOT consume
- All string building uses `rt_str_alloc` + `rt_str_set_byte` (zero intermediate allocations)
- `rt_str_concat` in loops leaks memory — avoided entirely in HTTP library

### Limits
- Max body size: 1,048,576 bytes (1 MB)
- Max headers: 64 per response
- No chunked transfer encoding
- No HTTPS

## Foundation Gaps Identified

1. **No chunked transfer encoding** — Acceptable for V1, documented as future work
2. **No timeout support** — TCP sockets block until completion; documented as future work
3. **No connection pooling** — Each request creates a new TCP connection
4. **No query string parsing** — URLs are parsed for host/port/path only

## Files Created
- `stdlib/http.mink` — HTTP library implementation (~600 lines)
- `tests/http_lib.rs` — Comprehensive test suite (35 tests)

## Dependencies
- `stdlib/network.mink` — TCP socket operations
- No new runtime intrinsics required
- No new backend changes required

## Security Considerations
- All header/body sizes bounded by `MAX_BODY()` (1 MB)
- Maximum 64 headers parsed per response
- Content-Length parsed with digit validation
- No dynamic memory allocation beyond string building
- CRLF injection possible in user-provided header names/values (V1 limitation, documented)

## Status
HTTP foundation architecture is internally consistent and ready for V1 implementation.
