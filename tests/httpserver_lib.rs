//! Tests for the MINK HTTP/1.1 **server** library (`stdlib/httpserver.mink`,
//! session 125).
//!
//! The module is the server counterpart of `stdlib/http.mink`: bind and
//! listen, accept, read a complete request, parse it, build and send a
//! response. Tests hs01-hs07 are offline (a request is built with
//! `rt_str_concat` and parsed directly); hs08-hs10 run a real MINK server on
//! 127.0.0.1 against a Rust client.
//!
//! MINK OWNERSHIP RULES FOR TESTS:
//! - Runtime intrinsics (rt_str_byte, rt_str_len, rt_str_concat, rt_str_eq,
//!   rt_str_free) do NOT consume their Str parameters.
//! - User functions with Str parameters DO consume the argument, so a value
//!   cannot be read after being handed to one.
//! - Every `Str`-taking helper here consumes its input exactly once, which is
//!   why a test that needs several parsed fields builds one heap copy of the
//!   request per field (`rt_str_concat(literal, "")`).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

fn httpserver_lib() -> String {
    std::fs::read_to_string("stdlib/httpserver.mink").expect("failed to read stdlib/httpserver.mink")
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mink_hs_test_{}_{name}", std::process::id()));
    std::fs::write(&path, content).unwrap();
    path
}

fn build_and_run(test_body: &str) -> (i32, Vec<u8>, Vec<u8>) {
    // `stdlib/httpserver.mink` declares `mod network;`, so the module loader
    // pulls in the TCP dependency itself; the test does not concatenate it.
    let source = with_module(test_body.to_string());
    let name = std::thread::current()
        .name()
        .unwrap_or("program")
        .replace("::", "_");
    let path = temp_source(&format!("{name}.mink"), &source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exe = path.with_extension("exe");
    assert!(exe.exists(), "no executable produced");
    let output = Command::new(&exe).output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (code, output.stdout, output.stderr)
}

/// Prepends the module under test to `body`.
fn with_module(body: String) -> String {
    let mut source = String::new();
    source.push_str(&httpserver_lib());
    source.push('\n');
    source.push_str(&body);
    source
}

/// Build the program, run it, and assert it exited 0 — no `E-R06` leak, no
/// fault, no assertion failure. The message names the test on failure.
#[track_caller]
fn assert_clean(body: &str) {
    let (code, out, err) = build_and_run(body);
    assert_eq!(
        code, 0,
        "expected a clean exit, got {code}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out),
        String::from_utf8_lossy(&err)
    );
}

// =========================================================================
// Offline parsing (hs01-hs07)
// =========================================================================

#[test]
fn hs01_request_line_method_path_and_version() {
    // The three fields of a request line, each taken from its own copy
    // because every `Str` helper consumes its input.
    let src = r#"
fn main() -> Int {
    let line = http_server_request_line(
        rt_str_concat("GET /index.html?q=1 HTTP/1.1\r\nHost: example\r\n\r\n", ""));
    let method = http_server_method(rt_str_concat("GET /index.html?q=1 HTTP/1.1\r\n", ""));
    let path = http_server_path(rt_str_concat("GET /index.html?q=1 HTTP/1.1\r\n", ""));
    let version = http_server_version(rt_str_concat("GET /index.html?q=1 HTTP/1.1\r\n", ""));
    let ok = rt_str_eq(line, "GET /index.html?q=1 HTTP/1.1");
    let ok2 = rt_str_eq(method, "GET");
    let ok3 = rt_str_eq(path, "/index.html?q=1");
    let ok4 = rt_str_eq(version, "HTTP/1.1");
    rt_str_free(line);
    rt_str_free(method);
    rt_str_free(path);
    rt_str_free(version);
    if ok == true && ok2 == true && ok3 == true && ok4 == true { return 0; }
    return 1;
}
"#;
    assert_clean(src);
}

#[test]
fn hs02_header_lookup_is_case_insensitive() {
    // A header is found whatever case the name is written in, and an absent
    // header yields an empty string rather than a partial match.
    let src = r#"
fn main() -> Int {
    let req = "GET / HTTP/1.1\r\nX-Mink-Test: hello world\r\nContent-Length: 0\r\n\r\n";
    let a = http_server_header(rt_str_concat(req, ""), "X-Mink-Test");
    let b = http_server_header(rt_str_concat(req, ""), "x-mink-test");
    let c = http_server_header(rt_str_concat(req, ""), "X-MINK-TEST");
    let d = http_server_header(rt_str_concat(req, ""), "X-Mink");
    let e = http_server_header(rt_str_concat(req, ""), "Absent");
    let ok1 = rt_str_eq(a, "hello world");
    let ok2 = rt_str_eq(b, "hello world");
    let ok3 = rt_str_eq(c, "hello world");
    let ok4 = rt_str_len(d) == 0;
    let ok5 = rt_str_len(e) == 0;
    rt_str_free(a);
    rt_str_free(b);
    rt_str_free(c);
    rt_str_free(d);
    rt_str_free(e);
    if ok1 == true && ok2 == true && ok3 == true && ok4 == true && ok5 == true { return 0; }
    return 1;
}
"#;
    assert_clean(src);
}

#[test]
fn hs03_content_length_and_body() {
    // Content-Length is read case-insensitively, the body is returned
    // separately, and a request with no body reports 0 and yields "".
    let src = r#"
fn main() -> Int {
    let post = "POST /submit HTTP/1.1\r\ncontent-length: 5\r\n\r\nhello";
    let get = "GET / HTTP/1.1\r\nHost: x\r\n\r\n";
    let p_len = http_server_content_length(rt_str_concat(post, ""));
    let p_body = http_server_body(rt_str_concat(post, ""));
    let g_len = http_server_content_length(rt_str_concat(get, ""));
    let g_body = http_server_body(rt_str_concat(get, ""));
    // No Content-Length at all.
    let n_len = http_server_content_length(rt_str_concat("GET / HTTP/1.1\r\n\r\n", ""));
    let ok1 = p_len == 5;
    let ok2 = rt_str_eq(p_body, "hello");
    let ok3 = g_len == -1;
    let ok4 = rt_str_len(g_body) == 0;
    let ok5 = n_len == -1;
    rt_str_free(p_body);
    rt_str_free(g_body);
    if ok1 == true && ok2 == true && ok3 == true && ok4 == true && ok5 == true { return 0; }
    return 1;
}
"#;
    assert_clean(src);
}

#[test]
fn hs04_parse_returns_every_field() {
    // http_server_parse is the one-call form: it takes six copies of the
    // request internally so a router can read all of it at once.
    let src = r#"
fn main() -> Int {
    let req = "PUT /a/b?c=d HTTP/1.1\r\nHost: example\r\nX-Mink-Test: yes\r\nContent-Length: 3\r\n\r\nabc";
    let parsed = http_server_parse(rt_str_concat(req, ""), "X-Mink-Test");
    let (method, path, version, declared, header, body) = parsed;
    let ok1 = rt_str_eq(method, "PUT");
    let ok2 = rt_str_eq(path, "/a/b?c=d");
    let ok3 = rt_str_eq(version, "HTTP/1.1");
    let ok4 = declared == 3;
    let ok5 = rt_str_eq(header, "yes");
    let ok6 = rt_str_eq(body, "abc");
    rt_str_free(method);
    rt_str_free(path);
    rt_str_free(version);
    rt_str_free(header);
    rt_str_free(body);
    if ok1 == true && ok2 == true && ok3 == true && ok4 == true && ok5 == true && ok6 == true {
        return 0;
    }
    return 1;
}
"#;
    assert_clean(src);
}

#[test]
fn hs05_request_copy_is_independent() {
    // A copy is byte-for-byte identical, and freeing it leaves the original
    // untouched (the copy exists precisely so a caller can hand out one
    // buffer per parsed field).
    let src = r#"
fn main() -> Int {
    let original = rt_str_concat("GET /x HTTP/1.1\r\n\r\n", "");
    let want_len = rt_str_len(original);
    let copy = http_server_request_copy(original);
    let same_len = rt_str_len(copy) == want_len;
    let same_bytes = rt_str_eq(copy, "GET /x HTTP/1.1\r\n\r\n");
    // The copy is an independent allocation: reading it again after the
    // original has been released (inside http_server_request_copy) still
    // gives the same bytes, so the caller can hand out one buffer per field.
    let n = rt_str_len(copy);
    let manual = rt_str_alloc(n);
    let mut i = 0;
    while i < n {
        rt_str_set_byte(manual, i, rt_str_byte(copy, i));
        i = i + 1;
    }
    let still_equal = rt_str_eq(manual, "GET /x HTTP/1.1\r\n\r\n");
    rt_str_free(manual);
    rt_str_free(copy);
    if same_len == true && same_bytes == true && still_equal == true { return 0; }
    return 1;
}
"#;
    assert_clean(src);
}

#[test]
fn hs06_response_has_exact_length_and_headers() {
    // The built response is checked byte for byte: status line, both headers
    // and a Content-Length that matches the body exactly.
    let src = r#"
fn main() -> Int {
    let response = http_server_response(200, "OK", "text/plain", "hello");
    let expected = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello";
    let ok = rt_str_eq(response, expected);
    rt_str_free(response);
    let text = http_server_response_text(404, "Not Found", "nope");
    let expected2 = "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 4\r\nConnection: close\r\n\r\nnope";
    let ok2 = rt_str_eq(text, expected2);
    rt_str_free(text);
    if ok == true && ok2 == true { return 0; }
    return 1;
}
"#;
    assert_clean(src);
}

#[test]
fn hs07_reason_phrases_and_empty_body() {
    // Known status codes have a reason phrase; an unknown one yields "", and
    // an empty body is announced as Content-Length: 0 rather than omitted.
    let src = r#"
fn main() -> Int {
    let ok1 = rt_str_eq(http_server_reason(200), "OK");
    let ok2 = rt_str_eq(http_server_reason(404), "Not Found");
    let ok3 = rt_str_eq(http_server_reason(500), "Internal Server Error");
    let ok4 = rt_str_len(http_server_reason(299)) == 0;
    let empty = http_server_ok("");
    let ok5 = rt_str_eq(
        empty,
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    rt_str_free(empty);
    if ok1 == true && ok2 == true && ok3 == true && ok4 == true && ok5 == true { return 0; }
    return 1;
}
"#;
    assert_clean(src);
}

// =========================================================================
// Live loopback server (hs08-hs10)
// =========================================================================

/// A free ephemeral TCP port, handed out exactly once per test process.
///
/// Binding `127.0.0.1:0` and dropping the listener frees the number again,
/// so under a parallel run two tests can be handed the same port and one
/// test's server answers the other's client. Numbers already handed out are
/// never returned twice, and a bounded scan is the fallback so the helper
/// itself can never spin.
fn free_port() -> u16 {
    static CLAIMED: Mutex<Vec<u16>> = Mutex::new(Vec::new());
    for _ in 0..64 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let mut claimed = CLAIMED.lock().unwrap();
        if !claimed.contains(&port) {
            claimed.push(port);
            return port;
        }
    }
    for port in 30_000u16..60_000 {
        let mut claimed = CLAIMED.lock().unwrap();
        if claimed.contains(&port) {
            continue;
        }
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            claimed.push(port);
            return port;
        }
    }
    panic!("no free loopback port could be reserved");
}

/// Joins a harness helper thread with a bound, so a socket mispairing fails
/// the test in bounded time instead of hanging the whole target.
fn join_bounded<T: Send + 'static>(handle: std::thread::JoinHandle<T>, what: &str) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(handle.join());
    });
    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(Ok(value)) => value,
        Ok(Err(_)) => panic!("{what}: harness helper thread panicked"),
        Err(_) => panic!("{what}: harness helper thread still blocked after 30s"),
    }
}

/// Connects to a loopback port, retrying until the peer is listening.
fn connect_when_listening(port: u16) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(stream) => return stream,
            Err(err) => {
                if Instant::now() >= deadline {
                    panic!("no listener on 127.0.0.1:{port} after 20s: {err}");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

/// Sends `request` and reads the whole reply until the peer closes.
fn round_trip(port: u16, request: &str) -> String {
    let mut stream = connect_when_listening(port);
    stream.write_all(request.as_bytes()).unwrap();
    stream.flush().unwrap();
    // The server closes after responding; shutdown the write half so a
    // request with no body is still delimited by Content-Length.
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply).unwrap();
    String::from_utf8_lossy(&reply).into_owned()
}

#[test]
fn hs08_server_parses_request_and_answers_with_exact_length() {
    // A MINK server takes two connections in a row; each reply echoes the
    // parsed method and path, so the Rust side verifies parsing and response
    // construction in one round trip.
    let port = free_port();
    let src = with_module(format!(
        r#"
fn main() -> Int {{
    let srv = http_server_listen(rt_str_concat("127.0.0.", "1"), {port});
    if srv == -1 {{ return 10; }}
    let mut served = 0;
    while served < 2 {{
        let client = http_server_accept(srv, 20000);
        if client == -1 {{ return 11; }}
        let request = http_server_read_request(client, 65536);
        if rt_str_len(request) == 0 {{ return 12; }}
        let parsed = http_server_parse(request, "X-Mink-Test");
        let (method, path, version, declared, header, body) = parsed;
        let reply = rt_str_format("{{}} {{}}", method, path, "");
        rt_str_free(method);
        rt_str_free(path);
        rt_str_free(version);
        rt_str_free(header);
        rt_str_free(body);
        let sent = http_server_respond_ok(client, reply);
        if sent < 0 {{ return 13; }}
        if declared < -1 {{ return 14; }}
        served = served + 1;
    }}
    http_server_close(srv);
    return 0;
}}
"#
    ));
    let name = std::thread::current().name().unwrap_or("program").replace("::", "_");
    let path = temp_source(&format!("{name}.mink"), &src);
    let build = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        build.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let exe = path.with_extension("exe");
    assert!(exe.exists(), "no executable produced");

    let server = std::thread::spawn(move || {
        let output = Command::new(&exe).output().unwrap();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        output.status.code().unwrap_or(-1)
    });

    let first = round_trip(
        port,
        "GET /index.html?q=1 HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Mink-Test: one\r\n\r\n",
    );
    assert_eq!(
        first,
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 19\r\nConnection: close\r\n\r\nGET /index.html?q=1",
        "first reply mismatch"
    );

    let second = round_trip(
        port,
        "POST /submit HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Mink-Test: two\r\nContent-Length: 5\r\n\r\nhello",
    );
    assert_eq!(
        second,
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 12\r\nConnection: close\r\n\r\nPOST /submit",
        "second reply mismatch"
    );

    let code = join_bounded(server, "MINK http server");
    assert_eq!(code, 0, "server must exit 0 with no live allocation");
}

#[test]
fn hs09_server_reads_a_post_body_in_several_chunks() {
    // A body larger than one receive buffer arrives across many reads; the
    // server must assemble it and report the declared length exactly.
    let port = free_port();
    let src = with_module(format!(
        r#"
fn main() -> Int {{
    let srv = http_server_listen(rt_str_concat("127.0.0.", "1"), {port});
    if srv == -1 {{ return 10; }}
    let client = http_server_accept(srv, 20000);
    if client == -1 {{ return 11; }}
    let request = http_server_read_request(client, 65536);
    if rt_str_len(request) == 0 {{ return 12; }}
    let n = rt_str_len(request);
    let copy = rt_str_alloc(n);
    let mut i = 0;
    while i < n {{
        rt_str_set_byte(copy, i, rt_str_byte(request, i));
        i = i + 1;
    }}
    let declared = http_server_content_length(copy);
    let body = http_server_body(request);
    let got = rt_str_len(body);
    let first = rt_str_byte(body, 0);
    let last = rt_str_byte(body, got - 1);
    let mut ok = declared == got;
    if first != 88 {{ ok = false; }}
    if last != 89 {{ ok = false; }}
    rt_str_free(body);
    let sent = http_server_respond_ok(client, rt_str_from_int(got));
    if sent < 0 {{ return 13; }}
    http_server_close(srv);
    if ok == true {{ return 0; }}
    return 14;
}}
"#
    ));
    let name = std::thread::current().name().unwrap_or("program").replace("::", "_");
    let path = temp_source(&format!("{name}.mink"), &src);
    let build = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        build.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let exe = path.with_extension("exe");
    let server = std::thread::spawn(move || {
        let output = Command::new(&exe).output().unwrap();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        output.status.code().unwrap_or(-1)
    });

    // 20 000 body bytes, written in 4 KiB chunks so the server needs five
    // or more receives, well past the single-read limit.
    let mut request = String::from("POST /big HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 20000\r\n\r\n");
    for i in 0..20_000 {
        request.push(if i % 2 == 0 { 'X' } else { 'Y' });
    }
    let reply = round_trip(port, &request);
    assert!(
        reply.ends_with("\r\n\r\n20000"),
        "reply must report the received body length, got {reply:?}"
    );
    let code = join_bounded(server, "MINK http server");
    assert_eq!(code, 0, "server must exit 0 with no live allocation");
}

#[test]
fn hs10_accept_timeout_and_closed_peer_report_failure() {
    // No client at all: http_server_accept must return -1 once the timeout
    // elapses rather than block forever. A peer that connects and closes
    // without sending anything must produce an empty request — the program
    // dials its own listener to produce exactly that — and http_server_serve
    // must turn the same situation into (-1, "") rather than handing back a
    // dead socket.
    let port = free_port();
    let src = format!(
        r#"
fn main() -> Int {{
    let srv = http_server_listen(rt_str_concat("127.0.0.", "1"), {port});
    if srv == -1 {{ return 10; }}
    // Nothing is connecting yet.
    let missed = http_server_accept(srv, 200);
    if missed != -1 {{ return 11; }}
    // Connect to our own listener and hang up without sending anything.
    let peer = net_dial(rt_str_concat("127.0.0.", "1"), {port});
    if peer == -1 {{ return 12; }}
    net_close(peer);
    let client = http_server_accept(srv, 20000);
    if client == -1 {{ return 13; }}
    let request = http_server_read_request(client, 65536);
    if rt_str_len(request) != 0 {{ return 14; }}
    rt_str_free(request);
    net_close(client);
    // The two-argument form applies the same rule to a quiet listener.
    let (sock, empty) = http_server_serve(srv, 200, 65536);
    if sock != -1 {{ return 15; }}
    if rt_str_len(empty) != 0 {{ return 16; }}
    http_server_close(srv);
    return 0;
}}
"#
    );
    assert_clean(&src);
}
