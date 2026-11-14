//! M7 Phase C — Gate D: native codegen and ABI conformance corpus.
//!
//! The backend defines its own calling convention (see the module docs of
//! `src/backend/emit/x86_64.rs`): arguments are pushed on the stack, the
//! result is returned in `rax`, `rsp` is 16-byte aligned at every `call`, and
//! `rbp`/`rsp` are the only callee-saved registers. The only place the
//! platform ABI appears is the boundary to `kernel32` imports and the PE entry
//! stub.
//!
//! These cases execute real binaries, because the properties they check are
//! only observable at run time: a misaligned stack, a frame that skips the
//! guard page, or an argument slot computed from the wrong width corrupts the
//! process rather than producing a wrong diagnostic. Each case prints one
//! number, so a wrong slot or a clobbered register shows up as a wrong number.
//!
//! The one behaviour not asserted as passing is stack exhaustion: a recursive
//! program deeper than the 1 MiB stack reserve dies with the Windows
//! stack-overflow exception (`0xC00000FD`, exit code 253 in a shell). That is
//! native behaviour, documented in `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md`,
//! and deliberately not turned into a test that would encode an exit code as
//! a language guarantee.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Compiles and runs `source` as a program, returning its exit code and
/// stdout. The source is written to a fresh directory so the PE image lands
/// beside it.
fn run_program(source: &str) -> (i32, String) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir: PathBuf = std::env::temp_dir().join(format!("mink_abi_{}_{n}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("main.mink");
    std::fs::write(&path, source).expect("write temp program");

    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("run the mink CLI");
    let _ = std::fs::remove_dir_all(&dir);

    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

fn assert_prints(source: &str, expected: &str) {
    let (code, stdout) = run_program(source);
    assert_eq!(code, 0, "program failed; stdout:\n{stdout}");
    assert_eq!(
        stdout.trim(),
        expected,
        "program printed the wrong value; source:\n{source}"
    );
}

/// Builds an array literal of `n` copies of `value`.
fn literal_array(n: usize, value: &str) -> String {
    vec![value; n].join(", ")
}

// ======================================================================
// D-ARGS: argument passing and result return
// ======================================================================

#[test]
fn d_args_twelve_parameters_keep_their_slots() {
    // Every argument is a stack slot; reading the wrong offset would sum the
    // wrong values instead of crashing, so the expected sum is exact.
    assert_prints(
        "fn sum12(a: Int, b: Int, c: Int, d: Int, e: Int, f: Int, g: Int, h: Int, i: Int, j: Int, k: Int, l: Int) -> Int {\n\
             return a + b + c + d + e + f + g + h + i + j + k + l;\n\
         }\n\
         fn main() { rt_print_int(sum12(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12)); return 0; }\n",
        "78",
    );
}

#[test]
fn d_args_twenty_parameters_keep_their_slots() {
    // Past the point where any register-argument ABI would spill, the stack
    // offsets must still advance by one word per parameter.
    assert_prints(
        "fn first_last(a: Int, b: Int, c: Int, d: Int, e: Int, f: Int, g: Int, h: Int, i: Int, j: Int, k: Int, l: Int, m: Int, n: Int, o: Int, p: Int, q: Int, r: Int, s: Int, t: Int) -> Int {\n\
             return a * 100 + t;\n\
         }\n\
         fn main() { rt_print_int(first_last(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20)); return 0; }\n",
        "120",
    );
}

#[test]
fn d_args_mixed_types_do_not_shift_slots() {
    // Bool and Int are both one word, so the parameter after a Bool must not
    // be read one word early or late.
    assert_prints(
        "fn pick(flag: Bool, x: Int, y: Int) -> Int {\n\
             if flag { return x * 10 + y; }\n\
             return y * 10 + x;\n\
         }\n\
         fn main() { rt_print_int(pick(true, 3, 4) + pick(false, 3, 4)); return 0; }\n",
        "77",
    );
}

#[test]
fn d_args_float_round_trips_through_the_stack() {
    // A Float is one word in the internal convention; the bit pattern must
    // survive being pushed as an argument and returned in `rax`.
    assert_prints(
        "fn half(x: Float) -> Float { return x / 2.0; }\n\
         fn main() { rt_print_float(half(5.0)); return 0; }\n",
        "2.5",
    );
}

#[test]
fn d_args_recursion_returns_the_right_depth() {
    assert_prints(
        "fn down(n: Int) -> Int {\n\
             if n == 0 { return 0; }\n\
             return 1 + down(n - 1);\n\
         }\n\
         fn main() { rt_print_int(down(1000)); return 0; }\n",
        "1000",
    );
}

#[test]
fn d_args_aggregate_return_keeps_all_fields() {
    // Returning a struct by value must not be truncated to the first field.
    assert_prints(
        "struct P { a: Int, b: Int, c: Int }\n\
         fn make(a: Int, b: Int, c: Int) -> P { return P { a: a, b: b, c: c }; }\n\
         fn main() { let p = make(1, 2, 3); rt_print_int(p.a + p.b + p.c); return 0; }\n",
        "6",
    );
}

#[test]
fn d_args_aggregate_with_array_field_round_trips() {
    assert_prints(
        "struct B { id: Int, vals: [Int; 4] }\n\
         fn make(id: Int) -> B { return B { id: id, vals: [id, id + 1, id + 2, id + 3] }; }\n\
         fn main() {\n\
             let b = make(10);\n\
             rt_print_int(b.id + b.vals[0] + b.vals[3]);\n\
             return 0;\n\
         }\n",
        // id = 10, vals[0] = 10, vals[3] = 13.
        "33",
    );
}

// ======================================================================
// D-FRAME: stack frames, page probing, and alignment at calls
// ======================================================================

#[test]
fn d_frame_sixty_four_kilobytes_is_probed() {
    // Larger than one page: the prologue must touch each page so Windows
    // grows the stack instead of faulting.
    let source = format!(
        "fn main() {{\n\
             let mut buf: [Int; 8192] = [{}];\n\
             buf[0] = 41;\n\
             buf[8191] = 1;\n\
             rt_print_int(buf[0] + buf[8191]);\n\
             return 0;\n\
         }}\n",
        literal_array(8192, "0")
    );
    assert_prints(&source, "42");
}

#[test]
fn d_frame_one_hundred_sixty_kilobytes_in_a_callee_is_probed() {
    // The frame is in a non-entry function, so the probing must also hold for
    // a call deeper in the program (the entry stub's stack is not reused).
    let source = format!(
        "fn big() -> Int {{\n\
             let mut buf: [Int; 20000] = [{}];\n\
             buf[19999] = 7;\n\
             return buf[19999];\n\
         }}\n\
         fn main() {{ rt_print_int(big()); return 0; }}\n",
        literal_array(20000, "0")
    );
    assert_prints(&source, "7");
}

#[test]
fn d_frame_large_frame_with_defer_runs_both_paths() {
    // `defer` blocks run at scope exit; a large frame must not disturb the
    // saved state the block reads.
    let source = format!(
        "fn work() -> Int {{\n\
             let big: [Int; 2048] = [{}];\n\
             defer {{ rt_print_int(99); }}\n\
             return big[0] + big[2047];\n\
         }}\n\
         fn main() {{ rt_print_int(work()); return 0; }}\n",
        literal_array(2048, "1")
    );
    let (code, stdout) = run_program(&source);
    assert_eq!(code, 0, "stdout:\n{stdout}");
    assert_eq!(stdout.trim().lines().collect::<Vec<_>>(), vec!["99", "2"]);
}

#[test]
fn d_frame_many_calls_after_odd_argument_counts_keep_the_stack_aligned() {
    // Calling the runtime (a `kernel32` path) right after a call that pushed
    // an odd number of argument words is where a missing alignment fixup
    // shows up: the runtime's own calls then run misaligned.
    assert_prints(
        "fn three(a: Int, b: Int, c: Int) -> Int { return a + b + c; }\n\
         fn five(a: Int, b: Int, c: Int, d: Int, e: Int) -> Int { return a + b + c + d + e; }\n\
         fn main() {\n\
             let mut total = 0;\n\
             for i in 0..50 {\n\
                 total = total + three(i, 1, 2) + five(i, 1, 2, 3, 4);\n\
             }\n\
             rt_print_int(total);\n\
             return 0;\n\
         }\n",
        // Sum of (i + 3) + (i + 10) over i in 0..50.
        "3100",
    );
}

#[test]
fn d_frame_nested_calls_clobber_no_callee_saved_state() {
    // `rbp`/`rsp` are the only callee-saved registers; a callee clobbering
    // them would corrupt the caller's frame. Nesting keeps a live frame
    // across many calls.
    assert_prints(
        "fn inner(x: Int) -> Int { return x * 2; }\n\
         fn middle(x: Int) -> Int { return inner(x) + inner(x + 1); }\n\
         fn outer(x: Int) -> Int { return middle(x) + middle(x + 10) + inner(x); }\n\
         fn main() { rt_print_int(outer(5)); return 0; }\n",
        // middle(5) = 22, middle(15) = 62, inner(5) = 10.
        "94",
    );
}

#[test]
fn d_frame_recursion_with_large_frames_stays_consistent() {
    // Each level allocates a page-sized array; the deepest level's value must
    // survive unwinding the recursion.
    let source = format!(
        "fn down(n: Int) -> Int {{\n\
             let pad: [Int; 512] = [{}];\n\
             if n == 0 {{ return pad[0]; }}\n\
             return 1 + down(n - 1);\n\
         }}\n\
         fn main() {{ rt_print_int(down(100)); return 0; }}\n",
        literal_array(512, "0")
    );
    // 100 levels x 4 KiB stays inside the 1 MiB stack reserve; 200 levels
    // does not, and dies with the stack-overflow exception (see the module
    // docs). The reserve, not the compiler, is the limit.
    assert_prints(&source, "100");
}

// ======================================================================
// D-ENTRY: the PE entry stub
// ======================================================================

#[test]
fn d_entry_main_result_is_the_exit_code() {
    let (code, stdout) = run_program("fn main() { return 7; }\n");
    assert_eq!(code, 7, "stdout:\n{stdout}");
}

#[test]
fn d_entry_unit_main_exits_zero_after_output() {
    let (code, stdout) = run_program("fn main() { rt_print_int(2026); return; }\n");
    assert_eq!(code, 0, "stdout:\n{stdout}");
    assert_eq!(stdout.trim(), "2026");
}

#[test]
fn d_entry_bool_main_exits_one_or_zero() {
    // The documented entry rule: a boolean result becomes 0/1.
    assert_eq!(run_program("fn main() -> Bool { return true; }\n").0, 1);
    assert_eq!(run_program("fn main() -> Bool { return false; }\n").0, 0);
}
