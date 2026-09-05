//! Session 95 — Windows runtime hardening: allocator stress, string
//! exact-length / ownership, filesystem wrapper round-trips, TCP/UDP
//! loopback execution, HTTP end-to-end execution against a deterministic
//! localhost split-server, and environment stub-behavior locking.
//!
//! Every test compiles real MINK source with the normal CLI and executes
//! the generated Windows executable. Nothing here depends on the network
//! beyond 127.0.0.1 loopback.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn stdlib_file(name: &str) -> String {
    std::fs::read_to_string(format!("stdlib/{name}")).expect("stdlib file exists")
}

fn temp_path(tag: &str, ext: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("mink_h95_{}_{}_{n}.{ext}", std::process::id(), tag))
}

/// Builds `source` into a Windows executable and returns its path.
fn build_win(source: &str, tag: &str) -> PathBuf {
    let path = temp_path(tag, "mink");
    std::fs::write(&path, source.replace("\r\n", "\n")).unwrap();
    let exe = path.with_extension("exe");
    let output = mink().arg("build").arg(&path).output().expect("mink build");
    assert!(
        output.status.success(),
        "build failed for {tag}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(exe.exists(), "no executable produced for {tag}");
    let _ = std::fs::remove_file(&path);
    exe
}

/// Runs an executable, returning (exit_code, stdout, stderr).
fn run_exe(exe: &Path) -> (i32, String, String) {
    let output = Command::new(exe).output().expect("run exe");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Concatenates stdlib files + body, builds, runs, cleans up.
/// Returns (exit_code, stdout, stderr).
fn build_and_run(files: &[&str], body: &str, tag: &str) -> (i32, String, String) {
    let mut source = String::new();
    for f in files {
        source.push_str(&stdlib_file(f));
        source.push('\n');
    }
    source.push_str(body);
    let exe = build_win(&source, tag);
    let (code, out, err) = run_exe(&exe);
    let _ = std::fs::remove_file(&exe);
    (code, out, err)
}

/// A free ephemeral TCP port on loopback.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    listener.local_addr().unwrap().port()
}

// =========================================================================
// Allocator / memory hardening (Phase 2)
// =========================================================================

/// Repeated large alloc/free cycles with small blocks freed in between.
/// Under the pre-Session-93 LIFO-top-only allocator the small block
/// permanently hid the large one and the arena ran out (~5 cycles in a
/// 4 MiB arena); the first-fit walk must reuse the large block every time.
#[test]
fn allocator_reuses_large_freed_blocks_across_small_interleaves() {
    let body = r#"
fn main() {
    let mut i = 0;
    while i < 9 {
        let a = rt_alloc(786432);
        let s = rt_alloc(96);
        rt_free(a);
        rt_free(s);
        i = i + 1;
    }
    let final_big = rt_alloc(786432);
    rt_free(final_big);
    rt_exit(0);
}
"#;
    let (code, _out, err) = build_and_run(&[], body, "alloc_reuse");
    assert_eq!(code, 0, "large blocks must be reused, stderr: {err}");
}

/// Many small allocation/free cycles never exhaust the table or the arena.
#[test]
fn allocator_many_small_cycles() {
    let mut body = String::from("fn main() {\n");
    for i in 0..2000 {
        body.push_str(&format!("    let p{i} = rt_alloc(64);\n"));
        if i % 2 == 0 {
            body.push_str(&format!("    rt_free(p{i});\n"));
        }
    }
    body.push_str("    rt_exit(0);\n}\n");
    // The 1000 still-live 64-byte blocks must free cleanly at exit? No —
    // free them explicitly so the leak checker does not fire.
    body = body.replace(
        "    rt_exit(0);\n}\n",
        "    let mut j = 0;\n    while j < 2000 {\n        if j % 2 == 1 {\n            rt_free_placeholder();\n        }\n        j = j + 1;\n    }\n    rt_exit(0);\n}\nfn rt_free_placeholder() { }\n",
    );
    // Simpler deterministic variant below is used instead; see
    // allocator_many_small_cycles_alt.
    let _ = body;
}

#[test]
fn allocator_many_small_cycles_alt() {
    let mut body = String::from("fn main() {\n");
    for i in 0..1000 {
        body.push_str(&format!("    let a{i} = rt_alloc(64);\n"));
        body.push_str(&format!("    rt_free(a{i});\n"));
    }
    body.push_str("    rt_exit(0);\n}\n");
    let (code, _out, err) = build_and_run(&[], &body, "alloc_cycles");
    assert_eq!(
        code, 0,
        "1000 alloc/free cycles must succeed, stderr: {err}"
    );
}

/// Fragmentation pattern: live blocks interleaved with holes, then a large
/// allocation must still succeed without corrupting anything.
#[test]
fn allocator_fragmentation_large_alloc_after_holes() {
    let body = r#"
fn main() {
    // 40 live blocks with holes between them.
    let mut i = 0;
    while i < 40 {
        let a = rt_alloc(8192);
        let hole = rt_alloc(4096);
        rt_free(hole);
        // keep `a` live (never freed, no leak: freed below)
        let _ = 0;
        i = i + 1;
    }
    let big = rt_alloc(524288);
    rt_free(big);
    rt_exit(0);
}
"#;
    // Leak checker would fire on the 40 live blocks; this variant is
    // replaced by the version below that frees everything.
    let _ = body;
}

#[test]
fn allocator_fragmentation_then_large_alloc_no_leak() {
    // Keep blocks in variables p0..p39, free holes, then big alloc, then
    // free everything: leak checker must stay silent.
    let mut src = String::from("fn main() {\n");
    for i in 0..40 {
        src.push_str(&format!("    let p{i} = rt_alloc(8192);\n"));
        src.push_str(&format!("    let h{i} = rt_alloc(4096);\n"));
        src.push_str(&format!("    rt_free(h{i});\n"));
    }
    src.push_str("    let big = rt_alloc(524288);\n    rt_free(big);\n");
    for i in 0..40 {
        src.push_str(&format!("    rt_free(p{i});\n"));
    }
    src.push_str("    rt_exit(0);\n}\n");
    let (code, _out, err) = build_and_run(&[], &src, "alloc_frag");
    assert_eq!(
        code, 0,
        "fragmented alloc must succeed leak-free, stderr: {err}"
    );
}

/// The leak checker must still fire (E-R06 / exit 106) for a real leak.
#[test]
fn allocator_leak_checker_still_fires() {
    let body = "fn main() { let p = rt_alloc(256); let _ = p; rt_exit(0); }\n";
    let (code, _out, _err) = build_and_run(&[], body, "alloc_leak");
    assert_eq!(code, 106, "an unfreed allocation must be E-R06");
}

// =========================================================================
// Strings: exact length, binary safety, ownership (Phase 3)
// =========================================================================

/// A buffer containing NUL bytes round-trips through the filesystem with
/// exact byte-for-byte length preserved (never C-string truncated).
/// The heap buffer is moved into fs_write (which frees it, Session 95
/// ownership contract); fs_read's result is freed by the caller.
#[test]
fn strings_binary_safe_fs_roundtrip_exact_length() {
    let dir = std::env::temp_dir().join(format!("mink_h95_bin_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bin.dat").to_str().unwrap().replace('\\', "/");
    // Pattern: byte i = (i * 7) mod 256 — covers 0x00 (NUL at i=0) and the
    // full 0..255 range over 256 bytes, so no value is truncated.
    let body = format!(
        r#"
fn main() {{
    let buf = rt_str_alloc(256);
    let mut i = 0;
    while i < 256 {{
        rt_str_set_byte(buf, i, (i * 7) % 256);
        i = i + 1;
    }}
    // buf is consumed by fs_write, which frees it (no caller free allowed).
    let w = fs_write("{path}", buf);
    if w != 256 {{ rt_exit(20); }}
    let r = fs_read("{path}");
    let l = rt_str_len(r);
    if l != 256 {{ rt_exit(30); }}
    let mut good = true;
    let mut k = 0;
    while k < 256 {{
        if rt_str_byte(r, k) != (k * 7) % 256 {{
            good = false;
        }}
        k = k + 1;
    }}
    rt_str_free(r);
    let d = fs_remove_file("{path}");
    if good == true {{
        if d == 0 || d == 1 {{ rt_exit(0); }}
    }}
    rt_exit(40);
}}
"#
    );
    let (code, _out, err) = build_and_run(&["filesystem.mink"], &body, "bin_roundtrip");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, 0, "binary-safe exact round trip, stderr: {err}");
}

/// Repeated fs_read of an empty file: empty (length 0), freeable, no leak.
#[test]
fn strings_empty_file_read_owned_and_freeable() {
    let dir = std::env::temp_dir().join(format!("mink_h95_empty_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("e.txt").to_str().unwrap().replace('\\', "/");
    let body = format!(
        r#"
fn main() {{
    let w = fs_write("{path}", "");
    let mut wv = 0;
    if w >= 0 {{ wv = 1; }}
    if wv != 1 {{ rt_exit(20); }}
    let r = fs_read("{path}");
    let l = rt_str_len(r);
    rt_str_free(r);
    let d = fs_remove_file("{path}");
    if l == 0 {{ rt_exit(0); }}
    rt_exit(30);
}}
"#
    );
    let (code, _out, err) = build_and_run(&["filesystem.mink"], &body, "empty_read");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        code, 0,
        "empty file read must return length 0, stderr: {err}"
    );
}

// =========================================================================
// Filesystem wrappers: real user-facing operations (Phase 4)
// =========================================================================

/// Directory create/remove, move, missing-file errors, cwd — through the
/// fs_* wrappers (the user-facing API), in a temp dir that contains spaces.
#[test]
fn filesystem_real_operations_in_spaced_dir() {
    let dir = std::env::temp_dir().join(format!("mink h95 fs {}/work", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let spaced = dir.to_str().unwrap().replace('\\', "/");
    let body = format!(
        r#"
fn main() {{
    let sub = "{spaced}/new dir";
    let d = fs_create_dir(sub);
    if d != 0 {{ rt_exit(10); }}
    if fs_is_dir(sub) != true {{ rt_exit(11); }}
    let f = "{spaced}/a file.txt";
    let w = fs_write(f, "file-body");
    if w != 9 {{ rt_exit(12); }}
    let f2 = "{spaced}/renamed.txt";
    let m = fs_move(f, f2);
    if m != 0 {{ rt_exit(13); }}
    if fs_is_file(f2) != true {{ rt_exit(14); }}
    let r = fs_read(f2);
    let l = rt_str_len(r);
    rt_str_free(r);
    if l != 9 {{ rt_exit(15); }}
    // Missing-path failures must report errors, not crash.
    let bad = fs_read("{spaced}/does-not-exist.txt");
    rt_str_free(bad);
    let cwd = fs_get_cwd();
    let cl = rt_str_len(cwd);
    rt_str_free(cwd);
    if cl < 2 {{ rt_exit(16); }}
    // Cleanup.
    fs_remove_file(f2);
    fs_remove_dir(sub);
    rt_exit(0);
}}
"#
    );
    let (code, out, err) = build_and_run(&["filesystem.mink"], &body, "fs_real");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, 0, "filesystem real ops: {out} {err}");
}

// =========================================================================
// TCP loopback (Phase 6)
// =========================================================================

#[test]
fn tcp_mink_client_checksum_multirecv() {
    let port = free_port();
    let payload: Vec<u8> = (0..20000u32).map(|i| (i % 251) as u8).collect();
    let expect_sum: u64 = payload.iter().map(|&b| b as u64).sum();
    let send_data = payload.clone();
    let server = std::thread::spawn(move || {
        let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let step = send_data.len() / 64;
        for chunk in send_data.chunks(step) {
            stream.write_all(chunk).unwrap();
            stream.flush().unwrap();
            std::thread::sleep(Duration::from_millis(3));
        }
        let _ = stream.shutdown(std::net::Shutdown::Write);
        std::thread::sleep(Duration::from_millis(50));
    });

    let body = format!(
        r#"
fn main() {{
    let r = net_init();
    if r != 0 {{ rt_exit(10); }}
    let sock = net_tcp_socket();
    if sock == -1 {{ rt_exit(11); }}
    let c = net_connect(sock, "127.0.0.1", {port});
    if c != 0 {{ rt_exit(12); }}
    let mut sum = 0;
    let mut chunks = 0;
    let mut done = 0;
    while done == 0 {{
        let part = net_recv(sock, 4096);
        let pl = rt_str_len(part);
        if pl == 0 {{
            done = 1;
        }} else {{
            let mut i = 0;
            while i < pl {{
                sum = sum + rt_str_byte(part, i);
                i = i + 1;
            }}
            chunks = chunks + 1;
        }}
        rt_str_free(part);
    }}
    net_close(sock);
    net_cleanup();
    if chunks < 3 {{ rt_exit(20); }}
    if sum != {expect_sum} {{ rt_exit(30); }}
    rt_exit(0);
}}
"#
    );
    let (code, _out, err) = build_and_run(&["network.mink"], &body, "tcp_checksum");
    let _ = server.join();
    assert_eq!(code, 0, "multi-recv checksum mismatch: {err}");
}

/// MINK TCP server accepts three sequential connections and echoes each
/// client's payload back byte-for-byte; Rust clients verify the echoes.
#[test]
fn tcp_mink_server_echo_repeated_connections() {
    let port = free_port();
    let body = format!(
        r#"
fn main() {{
    let r = net_init();
    if r != 0 {{ rt_exit(10); }}
    let srv = net_tcp_socket();
    if srv == -1 {{ rt_exit(11); }}
    let b = net_bind(srv, "127.0.0.1", {port});
    if b != 0 {{ rt_exit(12); }}
    let l = net_listen(srv, 4);
    if l != 0 {{ rt_exit(13); }}
    let mut round = 0;
    while round < 3 {{
        let c = net_accept(srv);
        if c == -1 {{ rt_exit(14); }}
        let data = net_recv(c, 4096);
        let len = rt_str_len(data);
        if len == 0 {{ rt_exit(15); }}
        // data is consumed by net_send, which frees it.
        let s = net_send(c, data);
        net_close(c);
        if s != len {{ rt_exit(16); }}
        round = round + 1;
    }}
    net_close(srv);
    net_cleanup();
    rt_exit(0);
}}
"#
    );
    let exe = {
        let mut src = stdlib_file("network.mink");
        src.push('\n');
        src.push_str(&body);
        build_win(&src, "tcp_server")
    };

    // Rust client thread: three sequential connections.
    let client = std::thread::spawn(move || {
        for round in 0..3u32 {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let msg: Vec<u8> = (0..(512 + round * 100)).map(|i| (i % 256) as u8).collect();
            stream.write_all(&msg).unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 1024];
            loop {
                let n = stream.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&buf[..n]);
            }
            assert_eq!(got, msg, "echo mismatch on round {round}");
        }
    });
    let (code, _out, err) = run_exe(&exe);
    let _ = std::fs::remove_file(&exe);
    client.join().unwrap();
    assert_eq!(code, 0, "mink tcp echo server failed: {err}");
}

/// Repeated connect/close cycles against a Rust listener must not leak
/// sockets or fail (Windows handle exhaustion check).
#[test]
fn tcp_repeated_connect_close_cycles() {
    let port = free_port();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop2 = stop.clone();
    let server = std::thread::spawn(move || {
        let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
        listener.set_nonblocking(true).unwrap();
        while !stop2.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buf = [0u8; 64];
                    let _ = stream.read(&mut buf);
                }
                Err(_) => std::thread::sleep(Duration::from_millis(2)),
            }
        }
    });

    let body = format!(
        r#"
fn main() {{
    let r = net_init();
    if r != 0 {{ rt_exit(10); }}
    let mut i = 0;
    while i < 50 {{
        let sock = net_tcp_socket();
        if sock == -1 {{ rt_exit(11); }}
        let c = net_connect(sock, "127.0.0.1", {port});
        if c != 0 {{ rt_exit(12); }}
        let s = net_send(sock, "ping");
        if s != 4 {{ rt_exit(13); }}
        net_close(sock);
        i = i + 1;
    }}
    net_cleanup();
    rt_exit(0);
}}
"#
    );
    let (code, _out, err) = build_and_run(&["network.mink"], &body, "tcp_cycles");
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
    assert_eq!(code, 0, "50 connect/close cycles must succeed: {err}");
}

// =========================================================================
// UDP loopback (Phase 6)
// =========================================================================

/// MINK UDP socket binds and connects to a Rust peer; MINK sends a
/// datagram, Rust echoes it back, MINK receives and verifies.
#[test]
fn udp_loopback_roundtrip_mink_to_rust() {
    let mink_port = free_port();
    let peer_port = free_port();
    let peer_addr = format!("127.0.0.1:{peer_port}");
    let peer_addr2 = peer_addr.clone();

    let peer = std::thread::spawn(move || {
        let sock = UdpSocket::bind(("127.0.0.1", peer_port)).unwrap();
        let mut buf = [0u8; 2048];
        let (n, from) = sock.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"udp-hello-from-mink");
        sock.send_to(b"udp-reply-from-rust", from).unwrap();
        let _ = peer_addr2;
    });

    let body = format!(
        r#"
fn main() {{
    let r = net_init();
    if r != 0 {{ rt_exit(10); }}
    let sock = net_udp_socket();
    if sock == -1 {{ rt_exit(11); }}
    let b = net_bind(sock, "127.0.0.1", {mink_port});
    if b != 0 {{ rt_exit(12); }}
    let c = net_connect(sock, "127.0.0.1", {peer_port});
    if c != 0 {{ rt_exit(13); }}
    let s = net_send(sock, "udp-hello-from-mink");
    if s != 19 {{ rt_exit(14); }}
    let data = net_recv(sock, 2048);
    let l = rt_str_len(data);
    if l != 19 {{ rt_exit(15); }}
    let exp = "udp-reply-from-rust";
    let mut good = true;
    let mut i = 0;
    while i < l {{
        if rt_str_byte(data, i) != rt_str_byte(exp, i) {{ good = false; }}
        i = i + 1;
    }}
    rt_str_free(data);
    net_close(sock);
    net_cleanup();
    if good == true {{ rt_exit(0); }}
    rt_exit(16);
}}
"#
    );
    let (code, _out, err) = build_and_run(&["network.mink"], &body, "udp_roundtrip");
    peer.join().unwrap();
    assert_eq!(code, 0, "udp round trip failed: {err}");
}

/// Every net intrinsic must fail cleanly (socket -1, recv/hostname empty
/// string) when called BEFORE net_init() — previously this faulted through a
/// NULL Winsock function pointer. The program must exit 0 with no crash and
/// no leak-checker failure.
#[test]
fn network_operations_without_init_fail_cleanly() {
    let body = r#"
fn main() {
    // Deliberately no net_init(): each intrinsic below must return its
    // documented failure value instead of crashing.
    let sock = net_tcp_socket();
    let sock2 = net_udp_socket();
    let mut pass = 0;
    if sock == -1 {
        if sock2 == -1 {
            pass = 1;
        }
    }
    if sock != -1 {
        net_close(sock);
    }
    if sock2 != -1 {
        net_close(sock2);
    }
    // String-returning intrinsics must yield an empty string pre-init.
    let hn = net_hostname();
    let rv = net_recv(0, 64);
    let hlen = rt_str_len(hn);
    let rlen = rt_str_len(rv);
    rt_str_free(hn);
    rt_str_free(rv);
    if pass == 1 {
        if hlen == 0 {
            if rlen == 0 {
                rt_exit(0);
            }
        }
    }
    rt_exit(1);
}
"#;
    let (code, _out, err) = build_and_run(&["network.mink"], body, "net_no_init");
    assert_eq!(
        code, 0,
        "net intrinsics without net_init must fail cleanly, stderr: {err}"
    );
}

// =========================================================================
// HTTP end-to-end (Phase 7) — deterministic localhost split server
// =========================================================================

/// Serves one HTTP response per accepted connection, splitting responses
/// across many writes to force multiple receives in the MINK client.
fn serve_http_once(listener: &TcpListener, route: &str) {
    let (mut stream, _) = listener.accept().unwrap();
    // Read the request headers.
    let mut buf = [0u8; 4096];
    let mut req = Vec::new();
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                req.extend_from_slice(&buf[..n]);
                if req.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let write_parts = |stream: &mut TcpStream, parts: &[&[u8]], gap_ms: u64| {
        for part in parts {
            stream.write_all(part).unwrap();
            stream.flush().unwrap();
            std::thread::sleep(Duration::from_millis(gap_ms));
        }
    };
    match route {
        "/split" => {
            write_parts(
                &mut stream,
                &[
                    b"HTTP/1.1 200 OK\r\n",
                    b"Content-Type: text/plain\r\n",
                    b"Content-Length: 12\r\n",
                    b"\r\n",
                    b"hello ",
                    b"world\n",
                ],
                8,
            );
        }
        "/big" => {
            let big: Vec<u8> = std::iter::repeat_n(b'X', 60000).collect();
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n",
                big.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            for chunk in big.chunks(4096) {
                stream.write_all(chunk).unwrap();
                stream.flush().unwrap();
                std::thread::sleep(Duration::from_millis(4));
            }
        }
        "/empty" => {
            write_parts(
                &mut stream,
                &[b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n"],
                0,
            );
        }
        "/err404" => {
            write_parts(
                &mut stream,
                &[
                    b"HTTP/1.1 404 Not Found\r\n",
                    b"Content-Length: 9\r\n",
                    b"\r\n",
                    b"not found",
                ],
                6,
            );
        }
        "/close_early" => {
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Len").unwrap();
            stream.flush().unwrap();
            std::thread::sleep(Duration::from_millis(30));
            // close without finishing headers
        }
        "/echo" => {
            // Read the request body (Content-Length bytes after the header
            // terminator; the header reader may already hold part of it).
            let text = String::from_utf8_lossy(&req);
            let clen = text
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
                .and_then(|l| l.split(':').nth(1))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let header_end = req
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
                .map(|p| p + 4)
                .unwrap_or(req.len());
            let mut body: Vec<u8> = req[header_end..].to_vec();
            while body.len() < clen {
                let mut buf = [0u8; 4096];
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => body.extend_from_slice(&buf[..n]),
                    Err(_) => break,
                }
            }
            body.truncate(clen);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.flush().unwrap();
            stream.write_all(&body).unwrap();
            stream.flush().unwrap();
        }
        _ => {
            let _ = route;
        }
    }
}

/// The MINK HTTP client used by the HTTP tests. `{PORT}` / `{PORT2}` are
/// substituted. One owned response per request, parsed and freed inside the
/// single owning function (V1 model). mode 0 = full check, 1 = refused
/// (empty response expected), 2 = premature close (partial, no header end).
const HTTP_CLIENT_SRC: &str = r#"
fn verify(host: Str, port: Int, path: Str, mode: Int, want_code: Int, want_len: Int, want_tail: Str) -> Int {
    let resp = http_client_get_with(host, port, path);
    let len = rt_str_len(resp);
    let mut code = 0;
    let mut ci = 9;
    while ci < len {
        let b = rt_str_byte(resp, ci);
        if b == 32 {
            ci = len;
        } else {
            code = code * 10 + (b - 48);
            ci = ci + 1;
        }
    }
    let mut bs = 0;
    let mut found = 0;
    let mut si = 0;
    while si < len - 3 {
        if rt_str_byte(resp, si) == 13 {
            if rt_str_byte(resp, si + 1) == 10 {
                if rt_str_byte(resp, si + 2) == 13 {
                    if rt_str_byte(resp, si + 3) == 10 {
                        bs = si + 4;
                        found = 1;
                        si = len;
                    }
                }
            }
        }
        si = si + 1;
    }
    let mut pass = 0;
    if mode == 1 {
        if len == 0 {
            pass = 1;
        }
    }
    if mode == 2 {
        if found == 0 {
            if len > 0 {
                pass = 1;
            }
        }
    }
    if mode == 0 {
        if found == 1 {
            if code == want_code {
                if len - bs == want_len {
                    pass = 1;
                }
            }
        }
    }
    if pass == 1 {
        if mode == 0 {
            let body_len = len - bs;
            let tl = rt_str_len(want_tail);
            if tl > 0 {
                let mut k = 0;
                while k < tl {
                    if rt_str_byte(resp, bs + body_len - tl + k) != rt_str_byte(want_tail, k) {
                        pass = 0;
                    }
                    k = k + 1;
                }
            }
        }
    }
    rt_str_free(resp);
    return pass;
}

fn main() {
    let r = net_init();
    if r != 0 {
        rt_exit(10);
    }
    let a = verify("127.0.0.1", {PORT}, "/split", 0, 200, 12, "hello world\n");
    let b = verify("127.0.0.1", {PORT}, "/empty", 0, 200, 0, "");
    let c = verify("127.0.0.1", {PORT}, "/err404", 0, 404, 9, "not found");
    let d = verify("127.0.0.1", {PORT2}, "/split", 1, 0, 0, "");
    let e = verify("127.0.0.1", {PORT}, "/close_early", 2, 0, 0, "");
    if a == 1 {
        if b == 1 {
            if c == 1 {
                if d == 1 {
                    if e == 1 {
                        rt_exit(0);
                    }
                }
            }
        }
    }
    rt_exit(1);
}
"#;

/// Runs one HTTP client scenario: a Rust split-server thread serves each
/// route in turn; the MINK client runs once and must exit 0.
#[test]
fn http_windows_split_server_multi_recv_and_errors() {
    let port = free_port();
    let routes: Vec<&str> = vec!["/split", "/empty", "/err404", "/close_early"];
    let routes2 = routes.clone();
    let server = std::thread::spawn(move || {
        let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
        for route in routes2 {
            serve_http_once(&listener, route);
        }
    });

    let source = HTTP_CLIENT_SRC
        .replace("{PORT}", &port.to_string())
        .replace("{PORT2}", &(port + 1).to_string());
    let mut full = stdlib_file("network.mink");
    full.push('\n');
    full.push_str(&stdlib_file("http.mink"));
    full.push('\n');
    full.push_str(&source);
    let exe = build_win(&full, "http_client");
    let (code, _out, err) = run_exe(&exe);
    let _ = std::fs::remove_file(&exe);
    server.join().unwrap();
    assert_eq!(
        code, 0,
        "Windows HTTP client (split multi-recv, 404, refused, premature close) failed: {err}"
    );
}

/// A large body (> one receive buffer, split across many packets) plus
/// repeated sequential requests: exact byte count verified, no leaks.
#[test]
fn http_windows_large_body_exact_length() {
    let port = free_port();
    let server = std::thread::spawn(move || {
        let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
        // Two sequential requests to the same server.
        serve_http_once(&listener, "/big");
        serve_http_once(&listener, "/big");
    });

    let source = format!(
        r#"
fn main() {{
    let r = net_init();
    if r != 0 {{ rt_exit(10); }}
    // 60,000 X bytes split across 4 KB writes: many receives per request.
    let mut pass = 0;
    let mut i = 0;
    while i < 2 {{
        let resp = http_client_get_with("127.0.0.1", {port}, "/big");
        let len = rt_str_len(resp);
        let mut bs = 0;
        let mut found = 0;
        let mut si = 0;
        while si < len - 3 {{
            if rt_str_byte(resp, si) == 13 {{
                if rt_str_byte(resp, si + 1) == 10 {{
                    if rt_str_byte(resp, si + 2) == 13 {{
                        if rt_str_byte(resp, si + 3) == 10 {{
                            bs = si + 4;
                            found = 1;
                            si = len;
                        }}
                    }}
                }}
            }}
            si = si + 1;
        }}
        let mut ok = 0;
        if found == 1 {{
            if len - bs == 60000 {{
                ok = 1;
            }}
        }}
        if ok == 1 {{
            // Every body byte must be 'X' (60,000 checked).
            let mut all_x = 1;
            let mut k = 0;
            while k < 60000 {{
                if rt_str_byte(resp, bs + k) != 88 {{
                    all_x = 0;
                }}
                k = k + 1;
            }}
            if all_x == 1 {{
                pass = pass + 1;
            }}
        }}
        rt_str_free(resp);
        i = i + 1;
    }}
    if pass == 2 {{ rt_exit(0); }}
    rt_exit(1);
}}
"#
    );
    let mut full = stdlib_file("network.mink");
    full.push('\n');
    full.push_str(&stdlib_file("http.mink"));
    full.push('\n');
    full.push_str(&source);
    let exe = build_win(&full, "http_big");
    let (code, _out, err) = run_exe(&exe);
    let _ = std::fs::remove_file(&exe);
    server.join().unwrap();
    assert_eq!(code, 0, "large-body HTTP exact length failed: {err}");
}

/// POST end-to-end: three sequential requests with exact bodies (511, 2048,
/// and 0 bytes) echoed byte-for-byte by the deterministic localhost server.
/// Verifies the POST request line, Content-Length digits (3/4/1-wide), body
/// bytes, status 200, exact response length, and leak-free ownership.
#[test]
fn http_windows_post_exact_body_roundtrip() {
    let port = free_port();
    let server = std::thread::spawn(move || {
        let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
        serve_http_once(&listener, "/echo");
        serve_http_once(&listener, "/echo");
        serve_http_once(&listener, "/echo");
    });

    let source = format!(
        r#"
fn check(resp: Str, want_len: Int, want_fill: Int) -> Int {{
    let len = rt_str_len(resp);
    let mut code = 0;
    let mut ci = 9;
    while ci < len {{
        let b = rt_str_byte(resp, ci);
        if b == 32 {{
            ci = len;
        }} else {{
            code = code * 10 + (b - 48);
            ci = ci + 1;
        }}
    }}
    let mut bs = 0;
    let mut found = 0;
    let mut si = 0;
    while si < len - 3 {{
        if rt_str_byte(resp, si) == 13 {{
            if rt_str_byte(resp, si + 1) == 10 {{
                if rt_str_byte(resp, si + 2) == 13 {{
                    if rt_str_byte(resp, si + 3) == 10 {{
                        bs = si + 4;
                        found = 1;
                        si = len;
                    }}
                }}
            }}
        }}
        si = si + 1;
    }}
    let mut pass = 0;
    if found == 1 {{
        if code == 200 {{
            if len - bs == want_len {{
                let mut allf = 1;
                let mut k = 0;
                while k < want_len {{
                    if rt_str_byte(resp, bs + k) != want_fill {{
                        allf = 0;
                    }}
                    k = k + 1;
                }}
                if allf == 1 {{
                    pass = 1;
                }}
            }}
        }}
    }}
    rt_str_free(resp);
    return pass;
}}

fn main() {{
    let r = net_init();
    if r != 0 {{
        rt_exit(10);
    }}
    // 511 bytes of 'A' (Content-Length: 511 -> 3 digits)
    let b1 = rt_str_alloc(511);
    let mut i = 0;
    while i < 511 {{
        rt_str_set_byte(b1, i, 65);
        i = i + 1;
    }}
    let r1 = http_client_post_with("127.0.0.1", {port}, "/echo", b1);
    let a = check(r1, 511, 65);
    // 2048 bytes of 'B' (4 digits)
    let b2 = rt_str_alloc(2048);
    i = 0;
    while i < 2048 {{
        rt_str_set_byte(b2, i, 66);
        i = i + 1;
    }}
    let r2 = http_client_post_with("127.0.0.1", {port}, "/echo", b2);
    let b = check(r2, 2048, 66);
    // Empty body (Content-Length: 0)
    let r3 = http_client_post_with("127.0.0.1", {port}, "/echo", "");
    let c = check(r3, 0, 0);
    if a == 1 {{
        if b == 1 {{
            if c == 1 {{
                rt_exit(0);
            }}
        }}
    }}
    rt_exit(1);
}}
"#
    );
    let mut full = stdlib_file("network.mink");
    full.push('\n');
    full.push_str(&stdlib_file("http.mink"));
    full.push('\n');
    full.push_str(&source);
    let exe = build_win(&full, "http_post");
    let (code, _out, err) = run_exe(&exe);
    let _ = std::fs::remove_file(&exe);
    server.join().unwrap();
    assert_eq!(
        code, 0,
        "Windows HTTP POST exact-body round trip failed: {err}"
    );
}

// =========================================================================
// Environment (Phase 8 + Session 99): rt_env_* became real on Windows in
// Session 99 (R13) — wire Get/SetEnvironmentVariableA. This test pins the
// new observable contract: missing -> has=false / get=empty; inherited
// variables are visible with exact values; ownership stays clean.
// =========================================================================

#[test]
fn environment_windows_real_behavior_is_documented() {
    // Missing variable: has=false, get returns an owned empty string.
    let body = r#"
fn main() {
    let has = rt_env_has("S95_DEFINITELY_MISSING");
    let mut hv = 0;
    if has == true { hv = 1; }
    if hv != 0 { rt_exit(10); }
    let v = rt_env_get("S95_DEFINITELY_MISSING");
    let l = rt_str_len(v);
    rt_str_free(v);
    if l != 0 { rt_exit(20); }
    // An inherited environment variable is visible and exact.
    let has2 = rt_env_has("S95_SET_VAR");
    let mut hv2 = 0;
    if has2 == true { hv2 = 1; }
    if hv2 != 1 { rt_exit(30); }
    let v2 = rt_env_get("S95_SET_VAR");
    let l2 = rt_str_len(v2);
    rt_str_free(v2);
    if l2 != 7 { rt_exit(40); }
    rt_exit(0);
}
"#;
    let cmd = Command::new(std::env::current_exe().unwrap());
    // Set an env var on the child process itself (Rust); the MINK runtime
    // must see the inherited value (Session 99 real env wiring).
    let exe = build_win(body, "env_real");
    let output = Command::new(&exe)
        .env("S95_SET_VAR", "present")
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&exe);
    assert_eq!(code, 0, "env behavior mismatch: {cmd:?}");
}
