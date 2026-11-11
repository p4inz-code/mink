//! Explicit move capture for `defer` — `defer move (a, b) { ... }`.
//!
//! A move capture transfers the listed local bindings into the deferred
//! operation when the `defer` statement executes. The originals become moved,
//! the cleanup reads the transferred values from capture slots, and nothing
//! else can reach them — so the value is guaranteed live when cleanup runs and
//! exactly one owner exists. These tests cover the grammar, semantic and
//! ownership rules, and native execution of every exit path, and they are the
//! permanent regression suite for that feature.

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
        "mink_defer_move_test_{}_{}.mink",
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
fn parser_accepts_move_capture() {
    assert!(parse_ok(
        "fn main() { let a = rt_str_alloc(2); let b = 1; \
         defer move (a, b) { rt_str_free(a); rt_print_int(b); } }"
    ));
}

#[test]
fn parser_accepts_move_capture_with_trailing_comma() {
    assert!(parse_ok(
        "fn main() { let a = rt_str_alloc(2); defer move (a,) { rt_str_free(a); } }"
    ));
}

#[test]
fn parser_rejects_empty_move_capture() {
    assert!(!parse_ok("fn main() { defer move () { } }"));
}

#[test]
fn parser_rejects_move_capture_without_list() {
    assert!(!parse_ok(
        "fn main() { let a = rt_str_alloc(2); defer move a { rt_str_free(a); } }"
    ));
}

#[test]
fn parser_rejects_unterminated_move_capture() {
    assert!(!parse_ok(
        "fn main() { let a = 1; defer move (a { rt_print_int(a); } }"
    ));
}

#[test]
fn parser_accepts_plain_defer_after_move_support() {
    assert!(parse_ok("fn main() { defer { } }"));
}

// ---------------------------------------------------------------------------
// Semantics
// ---------------------------------------------------------------------------

#[test]
fn semantics_rejects_move_capture_inside_defer() {
    let codes = semantic_codes(
        "fn main() { let s = rt_str_alloc(2); defer { defer move (s) { rt_str_free(s); } } }",
    );
    assert_eq!(codes, vec!["E-S19"]);
}

#[test]
fn semantics_rejects_duplicate_capture_name() {
    let codes = semantic_codes(
        "fn main() { let s = rt_str_alloc(2); defer move (s, s) { rt_str_free(s); } }",
    );
    assert_eq!(codes, vec!["E-S19"]);
}

#[test]
fn semantics_rejects_unresolved_capture() {
    // Both the capture entry and the body reference are unresolved; each use
    // reports independently (`E-S01`), which is the analyzer's contract.
    let codes = semantic_codes("fn main() { defer move (nope) { rt_print_int(nope); } }");
    assert!(!codes.is_empty());
    assert!(codes.iter().all(|code| code == "E-S01"), "codes: {codes:?}");
}

#[test]
fn semantics_rejects_constant_capture() {
    let codes = semantic_codes("const N = 4; fn main() { defer move (N) { rt_print_int(N); } }");
    assert_eq!(codes, vec!["E-S19"]);
}

#[test]
fn semantics_rejects_module_binding_capture() {
    let codes = semantic_codes("let g = 1; fn main() { defer move (g) { rt_print_int(g); } }");
    assert_eq!(codes, vec!["E-S19"]);
}

#[test]
fn semantics_rejects_function_name_capture() {
    let codes = semantic_codes(
        "fn helper() -> Int { return 1; } fn main() { defer move (helper) { helper(); } }",
    );
    assert_eq!(codes, vec!["E-S19"]);
}

#[test]
fn semantics_accepts_move_capture_in_loop_and_arm() {
    assert!(semantic_codes(
        "fn main() { let mut i = 0; \
         while i < 2 { i += 1; let s = rt_str_alloc(1); defer move (s) { rt_str_free(s); } } \
         match 1 { 1 => { let t = rt_str_alloc(1); defer move (t) { rt_str_free(t); } }, _ => { } } }"
    )
    .is_empty());
}

// ---------------------------------------------------------------------------
// Ownership: accepted captures
// ---------------------------------------------------------------------------

#[test]
fn ownership_allows_owned_str_capture() {
    assert!(
        ownership_codes(
            "fn main() { let s = rt_str_alloc(2); defer move (s) { rt_str_free(s); } }"
        )
        .is_empty()
    );
}

#[test]
fn ownership_allows_copy_and_owned_capture_together() {
    assert!(
        ownership_codes(
            "fn main() { let s = rt_str_alloc(2); let n = 3; \
         defer move (s, n) { rt_print_int(n); rt_str_free(s); } }"
        )
        .is_empty()
    );
}

#[test]
fn ownership_allows_struct_capture() {
    assert!(
        ownership_codes(
            "struct R { s: Str, n: Int } \
         fn main() { let r = R { s: rt_str_alloc(2), n: 1 }; \
         defer move (r) { rt_print_int(r.n); rt_str_free(r.s); } }"
        )
        .is_empty()
    );
}

#[test]
fn ownership_allows_enum_payload_capture() {
    assert!(
        ownership_codes(
            "enum E { A(Str), B } \
         fn main() { let e = E::A(rt_str_alloc(2)); \
         defer move (e) { match e { E::A(s) => { rt_str_free(s); }, _ => { } } } }"
        )
        .is_empty()
    );
}

#[test]
fn ownership_allows_cleanup_local_allocation() {
    // A binding declared inside a cleanup lives exactly as long as the
    // cleanup, so it is never a deferred capture.
    assert!(
        ownership_codes("fn main() { defer { let tmp = rt_str_alloc(2); rt_str_free(tmp); } }")
            .is_empty()
    );
}

#[test]
fn ownership_allows_for_variable_capture() {
    assert!(
        ownership_codes("fn main() { for n in 0..2 { defer move (n) { rt_print_int(n); } } }")
            .is_empty()
    );
}

#[test]
fn ownership_allows_partial_field_use_in_cleanup() {
    // The cleanup consumes one owned field of the transferred struct; the
    // original binding is dead, so no second move of the same field exists.
    assert!(
        ownership_codes(
            "struct R { a: Str, b: Str } \
         fn main() { let r = R { a: rt_str_alloc(1), b: rt_str_alloc(2) }; \
         defer move (r) { rt_str_free(r.a); rt_str_free(r.b); } }"
        )
        .is_empty()
    );
}

// ---------------------------------------------------------------------------
// Ownership: rejected shapes
// ---------------------------------------------------------------------------

#[test]
fn ownership_allows_immutable_string_capture_and_keeps_original_live() {
    // A string literal is immortal: capturing it copies, so the original
    // binding stays usable (a move of an immutable string never kills it).
    assert!(
        ownership_codes(
            "fn main() { let s = \"hi\"; defer move (s) { rt_print_str(s); } rt_print_str(s); }"
        )
        .is_empty()
    );
}

#[test]
fn ownership_rejects_use_after_move_of_captured() {
    let codes = ownership_codes(
        "fn main() { let s = rt_str_alloc(2); defer move (s) { rt_str_free(s); } \
         rt_str_free(s); }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_rejects_reading_captured_binding_after_registration() {
    let codes = ownership_codes(
        "fn main() { let s = rt_str_alloc(2); defer move (s) { rt_str_free(s); } \
         rt_print_int(rt_str_len(s)); }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_rejects_second_capture_of_same_value() {
    let codes = ownership_codes(
        "fn main() { let s = rt_str_alloc(2); \
         defer move (s) { rt_str_free(s); } \
         defer move (s) { rt_str_free(s); } }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_rejects_return_of_captured_value() {
    let codes = ownership_codes(
        "fn f() -> Str { let s = rt_str_alloc(2); defer move (s) { rt_str_free(s); } \
         return s; } \
         fn main() { }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_rejects_pass_of_captured_value() {
    let codes = ownership_codes(
        "fn take(s: Str) -> Int { return rt_str_len(s); } \
         fn main() { let s = rt_str_alloc(2); defer move (s) { rt_str_free(s); } take(s); }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_rejects_reference_capture() {
    let codes = ownership_codes(
        "fn main() { let n = 1; let r = &n; defer move (r) { rt_print_int(*r); } }",
    );
    assert_eq!(codes, vec!["E-S19"]);
}

#[test]
fn ownership_rejects_array_capture() {
    let codes =
        ownership_codes("fn main() { let a = [1, 2]; defer move (a) { rt_print_int(a[0]); } }");
    assert_eq!(codes, vec!["E-S19"]);
}

#[test]
fn ownership_rejects_tuple_capture() {
    let codes =
        ownership_codes("fn main() { let t = (1, true); defer move (t) { rt_print_int(t.0); } }");
    assert_eq!(codes, vec!["E-S19"]);
}

#[test]
fn ownership_rejects_collection_capture() {
    let codes =
        ownership_codes("fn main() { let v = rt_vec_new(8); defer move (v) { rt_vec_free(v); } }");
    assert_eq!(codes, vec!["E-S19"]);
}

#[test]
fn ownership_rejects_struct_capture_holding_reference() {
    let codes = ownership_codes(
        "struct H { r: &Int } \
         fn main() { let n = 1; let h = H { r: &n }; defer move (h) { rt_print_int(*h.r); } }",
    );
    assert_eq!(codes, vec!["E-S19"]);
}

#[test]
fn ownership_rejects_struct_capture_holding_array() {
    let codes = ownership_codes(
        "struct H { a: [Int; 2] } \
         fn main() { let h = H { a: [1, 2] }; defer move (h) { rt_print_int(h.a[0]); } }",
    );
    assert_eq!(codes, vec!["E-S19"]);
}

#[test]
fn ownership_rejects_unused_capture() {
    let codes = ownership_codes(
        "fn main() { let s = rt_str_alloc(2); defer move (s) { rt_print_str(\"x\"); } }",
    );
    assert_eq!(codes, vec!["E-S19"]);
}

#[test]
fn ownership_rejects_double_free_inside_cleanup() {
    let codes = ownership_codes(
        "fn main() { let s = rt_str_alloc(2); \
         defer move (s) { rt_str_free(s); rt_str_free(s); } }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_rejects_capture_while_borrowed() {
    let codes = ownership_codes(
        "fn main() { let s = rt_str_alloc(2); let r = &s; \
         defer move (s) { rt_str_free(s); } \
         rt_print_int(rt_str_len(*r)); }",
    );
    assert!(!codes.is_empty());
    assert!(codes.iter().all(|code| code == "E-S12"), "codes: {codes:?}");
}

#[test]
fn ownership_rejects_uncaptured_owned_read_in_move_defer() {
    // Listing `a` does not open the door for other owned locals: the plain
    // `E-S18` rule still applies to everything not transferred.
    let codes = ownership_codes(
        "fn main() { let a = rt_str_alloc(2); let b = rt_str_alloc(2); \
         defer move (a) { rt_str_free(a); rt_str_free(b); } \
         rt_str_free(b); }",
    );
    assert!(codes.iter().any(|code| code == "E-S18"), "codes: {codes:?}");
}

#[test]
fn ownership_still_rejects_plain_defer_owned_capture() {
    let codes = ownership_codes("fn main() { let s = rt_str_alloc(2); defer { rt_str_free(s); } }");
    assert_eq!(codes, vec!["E-S18"]);
}

#[test]
fn ownership_still_rejects_plain_defer_tuple_capture() {
    let codes = ownership_codes(
        "fn main() { let t = (rt_str_alloc(2), 1); defer { rt_str_free(t.0); } rt_str_free(t.0); }",
    );
    assert_eq!(codes, vec!["E-S18"]);
}

// ---------------------------------------------------------------------------
// Native execution
// ---------------------------------------------------------------------------

#[test]
fn native_move_capture_frees_once_on_normal_exit() {
    let (out, code) = native(
        "fn main() -> Int { \
         let s = rt_str_alloc(8); \
         defer move (s) { rt_print_str(\"freed\"); rt_str_free(s); } \
         rt_print_str(\"body\"); \
         return 0; }",
    );
    assert_eq!(out, "body\nfreed\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_two_resources_is_lifo() {
    let (out, code) = native(
        "fn main() -> Int { \
         let a = rt_str_alloc(4); \
         let b = rt_str_alloc(8); \
         defer move (a) { rt_print_str(\"free-a\"); rt_str_free(a); } \
         defer move (b) { rt_print_str(\"free-b\"); rt_str_free(b); } \
         rt_print_str(\"body\"); \
         return 0; }",
    );
    assert_eq!(out, "body\nfree-b\nfree-a\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_runs_on_return() {
    let (out, code) = native(
        "fn f() -> Int { \
           let s = rt_str_alloc(8); \
           defer move (s) { rt_print_str(\"cleanup\"); rt_str_free(s); } \
           rt_print_str(\"before\"); \
           return 7; } \
         fn main() -> Int { return f(); }",
    );
    assert_eq!(out, "before\ncleanup\n");
    assert_eq!(code, 7);
}

#[test]
fn native_move_capture_runs_each_loop_iteration() {
    let (out, code) = native(
        "fn main() -> Int { \
         let mut i = 0; \
         while i < 3 { \
           i += 1; \
           let s = rt_str_alloc(8); \
           defer move (s) { rt_print_int(i); rt_str_free(s); } \
         } \
         rt_print_str(\"done\"); \
         return 0; }",
    );
    assert_eq!(out, "1\n2\n3\ndone\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_conditional_registration_transfers_only_when_taken() {
    let (out, code) = native(
        "fn run(flag: Bool) -> Int { \
           let s = rt_str_alloc(8); \
           if flag { \
             defer move (s) { rt_print_str(\"freed\"); rt_str_free(s); } \
           } else { \
             rt_str_free(s); \
           } \
           return 0; } \
         fn main() -> Int { run(true); run(false); return 0; }",
    );
    assert_eq!(out, "freed\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_handles_break_and_continue() {
    let (out, code) = native(
        "fn main() -> Int { \
         let mut i = 0; \
         while i < 4 { \
           i += 1; \
           let s = rt_str_alloc(i); \
           defer move (s) { rt_print_int(rt_str_len(s)); rt_str_free(s); } \
           if i == 1 { continue; } \
           if i == 3 { break; } \
         } \
         return 0; }",
    );
    assert_eq!(out, "1\n2\n3\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_nested_scopes_run_inside_out() {
    let (out, code) = native(
        "fn main() -> Int { \
         let outer = rt_str_alloc(2); \
         defer move (outer) { rt_print_str(\"outer-free\"); rt_str_free(outer); } \
         if true { \
           let inner = rt_str_alloc(3); \
           defer move (inner) { rt_print_str(\"inner-free\"); rt_str_free(inner); } \
           rt_print_str(\"in-block\"); \
         } \
         rt_print_str(\"after-block\"); \
         return 0; }",
    );
    assert_eq!(out, "in-block\ninner-free\nafter-block\nouter-free\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_struct_cleanup() {
    let (out, code) = native(
        "struct Res { name: Str, size: Int } \
         fn main() -> Int { \
           let r = Res { name: rt_str_alloc(4), size: 3 }; \
           defer move (r) { rt_print_int(r.size); rt_str_free(r.name); } \
           rt_print_str(\"body\"); \
           return 0; }",
    );
    assert_eq!(out, "body\n3\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_enum_payload_cleanup() {
    let (out, code) = native(
        "enum State { Empty, Data(Str) } \
         fn main() -> Int { \
           let st = State::Data(rt_str_alloc(6)); \
           defer move (st) { \
             match st { \
               State::Data(s) => { rt_print_int(rt_str_len(s)); rt_str_free(s); }, \
               _ => { rt_print_str(\"empty\"); }, \
             } \
           } \
           rt_print_str(\"body\"); \
           return 0; }",
    );
    assert_eq!(out, "body\n6\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_copy_values_snapshot_at_registration() {
    let (out, code) = native(
        "fn main() -> Int { \
         let mut n = 1; \
         let s = rt_str_alloc(4); \
         defer move (s, n) { rt_print_int(n); rt_str_free(s); } \
         n = 99; \
         return 0; }",
    );
    assert_eq!(out, "1\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_then_reassignment_frees_both_values() {
    let (out, code) = native(
        "fn main() -> Int { \
         let mut s = rt_str_alloc(4); \
         defer move (s) { rt_print_str(\"first\"); rt_str_free(s); } \
         s = rt_str_alloc(8); \
         rt_str_free(s); \
         rt_print_str(\"done\"); \
         return 0; }",
    );
    assert_eq!(out, "done\nfirst\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_keeps_return_value_snapshot() {
    let (out, code) = native(
        "fn f() -> Int { \
           let mut x = 1; \
           let s = rt_str_alloc(4); \
           defer move (s) { rt_str_free(s); x = 9; } \
           return x; } \
         fn main() -> Int { let got = f(); rt_print_int(got); return got; }",
    );
    assert_eq!(out, "1\n");
    assert_eq!(code, 1);
}

#[test]
fn native_move_capture_inside_match_arm() {
    let (out, code) = native(
        "fn main() -> Int { \
         let mut i = 0; \
         loop { \
           i += 1; \
           match i { \
             1 => { let s = rt_str_alloc(1); \
                    defer move (s) { rt_print_str(\"arm1\"); rt_str_free(s); } }, \
             2 => { let s = rt_str_alloc(2); \
                    defer move (s) { rt_print_str(\"arm2\"); rt_str_free(s); } break; }, \
             _ => { }, \
           } \
         } \
         rt_print_str(\"done\"); \
         return 0; }",
    );
    assert_eq!(out, "arm1\narm2\ndone\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_cleanup_can_allocate_and_free_temporaries() {
    let (out, code) = native(
        "fn main() -> Int { \
         let s = rt_str_alloc(4); \
         defer move (s) { \
           let tmp = rt_str_alloc(2); \
           rt_print_int(rt_str_len(tmp) + rt_str_len(s)); \
           rt_str_free(tmp); \
           rt_str_free(s); \
         } \
         return 0; }",
    );
    assert_eq!(out, "6\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_in_generic_function_both_instantiations() {
    let (out, code) = native(
        "fn hold<T>(tag: T) -> Int { \
           let s = rt_str_alloc(3); \
           defer move (s) { rt_print_str(\"freed\"); rt_str_free(s); } \
           return 0; } \
         fn main() -> Int { hold(1); hold(true); return 0; }",
    );
    assert_eq!(out, "freed\nfreed\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_keeps_break_value_snapshot() {
    let (out, code) = native(
        "fn main() -> Int { \
         let mut x = 1; \
         let s = rt_str_alloc(4); \
         let got = loop { \
           defer move (s) { rt_str_free(s); x = 9; } \
           break x; \
         }; \
         rt_print_int(got); \
         return got; }",
    );
    assert_eq!(out, "1\n");
    assert_eq!(code, 1);
}

#[test]
fn native_move_capture_continue_before_registration_cleans_nothing() {
    let (out, code) = native(
        "fn main() -> Int { \
         let mut i = 0; \
         while i < 3 { \
           i += 1; \
           if i == 2 { continue; } \
           let s = rt_str_alloc(i); \
           defer move (s) { rt_print_int(rt_str_len(s)); rt_str_free(s); } \
         } \
         return 0; }",
    );
    assert_eq!(out, "1\n3\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_immutable_string_prints_twice() {
    let (out, code) = native(
        "fn main() -> Int { \
         let s = \"hi\"; \
         defer move (s) { rt_print_str(s); } \
         rt_print_str(s); \
         return 0; }",
    );
    assert_eq!(out, "hi\nhi\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_parameter_cleanup() {
    let (out, code) = native(
        "fn consume(payload: Str) -> Int { \
           defer move (payload) { rt_print_str(\"closing\"); rt_str_free(payload); } \
           return 0; } \
         fn main() -> Int { return consume(rt_str_alloc(2)); }",
    );
    assert_eq!(out, "closing\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_mixed_with_plain_defer_order() {
    let (out, code) = native(
        "fn main() -> Int { \
         let s = rt_str_alloc(4); \
         defer { rt_print_str(\"plain\"); } \
         defer move (s) { rt_print_str(\"moved\"); rt_str_free(s); } \
         rt_print_str(\"body\"); \
         return 0; }",
    );
    assert_eq!(out, "body\nmoved\nplain\n");
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// Session 268: conditional ownership and control-flow correctness
// ---------------------------------------------------------------------------

/// A capture registered in a loop body transfers the value on the first
/// iteration, so a second iteration would transfer an already freed value.
/// The loop head is the join of the loop entry and every back edge, and the
/// transfer is diagnosed there.
#[test]
fn ownership_rejects_capture_of_outer_binding_registered_in_while_loop() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let s = rt_str_alloc(3); \
         let mut i = 0; \
         while i < 2 { \
           i += 1; \
           defer move (s) { rt_str_free(s); } \
         } \
         return 0; }",
    );
    assert_eq!(
        codes,
        vec!["E-S10"],
        "one diagnostic for the repeated transfer"
    );
}

#[test]
fn ownership_rejects_capture_of_outer_binding_registered_in_for_loop() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let s = rt_str_alloc(3); \
         for i in 0..3 { defer move (s) { rt_str_free(s); } } \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

#[test]
fn ownership_rejects_capture_reached_by_continue_back_edge() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let s = rt_str_alloc(3); \
         let mut i = 0; \
         while i < 2 { \
           i += 1; \
           defer move (s) { rt_str_free(s); } \
           if i == 1 { continue; } \
           break; \
         } \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

/// A body-declared value is a fresh owner each iteration, so registering it in
/// the loop body is safe — the case the loop rule must not reject.
#[test]
fn ownership_allows_capture_of_loop_local_in_loop_body() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let mut i = 0; \
         while i < 2 { \
           i += 1; \
           let s = rt_str_alloc(3); \
           defer move (s) { rt_str_free(s); } \
         } \
         return 0; }",
    );
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

/// A `break` leaves the loop, so a body that always breaks has no back edge:
/// the transfer happens exactly once and the program is accepted.
#[test]
fn ownership_allows_capture_registered_once_before_break() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let s = rt_str_alloc(3); \
         loop { defer move (s) { rt_str_free(s); } break; } \
         return 0; }",
    );
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

/// The captured value is transferred on the path that reaches the loop, so it
/// cannot be used after the loop either.
#[test]
fn ownership_rejects_use_after_loop_of_captured_binding() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let s = rt_str_alloc(3); \
         loop { defer move (s) { rt_str_free(s); } break; } \
         rt_str_free(s); \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

/// A binding freed on one branch and captured after the join is moved on the
/// taken path; the join reports the later transfer once.
#[test]
fn ownership_rejects_capture_after_branch_that_freed_it() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let s = rt_str_alloc(3); \
         if true { rt_str_free(s); } \
         defer move (s) { rt_str_free(s); } \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

/// Each branch owns the value on its own path: capture in one arm, free in the
/// other. Every path frees exactly once, so this must be accepted.
#[test]
fn ownership_allows_conditional_capture_with_freeing_else_branch() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let s = rt_str_alloc(3); \
         if true { defer move (s) { rt_str_free(s); } } \
         else { rt_str_free(s); } \
         return 0; }",
    );
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

/// The general move analysis shares the loop head, so a plain move repeated by
/// a second iteration is rejected too (previously accepted, then a double free
/// at run time).
#[test]
fn ownership_rejects_move_repeated_by_loop_iteration() {
    let codes = ownership_codes(
        "fn main() -> Int { \
         let s = rt_str_alloc(3); \
         let mut i = 0; \
         while i < 2 { i += 1; let t = s; rt_str_free(t); } \
         return 0; }",
    );
    assert_eq!(codes, vec!["E-S10"]);
}

/// A registration inside a branch is scoped to that branch's block, so its
/// cleanup runs when the block ends — before the statements after the `if`.
#[test]
fn native_move_capture_conditional_registration_frees_at_branch_exit() {
    let (out, code) = native(
        "fn main() -> Int { \
         let s = rt_str_alloc(3); \
         if true { defer move (s) { rt_print_str(\"freed\"); rt_str_free(s); } } \
         rt_print_str(\"body\"); \
         return 0; }",
    );
    assert_eq!(out, "freed\nbody\n");
    assert_eq!(code, 0);
}

#[test]
fn native_move_capture_break_only_registration_frees_once() {
    let (out, code) = native(
        "fn main() -> Int { \
         let s = rt_str_alloc(3); \
         loop { defer move (s) { rt_print_str(\"freed\"); rt_str_free(s); } break; } \
         rt_print_str(\"after\"); \
         return 0; }",
    );
    assert_eq!(out, "freed\nafter\n");
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// Session 269/270: aggregate and generics edge cases
// ---------------------------------------------------------------------------

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

#[test]
fn ownership_rejects_tuple_capture_with_untracked_reason() {
    let messages = ownership_messages(
        "fn main() -> Int { \
         let t = (rt_str_alloc(3), 1); \
         defer move (t) { rt_str_free(t.0); } \
         return 0; }",
    );
    assert_eq!(messages.len(), 1, "messages: {messages:?}");
    assert!(messages[0].contains("E-S19"), "messages: {messages:?}");
    assert!(
        messages[0].contains("tuple") && messages[0].contains("not ownership-tracked"),
        "messages: {messages:?}"
    );
}

#[test]
fn ownership_rejects_array_capture_with_whole_value_reason() {
    let messages = ownership_messages(
        "fn main() -> Int { \
         let a = [rt_str_alloc(3), rt_str_alloc(4)]; \
         defer move (a) { rt_str_free(a[0]); } \
         return 0; }",
    );
    assert_eq!(messages.len(), 1, "messages: {messages:?}");
    assert!(messages[0].contains("E-S19"), "messages: {messages:?}");
    assert!(
        messages[0].contains("array") && messages[0].contains("whole array"),
        "messages: {messages:?}"
    );
}

/// The generic *parameter* path is checked through the compiler driver: the
/// whole-program pipeline registers the type parameter, which the bare
/// `typecheck::check` entry point used above does not.
fn check_codes(src: &str) -> Vec<String> {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "mink_defer_move_check_{}_{}.mink",
        std::process::id(),
        n
    ));
    std::fs::write(&path, src).unwrap();
    let output = mink().arg("check").arg(&path).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let _ = std::fs::remove_file(&path);
    stderr
        .lines()
        .filter_map(|line| {
            line.find("error[E-").map(|at| {
                let rest = &line[at + "error[".len()..];
                rest[..rest.find(']').unwrap_or(rest.len())].to_string()
            })
        })
        .collect()
}

/// A generic-typed value can be captured and copied inside the cleanup, but
/// there is no generic free operation, so the cleanup can leak it — never
/// double free it.
#[test]
fn ownership_allows_generic_capture_copied_in_cleanup() {
    let codes = check_codes(
        "fn hold<T>(value: T) -> Int {            defer move (value) { let copy = value; rt_print_str(\"cleanup\"); }            return 0; }          fn main() -> Int { hold(1); hold(true); return 0; }",
    );
    assert!(codes.is_empty(), "unexpected diagnostics: {codes:?}");
}

#[test]
fn ownership_rejects_unused_generic_capture() {
    let codes = check_codes(
        "fn hold<T>(value: T) -> Int {            defer move (value) { rt_print_str(\"cleanup\"); }            return 0; }          fn main() -> Int { hold(1); return 0; }",
    );
    assert_eq!(codes, vec!["E-S19"]);
}

/// A closure inside the cleanup body may use the capture: the cleanup owns the
/// value, the closure runs inside it, and exactly one free remains.
#[test]
fn native_move_capture_closure_in_cleanup_frees_once() {
    let (out, code) = native(
        "fn main() -> Int { \
         let s = rt_str_alloc(3); \
         defer move (s) { \
           let release = | | rt_str_free(s); \
           rt_print_str(\"in-cleanup\"); \
           release(); \
         } \
         rt_print_str(\"body\"); \
         return 0; }",
    );
    assert_eq!(out, "body\nin-cleanup\n");
    assert_eq!(code, 0);
}
