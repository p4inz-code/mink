//! Windows V1 Smoke & Integration Test Suite (Session 84)
//!
//! Tests the MINK compiler and runtime end-to-end using only intrinsics.
//! Stdlib functions (time_format, json_parse, fs_get_cwd, etc.) require
//! .mink files on disk and are tested in their respective test files.

use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn build_and_run(source: &str) -> (i32, String) {
    let out_dir = std::env::temp_dir();
    let mink_path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("mink.exe");
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let test_file = out_dir.join(format!("mink_smoke_t{}.mink", id));
    std::fs::write(&test_file, source).unwrap();
    let build = Command::new(&mink_path)
        .args(["build", test_file.to_str().unwrap()])
        .output()
        .unwrap();
    if !build.status.success() {
        let stderr = String::from_utf8_lossy(&build.stderr).to_string();
        return (build.status.code().unwrap_or(1), stderr);
    }
    let exe = out_dir.join(format!("mink_smoke_t{}.exe", id));
    let run = Command::new(&exe).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let code = run.status.code().unwrap_or(-1);
    (code, stdout)
}

// ============================================================
// 1. STRINGS
// ============================================================

#[test]
fn smoke_strings_basic() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let s = "hello world";
    rt_print_str(s);
    let len = rt_str_len(s);
    rt_print_int(len);
    let a = rt_str_alloc(3);
    rt_str_set_byte(a, 0, 72);
    rt_str_set_byte(a, 1, 73);
    rt_str_set_byte(a, 2, 33);
    rt_print_str(a);
    rt_str_free(a);
    rt_exit(0);
}"#,
    );
    assert_eq!(code, 0, "exit code: {out}");
    assert!(out.contains("hello world"), "{out}");
    assert!(out.contains("11"), "{out}");
    assert!(out.contains("HI!"), "{out}");
}

#[test]
fn smoke_string_concat() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = "hello";
    let b = rt_str_concat(a, " world");
    rt_print_str(b);
    rt_str_free(b);
    rt_exit(0);
}"#,
    );
    assert_eq!(code, 0, "exit code: {out}");
    assert!(out.contains("hello world"), "{out}");
}

// ============================================================
// 2. MATH
// ============================================================

#[test]
fn smoke_math_basic() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_print_int(2 + 3);
    rt_print_int(10 - 4);
    rt_print_int(3 * 7);
    rt_print_int(100 / 7);
    rt_exit(0);
}"#,
    );
    assert_eq!(code, 0, "exit code: {out}");
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines, vec!["5", "6", "21", "14"]);
}

// ============================================================
// 3. COLLECTIONS (Vec intrinsics)
// ============================================================

#[test]
fn smoke_vec_basic() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 20);
    v = rt_vec_push(v, 30);
    rt_print_int(rt_vec_len(v));
    rt_print_int(rt_vec_get(v, 0));
    rt_print_int(rt_vec_get(v, 1));
    rt_print_int(rt_vec_get(v, 2));
    rt_vec_free(v);
    rt_exit(0);
}"#,
    );
    assert_eq!(code, 0, "exit code: {out}");
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines, vec!["3", "10", "20", "30"]);
}

// ============================================================
// 4. FILESYSTEM — rt_fs_get_cwd returns no trailing NUL
// ============================================================

#[test]
fn smoke_fs_get_cwd_no_nul() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let cwd = rt_fs_get_cwd();
    let len = rt_str_len(cwd);
    let last = rt_str_byte(cwd, len - 1);
    rt_print_int(last);
    rt_str_free(cwd);
    rt_exit(0);
}"#,
    );
    assert_eq!(code, 0, "exit code: {out}");
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_ne!(lines.last(), Some(&"0"), "CWD ends with NUL! {out}");
}

// ============================================================
// 5. NETWORKING — connect to unreachable port returns error
// ============================================================

#[test]
fn smoke_network_connect_refused() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let init = rt_net_wsa_startup();
    let sock = rt_net_socket(2, 1, 6);
    let conn = rt_net_connect(sock, "127.0.0.1", 19999);
    if conn != 0 {
        rt_print_str("PASS");
    } else {
        rt_print_str("FAIL");
    }
    rt_net_close(sock);
    rt_net_wsa_cleanup();
    rt_exit(0);
}"#,
    );
    assert_eq!(code, 0, "exit code: {out}");
    assert!(out.contains("PASS"), "{out}");
}

// ============================================================
// 6. NETWORKING — hostname
// ============================================================

#[test]
fn smoke_network_hostname() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let init = rt_net_wsa_startup();
    let name = rt_net_gethostname();
    let len = rt_str_len(name);
    if len > 0 {
        rt_print_str("PASS");
    } else {
        rt_print_str("FAIL");
    }
    rt_str_free(name);
    rt_net_wsa_cleanup();
    rt_exit(0);
}"#,
    );
    assert_eq!(code, 0, "exit code: {out}");
    assert!(out.contains("PASS"), "{out}");
}

// ============================================================
// 7. MATCH EXPRESSIONS
// ============================================================

#[test]
fn smoke_match_basic() {
    let (code, out) = build_and_run(
        r#"
fn describe(x: Int) -> Int {
    match x {
        1 => { return 10; },
        2 => { return 20; },
        _ => { return 0; },
    }
}
fn main() {
    rt_print_int(describe(1));
    rt_print_int(describe(2));
    rt_print_int(describe(99));
    rt_exit(0);
}"#,
    );
    assert_eq!(code, 0, "exit code: {out}");
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines, vec!["10", "20", "0"]);
}

// ============================================================
// 8. STRUCTS
// ============================================================

#[test]
fn smoke_struct_basic() {
    let (code, out) = build_and_run(
        r#"
struct Point { x: Int, y: Int }
fn main() {
    let p = Point { x: 10, y: 20 };
    rt_print_int(p.x);
    rt_print_int(p.y);
    rt_exit(0);
}"#,
    );
    assert_eq!(code, 0, "exit code: {out}");
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines, vec!["10", "20"]);
}

// ============================================================
// 9. REALISTIC MULTI-FUNCTION APPLICATION
// ============================================================

#[test]
fn smoke_realistic_application() {
    let (code, out) = build_and_run(
        r#"
fn factorial(n: Int) -> Int {
    if n <= 1 { return 1; }
    return n * factorial(n - 1);
}
fn is_prime(n: Int) -> Bool {
    if n <= 1 { return false; }
    if n <= 3 { return true; }
    if n - (n / 2) * 2 == 0 { return false; }
    let mut i = 3;
    while i * i <= n {
        if n - (n / i) * i == 0 { return false; }
        i = i + 2;
    }
    return true;
}
fn fibonacci(n: Int) -> Int {
    if n <= 0 { return 0; }
    if n == 1 { return 1; }
    let mut a = 0;
    let mut b = 1;
    let mut i = 2;
    while i <= n {
        let temp = a + b;
        a = b;
        b = temp;
        i = i + 1;
    }
    return b;
}
fn main() {
    rt_print_int(factorial(10));
    rt_print_int(fibonacci(10));
    let mut count = 0;
    let mut i = 2;
    while i <= 50 {
        if is_prime(i) { count = count + 1; }
        i = i + 1;
    }
    rt_print_int(count);
    rt_exit(0);
}"#,
    );
    assert_eq!(code, 0, "exit code: {out}");
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines, vec!["3628800", "55", "15"]);
}

// ============================================================
// 10. ENUMS AND PATTERN MATCHING
// ============================================================

#[test]
fn smoke_enum_basic() {
    let (code, out) = build_and_run(
        r#"
enum Color { Red, Green, Blue }
fn main() {
    let c = Color::Green;
    match c {
        Color::Red => { rt_print_str("red"); },
        Color::Green => { rt_print_str("green"); },
        Color::Blue => { rt_print_str("blue"); },
    }
    rt_exit(0);
}"#,
    );
    assert_eq!(code, 0, "exit code: {out}");
    assert!(out.contains("green"), "{out}");
}

// ============================================================
// 11. LOOP AND WHILE
// ============================================================

#[test]
fn smoke_loop_while() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut sum = 0;
    let mut i = 1;
    while i <= 100 {
        sum = sum + i;
        i = i + 1;
    }
    rt_print_int(sum);
    rt_exit(0);
}"#,
    );
    assert_eq!(code, 0, "exit code: {out}");
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines, vec!["5050"]);
}

// ============================================================
// 12. MEMORY — alloc/free round-trip
// ============================================================

#[test]
fn smoke_alloc_free_roundtrip() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let p = rt_alloc(64);
    rt_mem_store(p, 42);
    let v = rt_mem_load(p);
    rt_print_int(v);
    rt_free(p);
    rt_exit(0);
}"#,
    );
    assert_eq!(code, 0, "exit code: {out}");
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines, vec!["42"]);
}
