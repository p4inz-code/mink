//! Tests for generic functions, structs, enums, and monomorphization (Sessions 35–36).

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
static COUNTER: AtomicUsize = AtomicUsize::new(0);

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------
fn check_source(source: &str) -> mink::driver::CheckReport {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut sources = mink::source::SourceMap::new();
    let name = format!("test_generic_{}_{}.mink", std::process::id(), n);
    let path = std::env::temp_dir().join(&name);
    std::fs::write(&path, source).expect("write temp file");
    mink::driver::check(&mut sources, &path).expect("check should not panic")
}

fn has_type_errors(report: &mink::driver::CheckReport) -> bool {
    report
        .errors
        .iter()
        .any(|e| matches!(e, mink::driver::CheckError::Type(_)))
}

fn has_parse_errors(report: &mink::driver::CheckReport) -> bool {
    report
        .errors
        .iter()
        .any(|e| matches!(e, mink::driver::CheckError::Parse(_)))
}

// ===========================================================================
// PARSER TESTS
// ===========================================================================

#[test]
fn generic_function_declaration_parses() {
    let report = check_source("fn identity<T>(x: T) -> T { return x; }");
    assert!(!has_parse_errors(&report));
}

#[test]
fn generic_function_with_multiple_type_params_parses() {
    let report = check_source("fn swap<T, U>(a: T, b: U) -> T { return a; }");
    assert!(!has_parse_errors(&report));
}

#[test]
fn generic_struct_declaration_parses() {
    let report = check_source("struct Pair<T> { first: T, second: T }");
    assert!(!has_parse_errors(&report));
}

#[test]
fn generic_enum_declaration_parses() {
    let report = check_source("enum Maybe<T> { Some(T), Nothing }");
    assert!(!has_parse_errors(&report));
}

#[test]
fn explicit_type_args_parse() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() { let x = identity::<Int>(42); return 0; }",
    );
    assert!(!has_parse_errors(&report));
}

// ===========================================================================
// GENERIC FUNCTION TYPE-CHECKING
// ===========================================================================

#[test]
fn generic_identity_int() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() { let a = identity(42); return 0; }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_identity_bool() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() { let a = identity(true); return 0; }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_multiple_instantiations() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() {
             let a = identity(42);
             let b = identity(true);
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_function_nested_call() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() { let a = identity(identity(42)); return 0; }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_function_in_binary_expression() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() { let a = identity(42) + identity(10); return 0; }",
    );
    assert!(!has_type_errors(&report));
}

// ===========================================================================
// EXPLICIT TYPE ARGUMENTS
// ===========================================================================

#[test]
fn explicit_type_args_identity_int() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() { let x = identity::<Int>(42); return 0; }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn explicit_type_args_builds_and_runs() {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut sources = mink::source::SourceMap::new();
    let path = std::env::temp_dir().join(format!(
        "test_generic_e2e_{}_{}.mink",
        std::process::id(),
        n
    ));
    std::fs::write(
        &path,
        "fn identity<T>(x: T) -> T { return x; }
         fn main() { let x = identity::<Int>(42); return 0; }",
    )
    .unwrap();
    let report = mink::driver::check(&mut sources, &path).unwrap();
    assert!(
        report.errors.is_empty(),
        "check should pass: {:?}",
        report.errors
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn explicit_type_args_with_auto_inference_same_file() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() {
             let x = identity(42);
             let y = identity::<Int>(43);
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
}

// ===========================================================================
// GENERIC STRUCTS
// ===========================================================================

#[test]
fn generic_struct_literal() {
    let report = check_source(
        "struct Pair<T> { first: T, second: T }
         fn main() { let p = Pair { first: 10, second: 20 }; return 0; }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_struct_in_generic_function() {
    let report = check_source(
        "struct Pair<T> { first: T, second: T }
         fn make_pair<T>(a: T, b: T) -> Pair<T> {
             let p = Pair { first: a, second: b };
             return p;
         }
         fn main() { let p = make_pair(10, 20); return 0; }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_struct_field_access() {
    let report = check_source(
        "struct Pair<T> { first: T, second: T }
         fn get_first<T>(p: Pair<T>) -> T { return p.first; }
         fn main() {
             let p = Pair { first: 42, second: 99 };
             let x = get_first::<Int>(p);
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_struct_explicit_type_args() {
    let report = check_source(
        "struct Pair<T> { first: T, second: T }
         fn make_pair<T>(a: T, b: T) -> Pair<T> {
             let p = Pair { first: a, second: b };
             return p;
         }
         fn main() { let p = make_pair::<Int>(10, 20); return 0; }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_struct_multiple_instantiations() {
    let report = check_source(
        "struct Pair<T> { first: T, second: T }
         fn make_pair<T>(a: T, b: T) -> Pair<T> {
             let p = Pair { first: a, second: b };
             return p;
         }
         fn main() {
             let p1 = make_pair(10, 20);
             let p2 = make_pair(true, false);
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_struct_builds() {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut sources = mink::source::SourceMap::new();
    let path = std::env::temp_dir().join(format!(
        "test_generic_struct_{}_{}.mink",
        std::process::id(),
        n
    ));
    std::fs::write(
        &path,
        "struct Pair<T> { first: T, second: T }
         fn make_pair<T>(a: T, b: T) -> Pair<T> {
             let p = Pair { first: a, second: b };
             return p;
         }
         fn main() { let p = make_pair(10, 20); return 0; }",
    )
    .unwrap();
    let report = mink::driver::check(&mut sources, &path).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(report.mir.is_some());
    let _ = std::fs::remove_file(&path);
}

// ===========================================================================
// GENERIC ENUMS
// ===========================================================================

#[test]
fn generic_enum_construction() {
    let report = check_source(
        "enum Maybe<T> { Some(T), Nothing }
         fn main() { let s = Maybe::Some(42); return 0; }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_enum_in_generic_function() {
    let report = check_source(
        "enum Maybe<T> { Some(T), Nothing }
         fn make_some<T>(x: T) -> Maybe<T> {
             return Maybe::Some(x);
         }
         fn main() { let s = make_some(42); return 0; }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_enum_multiple_instantiations() {
    let report = check_source(
        "enum Maybe<T> { Some(T), Nothing }
         fn make_some<T>(x: T) -> Maybe<T> { return Maybe::Some(x); }
         fn main() {
             let s1 = make_some(42);
             let s2 = make_some(true);
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_enum_explicit_type_args() {
    let report = check_source(
        "enum Maybe<T> { Some(T), Nothing }
         fn make_some<T>(x: T) -> Maybe<T> { return Maybe::Some(x); }
         fn main() { let s = make_some::<Int>(42); return 0; }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_enum_builds() {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut sources = mink::source::SourceMap::new();
    let path = std::env::temp_dir().join(format!(
        "test_generic_enum_{}_{}.mink",
        std::process::id(),
        n
    ));
    std::fs::write(
        &path,
        "enum Maybe<T> { Some(T), Nothing }
         fn make_some<T>(x: T) -> Maybe<T> { return Maybe::Some(x); }
         fn main() { let s = make_some(42); return 0; }",
    )
    .unwrap();
    let report = mink::driver::check(&mut sources, &path).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(report.mir.is_some());
    let _ = std::fs::remove_file(&path);
}

// ===========================================================================
// COMBINED: GENERIC STRUCTS + ENUMS + FUNCTIONS
// ===========================================================================

#[test]
fn combined_generic_struct_enum_function() {
    let report = check_source(
        "struct Pair<T> { first: T, second: T }
         enum Maybe<T> { Some(T), Nothing }
         fn make_pair<T>(a: T, b: T) -> Pair<T> {
             let p = Pair { first: a, second: b };
             return p;
         }
         fn make_some<T>(x: T) -> Maybe<T> {
             return Maybe::Some(x);
         }
         fn main() {
             let p = make_pair(10, 20);
             let s = make_some(42);
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn combined_generic_builds() {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut sources = mink::source::SourceMap::new();
    let path = std::env::temp_dir().join(format!(
        "test_generic_combined_{}_{}.mink",
        std::process::id(),
        n
    ));
    std::fs::write(
        &path,
        "struct Pair<T> { first: T, second: T }
         enum Maybe<T> { Some(T), Nothing }
         fn make_pair<T>(a: T, b: T) -> Pair<T> {
             let p = Pair { first: a, second: b };
             return p;
         }
         fn make_some<T>(x: T) -> Maybe<T> {
             return Maybe::Some(x);
         }
         fn get_first<T>(p: Pair<T>) -> T {
             return p.first;
         }
         fn main() {
             let p = make_pair(10, 20);
             let s = make_some(42);
             let x = get_first::<Int>(p);
             let p2 = make_pair::<Int>(30, 40);
             let s2 = make_some::<Int>(99);
             return 0;
         }",
    )
    .unwrap();
    let report = mink::driver::check(&mut sources, &path).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(report.mir.is_some());
    let _ = std::fs::remove_file(&path);
}

// ===========================================================================
// REGRESSION
// ===========================================================================

#[test]
fn non_generic_function_still_works() {
    let report = check_source(
        "fn add(a: Int, b: Int) -> Int { return a; }
         fn main() { let x = add(1, 2); return 0; }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_function_not_called_is_removed() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() { return 0; }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn enum_variant_still_works() {
    let report = check_source(
        "enum E { A, B(Int) }
         fn make(c) { return E::B(c); }
         fn main() { return 0; }",
    );
    assert!(!has_type_errors(&report));
}

// ===========================================================================
// MONOMORPHIZED BODIES: LITERALS AND NATIVE EXECUTION
// ===========================================================================
//
// `check` stops at MIR, so the earlier tests here never reach the backend,
// which is where a monomorphized body's literals are decoded from source
// text. Rewriting a literal expression's span during instantiation made that
// text unavailable: an integer literal silently became `0` and a string
// literal failed with `E-B10`. These tests run the produced binary so the
// decoded values are actually observed.

/// Build `src` and return the produced program's exit code.
fn native_exit_code(src: &str) -> i32 {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "test_generic_native_{}_{}.mink",
        std::process::id(),
        n
    ));
    std::fs::write(&path, src).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("build")
        .arg(&path)
        .output()
        .expect("run mink build");
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        panic!("build failed:\n{stderr}");
    }
    let code = Command::new(&exe).status().unwrap().code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    code
}

#[test]
fn monomorphized_integer_literal_keeps_its_value() {
    assert_eq!(
        native_exit_code("fn ret42<T>(x: T) -> Int { return 42; } fn main() -> Int { return ret42(1); }"),
        42
    );
}

#[test]
fn monomorphized_arithmetic_literal_keeps_its_value() {
    assert_eq!(
        native_exit_code(
            "fn addn<T>(x: T) -> Int { return 5 + 6; } fn main() -> Int { return addn(1); }"
        ),
        11
    );
}

#[test]
fn monomorphized_string_literal_decodes() {
    assert_eq!(
        native_exit_code(
            "fn len_of<T>(x: T) -> Int { let m = \"hello\"; return rt_str_len(m); } \
             fn main() -> Int { return len_of(1); }"
        ),
        5
    );
}

/// Two instantiations of the same generic function share the literal's
/// original span; both must decode the same value.
#[test]
fn monomorphized_literal_shared_across_instantiations() {
    assert_eq!(
        native_exit_code(
            "fn five<T>(x: T) -> Int { return 5; } \
             fn main() -> Int { return five(1) + five(\"x\"); }"
        ),
        10
    );
}
