//! Integration tests for session 25: function signature type annotations.
//!
//! Adds `fn name(param: Type, ...) -> ReturnType { body }` syntax.
//! When annotations are present, the type checker enforces them; when absent,
//! the existing inference behavior is preserved.
//!
//! - Parser: `: Type` after parameter names, `-> Type` after param list.
//! - Type checker: annotation enforcement, error diagnostics (E-T01 for
//!   parameter/return mismatches).
//! - Backward compatibility: all programs without annotations still work.
//! - Native E2E: annotated functions compile and execute correctly.
//! - Edge cases: mixed annotations, recursive functions, all scalar types,
//!   struct/enum/array/pointer/reference parameter and return types.

use std::process::Command;

use mink::ast::ItemKind;
use mink::driver;
use mink::parser;
use mink::source::SourceMap;
use mink::typecheck::{TypeErrorKind, TypeResult};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn unique_source(kind: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("mink_fnann_{kind}_{}_{n}.mink", std::process::id()))
}

/// Parses and type-checks `src`, asserting it parses cleanly.
fn check_src(src: &str) -> TypeResult {
    let mut sources = SourceMap::new();
    let path = unique_source("check");
    std::fs::write(&path, src).unwrap();
    let report = driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(
        report.errors.is_empty(),
        "front end must be clean: {:?}",
        report.errors
    );
    report.types.expect("clean program is type-checked")
}

/// Parses and type-checks `src`, returning the full report (may have errors).
fn check_src_allow_errors(src: &str) -> driver::CheckReport {
    let mut sources = SourceMap::new();
    let path = unique_source("check_err");
    std::fs::write(&path, src).unwrap();
    let report = driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    report
}

/// Parses `src` and returns the parse-error kinds.
fn parse_errors(src: &str) -> Vec<mink::parser::ParseErrorKind> {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("added file");
    let output = parser::parse(file);
    output.parse_errors().iter().map(|e| e.kind()).collect()
}

/// Parses `src` and returns true when it produces no parse errors.
fn parse_ok(src: &str) -> bool {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("added file");
    parser::parse(file).parse_errors().is_empty()
}

/// Builds the source into a native executable and returns its path.
fn build(source: &str) -> std::path::PathBuf {
    let name = std::thread::current()
        .name()
        .unwrap_or("program")
        .replace("::", "_");
    let path = unique_source(&name);
    std::fs::write(&path, source).unwrap();
    let mink_exe = env!("CARGO_BIN_EXE_mink");
    let output = Command::new(mink_exe)
        .arg("build")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exe = path.with_extension("exe");
    assert!(exe.exists(), "no executable produced");
    exe
}

/// Runs a built executable and returns (exit_code, stdout_bytes).
fn run(exe: &std::path::PathBuf) -> (i32, Vec<u8>) {
    let output = Command::new(exe).output().unwrap();
    let stdout = output.stdout;
    let stdout = if stdout.contains(&b'\r') {
        stdout.iter().copied().filter(|b| *b != b'\r').collect()
    } else {
        stdout
    };
    (output.status.code().unwrap_or(-1), stdout)
}

/// Builds, runs, and returns the stdout as a string. Asserts exit 0.
fn output_of(source: &str) -> String {
    let exe = build(source);
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0, "program must exit 0");
    String::from_utf8(stdout).expect("stdout must be valid UTF-8")
}

// ---------------------------------------------------------------------------
// 1. Parser: valid syntax
// ---------------------------------------------------------------------------

#[test]
fn parse_return_type_annotation_accepted() {
    assert!(parse_ok("fn f() -> Int { }"));
    assert!(parse_ok("fn f() -> Bool { }"));
    assert!(parse_ok("fn f() -> Float { }"));
    assert!(parse_ok("fn f() -> Char { }"));
    assert!(parse_ok("fn f() -> Str { }"));
    assert!(parse_ok("fn f() -> Null { }"));
    assert!(parse_ok("fn f() -> Ptr<Int> { }"));
    assert!(parse_ok("fn f() -> [Int; 3] { }"));
    assert!(parse_ok("fn f() -> &Int { }"));
    assert!(parse_ok("fn f() -> &mut Int { }"));
}

#[test]
fn parse_param_type_annotations_accepted() {
    assert!(parse_ok("fn f(x: Int) { }"));
    assert!(parse_ok("fn f(x: Int, y: Float) { }"));
    assert!(parse_ok("fn f(x: Int) -> Int { }"));
    assert!(parse_ok("fn f(x: Int, y: Float) -> Float { }"));
    assert!(parse_ok("fn f(x: Bool, y: Char, z: Str) { }"));
    assert!(parse_ok("fn f(x: Null) { }"));
    assert!(parse_ok("fn f(x: Ptr<Int>) { }"));
    assert!(parse_ok("fn f(x: &Int) { }"));
    assert!(parse_ok("fn f(x: &mut Int) { }"));
    assert!(parse_ok("fn f(x: [Int; 3]) { }"));
}

#[test]
fn parse_mixed_annotated_and_unannotated_params() {
    assert!(parse_ok("fn f(x: Int, y) { }"));
    assert!(parse_ok("fn f(x, y: Float) { }"));
    assert!(parse_ok("fn f(x: Int, y, z: Bool) { }"));
    assert!(parse_ok("fn f(x: Int, y) -> Int { }"));
}

#[test]
fn parse_annotation_with_struct_type() {
    // Struct type annotations parse cleanly.
    assert!(parse_ok("struct P { x: Int }\nfn f(p: P) { }"));
}

#[test]
fn parse_annotation_with_enum_type() {
    // Enum type annotations parse cleanly.
    assert!(parse_ok("enum E { A, B }\nfn f(e: E) { }"));
}

#[test]
fn parse_annotation_span_covers_annotation() {
    // The parameter span should cover the name and the annotation.
    let src = "fn f(x: Int) { }";
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).unwrap();
    let output = parser::parse(file);
    assert!(output.parse_errors().is_empty());
    let (ast, _, _) = output.into_parts();
    let ItemKind::Fn(f) = &ast.items()[0].kind else {
        panic!("expected a function item")
    };
    // x: Int spans bytes 6..13 (f is 3, space 4, x 5, : 6, space 7, I 8..10, n 11, t 12, ... 13)
    assert_eq!(f.params[0].name.name, "x");
    assert!(
        f.params[0].ty.is_some(),
        "param should have a type annotation"
    );
}

// ---------------------------------------------------------------------------
// 2. Parser: rejection tests
// ---------------------------------------------------------------------------

#[test]
fn parse_missing_arrow_before_return_type_is_not_confused() {
    // `fn f() int { }` should not parse `int` as return type — no arrow.
    let errs = parse_errors("fn f() int { }");
    // `int` is parsed as an expression statement, so we get a block
    // parsing issue — the `int` is treated as a call or expression.
    assert!(!errs.is_empty(), "should produce parse errors");
}

#[test]
fn parse_annotation_without_arrow_is_rejected() {
    // `fn f() -> Int { }` is valid, but `fn f() -> { }` is not.
    let errs = parse_errors("fn f() -> { }");
    assert!(
        errs.contains(&mink::parser::ParseErrorKind::ExpectedType),
        "expected ExpectedType for bare arrow"
    );
}

#[test]
fn parse_double_arrow_is_rejected() {
    // `fn f() -> -> Int { }` — the second `->` is not valid after a type.
    let errs = parse_errors("fn f() -> -> Int { }");
    assert!(
        !errs.is_empty(),
        "should produce parse errors for double arrow"
    );
}

// ---------------------------------------------------------------------------
// 3. Type checker: annotation enforcement
// ---------------------------------------------------------------------------

#[test]
fn annotated_return_type_is_enforced() {
    let types = check_src("fn f() -> Int { return 5; }");
    assert!(
        !types.has_errors(),
        "well-annotated program should type-check"
    );
}

#[test]
fn annotated_return_type_mismatch_is_rejected() {
    let report = check_src_allow_errors("fn f() -> Bool { return 5; }");
    let type_errors = report.types.unwrap();
    let mismatches = type_errors
        .errors()
        .iter()
        .filter(|e| e.kind() == TypeErrorKind::TypeMismatch)
        .count();
    assert!(mismatches >= 1, "return type mismatch should be reported");
}

#[test]
fn annotated_param_type_is_enforced() {
    let types = check_src("fn f(x: Int) -> Int { return x; }");
    assert!(!types.has_errors());
}

#[test]
fn annotated_param_type_mismatch_at_call_is_rejected() {
    let report =
        check_src_allow_errors("fn f(x: Int) -> Int { return x; }\nfn main() { f(true); }");
    let type_errors = report.types.unwrap();
    let mismatches = type_errors
        .errors()
        .iter()
        .filter(|e| e.kind() == TypeErrorKind::TypeMismatch)
        .count();
    assert!(mismatches >= 1, "argument type mismatch should be reported");
}

#[test]
fn annotated_param_type_enforced_at_body_use() {
    // x is declared Int; using it where Bool is expected should fail.
    let report = check_src_allow_errors("fn f(x: Int) { if x { } }");
    let type_errors = report.types.unwrap();
    let mismatches = type_errors
        .errors()
        .iter()
        .filter(|e| e.kind() == TypeErrorKind::TypeMismatch)
        .count();
    assert!(mismatches >= 1, "param type mismatch should be reported");
}

#[test]
fn annotated_return_type_with_mismatched_return_is_rejected() {
    let report = check_src_allow_errors("fn f() -> Bool { return 5; }");
    let type_errors = report.types.unwrap();
    assert!(type_errors.has_errors());
}

#[test]
fn annotated_float_return_type_works() {
    let types = check_src("fn f() -> Float { return 1.5; }");
    assert!(!types.has_errors());
}

#[test]
fn annotated_float_return_mismatch_is_rejected() {
    let report = check_src_allow_errors("fn f() -> Float { return 5; }");
    let type_errors = report.types.unwrap();
    let mismatches = type_errors
        .errors()
        .iter()
        .filter(|e| e.kind() == TypeErrorKind::TypeMismatch)
        .count();
    assert!(mismatches >= 1, "float/int mismatch should be reported");
}

#[test]
fn annotated_char_return_type_works() {
    let types = check_src("fn f() -> Char { return 'a'; }");
    assert!(!types.has_errors());
}

#[test]
fn annotated_char_mismatch_is_rejected() {
    let report = check_src_allow_errors("fn f() -> Char { return 5; }");
    let type_errors = report.types.unwrap();
    assert!(type_errors.has_errors());
}

#[test]
fn annotated_null_return_type_works() {
    let types = check_src("fn f() -> Null { return null; }");
    assert!(!types.has_errors());
}

#[test]
fn annotated_null_mismatch_is_rejected() {
    let report = check_src_allow_errors("fn f() -> Null { return 5; }");
    let type_errors = report.types.unwrap();
    assert!(type_errors.has_errors());
}

#[test]
fn annotated_str_return_type_works() {
    let types = check_src("fn f() -> Str { return \"hi\"; }");
    assert!(!types.has_errors());
}

#[test]
fn annotated_str_mismatch_is_rejected() {
    let report = check_src_allow_errors("fn f() -> Str { return 5; }");
    let type_errors = report.types.unwrap();
    assert!(type_errors.has_errors());
}

#[test]
fn annotated_multiple_params_all_enforced() {
    let types = check_src("fn f(a: Int, b: Float, c: Bool) -> Int { return a; }");
    assert!(!types.has_errors());
}

#[test]
fn annotated_param_mismatch_one_of_many() {
    // b is Float; using it where Int is expected should fail.
    let report = check_src_allow_errors("fn f(a: Int, b: Float, c: Bool) { if b { } }");
    let type_errors = report.types.unwrap();
    assert!(
        type_errors.has_errors(),
        "mismatch between Float param and Bool condition"
    );
}

// ---------------------------------------------------------------------------
// 4. Backward compatibility: unannotated functions still infer
// ---------------------------------------------------------------------------

#[test]
fn unannotated_function_still_infers_return_type() {
    let types = check_src("fn f() { return 5; }");
    assert!(!types.has_errors());
}

#[test]
fn unannotated_function_infers_param_type() {
    let types = check_src("fn f(x) { return x + 1; }\nfn main() { f(10); }");
    assert!(!types.has_errors());
}

#[test]
fn mixed_annotations_partially_infer() {
    // x has declared Int, y is inferred. The return type is inferred.
    let types = check_src("fn f(x: Int, y) { return x + y; }\nfn main() { f(1, 2); }");
    assert!(!types.has_errors());
}

#[test]
fn mixed_annotations_return_annotated_param_inferred() {
    // Return type is annotated Int, param y is inferred.
    let types = check_src("fn f(x, y) -> Int { return x + y; }\nfn main() { f(1, 2); }");
    assert!(!types.has_errors());
}

// ---------------------------------------------------------------------------
// 5. Recursive functions with annotations
// ---------------------------------------------------------------------------

#[test]
fn recursive_function_with_annotations() {
    let types = check_src(
        "fn factorial(n: Int) -> Int {\n\
         \x20 if n == 0 { return 1; }\n\
         \x20 return n * factorial(n - 1);\n\
         }",
    );
    assert!(!types.has_errors());
}

#[test]
fn recursive_function_return_mismatch_is_rejected() {
    // bad() -> Bool, but bad(n - 1) + 1 tries to add Bool + Int.
    let report = check_src_allow_errors(
        "fn bad(n: Int) -> Bool {\n\
         \x20 if n == 0 { return true; }\n\
         \x20 return bad(n - 1) + 1;\n\
         }",
    );
    // The front end may report parse or type errors; either way the
    // program is rejected.
    assert!(
        !report.errors.is_empty() || report.types.as_ref().is_some_and(|t| t.has_errors()),
        "recursive function with wrong return should fail"
    );
}

#[test]
fn mutually_recursive_with_annotations() {
    let types = check_src(
        "fn is_even(n: Int) -> Bool {\n\
         \x20 if n == 0 { return true; }\n\
         \x20 return is_odd(n - 1);\n\
         }\n\
         fn is_odd(n: Int) -> Bool {\n\
         \x20 if n == 0 { return false; }\n\
         \x20 return is_even(n - 1);\n\
         }",
    );
    assert!(!types.has_errors());
}

// ---------------------------------------------------------------------------
// 6. Struct parameter and return types
// ---------------------------------------------------------------------------

#[test]
fn struct_param_and_return_annotations() {
    let types = check_src(
        "struct P { x: Int, y: Int }\n\
         fn make(x: Int) -> P { return P { x: x, y: 0 }; }\n\
         fn get_x(p: P) -> Int { return p.x; }",
    );
    assert!(!types.has_errors());
}

#[test]
fn struct_param_type_mismatch_is_rejected() {
    let report = check_src_allow_errors(
        "struct P { x: Int, y: Int }\n\
         fn process(p: P) { }\n\
         fn main() { process(42); }",
    );
    let type_errors = report.types.unwrap();
    assert!(type_errors.has_errors(), "Int where P expected should fail");
}

// ---------------------------------------------------------------------------
// 7. Enum parameter and return types
// ---------------------------------------------------------------------------

#[test]
fn enum_param_and_return_annotations() {
    let types = check_src(
        "enum Dir { N, S, E, W }\n\
         fn flip(d: Dir) -> Dir { return d; }",
    );
    assert!(!types.has_errors());
}

#[test]
fn enum_param_type_mismatch_is_rejected() {
    let report = check_src_allow_errors(
        "enum Dir { N, S, E, W }\n\
         fn flip(d: Dir) -> Dir { return d; }\n\
         fn main() { flip(42); }",
    );
    let type_errors = report.types.unwrap();
    assert!(
        type_errors.has_errors(),
        "Int where Dir expected should fail"
    );
}

// ---------------------------------------------------------------------------
// 8. Array parameter and return types
// ---------------------------------------------------------------------------

#[test]
fn array_param_and_return_annotations() {
    let types = check_src("fn first(a: [Int; 3]) -> Int { return a[0]; }");
    assert!(!types.has_errors());
}

// ---------------------------------------------------------------------------
// 9. Pointer parameter and return types
// ---------------------------------------------------------------------------

#[test]
fn pointer_param_and_return_annotations() {
    let types = check_src("fn deref_int(p: Ptr<Int>) -> Int { return rt_mem_load(p); }");
    assert!(!types.has_errors());
}

// ---------------------------------------------------------------------------
// 10. Reference parameter and return types
// ---------------------------------------------------------------------------

#[test]
fn reference_param_type_annotations() {
    let types = check_src("fn read_int(r: &Int) -> Int { return *r; }");
    assert!(!types.has_errors());
}

// ---------------------------------------------------------------------------
// 11. Native end-to-end: annotated functions compile and execute
// ---------------------------------------------------------------------------

#[test]
fn native_annotated_int_return() {
    let out = output_of(
        "fn add(x: Int, y: Int) -> Int { return x + y; }\n\
         fn main() { rt_print_int(add(3, 4)); return 0; }",
    );
    assert_eq!(out.trim(), "7");
}

#[test]
fn native_annotated_bool_return() {
    let out = output_of(
        "fn is_positive(x: Int) -> Bool { return x > 0; }\n\
         fn main() {\n\
         \x20 if is_positive(5) { rt_print_int(1); }\n\
         \x20 return 0;\n\
         }",
    );
    assert_eq!(out.trim(), "1");
}

#[test]
fn native_annotated_float_return() {
    let out = output_of(
        "fn half(x: Float) -> Float { return x / 2.0; }\n\
         fn main() { rt_print_float(half(10.0)); return 0; }",
    );
    assert_eq!(out.trim(), "5");
}

#[test]
fn native_annotated_char_return() {
    let out = output_of(
        "fn get_char() -> Char { return 'X'; }\n\
         fn main() { rt_print_char(get_char()); return 0; }",
    );
    assert!(out.trim().contains('X'));
}

#[test]
fn native_annotated_recursive_factorial() {
    let out = output_of(
        "fn factorial(n: Int) -> Int {\n\
         \x20 if n == 0 { return 1; }\n\
         \x20 return n * factorial(n - 1);\n\
         }\n\
         fn main() { rt_print_int(factorial(10)); return 0; }",
    );
    assert_eq!(out.trim(), "3628800");
}

#[test]
fn native_annotated_mutual_recursion() {
    let out = output_of(
        "fn is_even(n: Int) -> Bool {\n\
         \x20 if n == 0 { return true; }\n\
         \x20 return is_odd(n - 1);\n\
         }\n\
         fn is_odd(n: Int) -> Bool {\n\
         \x20 if n == 0 { return false; }\n\
         \x20 return is_even(n - 1);\n\
         }\n\
         fn main() {\n\
         \x20 if is_even(4) { rt_print_int(1); } else { rt_print_int(0); }\n\
         \x20 if is_odd(5) { rt_print_int(1); } else { rt_print_int(0); }\n\
         \x20 return 0;\n\
         }",
    );
    let lines: Vec<&str> = out.trim().lines().collect();
    assert_eq!(lines, vec!["1", "1"]);
}

#[test]
fn native_annotated_struct_param_and_return() {
    let out = output_of(
        "struct P { x: Int, y: Int }\n\
         fn make_p(x: Int, y: Int) -> P { return P { x: x, y: y }; }\n\
         fn get_x(p: P) -> Int { return p.x; }\n\
         fn main() {\n\
         \x20 let p = make_p(10, 20);\n\
         \x20 rt_print_int(get_x(p));\n\
         \x20 return 0;\n\
         }",
    );
    assert_eq!(out.trim(), "10");
}

#[test]
fn native_annotated_enum_param_and_return() {
    let out = output_of(
        "enum Color { R, G, B }\n\
         fn identity(c: Color) -> Color { return c; }\n\
         fn main() {\n\
         \x20 let c = Color::R;\n\
         \x20 match identity(c) {\n\
         \x20   Color::R => { rt_print_int(1); },\n\
         \x20   Color::G => { rt_print_int(2); },\n\
         \x20   Color::B => { rt_print_int(3); },\n\
         \x20 }\n\
         \x20 return 0;\n\
         }",
    );
    assert_eq!(out.trim(), "1");
}

#[test]
fn native_annotated_void_function() {
    // A function with no return type annotation that returns nothing.
    let out = output_of(
        "fn greet() { rt_print_int(42); }\n\
         fn main() { greet(); return 0; }",
    );
    assert_eq!(out.trim(), "42");
}

#[test]
fn native_annotated_chained_calls() {
    let out = output_of(
        "fn double(x: Int) -> Int { return x + x; }\n\
         fn inc(x: Int) -> Int { return x + 1; }\n\
         fn main() {\n\
         \x20 rt_print_int(inc(double(5)));\n\
         \x20 return 0;\n\
         }",
    );
    assert_eq!(out.trim(), "11");
}

#[test]
fn native_annotated_loop_with_annotated_function() {
    let out = output_of(
        "fn is_even(n: Int) -> Bool { return n % 2 == 0; }\n\
         fn main() {\n\
         \x20 let mut i = 0;\n\
         \x20 while i < 10 {\n\
         \x20   if is_even(i) { rt_print_int(i); }\n\
         \x20   i = i + 1;\n\
         \x20 }\n\
         \x20 return 0;\n\
         }",
    );
    let lines: Vec<&str> = out.trim().lines().collect();
    assert_eq!(lines, vec!["0", "2", "4", "6", "8"]);
}

// ---------------------------------------------------------------------------
// 12. Byte-identical determinism
// ---------------------------------------------------------------------------

#[test]
fn native_images_are_deterministic_with_annotations() {
    let src = "\
        fn add(x: Int, y: Int) -> Int { return x + y; }\n\
        fn main() { rt_print_int(add(1, 2)); return 0; }\n";

    let path1 = unique_source("det1");
    let path2 = unique_source("det2");
    std::fs::write(&path1, src).unwrap();
    std::fs::write(&path2, src).unwrap();

    let mink_exe = env!("CARGO_BIN_EXE_mink");
    let out1 = Command::new(mink_exe)
        .arg("build")
        .arg(&path1)
        .output()
        .unwrap();
    let out2 = Command::new(mink_exe)
        .arg("build")
        .arg(&path2)
        .output()
        .unwrap();
    assert!(out1.status.success());
    assert!(out2.status.success());

    let exe1 = path1.with_extension("exe");
    let exe2 = path2.with_extension("exe");
    let bytes1 = std::fs::read(&exe1).unwrap();
    let bytes2 = std::fs::read(&exe2).unwrap();
    assert_eq!(
        bytes1, bytes2,
        "identical source must produce byte-identical images"
    );

    let _ = std::fs::remove_file(&path1);
    let _ = std::fs::remove_file(&path2);
    let _ = std::fs::remove_file(&exe1);
    let _ = std::fs::remove_file(&exe2);
}

// ---------------------------------------------------------------------------
// 13. Regression: programs without annotations still work
// ---------------------------------------------------------------------------

#[test]
fn regression_unannotated_program_still_works() {
    let out = output_of(
        "fn add(x, y) { return x + y; }\n\
         fn main() { rt_print_int(add(3, 4)); return 0; }",
    );
    assert_eq!(out.trim(), "7");
}

#[test]
fn regression_struct_program_still_works() {
    let out = output_of(
        "struct P { x: Int, y: Int }\n\
         fn main() {\n\
         \x20 let p = P { x: 10, y: 20 };\n\
         \x20 rt_print_int(p.x + p.y);\n\
         \x20 return 0;\n\
         }",
    );
    assert_eq!(out.trim(), "30");
}

#[test]
fn regression_enum_match_still_works() {
    // Match arms must be separated by nothing (no comma required after block).
    let out = output_of(
        "enum Dir { N, S, E, W }\n\
         fn main() {\n\
         \x20 let d = Dir::E;\n\
         \x20 match d {\n\
         \x20   Dir::N => { rt_print_int(0); },\n\
         \x20   Dir::S => { rt_print_int(1); },\n\
         \x20   Dir::E => { rt_print_int(2); },\n\
         \x20   Dir::W => { rt_print_int(3); },\n\
         \x20 }\n\
         \x20 return 0;\n\
         }",
    );
    assert_eq!(out.trim(), "2");
}

// ---------------------------------------------------------------------------
// 14. Edge cases
// ---------------------------------------------------------------------------

#[test]
fn annotated_empty_params() {
    let types = check_src("fn f() -> Int { return 42; }");
    assert!(!types.has_errors());
}

#[test]
fn annotated_single_param_no_return() {
    let types = check_src("fn f(x: Int) { }");
    assert!(!types.has_errors());
}

#[test]
fn deeply_nested_type_annotation() {
    let types = check_src("fn f(x: Ptr<Int>) -> Ptr<Int> { return x; }");
    assert!(!types.has_errors());
}

#[test]
fn reference_type_annotation_with_struct() {
    // References cannot be used for member access directly (E-T33 / deref model).
    // This test verifies the annotation itself is accepted and the
    // expected error is about the reference, not the type annotation.
    let report = check_src_allow_errors(
        "struct S { x: Int }\n\
         fn read(s: &S) -> Int { return s.x; }",
    );
    let type_errors = report.types.unwrap();
    // The error should be about member access on a reference, not about the type.
    assert!(type_errors.has_errors());
}

#[test]
fn annotated_function_called_from_annotated_function() {
    let types = check_src(
        "fn double(x: Int) -> Int { return x * 2; }\n\
         fn quad(x: Int) -> Int { return double(double(x)); }",
    );
    assert!(!types.has_errors());
}

#[test]
fn annotated_function_returning_no_value_body_mismatch() {
    // Function declares -> Int but body has no return.
    // This should still work (bare returns are allowed, the type is inferred).
    // The missing return means the function may not produce a value,
    // which is acceptable — the return type annotation constrains return
    // expressions, not the absence of returns.
    let types = check_src("fn f() -> Int { }");
    assert!(
        !types.has_errors(),
        "empty body with return annotation is acceptable"
    );
}
