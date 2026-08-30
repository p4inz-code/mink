//! Release-focused tests for MINK 1.0.0 (Session 47).
//!
//! Tests that validate release readiness: CLI behavior, successful
//! compile/run, compile failure, runtime failure, malformed input,
//! and representative V1 feature coverage.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_rel_test_{n}_{name}"));
    std::fs::write(&path, content).unwrap();
    path
}

/// Build and run a MINK source file, returning the process exit code.
fn native_exit_code(src: &str) -> i32 {
    let path = temp_source("e2e.mink", src);
    let output = mink().arg("build").arg(&path).output().unwrap();
    if !output.status.success() {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("exe"));
        return -1;
    }
    let exe = path.with_extension("exe");
    let run = Command::new(&exe).output().unwrap();
    let code = run.status.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    code
}

/// Assert exit code.
fn assert_exit_code(src: &str, expected: i32) {
    let actual = native_exit_code(src);
    assert_eq!(actual, expected, "expected exit {expected}, got {actual}");
}

/// Assert check fails with given error code.
fn assert_check_err(src: &str, error_code: &str) {
    let path = temp_source("chk.mink", src);
    let output = mink().arg("check").arg(&path).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.code() != Some(0),
        "expected error but succeeded"
    );
    assert!(
        stderr.contains(error_code),
        "expected `{error_code}` in:\n{stderr}"
    );
}

// =========================================================================
// SECTION A: CLI SMOKE TESTS
// =========================================================================

#[test]
fn cli_no_args_shows_help() {
    let output = mink().output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("Usage:"), "expected help output");
}

#[test]
fn cli_help_flag() {
    let output = mink().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("MINK compiler"));
}

#[test]
fn cli_version_flag() {
    let output = mink().arg("--version").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.starts_with("mink "));
}

#[test]
fn cli_version_subcommand() {
    let output = mink().arg("version").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.starts_with("mink "));
}

#[test]
fn cli_unknown_command_exits_1() {
    let output = mink().arg("bogus").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown command"));
}

#[test]
fn cli_build_missing_path() {
    let output = mink().arg("build").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing path"));
}

#[test]
fn cli_check_missing_path() {
    let output = mink().arg("check").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing path"));
}

#[test]
fn cli_nonexistent_file() {
    let output = mink()
        .arg("build")
        .arg("nonexistent_file_xyz.mink")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found") || stderr.contains("failed to read"));
}

#[test]
fn cli_run_compiles_and_executes() {
    let path =
        std::env::temp_dir().join(format!("mink_release_test_{}_run.mink", std::process::id()));
    std::fs::write(&path, "fn main() { return 99; }\n").unwrap();
    let output = mink().arg("run").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(output.status.code(), Some(99));
}

#[test]
fn cli_unknown_commands_rejected() {
    for command in ["test", "fmt"] {
        let output = mink().arg(command).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "for command '{command}'");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("unknown command"),
            "for command '{command}': {stderr}"
        );
    }
}

// =========================================================================
// SECTION B: VERSION / HELP
// =========================================================================

#[test]
fn version_contains_semver() {
    let output = mink().arg("--version").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout.trim().strip_prefix("mink ").unwrap_or("unknown");
    assert!(
        version.starts_with("1."),
        "expected version 1.x.x, got: {version}"
    );
}

#[test]
fn help_lists_commands() {
    let output = mink().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("build"));
    assert!(stdout.contains("check"));
    assert!(stdout.contains("version"));
    assert!(stdout.contains("help"));
}

// =========================================================================
// SECTION C: SUCCESSFUL COMPILE / RUN
// =========================================================================

#[test]
fn release_hello_world() {
    assert_exit_code("fn main() { rt_print_int(42); return 42; }", 42);
}

#[test]
fn release_no_main_rejected() {
    let output = mink()
        .arg("build")
        .arg(temp_source("nomain.mink", "fn helper() { return 1; }"))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "expected failure for no main function"
    );
    assert!(stderr.contains("no `main`"));
}

// =========================================================================
// SECTION D: COMPILE FAILURE
// =========================================================================

#[test]
fn release_type_error_rejected() {
    assert_check_err("fn main() { let x: Int = true; return 0; }", "E-T01");
}

#[test]
fn release_syntax_error_rejected() {
    assert_check_err("fn main() { if { } return 0; }", "E-P");
}

#[test]
fn release_undefined_var_rejected() {
    assert_check_err("fn main() { return x; }", "E-S01");
}

#[test]
fn release_compile_failure_exits_nonzero() {
    let path = temp_source("fail.mink", "fn main() { let x: Int = true; }");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);
    assert_ne!(output.status.code(), Some(0));
}

// =========================================================================
// SECTION E: RUNTIME FAILURE
// =========================================================================

#[test]
fn release_runtime_leak_detected() {
    // Allocating without freeing should be caught by E-R06.
    let src = r#"
fn main() -> Int {
    let s = rt_str_alloc(5);
    return 0;
}"#;
    let path = temp_source("leak.mink", src);
    let build = mink().arg("build").arg(&path).output().unwrap();
    assert!(build.status.success(), "build should succeed");
    let exe = path.with_extension("exe");
    let run = Command::new(&exe).output().unwrap();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    // Runtime should detect the leak.
    assert_ne!(run.status.code(), Some(0));
}

#[test]
fn release_runtime_exit_code_preserved() {
    assert_exit_code("fn main() { return 42; }", 42);
}

// =========================================================================
// SECTION F: MALFORMED INPUT
// =========================================================================

#[test]
fn release_empty_source() {
    let path = temp_source("empty.mink", "");
    let output = mink().arg("build").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);
    // Empty source has no main function — should fail at backend.
    assert!(!output.status.success());
}

#[test]
fn release_only_comments() {
    let path = temp_source("comments.mink", "// just a comment\n// another");
    let output = mink().arg("build").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(!output.status.success());
}

#[test]
fn release_binary_garbage() {
    let path = temp_source("garbage.mink", "\x00\x01\x02\x03\x04");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);
    assert_ne!(output.status.code(), Some(0));
}

// =========================================================================
// SECTION G: RELEASE BINARY EXECUTION
// =========================================================================

#[test]
fn release_binary_version_works() {
    // Verify the release-built binary actually runs and reports version.
    let output = mink().arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1.0.1"));
}

// =========================================================================
// SECTION H: REPRESENTATIVE V1 FEATURE PROGRAM
// =========================================================================

#[test]
fn release_v1_comprehensive() {
    let src = r#"
enum Option<T> {
    Some(T),
    None,
}

enum Result<T, E> {
    Ok(T),
    Err(E),
}

struct Point {
    x: Int,
    y: Int,
}

enum Shape {
    Circle(Int),
    Square(Int),
    Empty,
}

fn add(x: Int, y: Int) -> Int {
    return x + y;
}

fn factorial(n: Int) -> Int {
    if n <= 1 {
        return 1;
    } else {
        return n * factorial(n - 1);
    }
}

fn apply_to_int(f, x: Int) -> Int {
    return f(x);
}

fn main() {
    // Variables
    let mut x: Int = 10;
    let y: Int = 20;
    let z = x + y;

    // Functions
    let sum = add(5, 7);

    // Recursion
    let fact5 = factorial(5);

    // While
    let mut i = 0;
    let mut total = 0;
    while i < 10 {
        total = total + i;
        i = i + 1;
    }

    // For range
    let mut sum2 = 0;
    for j in 1..=5 {
        sum2 = sum2 + j;
    }

    // Structs
    let p = Point { x: 3, y: 4 };

    // Enum match
    let c = Shape::Circle(5);
    match c {
        Shape::Circle(radius) => { rt_print_int(radius); },
        _ => { rt_print_int(0); },
    }

    // Range patterns
    let val = 3;
    match val {
        2..=4 => { rt_print_int(200); },
        _ => { rt_print_int(999); },
    }

    // Or-patterns
    let v2 = 2;
    match v2 {
        2 | 4 | 6 => { rt_print_int(20); },
        _ => { rt_print_int(0); },
    }

    // If expression
    let msg = if z > 25 { 1 } else { 0 };

    // Block expression
    let blk = { let a = 10; let b = 20; a + b };

    // Loop expression
    let mut counter = 0;
    let result = loop {
        counter = counter + 1;
        if counter >= 5 { break counter; }
    };

    // Generics
    let id_val: Int = identity(42);

    // Closures
    let base = 100;
    let adder = |x: Int| base + x;
    let closure_result = adder(42);

    // Closure passed to function
    let inc = |x: Int| x + 1;
    let applied = apply_to_int(inc, 41);

    // Option
    let opt = Option::Some(99);
    match opt {
        Option::Some(v) => { rt_print_int(v); },
        Option::None => { rt_print_int(0); },
    }

    // Result
    let res = Result::Ok(42);
    match res {
        Result::Ok(v) => { rt_print_int(v); },
        Result::Err(e) => { rt_print_int(0); },
    }

    // Arrays
    let arr = [10, 20, 30];
    rt_print_int(arr[0]);

    // For-in array
    let mut arr_sum = 0;
    for item in arr {
        arr_sum = arr_sum + item;
    }

    // Vec
    let mut v = rt_vec_new(10);
    v = rt_vec_push(v, 100);
    let vlen = rt_vec_len(v);
    rt_vec_free(v);

    // Strings
    let s1 = "Hello";
    let s2 = "World";
    let tmp = rt_str_concat(" ", s2);
    let combined = rt_str_concat(s1, tmp);
    rt_str_free(tmp);
    rt_str_free(combined);

    // Struct destructuring
    let Point { x: px, y: py } = p;

    // Tuple destructuring
    let pair = (100, 200);
    let (a, b) = pair;

    // Tuple field access
    rt_print_int(pair.0);

    // References
    let val2 = 42;
    let r1 = &val2;

    // Match guard
    let x2 = 10;
    match x2 {
        n if n > 5 => { rt_print_int(1); },
        _ => { rt_print_int(0); },
    }

    return 0;
}

fn identity<T>(x: T) -> T {
    return x;
}"#;
    assert_exit_code(src, 0);
}

// =========================================================================
// SECTION I: STRING OPERATIONS
// =========================================================================

#[test]
fn release_string_concat_eq() {
    let src = r#"
fn main() -> Int {
    let s = rt_str_concat("hello", " world");
    let mut result = 0;
    if s == "hello world" { result = 1; }
    rt_str_free(s);
    return result;
}"#;
    assert_exit_code(src, 1);
}

#[test]
fn release_string_from_int() {
    let src = r#"
fn main() -> Int {
    let s = rt_str_from_int(42);
    let mut result = 0;
    if s == "42" { result = 1; }
    rt_str_free(s);
    return result;
}"#;
    assert_exit_code(src, 1);
}

#[test]
fn release_string_from_bool() {
    let src = r#"
fn main() -> Int {
    let t = rt_str_from_bool(true);
    let f = rt_str_from_bool(false);
    let mut result = 0;
    if t == "true" { result = result + 1; }
    if f == "false" { result = result + 1; }
    rt_str_free(t);
    rt_str_free(f);
    return result;
}"#;
    assert_exit_code(src, 2);
}

#[test]
fn release_string_eq_operator() {
    let src = r#"
fn main() -> Int {
    if "hello" == "hello" { return 1; }
    return 0;
}"#;
    assert_exit_code(src, 1);
}

#[test]
fn release_string_ne_operator() {
    let src = r#"
fn main() -> Int {
    if "hello" != "world" { return 1; }
    return 0;
}"#;
    assert_exit_code(src, 1);
}

// =========================================================================
// SECTION J: OWNERSHIP
// =========================================================================

#[test]
fn release_ownership_move() {
    let src = r#"
fn consume(s: Str) -> Int {
    let len = rt_str_len(s);
    rt_str_free(s);
    return len;
}
fn main() -> Int {
    let s = rt_str_alloc(3);
    rt_str_set_byte(s, 0, 104);
    rt_str_set_byte(s, 1, 105);
    rt_str_set_byte(s, 2, 33);
    return consume(s);
}"#;
    assert_exit_code(src, 3);
}

#[test]
fn release_ownership_return() {
    let src = r#"
fn make_string() -> Str {
    let s = rt_str_alloc(3);
    rt_str_set_byte(s, 0, 104);
    rt_str_set_byte(s, 1, 105);
    rt_str_set_byte(s, 2, 33);
    return s;
}
fn main() -> Int {
    let s = make_string();
    let len = rt_str_len(s);
    rt_str_free(s);
    return len;
}"#;
    assert_exit_code(src, 3);
}

#[test]
fn release_literal_copies() {
    let src = r#"
fn main() -> Int {
    let a = "hello";
    let b = a;
    let c = a;
    return rt_str_len(a) + rt_str_len(b) + rt_str_len(c);
}"#;
    assert_exit_code(src, 15);
}

// =========================================================================
// SECTION K: OPTION / RESULT
// =========================================================================

#[test]
fn release_option_some() {
    let src = r#"
enum Option<T> { Some(T), None }
fn main() -> Int {
    let x = Option::Some(42);
    match x {
        Option::Some(v) => { return v; },
        Option::None => { return 0; },
    }
}"#;
    assert_exit_code(src, 42);
}

#[test]
fn release_option_none() {
    // Note: unit variant construction (Enum::None) has parser ambiguity;
    // use a generic function to construct it.
    let src = r#"
enum Option<T> { Some(T), None }
fn get_none<T>() -> Option<T> { return Option::None; }
fn main() -> Int {
    let x = get_none::<Int>();
    match x {
        Option::Some(v) => { return v; },
        Option::None => { return 99; },
    }
}"#;
    assert_exit_code(src, 99);
}

#[test]
fn release_result_ok() {
    let src = r#"
enum Result<T, E> { Ok(T), Err(E) }
fn main() -> Int {
    let r = Result::Ok(42);
    match r {
        Result::Ok(v) => { return v; },
        Result::Err(e) => { return 0; },
    }
}"#;
    assert_exit_code(src, 42);
}

#[test]
fn release_result_err() {
    let src = r#"
enum Result<T, E> { Ok(T), Err(E) }
fn main() -> Int {
    let r = Result::Err(99);
    match r {
        Result::Ok(v) => { return 0; },
        Result::Err(e) => { return e; },
    }
}"#;
    assert_exit_code(src, 99);
}

// =========================================================================
// SECTION L: GENERICS
// =========================================================================

#[test]
fn release_generic_identity_int() {
    assert_exit_code(
        "fn identity<T>(x: T) -> T { return x; }\nfn main() -> Int { return identity(42); }",
        42,
    );
}

#[test]
fn release_generic_identity_bool() {
    let src = r#"
fn identity<T>(x: T) -> T { return x; }
fn main() -> Int {
    let b = identity(true);
    if b { return 1; }
    return 0;
}"#;
    assert_exit_code(src, 1);
}

#[test]
fn release_generic_struct() {
    let src = r#"
struct Box<T> { value: T }
fn main() -> Int {
    let b = Box { value: 42 };
    return b.value;
}"#;
    assert_exit_code(src, 42);
}

// =========================================================================
// SECTION M: CLOSURES
// =========================================================================

#[test]
fn release_closure_identity() {
    assert_exit_code("fn main() -> Int { let f = |x: Int| x; return f(42); }", 42);
}

#[test]
fn release_closure_capture() {
    assert_exit_code(
        "fn main() -> Int { let n = 10; let f = |x: Int| x + n; return f(32); }",
        42,
    );
}

#[test]
fn release_closure_passed_to_fn() {
    assert_exit_code(
        "fn apply(f, x) -> Int { return f(x); }\nfn main() -> Int { return apply(|x: Int| x + 1, 41); }",
        42,
    );
}

// =========================================================================
// SECTION N: CONTROL FLOW
// =========================================================================

#[test]
fn release_if_else() {
    assert_exit_code(
        "fn main() -> Int { if 1 == 1 { return 42; } else { return 0; } }",
        42,
    );
}

#[test]
fn release_while_loop() {
    let src = r#"
fn main() -> Int {
    let mut i = 0;
    let mut sum = 0;
    while i < 10 { sum = sum + i; i = i + 1; }
    return sum;
}"#;
    assert_exit_code(src, 45);
}

#[test]
fn release_for_range() {
    let src = r#"
fn main() -> Int {
    let mut sum = 0;
    for i in 0..=9 { sum = sum + i; }
    return sum;
}"#;
    assert_exit_code(src, 45);
}

#[test]
fn release_loop_break() {
    let src = r#"
fn main() -> Int {
    let mut i = 0;
    let r = loop { i = i + 1; if i >= 10 { break i; } };
    return r;
}"#;
    assert_exit_code(src, 10);
}

#[test]
fn release_if_expression() {
    assert_exit_code(
        "fn main() -> Int { let r = if 1 == 1 { 42 } else { 0 }; return r; }",
        42,
    );
}

#[test]
fn release_block_expression() {
    assert_exit_code("fn main() -> Int { let r = { 10 + 32 }; return r; }", 42);
}

// =========================================================================
// SECTION O: ARRAYS / VEC
// =========================================================================

#[test]
fn release_array_index() {
    assert_exit_code(
        "fn main() -> Int { let a = [10, 20, 30]; return a[2]; }",
        30,
    );
}

#[test]
fn release_array_for() {
    let src = r#"
fn main() -> Int {
    let arr = [1, 2, 3, 4, 5];
    let mut sum = 0;
    for x in arr { sum = sum + x; }
    return sum;
}"#;
    assert_exit_code(src, 15);
}

#[test]
fn release_vec_push_get() {
    let src = r#"
fn main() -> Int {
    let mut v = rt_vec_new(5);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 20);
    v = rt_vec_push(v, 30);
    let e = rt_vec_get(v, 2);
    rt_vec_free(v);
    return e;
}"#;
    assert_exit_code(src, 30);
}

#[test]
fn release_vec_for() {
    let src = r#"
fn main() -> Int {
    let mut v = rt_vec_new(5);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 20);
    v = rt_vec_push(v, 30);
    let mut sum = 0;
    for x in v { sum = sum + x; }
    rt_vec_free(v);
    return sum;
}"#;
    assert_exit_code(src, 60);
}

// =========================================================================
// SECTION P: STRUCTS / ENUMS
// =========================================================================

#[test]
fn release_struct_access() {
    assert_exit_code(
        "struct P { x: Int }\nfn main() -> Int { let p = P { x: 42 }; return p.x; }",
        42,
    );
}

#[test]
fn release_struct_destructure() {
    assert_exit_code(
        "struct P { x: Int, y: Int }\nfn main() -> Int { let p = P { x: 10, y: 32 }; let P { x, y } = p; return x + y; }",
        42,
    );
}

#[test]
fn release_enum_match() {
    let src = r#"
enum E { A, B(Int) }
fn main() -> Int {
    let e = E::B(42);
    match e {
        E::B(x) => { return x; },
        E::A => { return 0; },
    }
}"#;
    assert_exit_code(src, 42);
}

#[test]
fn release_discriminant_enum() {
    let src = r#"
enum Color { Red = 1, Green = 2, Blue = 3 }
fn main() -> Int {
    let c = Color::Green;
    match c {
        Color::Red => { return 1; },
        Color::Green => { return 2; },
        Color::Blue => { return 3; },
    }
}"#;
    assert_exit_code(src, 2);
}

// =========================================================================
// SECTION Q: PATTERNS
// =========================================================================

#[test]
fn release_or_patterns() {
    let src = r#"
fn classify(x: Int) -> Int {
    match x {
        1 | 2 | 3 => { return 1; },
        4 | 5 | 6 => { return 2; },
        _ => { return 0; },
    }
}
fn main() -> Int { return classify(2) + classify(5); }"#;
    assert_exit_code(src, 3);
}

#[test]
fn release_range_patterns() {
    let src = r#"
fn classify(x: Int) -> Int {
    match x {
        1..=5 => { return 1; },
        6..=10 => { return 2; },
        _ => { return 0; },
    }
}
fn main() -> Int { return classify(3) + classify(8); }"#;
    assert_exit_code(src, 3);
}

#[test]
fn release_match_guard() {
    let src = r#"
fn main() -> Int {
    let x = 10;
    match x {
        n if n > 5 => { return 1; },
        _ => { return 0; },
    }
}"#;
    assert_exit_code(src, 1);
}

#[test]
fn release_tuple_destructure() {
    assert_exit_code(
        "fn main() -> Int { let (a, b) = (10, 32); return a + b; }",
        42,
    );
}

#[test]
fn release_match_expression() {
    let src = r#"
fn main() -> Int {
    let r = match 42 {
        42 => { 1 },
        _ => { 0 },
    };
    return r;
}"#;
    assert_exit_code(src, 1);
}

// =========================================================================
// SECTION R: EDGE CASES
// =========================================================================

#[test]
fn release_negative_exit_code() {
    assert_exit_code("fn main() { return -1; }", -1);
}

#[test]
fn release_large_integers() {
    assert_exit_code("fn main() -> Int { return 1000000 + 2000000; }", 3000000);
}

#[test]
fn release_nested_structs() {
    let src = r#"
struct Inner { value: Int }
struct Outer { inner: Inner }
fn main() -> Int {
    let o = Outer { inner: Inner { value: 42 } };
    return o.inner.value;
}"#;
    assert_exit_code(src, 42);
}

#[test]
fn release_nested_if() {
    let src = r#"
fn main() -> Int {
    if 1 == 1 {
        if 1 == 1 {
            if 1 == 1 {
                return 42;
            }
        }
    }
    return 0;
}"#;
    assert_exit_code(src, 42);
}
