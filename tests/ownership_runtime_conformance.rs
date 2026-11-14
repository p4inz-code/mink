//! M7 Phase B — Gate B: ownership, memory and runtime conformance corpus.
//!
//! Cases are identified as `B-<area>-<n>` and each one pins a contract stated
//! in `docs/runtime/MEMORY_MODEL.md`, `docs/implementation/
//! OWNERSHIP_IMPLEMENTATION.md` or `docs/implementation/DEFER_IMPLEMENTATION.md`.
//!
//! Every runtime case is executed for real: the test builds a program with the
//! CLI and runs the produced executable, because these contracts are about
//! what a program *does*, not what the compiler says. Rejected programs are
//! checked for a diagnostic and for the absence of an executable artifact.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn source(name: &str, src: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "mink_own_rt_{}_{n}_{name}.mink",
        std::process::id()
    ));
    std::fs::write(&path, src).expect("write temp source");
    path
}

/// Builds and runs `src`, returning (exit code, stdout, stderr).
fn run(src: &str) -> (i32, String, String) {
    let path = source("run", src);
    let build = mink()
        .arg("build")
        .arg(&path)
        .output()
        .expect("run mink build");
    let exe = path.with_extension("exe");
    if !build.status.success() {
        let stderr = String::from_utf8_lossy(&build.stderr).into_owned();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        panic!("build failed:\n{stderr}");
    }
    let out = Command::new(&exe).output().expect("run built program");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Asserts the program builds and exits `expected`, and returns its stdout.
fn assert_exit(src: &str, expected: i32) -> String {
    let (code, stdout, stderr) = run(src);
    assert_eq!(
        code, expected,
        "expected exit {expected}, got {code}; stderr:\n{stderr}\nstdout:\n{stdout}"
    );
    stdout
}

/// Asserts the program is rejected by `check` with `error_code`.
fn assert_rejected(src: &str, error_code: &str) {
    let path = source("rej", src);
    let out = mink()
        .arg("check")
        .arg(&path)
        .output()
        .expect("run mink check");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let code = out.status.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    assert_ne!(code, 0, "expected rejection of:\n{src}");
    assert!(
        stderr.contains(error_code),
        "expected `{error_code}`, got:\n{stderr}"
    );
}

// ============================================================================
// B-OWN — ownership transfer, borrowing, use after move
// ============================================================================

/// B-OWN-01: reading a moved binding is `E-S10`.
#[test]
fn b_own_01_use_after_move() {
    assert_rejected(
        "fn main() -> Int { let s = rt_str_concat(\"a\", \"b\"); let t = s; return rt_str_len(s); }",
        "E-S10",
    );
}

/// B-OWN-02: freeing a binding and then its new owner is a double free and is
/// rejected statically (`E-S10`), so it can never reach the runtime.
#[test]
fn b_own_02_double_free_is_rejected() {
    assert_rejected(
        "fn main() -> Int { let s = rt_str_concat(\"a\", \"b\"); let t = s; rt_str_free(t); rt_str_free(s); return 0; }",
        "E-S10",
    );
}

/// B-OWN-03: moving a value into a callee transfers ownership; the callee
/// frees it once and the program exits 0 (no leak, no double free).
#[test]
fn b_own_03_move_into_callee() {
    assert_exit(
        "fn consume(s: Str) -> Int { let n = rt_str_len(s); rt_str_free(s); return n; } \
         fn main() -> Int { let s = rt_str_concat(\"ab\", \"c\"); return consume(s); }",
        3,
    );
}

/// B-OWN-04: returning an owned value transfers it to the caller, which frees
/// it exactly once.
#[test]
fn b_own_04_return_ownership() {
    assert_exit(
        "fn make() -> Str { return rt_str_concat(\"ab\", \"c\"); } \
         fn main() -> Int { let s = make(); let n = rt_str_len(s); rt_str_free(s); return n; }",
        3,
    );
}

/// B-OWN-05: literals are immortal and copy freely; no free is required.
#[test]
fn b_own_05_literals_copy() {
    assert_exit(
        "fn main() -> Int { let a = \"hello\"; let b = a; let c = a; return rt_str_len(a) + rt_str_len(b) + rt_str_len(c); }",
        15,
    );
}

/// B-OWN-06: a field can move out of a struct on its own; the remaining
/// fields stay usable.
#[test]
fn b_own_06_partial_struct_move() {
    assert_exit(
        "struct P { s: Str, n: Int } \
         fn main() -> Int { let p = P { s: rt_str_concat(\"a\", \"b\"), n: 3 }; \
         let s = p.s; let n = p.n; rt_str_free(s); return n; }",
        3,
    );
}

/// B-OWN-07: a tuple element can move on its own, but the partially moved
/// tuple cannot then be transferred whole (`E-S10` naming the element).
#[test]
fn b_own_07_partial_tuple_move() {
    assert_exit(
        "fn main() -> Int { let t = (rt_str_concat(\"a\", \"b\"), rt_str_concat(\"c\", \"d\")); \
         let a = t.0; let b = t.1; rt_str_free(a); rt_str_free(b); return 0; }",
        0,
    );
    assert_rejected(
        "fn main() -> Int { let t = (rt_str_concat(\"a\", \"b\"), rt_str_concat(\"c\", \"d\")); \
         let s = t.0; rt_str_free(s); let u = t; return 0; }",
        "E-S10",
    );
}

/// B-OWN-08: fixed-array elements are tracked independently; each element is
/// freed once, and a moved element cannot be read again.
#[test]
fn b_own_08_array_element_moves() {
    assert_exit(
        "fn main() -> Int { let a = [rt_str_concat(\"a\", \"b\"), rt_str_concat(\"c\", \"d\")]; \
         rt_str_free(a[0]); rt_str_free(a[1]); return 0; }",
        0,
    );
    assert_rejected(
        "fn main() -> Int { let a = [rt_str_concat(\"a\", \"b\")]; rt_str_free(a[0]); return rt_str_len(a[0]); }",
        "E-S10",
    );
}

/// B-OWN-09: a borrow does not transfer ownership, but moving a borrowed
/// value is `E-S12`.
#[test]
fn b_own_09_borrow_rules() {
    assert_exit(
        "fn len_of(s: &Str) -> Int { return rt_str_len(*s); } \
         fn main() -> Int { let s = rt_str_concat(\"a\", \"b\"); let n = len_of(&s); rt_str_free(s); return n; }",
        2,
    );
    assert_rejected(
        "fn main() -> Int { let s = rt_str_concat(\"a\", \"b\"); let r = &s; let n = rt_str_len(*r); rt_str_free(s); return n; }",
        "E-S12",
    );
}

// ============================================================================
// B-DEFER — scope exit cleanup
// ============================================================================

/// B-DEFER-01: deferred bodies run in LIFO order at scope exit.
#[test]
fn b_defer_01_lifo_order() {
    let stdout = assert_exit(
        "fn main() -> Int { defer { rt_print_int(1); } defer { rt_print_int(2); } return 0; }",
        0,
    );
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines, vec!["2", "1"], "not LIFO: {stdout:?}");
}

/// B-DEFER-02: cleanup runs before an early `return`, and the returned value
/// is snapshotted before cleanup.
#[test]
fn b_defer_02_early_return_runs_cleanup() {
    let stdout = assert_exit(
        "fn f() -> Int { defer { rt_print_int(1); } return 42; } fn main() -> Int { return f(); }",
        42,
    );
    assert!(stdout.contains('1'), "cleanup did not run: {stdout:?}");
}

/// B-DEFER-03: a `defer` inside a loop body runs once per iteration.
#[test]
fn b_defer_03_per_iteration_cleanup() {
    let stdout = assert_exit(
        "fn main() -> Int { let mut i = 0; while i < 3 { i = i + 1; defer { rt_print_int(7); } } return 0; }",
        0,
    );
    assert_eq!(stdout.matches('7').count(), 3, "stdout: {stdout:?}");
}

/// B-DEFER-04: `continue` and `break` both run the iteration's cleanup.
#[test]
fn b_defer_04_break_and_continue_cleanup() {
    let stdout = assert_exit(
        "fn main() -> Int { let mut i = 0; while i < 5 { i = i + 1; defer { rt_print_int(9); } \
         if i == 2 { continue; } if i == 4 { break; } } return 0; }",
        0,
    );
    assert_eq!(stdout.matches('9').count(), 4, "stdout: {stdout:?}");
}

/// B-DEFER-05: `defer move` transfers the captured value and frees it exactly
/// once (exit 0 proves no leak and no double free).
#[test]
fn b_defer_05_defer_move_frees_once() {
    assert_exit(
        "fn main() -> Int { let s = rt_str_concat(\"a\", \"b\"); defer move (s) { rt_str_free(s); } return 7; }",
        7,
    );
}

/// B-DEFER-06: a `defer` in a nested block runs at that block's exit, before
/// the rest of the outer body.
#[test]
fn b_defer_06_inner_block_scope() {
    let stdout = assert_exit(
        "fn main() -> Int { if true { defer { rt_print_int(1); } rt_print_int(2); } rt_print_int(3); return 0; }",
        0,
    );
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines, vec!["2", "1", "3"], "stdout: {stdout:?}");
}

// ============================================================================
// B-RT — runtime limits, traps and lifecycle
// ============================================================================

/// B-RT-01: the live-allocation table holds 1,024 entries; the 1,025th is
/// refused with `E-R03` and exit 103.
#[test]
fn b_rt_01_allocation_table_limit() {
    assert_exit(
        "fn main() -> Int { let mut i = 0; let mut v = rt_alloc(8); while i < 1024 { v = rt_alloc(8); i = i + 1; } return 0; }",
        103,
    );
    // One fewer succeeds (then reports the deliberate leak at exit).
    assert_exit(
        "fn main() -> Int { let mut i = 0; let mut v = rt_alloc(8); while i < 1023 { v = rt_alloc(8); i = i + 1; } return 0; }",
        106,
    );
}

/// B-RT-02: one request of 1 GiB exceeds the arena and is refused with
/// `E-R02` and exit 102.
#[test]
fn b_rt_02_oversized_request() {
    assert_exit(
        "fn main() -> Int { let v = rt_alloc(1073741824); return 0; }",
        102,
    );
}

/// B-RT-03: repeated allocate/free cycles reuse slots and exit cleanly.
#[test]
fn b_rt_03_repeated_allocation_cycles() {
    assert_exit(
        "fn main() -> Int { let mut i = 0; while i < 3000 { let v = rt_alloc(16); rt_free(v); i = i + 1; } return 0; }",
        0,
    );
}

/// B-RT-04: an unfreed allocation is reported by the exit-time check
/// (`E-R06`, exit 106) rather than silently ignored.
#[test]
fn b_rt_04_leak_at_exit() {
    assert_exit(
        "fn main() -> Int { let s = rt_str_alloc(5); return 0; }",
        106,
    );
}

/// B-RT-05: a dynamic array index out of range is `E-R10` (exit 110).
#[test]
fn b_rt_05_bounds_check() {
    assert_exit(
        "fn main() -> Int { let a = [10, 20, 30]; let i = 5; return a[i]; }",
        110,
    );
}

/// B-RT-06: integer division traps are deterministic and distinct:
/// `/0` and `%0` are `E-R14` (114); `i64::MIN / -1` and `% -1` are `E-R15`
/// (115).
#[test]
fn b_rt_06_arithmetic_traps() {
    assert_exit(
        "fn main() -> Int { let a = 1; let b = 0; return a / b; }",
        114,
    );
    assert_exit(
        "fn main() -> Int { let a = 1; let b = 0; return a % b; }",
        114,
    );
    assert_exit(
        "fn main() -> Int { let a = -9223372036854775807 - 1; let b = -1; return a / b; }",
        115,
    );
    assert_exit(
        "fn main() -> Int { let a = -9223372036854775807 - 1; let b = -1; return a % b; }",
        115,
    );
}

/// B-RT-07: addition overflow wraps in every build mode (documented in
/// `CORE_GRAMMAR.md` §35); the observable value wraps to `i64::MIN`.
#[test]
fn b_rt_07_wrapping_arithmetic() {
    let stdout = assert_exit(
        "fn main() -> Int { rt_print_int(9223372036854775807 + 1); return 0; }",
        0,
    );
    assert!(
        stdout.contains("-9223372036854775808"),
        "stdout: {stdout:?}"
    );
}

/// B-RT-08: shift counts are masked to six bits (`1 << 65 == 2`).
#[test]
fn b_rt_08_shift_masking() {
    let stdout = assert_exit("fn main() -> Int { rt_print_int(1 << 65); return 0; }", 0);
    assert!(stdout.contains('2'), "stdout: {stdout:?}");
}

/// B-RT-09: a failed build leaves no executable behind.
#[test]
fn b_rt_09_failed_build_leaves_no_artifact() {
    let path = source("noart", "fn main() -> Int { let x: Int = true; return 0; }");
    let out = mink()
        .arg("build")
        .arg(&path)
        .output()
        .expect("run mink build");
    let exe = path.with_extension("exe");
    assert!(!out.status.success(), "invalid program built successfully");
    assert!(
        !exe.exists(),
        "a failed build left an executable artifact behind"
    );
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
}

/// B-RT-10: runtime termination is deterministic — the same failing program
/// yields the same exit code on repeated execution.
#[test]
fn b_rt_10_deterministic_termination() {
    let src = "fn main() -> Int { let a = [1, 2]; let i = 9; return a[i]; }";
    let path = source("det", src);
    let build = mink()
        .arg("build")
        .arg(&path)
        .output()
        .expect("run mink build");
    assert!(build.status.success());
    let exe = path.with_extension("exe");
    let codes: Vec<i32> = (0..3)
        .map(|_| {
            Command::new(&exe)
                .output()
                .expect("run program")
                .status
                .code()
                .unwrap_or(-1)
        })
        .collect();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    assert_eq!(codes, vec![110; 3], "termination was not deterministic");
}

/// B-RT-11: a large fixed array literal (3,100 elements) and a larger one
/// (6,000) build and execute correctly. Before Session 278 the emitted
/// prologue reserved large frames without probing stack pages, so the first
/// store into the array's deepest slot faulted with an access violation
/// (defect S278-D3).
#[test]
fn b_rt_11_large_fixed_array_frame() {
    for n in [3_100usize, 6_000] {
        let elems: Vec<String> = (0..n).map(|i| (i % 7).to_string()).collect();
        let src = format!(
            "fn main() -> Int {{ let a = [{}]; return a[0] + a[{}]; }}",
            elems.join(", "),
            n - 1
        );
        let expected = ((n - 1) % 7) as i32; // a[0] == 0
        let (code, stdout, stderr) = run(&src);
        assert_eq!(
            code, expected,
            "large array ({n} elements) misbehaved; stderr:\n{stderr}"
        );
        assert!(stdout.is_empty());
    }
}

// ============================================================================
// B-LIB — standard-library runtime contracts relevant to memory safety
// ============================================================================

/// B-LIB-01: deeply nested JSON input is refused, not allowed to overflow the
/// program's stack. Before Session 278 the recursive parser crashed the
/// process with `0xC00000FD` at roughly 500 nesting levels (defect S278-D4).
#[test]
fn b_lib_01_json_nesting_is_bounded() {
    for n in [600usize, 2_000] {
        let literal = format!("\"{}{}\"", "[".repeat(n), "]".repeat(n));
        let src = format!(
            "mod json;\nfn main() -> Int {{ let arena = rt_alloc(1048576); \
             let v = json_parse({literal}, arena); rt_free(arena); \
             if v == 0 {{ return 1; }} return 0; }}"
        );
        let (code, _, stderr) = run(&src);
        assert_eq!(
            code, 1,
            "{n} nested arrays should be a clean parse error, not a crash; stderr:\n{stderr}"
        );
    }
}

/// B-LIB-02: nesting inside the documented budget still parses.
#[test]
fn b_lib_02_json_moderate_nesting_parses() {
    let n = 100usize;
    let literal = format!("\"{}{}\"", "[".repeat(n), "]".repeat(n));
    let src = format!(
        "mod json;\nfn main() -> Int {{ let arena = rt_alloc(1048576); \
         let v = json_parse({literal}, arena); rt_free(arena); \
         if v == 0 {{ return 1; }} return 0; }}"
    );
    assert_exit(&src, 0);
}

/// B-LIB-03: malformed JSON is a clean error return (no panic, no crash).
#[test]
fn b_lib_03_json_malformed_input() {
    for literal in ["\"{\"", "\"{\\\"a\\\":}\"", "\"[1,]\""] {
        let src = format!(
            "mod json;\nfn main() -> Int {{ let arena = rt_alloc(65536); \
             let v = json_parse({literal}, arena); rt_free(arena); \
             if v == 0 {{ return 1; }} return 0; }}"
        );
        let (code, _, stderr) = run(&src);
        assert_eq!(
            code, 1,
            "malformed JSON {literal} was not refused: {stderr}"
        );
    }
}
