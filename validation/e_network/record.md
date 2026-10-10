# E — netcheck (HTTP / HTTPS / TCP / UDP clients)

PROJECT: E networking / internet
PURPOSE: Fetch a URL or exchange a payload with a server — the "call the API
and read the reply" task a Python developer reaches for with `urllib` /
`http.client` (HTTP), `ssl` (HTTPS) and `socket` (TCP/UDP).
MINK SOURCE: `validation/e_network/main.mink` (158 functions emitted)
PYTHON COMPARISON: `validation/e_network/python_ref.py` (`http.client`, `ssl`,
`socket`)
CROSS-CHECK HARNESS: `validation/e_network/verify.py` (starts local HTTP, TLS,
TCP-echo and UDP-echo servers, then runs both implementations against them)
FIXTURES: `validation/e_network/fixtures/tlsroot/index.html` (served over the
repository's deterministic TLS fixture server, `tests/tls/server.py`, with the
committed test CA `tests/tls/ca.pem`)

BUILD: PASS — `mink build validation/e_network/main.mink` → `main.exe`
(x86_64-windows-pe, no external toolchain, no runtime DLL).

EXECUTION: PASS — 18/18 cases produce byte-identical stdout, stderr and exit
codes compared with the Python reference (`python verify.py` → `18/18 cases
match the Python reference`), covering HTTP 200 / empty body / 404 /
connection-refused, HTTPS 200 / 404 / connection-refused, TCP echo (twice),
TCP refused, UDP echo (twice), and the usage-error paths.

FUNCTIONAL CORRECTNESS: PASS — `http` drives the stdlib HTTP/1.1 client
(`http_client_get`, `Connection: close`, read to EOF) and reports status,
reason, content type, content length and body; `https` drives the TLS client
(`https_client_get` with an explicit trust anchor, Schannel-backed) and
reports the same fields; `tcp` connects, sends the payload and reports the
byte count and echoed reply; `udp` sends one datagram and reports the reply.
Status/reason/header parsing matches `http.client` on the same bytes,
including the generated 404 page's `text/html;charset=utf-8` type and 330-byte
length.

ERROR HANDLING: PASS — distinct exit codes verified against Python: 1 usage
(no/extra arguments, unknown command, non-numeric or zero port, any option),
2 connection or I/O failure (message on stderr). A refused TCP/HTTP/HTTPS
connection is reported identically by both implementations.

MEMORY: PASS — the runtime leak check (`E-R06`, exit 106) is enabled; all 18
cases exit 0/1/2, never 106. Two issues were found and fixed during validation
(see CLASSIFICATION): one in the MINK standard library, one in the app.

REALISTIC INPUT: PASS — a real local HTTP server, a real TLS handshake with
certificate-chain and hostname validation against a committed CA, and real
TCP/UDP round trips, each exercised twice (a second connection / a second
datagram on the same socket family).

REPEATED EXECUTION: PASS — the harness runs the second TCP connection and the
second UDP datagram through the same servers; repeated full runs give
identical results (18/18 each time).

STANDALONE EXE: PASS — `main.exe` runs with no interpreter and no runtime;
the only dependency is the Windows API.

WINDOWS: PASS.

MINK LIMITATION: see CLASSIFICATION; none of the findings blocked the
application.

CLASSIFICATION:
- B (defect, FIXED this session) — `http_client_get` / `http_client_post`
  could not work in a fresh program: `http_send` called `net_tcp_socket()`
  before any `WSAStartup`, and the runtime's socket intrinsic deliberately
  returns `-1` (INVALID_SOCKET) when Winsock is not initialized, so the very
  first request failed with an empty response and `HTTP_ERR_SOCKET`. The
  existing `h41_client_get_refused_reports_error` test did not catch it
  because it only asserts an *error* result, and `h40_client_get_by_host_name`
  passes only because its in-program server calls `net_listen_on` (which does
  initialize Winsock) before the client runs; the `examples/http_client`
  example works around the gap by calling `rt_net_wsa_startup` itself. Fixed
  in `stdlib/http.mink` (mirrored in `npm/mink/stdlib/http.mink`): `http_send`
  now calls `net_init()` before creating the socket, matching the header's
  claim that "a user only needs `mod http;`". WSAStartup is idempotent and the
  session is released at process exit (or by a caller's `net_cleanup`).
  Regression test: `tests/http_lib.rs` `h42_client_get_initializes_winsock_standalone`
  (starts a Rust `TcpListener`, builds a program whose only network call is
  `http_client_get`, asserts a 200) — verified to FAIL without the fix
  (`left: 0, right: 42`) and PASS with it; `http_lib` 38/38, `httpserver_lib`
  10/10, `network_lib` 36/36, `tls_lib` 19/19 (+1 ignored, live-internet test).
- A (application defect, found & fixed during validation) — `run_http` built
  the URL with `rt_str_concat("http://", host)` and never released the
  consumed `host` parameter on the success/refused paths, so every HTTP run
  ended with `E-R06` (exit 106). Fixed by releasing `host` right after the
  first concatenation (the intrinsic borrows it) and by having `report()`
  release the response once.
- C (architectural limitation) — the HTTP client is HTTP/1.1 with
  `Connection: close` only: no keep-alive, no chunked transfer encoding, no
  redirect following, no cookies, max 64 response headers and a 4 MB body cap
  (the reason for the cap is documented in `http_recv_all`). The parity target
  is a single request/response exchange.
- C (architectural limitation) — HTTPS is a separate library (`stdlib/tls.mink`)
  and requires an explicit trust anchor (a PEM CA path) or `""` for the
  Windows system store; there is no bundled CA list and no `SSL_CERT_FILE`
  style environment override.
- C (architectural limitation) — no IPv6 in these paths, no non-blocking
  request pipeline in the client helpers (the poll layer in `stdlib/network.mink`
  covers readiness separately).
- C (ergonomic trap, documented) — a nested `rt_str_concat` leaks the inner
  temporary (the intrinsic is read-borrowing); each step of the URL build is
  bound and released.
- D (deliberate difference) — `rt_print_str` appends CRLF without translating
  embedded newlines, so the HTTP body (which may end in LF) is followed by an
  extra CRLF; the Python reference writes raw bytes + `\r\n` to match.
- D (deliberate difference) — `rt_argc`/`rt_argv` exclude the program name; a
  leading `-` marks a usage error because the tool defines no options.
- D (deliberate difference) — `http_client_get`/`https_client_get` consume the
  URL (and the CA path) and return an OWNED response string; an empty response
  is always a failure (the stdlib documents that a response always contains a
  status line), with the reason in `http_last_error()`.
- D (harness note) — the HTTPS cases re-use the repository's deterministic TLS
  fixture server and committed test CA instead of the public internet, so the
  suite stays offline and reproducible; the live-internet TLS test in
  `tests/tls_lib.rs` remains `#[ignore]`d.

EVIDENCE:
- TEST — `validation/e_network/verify.py`, 18 cases, 0 differences (stdout,
  stderr, exit code).
- FIXTURE — `validation/e_network/fixtures/tlsroot/index.html` plus the
  committed `tests/tls/` server and CA.
- OUTPUT — see the transcript below.
- REGRESSION — `cargo test --test http_lib` 38 passed (includes the new
  `h42`), `--test httpserver_lib` 10 passed, `--test network_lib` 36 passed,
  `--test tls_lib` 19 passed + 1 ignored. The full suite (73 integration
  targets + `--lib`, 3040 tests) was re-run green before starting E.

## Transcript

```
$ mink build validation/e_network/main.mink
mink: build: 'validation/e_network/main.mink' -> '.../main.exe'
(target: x86_64-windows-pe, 158 function(s), 1 binding(s))

$ python validation/e_network/verify.py
ok   http_index (exit 0)
ok   http_empty_body (exit 0)
ok   http_404 (exit 0)
ok   http_refused (exit 2)
ok   https_index (exit 0)
ok   https_404 (exit 0)
ok   https_refused (exit 2)
ok   tcp_echo (exit 0)
ok   tcp_second_connection (exit 0)
ok   tcp_refused (exit 2)
ok   udp_echo (exit 0)
ok   udp_second_datagram (exit 0)
ok   no_args (exit 1)
ok   unknown_command (exit 1)
ok   bad_port_text (exit 1)
ok   zero_port (exit 1)
ok   option_looking_arg (exit 1)
ok   too_few_args (exit 1)

18/18 cases match the Python reference

$ main.exe http 127.0.0.1 <port> /index.txt
status=200
reason=OK
content-type=text/plain; charset=utf-8
content-length=26
body=hello from the test server
exit=0

$ main.exe https localhost <tls-port> /index.html
status=200
reason=OK
content-type=text/html
content-length=71
body=<!DOCTYPE html>
<title>MINK TLS fixture</title>
<p>hello over TLS</p>
exit=0

$ main.exe tcp 127.0.0.1 <port> ping-1234567890
sent=15
recv=15
echo=ping-1234567890
exit=0

$ main.exe udp 127.0.0.1 <port> datagram-42
sent=11
recv=11
echo=datagram-42
exit=0

$ main.exe http 127.0.0.1 <closed-port> /index.txt
netcheck: connection failed
exit=2
```

## Python comparison notes (concrete differences, no ranking)

1. Python's `http.client.HTTPConnection` maps to `http_client_get`
   (URL-parsing wrapper) / `http_client_get_with` (host/port/path form); MINK
   builds the request bytes explicitly and parses the response with
   `http_status_code`/`http_status_text`/`http_header`/`http_content_length`/
   `http_body`. Each parser consumes its response argument, so every call is
   handed a copy.
2. Python's `ssl.create_default_context(cafile=...)` maps to
   `https_client_get(url, ca_path)`; both verify the chain against the given
   anchor and the hostname against the certificate SANs.
3. Python's `socket.create_connection` + `sendall` + `recv` maps to
   `net_init`/`net_tcp_socket`/`net_connect`/`net_send`/`net_recv`; one `recv`
   returns whatever arrived first, so the echo servers answer in a single
   `sendall` and both clients read it in one call.
4. Python's `sendto`/`recvfrom` map to `net_sendto`/`net_recvfrom`; the
   datagram payload and reply are byte-compared through the printed `echo=`
   line.
5. Python's exceptions map to status codes: a failed connection prints
   `netcheck: connection failed` (or `connect failed`) on stderr and exits 2;
   there is no exception type.
6. Python frees socket objects with the context manager; MINK releases every
   returned string and closes each socket explicitly (`net_close`,
   `net_cleanup`) and the `E-R06` leak checker enforces it.
