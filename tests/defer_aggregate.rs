//! Element-level aggregate ownership (session 273).
//!
//! Tuples and fixed arrays track the ownership state of each element, so moving
//! one element out leaves the others usable, moving the whole aggregate is
//! still possible while nothing has been moved out, and a partially moved
//! aggregate can never be transferred whole (which would duplicate the moved
//! element and free it twice). These tests cover the state rules, the
//! control-flow joins, borrowing, destructuring, `defer move` integration, and
//! native execution of the resulting programs.

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

fn ownership_messages(src: &str) -> Vec<String> {
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
        .map(|error| format!("{} {error}", error.code()))
        .collect()
}

fn ownership_codes(src: &str) -> Vec<String> {
    ownership_messages(src)
        .iter()
        .map(|message| message.split(' ').next().unwrap_or("").to_string())
        .collect()
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn native(src: &str) -> (String, i32) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "mink_defer_aggregate_{}_{}.mink",
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
// Element-level state
// ---------------------------------------------------------------------------

#[test]
fn ownership_allows_freeing_each_array_element() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let a = [rt_str_alloc(1), rt_str_alloc(2)]; \
         rt_str_free(a[0]); \
         rt_str_free(a[1]); \
         return 0; }",
    );
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

#[test]
fn ownership_allows_freeing_each_tuple_field() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let t = (rt_str_alloc(3), rt_str_alloc(4)); \
         rt_str_free(t.0); \
         rt_str_free(t.1); \
         return 0; }",
    );
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

#[test]
fn ownership_rejects_second_free_of_one_element() {
    let messages = ownership_messages(
        "fn main() -> Int { \
         let a = [rt_str_alloc(1), rt_str_alloc(2)]; \
         rt_str_free(a[0]); \
         rt_str_free(a[0]); \
         return 0; }",
    );
    assert_eq!(messages.len(), 1, "messages: {messages:?}");
    assert!(messages[0].starts_with("E-S10"), "messages: {messages:?}");
    assert!(
        messages[0].contains("element 0 of `a` was moved"),
        "the diagnostic names the moved element: {messages:?}"
    );
}

#[test]
fn ownership_rejects_reading_a_moved_element() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let t = (rt_str_alloc(3), rt_str_alloc(4)); \
         let x = t.0; \
         rt_print_int(rt_str_len(t.0)); \
         rt_str_free(x); \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_allows_reading_the_other_element_after_a_move() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let t = (rt_str_alloc(3), rt_str_alloc(4)); \
         let x = t.0; \
         rt_print_int(rt_str_len(t.1)); \
         rt_str_free(t.1); \
         rt_str_free(x); \
         return 0; }",
    );
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

#[test]
fn ownership_rejects_whole_move_of_a_partially_moved_aggregate() {
    let messages = ownership_messages(
        "fn main() -> Int { \
         let t = (rt_str_alloc(3), rt_str_alloc(4)); \
         let x = t.0; \
         let y = t; \
         rt_str_free(x); \
         return 0; }",
    );
    assert_eq!(messages.len(), 1, "messages: {messages:?}");
    assert!(
        messages[0].contains("E-S10") && messages[0].contains("element 0"),
        "messages: {messages:?}"
    );
}

#[test]
fn ownership_rejects_element_use_after_whole_array_move() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let a = [rt_str_alloc(1), rt_str_alloc(2)]; \
         let b = a; \
         rt_str_free(a[0]); \
         rt_str_free(b[0]); \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_rejects_runtime_index_after_element_move() {
    let messages = ownership_messages(
        "fn main() -> Int { \
         let a = [rt_str_alloc(1), rt_str_alloc(2)]; \
         let s = a[0]; \
         let mut i = 1; \
         rt_print_int(rt_str_len(a[i])); \
         rt_str_free(s); \
         return 0; }",
    );
    assert_eq!(messages.len(), 1, "messages: {messages:?}");
    assert!(
        messages[0].contains("runtime index could select it"),
        "messages: {messages:?}"
    );
}

#[test]
fn ownership_moves_whole_array_through_runtime_index() {
    // A transfer through a runtime index moves the whole array, so a later
    // element transfer is a use-after-move.
    let codes = ownership_codes(
        "fn main() -> Int { \
         let a = [rt_str_alloc(1), rt_str_alloc(2)]; \
         let mut i = 0; \
         let s = a[i]; \
         rt_str_free(a[0]); \
         rt_str_free(s); \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_allows_copy_aggregates_to_copy() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let t = (1, 2); \
         let u = t; \
         let v = t; \
         let a = [3, 4]; \
         let b = a; \
         return u.0 + v.1 + b[0]; }",
    );
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

// ---------------------------------------------------------------------------
// Control flow
// ---------------------------------------------------------------------------

#[test]
fn ownership_rejects_element_move_in_one_branch_used_after_it() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let t = (rt_str_alloc(2), rt_str_alloc(3)); \
         if true { let x = t.0; rt_str_free(x); } \
         rt_str_free(t.0); \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_allows_element_moves_in_disjoint_branches() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let t = (rt_str_alloc(2), rt_str_alloc(3)); \
         if true { rt_str_free(t.0); } else { rt_str_free(t.1); } \
         return 0; }",
    );
    // Each branch owns the element it frees; the other element is still live
    // afterwards, and nothing uses it.
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

#[test]
fn ownership_rejects_element_move_repeated_by_loop_iteration() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let a = [rt_str_alloc(1), rt_str_alloc(2)]; \
         let mut i = 0; \
         while i < 2 { i += 1; let s = a[0]; rt_str_free(s); } \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_allows_element_move_inside_loop_with_body_local_aggregate() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let mut i = 0; \
         while i < 2 { \
           i += 1; \
           let a = [rt_str_alloc(1), rt_str_alloc(2)]; \
           rt_str_free(a[0]); \
           rt_str_free(a[1]); \
         } \
         return 0; }",
    );
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

// ---------------------------------------------------------------------------
// Borrowing
// ---------------------------------------------------------------------------

#[test]
fn ownership_rejects_element_move_while_borrowed() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let a = [rt_str_alloc(1), rt_str_alloc(2)]; \
         let r = &a; \
         rt_str_free(a[0]); \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S12"]);
}

#[test]
fn ownership_rejects_whole_move_while_borrowed() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let t = (rt_str_alloc(1), rt_str_alloc(2)); \
         let r = &t; \
         let u = t; \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S12"]);
}

// ---------------------------------------------------------------------------
// Reassignment, reconstruction and destructuring
// ---------------------------------------------------------------------------

#[test]
fn ownership_allows_element_reassignment_after_a_move() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let mut a = [rt_str_alloc(1), rt_str_alloc(2)]; \
         rt_str_free(a[0]); \
         a[0] = rt_str_alloc(5); \
         rt_str_free(a[0]); \
         rt_str_free(a[1]); \
         return 0; }",
    );
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

#[test]
fn ownership_allows_whole_array_reassignment() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let mut a = [rt_str_alloc(1), rt_str_alloc(2)]; \
         a = [rt_str_alloc(3), rt_str_alloc(4)]; \
         rt_str_free(a[0]); \
         rt_str_free(a[1]); \
         return 0; }",
    );
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

#[test]
fn ownership_tuple_destructuring_moves_the_elements() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let t = (rt_str_alloc(2), rt_str_alloc(3)); \
         let (x, y) = t; \
         rt_str_free(x); \
         rt_str_free(y); \
         rt_str_free(t.0); \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"], "the source tuple is moved");
}

#[test]
fn ownership_tuple_destructuring_of_copy_values_copies() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let t = (1, 2); \
         let (x, y) = t; \
         return x + y + t.0; }",
    );
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

// ---------------------------------------------------------------------------
// Nested places (conservative) and unsupported compositions
// ---------------------------------------------------------------------------

#[test]
fn ownership_nested_array_field_move_is_conservative() {
    // An array inside a struct field has no per-element state, so moving one
    // element moves the whole struct value (conservative, sound).
    let codes = ownership_codes(
        "struct H { a: [Str; 2] } \
         fn main() -> Int { \
         let h = H { a: [rt_str_alloc(1), rt_str_alloc(2)] }; \
         rt_str_free(h.a[0]); \
         rt_str_free(h.a[1]); \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_aggregate_match_is_rejected_by_the_type_checker() {
    // Tuples are not matchable today, so no aggregate ever reaches a match
    // arm; the parser and checker reject it (E-T26).
    let (map, id) = parsed(
        "fn main() -> Int { \
         let t = (1, 2); \
         match t { (a, b) => { return a; }, } }",
    );
    let file = map.get(id).expect("source file present");
    let result = parser::parse(file);
    let semantic = mink::semantics::analyze(result.ast());
    let types = mink::typecheck::check(result.ast(), &semantic, &map);
    assert!(
        types.errors().iter().any(|error| error.code() == "E-T26"),
        "expected E-T26: {:?}",
        types.errors()
    );
}

// ---------------------------------------------------------------------------
// `defer move` integration
// ---------------------------------------------------------------------------

#[test]
fn ownership_allows_array_capture_of_owned_elements() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let a = [rt_str_alloc(2), rt_str_alloc(3)]; \
         defer move (a) { rt_str_free(a[0]); rt_str_free(a[1]); } \
         return 0; }",
    );
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

#[test]
fn ownership_allows_tuple_capture_of_owned_elements() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let t = (rt_str_alloc(2), rt_str_alloc(3)); \
         defer move (t) { rt_str_free(t.0); rt_str_free(t.1); } \
         return 0; }",
    );
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

#[test]
fn ownership_rejects_capture_of_a_partially_moved_aggregate() {
    let messages = ownership_messages(
        "fn main() -> Int { \
         let a = [rt_str_alloc(1), rt_str_alloc(2)]; \
         let s = a[0]; \
         defer move (a) { rt_str_free(a[1]); } \
         rt_str_free(s); \
         return 0; }",
    );
    assert_eq!(messages.len(), 1, "messages: {messages:?}");
    assert!(
        messages[0].contains("E-S10") && messages[0].contains("element 0"),
        "messages: {messages:?}"
    );
}

#[test]
fn ownership_rejects_element_use_after_aggregate_capture() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let a = [rt_str_alloc(2), rt_str_alloc(3)]; \
         defer move (a) { rt_str_free(a[0]); rt_str_free(a[1]); } \
         rt_print_int(rt_str_len(a[0])); \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_rejects_duplicate_cleanup_of_one_captured_element() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let a = [rt_str_alloc(2), rt_str_alloc(3)]; \
         defer move (a) { rt_str_free(a[0]); rt_str_free(a[0]); rt_str_free(a[1]); } \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_rejects_loop_reregistration_of_aggregate_capture() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let a = [rt_str_alloc(1), rt_str_alloc(2)]; \
         let mut i = 0; \
         while i < 2 { i += 1; defer move (a) { rt_str_free(a[0]); rt_str_free(a[1]); } } \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

// ---------------------------------------------------------------------------
// Native execution
// ---------------------------------------------------------------------------

#[test]
fn native_array_elements_free_individually() {
    let (out, code) = native(
        "fn main() -> Int { \
         let a = [rt_str_alloc(2), rt_str_alloc(3)]; \
         rt_print_int(rt_str_len(a[0])); \
         rt_print_int(rt_str_len(a[1])); \
         rt_str_free(a[1]); \
         rt_str_free(a[0]); \
         rt_print_str(\"done\"); \
         return 0; }",
    );
    assert_eq!(out, "2\n3\ndone\n");
    assert_eq!(code, 0);
}

#[test]
fn native_tuple_elements_free_individually() {
    let (out, code) = native(
        "fn main() -> Int { \
         let t = (rt_str_alloc(5), rt_str_alloc(6)); \
         rt_print_int(rt_str_len(t.0) + rt_str_len(t.1)); \
         rt_str_free(t.0); \
         rt_str_free(t.1); \
         rt_print_str(\"done\"); \
         return 0; }",
    );
    assert_eq!(out, "11\ndone\n");
    assert_eq!(code, 0);
}

#[test]
fn native_array_capture_frees_both_elements() {
    let (out, code) = native(
        "fn main() -> Int { \
         let a = [rt_str_alloc(2), rt_str_alloc(3)]; \
         defer move (a) { \
           rt_print_int(rt_str_len(a[0]) + rt_str_len(a[1])); \
           rt_str_free(a[1]); \
           rt_str_free(a[0]); \
         } \
         rt_print_str(\"body\"); \
         return 0; }",
    );
    assert_eq!(out, "body\n5\n");
    assert_eq!(code, 0);
}

#[test]
fn native_tuple_capture_frees_both_elements() {
    let (out, code) = native(
        "fn main() -> Int { \
         let t = (rt_str_alloc(3), rt_str_alloc(4)); \
         defer move (t) { rt_str_free(t.0); rt_str_free(t.1); } \
         rt_print_str(\"body\"); \
         return 0; }",
    );
    assert_eq!(out, "body\n");
    assert_eq!(code, 0);
}

#[test]
fn native_array_element_cleanup_on_early_return() {
    let (out, code) = native(
        "fn run() -> Int { \
           let a = [rt_str_alloc(2), rt_str_alloc(3)]; \
           defer move (a) { rt_str_free(a[0]); rt_str_free(a[1]); } \
           return 7; } \
         fn main() -> Int { rt_print_int(run()); return 0; }",
    );
    assert_eq!(out, "7\n");
    assert_eq!(code, 0);
}

#[test]
fn native_element_reassignment_frees_the_new_value() {
    let (out, code) = native(
        "fn main() -> Int { \
         let mut a = [rt_str_alloc(1), rt_str_alloc(2)]; \
         rt_str_free(a[0]); \
         a[0] = rt_str_alloc(9); \
         rt_print_int(rt_str_len(a[0])); \
         rt_str_free(a[0]); \
         rt_str_free(a[1]); \
         return 0; }",
    );
    assert_eq!(out, "9\n");
    assert_eq!(code, 0);
}

#[test]
fn native_tuple_destructuring_frees_both_elements() {
    let (out, code) = native(
        "fn main() -> Int { \
         let t = (rt_str_alloc(2), rt_str_alloc(3)); \
         let (x, y) = t; \
         rt_print_int(rt_str_len(x) + rt_str_len(y)); \
         rt_str_free(x); \
         rt_str_free(y); \
         return 0; }",
    );
    assert_eq!(out, "5\n");
    assert_eq!(code, 0);
}

#[test]
fn native_array_per_loop_iteration() {
    let (out, code) = native(
        "fn main() -> Int { \
         let mut i = 0; \
         while i < 3 { \
           i += 1; \
           let a = [rt_str_alloc(i), rt_str_alloc(i)]; \
           rt_print_int(rt_str_len(a[0])); \
           defer move (a) { rt_str_free(a[0]); rt_str_free(a[1]); } \
         } \
         rt_print_str(\"done\"); \
         return 0; }",
    );
    assert_eq!(out, "1\n2\n3\ndone\n");
    assert_eq!(code, 0);
}

#[test]
fn native_conditional_element_move_and_free() {
    let (out, code) = native(
        "fn main() -> Int { \
         let t = (rt_str_alloc(2), rt_str_alloc(3)); \
         if true { let x = t.0; rt_print_int(rt_str_len(x)); rt_str_free(x); } \
         rt_print_int(rt_str_len(t.1)); \
         rt_str_free(t.1); \
         rt_print_str(\"done\"); \
         return 0; }",
    );
    assert_eq!(out, "2\n3\ndone\n");
    assert_eq!(code, 0);
}

#[test]
fn native_whole_array_transfer_keeps_elements_alive() {
    let (out, code) = native(
        "fn main() -> Int { \
         let a = [rt_str_alloc(2), rt_str_alloc(3)]; \
         let b = a; \
         rt_print_int(rt_str_len(b[0]) + rt_str_len(b[1])); \
         rt_str_free(b[0]); \
         rt_str_free(b[1]); \
         return 0; }",
    );
    assert_eq!(out, "5\n");
    assert_eq!(code, 0);
}

#[test]
fn native_parameter_aggregate_capture() {
    let (out, code) = native(
        "fn release(a: [Str; 2]) -> Int { \
           defer move (a) { rt_str_free(a[0]); rt_str_free(a[1]); } \
           rt_print_str(\"in-body\"); \
           return 0; } \
         fn main() -> Int { return release([rt_str_alloc(2), rt_str_alloc(3)]); }",
    );
    assert_eq!(out, "in-body\n");
    assert_eq!(code, 0);
}

#[test]
fn native_conditional_aggregate_capture_runs_at_branch_exit() {
    let (out, code) = native(
        "fn main() -> Int { \
         let a = [rt_str_alloc(2), rt_str_alloc(3)]; \
         if true { \
           defer move (a) { rt_print_str(\"freed\"); rt_str_free(a[0]); rt_str_free(a[1]); } \
           rt_print_str(\"in-branch\"); \
         } \
         rt_print_str(\"after\"); \
         return 0; }",
    );
    assert_eq!(out, "in-branch\nfreed\nafter\n");
    assert_eq!(code, 0);
}
