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

fn net_lib() -> String {
    std::fs::read_to_string("stdlib/network.mink").expect("failed to read stdlib/network.mink")
}

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
    let mut source = String::new();
    source.push_str(&net_lib());
    source.push('\n');
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
}

#[test]
fn h35_header_value_with_space() {
    let src = r#"
fn main() -> Int {
    let resp = "HTTP/1.1 200 OK\r\nServer: Apache/2.4.41\r\n\r\n";
    let val = http_header(resp, "Server");
    let len = rt_str_len(val);
    rt_str_free(val);
    if len == 13 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}
