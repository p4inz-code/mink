//! Comprehensive tests for the MINK Networking library (Session 67).
//!
//! Tests basic Winsock2 initialization, socket creation, TCP operations,
//! error handling, and integration with other ecosystem libraries.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn net_lib() -> String {
    std::fs::read_to_string("stdlib/network.mink").expect("failed to read stdlib/network.mink")
}

fn time_lib() -> String {
    std::fs::read_to_string("stdlib/time.mink").expect("failed to read stdlib/time.mink")
}

fn process_lib() -> String {
    std::fs::read_to_string("stdlib/process.mink").expect("failed to read stdlib/process.mink")
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mink_net_test_{}_{name}", std::process::id()));
    std::fs::write(&path, content).unwrap();
    path
}

fn build_and_run(test_body: &str) -> (i32, Vec<u8>, Vec<u8>) {
    let lib = net_lib();
    let source = format!("{lib}\n{test_body}");
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

fn build_and_run_with_libs(test_body: &str, libs: &[&str]) -> (i32, Vec<u8>, Vec<u8>) {
    let mut source = String::new();
    for lib in libs {
        source.push_str(lib);
        source.push('\n');
    }
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

fn native_exit_code_multi(test_body: &str, libs: &[&str]) -> i32 {
    let (code, _, _) = build_and_run_with_libs(test_body, libs);
    code
}

// ==========================================================================
// SECTION 1: Winsock initialization
// ==========================================================================

#[test]
fn n01_net_init_succeeds() {
    let src = r#"
fn main() -> Int {
    let result = net_init();
    return result;
}"#;
    assert_eq!(native_exit_code(src), 0);
}

#[test]
fn n02_net_init_idempotent() {
    let src = r#"
fn main() -> Int {
    let r1 = net_init();
    let r2 = net_init();
    if r1 == 0 {
        if r2 == 0 { return 1; }
    }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n03_net_cleanup_succeeds() {
    let src = r#"
fn main() -> Int {
    net_init();
    let result = net_cleanup();
    return result;
}"#;
    assert_eq!(native_exit_code(src), 0);
}

// ==========================================================================
// SECTION 2: Socket creation
// ==========================================================================

#[test]
fn n10_tcp_socket_creation() {
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    net_close(sock);
    net_cleanup();
    if sock != -1 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n11_udp_socket_creation() {
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_udp_socket();
    net_close(sock);
    net_cleanup();
    if sock != -1 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n12_socket_close_succeeds() {
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    let result = net_close(sock);
    net_cleanup();
    return result;
}"#;
    assert_eq!(native_exit_code(src), 0);
}

#[test]
fn n13_invalid_socket_close_fails() {
    let src = r#"
fn main() -> Int {
    net_init();
    let result = net_close(-1);
    net_cleanup();
    if result == -1 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 3: Bind and Listen
// ==========================================================================

#[test]
fn n20_bind_to_localhost() {
    let src = r#"fn main() -> Int {
    net_init();
    // Windows reserves whole port ranges (WSL/Hyper-V exclusions), so a
    // fixed port can fail to bind with WSAEACCES. Try a few candidates.
    let ports = [19876, 19076, 21076, 25076, 17076, 16076];
    let mut result = -1;
    let mut i = 0;
    while i < 6 && result != 0 {
        let sock = net_tcp_socket();
        result = net_bind(sock, "127.0.0.1", ports[i]);
        net_close(sock);
        i = i + 1;
    }
    net_cleanup();
    return result;
}
"#;
    assert_eq!(native_exit_code(src), 0);
}

#[test]
fn n21_listen_succeeds() {
    let src = r#"fn main() -> Int {
    net_init();
    let ports = [19877, 19077, 21077, 25077, 17077, 16077];
    let mut result = -1;
    let mut i = 0;
    while i < 6 && result != 0 {
        let sock = net_tcp_socket();
        net_bind(sock, "127.0.0.1", ports[i]);
        result = net_listen(sock, 1);
        net_close(sock);
        i = i + 1;
    }
    net_cleanup();
    return result;
}
"#;
    assert_eq!(native_exit_code(src), 0);
}

#[test]
fn n22_listen_on_all_interfaces() {
    let src = r#"fn main() -> Int {
    net_init();
    let ports = [19878, 19078, 21078, 25078, 17078, 16078];
    let mut result = -1;
    let mut i = 0;
    while i < 6 && result != 0 {
        let sock = net_tcp_socket();
        result = net_bind(sock, "0.0.0.0", ports[i]);
        net_close(sock);
        i = i + 1;
    }
    net_cleanup();
    return result;
}
"#;
    assert_eq!(native_exit_code(src), 0);
}

// ==========================================================================
// SECTION 4: Connect
// ==========================================================================

#[test]
fn n30_connect_refused() {
    // Connecting to a port with no listener should fail
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    let result = net_connect(sock, "127.0.0.1", 19999);
    net_close(sock);
    net_cleanup();
    if result == -1 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 5: Byte order
// ==========================================================================

#[test]
fn n40_htons_converts() {
    let src = r#"
fn main() -> Int {
    net_init();
    // htons(0x0102) should swap bytes to 0x0201 = 513
    let result = net_htons(258);
    net_cleanup();
    return result;
}"#;
    assert_eq!(native_exit_code(src), 513);
}

#[test]
fn n41_ntohs_same_as_htons() {
    let src = r#"
fn main() -> Int {
    net_init();
    let a = net_htons(1234);
    let b = net_ntohs(1234);
    net_cleanup();
    if a == b { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n42_htons_zero() {
    let src = r#"
fn main() -> Int {
    net_init();
    let result = net_htons(0);
    net_cleanup();
    return result;
}"#;
    assert_eq!(native_exit_code(src), 0);
}

// ==========================================================================
// SECTION 6: Hostname
// ==========================================================================

#[test]
fn n50_hostname_nonempty() {
    let src = r#"
fn main() -> Int {
    net_init();
    let name = net_hostname();
    let len = rt_str_len(name);
    rt_str_free(name);
    net_cleanup();
    if len > 0 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 7: Convenience helpers
// ==========================================================================

#[test]
fn n60_is_valid_socket() {
    let src = r#"
fn main() -> Int {
    net_init();
    let valid = net_is_valid_socket(5);
    let invalid = net_is_valid_socket(-1);
    net_cleanup();
    if valid == true {
        if invalid == false { return 1; }
    }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n61_is_ok() {
    let src = r#"
fn main() -> Int {
    net_init();
    let ok = net_is_ok(0);
    let fail = net_is_ok(-1);
    net_cleanup();
    if ok == true {
        if fail == false { return 1; }
    }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 8: Integration with other libraries
// ==========================================================================

#[test]
fn n70_net_with_time() {
    let src = r#"
fn main() -> Int {
    net_init();
    let before = time_now();
    let sock = net_tcp_socket();
    net_close(sock);
    let after = time_now();
    net_cleanup();
    if after >= before { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code_multi(src, &[&net_lib(), &time_lib()]), 1);
}

#[test]
fn n71_net_with_process() {
    let src = r#"
fn main() -> Int {
    net_init();
    let pid = process_id();
    net_cleanup();
    if pid > 0 { return 1; }
    return 0;
}"#;
    assert_eq!(
        native_exit_code_multi(src, &[&net_lib(), &process_lib()]),
        1
    );
}

#[test]
fn n72_net_with_strings() {
    // Test that dynamically constructed strings work with networking.
    // Uses rt_str_len (intrinsic, doesn't move) to verify construction,
    // then rt_str_free to avoid leak.
    let src = r#"
fn main() -> Int {
    net_init();
    let addr = rt_str_concat("127", ".");
    let addr2 = rt_str_concat(addr, "0");
    rt_str_free(addr);
    let addr3 = rt_str_concat(addr2, ".0.1");
    rt_str_free(addr2);
    let len = rt_str_len(addr3);
    rt_str_free(addr3);
    net_close(net_tcp_socket());
    net_cleanup();
    if len == 9 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 9: Error handling
// ==========================================================================

#[test]
fn n80_net_last_error_after_failed_connect() {
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    net_connect(sock, "127.0.0.1", 19998);
    let err = net_last_error();
    net_close(sock);
    net_cleanup();
    // WSAECONNREFUSED = 10061
    if err == 10061 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n81_double_close_returns_error() {
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    net_close(sock);
    let result = net_close(sock);
    net_cleanup();
    if result == -1 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n82_bind_already_bound() {
    let src = r#"
fn main() -> Int {
    net_init();
    let s1 = net_tcp_socket();
    net_bind(s1, "127.0.0.1", 19880);
    let s2 = net_tcp_socket();
    let result = net_bind(s2, "127.0.0.1", 19880);
    net_close(s1);
    net_close(s2);
    net_cleanup();
    if result == -1 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 10: Edge cases
// ==========================================================================

#[test]
fn n90_send_on_unconnected_socket() {
    // On Windows, send() on an unconnected TCP socket may succeed
    // with 0 bytes sent (not -1). Test that the call doesn't crash.
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    let result = net_send(sock, "hello");
    net_close(sock);
    net_cleanup();
    // 0 bytes sent or -1 error — both are valid outcomes
    if result <= 0 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n91_recv_empty_on_unconnected() {
    // On Windows, recv() on an unconnected TCP socket returns -1
    // (error), which results in len=0 from the runtime.
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    let data = net_recv(sock, 100);
    let len = rt_str_len(data);
    rt_str_free(data);
    net_close(sock);
    net_cleanup();
    // recv on unconnected socket returns empty/error data
    return len;
}"#;
    assert_eq!(native_exit_code(src), 0);
}

#[test]
fn n92_resolve_returns_ipv4_text() {
    // `net_resolve` runs Winsock's `getaddrinfo`, so a literal comes back
    // unchanged and a host name comes back as IPv4 text. `net_connect` runs
    // the same resolver, so a host name is directly connectable.
    let src = r#"
fn main() -> Int {
    net_init();
    let a = net_resolve("192.168.1.1", 80);
    let alen = rt_str_len(a);
    rt_str_free(a);
    let b = net_resolve("localhost", 80);
    let blen = rt_str_len(b);
    let mut dots = 0;
    let mut numeric = 1;
    let mut i = 0;
    while i < blen {
        let c = rt_str_byte(b, i);
        if c == 46 { dots = dots + 1; }
        else if c < 48 || c > 57 { numeric = 0; }
        i = i + 1;
    }
    rt_str_free(b);
    net_cleanup();
    // The literal must survive resolution byte for byte, and the name must
    // resolve to a dotted quad (four numeric labels, 7..15 bytes).
    if alen == 11 && numeric == 1 && dots == 3 && blen >= 7 && blen <= 15 { return 1; }
    return 0;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 11: resolver-based connect (Session 125 regressions)
// ==========================================================================
//
// `net_connect` resolves through Winsock's `getaddrinfo`, so a host name is
// usable everywhere an IPv4 literal is. These cases pin the three outcomes a
// caller has to tell apart: a resolved name, a resolved name whose port has no
// listener, and a name the resolver cannot answer.

#[test]
fn n93_resolve_unresolvable_name_is_empty() {
    // `.invalid` is reserved by RFC 2606 and can never resolve, so the
    // resolver failure is deterministic rather than dependent on a live DNS.
    let src = r#"
fn main() -> Int {
    net_init();
    let bad = net_resolve("mink-no-such-host.invalid", 80);
    let len = rt_str_len(bad);
    rt_str_free(bad);
    net_cleanup();
    if len == 0 { return 1; }
    return 0;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n94_connect_host_name_reports_refused() {
    // The name resolves and the connection is refused: the failure is a
    // socket-level one (WSAECONNREFUSED = 10061), never a resolver one.
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    let result = net_connect(sock, "localhost", 19997);
    let err = net_last_error();
    net_close(sock);
    net_cleanup();
    if result == -1 && err == 10061 { return 1; }
    return 0;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n95_connect_unresolvable_name_reports_host_not_found() {
    // A resolver failure is explicit rather than silent: -1 with the
    // Winsock resolver code (WSAHOST_NOT_FOUND = 11001).
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    let result = net_connect(sock, "mink-no-such-host.invalid", 80);
    let err = net_last_error();
    net_close(sock);
    net_cleanup();
    if result == -1 && err == 11001 { return 1; }
    return 0;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n96_host_name_tcp_round_trip() {
    // A full send/receive round trip addressed by host name: the listener is
    // bound in `main`, the echo runs on a thread, and the client connects with
    // "localhost" rather than an IPv4 literal.
    let src = r#"
mod threads;

fn echo(p: Ptr<Int>) -> Int {
    let listener = rt_mem_load(p);
    let client = net_accept(listener);
    if client == -1 { return 0; }
    let data = net_recv(client, 64);
    net_send(client, data);
    net_shutdown(client, SD_BOTH());
    net_close(client);
    return 1;
}

fn main() -> Int {
    // Windows reserves whole port ranges (WSL/Hyper-V exclusions), so try a
    // few candidates and use the first that binds.
    let ports = [19871, 19071, 21071, 25071, 17071, 16071];
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
    let t = thread_spawn(echo, p);
    let sock = net_dial("localhost", port);
    if sock == -1 {
        thread_join(t);
        rt_free(p);
        net_close(listener);
        return 3;
    }
    net_send(sock, "mink");
    let reply = net_recv(sock, 64);
    let len = rt_str_len(reply);
    rt_str_free(reply);
    net_close(sock);
    let served = thread_join(t);
    rt_free(p);
    net_close(listener);
    net_cleanup();
    if len == 4 && served == 1 { return 1; }
    return 4;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n97_ipv4_literal_tcp_round_trip() {
    // The same round trip over the IPv4 literal: a resolved name and a literal
    // are interchangeable on the wire.
    let src = r#"
mod threads;

fn echo(p: Ptr<Int>) -> Int {
    let listener = rt_mem_load(p);
    let client = net_accept(listener);
    if client == -1 { return 0; }
    let data = net_recv(client, 64);
    net_send(client, data);
    net_shutdown(client, SD_BOTH());
    net_close(client);
    return 1;
}

fn main() -> Int {
    let ports = [19872, 19072, 21072, 25072, 17072, 16072];
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
    let t = thread_spawn(echo, p);
    let sock = net_dial("127.0.0.1", port);
    if sock == -1 {
        thread_join(t);
        rt_free(p);
        net_close(listener);
        return 3;
    }
    net_send(sock, "mink");
    let reply = net_recv(sock, 64);
    let len = rt_str_len(reply);
    rt_str_free(reply);
    net_close(sock);
    let served = thread_join(t);
    rt_free(p);
    net_close(listener);
    net_cleanup();
    if len == 4 && served == 1 { return 1; }
    return 4;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 12: UDP datagrams (Session 125 regressions)
// ==========================================================================
//
// `net_sendto` / `net_recvfrom` address a socket that was never connected, and
// `net_sender_ip` / `net_sender_port` report who the last datagram came from.
// Each case binds a distinct port so the suite stays order-independent, and
// every owned Str is released so the exit-time leak scan (E-R06) stays quiet.

#[test]
fn n98_udp_self_send_receive_round_trip() {
    // A datagram sent to the sender's own bound port must come back byte for
    // byte, and the recorded sender must be that same socket: the loopback
    // address text and the bound port in host order.
    let src = r#"
fn main() -> Int {
    net_init();
    let port = 19961;
    let sock = net_udp_socket();
    if sock == -1 { return 2; }
    if net_bind(sock, "127.0.0.1", port) != 0 { return 3; }

    let sent = net_sendto(sock, "hello-udp", "127.0.0.1", port);
    if sent != 9 { return 4; }

    let data = net_recvfrom(sock, 2048);
    let len = rt_str_len(data);
    let mut good = 1;
    let mut i = 0;
    while i < len {
        if rt_str_byte(data, i) != rt_str_byte("hello-udp", i) { good = 0; }
        i = i + 1;
    }
    rt_str_free(data);

    let sip = net_sender_ip();
    let sip_len = rt_str_len(sip);
    let mut same_ip = 1;
    if sip_len != 9 { same_ip = 0; }
    let mut j = 0;
    while j < sip_len {
        if rt_str_byte(sip, j) != rt_str_byte("127.0.0.1", j) { same_ip = 0; }
        j = j + 1;
    }
    rt_str_free(sip);
    let sport = net_sender_port();

    net_close(sock);
    net_cleanup();
    if len == 9 && good == 1 && same_ip == 1 && sport == port { return 1; }
    return 5;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n99_udp_recvfrom_maxlen_is_the_receive_buffer() {
    // `maxlen` is the receive buffer, exactly as with `net_recv`: it bounds
    // the read, a non-positive request does not receive at all, and a request
    // larger than the datagram is not a reason to reject it. Winsock does not
    // truncate a datagram that does not fit: it fails with WSAEMSGSIZE, which
    // surfaces as an empty Str plus that code rather than a partial read.
    let src = r#"
fn main() -> Int {
    net_init();
    let port = 19962;
    let sock = net_udp_socket();
    if sock == -1 { return 2; }
    if net_bind(sock, "127.0.0.1", port) != 0 { return 3; }

    if net_sendto(sock, "0123456789abcdefghij", "127.0.0.1", port) != 20 { return 4; }
    let short_read = net_recvfrom(sock, 8);
    let short_len = rt_str_len(short_read);
    rt_str_free(short_read);
    let short_err = net_last_error();

    // A request past the old 4088-byte scratch buffer is just a larger buffer:
    // the datagram is still read whole.
    if net_sendto(sock, "abcdefghij", "127.0.0.1", port) != 10 { return 5; }
    let big_read = net_recvfrom(sock, 1048576);
    let big_len = rt_str_len(big_read);
    rt_str_free(big_read);

    // A non-positive request is "receive nothing", like net_recv's: the
    // datagram stays queued and the result is an owned empty Str with no
    // error.
    if net_sendto(sock, "xyz", "127.0.0.1", port) != 3 { return 6; }
    let odd_read = net_recvfrom(sock, 0);
    let odd_len = rt_str_len(odd_read);
    rt_str_free(odd_read);
    let odd_err = net_last_error();

    net_close(sock);
    net_cleanup();
    if short_len == 0 && short_err == 10040 && big_len == 10 && odd_len == 0 && odd_err == 0 { return 1; }
    return 7;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n100_udp_recvfrom_reports_error_as_empty_string() {
    // A failing receive is an owned EMPTY Str with the reason still visible
    // through `net_last_error` — never garbage bytes and never a fault. The
    // socket is closed first so the failure is WSAENOTSOCK, which is
    // immediate rather than a blocking wait.
    let src = r#"
fn main() -> Int {
    net_init();
    let port = 19963;
    let sock = net_udp_socket();
    if sock == -1 { return 2; }
    if net_bind(sock, "127.0.0.1", port) != 0 { return 3; }
    net_close(sock);

    let data = net_recvfrom(sock, 64);
    let len = rt_str_len(data);
    rt_str_free(data);
    let err = net_last_error();
    net_cleanup();
    if len == 0 && err != 0 { return 1; }
    return 4;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n101_udp_sendto_unresolvable_host_reports_error() {
    // The resolver runs inside `sendto`, so a name that cannot resolve is
    // reported as -1 with the resolver code republished through
    // `net_last_error` (WSAHOST_NOT_FOUND), not as a send failure on a healthy
    // socket.
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_udp_socket();
    if sock == -1 { return 2; }
    let sent = net_sendto(sock, "x", "mink-no-such-host.invalid", 9);
    let err = net_last_error();
    net_close(sock);
    net_cleanup();
    if sent == -1 && err == 11001 { return 1; }
    return 3;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n102_udp_sender_helpers_before_any_datagram() {
    // Before the first `net_recvfrom` there is no sender: the accessors report
    // that as -1 and an owned empty Str instead of reading uninitialised BSS.
    let src = r#"
fn main() -> Int {
    net_init();
    let port = net_sender_port();
    let sip = net_sender_ip();
    let len = rt_str_len(sip);
    rt_str_free(sip);
    net_cleanup();
    if port == -1 && len == 0 { return 1; }
    return 2;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 7: concurrent implicit initialization (Session 133, M2 P5)
// ==========================================================================
//
// `http_send` and `net_dial` call `net_init` so each is a standalone entry
// point. The first-time Winsock sequence stages ws2_32 entry-point names and
// WSADATA in the shared runtime `recv_buf`. Without serialization, two threads
// entering that sequence together overwrite each other's names between
// GetProcAddress calls; the table then holds null pointers and the next call
// through it segfaults (observed ~4/20 runs with 8 threads, before the lock).
// Every process has one such race window, so the test runs the built program
// repeatedly rather than once.
//
// Windows-only: the race is in the Windows Winsock initialization path.

#[cfg(target_os = "windows")]
#[test]
fn n100_concurrent_implicit_net_init_is_serialized() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    const RUNS: usize = 25;
    const WORKERS: usize = 8;
    const REQUESTS_PER_WORKER: usize = 10;

    let http = std::fs::read_to_string("stdlib/http.mink").unwrap();
    let threads = std::fs::read_to_string("stdlib/threads.mink").unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let expected_connections = RUNS * WORKERS * REQUESTS_PER_WORKER;
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(90);
        for _ in 0..expected_connections {
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Windows makes the accepted socket inherit the
                        // listener's non-blocking mode. The fixture must switch
                        // it back: otherwise `read` can return WouldBlock
                        // before the request arrives, the fixture answers and
                        // closes with the request still in flight, and the
                        // client legitimately sees an aborted connection
                        // (WSAECONNABORTED). That race made this test fail
                        // roughly once every few runs for a fixture reason, not
                        // a runtime one.
                        stream.set_nonblocking(false).unwrap();
                        let mut buf = [0u8; 1024];
                        let _ = stream.read(&mut buf);
                        let _ = stream.write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 17\r\nConnection: close\r\n\r\nMINK-M2P5-HTTP-OK",
                        );
                        let _ = stream.flush();
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(_) => return,
                }
            }
        }
    });

    let body = format!(
        r#"
fn worker(p: Ptr<Int>) -> Int {{
    let mut fails = 0;
    let mut i = 0;
    while i < {requests} {{
        let r = http_client_get("http://localhost:{port}/small");
        let b = http_body(r);
        if rt_str_len(b) != 17 {{ fails = fails + 1; }}
        if rt_str_eq(b, "MINK-M2P5-HTTP-OK") == false {{ fails = fails + 1; }}
        rt_str_free(b);
        i = i + 1;
    }}
    return fails;
}}

fn main() -> Int {{
    // Deliberately no net_init() here: the workers must race the first-time
    // initialization through their own implicit calls.
    let mut handles = rt_alloc({workers} * 8);
    let mut args = rt_alloc({workers} * 16);
    let mut i = 0;
    while i < {workers} {{
        rt_mem_store(args + i * 16, i);
        rt_mem_store(handles + i * 8, thread_spawn(worker, args + i * 16));
        i = i + 1;
    }}
    let mut fails = 0;
    i = 0;
    while i < {workers} {{
        fails = fails + thread_join(rt_mem_load(handles + i * 8));
        i = i + 1;
    }}
    rt_free(handles);
    rt_free(args);
    rt_exit(fails);
}}
"#,
        requests = REQUESTS_PER_WORKER,
        port = port,
        workers = WORKERS,
    );

    let source = format!("{http}\n{threads}\n{body}");
    let path = temp_source("n100_concurrent_init.mink", &source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exe = path.with_extension("exe");
    assert!(exe.exists(), "no executable produced");

    for run in 0..RUNS {
        let output = Command::new(&exe).output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "run {run} crashed or failed: stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    let _ = server.join();
}

// ==========================================================================
// SECTION 12: `net_dial` error preservation (Session 141 regression)
// ==========================================================================
//
// `net_dial` closes the failed socket before returning, and `closesocket`
// clears the calling thread's Winsock last-error. The helper therefore
// captures the connect/resolver code first, closes, and republishes it
// through `net_set_last_error`, so `net_last_error()` still reports why the
// dial failed. These cases pin the exact reflector code, not just -1.

#[test]
fn n103_net_dial_reports_refused_error_after_cleanup() {
    // A resolved name whose port has no listener: the dial must return -1
    // and leave WSAECONNREFUSED (10061) behind, not the 0 that closesocket
    // writes.
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_dial("127.0.0.1", 19992);
    let err = net_last_error();
    net_cleanup();
    if sock == -1 && err == 10061 { return 1; }
    return 0;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n104_net_dial_reports_resolver_error_after_cleanup() {
    // `.invalid` is reserved by RFC 2606, so the resolver failure is
    // deterministic and offline. The dial must preserve WSAHOST_NOT_FOUND
    // (11001) across the socket cleanup.
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_dial("mink-no-such-host.invalid", 80);
    let err = net_last_error();
    net_cleanup();
    if sock == -1 && err == 11001 { return 1; }
    return 0;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n105_net_dial_repeated_failures_report_their_own_error() {
    // Each failure republishes its own code: a refused dial (10061), then a
    // resolver failure (11001), then a refused dial again. A stale or
    // cleared last-error would break at least one arm.
    let src = r#"
fn main() -> Int {
    net_init();
    let a_sock = net_dial("127.0.0.1", 19992);
    let a = net_last_error();
    let b_sock = net_dial("mink-no-such-host.invalid", 80);
    let b = net_last_error();
    let c_sock = net_dial("127.0.0.1", 19992);
    let c = net_last_error();
    net_cleanup();
    if a_sock == -1 && a == 10061 {
        if b_sock == -1 && b == 11001 {
            if c_sock == -1 && c == 10061 { return 1; }
        }
    }
    return 0;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n106_net_dial_success_still_returns_a_usable_socket() {
    // The success path is unchanged: a listener in the backlog is enough
    // for the connect, and the returned handle is a real socket that closes
    // cleanly.
    let src = r#"
fn main() -> Int {
    let listener = net_listen_on("127.0.0.1", 19993, 1);
    if listener == -1 { return 0; }
    let sock = net_dial("127.0.0.1", 19993);
    if sock == -1 {
        net_close(listener);
        net_cleanup();
        return 0;
    }
    let closed = net_close(sock);
    net_close(listener);
    net_cleanup();
    if closed == 0 { return 1; }
    return 0;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 13: `net_last_error` before `net_init` (Session 142 regression)
// ==========================================================================
//
// Winsock's function table lives in the runtime's BSS and is resolved by
// `net_init`. The getter used that table unconditionally, so calling it
// before initialization dereferenced a null pointer (exit 139). The defined
// behaviour is 0: the value `WSAGetLastError` itself reports when no error
// has been recorded, the fixed Linux result, and the runtime's established
// "not initialized" answer.

#[test]
fn n107_net_last_error_before_net_init_is_zero_and_does_not_crash() {
    let src = r#"
fn main() -> Int {
    let before = net_last_error();
    let init = net_init();
    net_cleanup();
    let cleaned = net_last_error();
    if before == 0 && init == 0 && cleaned == 0 { return 1; }
    return 0;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

#[test]
fn n108_net_last_error_still_reports_failures_after_init() {
    // The initialized path is unchanged: a refused connect still leaves
    // WSAECONNREFUSED (10061) available to the getter.
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    let result = net_connect(sock, "127.0.0.1", 19994);
    let err = net_last_error();
    net_close(sock);
    net_cleanup();
    if sock != -1 && result == -1 && err == 10061 { return 1; }
    return 0;
}
"#;
    assert_eq!(native_exit_code(src), 1);
}

// ==========================================================================
// SECTION 14: bounded connect/read timeouts (Session 145)
// ==========================================================================
//
// A blocking `net_recv`/`net_connect` has no bound: a peer that accepts and
// then sends nothing, or a handshake that never completes, holds the process
// indefinitely. `net_recv_timeout` and `net_connect_timeout` add a
// caller-supplied budget on top of the existing readiness layer (WSAPoll).
//
// Fixtures are Rust listeners on 127.0.0.1:0, so the OS picks the port and no
// candidate-port guessing is needed. Every program runs under a wall-clock
// watchdog: the behavior under test is that the program *returns* within its
// budget, so a regression must fail the test rather than hang the suite.
// Timing assertions use generous windows (a lower bound well under the
// requested wait and an upper bound seconds above it) and never require
// millisecond-perfect scheduling.

/// Build a network-library program and run it under a wall-clock deadline.
/// Returns `None` for the exit code when the deadline expired; the child has
/// then been killed and reaped.
fn build_and_run_bounded(
    test_body: &str,
    libs: &[&str],
    deadline: Duration,
) -> (Option<i32>, Vec<u8>, Vec<u8>) {
    let mut source = String::new();
    for lib in libs {
        source.push_str(lib);
        source.push('\n');
    }
    source.push_str(test_body);
    let name = std::thread::current()
        .name()
        .unwrap_or("program")
        .replace("::", "_");
    let path = temp_source(&format!("{name}_bounded.mink"), &source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exe = path.with_extension("exe");
    assert!(exe.exists(), "no executable produced");
    let mut child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    loop {
        match child.try_wait().unwrap() {
            Some(_) => {
                let output = child.wait_with_output().unwrap();
                return (output.status.code(), output.stdout, output.stderr);
            }
            None if start.elapsed() >= deadline => {
                let _ = child.kill();
                let output = child.wait_with_output().unwrap();
                return (None, output.stdout, output.stderr);
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

/// Accept one connection, waiting at most `deadline` so a test whose client
/// never connects cannot hang on the fixture itself.
fn accept_with_deadline(listener: &TcpListener, deadline: Duration) -> Option<TcpStream> {
    listener.set_nonblocking(true).unwrap();
    let start = Instant::now();
    while start.elapsed() < deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                return Some(stream);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return None,
        }
    }
    None
}

/// A local peer that accepts one connection and then sends nothing until the
/// test releases it — the unbounded-blocking condition these calls bound.
fn silent_peer() -> (u16, mpsc::Sender<()>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (release, released) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        if let Some(stream) = accept_with_deadline(&listener, Duration::from_secs(20)) {
            let _ = released.recv_timeout(Duration::from_secs(20));
            drop(stream);
        }
    });
    (port, release, handle)
}

/// A local echo peer: accept, read one request, send one reply.
fn echo_peer(reply: &'static [u8]) -> (u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        if let Some(mut stream) = accept_with_deadline(&listener, Duration::from_secs(20)) {
            let mut buf = [0u8; 16];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(reply);
        }
    });
    (port, handle)
}

/// A local peer that accepts one connection and then writes each planned
/// chunk at its offset from the accept, in order.
fn timed_peer(plan: &'static [(u64, &'static [u8])]) -> (u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        if let Some(mut stream) = accept_with_deadline(&listener, Duration::from_secs(20)) {
            let start = Instant::now();
            for (at_ms, bytes) in plan {
                let at = Duration::from_millis(*at_ms);
                let now = start.elapsed();
                if at > now {
                    std::thread::sleep(at - now);
                }
                if stream.write_all(bytes).is_err() {
                    return;
                }
            }
        }
    });
    (port, handle)
}

/// The program must have returned inside the watchdog window with exit 1
/// (every MINK body below returns 1 only when all of its assertions hold).
#[track_caller]
fn assert_bounded_success(code: Option<i32>, out: &[u8], err: &[u8], what: &str) {
    match code {
        None => panic!(
            "{what}: program did not return before the watchdog deadline (unbounded blocking)\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(out),
            String::from_utf8_lossy(err)
        ),
        Some(1) => {}
        Some(other) => panic!(
            "{what}: expected exit 1, got {other}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(out),
            String::from_utf8_lossy(err)
        ),
    }
}

// T1 — an ordinary successful connection and read still succeed through the
// timeout-aware path.
#[test]
fn n109_connect_timeout_success_and_read_timeout_round_trip() {
    let (port, server) = echo_peer(b"pong");
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    if sock == -1 { net_cleanup(); return 0; }
    if net_connect_timeout(sock, "127.0.0.1", __PORT__, 5000) != 0 {
        net_close(sock);
        net_cleanup();
        return 0;
    }
    let sent = net_send(sock, "ping");
    let reply = net_recv_timeout(sock, 64, 5000);
    let len = rt_str_len(reply);
    rt_str_free(reply);
    let err = net_last_error();
    net_close(sock);
    net_cleanup();
    if sent == 4 && len == 4 && err == 0 { return 1; }
    return 0;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let _ = server.join();
    assert_bounded_success(code, &out, &err, "n109");
}

// T2 — the peer accepts and sends nothing: the read returns inside the
// budget, reports WSAETIMEDOUT, and yields an owned empty Str.
#[test]
fn n110_read_timeout_returns_at_the_budget_with_wsaetimedout() {
    let (port, release, server) = silent_peer();
    let net = net_lib();
    let time = time_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    if sock == -1 { net_cleanup(); return 0; }
    if net_connect_timeout(sock, "127.0.0.1", __PORT__, 5000) != 0 {
        net_close(sock);
        net_cleanup();
        return 0;
    }
    let t0 = time_millis();
    let data = net_recv_timeout(sock, 64, 150);
    let elapsed = time_millis() - t0;
    let len = rt_str_len(data);
    rt_str_free(data);
    let err = net_last_error();
    net_close(sock);
    net_cleanup();
    if len == 0 && err == 10060 && elapsed >= 100 && elapsed < 5000 { return 1; }
    return 0;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) =
        build_and_run_bounded(&src, &[&net, &time], Duration::from_secs(25));
    let _ = release.send(());
    let _ = server.join();
    assert_bounded_success(code, &out, &err, "n110");
}

// T3 — data that arrives before the budget is returned normally, with no
// timeout error left behind.
#[test]
fn n111_data_before_the_timeout_is_received_normally() {
    let (port, server) = timed_peer(&[(300, b"mink")]);
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    if sock == -1 { net_cleanup(); return 0; }
    if net_connect_timeout(sock, "127.0.0.1", __PORT__, 5000) != 0 {
        net_close(sock);
        net_cleanup();
        return 0;
    }
    let data = net_recv_timeout(sock, 64, 8000);
    let len = rt_str_len(data);
    rt_str_free(data);
    let err = net_last_error();
    net_close(sock);
    net_cleanup();
    if len == 4 && err == 0 { return 1; }
    return 0;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let _ = server.join();
    assert_bounded_success(code, &out, &err, "n111");
}

// T4 — the same socket may time out repeatedly; a real read afterwards is
// still correct, so neither the socket nor the error path is corrupted.
#[test]
fn n112_repeated_timeouts_on_one_socket_leave_it_usable() {
    let (port, server) = timed_peer(&[(900, b"late")]);
    let net = net_lib();
    let time = time_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    if sock == -1 { net_cleanup(); return 0; }
    if net_connect_timeout(sock, "127.0.0.1", __PORT__, 5000) != 0 {
        net_close(sock);
        net_cleanup();
        return 0;
    }
    let t0 = time_millis();
    let mut timed_out = 0;
    let mut i = 0;
    while i < 3 {
        let empty = net_recv_timeout(sock, 64, 200);
        let len = rt_str_len(empty);
        rt_str_free(empty);
        if len == 0 && net_last_error() == 10060 { timed_out = timed_out + 1; }
        i = i + 1;
    }
    let elapsed = time_millis() - t0;
    let data = net_recv_timeout(sock, 64, 8000);
    let len = rt_str_len(data);
    rt_str_free(data);
    let err = net_last_error();
    net_close(sock);
    net_cleanup();
    if timed_out == 3 && elapsed >= 400 && elapsed < 8000 && len == 4 && err == 0 { return 1; }
    return 0;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) =
        build_and_run_bounded(&src, &[&net, &time], Duration::from_secs(25));
    let _ = server.join();
    assert_bounded_success(code, &out, &err, "n112");
}

// T5 — the budget is per call, so it can be raised on the same socket.
#[test]
fn n113_the_timeout_can_be_changed_between_reads() {
    let (port, server) = timed_peer(&[(500, b"later")]);
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    if sock == -1 { net_cleanup(); return 0; }
    if net_connect_timeout(sock, "127.0.0.1", __PORT__, 5000) != 0 {
        net_close(sock);
        net_cleanup();
        return 0;
    }
    let first = net_recv_timeout(sock, 64, 100);
    let first_len = rt_str_len(first);
    rt_str_free(first);
    let first_err = net_last_error();
    let second = net_recv_timeout(sock, 64, 8000);
    let second_len = rt_str_len(second);
    rt_str_free(second);
    let second_err = net_last_error();
    net_close(sock);
    net_cleanup();
    if first_len == 0 && first_err == 10060 && second_len == 5 && second_err == 0 { return 1; }
    return 0;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let _ = server.join();
    assert_bounded_success(code, &out, &err, "n113");
}

// T6 — a successful `net_connect_timeout` leaves the socket in the documented
// default (blocking) mode, proven by a plain blocking `net_recv` that waits
// for data sent afterwards.
#[test]
fn n114_successful_connect_timeout_restores_blocking_mode() {
    let (port, server) = timed_peer(&[(300, b"block")]);
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    if sock == -1 { net_cleanup(); return 0; }
    if net_connect_timeout(sock, "127.0.0.1", __PORT__, 5000) != 0 {
        net_close(sock);
        net_cleanup();
        return 0;
    }
    let data = net_recv(sock, 64);
    let len = rt_str_len(data);
    rt_str_free(data);
    let err = net_last_error();
    net_close(sock);
    net_cleanup();
    if len == 5 && err == 0 { return 1; }
    return 0;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let _ = server.join();
    assert_bounded_success(code, &out, &err, "n114");
}

// T7 — the contract for zero, negative and out-of-range budgets: 0 polls
// once, negative waits indefinitely, and a value beyond INT_MAX is capped
// rather than truncated into a wrong wait.
#[test]
fn n115_zero_negative_and_oversized_timeouts_follow_the_contract() {
    let (port, server) = timed_peer(&[(500, b"first"), (900, b"second")]);
    let net = net_lib();
    let time = time_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    if sock == -1 { net_cleanup(); return 0; }
    if net_connect_timeout(sock, "127.0.0.1", __PORT__, 5000) != 0 {
        net_close(sock);
        net_cleanup();
        return 0;
    }
    // 0: one poll, no wait.
    let t0 = time_millis();
    let immediate = net_recv_timeout(sock, 64, 0);
    let immediate_ms = time_millis() - t0;
    let immediate_len = rt_str_len(immediate);
    rt_str_free(immediate);
    let immediate_err = net_last_error();
    // negative: no limit; the peer writes at 500 ms.
    let first = net_recv_timeout(sock, 64, -1);
    let first_len = rt_str_len(first);
    rt_str_free(first);
    let first_err = net_last_error();
    // INT_MAX + 1: capped, not truncated; the peer writes "second" at 900 ms.
    let second = net_recv_timeout(sock, 64, 9223372036854775807);
    let second_len = rt_str_len(second);
    rt_str_free(second);
    let second_err = net_last_error();
    net_close(sock);
    net_cleanup();
    if immediate_len == 0 && immediate_err == 10060 && immediate_ms < 4000 {
        if first_len == 5 && first_err == 0 {
            if second_len == 6 && second_err == 0 { return 1; }
        }
    }
    return 0;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) =
        build_and_run_bounded(&src, &[&net, &time], Duration::from_secs(25));
    let _ = server.join();
    assert_bounded_success(code, &out, &err, "n115");
}

// T8 — a timed-out operation followed by close is clean: the close succeeds,
// the cleanup succeeds, and nothing crashes.
#[test]
fn n116_close_after_a_timeout_is_clean() {
    let (port, release, server) = silent_peer();
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    if sock == -1 { net_cleanup(); return 0; }
    if net_connect_timeout(sock, "127.0.0.1", __PORT__, 5000) != 0 {
        net_close(sock);
        net_cleanup();
        return 0;
    }
    let empty = net_recv_timeout(sock, 64, 100);
    let len = rt_str_len(empty);
    rt_str_free(empty);
    let err = net_last_error();
    let closed = net_close(sock);
    let cleaned = net_cleanup();
    if len == 0 && err == 10060 && closed == 0 && cleaned == 0 { return 1; }
    return 0;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let _ = release.send(());
    let _ = server.join();
    assert_bounded_success(code, &out, &err, "n116");
}

// T9 — timeouts are per call, not per socket or per process: one socket's
// expiry cannot shorten or lengthen another socket's budget.
#[test]
fn n117_one_sockets_timeout_does_not_affect_another_socket() {
    let (silent_port, release, silent) = silent_peer();
    let (slow_port, slow) = timed_peer(&[(400, b"b")]);
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let quiet = net_tcp_socket();
    let talker = net_tcp_socket();
    if quiet == -1 || talker == -1 {
        net_cleanup();
        return 0;
    }
    if net_connect_timeout(quiet, "127.0.0.1", __QUIET_PORT__, 5000) != 0 {
        net_close(quiet);
        net_close(talker);
        net_cleanup();
        return 0;
    }
    if net_connect_timeout(talker, "127.0.0.1", __SLOW_PORT__, 5000) != 0 {
        net_close(quiet);
        net_close(talker);
        net_cleanup();
        return 0;
    }
    let a = net_recv_timeout(quiet, 64, 150);
    let a_len = rt_str_len(a);
    rt_str_free(a);
    let a_err = net_last_error();
    let b = net_recv_timeout(talker, 64, 5000);
    let b_len = rt_str_len(b);
    rt_str_free(b);
    let b_err = net_last_error();
    net_close(quiet);
    net_close(talker);
    net_cleanup();
    if a_len == 0 && a_err == 10060 && b_len == 1 && b_err == 0 { return 1; }
    return 0;
}
"#
    .replace("__QUIET_PORT__", &silent_port.to_string())
    .replace("__SLOW_PORT__", &slow_port.to_string());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let _ = release.send(());
    let _ = silent.join();
    let _ = slow.join();
    assert_bounded_success(code, &out, &err, "n117");
}

// T10 — connect-outcome reporting. A refused local port is deterministic:
// the non-blocking attempt finishes and SO_ERROR carries the real
// WSAECONNREFUSED code, which the call republishes. A locally reproducible
// "SYN never answered" peer does not exist on Windows loopback without
// privileged network reconfiguration, so the bounded-wait mechanism itself is
// covered by the read tests above and this pins the connect result path.
#[test]
fn n118_connect_timeout_reports_the_socket_error_of_a_refused_port() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    if sock == -1 { net_cleanup(); return 0; }
    let result = net_connect_timeout(sock, "127.0.0.1", __PORT__, 5000);
    let err = net_last_error();
    net_close(sock);
    net_cleanup();
    if result == -1 && err == 10061 { return 1; }
    return 0;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    assert_bounded_success(code, &out, &err, "n118");
}

// T10b — the writable readiness primitive itself. It is the step the connect
// timeout waits on, and its event mask used to contain POLLWRBAND, which
// WSAPoll rejects with WSAEINVAL; the SOCKET_ERROR then arrived as
// 0x00000000FFFFFFFF and read as "ready". A valid mask must report 1 (not
// -1, and not a huge unsigned word) on a connected socket.
#[test]
fn n119_writable_readiness_uses_a_valid_event_mask() {
    let (port, server) = timed_peer(&[(400, b"ready")]);
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    if sock == -1 { net_cleanup(); return 0; }
    if net_connect(sock, "127.0.0.1", __PORT__) != 0 {
        net_close(sock);
        net_cleanup();
        return 0;
    }
    let ready = net_wait_writable(sock, 2000);
    let err = net_last_error();
    let writable = net_can_write(sock);
    net_close(sock);
    net_cleanup();
    if ready == 1 && err == 0 && writable { return 1; }
    return 0;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let _ = server.join();
    assert_bounded_success(code, &out, &err, "n119");
}

// ==========================================================================
// SECTION 15: UDP receive-buffer parity (Session 146)
// ==========================================================================
//
// `net_recvfrom` used to receive into the process-global 4096-byte BSS
// `recv_buf` and clamp `maxlen` to its 4088 usable bytes. A datagram larger
// than 4088 failed with WSAEMSGSIZE no matter how large a buffer the caller
// asked for, `maxlen == 0` silently received up to 4088 bytes, and two
// receives could overwrite the shared destination between `recvfrom` and the
// copy into the returned Str. The destination is now a per-call Str of
// `maxlen` bytes, exactly as `net_recv` has been since Session 133.
//
// The peer below is a Rust `UdpSocket` on 127.0.0.1:0. The MINK socket binds
// port 0 (the OS chooses) and announces itself with a "ready" datagram; the
// peer replies to the source address, so no port coordination and no timing
// race is involved. Every round echoes the received bytes back and the test
// compares the echo byte for byte — a truncated, corrupted or dropped
// datagram cannot pass on a length or an exit code alone.

fn udp_peer(payloads: Vec<Vec<u8>>) -> (u16, std::thread::JoinHandle<Vec<Vec<u8>>>) {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let handle = std::thread::spawn(move || {
        let mut echoes = Vec::new();
        let mut buf = vec![0u8; 65536];
        for payload in payloads {
            let (n, from) = match socket.recv_from(&mut buf) {
                Ok(v) => v,
                Err(_) => return echoes,
            };
            if &buf[..n] != b"ready" {
                return echoes;
            }
            if socket.send_to(&payload, from).is_err() {
                return echoes;
            }
            match socket.recv_from(&mut buf) {
                Ok((n, _)) => echoes.push(buf[..n].to_vec()),
                Err(_) => return echoes,
            }
        }
        echoes
    });
    (port, handle)
}

/// A peer that only replies: it announces nothing back and exits after the
/// single send, for the tests that free the datagram instead of echoing it.
fn udp_send_peer(payload: Vec<u8>) -> (u16, std::thread::JoinHandle<()>) {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let handle = std::thread::spawn(move || {
        let mut buf = [0u8; 16];
        let (n, from) = match socket.recv_from(&mut buf) {
            Ok(v) => v,
            Err(_) => return,
        };
        if &buf[..n] != b"ready" {
            return;
        }
        let _ = socket.send_to(&payload, from);
    });
    (port, handle)
}

/// A one-round MINK program: bind an ephemeral loopback port, announce it,
/// receive one datagram with `maxlen`, check the length and the error, echo
/// the bytes back, and close cleanly.
fn udp_round_src(port: u16, maxlen: usize, expected: usize) -> String {
    const TEMPLATE: &str = r#"
fn main() -> Int {
    net_init();
    let sock = net_udp_socket();
    if sock == -1 { return 2; }
    if net_bind(sock, "127.0.0.1", 0) != 0 {
        net_close(sock);
        net_cleanup();
        return 3;
    }
    if net_sendto(sock, "ready", "127.0.0.1", __PORT__) != 5 {
        net_close(sock);
        net_cleanup();
        return 4;
    }
    let data = net_recvfrom(sock, __MAXLEN__);
    let len = rt_str_len(data);
    let err = net_last_error();
    if len != __EXPECTED__ || err != 0 {
        rt_str_free(data);
        net_close(sock);
        net_cleanup();
        return 5;
    }
    let sent = net_sendto(sock, data, "127.0.0.1", __PORT__);
    let closed = net_close(sock);
    let cleaned = net_cleanup();
    if sent == __EXPECTED__ && closed == 0 && cleaned == 0 { return 1; }
    return 6;
}
"#;
    TEMPLATE
        .replace("__PORT__", &port.to_string())
        .replace("__MAXLEN__", &maxlen.to_string())
        .replace("__EXPECTED__", &expected.to_string())
}

#[track_caller]
fn assert_udp_round(
    code: Option<i32>,
    out: &[u8],
    err: &[u8],
    echoes: Vec<Vec<u8>>,
    want: &[u8],
    what: &str,
) {
    assert_bounded_success(code, out, err, what);
    assert_eq!(
        echoes.len(),
        1,
        "{what}: expected one echo, got {}",
        echoes.len()
    );
    assert_eq!(
        echoes[0].as_slice(),
        want,
        "{what}: echoed bytes differ ({} vs {} bytes)",
        echoes[0].len(),
        want.len()
    );
}

#[track_caller]
fn assert_udp_rounds(
    code: Option<i32>,
    out: &[u8],
    err: &[u8],
    echoes: Vec<Vec<u8>>,
    want: &[Vec<u8>],
    what: &str,
) {
    assert_bounded_success(code, out, err, what);
    assert_eq!(
        echoes.len(),
        want.len(),
        "{what}: expected {} echoes, got {}",
        want.len(),
        echoes.len()
    );
    for (i, (got, want)) in echoes.iter().zip(want).enumerate() {
        assert_eq!(
            got.as_slice(),
            want.as_slice(),
            "{what}: round {i} echoed {} bytes, expected {} (content differs)",
            got.len(),
            want.len()
        );
    }
}

// U1 — a small datagram is received and echoed byte for byte.
#[test]
fn u01_udp_small_datagram_round_trip_is_byte_exact() {
    let payload = b"hello-udp".to_vec();
    let (port, peer) = udp_peer(vec![payload.clone()]);
    let net = net_lib();
    let src = udp_round_src(port, 64, payload.len());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let echoes = peer.join().unwrap_or_default();
    assert_udp_round(code, &out, &err, echoes, &payload, "u01");
}

// U2 — a datagram larger than the retired 4088-byte scratch buffer. This is
// the pre-fix failure: the old clamp made recvfrom reject it with
// WSAEMSGSIZE even though `maxlen` was 8192.
#[test]
fn u02_udp_large_datagram_above_the_old_buffer_is_received_whole() {
    let payload = vec![b'x'; 4096];
    let (port, peer) = udp_peer(vec![payload.clone()]);
    let net = net_lib();
    let src = udp_round_src(port, 8192, payload.len());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let echoes = peer.join().unwrap_or_default();
    assert_udp_round(code, &out, &err, echoes, &payload, "u02");
}

// U3 — the exact retired boundary: 4088 bytes fit the old buffer, 4089 did
// not. Both must be received whole now, on the same socket, in sequence.
#[test]
fn u03_udp_boundary_at_the_old_4088_limit() {
    let first = vec![b'a'; 4088];
    let second = vec![b'b'; 4089];
    let (port, peer) = udp_peer(vec![first.clone(), second.clone()]);
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_udp_socket();
    if sock == -1 { return 2; }
    if net_bind(sock, "127.0.0.1", 0) != 0 { net_close(sock); net_cleanup(); return 3; }

    if net_sendto(sock, "ready", "127.0.0.1", __PORT__) != 5 { net_close(sock); net_cleanup(); return 4; }
    let a = net_recvfrom(sock, 4088);
    let alen = rt_str_len(a);
    let aerr = net_last_error();
    if alen != 4088 || aerr != 0 { rt_str_free(a); net_close(sock); net_cleanup(); return 5; }
    if net_sendto(sock, a, "127.0.0.1", __PORT__) != 4088 { net_close(sock); net_cleanup(); return 6; }

    if net_sendto(sock, "ready", "127.0.0.1", __PORT__) != 5 { net_close(sock); net_cleanup(); return 7; }
    let b = net_recvfrom(sock, 4089);
    let blen = rt_str_len(b);
    let berr = net_last_error();
    if blen != 4089 || berr != 0 { rt_str_free(b); net_close(sock); net_cleanup(); return 8; }
    if net_sendto(sock, b, "127.0.0.1", __PORT__) != 4089 { net_close(sock); net_cleanup(); return 9; }

    let closed = net_close(sock);
    let cleaned = net_cleanup();
    if closed == 0 && cleaned == 0 { return 1; }
    return 10;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let echoes = peer.join().unwrap_or_default();
    assert_udp_rounds(code, &out, &err, echoes, &[first, second], "u03");
}

// U4 — embedded NUL and high bytes survive the receive unchanged.
#[test]
fn u04_udp_embedded_nul_and_binary_bytes_are_preserved() {
    let payload = vec![b'a', 0, b'b', 0, b'c', 0xFF];
    let (port, peer) = udp_peer(vec![payload.clone()]);
    let net = net_lib();
    let src = udp_round_src(port, 16, payload.len());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let echoes = peer.join().unwrap_or_default();
    assert_udp_round(code, &out, &err, echoes, &payload, "u04");
}

// U5 — repeated datagrams of different sizes on one socket: no stale bytes,
// no stale length, and a zero-length datagram in the middle.
#[test]
fn u05_udp_repeated_datagrams_of_different_sizes() {
    let payloads = vec![
        b"xyz".to_vec(),
        vec![b'm'; 4096],
        Vec::new(),
        b"hello-udp".to_vec(),
    ];
    let (port, peer) = udp_peer(payloads.clone());
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_udp_socket();
    if sock == -1 { return 2; }
    if net_bind(sock, "127.0.0.1", 0) != 0 { net_close(sock); net_cleanup(); return 3; }

    if net_sendto(sock, "ready", "127.0.0.1", __PORT__) != 5 { net_close(sock); net_cleanup(); return 4; }
    let a = net_recvfrom(sock, 16);
    let al = rt_str_len(a);
    let ae = net_last_error();
    if al != 3 || ae != 0 { rt_str_free(a); net_close(sock); net_cleanup(); return 5; }
    if net_sendto(sock, a, "127.0.0.1", __PORT__) != 3 { net_close(sock); net_cleanup(); return 6; }

    if net_sendto(sock, "ready", "127.0.0.1", __PORT__) != 5 { net_close(sock); net_cleanup(); return 7; }
    let b = net_recvfrom(sock, 8192);
    let bl = rt_str_len(b);
    let be = net_last_error();
    if bl != 4096 || be != 0 { rt_str_free(b); net_close(sock); net_cleanup(); return 8; }
    if net_sendto(sock, b, "127.0.0.1", __PORT__) != 4096 { net_close(sock); net_cleanup(); return 9; }

    if net_sendto(sock, "ready", "127.0.0.1", __PORT__) != 5 { net_close(sock); net_cleanup(); return 10; }
    let c = net_recvfrom(sock, 16);
    let cl = rt_str_len(c);
    let ce = net_last_error();
    if cl != 0 || ce != 0 { rt_str_free(c); net_close(sock); net_cleanup(); return 11; }
    if net_sendto(sock, c, "127.0.0.1", __PORT__) != 0 { net_close(sock); net_cleanup(); return 12; }

    if net_sendto(sock, "ready", "127.0.0.1", __PORT__) != 5 { net_close(sock); net_cleanup(); return 13; }
    let d = net_recvfrom(sock, 64);
    let dl = rt_str_len(d);
    let de = net_last_error();
    if dl != 9 || de != 0 { rt_str_free(d); net_close(sock); net_cleanup(); return 14; }
    if net_sendto(sock, d, "127.0.0.1", __PORT__) != 9 { net_close(sock); net_cleanup(); return 15; }

    let closed = net_close(sock);
    let cleaned = net_cleanup();
    if closed == 0 && cleaned == 0 { return 1; }
    return 16;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let echoes = peer.join().unwrap_or_default();
    assert_udp_rounds(code, &out, &err, echoes, &payloads, "u05");
}

// U6 — a zero-length datagram is a successful empty receive (not a timeout and
// not an error), the sender metadata is still recorded, and the empty echo is
// delivered as a zero-length datagram.
#[test]
fn u06_udp_zero_length_datagram() {
    let payload = Vec::new();
    let (port, peer) = udp_peer(vec![payload.clone()]);
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_udp_socket();
    if sock == -1 { return 2; }
    if net_bind(sock, "127.0.0.1", 0) != 0 { net_close(sock); net_cleanup(); return 3; }
    if net_sendto(sock, "ready", "127.0.0.1", __PORT__) != 5 { net_close(sock); net_cleanup(); return 4; }
    let data = net_recvfrom(sock, 64);
    let len = rt_str_len(data);
    let err = net_last_error();
    let sport = net_sender_port();
    let sip = net_sender_ip();
    let sip_len = rt_str_len(sip);
    rt_str_free(sip);
    if len != 0 || err != 0 || sport != __PORT__ || sip_len != 9 {
        rt_str_free(data);
        net_close(sock);
        net_cleanup();
        return 5;
    }
    let sent = net_sendto(sock, data, "127.0.0.1", __PORT__);
    let closed = net_close(sock);
    let cleaned = net_cleanup();
    if sent == 0 && closed == 0 && cleaned == 0 { return 1; }
    return 6;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let echoes = peer.join().unwrap_or_default();
    assert_udp_round(code, &out, &err, echoes, &payload, "u06");
}

/// A peer that maps two announced sockets by message content (not by arrival
/// order) and then sends each its own payload, so U7 has no ordering race.
fn udp_two_peer(
    first: Vec<u8>,
    second: Vec<u8>,
) -> (u16, std::thread::JoinHandle<Vec<Vec<u8>>>) {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let handle = std::thread::spawn(move || {
        let mut addr_a = None;
        let mut addr_b = None;
        let mut small = [0u8; 16];
        while addr_a.is_none() || addr_b.is_none() {
            let (n, from) = match socket.recv_from(&mut small) {
                Ok(v) => v,
                Err(_) => return Vec::new(),
            };
            match &small[..n] {
                b"readyA" => addr_a = Some(from),
                b"readyB" => addr_b = Some(from),
                _ => {}
            }
        }
        if socket.send_to(&first, addr_a.unwrap()).is_err() {
            return Vec::new();
        }
        if socket.send_to(&second, addr_b.unwrap()).is_err() {
            return Vec::new();
        }
        let mut echoes = Vec::new();
        let mut buf = vec![0u8; 65536];
        for _ in 0..2 {
            match socket.recv_from(&mut buf) {
                Ok((n, _)) => echoes.push(buf[..n].to_vec()),
                Err(_) => return echoes,
            }
        }
        echoes
    });
    (port, handle)
}

// U7 — two independent UDP sockets with different payload sizes: one socket's
// receive cannot corrupt the other's bytes or length.
#[test]
fn u07_udp_two_independent_sockets_keep_their_own_datagrams() {
    let first = vec![b'q'; 4096];
    let second = b"hello-udp".to_vec();
    let (port, peer) = udp_two_peer(first.clone(), second.clone());
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let a = net_udp_socket();
    let b = net_udp_socket();
    if a == -1 { net_close(b); net_cleanup(); return 2; }
    if b == -1 { net_close(a); net_cleanup(); return 2; }
    if net_bind(a, "127.0.0.1", 0) != 0 { net_close(a); net_close(b); net_cleanup(); return 3; }
    if net_bind(b, "127.0.0.1", 0) != 0 { net_close(a); net_close(b); net_cleanup(); return 4; }

    if net_sendto(a, "readyA", "127.0.0.1", __PORT__) != 6 { net_close(a); net_close(b); net_cleanup(); return 5; }
    if net_sendto(b, "readyB", "127.0.0.1", __PORT__) != 6 { net_close(a); net_close(b); net_cleanup(); return 6; }

    let da = net_recvfrom(a, 8192);
    let dal = rt_str_len(da);
    let dae = net_last_error();
    let db = net_recvfrom(b, 64);
    let dbl = rt_str_len(db);
    let dbe = net_last_error();
    if dal != 4096 || dae != 0 || dbl != 9 || dbe != 0 {
        rt_str_free(da);
        rt_str_free(db);
        net_close(a);
        net_close(b);
        net_cleanup();
        return 7;
    }
    let ea = net_sendto(a, da, "127.0.0.1", __PORT__);
    let eb = net_sendto(b, db, "127.0.0.1", __PORT__);
    let ca = net_close(a);
    let cb = net_close(b);
    let cleaned = net_cleanup();
    if ea == 4096 && eb == 9 && ca == 0 && cb == 0 && cleaned == 0 { return 1; }
    return 8;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let echoes = peer.join().unwrap_or_default();
    assert_eq!(echoes.len(), 2, "u07: expected two echoes, got {}", echoes.len());
    let mut got = echoes.clone();
    got.sort();
    let mut want = vec![first, second];
    want.sort();
    assert_eq!(got, want, "u07: echoed payloads differ");
    assert_bounded_success(code, &out, &err, "u07");
}

// U8 — the error paths: sendto and recvfrom on a closed socket report failure
// through the established error channel and never fault or return garbage.
#[test]
fn u08_udp_send_and_receive_error_paths() {
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_udp_socket();
    if sock == -1 { net_cleanup(); return 2; }
    if net_bind(sock, "127.0.0.1", 0) != 0 { net_close(sock); net_cleanup(); return 3; }
    net_close(sock);

    let sent = net_sendto(sock, "x", "127.0.0.1", 19999);
    let send_err = net_last_error();
    let data = net_recvfrom(sock, 64);
    let len = rt_str_len(data);
    let recv_err = net_last_error();
    rt_str_free(data);
    net_cleanup();
    if sent == -1 && send_err != 0 && len == 0 && recv_err != 0 { return 1; }
    return 4;
}
"#;
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    assert_bounded_success(code, &out, &err, "u08");
}

// U9 — cleanup after a successful receive: the returned Str is freed, the
// socket closes and net_cleanup succeeds; the exit-time leak scan stays quiet.
#[test]
fn u09_udp_cleanup_after_successful_receive() {
    let payload = b"hello-udp".to_vec();
    let (port, peer) = udp_send_peer(payload.clone());
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_udp_socket();
    if sock == -1 { return 2; }
    if net_bind(sock, "127.0.0.1", 0) != 0 { net_close(sock); net_cleanup(); return 3; }
    if net_sendto(sock, "ready", "127.0.0.1", __PORT__) != 5 { net_close(sock); net_cleanup(); return 4; }
    let data = net_recvfrom(sock, 64);
    let len = rt_str_len(data);
    let err = net_last_error();
    rt_str_free(data);
    let closed = net_close(sock);
    let cleaned = net_cleanup();
    if len == 9 && err == 0 && closed == 0 && cleaned == 0 { return 1; }
    return 5;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let _ = peer.join();
    assert_bounded_success(code, &out, &err, "u09");
}

// U10 — cleanup after a failed (too-small buffer) receive: WSAEMSGSIZE leaves
// an owned empty Str, which is freed, and close/cleanup stay clean.
#[test]
fn u10_udp_cleanup_after_truncation_failure() {
    let payload = vec![b'x'; 4096];
    let (port, peer) = udp_send_peer(payload);
    let net = net_lib();
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_udp_socket();
    if sock == -1 { return 2; }
    if net_bind(sock, "127.0.0.1", 0) != 0 { net_close(sock); net_cleanup(); return 3; }
    if net_sendto(sock, "ready", "127.0.0.1", __PORT__) != 5 { net_close(sock); net_cleanup(); return 4; }
    let data = net_recvfrom(sock, 64);
    let len = rt_str_len(data);
    let err = net_last_error();
    rt_str_free(data);
    let closed = net_close(sock);
    let cleaned = net_cleanup();
    if len == 0 && err == 10040 && closed == 0 && cleaned == 0 { return 1; }
    return 5;
}
"#
    .replace("__PORT__", &port.to_string());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let _ = peer.join();
    assert_bounded_success(code, &out, &err, "u10");
}

// U11 — the documented maximum: a 65,507-byte IPv4 UDP payload is received
// whole when `maxlen` is the same size. The pre-fix clamp made this 10040.
#[test]
fn u11_udp_max_ipv4_payload_round_trip() {
    let payload = vec![b'z'; 65507];
    let (port, peer) = udp_peer(vec![payload.clone()]);
    let net = net_lib();
    let src = udp_round_src(port, 65507, payload.len());
    let (code, out, err) = build_and_run_bounded(&src, &[&net], Duration::from_secs(25));
    let echoes = peer.join().unwrap_or_default();
    assert_udp_round(code, &out, &err, echoes, &payload, "u11");
}
