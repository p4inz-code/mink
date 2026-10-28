//! Comprehensive tests for the MINK Networking library (Session 67).
//!
//! Tests basic Winsock2 initialization, socket creation, TCP operations,
//! error handling, and integration with other ecosystem libraries.

use std::path::PathBuf;
use std::process::Command;

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
fn n99_udp_recvfrom_clamps_maxlen() {
    // `maxlen` is a bound on the read, never an allocation size. A request
    // above the runtime receive buffer — or a nonsensical one — reads the
    // whole datagram instead of overrunning it. Winsock does not truncate a
    // datagram that does not fit: it fails with WSAEMSGSIZE, which surfaces as
    // an empty Str plus that code rather than a partial read.
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

    // A request past the 4088-byte receive buffer reads the datagram whole.
    if net_sendto(sock, "abcdefghij", "127.0.0.1", port) != 10 { return 5; }
    let big_read = net_recvfrom(sock, 1048576);
    let big_len = rt_str_len(big_read);
    rt_str_free(big_read);

    // A nonsensical request is clamped up to the buffer, not treated as zero.
    if net_sendto(sock, "xyz", "127.0.0.1", port) != 3 { return 6; }
    let odd_read = net_recvfrom(sock, 0);
    let odd_len = rt_str_len(odd_read);
    rt_str_free(odd_read);

    net_close(sock);
    net_cleanup();
    if short_len == 0 && short_err == 10040 && big_len == 10 && odd_len == 3 { return 1; }
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
