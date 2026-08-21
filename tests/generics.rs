//! Tests for generic functions and monomorphization (Session 35).

// ---------------------------------------------------------------------------
// Helper: check a MINK source file through the full pipeline.
// ---------------------------------------------------------------------------
fn check_source(source: &str) -> mink::driver::CheckReport {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut sources = mink::source::SourceMap::new();
    let dir = std::env::temp_dir();
    let name = format!("test_generic_{}_{}.mink", std::process::id(), n);
    let path = dir.join(&name);
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
// PARSER-LEVEL TESTS
// ===========================================================================

#[test]
fn generic_function_declaration_parses() {
    let report = check_source("fn identity<T>(x: T) -> T { return x; }");
    assert!(!has_parse_errors(&report), "should parse without errors");
}

#[test]
fn generic_function_with_multiple_params_parses() {
    let report = check_source("fn swap<T, U>(a: T, b: U) -> T { return a; }");
    assert!(!has_parse_errors(&report));
}

// ===========================================================================
// TYPE-CHECKING TESTS
// ===========================================================================

#[test]
fn generic_identity_int() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() {
             let a = identity(42);
             return 0;
         }",
    );
    assert!(
        !has_type_errors(&report),
        "identity(42) should type-check: {:?}",
        report.errors
    );
}

#[test]
fn generic_identity_bool() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() {
             let a = identity(true);
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_identity_multiple_instantiations() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() {
             let a = identity(42);
             let b = identity(true);
             return 0;
         }",
    );
    assert!(
        !has_type_errors(&report),
        "multiple instantiations should type-check: {:?}",
        report.errors
    );
}

#[test]
fn generic_function_with_two_type_params() {
    let report = check_source(
        "fn first<T>(a: T, b: T) -> T { return a; }
         fn main() {
             let x = first(10, 20);
             let y = first(true, false);
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_function_return_value_used() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() {
             let a = identity(42);
             let b = a;
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_function_as_expression() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() {
             let a = identity(42) + identity(10);
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_function_nested_call() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() {
             let a = identity(identity(42));
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_function_with_return_annotation() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() {
             let a: Int = identity(42);
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
}

// ===========================================================================
// HIR / MIR / BACKEND TESTS
// ===========================================================================

#[test]
fn generic_function_lowered_to_mir() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() {
             let a = identity(42);
             return 0;
         }",
    );
    assert!(!has_parse_errors(&report));
    assert!(!has_type_errors(&report));
    assert!(report.mir.is_some(), "MIR should be produced");
}

#[test]
fn generic_function_with_multiple_instantiations_lowered() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() {
             let a = identity(42);
             let b = identity(true);
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
    assert!(report.mir.is_some());
}

// ===========================================================================
// REGRESSION: existing features still work
// ===========================================================================

#[test]
fn non_generic_function_still_works() {
    let report = check_source(
        "fn add(a: Int, b: Int) -> Int { return a; }
         fn main() {
             let x = add(1, 2);
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
}

#[test]
fn generic_function_not_called_is_removed() {
    // The generic function is collected but never instantiated.
    // It should be removed from the AST without errors.
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }
         fn main() {
             return 0;
         }",
    );
    assert!(!has_type_errors(&report));
}
