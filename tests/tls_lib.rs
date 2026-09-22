#![cfg(windows)]
//! TLS / HTTPS integration tests — Session 117 (S62).
//!
//! Python parity target: `ssl` client sockets and `http.client`
//! `HTTPSConnection` — a TLS client that negotiates with the Windows Schannel
//! stack, validates the peer certificate against a pinned trust anchor, checks
//! the host name against the certificate's `subjectAltName` entries, and speaks
//! HTTP over the resulting record layer.
//!
//! Every test prepends `stdlib/tls.mink`, compiles a genuine Windows PE
//! through the real `mink build` CLI and executes it. A clean exit (code 0) is
//! also the ownership/leak proof: the runtime arena is validated on exit and
//! reports `E-R06` (exit code 106) when anything is still owned.
//!
//! Test infrastructure is deterministic and local: `tests/tls/server.py` is a
//! Python TLS endpoint that uses the committed fixtures from
//! `tests/tls/make_certs.py` (an RSA test CA plus leaves with known validity,
//! usage and SANs) and prints the port it bound on stdout. Nothing here
//! contacts the public internet except `s62_system_store_public_https`, which
//! is `#[ignore]`d for that reason. Certificate verification is never
//! disabled anywhere in the suite — the four negative trust tests exist to
//! prove that each rejection reason is enforced.
//!
//! Test artifacts are written under `target/mink-artifacts/` rather than the
//! system temp directory: Windows Defender quarantines freshly linked PEs in
//! `%TEMP%` (see the Session 109 report), which made those harnesses
//! non-deterministic.

use std::io::BufRead;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

// Trust statuses the module translates into `tls_status` (winerror.h).
const CERT_E_EXPIRED: i64 = 2148204801; // 0x800B0101
const CERT_E_UNTRUSTEDROOT: i64 = 2148204809; // 0x800B0109
const CERT_E_CN_NO_MATCH: i64 = 2148204815; // 0x800B010F
const CERT_E_WRONG_USAGE: i64 = 2148204816; // 0x800B0110

fn tls_source() -> String {
    std::fs::read_to_string("stdlib/tls.mink").expect("failed to read stdlib/tls.mink")
}

/// Helpers shared by the generated programs. No user function here may free a
/// string literal: only owned values are released.
const PRELUDE: &str = r#"
fn b(v: Bool) -> Int { if v { return 1; } return 0; }
fn say_i(v: Int) { rt_print_int(v); rt_print_str("\n"); }
fn zw(p: Ptr<Int>, n: Int) {
    let mut i = 0;
    while i < n { rt_mem_store(p + i * 8, 0); i = i + 1; }
}
"#;

fn artifact_dir() -> PathBuf {
    let dir = PathBuf::from("target").join("mink-artifacts");
    std::fs::create_dir_all(&dir).expect("failed to create target/mink-artifacts");
    dir
}

/// A file-server root holding `index.html` with a recognizable marker and a
/// binary `big.bin` for the large-transfer test.
///
/// One root per marker: tests run in parallel and must not overwrite each
/// other's `index.html`.
fn file_root(marker: &str) -> PathBuf {
    let slug: String = marker
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let root = artifact_dir().join(format!("tls_root_{slug}"));
    std::fs::create_dir_all(&root).expect("failed to create TLS test root");
    std::fs::write(
        root.join("index.html"),
        format!("<!doctype html><title>tls</title><p>{marker}</p>\n"),
    )
    .expect("failed to write index.html");
    if !root.join("big.bin").exists() {
        let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(root.join("big.bin"), big).expect("failed to write big.bin");
    }
    root
}

/// The deterministic payload used by the echo test.
fn echo_payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| ((i * 7 + 3) % 251) as u8).collect()
}

fn echo_checksum(bytes: &[u8]) -> i64 {
    let mut sum: i64 = 0;
    for &byte in bytes {
        sum = (sum * 31 + byte as i64) % 1_000_003;
    }
    sum
}

// ---------------------------------------------------------------------------
// TLS test server
// ---------------------------------------------------------------------------

struct TlsServer {
    child: Child,
    port: u16,
}

impl Drop for TlsServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Start `tests/tls/server.py` with the committed `<cert>.pem` /
/// `<cert>.key.pem` pair. `expect >= 0` selects the binary echo server that
/// answers with the first chunk it reads; otherwise files under `root` are
/// served over HTTPS. The port is read from the server's stdout.
fn start_server(cert: &str, root: &Path, expect: i64) -> TlsServer {
    let mut child = Command::new("python3")
        .arg("tests/tls/server.py")
        .arg("--cert")
        .arg(format!("{cert}.pem"))
        .arg("--key")
        .arg(format!("{cert}.key.pem"))
        .arg("--root")
        .arg(root)
        .arg("--expect")
        .arg(expect.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start python3 (required for the TLS test server)");
    let stdout = child.stdout.take().expect("server stdout");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = std::io::BufReader::new(stdout).read_line(&mut line);
        let _ = tx.send(line);
    });
    let line = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("TLS test server did not report a port within 30s");
    let port = line
        .trim()
        .parse::<u16>()
        .unwrap_or_else(|_| panic!("bad port line from TLS test server: {line:?}"));
    TlsServer { child, port }
}

// ---------------------------------------------------------------------------
// Build + run harness
// ---------------------------------------------------------------------------

/// Compile and run one generated program with a hard timeout. Returns the exit
/// code, stdout bytes and stderr text.
fn build_and_run(decls: &str, body: &str) -> (i32, Vec<u8>, String) {
    let source = format!(
        "{}\n{}\n{}\nfn main() {{\n{}\n}}\n",
        tls_source(),
        PRELUDE,
        decls,
        body
    );
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("tls_test_{n}.mink"));
    std::fs::write(&path, source.replace("\r\n", "\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("build")
        .arg(&path)
        .output()
        .unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&exe);
        panic!(
            "build failed (source kept at {}):\n{stderr}",
            path.display()
        );
    }
    let mut child = Command::new(&exe)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run test exe");
    // Drain both pipes while the program runs: a large body (the 300 KiB
    // transfer) fills the pipe buffer and would otherwise wedge the child.
    let mut out_pipe = child.stdout.take().expect("stdout pipe");
    let mut err_pipe = child.stderr.take().expect("stderr pipe");
    let out_thread = std::thread::spawn(move || {
        let mut data = Vec::new();
        let _ = out_pipe.read_to_end(&mut data);
        data
    });
    let err_thread = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = err_pipe.read_to_string(&mut text);
        text
    });
    let deadline = Instant::now() + Duration::from_secs(90);
    let status = loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => break status,
            None if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("test exe timed out (source kept at {})", path.display());
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    let stdout = out_thread.join().expect("stdout reader");
    let stderr = err_thread.join().expect("stderr reader");
    let code = status.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    (code, stdout, stderr)
}

/// A program that performs one `tls_connect` against `port` with `ca` and
/// prints: the MINK error code, whether a handle was returned, and — on
/// failure — the failing-stage status from the diagnostic block.
fn handshake_probe_body(port: u16, host: &str, ca: &str) -> String {
    format!(
        r#"    let errw = rt_alloc(64);
    zw(errw, 8);
    let h = tls_connect("{host}", {port}, "{ca}", errw);
    let ok = b(rt_ptr_to_int(h) != 0);
    say_i(rt_mem_load(errw));
    say_i(ok);
    if ok == 0 {{
        say_i(rt_mem_load(errw + 8));
    }} else {{
        say_i(b(tls_cipher(h) != 0));
        say_i(tls_close(h));
    }}
    rt_free(errw);
    return 0;"#
    )
}

/// The integers `say_i` printed, in order. The runtime's integer printer
/// emits `\r\n` and the helper adds `\n`, so blank lines are noise.
fn numbers(stdout: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// Read the integer a program printed right after `marker`. The runtime's
/// `rt_print_str`/`rt_print_int` terminate every write with CRLF, so the value
/// is the next whitespace-separated token after the marker.
fn marker_number(stdout: &[u8], marker: &str) -> usize {
    let text = String::from_utf8_lossy(stdout);
    let idx = text
        .find(marker)
        .unwrap_or_else(|| panic!("no {marker} marker in transcript"));
    let token = text[idx + marker.len()..]
        .split_whitespace()
        .next()
        .unwrap_or_else(|| panic!("no value after {marker}"));
    token
        .parse()
        .unwrap_or_else(|e| panic!("bad value after {marker}: {token:?} ({e})"))
}

/// Parse the `handshake_probe_body` transcript: error code, handle flag and
/// — depending on the outcome — the failing status or the cipher flag.
fn handshake_result(stdout: &[u8]) -> (i64, i64, i64) {
    let values: Vec<i64> = numbers(stdout)
        .iter()
        .map(|l| {
            l.parse::<i64>()
                .unwrap_or_else(|_| panic!("bad line {l:?}"))
        })
        .collect();
    assert!(
        values.len() == 3 || values.len() == 4,
        "unexpected transcript: {values:?}"
    );
    (values[0], values[1], values[2])
}

// ---------------------------------------------------------------------------
// S62-01..03  Positive paths
// ---------------------------------------------------------------------------

#[test]
fn s62_handshake_with_pinned_ca() {
    let server = start_server("good", &file_root("MINK-TLS-OK"), -1);
    let body = handshake_probe_body(server.port, "localhost", "tests/tls/ca.pem");
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "leak check / exit failed: {stderr}");
    // err 0, handle present, cipher negotiated, clean close.
    let (err, ok, extra) = handshake_result(&stdout);
    assert_eq!((err, ok), (0, 1), "handshake failed: {stdout:?}");
    assert_eq!(extra, 1, "no cipher suite was negotiated");
    assert_eq!(numbers(&stdout).len(), 4);
    assert_eq!(numbers(&stdout)[3], "0", "close must succeed");
}

#[test]
fn s62_https_get_with_pinned_ca() {
    let server = start_server("good", &file_root("MINK-TLS-OK"), -1);
    let body = format!(
        r#"    let r = https_client_get_with("localhost", {}, "/index.html", "tests/tls/ca.pem");
    rt_print_str(r);
    let n = rt_str_len(r);
    rt_str_free(r);
    rt_print_str("LEN=");
    say_i(n);
    return 0;"#,
        server.port
    );
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "exit {code}: {stderr}");
    let text = String::from_utf8_lossy(&stdout);
    assert!(text.contains("200 OK"), "no HTTP status line: {text}");
    assert!(text.contains("MINK-TLS-OK"), "body marker missing: {text}");
    let len = marker_number(&stdout, "LEN=");
    assert!(len > 150, "response too short: {len}");
}

#[test]
fn s62_https_get_url_form() {
    let server = start_server("good", &file_root("MINK-TLS-URL"), -1);
    let body = format!(
        r#"    let r = https_client_get("https://localhost:{}/index.html", "tests/tls/ca.pem");
    rt_print_str(r);
    rt_str_free(r);
    return 0;"#,
        server.port
    );
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "exit {code}: {stderr}");
    let text = String::from_utf8_lossy(&stdout);
    assert!(
        text.contains("200 OK") && text.contains("MINK-TLS-URL"),
        "{text}"
    );
}

#[test]
fn s62_san_match_on_second_entry() {
    let server = start_server("sanmulti", &file_root("x"), -1);
    let body = handshake_probe_body(server.port, "localhost", "tests/tls/ca.pem");
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    let (err, ok, _) = handshake_result(&stdout);
    assert_eq!(
        (err, ok),
        (0, 1),
        "a later SAN entry must still match: {stdout:?}"
    );
}

#[test]
fn s62_ip_literal_san() {
    let server = start_server("ipliteral", &file_root("x"), -1);
    let body = handshake_probe_body(server.port, "127.0.0.1", "tests/tls/ca.pem");
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    let (err, ok, _) = handshake_result(&stdout);
    assert_eq!(
        (err, ok),
        (0, 1),
        "an iPAddress SAN must match an IPv4 literal: {stdout:?}"
    );
}

// ---------------------------------------------------------------------------
// S62-04..08  Trust rejection: every reason must be enforced
// ---------------------------------------------------------------------------

#[test]
fn s62_untrusted_root_rejected() {
    // The server presents a self-signed certificate; the client pins the test
    // CA, so the leaf's chain roots somewhere else.
    let server = start_server("untrusted", &file_root("x"), -1);
    let body = handshake_probe_body(server.port, "localhost", "tests/tls/ca.pem");
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    let (err, ok, status) = handshake_result(&stdout);
    assert_eq!((err, ok), (1005, 0), "{stdout:?}");
    assert_eq!(status, CERT_E_UNTRUSTEDROOT, "{stdout:?}");
}

#[test]
fn s62_system_store_does_not_trust_private_ca() {
    // An empty `ca_path` validates against the Windows system trust store. The
    // test CA is private, so the same handshake must fail: this proves the
    // system-store path is real verification and not a bypass.
    let server = start_server("good", &file_root("x"), -1);
    let body = handshake_probe_body(server.port, "localhost", "");
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    let (err, ok, status) = handshake_result(&stdout);
    assert_eq!((err, ok), (1005, 0), "{stdout:?}");
    assert_eq!(status, CERT_E_UNTRUSTEDROOT, "{stdout:?}");
}

#[test]
fn s62_wrong_hostname_rejected() {
    let server = start_server("wrongname", &file_root("x"), -1);
    let body = handshake_probe_body(server.port, "localhost", "tests/tls/ca.pem");
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    let (err, ok, status) = handshake_result(&stdout);
    assert_eq!((err, ok), (1005, 0), "{stdout:?}");
    assert_eq!(status, CERT_E_CN_NO_MATCH, "{stdout:?}");
}

#[test]
fn s62_wildcard_needs_a_second_label() {
    // SAN is `*.localhost`; RFC 6125 wildcards must match exactly one label, so
    // the single-label host `localhost` is not a match.
    let server = start_server("wildcard", &file_root("x"), -1);
    let body = handshake_probe_body(server.port, "localhost", "tests/tls/ca.pem");
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    let (err, ok, status) = handshake_result(&stdout);
    assert_eq!((err, ok), (1005, 0), "{stdout:?}");
    assert_eq!(status, CERT_E_CN_NO_MATCH, "{stdout:?}");
}

#[test]
fn s62_expired_certificate_rejected() {
    let server = start_server("expired", &file_root("x"), -1);
    let body = handshake_probe_body(server.port, "localhost", "tests/tls/ca.pem");
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    let (err, ok, status) = handshake_result(&stdout);
    assert_eq!((err, ok), (1005, 0), "{stdout:?}");
    assert_eq!(status, CERT_E_EXPIRED, "{stdout:?}");
}

#[test]
fn s62_wrong_extended_key_usage_rejected() {
    // The leaf is valid for clientAuth only; serverAuth is required.
    let server = start_server("clientauth", &file_root("x"), -1);
    let body = handshake_probe_body(server.port, "localhost", "tests/tls/ca.pem");
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    let (err, ok, status) = handshake_result(&stdout);
    assert_eq!((err, ok), (1005, 0), "{stdout:?}");
    assert_eq!(status, CERT_E_WRONG_USAGE, "{stdout:?}");
}

// ---------------------------------------------------------------------------
// S62-09..12  Failure paths
// ---------------------------------------------------------------------------

#[test]
fn s62_connection_refused() {
    // Take a port that was genuinely listening, then stop the server.
    let port = {
        let server = start_server("good", &file_root("x"), -1);
        server.port
    };
    let body = handshake_probe_body(port, "localhost", "tests/tls/ca.pem");
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    let (err, ok, _) = handshake_result(&stdout);
    assert_eq!(
        (err, ok),
        (1004, 0),
        "refused connect must report E-TLS-SOCKET: {stdout:?}"
    );
    assert_eq!(numbers(&stdout).len(), 3);
}

#[test]
fn s62_missing_ca_file() {
    let server = start_server("good", &file_root("x"), -1);
    let body = handshake_probe_body(server.port, "localhost", "tests/tls/definitely-missing.pem");
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    let (err, ok, _) = handshake_result(&stdout);
    assert_eq!((err, ok), (1003, 0), "{stdout:?}");
}

#[test]
fn s62_malformed_ca_file() {
    let path = artifact_dir().join("bad_ca.pem");
    std::fs::write(&path, b"not a certificate at all\n").unwrap();
    let server = start_server("good", &file_root("x"), -1);
    let body = handshake_probe_body(server.port, "localhost", "target/mink-artifacts/bad_ca.pem");
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    let (err, ok, _) = handshake_result(&stdout);
    assert_eq!((err, ok), (1003, 0), "{stdout:?}");
}

#[test]
fn s62_malformed_url() {
    // `https_client_get` follows the http.mink convention: the static "" on
    // failure. It must not open a connection or leak the argument.
    let body = r#"    let r = https_client_get("not a url", "tests/tls/ca.pem");
    say_i(rt_str_len(r));
    let r2 = https_client_get("http://localhost:1/plain", "tests/tls/ca.pem");
    say_i(rt_str_len(r2));
    return 0;"#;
    let (code, stdout, stderr) = build_and_run("", body);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(numbers(&stdout), ["0", "0"]);
}

#[test]
fn s62_invalid_and_closed_handle_operations() {
    // `tls_close` releases the handle (a closed connection is a dead pointer,
    // exactly like `rt_free`). Its *contract* is that an invalid handle is
    // rejected instead of dereferenced: an empty handle must answer every
    // operation with the documented failure value and never fault.
    let server = start_server("good", &file_root("x"), -1);
    let body = format!(
        r#"    let errw = rt_alloc(64);
    zw(errw, 8);
    let h = tls_connect("localhost", {}, "tests/tls/ca.pem", errw);
    say_i(tls_close(h));
    let dead = _pi(0);
    say_i(tls_close(dead));
    let e = tls_recv(dead, 16);
    say_i(rt_str_len(e));
    rt_str_free(e);
    say_i(tls_err(dead));
    say_i(tls_send(dead, "x"));
    say_i(tls_status(dead));
    say_i(tls_cipher(dead));
    say_i(rt_mem_load(errw));
    rt_free(errw);
    return 0;"#,
        server.port
    );
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        numbers(&stdout),
        ["0", "-1", "0", "1007", "-1", "0", "0", "0"],
        "invalid-handle behavior changed"
    );
}

// ---------------------------------------------------------------------------
// S62-13..15  Data path
// ---------------------------------------------------------------------------

#[test]
fn s62_binary_echo_round_trip_and_partial_reads() {
    let server = start_server("good", &file_root("x"), 0);
    let payload = echo_payload(5000);
    let checksum = echo_checksum(&payload);
    let body = format!(
        r#"    let errw = rt_alloc(64);
    zw(errw, 8);
    let h = tls_connect("localhost", {}, "tests/tls/ca.pem", errw);
    if rt_ptr_to_int(h) == 0 {{
        say_i(rt_mem_load(errw));
        say_i(-1);
        say_i(-1);
        rt_free(errw);
        return 0;
    }}
    let n = 5000;
    let p = rt_str_alloc(n);
    let mut i = 0;
    while i < n {{
        rt_str_set_byte(p, i, (i * 7 + 3) % 251);
        i = i + 1;
    }}
    say_i(b(tls_send(h, p) == n));
    let mut sum = 0;
    let mut got = 0;
    while got < n {{
        let chunk = tls_recv(h, 1024);
        let cn = rt_str_len(chunk);
        if cn == 0 {{ break; }}
        let mut j = 0;
        while j < cn {{
            sum = (sum * 31 + rt_str_byte(chunk, j)) % 1000003;
            j = j + 1;
        }}
        got = got + cn;
        rt_str_free(chunk);
    }}
    say_i(got);
    say_i(sum);
    let eof = tls_recv(h, 64);
    say_i(rt_str_len(eof));
    rt_str_free(eof);
    say_i(tls_close(h));
    rt_free(errw);
    return 0;"#,
        server.port
    );
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    let lines = numbers(&stdout);
    assert_eq!(lines[0], "1", "tls_send must report all bytes: {stdout:?}");
    assert_eq!(
        lines[1], "5000",
        "short read after buffer split: {stdout:?}"
    );
    assert_eq!(
        lines[2],
        checksum.to_string(),
        "payload corrupted in transit"
    );
    assert_eq!(lines[3], "0", "peer close should end the stream");
    assert_eq!(lines[4], "0", "close after EOF must succeed");
}

#[test]
fn s62_large_transfer() {
    let root = file_root("x");
    let big = std::fs::read(root.join("big.bin")).unwrap();
    let server = start_server("good", &root, -1);
    let body = format!(
        r#"    let r = https_client_get_with("localhost", {}, "/big.bin", "tests/tls/ca.pem");
    let n = rt_str_len(r);
    rt_print_str(r);
    rt_str_free(r);
    rt_print_str("LEN=");
    say_i(n);
    return 0;"#,
        server.port
    );
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    let len = marker_number(&stdout, "LEN=");
    assert!(
        len >= big.len(),
        "truncated transfer: {len} < {}",
        big.len()
    );
    // The last 64 payload bytes must appear in order in the response.
    let needle = &big[big.len() - 64..];
    assert!(
        stdout.windows(needle.len()).any(|w| w == needle),
        "large transfer is corrupted (needle not found)"
    );
}

#[test]
fn s62_repeated_connections_do_not_leak() {
    // Several full connect/handshake/GET/close cycles on one server. Exit code
    // 0 is the runtime arena check: any per-cycle context, buffer or string
    // left behind would fail the program with E-R06.
    let server = start_server("good", &file_root("MINK-TLS-REPEAT"), -1);
    let body = format!(
        r#"    let mut i = 0;
    while i < 4 {{
        let r = https_client_get_with("localhost", {}, "/index.html", "tests/tls/ca.pem");
        let mut found = 0;
        let n = rt_str_len(r);
        let mut j = 0;
        while j + 12 <= n {{
            let mut m = 1;
            let mut k = 0;
            while k < 12 {{
                if rt_str_byte(r, j + k) != rt_str_byte("MINK-TLS-REPEAT", k) {{ m = 0; }}
                k = k + 1;
            }}
            if m == 1 {{ found = 1; }}
            j = j + 1;
        }}
        say_i(found);
        rt_str_free(r);
        i = i + 1;
    }}
    return 0;"#,
        server.port
    );
    let (code, stdout, stderr) = build_and_run("", &body);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        numbers(&stdout),
        ["1", "1", "1", "1"],
        "a repeated connection failed"
    );
}

// ---------------------------------------------------------------------------
// S62-16  System trust store (public CA) — live-network test, opt-in
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires public internet access; run manually for the system-store positive path"]
fn s62_system_store_public_https() {
    // The Windows system trust store path (empty `ca_path`) is exercised
    // negatively by `s62_system_store_does_not_trust_private_ca`, which is
    // deterministic and runs in CI. This companion proves the positive case
    // against a chain rooted at a public CA.
    let body = r#"    let r = https_client_get("https://example.com/", "");
    let n = rt_str_len(r);
    rt_print_str(r);
    rt_str_free(r);
    rt_print_str("\nLEN=");
    say_i(n);
    return 0;"#;
    let (code, stdout, stderr) = build_and_run("", body);
    assert_eq!(code, 0, "{stderr}");
    let text = String::from_utf8_lossy(&stdout);
    assert!(text.contains("200 OK"), "no HTTP status line: {text}");
}
