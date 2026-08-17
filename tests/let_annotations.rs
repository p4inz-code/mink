//! Integration tests for session 26: let-binding type annotations.
//!
//! Adds `let [mut] name: Type = expr;` and `const name: Type = expr;` syntax.
//! When annotations are present, the type checker enforces them; when absent,
//! the existing inference behavior is preserved.

use std::process::Command;

use mink::driver;
use mink::parser;
use mink::source::SourceMap;
use mink::typecheck::TypeResult;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn unique_source(kind: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "mink_letann_{}_{}_{n}.mink",
        kind,
        std::process::id()
    ))
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

// ===========================================================================
// 1. Parser: valid syntax
// ===========================================================================

#[test]
fn parse_let_int_annotation() {
    assert!(parse_ok("fn f() { let x: Int = 1; }"));
}

#[test]
fn parse_let_float_annotation() {
    assert!(parse_ok("fn f() { let x: Float = 1.0; }"));
}

#[test]
fn parse_let_bool_annotation() {
    assert!(parse_ok("fn f() { let x: Bool = true; }"));
}

#[test]
fn parse_let_char_annotation() {
    assert!(parse_ok("fn f() { let x: Char = 'a'; }"));
}

#[test]
fn parse_let_str_annotation() {
    assert!(parse_ok("fn f() { let x: Str = \"hi\"; }"));
}

#[test]
fn parse_let_null_annotation() {
    assert!(parse_ok("fn f() { let x: Null = null; }"));
}

#[test]
fn parse_let_mut_with_annotation() {
    assert!(parse_ok("fn f() { let mut x: Int = 1; }"));
}

#[test]
fn parse_let_struct_type_annotation() {
    assert!(parse_ok(
        "struct P { x: Int }\nfn f() { let p: P = P { x: 1 }; }"
    ));
}

#[test]
fn parse_let_enum_type_annotation() {
    assert!(parse_ok("enum E { A, B }\nfn f() { let e: E = E::A; }"));
}

#[test]
fn parse_let_ptr_type_annotation() {
    assert!(parse_ok("fn f() { let p: Ptr<Int> = rt_alloc(8); }"));
}

#[test]
fn parse_let_array_type_annotation() {
    assert!(parse_ok("fn f() { let a: [Int; 3] = [1, 2, 3]; }"));
}

#[test]
fn parse_let_ref_type_annotation() {
    assert!(parse_ok("fn f() { let x: Int = 1; let r: &Int = &x; }"));
}

#[test]
fn parse_let_mut_ref_type_annotation() {
    assert!(parse_ok(
        "fn f() { let mut x: Int = 1; let r: &mut Int = &mut x; }"
    ));
}

#[test]
fn parse_let_no_annotation_still_works() {
    assert!(parse_ok("fn f() { let x = 1; }"));
    assert!(parse_ok("fn f() { let mut x = 1; }"));
}

#[test]
fn parse_const_annotation() {
    assert!(parse_ok("const X: Int = 42;"));
}

#[test]
fn parse_const_no_annotation_still_works() {
    assert!(parse_ok("const X = 42;"));
}

#[test]
fn parse_mixed_bindings() {
    assert!(parse_ok(
        "fn f() { let x: Int = 1; let y = 2; let mut z: Int = 3; }"
    ));
}

#[test]
fn parse_annotation_span_covers_full_annotation() {
    let src = "fn f() { let x: Int = 1; }";
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).unwrap();
    let output = parser::parse(file);
    assert!(output.parse_errors().is_empty());
    let func = &output.ast().items()[0];
    let mink::ast::ItemKind::Fn(f) = &func.kind else {
        panic!("expected fn item");
    };
    let mink::ast::StmtKind::Let(binding) = &f.body.stmts[0].kind else {
        panic!("expected let binding");
    };
    assert!(binding.ty.is_some());
    let ty = binding.ty.as_ref().unwrap();
    assert_eq!(file.span_text(ty.span), Some("Int"));
}

// ===========================================================================
// 2. Type checker: annotation enforcement (positive)
// ===========================================================================

#[test]
fn typecheck_let_int_annotation_matches() {
    let types = check_src("fn f() { let x: Int = 1; }");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_let_float_annotation_matches() {
    let types = check_src("fn f() { let x: Float = 1.0; }");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_let_bool_annotation_matches() {
    let types = check_src("fn f() { let x: Bool = true; }");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_let_char_annotation_matches() {
    let types = check_src("fn f() { let x: Char = 'a'; }");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_let_str_annotation_matches() {
    let types = check_src("fn f() { let x: Str = \"hi\"; }");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_let_null_annotation_matches() {
    let types = check_src("fn f() { let x: Null = null; }");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_let_mut_annotation_matches() {
    let types = check_src("fn f() { let mut x: Int = 42; }");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_let_struct_annotation_matches() {
    let types = check_src("struct P { x: Int }\nfn f() { let p: P = P { x: 1 }; }");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_let_enum_annotation_matches() {
    let types = check_src("enum E { A, B }\nfn f() { let e: E = E::A; }");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_let_array_annotation_matches() {
    let types = check_src("fn f() { let a: [Int; 3] = [1, 2, 3]; }");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_let_ptr_annotation_matches() {
    let types = check_src("fn f() { let p: Ptr<Int> = rt_alloc(8); }");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_let_ref_annotation_matches() {
    let types = check_src("fn f() { let x: Int = 1; let r: &Int = &x; }");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_const_annotation_matches() {
    let types = check_src("const X: Int = 42;");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_mixed_annotated_and_unannotated() {
    let types = check_src("fn f() { let x: Int = 1; let y = 2; let mut z: Bool = true; }");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_annotated_let_used_in_expression() {
    let types = check_src("fn f() { let x: Int = 5; let y: Int = x + 1; }");
    assert!(!types.has_errors());
}

#[test]
fn typecheck_annotated_let_used_in_function_call() {
    let types = check_src(
        "fn add(a: Int, b: Int) -> Int { return a + b; }\nfn f() { let x: Int = 3; let y: Int = 4; rt_print_int(add(x, y)); }",
    );
    assert!(!types.has_errors());
}

// ===========================================================================
// 3. Type checker: annotation enforcement (negative)
// ===========================================================================

#[test]
fn typecheck_let_int_annotation_mismatch() {
    let report = check_src_allow_errors("fn f() { let x: Int = true; }");
    let types = report.types.unwrap();
    assert!(
        types.has_errors(),
        "Bool assigned to Int-annotated let should fail"
    );
}

#[test]
fn typecheck_let_float_annotation_mismatch() {
    let report = check_src_allow_errors("fn f() { let x: Float = 42; }");
    let types = report.types.unwrap();
    assert!(
        types.has_errors(),
        "Int assigned to Float-annotated let should fail"
    );
}

#[test]
fn typecheck_let_bool_annotation_mismatch() {
    let report = check_src_allow_errors("fn f() { let x: Bool = 1; }");
    let types = report.types.unwrap();
    assert!(
        types.has_errors(),
        "Int assigned to Bool-annotated let should fail"
    );
}

#[test]
fn typecheck_let_char_annotation_mismatch() {
    let report = check_src_allow_errors("fn f() { let x: Char = 42; }");
    let types = report.types.unwrap();
    assert!(
        types.has_errors(),
        "Int assigned to Char-annotated let should fail"
    );
}

#[test]
fn typecheck_let_str_annotation_mismatch() {
    let report = check_src_allow_errors("fn f() { let x: Str = 42; }");
    let types = report.types.unwrap();
    assert!(
        types.has_errors(),
        "Int assigned to Str-annotated let should fail"
    );
}

#[test]
fn typecheck_let_struct_annotation_mismatch() {
    let report = check_src_allow_errors("struct P { x: Int }\nfn f() { let p: P = 42; }");
    let types = report.types.unwrap();
    assert!(
        types.has_errors(),
        "Int assigned to P-annotated let should fail"
    );
}

#[test]
fn typecheck_let_enum_annotation_mismatch() {
    let report = check_src_allow_errors("enum E { A, B }\nfn f() { let e: E = 42; }");
    let types = report.types.unwrap();
    assert!(
        types.has_errors(),
        "Int assigned to E-annotated let should fail"
    );
}

#[test]
fn typecheck_const_annotation_mismatch() {
    let report = check_src_allow_errors("const X: Int = true;");
    let types = report.types.unwrap();
    assert!(
        types.has_errors(),
        "Bool assigned to Int-annotated const should fail"
    );
}

#[test]
fn typecheck_let_annotation_unknown_type() {
    let report = check_src_allow_errors("fn f() { let x: Missing = 1; }");
    let types = report.types.unwrap();
    assert!(
        types.has_errors(),
        "Unknown type name should produce an error"
    );
}

#[test]
fn typecheck_let_annotation_mismatch_with_init_function() {
    let report = check_src_allow_errors(
        "fn get_int() -> Int { return 42; }\nfn f() { let x: Bool = get_int(); }",
    );
    let types = report.types.unwrap();
    assert!(
        types.has_errors(),
        "Int from function assigned to Bool-annotated let should fail"
    );
}

// ===========================================================================
// 4. Backward compatibility
// ===========================================================================

#[test]
fn backward_compat_unannotated_let_still_infers() {
    let types = check_src("fn f() { let x = 1; let y = x + 2; }");
    assert!(!types.has_errors());
}

#[test]
fn backward_compat_unannotated_mutable_let() {
    let types = check_src("fn f() { let mut x = 1; x = x + 1; }");
    assert!(!types.has_errors());
}

#[test]
fn backward_compat_unannotated_const() {
    let types = check_src("const X = 42;");
    assert!(!types.has_errors());
}

#[test]
fn backward_compat_complex_unannotated_program() {
    let types = check_src(
        "struct P { x: Int, y: Int }\nfn f() {\n let p = P { x: 1, y: 2 };\n let q = p;\n rt_print_int(q.x);\n}",
    );
    assert!(!types.has_errors());
}

// ===========================================================================
// 5. Recursive and mutual recursion with annotations
// ===========================================================================

#[test]
fn typecheck_recursive_function_with_let_annotation() {
    let types = check_src(
        "fn factorial(n: Int) -> Int {\n let result: Int = n;\n if n == 0 { return 1; }\n return n * factorial(n - 1);\n}",
    );
    assert!(!types.has_errors());
}

#[test]
fn typecheck_mutual_recursion_with_let_annotation() {
    let types = check_src(
        "fn is_even(n: Int) -> Bool {\n if n == 0 { return true; }\n let remaining: Int = n - 1;\n return is_odd(remaining);\n}\nfn is_odd(n: Int) -> Bool {\n if n == 0 { return false; }\n let remaining: Int = n - 1;\n return is_even(remaining);\n}",
    );
    assert!(!types.has_errors());
}

// ===========================================================================
// 6. Native end-to-end: annotated let bindings compile and execute
// ===========================================================================

#[test]
fn native_let_int_annotation() {
    let out = output_of("fn main() { let x: Int = 42; rt_print_int(x); return 0; }");
    assert_eq!(out.trim(), "42");
}

#[test]
fn native_let_bool_annotation() {
    let out =
        output_of("fn main() {\n let b: Bool = true;\n if b { rt_print_int(1); }\n return 0;\n}");
    assert_eq!(out.trim(), "1");
}

#[test]
fn native_let_float_annotation() {
    let out = output_of("fn main() { let f: Float = 3.5; rt_print_float(f); return 0; }");
    assert_eq!(out.trim(), "3.5");
}

#[test]
fn native_let_char_annotation() {
    let out = output_of("fn main() { let c: Char = 'Z'; rt_print_char(c); return 0; }");
    assert!(out.trim().contains('Z'));
}

#[test]
fn native_let_mut_annotation() {
    let out = output_of(
        "fn main() {\n let mut x: Int = 10;\n x = x + 5;\n rt_print_int(x);\n return 0;\n}",
    );
    assert_eq!(out.trim(), "15");
}

#[test]
fn native_let_annotation_in_loop() {
    let out = output_of(
        "fn main() {\n let mut total: Int = 0;\n for i in 1..=5 {\n  let step: Int = i;\n  total = total + step;\n }\n rt_print_int(total);\n return 0;\n}",
    );
    assert_eq!(out.trim(), "15");
}

#[test]
fn native_let_annotation_struct() {
    let out = output_of(
        "struct P { x: Int, y: Int }\nfn main() {\n let p: P = P { x: 10, y: 20 };\n rt_print_int(p.x + p.y);\n return 0;\n}",
    );
    assert_eq!(out.trim(), "30");
}

#[test]
fn native_let_annotation_enum() {
    let out = output_of(
        "enum Color { R, G, B }\nfn main() {\n let c: Color = Color::G;\n match c {\n  Color::R => { rt_print_int(1); },\n  Color::G => { rt_print_int(2); },\n  Color::B => { rt_print_int(3); },\n }\n return 0;\n}",
    );
    assert_eq!(out.trim(), "2");
}

#[test]
fn native_let_annotation_array() {
    let out = output_of(
        "fn main() {\n let a: [Int; 3] = [10, 20, 30];\n rt_print_int(a[0] + a[1] + a[2]);\n return 0;\n}",
    );
    assert_eq!(out.trim(), "60");
}

#[test]
fn native_let_annotation_chained_function_calls() {
    let out = output_of(
        "fn double(x: Int) -> Int { return x * 2; }\nfn main() {\n let v: Int = double(7);\n rt_print_int(v);\n return 0;\n}",
    );
    assert_eq!(out.trim(), "14");
}

#[test]
fn native_const_annotation() {
    let out = output_of("const LIMIT: Int = 100;\nfn main() { rt_print_int(LIMIT); return 0; }");
    assert_eq!(out.trim(), "100");
}

#[test]
fn native_multiple_annotated_bindings() {
    let out = output_of(
        "fn main() {\n let a: Int = 10;\n let b: Int = 20;\n let c: Int = a + b;\n rt_print_int(c);\n return 0;\n}",
    );
    assert_eq!(out.trim(), "30");
}

#[test]
fn native_let_annotation_with_loop_counter() {
    let out = output_of(
        "fn main() {\n let mut sum: Int = 0;\n for i in 1..=10 {\n  let val: Int = i;\n  sum = sum + val;\n }\n rt_print_int(sum);\n return 0;\n}",
    );
    assert_eq!(out.trim(), "55");
}

// ===========================================================================
// 7. Byte-identical determinism
// ===========================================================================

#[test]
fn native_let_annotation_byte_identical_determinism() {
    let src = "fn main() { let x: Int = 42; rt_print_int(x); return 0; }";
    let exe1 = build(src);
    let exe2 = build(src);
    let bytes1 = std::fs::read(&exe1).unwrap();
    let bytes2 = std::fs::read(&exe2).unwrap();
    assert_eq!(
        bytes1, bytes2,
        "identical source must produce byte-identical executables"
    );
}

// ===========================================================================
// 8. Regression tests
// ===========================================================================

#[test]
fn regression_unannotated_program_still_works() {
    let out = output_of(
        "fn main() {\n let mut total = 0;\n for i in 1..=10 {\n  total = total + i;\n }\n rt_print_int(total);\n return 0;\n}",
    );
    assert_eq!(out.trim(), "55");
}

#[test]
fn regression_struct_program_still_works() {
    let out = output_of(
        "struct P { x: Int, y: Int }\nfn main() {\n let p = P { x: 3, y: 4 };\n rt_print_int(p.x * p.y);\n return 0;\n}",
    );
    assert_eq!(out.trim(), "12");
}

#[test]
fn regression_enum_match_still_works() {
    let out = output_of(
        "enum Dir { N, S, E, W }\nfn main() {\n let d = Dir::E;\n match d {\n  Dir::N => { rt_print_int(1); },\n  Dir::S => { rt_print_int(2); },\n  Dir::E => { rt_print_int(3); },\n  Dir::W => { rt_print_int(4); },\n }\n return 0;\n}",
    );
    assert_eq!(out.trim(), "3");
}

#[test]
fn regression_function_annotations_still_work() {
    let out = output_of(
        "fn add(x: Int, y: Int) -> Int { return x + y; }\nfn main() { rt_print_int(add(3, 4)); return 0; }",
    );
    assert_eq!(out.trim(), "7");
}

#[test]
fn regression_borrowing_still_works() {
    let types =
        check_src("fn f() {\n let mut x: Int = 1;\n let r: &mut Int = &mut x;\n *r = 42;\n}");
    assert!(!types.has_errors());
}

// ===========================================================================
// 9. Edge cases
// ===========================================================================

#[test]
fn edge_case_annotation_with_function_return_value() {
    let types = check_src("fn get_val() -> Int { return 99; }\nfn f() { let x: Int = get_val(); }");
    assert!(!types.has_errors());
}

#[test]
fn edge_case_annotation_with_complex_expression() {
    let types = check_src("fn f() { let x: Int = 1 + 2 * 3 - 4 / 2; }");
    assert!(!types.has_errors());
}

#[test]
fn edge_case_annotation_with_grouped_expression() {
    let types = check_src("fn f() { let x: Int = (1 + 2) * 3; }");
    assert!(!types.has_errors());
}

#[test]
fn edge_case_annotation_with_negated_literal() {
    let types = check_src("fn f() { let x: Int = -5; }");
    assert!(!types.has_errors());
}

#[test]
fn edge_case_annotation_with_comparison() {
    let types = check_src("fn f() { let b: Bool = 1 < 2; }");
    assert!(!types.has_errors());
}

#[test]
fn edge_case_annotation_with_logical_op() {
    let types = check_src("fn f() { let b: Bool = true && false || true; }");
    assert!(!types.has_errors());
}

#[test]
fn edge_case_annotation_on_if_condition_variable() {
    let types = check_src("fn f() {\n let b: Bool = true;\n if b { rt_print_int(1); }\n}");
    assert!(!types.has_errors());
}

#[test]
fn edge_case_annotation_with_ternary_pattern() {
    let types = check_src(
        "fn f() -> Int {\n let x: Int = 1;\n if x > 0 { return x; } else { return 0; }\n}",
    );
    assert!(!types.has_errors());
}

#[test]
fn edge_case_const_annotation_used_in_function() {
    let types = check_src("const LIMIT: Int = 10;\nfn f() -> Int { return LIMIT; }");
    assert!(!types.has_errors());
}

#[test]
fn edge_case_let_annotation_forwards_declaration() {
    let types = check_src("fn f() { let x: Int = helper(); }\nfn helper() -> Int { return 42; }");
    assert!(!types.has_errors());
}

#[test]
fn edge_case_multiple_annotated_bindings_same_type() {
    let types = check_src(
        "fn f() {\n let a: Int = 1;\n let b: Int = 2;\n let c: Int = 3;\n let d: Int = a + b + c;\n}",
    );
    assert!(!types.has_errors());
}

#[test]
fn edge_case_let_annotation_with_string_literal() {
    let types = check_src("fn f() { let s: Str = \"hello\"; }");
    assert!(!types.has_errors());
}

#[test]
fn edge_case_empty_body_with_annotation_does_not_affect_other_items() {
    let types = check_src("fn f() { let x: Int = 1; }\nfn g() -> Int { return 42; }");
    assert!(!types.has_errors());
}
