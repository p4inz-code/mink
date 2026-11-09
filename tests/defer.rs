//! Deterministic `defer { ... }` cleanup.
//!
//! Deferred blocks are lexical-scope registrations: they run in reverse
//! registration order when the scope exits normally, returns, breaks, or
//! continues. The tests cover parsing, semantic validation, type checking,
//! ownership, and actual native Windows execution of every exit path.

use mink::parser;
use mink::source::SourceMap;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn parse_ok(src: &str) -> bool {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("source file present");
    parser::parse(file).is_valid()
}

fn parsed(src: &str) -> (SourceMap, mink::source::SourceId) {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("source file present");
    let result = parser::parse(file);
    assert!(
        result.is_valid(),
        "test source must parse: {:?}",
        result.parse_errors()
    );
    (map, id)
}

fn semantic_codes(src: &str) -> Vec<String> {
    let (map, id) = parsed(src);
    let file = map.get(id).expect("source file present");
    let result = parser::parse(file);
    mink::semantics::analyze(result.ast())
        .errors()
        .iter()
        .map(|error| error.code().to_string())
        .collect()
}

fn type_codes(src: &str) -> Vec<String> {
    let (map, id) = parsed(src);
    let file = map.get(id).expect("source file present");
    let result = parser::parse(file);
    let semantic = mink::semantics::analyze(result.ast());
    assert!(
        semantic.errors().is_empty(),
        "test source must be semantically clean: {:?}",
        semantic.errors()
    );
    mink::typecheck::check(result.ast(), &semantic, &map)
        .errors()
        .iter()
        .map(|error| error.code().to_string())
        .collect()
}

fn ownership_codes(src: &str) -> Vec<String> {
    let (map, id) = parsed(src);
    let file = map.get(id).expect("source file present");
    let result = parser::parse(file);
    let semantic = mink::semantics::analyze(result.ast());
    assert!(
        semantic.errors().is_empty(),
        "test source must be semantically clean: {:?}",
        semantic.errors()
    );
    let types = mink::typecheck::check(result.ast(), &semantic, &map);
    assert!(
        types.errors().is_empty(),
        "test source must type-check cleanly: {:?}",
        types.errors()
    );
    mink::ownership::check(result.ast(), &semantic, &types)
        .errors()
        .iter()
        .map(|error| error.code().to_string())
        .collect()
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn native(src: &str) -> (String, i32) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "mink_defer_test_{}_{}.mink",
        std::process::id(),
        n
    ));
    std::fs::write(&path, src).unwrap();
    let build = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        build.status.code() == Some(0),
        "build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let exe = path.with_extension("exe");
    let run = Command::new(&exe).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    let code = run.status.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(exe);
    (stdout, code)
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

#[test]
fn parser_accepts_empty_defer_block() {
    assert!(parse_ok("fn main() { defer { } return 0; }"));
}

#[test]
fn parser_accepts_nested_statements_and_defer() {
    assert!(parse_ok(
        "fn main() { defer { let x = 1; if x == 1 { defer { rt_print_str(\"x\"); } } } }"
    ));
}

#[test]
fn parser_rejects_defer_without_a_block() {
    assert!(!parse_ok("fn main() { defer; }"));
}

#[test]
fn parser_rejects_unterminated_statement_in_defer() {
    assert!(!parse_ok("fn main() { defer { rt_print_str(\"x\") } }"));
}

// ---------------------------------------------------------------------------
// Semantics, ownership and types
// ---------------------------------------------------------------------------

#[test]
fn semantics_accepts_defer_in_function_and_loop() {
    assert!(semantic_codes(
        "fn main() { defer { } while true { defer { } break; } }"
    )
    .is_empty());
}

#[test]
fn semantics_rejects_return_from_defer() {
    let codes = semantic_codes("fn main() { defer { return; } }");
    assert_eq!(codes, vec!["E-S17"]);
}

#[test]
fn semantics_rejects_break_leaving_defer() {
    let codes = semantic_codes("fn main() { while true { defer { break; } } }");
    assert_eq!(codes, vec!["E-S17"]);
}

#[test]
fn semantics_rejects_continue_leaving_defer() {
    let codes = semantic_codes("fn main() { while true { defer { continue; } } }");
    assert_eq!(codes, vec!["E-S17"]);
}

#[test]
fn semantics_allows_inner_loop_control_flow_in_defer() {
    assert!(semantic_codes(
        "fn main() { defer { let mut i = 0; while i < 2 { i += 1; if i == 1 { continue; } break; } } }"
    )
    .is_empty());
}

#[test]
fn semantics_rejects_defer_with_trailing_expression_block() {
    let codes = semantic_codes("fn main() { let x = { defer { } 1 }; }");
    assert_eq!(codes, vec!["E-S17"]);
}

#[test]
fn ownership_rejects_owned_local_capture() {
    let codes = ownership_codes(
        "fn main() { let s = rt_str_alloc(2); defer { rt_print_str(s); } rt_str_free(s); }",
    );
    assert_eq!(codes, vec!["E-S18"]);
}

#[test]
fn ownership_allows_copy_and_immutable_captures() {
    assert!(ownership_codes(
        "fn main() { let s = \"literal\"; let mut n = 1; defer { rt_print_str(s); n += 1; } }"
    )
    .is_empty());
}

#[test]
fn typecheck_reports_error_inside_defer() {
    let codes = type_codes("fn main() { defer { let x: Bool = 1; } }");
    assert_eq!(codes, vec!["E-T01"]);
}

// ---------------------------------------------------------------------------
// Native execution
// ---------------------------------------------------------------------------

#[test]
fn native_runs_lifo_on_normal_exit() {
    let (out, code) = native(
        "fn main() -> Int { \
         defer { rt_print_str(\"first\"); } \
         defer { rt_print_str(\"second\"); } \
         rt_print_str(\"body\"); \
         return 0; }",
    );
    assert_eq!(out, "body\nsecond\nfirst\n");
    assert_eq!(code, 0);
}

#[test]
fn native_runs_cleanup_before_return() {
    let (out, code) = native(
        "fn main() -> Int { \
         defer { rt_print_str(\"cleanup\"); } \
         rt_print_str(\"before\"); \
         return 7; }",
    );
    assert_eq!(out, "before\ncleanup\n");
    assert_eq!(code, 7);
}

#[test]
fn native_runs_cleanup_on_break_and_continue() {
    let (out, code) = native(
        "fn main() -> Int { \
         let mut i = 0; \
         while i < 3 { \
           i += 1; \
           defer { rt_print_int(i); } \
           if i == 1 { continue; } \
           if i == 2 { break; } \
         } \
         return 0; }",
    );
    assert_eq!(out, "1\n2\n");
    assert_eq!(code, 0);
}

#[test]
fn native_runs_cleanup_each_normal_iteration() {
    let (out, code) = native(
        "fn main() -> Int { \
         for n in 0..2 { defer { rt_print_int(n); } } \
         return 0; }",
    );
    assert_eq!(out, "0\n1\n");
    assert_eq!(code, 0);
}

#[test]
fn native_nested_scopes_cleanup_inside_out() {
    let (out, code) = native(
        "fn main() -> Int { \
         defer { rt_print_str(\"outer\"); } \
         if true { \
           defer { rt_print_str(\"inner1\"); } \
           defer { rt_print_str(\"inner2\"); } \
           rt_print_str(\"body\"); \
         } \
         rt_print_str(\"after\"); \
         return 0; }",
    );
    assert_eq!(out, "body\ninner2\ninner1\nafter\nouter\n");
    assert_eq!(code, 0);
}

#[test]
fn native_conditional_registration_runs_only_on_taken_path() {
    let (out, code) = native(
        "fn main() -> Int { \
         if false { defer { rt_print_str(\"skipped\"); } } \
         if true { defer { rt_print_str(\"taken\"); } } \
         defer { rt_print_str(\"outer\"); } \
         return 0; }",
    );
    assert_eq!(out, "taken\nouter\n");
    assert_eq!(code, 0);
}

#[test]
fn native_return_inside_loop_runs_inner_scopes_then_function() {
    let (out, code) = native(
        "fn main() -> Int { \
         defer { rt_print_str(\"function\"); } \
         while true { \
           defer { rt_print_str(\"loop\"); } \
           if true { \
             defer { rt_print_str(\"if\"); } \
             return 9; \
           } \
         } \
         return 0; }",
    );
    assert_eq!(out, "if\nloop\nfunction\n");
    assert_eq!(code, 9);
}

#[test]
fn native_continue_before_registration_does_not_run_later_defer() {
    let (out, code) = native(
        "fn main() -> Int { \
         let mut i = 0; \
         while i < 3 { \
           i += 1; \
           if i == 2 { continue; } \
           defer { rt_print_int(i); } \
         } \
         return 0; }",
    );
    assert_eq!(out, "1\n3\n");
    assert_eq!(code, 0);
}

#[test]
fn native_return_value_is_snapshotted_before_cleanup() {
    let (out, code) = native(
        "fn f() -> Int { \
         let mut x = 1; \
         defer { x = 9; } \
         return x; } \
         fn main() -> Int { let got = f(); rt_print_int(got); return got; }",
    );
    assert_eq!(out, "1\n");
    assert_eq!(code, 1);
}

#[test]
fn native_break_value_is_snapshotted_before_cleanup() {
    let (out, code) = native(
        "fn main() -> Int { \
         let mut x = 1; \
         let got = loop { defer { x = 9; } break x; }; \
         rt_print_int(got); \
         return got; }",
    );
    assert_eq!(out, "1\n");
    assert_eq!(code, 1);
}

#[test]
fn native_defer_body_supports_nested_statements() {
    let (out, code) = native(
        "fn main() -> Int { \
         defer { \
           let x = 3; \
           if x == 3 { rt_print_str(\"nested\"); } \
           for n in 0..2 { rt_print_int(n); } \
         } \
         return 0; }",
    );
    assert_eq!(out, "nested\n0\n1\n");
    assert_eq!(code, 0);
}

#[test]
fn native_nested_defer_inside_defer_runs_inner_first() {
    let (out, code) = native(
        "fn main() -> Int { \
         defer { \
           defer { rt_print_str(\"inner\"); } \
           rt_print_str(\"outer-cleanup\"); \
         } \
         return 0; }",
    );
    assert_eq!(out, "outer-cleanup\ninner\n");
    assert_eq!(code, 0);
}
