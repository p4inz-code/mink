//! Comprehensive tests for the MINK HTTP library (Sessions 69-70).
//!
//! Tests URL parsing, HTTP request construction, response parsing,
//! header extraction, and client operations.
//!
//! MINK OWNERSHIP RULES FOR TESTS:
//! - Runtime intrinsics (rt_str_byte, rt_str_len, rt_str_concat, rt_str_free)
//!   do NOT consume their Str parameters. You can call rt_str_free after them.
//! - User functions with Str parameters DO consume the argument.
//!   You CANNOT call rt_str_free after passing a string to a user function.
//! - String literals are NOT heap-allocated and must NOT be freed.
//! - Chain string building using rt_str_concat (runtime intrinsic), not user functions.
//! - For user function tests, test with string literals (no leak) or test final result only.

use std::path::PathBuf;
use std::process::Command;

fn http_lib() -> String {
    std::fs::read_to_string("stdlib/http.mink").expect("failed to read stdlib/http.mink")
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mink_http_test_{}_{name}", std::process::id()));
    std::fs::write(&path, content).unwrap();
    path
}

fn build_and_run(test_body: &str) -> (i32, Vec<u8>, Vec<u8>) {
    // `stdlib/http.mink` declares `mod network;`, so the module loader pulls
    // in the network dependency itself (resolved from the bundled
    // `stdlib/`); the test no longer concatenates it by hand.
    let mut source = String::new();
    source.push_str(&http_lib());
    source.push('\n');
    source.push_str(test_body);
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
    (code, output.stdout, output.stderr)
}

fn native_exit_code(source: &str) -> i32 {
    let (code, _, _) = build_and_run(source);
    code
}

// ==========================================================================
// SECTION 1: URL Parsing
// ==========================================================================

#[test]
fn h01_url_host_simple() {
    let src = r#"
fn main() -> Int {
    let host = http_url_host("example.com:8080/path");
    let len = rt_str_len(host);
    rt_str_free(host);
    if len == 11 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h02_url_host_with_http() {
    let src = r#"
fn main() -> Int {
    let host = http_url_host("http://example.com/path");
    let len = rt_str_len(host);
    rt_str_free(host);
    if len == 11 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h03_url_host_with_port_and_path() {
    let src = r#"
fn main() -> Int {
    let host = http_url_host("http://example.com:9090/api/test");
    let len = rt_str_len(host);
    rt_str_free(host);
    if len == 11 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h04_url_port_default() {
    let src = r#"
fn main() -> Int {
    let port = http_url_port("example.com/path");
    if port == 80 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h05_url_port_custom() {
    let src = r#"
fn main() -> Int {
    let port = http_url_port("example.com:8080/path");
    if port == 8080 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h06_url_port_with_http_prefix() {
    let src = r#"
fn main() -> Int {
    let port = http_url_port("http://example.com:3000/api");
    if port == 3000 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h07_url_path_simple() {
    let src = r#"
fn main() -> Int {
    let path = http_url_path("example.com/api/test");
    let len = rt_str_len(path);
    rt_str_free(path);
    if len == 9 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h08_url_path_root() {
    let src = r#"
fn main() -> Int {
    let path = http_url_path("example.com");
    let len = rt_str_len(path);
    if len == 1 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h09_url_path_with_http_prefix() {
    let src = r#"
fn main() -> Int {
    let path = http_url_path("http://example.com:8080/api/v1");
    let len = rt_str_len(path);
    rt_str_free(path);
    if len == 7 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 2: Request Construction
// ==========================================================================

#[test]
fn h10_http_get_request() {
    // "GET /index.html HTTP/1.1\r\n" = 26 chars
    let src = r#"
fn main() -> Int {
    let req = http_get("/index.html");
    let len = rt_str_len(req);
    rt_str_free(req);
    if len == 26 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h11_http_request_custom_method() {
    // "HEAD /status HTTP/1.1\r\n" = 25 chars
    let src = r#"
fn main() -> Int {
    let req = http_request("HEAD", "/status");
    let len = rt_str_len(req);
    rt_str_free(req);
    if len == 23 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h12_http_add_header() {
    // Test with a string literal (no leak since literal isn't heap).
    // "POST / HTTP/1.1\r\nHost: x\r\n" = 27 chars
    let src = r#"
fn main() -> Int {
    let req = http_add_header("POST / HTTP/1.1\r\n", "Host", "x");
    let len = rt_str_len(req);
    rt_str_free(req);
    if len == 26 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h13_http_add_multiple_headers() {
    // Test multiple headers on a string literal base (no leak).
    // Verify by checking intermediate lengths.
    let src = r#"
fn main() -> Int {
    // "GET / HTTP/1.1\r\n" = 17
    let r1 = http_add_header("GET / HTTP/1.1\r\n", "Host", "x");
    // r1 = "GET / HTTP/1.1\r\nHost: x\r\n" = 17 + 9 = 26
    let l1 = rt_str_len(r1);
    rt_str_free(r1);
    if l1 == 25 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 3: Response Parsing (all use string literals - no leak)
// ==========================================================================

#[test]
fn h14_status_code_200() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
    let code = http_status_code(resp);
    if code == 200 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h15_status_code_404() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
    let code = http_status_code(resp);
    if code == 404 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h16_status_text_ok() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
    let text = http_status_text(resp);
    let len = rt_str_len(text);
    rt_str_free(text);
    if len == 2 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h17_status_text_not_found() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
    let text = http_status_text(resp);
    let len = rt_str_len(text);
    rt_str_free(text);
    if len == 9 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 4: Header Extraction (all use string literals)
// ==========================================================================

#[test]
fn h18_header_content_length() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\nContent-Length: 42\r\n\r\nbody";
    let val = http_header(resp, "Content-Length");
    let len = rt_str_len(val);
    rt_str_free(val);
    if len == 2 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h19_header_content_type() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n";
    let val = http_header(resp, "Content-Type");
    let len = rt_str_len(val);
    rt_str_free(val);
    if len == 9 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h20_header_case_insensitive() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n";
    let val = http_header(resp, "Content-Type");
    let len = rt_str_len(val);
    rt_str_free(val);
    if len == 16 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h21_header_not_found() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
    let val = http_header(resp, "X-Custom");
    let len = rt_str_len(val);
    if len == 0 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h22_header_multiple() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain\r\nX-Custom: value\r\n\r\nhello";
    let cl = http_header(resp, "Content-Length");
    let cl_len = rt_str_len(cl);
    rt_str_free(cl);
    let ct = http_header(resp, "Content-Type");
    let ct_len = rt_str_len(ct);
    rt_str_free(ct);
    let xc = http_header(resp, "X-Custom");
    let xc_len = rt_str_len(xc);
    rt_str_free(xc);
    if cl_len == 1 {
        if ct_len == 10 {
            if xc_len == 5 {
                return 1;
            }
        }
    }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 5: Body Extraction (all use string literals)
// ==========================================================================

#[test]
fn h23_body_simple() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
    let body = http_body(resp);
    let len = rt_str_len(body);
    rt_str_free(body);
    if len == 5 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h24_body_empty() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
    let body = http_body(resp);
    let len = rt_str_len(body);
    if len == 0 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h25_body_multiline() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\nContent-Length: 12\r\n\r\nhello\nworld";
    let body = http_body(resp);
    let len = rt_str_len(body);
    rt_str_free(body);
    if len == 11 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h26_content_length_func() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\nContent-Length: 1024\r\n\r\n";
    let cl = http_content_length(resp);
    if cl == 1024 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h27_content_length_missing() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\n\r\nbody";
    let cl = http_content_length(resp);
    if cl == -1 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 6: POST Request (builds request from string literal base)
// ==========================================================================

#[test]
fn h28_post_request_body() {
    // Build POST request from literal base to avoid consuming heap strings.
    // "POST /submit HTTP/1.1\r\nContent-Length: 11\r\n" = 41
    // + "Content-Type: application/x-www-form-urlencoded\r\n" = 50
    // + "Connection: close\r\n" = 20
    // + "\r\n" = 2
    // + "hello=world" = 11
    // = 124 total (but we verify by extracting parts)
    let src = r#"
fn main() -> Int {
    let resp = "POST /submit HTTP/1.1\r\nContent-Length: 11\r\nContent-Type: application/x-www-form-urlencoded\r\nConnection: close\r\n\r\nhello=world";
    let cl = http_header(resp, "Content-Length");
    let cl_len = rt_str_len(cl);
    rt_str_free(cl);
    let ct = http_header(resp, "Content-Type");
    let ct_len = rt_str_len(ct);
    rt_str_free(ct);
    let bd = http_body(resp);
    let bd_len = rt_str_len(bd);
    rt_str_free(bd);
    if cl_len == 2 {
        if ct_len == 33 {
            if bd_len == 11 {
                return 1;
            }
        }
    }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 7: Edge Cases
// ==========================================================================

#[test]
fn h29_status_code_201() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 201 Created\r\nLocation: /new\r\n\r\n";
    let code = http_status_code(resp);
    let text = http_status_text(resp);
    let text_len = rt_str_len(text);
    rt_str_free(text);
    if code == 201 {
        if text_len == 7 {
            return 1;
        }
    }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h30_status_code_500() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
    let code = http_status_code(resp);
    let text = http_status_text(resp);
    let text_len = rt_str_len(text);
    rt_str_free(text);
    if code == 500 {
        if text_len == 21 {
            return 1;
        }
    }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h31_header_with_colon_in_value() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\nWWW-Authenticate: Basic realm=\"test\"\r\n\r\n";
    let val = http_header(resp, "WWW-Authenticate");
    let len = rt_str_len(val);
    rt_str_free(val);
    if len == 18 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h32_header_empty_value() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\nX-Empty:\r\n\r\n";
    let val = http_header(resp, "X-Empty");
    let len = rt_str_len(val);
    if len == 0 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h33_url_path_no_host() {
    let src = r#"
fn main() -> Int {
    let path = http_url_path("http://localhost:3000/test");
    let len = rt_str_len(path);
    rt_str_free(path);
    if len == 5 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h34_request_line_format() {
    // "DELETE /resource/1 HTTP/1.1\r\n" = 29 chars
    let src = r#"
fn main() -> Int {
    let req = http_request("DELETE", "/resource/1");
    let len = rt_str_len(req);
    rt_str_free(req);
    if len == 29 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}#[test]
fn h35_header_value_with_space() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\nServer: Apache/2.4.41\r\n\r\n";
    let val = http_header(resp, "Server");
    let len = rt_str_len(val);
    rt_str_free(val);
    if len == 13 { return 1; }
    return 0;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 9: live request against a local fixture server (Session 125)
// ==========================================================================
//
// The rest of this file parses hand-written responses, which never exercises
// the socket path. These two cases drive `http_client_get` end to end against
// a server that lives in the test program itself: a thread bound before the
// client connects, so there is no timing race and no external dependency.

#[test]
fn h40_client_get_by_host_name() {
    // URL parsing, host-name resolution (getaddrinfo), connect, send, receive
    // and status parsing in one request. The URL carries a host name, not an
    // IPv4 literal, so the resolver path is part of what is verified.
    let src = r#"
mod threads;

fn fixture(p: Ptr<Int>) -> Int {
    let listener = rt_mem_load(p);
    let client = net_accept(listener);
    if client == -1 { return 0; }
    let request = net_recv(client, 1024);
    rt_str_free(request);
    net_send(client, "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello");
    net_shutdown(client, SD_BOTH());
    net_close(client);
    return 1;
}

fn main() -> Int {
    // Windows reserves whole port ranges (WSL/Hyper-V excluded ranges), so a
    // hardcoded port can fail to bind with WSAEACCES (10013). Try a few
    // candidates and use the first that binds.
    let ports = [19881, 19081, 21081, 25081, 17081, 16081];
    let mut listener = -1;
    let mut port = 0;
    let mut i = 0;
    while i < 6 && listener == -1 {
        let candidate = net_listen_on("127.0.0.1", ports[i], 1);
        if candidate != -1 {
            listener = candidate;
            port = ports[i];
        }
        i = i + 1;
    }
    if listener == -1 { return 2; }
    let p = rt_alloc(8);
    rt_mem_store(p, listener);
    let t = thread_spawn(fixture, p);
    let host = rt_str_from_int(port);
    let url = rt_str_concat("http://localhost:", host);
    rt_str_free(host); // rt_str_concat borrows its arguments
    let response = http_client_get(url);
    let code = http_status_code(response);
    let err = http_last_error();
    let served = thread_join(t);
    rt_free(p);
    net_close(listener);
    if code == 200 && err == 0 && served == 1 { return 1; }
    return 4;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn h42_client_get_initializes_winsock_standalone() {
    // Session 127: `http_client_get` must initialize Winsock itself. In a
    // fresh program with no in-program server — nothing else calls
    // `net_init` — the request used to fail with a socket error because
    // `http_send` called `net_tcp_socket` before any WSAStartup. The stdlib
    // client now calls `net_init` in `http_send`, so the standalone client is
    // a complete entry point as its own header documents.
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 512];
            let _ = stream.read(&mut buf);
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello");
            let _ = stream.flush();
        }
    });

    let src = format!(
        r#"
fn main() -> Int {{
    let response = http_client_get("http://127.0.0.1:{port}/x");
    let code = http_status_code(response);
    if code == 200 {{ return 42; }}
    return 0;
}}
"#,
        port = port
    );
    assert_eq!(native_exit_code(&src), 42);
    let _ = server.join();
}

#[test]
fn h41_client_get_refused_reports_error() {
    // The failure model: a request that cannot connect returns an empty
    // string and leaves the reason in http_last_error() (WSAECONNREFUSED),
    // so an empty result is never mistaken for a successful empty response.
    let src = r#"
fn main() -> Int {
    let response = http_client_get("http://localhost:19996/nothing");
    let len = rt_str_len(response);
    rt_str_free(response);
    let err = http_last_error();
    if len == 0 && err != 0 { return 1; }
    return 0;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 10: receive budget boundary (Session 132, M2 P4)
// ==========================================================================
//
// A complete 4 MiB body is exactly what MAX_BODY() advertises. The receive
// loop used to stop after 1024 chunks of at most 4096 bytes, which also have
// to carry the response headers, so a full-size body came back ~4 KiB short
// and was reported as HTTP_ERR_TOO_LARGE. An oversized response then read past
// the capacity-sized buffer (E-R09) instead of returning the documented
// TOO_LARGE prefix. This test drives both paths against a loopback server.
//
// Windows-only: a 4 MiB body plus the accumulation buffer and the extracted
// copy exceed the fixed 4 MiB Linux arena, so the case is not runnable there.

#[cfg(target_os = "windows")]
#[test]
fn h44_client_get_closed_without_response_reports_error() {
    // The failure model promises that an empty result means a failed request
    // and that http_last_error() holds the reason. A peer that accepts and
    // closes without sending a status line used to leave the error at 0, so an
    // aborted or empty transfer looked like a completed response. Session 135
    // (M2 P7): the receive path now reports HTTP_ERR_RECV (-5).
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            drop(stream);
        }
    });

    let src = format!(
        r#"
fn main() -> Int {{
    let response = http_client_get("http://127.0.0.1:{port}/x");
    let len = rt_str_len(response);
    rt_str_free(response);
    let err = http_last_error();
    if len == 0 && err == 0 - 5 {{ return 44; }}
    return 0;
}}
"#,
        port = port
    );
    assert_eq!(native_exit_code(&src), 44);
    let _ = server.join();
}

#[test]
fn h43_client_get_receives_full_max_body_and_reports_overflow() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    const MAX_BODY: usize = 4 * 1024 * 1024;
    const OVERFLOW: usize = MAX_BODY + 16 * 1024;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(60);
        let body: Vec<u8> = (0..MAX_BODY).map(|i| ((i * 131 + 17) & 0xFF) as u8).collect();
        for total in [MAX_BODY, OVERFLOW] {
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buf = [0u8; 512];
                        let _ = stream.read(&mut buf);
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n"
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&body[..total.min(body.len())]);
                        if total > body.len() {
                            // The overflow body continues the same pattern.
                            let tail: Vec<u8> = (body.len()..total)
                                .map(|i| ((i * 131 + 17) & 0xFF) as u8)
                                .collect();
                            let _ = stream.write_all(&tail);
                        }
                        let _ = stream.flush();
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => return,
                }
            }
        }
    });

    let src = format!(
        r#"
fn main() -> Int {{
    let response = http_client_get("http://127.0.0.1:{port}/full");
    let err = http_last_error();
    let body = http_body(response);
    let n = rt_str_len(body);
    if err != 0 {{ return 1; }}
    if n != {max} {{ return 2; }}
    if rt_str_byte(body, 0) != 17 {{ return 3; }}
    if rt_str_byte(body, n - 1) != (((n - 1) * 131 + 17) % 256) {{ return 4; }}
    rt_str_free(body);

    let response2 = http_client_get("http://127.0.0.1:{port}/over");
    let err2 = http_last_error();
    let body2 = http_body(response2);
    let n2 = rt_str_len(body2);
    if err2 != -4 {{ return 5; }}
    if n2 <= {max} {{ return 6; }}
    if n2 > {budget} {{ return 7; }}
    if rt_str_byte(body2, 0) != 17 {{ return 8; }}
    if rt_str_byte(body2, n2 - 1) != (((n2 - 1) * 131 + 17) % 256) {{ return 9; }}
    rt_str_free(body2);
    return 0;
}}
"#,
        port = port,
        max = MAX_BODY,
        budget = MAX_BODY + 8192,
    );
    assert_eq!(native_exit_code(&src), 0);
    let _ = server.join();
}
// ==========================================================================
// SECTION 11: Content-Length completeness (Session 137, M2 P8.1)
// ==========================================================================
//
// Session 136 (M2 P8) proved the defect deterministically: a server that
// declares `Content-Length: 524288` and sends 200000 bytes came back as
// status 200, a 200000-byte body and `http_last_error() == 0` — silent
// truncation presented as a completed request. The completion check added in
// `http_send` now reports the existing `HTTP_ERR_RECV` (-5, "the transfer did
// not complete") for exactly that case while leaving every response the old
// implementation accepted alone:
//   - a body that matches its declared length,
//   - a response with no Content-Length (the existing protocol behavior),
//   - a response that declares *less* than it sent (extra bytes are returned),
//   - a Transfer-Encoding response, whose framing is not
//     Content-Length-delimited (all chunked responses),
//   - a status that cannot carry a body (1xx/204/304) or a reply to HEAD.
//
// The fixtures below are raw loopback servers: the response bytes are written
// verbatim, so "declared more than sent" is expressed by the bytes themselves
// and no timing or internet access is involved. `serve_raw` closes each
// connection after writing, which is what makes the short body observable.

/// Serve `responses` one per connection, in order, on a fresh loopback port.
fn serve_raw(responses: Vec<Vec<u8>>) -> u16 {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for response in responses {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(&response);
                let _ = stream.flush();
            }
        }
    });
    port
}

/// The truncated Content-Length case from the P8 record, at P8's exact sizes.
#[test]
fn h45_client_get_truncated_content_length_reports_error() {
    const DECLARED: usize = 524288;
    const SENT: usize = 200000;

    let mut response =
        format!("HTTP/1.1 200 OK\r\nContent-Length: {DECLARED}\r\nConnection: close\r\n\r\n")
            .into_bytes();
    response.extend((0..SENT).map(|i| ((i * 131 + 17) & 0xFF) as u8));
    let port = serve_raw(vec![response]);

    let src = format!(
        r#"
fn main() -> Int {{
    let response = http_client_get("http://127.0.0.1:{port}/truncated");
    let err = http_last_error();
    let body = http_body(response);
    let n = rt_str_len(body);
    if err != 0 - 5 {{ return 1; }}
    if n != {sent} {{ return 2; }}
    if rt_str_byte(body, 0) != 17 {{ return 3; }}
    if rt_str_byte(body, n - 1) != (((n - 1) * 131 + 17) % 256) {{ return 4; }}
    rt_str_free(body);
    return 45;
}}
"#,
        port = port,
        sent = SENT,
    );
    assert_eq!(native_exit_code(&src), 45);
}

/// The control for h45: a body that matches its declared length is complete.
#[test]
fn h46_client_get_exact_content_length_completes() {
    const SIZE: usize = 200000;

    let mut response =
        format!("HTTP/1.1 200 OK\r\nContent-Length: {SIZE}\r\nConnection: close\r\n\r\n")
            .into_bytes();
    response.extend((0..SIZE).map(|i| ((i * 131 + 17) & 0xFF) as u8));
    let port = serve_raw(vec![response]);

    let src = format!(
        r#"
fn main() -> Int {{
    let response = http_client_get("http://127.0.0.1:{port}/exact");
    let err = http_last_error();
    let body = http_body(response);
    let n = rt_str_len(body);
    if err != 0 {{ return 1; }}
    if n != {size} {{ return 2; }}
    if rt_str_byte(body, 0) != 17 {{ return 3; }}
    if rt_str_byte(body, n - 1) != (((n - 1) * 131 + 17) % 256) {{ return 4; }}
    rt_str_free(body);
    return 46;
}}
"#,
        port = port,
        size = SIZE,
    );
    assert_eq!(native_exit_code(&src), 46);
}

/// A response that declares *less* than it sent keeps the previous behavior:
/// the extra bytes are returned and the request is a success. The completion
/// check is strictly `received < declared`, so it must not fire here.
#[test]
fn h47_client_get_declared_shorter_than_sent_is_unchanged() {
    const DECLARED: usize = 100;
    const SENT: usize = 200;

    let mut response =
        format!("HTTP/1.1 200 OK\r\nContent-Length: {DECLARED}\r\nConnection: close\r\n\r\n")
            .into_bytes();
    response.extend((0..SENT).map(|i| ((i * 131 + 17) & 0xFF) as u8));
    let port = serve_raw(vec![response]);

    let src = format!(
        r#"
fn main() -> Int {{
    let response = http_client_get("http://127.0.0.1:{port}/over");
    let err = http_last_error();
    let body = http_body(response);
    let n = rt_str_len(body);
    if err != 0 {{ return 1; }}
    if n != {sent} {{ return 2; }}
    if rt_str_byte(body, 0) != 17 {{ return 3; }}
    if rt_str_byte(body, n - 1) != (((n - 1) * 131 + 17) % 256) {{ return 4; }}
    rt_str_free(body);
    return 47;
}}
"#,
        port = port,
        sent = SENT,
    );
    assert_eq!(native_exit_code(&src), 47);
}

/// A response with no Content-Length is accepted by the existing protocol
/// behavior (body = everything up to the close), which the check must not
/// change. The second response is a 204 whose Content-Length describes a body
/// that a bodyless status deliberately does not send.
#[test]
fn h48_client_get_without_content_length_is_unchanged() {
    let port = serve_raw(vec![
        b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nhello".to_vec(),
        b"HTTP/1.1 204 No Content\r\nContent-Length: 9\r\nConnection: close\r\n\r\n".to_vec(),
    ]);

    let src = format!(
        r#"
fn main() -> Int {{
    let a = http_client_get("http://127.0.0.1:{port}/plain");
    let err_a = http_last_error();
    let body_a = http_body(a);
    let n_a = rt_str_len(body_a);
    if err_a != 0 {{ return 1; }}
    if n_a != 5 {{ return 2; }}
    if rt_str_byte(body_a, 0) != 104 {{ return 3; }}
    rt_str_free(body_a);

    let b = http_client_get("http://127.0.0.1:{port}/nocontent");
    let err_b = http_last_error();
    let body_b = http_body(b);
    let n_b = rt_str_len(body_b);
    if err_b != 0 {{ return 4; }}
    if n_b != 0 {{ return 5; }}
    rt_str_free(body_b);
    return 48;
}}
"#,
        port = port,
    );
    assert_eq!(native_exit_code(&src), 48);
}

/// A chunked response is decoded by `http_body` (Session 147): the framed
/// bytes never reach the caller, and a Transfer-Encoding response is still
/// exempt from the Content-Length completeness check (its framing is not
/// Content-Length-delimited), even when an inconsistent Content-Length is
/// present.
#[test]
fn h49_client_get_chunked_response_is_decoded() {
    let chunked =
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nabc\r\n0\r\n\r\n";
    let chunked_with_cl = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 99\r\nConnection: close\r\n\r\n3\r\nxyz\r\n0\r\n\r\n";
    let port = serve_raw(vec![chunked.to_vec(), chunked_with_cl.to_vec()]);

    let src = format!(
        r#"
fn main() -> Int {{
    let a = http_client_get("http://127.0.0.1:{port}/chunked");
    let err_a = http_last_error();
    let body_a = http_body(a);
    let n_a = rt_str_len(body_a);
    if err_a != 0 {{ return 1; }}
    if n_a != 3 {{ return 2; }}
    if rt_str_byte(body_a, 0) != 97 {{ return 3; }}
    if rt_str_byte(body_a, 1) != 98 {{ return 4; }}
    if rt_str_byte(body_a, 2) != 99 {{ return 5; }}
    rt_str_free(body_a);

    let b = http_client_get("http://127.0.0.1:{port}/chunked_cl");
    let err_b = http_last_error();
    let body_b = http_body(b);
    let n_b = rt_str_len(body_b);
    if err_b != 0 {{ return 6; }}
    if n_b != 3 {{ return 7; }}
    if rt_str_byte(body_b, 0) != 120 {{ return 8; }}
    rt_str_free(body_b);
    return 49;
}}
"#,
        port = port
    );
    assert_eq!(native_exit_code(&src), 49);
}

/// A HEAD reply declares the length the body *would* have and sends none, so
/// it is exempt from the completion check. The GET control shares the same
/// declared length and really sends the body.
#[test]
fn h50_client_head_without_body_stays_successful() {
    let server_bytes = vec![
        b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello".to_vec(),
    ];
    let port = serve_raw(server_bytes);

    let src = format!(
        r#"
fn main() -> Int {{
    let req = http_request("HEAD", "/head");
    let r = http_send(req, "127.0.0.1", {port});
    let err = http_last_error();
    let n = rt_str_len(r);
    rt_str_free(r);
    if err != 0 {{ return 1; }}
    if n == 0 {{ return 2; }}

    let g = http_client_get("http://127.0.0.1:{port}/head");
    let err_g = http_last_error();
    let body = http_body(g);
    let n_g = rt_str_len(body);
    if err_g != 0 {{ return 3; }}
    if n_g != 5 {{ return 4; }}
    rt_str_free(body);
    return 50;
}}
"#,
        port = port,
    );
    assert_eq!(native_exit_code(&src), 50);
}

// ==========================================================================
// SECTION 12: last-error thread locality (Session 144, M3 P4)
// ==========================================================================
//
// `http_last_error()` used to read a module-level `let mut` word, so it was
// process-global: any thread's request overwrote the reason every other
// thread observed. The failure model (an empty response plus the reason in
// `http_last_error()`) is only usable if the reason belongs to the thread
// that made the request. These two programs drive a deterministic
// cross-thread case and a bounded repetition stress.

/// Both workers complete their request, then rendezvous on a two-arrival
/// barrier, and only then read `http_last_error()`. With a shared word both
/// reads see whichever request finished last, so at least one worker must
/// observe the other thread's code — deterministically, no timing luck.
#[cfg(target_os = "windows")]
#[test]
fn h51_client_last_error_is_thread_local() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    // The successful request is served by a loopback fixture; the failing
    // request targets a port that was bound and immediately released, so its
    // connect is refused (HTTP_ERR_CONNECT, -2) without any internet access.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let good_port = listener.local_addr().unwrap().port();
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let closed_port = closed.local_addr().unwrap().port();
    drop(closed);
    let server = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 512];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
            );
            let _ = stream.flush();
        }
    });

    let src = format!(
        r#"
fn m3p4_barrier(p: Ptr<Int>) {{
    let lock = rt_mem_load(p);
    rt_mutex_lock(lock);
    rt_mem_store(p + 8, rt_mem_load(p + 8) + 1);
    rt_mutex_unlock(lock);
    while rt_mem_load(p + 8) < 2 {{
        rt_sleep(1);
    }}
}}

fn m3p4_fail(p: Ptr<Int>) -> Int {{
    rt_mem_store(p + 32, http_last_error());
    let r = http_client_get("http://127.0.0.1:{closed}/x");
    rt_str_free(r);
    m3p4_barrier(p);
    rt_mem_store(p + 16, http_last_error());
    return 0;
}}

fn m3p4_ok(p: Ptr<Int>) -> Int {{
    let r = http_client_get("http://127.0.0.1:{good}/x");
    rt_str_free(r);
    m3p4_barrier(p);
    rt_mem_store(p + 24, http_last_error());
    return 0;
}}

fn main() -> Int {{
    let p = rt_alloc(40);
    rt_mem_store(p, rt_mutex_new());
    rt_mem_store(p + 8, 0);
    rt_mem_store(p + 16, -999);
    rt_mem_store(p + 24, -999);
    rt_mem_store(p + 32, -999);
    let t1 = rt_thread_spawn(m3p4_fail, p);
    let t2 = rt_thread_spawn(m3p4_ok, p);
    rt_thread_join(t1);
    rt_thread_join(t2);
    let fail_err = rt_mem_load(p + 16);
    let ok_err = rt_mem_load(p + 24);
    let fresh_err = rt_mem_load(p + 32);
    rt_mutex_free(rt_mem_load(p));
    rt_free(p);
    if fresh_err != 0 {{ return 3; }}
    if fail_err != -2 {{ return 1; }}
    if ok_err != 0 {{ return 2; }}
    return 51;
}}
"#,
        closed = closed_port,
        good = good_port,
    );
    assert_eq!(native_exit_code(&src), 51);
    let _ = server.join();
}

/// Two threads repeat their own failure/success 6 times and compare each
/// observed code with the code their own request produces. The two values
/// differ (-2 vs 0), so any cross-thread overwrite is counted. (A bounded
/// sample: h51 is the deterministic proof.)
#[cfg(target_os = "windows")]
#[test]
fn h52_client_last_error_stays_thread_local_under_repetition() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    const ROUNDS: usize = 6;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let good_port = listener.local_addr().unwrap().port();
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let closed_port = closed.local_addr().unwrap().port();
    drop(closed);
    let server = std::thread::spawn(move || {
        for _ in 0..ROUNDS {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 512];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
            );
            let _ = stream.flush();
        }
    });

    let src = format!(
        r#"
fn m3p4_note_wrong(p: Ptr<Int>) {{
    let lock = rt_mem_load(p);
    rt_mutex_lock(lock);
    rt_mem_store(p + 8, rt_mem_load(p + 8) + 1);
    rt_mutex_unlock(lock);
}}

fn m3p4_fail_loop(p: Ptr<Int>) -> Int {{
    let mut i = 0;
    while i < {rounds} {{
        let r = http_client_get("http://127.0.0.1:{closed}/x");
        rt_str_free(r);
        if http_last_error() != -2 {{ m3p4_note_wrong(p); }}
        i = i + 1;
    }}
    return 0;
}}

fn m3p4_ok_loop(p: Ptr<Int>) -> Int {{
    let mut i = 0;
    while i < {rounds} {{
        let r = http_client_get("http://127.0.0.1:{good}/x");
        rt_str_free(r);
        if http_last_error() != 0 {{ m3p4_note_wrong(p); }}
        i = i + 1;
    }}
    return 0;
}}

fn main() -> Int {{
    let p = rt_alloc(16);
    rt_mem_store(p, rt_mutex_new());
    rt_mem_store(p + 8, 0);
    let t1 = rt_thread_spawn(m3p4_fail_loop, p);
    let t2 = rt_thread_spawn(m3p4_ok_loop, p);
    rt_thread_join(t1);
    rt_thread_join(t2);
    let wrong = rt_mem_load(p + 8);
    rt_print_int(wrong);
    rt_mutex_free(rt_mem_load(p));
    rt_free(p);
    if wrong == 0 {{ return 52; }}
    return 0;
}}
"#,
        rounds = ROUNDS,
        closed = closed_port,
        good = good_port,
    );
    let (code, out, _) = build_and_run(&src);
    assert_eq!(
        code, 52,
        "cross-thread last-error overwrites: stdout={:?}",
        String::from_utf8_lossy(&out)
    );
    let _ = server.join();
}

/// One thread, sequential requests: a repeated connect failure keeps its
/// code, a different failure class (peer closes without a response) replaces
/// it, a completed request clears it to 0, and a later failure replaces that.
/// This is the single-thread half of the matrix; no internet access.
#[cfg(target_os = "windows")]
#[test]
fn h53_client_last_error_tracks_the_last_operation() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let good = TcpListener::bind("127.0.0.1:0").unwrap();
    let good_port = good.local_addr().unwrap().port();
    let dropping = TcpListener::bind("127.0.0.1:0").unwrap();
    let drop_port = dropping.local_addr().unwrap().port();
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let closed_port = closed.local_addr().unwrap().port();
    drop(closed);

    let good_server = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = good.accept() {
            let mut buf = [0u8; 512];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
            );
            let _ = stream.flush();
        }
    });
    let drop_server = std::thread::spawn(move || {
        if let Ok((stream, _)) = dropping.accept() {
            drop(stream);
        }
    });

    let src = format!(
        r#"
fn m3p4_get(url: Str) -> Int {{
    let r = http_client_get(url);
    rt_str_free(r);
    return http_last_error();
}}

fn main() -> Int {{
    if m3p4_get("http://127.0.0.1:{closed}/a") != -2 {{ return 1; }}
    if m3p4_get("http://127.0.0.1:{closed}/b") != -2 {{ return 2; }}
    if m3p4_get("http://127.0.0.1:{drop}/c") != -5 {{ return 3; }}
    if m3p4_get("http://127.0.0.1:{good}/d") != 0 {{ return 4; }}
    if m3p4_get("http://127.0.0.1:{closed}/e") != -2 {{ return 5; }}
    return 53;
}}
"#,
        closed = closed_port,
        drop = drop_port,
        good = good_port,
    );
    assert_eq!(native_exit_code(&src), 53);
    let _ = good_server.join();
    let _ = drop_server.join();
}
// ==========================================================================
// SECTION 14: chunked transfer decoding (Session 147)
// ==========================================================================
//
// `http_body` now decodes a `Transfer-Encoding: chunked` body instead of
// returning the framing. The receive helper already accumulates the whole
// response until peer close, so the decoder is a walk over bytes, never over
// packets: a chunk-size line, a payload or its CRLF may be split across any
// number of receives. The fragmented cases below make the server write in
// separate segments (with a 1 ms pause to encourage separate TCP sends); the
// decoded result must be identical either way.
//
// Contract pinned here:
//   - hex sizes are case-insensitive; `;extensions` are ignored;
//   - trailers are consumed and never appended to the body;
//   - malformed framing (bad size, missing CRLF, premature EOF, no zero
//     chunk) is an empty body plus HTTP_ERR_RECV (-5), never partial data;
//   - a decoded body beyond MAX_BODY is an empty body plus
//     HTTP_ERR_TOO_LARGE (-4), and an absurd size is never an allocation;
//   - non-chunked responses (Content-Length or close-delimited, including a
//     Transfer-Encoding coding this client does not decode) are unchanged.

/// Serve each connection's `pieces` as separate writes, in order, on a fresh
/// loopback port. A 1 ms pause between pieces encourages separate TCP sends;
/// the client reads until close, so correctness never depends on the split.
fn serve_fragmented(connections: Vec<Vec<Vec<u8>>>) -> u16 {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for pieces in connections {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                for piece in pieces {
                    let _ = stream.write_all(&piece);
                    let _ = stream.flush();
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    });
    port
}

// H1 — two normal chunks produce one contiguous body.
#[test]
fn h55_chunked_multiple_chunks_are_concatenated() {
    let src = r#"
fn main() -> Int {
    let b = http_body("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n");
    let n = rt_str_len(b);
    if http_last_error() != 0 { return 1; }
    if n != 9 { return 2; }
    if rt_str_byte(b, 0) != 87 { return 3; }  // 'W'
    if rt_str_byte(b, 4) != 112 { return 4; } // 'p'
    if rt_str_byte(b, 8) != 97 { return 5; }  // 'a'
    rt_str_free(b);
    return 55;
}
"#;
    assert_eq!(native_exit_code(src), 55);
}

// H2 + extensions — one chunk with an upper-case hex size and a `;foo=bar`
// extension: the extension is ignored, never parsed as part of the size.
#[test]
fn h56_chunked_single_chunk_extension_and_upper_hex() {
    let src = r#"
fn main() -> Int {
    let b = http_body("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nA;foo=bar\r\n0123456789\r\n0\r\n\r\n");
    let n = rt_str_len(b);
    if http_last_error() != 0 { return 1; }
    if n != 10 { return 2; }
    if rt_str_byte(b, 0) != 48 { return 3; }
    if rt_str_byte(b, 9) != 57 { return 4; }
    rt_str_free(b);
    return 56;
}
"#;
    assert_eq!(native_exit_code(src), 56);
}

// H4 — lower-case hex, and a case-insensitive header name/value pair.
#[test]
fn h57_chunked_lower_hex_and_case_insensitive_header() {
    let src = r#"
fn main() -> Int {
    let b = http_body("HTTP/1.1 200 OK\r\ntransfer-encoding: CHUNKED\r\n\r\n1a\r\nabcdefghijklmnopqrstuvwxyz\r\n0\r\n\r\n");
    let n = rt_str_len(b);
    if http_last_error() != 0 { return 1; }
    if n != 26 { return 2; }
    if rt_str_byte(b, 0) != 97 { return 3; }
    if rt_str_byte(b, 25) != 122 { return 4; }
    rt_str_free(b);
    return 57;
}
"#;
    assert_eq!(native_exit_code(src), 57);
}

// H3 — a valid empty chunked body is zero bytes with no error.
#[test]
fn h58_chunked_empty_body_is_empty_success() {
    let src = r#"
fn main() -> Int {
    let b = http_body("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n");
    let n = rt_str_len(b);
    if http_last_error() != 0 { return 1; }
    if n != 0 { return 2; }
    rt_str_free(b);
    return 58;
}
"#;
    assert_eq!(native_exit_code(src), 58);
}

// H10 — trailers after the zero chunk are consumed, never appended.
#[test]
fn h59_chunked_trailers_are_ignored() {
    let src = r#"
fn main() -> Int {
    let b = http_body("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\nX-Trailer: value\r\nAnother: 1\r\n\r\n");
    let n = rt_str_len(b);
    if http_last_error() != 0 { return 1; }
    if n != 3 { return 2; }
    if rt_str_byte(b, 0) != 97 { return 3; }
    if rt_str_byte(b, 2) != 99 { return 4; }
    rt_str_free(b);
    return 59;
}
"#;
    assert_eq!(native_exit_code(src), 59);
}

// H11 — a non-hex chunk size is malformed, not zero and not partial data.
#[test]
fn h60_chunked_malformed_size_reports_recv_error() {
    let src = r#"
fn main() -> Int {
    let b = http_body("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\nabc\r\n0\r\n\r\n");
    let n = rt_str_len(b);
    if http_last_error() != 0 - 5 { return 1; }
    if n != 0 { return 2; }
    rt_str_free(b);
    return 60;
}
"#;
    assert_eq!(native_exit_code(src), 60);
}

// H12 — the CRLF that must close the chunk payload is missing.
#[test]
fn h61_chunked_missing_payload_crlf_reports_recv_error() {
    let src = r#"
fn main() -> Int {
    let b = http_body("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabcX\r\n0\r\n\r\n");
    let n = rt_str_len(b);
    if http_last_error() != 0 - 5 { return 1; }
    if n != 0 { return 2; }
    rt_str_free(b);
    return 61;
}
"#;
    assert_eq!(native_exit_code(src), 61);
}

// H13 — the transfer ends inside the declared chunk payload.
#[test]
fn h62_chunked_premature_eof_inside_payload() {
    let src = r#"
fn main() -> Int {
    let b = http_body("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nA\r\nabc");
    let n = rt_str_len(b);
    if http_last_error() != 0 - 5 { return 1; }
    if n != 0 { return 2; }
    rt_str_free(b);
    return 62;
}
"#;
    assert_eq!(native_exit_code(src), 62);
}

// H14 — the payload arrived but its closing CRLF (and the zero chunk) did not.
#[test]
fn h63_chunked_premature_eof_after_payload() {
    let src = r#"
fn main() -> Int {
    let b = http_body("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc");
    let n = rt_str_len(b);
    if http_last_error() != 0 - 5 { return 1; }
    if n != 0 { return 2; }
    rt_str_free(b);
    return 63;
}
"#;
    assert_eq!(native_exit_code(src), 63);
}

// H17 — an absurd hex size is bounded by MAX_BODY, never used as an
// allocation, and reports TOO_LARGE rather than overflow.
#[test]
fn h64_chunked_absurd_size_is_bounded() {
    let src = r#"
fn main() -> Int {
    let b = http_body("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFFFFFFFFFFF\r\nZ");
    let n = rt_str_len(b);
    if http_last_error() != 0 - 4 { return 1; }
    if n != 0 { return 2; }
    rt_str_free(b);
    return 64;
}
"#;
    assert_eq!(native_exit_code(src), 64);
}

// H9 — a chunked body without its terminating zero chunk is incomplete.
#[test]
fn h65_chunked_missing_zero_chunk_reports_recv_error() {
    let src = r#"
fn main() -> Int {
    let b = http_body("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n");
    let n = rt_str_len(b);
    if http_last_error() != 0 - 5 { return 1; }
    if n != 0 { return 2; }
    rt_str_free(b);
    return 65;
}
"#;
    assert_eq!(native_exit_code(src), 65);
}

// H18 — non-chunked responses are unchanged: Content-Length bodies and a
// Transfer-Encoding coding this client does not decode stay raw.
#[test]
fn h66_non_chunked_bodies_are_unchanged() {
    let src = r#"
fn main() -> Int {
    let a = http_body("HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");
    let na = rt_str_len(a);
    if http_last_error() != 0 { return 1; }
    if na != 5 { return 2; }
    if rt_str_byte(a, 0) != 104 { return 3; }
    rt_str_free(a);

    let b = http_body("HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\nhello");
    let nb = rt_str_len(b);
    if http_last_error() != 0 { return 4; }
    if nb != 5 { return 5; }
    if rt_str_byte(b, 4) != 111 { return 6; }
    rt_str_free(b);
    return 66;
}
"#;
    assert_eq!(native_exit_code(src), 66);
}

// H5 — the chunk-size line is split across receives.
#[test]
fn h67_chunked_size_line_fragmented_across_receives() {
    let port = serve_fragmented(vec![vec![
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec(),
        b"3".to_vec(),
        b"\r".to_vec(),
        b"\n".to_vec(),
        b"abc".to_vec(),
        b"\r\n0\r\n\r\n".to_vec(),
    ]]);
    let src = format!(
        r#"
fn main() -> Int {{
    let r = http_client_get("http://127.0.0.1:{port}/frag-size");
    let err = http_last_error();
    let b = http_body(r);
    let n = rt_str_len(b);
    if err != 0 {{ return 1; }}
    if n != 3 {{ return 2; }}
    if rt_str_byte(b, 0) != 97 {{ return 3; }}
    if rt_str_byte(b, 2) != 99 {{ return 4; }}
    rt_str_free(b);
    return 67;
}}
"#,
        port = port
    );
    assert_eq!(native_exit_code(&src), 67);
}

// H6 — the chunk payload is split across receives.
#[test]
fn h68_chunked_payload_fragmented_across_receives() {
    let port = serve_fragmented(vec![vec![
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec(),
        b"6\r\n".to_vec(),
        b"ab".to_vec(),
        b"cd".to_vec(),
        b"ef".to_vec(),
        b"\r\n0\r\n\r\n".to_vec(),
    ]]);
    let src = format!(
        r#"
fn main() -> Int {{
    let r = http_client_get("http://127.0.0.1:{port}/frag-payload");
    let err = http_last_error();
    let b = http_body(r);
    let n = rt_str_len(b);
    if err != 0 {{ return 1; }}
    if n != 6 {{ return 2; }}
    if rt_str_byte(b, 0) != 97 {{ return 3; }}
    if rt_str_byte(b, 1) != 98 {{ return 4; }}
    if rt_str_byte(b, 4) != 101 {{ return 5; }}
    if rt_str_byte(b, 5) != 102 {{ return 6; }}
    rt_str_free(b);
    return 68;
}}
"#,
        port = port
    );
    assert_eq!(native_exit_code(&src), 68);
}

// H7 — the CRLF that closes the payload is split across receives.
#[test]
fn h69_chunked_payload_crlf_fragmented_across_receives() {
    let port = serve_fragmented(vec![vec![
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec(),
        b"3\r\nabc".to_vec(),
        b"\r".to_vec(),
        b"\n".to_vec(),
        b"0\r\n\r\n".to_vec(),
    ]]);
    let src = format!(
        r#"
fn main() -> Int {{
    let r = http_client_get("http://127.0.0.1:{port}/frag-crlf");
    let err = http_last_error();
    let b = http_body(r);
    let n = rt_str_len(b);
    if err != 0 {{ return 1; }}
    if n != 3 {{ return 2; }}
    if rt_str_byte(b, 0) != 97 {{ return 3; }}
    if rt_str_byte(b, 2) != 99 {{ return 4; }}
    rt_str_free(b);
    return 69;
}}
"#,
        port = port
    );
    assert_eq!(native_exit_code(&src), 69);
}

// H8 — three chunks, boundaries at arbitrary positions, including the zero
// chunk and its final CRLF.
#[test]
fn h70_chunked_multiple_chunks_fragmented_arbitrarily() {
    let port = serve_fragmented(vec![vec![
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec(),
        b"1".to_vec(),
        b"\r\nA".to_vec(),
        b"\r\n".to_vec(),
        b"2\r".to_vec(),
        b"\nBC\r\n".to_vec(),
        b"3\r\nD".to_vec(),
        b"EF\r".to_vec(),
        b"\n0".to_vec(),
        b"\r\n".to_vec(),
        b"\r".to_vec(),
        b"\n".to_vec(),
    ]]);
    let src = format!(
        r#"
fn main() -> Int {{
    let r = http_client_get("http://127.0.0.1:{port}/frag-any");
    let err = http_last_error();
    let b = http_body(r);
    let n = rt_str_len(b);
    if err != 0 {{ return 1; }}
    if n != 6 {{ return 2; }}
    if rt_str_byte(b, 0) != 65 {{ return 3; }}
    if rt_str_byte(b, 1) != 66 {{ return 4; }}
    if rt_str_byte(b, 2) != 67 {{ return 5; }}
    if rt_str_byte(b, 3) != 68 {{ return 6; }}
    if rt_str_byte(b, 4) != 69 {{ return 7; }}
    if rt_str_byte(b, 5) != 70 {{ return 8; }}
    rt_str_free(b);
    return 70;
}}
"#,
        port = port
    );
    assert_eq!(native_exit_code(&src), 70);
}

// H20 — repeated chunked requests on independent connections each decode
// correctly; no state survives from one response to the next.
#[test]
fn h71_repeated_chunked_requests_stay_independent() {
    let one =
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n1\r\nA\r\n0\r\n\r\n";
    let two = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n2\r\nBC\r\n0\r\n\r\n";
    let three = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nDEF\r\n0\r\n\r\n";
    let port = serve_raw(vec![one.to_vec(), two.to_vec(), three.to_vec()]);

    let src = format!(
        r#"
fn main() -> Int {{
    let a = http_client_get("http://127.0.0.1:{port}/one");
    let ea = http_last_error();
    let ba = http_body(a);
    let na = rt_str_len(ba);
    if ea != 0 {{ return 1; }}
    if na != 1 {{ return 2; }}
    if rt_str_byte(ba, 0) != 65 {{ return 3; }}
    rt_str_free(ba);

    let b = http_client_get("http://127.0.0.1:{port}/two");
    let eb = http_last_error();
    let bb = http_body(b);
    let nb = rt_str_len(bb);
    if eb != 0 {{ return 4; }}
    if nb != 2 {{ return 5; }}
    if rt_str_byte(bb, 1) != 67 {{ return 6; }}
    rt_str_free(bb);

    let c = http_client_get("http://127.0.0.1:{port}/three");
    let ec = http_last_error();
    let bc = http_body(c);
    let nc = rt_str_len(bc);
    if ec != 0 {{ return 7; }}
    if nc != 3 {{ return 8; }}
    if rt_str_byte(bc, 2) != 70 {{ return 9; }}
    rt_str_free(bc);
    return 71;
}}
"#,
        port = port
    );
    assert_eq!(native_exit_code(&src), 71);
}

/// Build a chunked response whose decoded body is `chunks * 65_536` bytes,
/// optionally plus one extra 1-byte chunk. Each chunk's bytes are a distinct
/// value so the test can verify the first byte of three different chunks.
fn chunked_response(chunks: usize, extra_byte: bool) -> Vec<u8> {
    let mut response = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
        .to_vec();
    for i in 0..chunks {
        response.extend(format!("10000\r\n").as_bytes());
        let fill = ((i * 37 + 11) & 0xFF) as u8;
        response.extend(std::iter::repeat(fill).take(65536));
        response.extend(b"\r\n");
    }
    if extra_byte {
        response.extend(b"1\r\nZ\r\n");
    }
    response.extend(b"0\r\n\r\n");
    response
}

// H15 — a chunked body that decodes to exactly MAX_BODY (4,194,304 bytes) is
// accepted whole: the limit applies to the decoded body, not the wire bytes.
#[test]
fn h72_chunked_body_at_exact_limit_is_accepted() {
    let port = serve_raw(vec![chunked_response(64, false)]);
    let src = format!(
        r#"
fn main() -> Int {{
    let r = http_client_get("http://127.0.0.1:{port}/at-limit");
    let err = http_last_error();
    let b = http_body(r);
    let n = rt_str_len(b);
    if err != 0 {{ return 1; }}
    if n != 4194304 {{ return 2; }}
    if rt_str_byte(b, 0) != {first} {{ return 3; }}
    if rt_str_byte(b, 65536) != {second} {{ return 4; }}
    if rt_str_byte(b, n - 1) != {last} {{ return 5; }}
    rt_str_free(b);
    return 72;
}}
"#,
        port = port,
        first = 11,
        second = 48,
        last = (63 * 37 + 11) & 0xFF,
    );
    assert_eq!(native_exit_code(&src), 72);
}

// H16 — one byte past MAX_BODY in decoded form fails with TOO_LARGE and never
// returns a partial body, even though the wire response fits the receive
// budget.
#[test]
fn h73_chunked_body_over_limit_reports_too_large() {
    let port = serve_raw(vec![chunked_response(64, true)]);
    let src = format!(
        r#"
fn main() -> Int {{
    let r = http_client_get("http://127.0.0.1:{port}/over-limit");
    let b = http_body(r);
    let err = http_last_error();
    let n = rt_str_len(b);
    if err != 0 - 4 {{ return 1; }}
    if n != 0 {{ return 2; }}
    rt_str_free(b);
    return 73;
}}
"#,
        port = port
    );
    assert_eq!(native_exit_code(&src), 73);
}
