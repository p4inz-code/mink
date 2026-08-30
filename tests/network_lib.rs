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

fn native_stdout(source: &str) -> String {
    let (_, stdout, _) = build_and_run(source);
    String::from_utf8_lossy(&stdout).to_string()
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
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    let result = net_bind(sock, "127.0.0.1", 19876);
    net_close(sock);
    net_cleanup();
    return result;
}"#;
    assert_eq!(native_exit_code(src), 0);
}

#[test]
fn n21_listen_succeeds() {
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    net_bind(sock, "127.0.0.1", 19877);
    let result = net_listen(sock, 1);
    net_close(sock);
    net_cleanup();
    return result;
}"#;
    assert_eq!(native_exit_code(src), 0);
}

#[test]
fn n22_listen_on_all_interfaces() {
    let src = r#"
fn main() -> Int {
    net_init();
    let sock = net_tcp_socket();
    let result = net_bind(sock, "0.0.0.0", 19878);
    net_close(sock);
    net_cleanup();
    return result;
}"#;
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
fn n92_resolve_returns_input() {
    // V1: net_resolve just returns the host string (no allocation)
    let src = r#"
fn main() -> Int {
    net_init();
    let resolved = net_resolve("192.168.1.1", 80);
    let len = rt_str_len(resolved);
    net_cleanup();
    // V1 resolve returns a copy with same length as input (11 chars)
    if len == 11 { return 1; }
    return 0;
}"#;
    assert_eq!(native_exit_code(src), 1);
}
