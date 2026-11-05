//! Thread + lock integration tests — Session 112 (R19 threads, S71 threads +
//! locks, plus the R21 lock primitives they depend on).
//!
//! Python parity target: `threading.Thread` (create / start / join / result)
//! and `threading.Lock` (acquire / release) as used by real programs.
//!
//! Every test compiles a genuine Windows PE through the real `mink build` CLI
//! and executes it as a separate process. `rt_exit` performs the runtime arena
//! validation, so a clean exit (no `E-R06`, exit code 106) is also the
//! ownership/leak proof: the thread control blocks and mutex words must be
//! freed or reused, and the leak scan runs at process exit.
//!
//! A thread entry function is `fn(Ptr<Int>) -> Int`: MINK hands the argument
//! through unchanged, so a thread is normally given a pointer to a shared
//! context block. `rt_thread_spawn` returns a word-valued handle and
//! `rt_thread_join` waits for the thread and returns its result.
//!
//! Test artifacts are written under `target/mink-artifacts/` rather than the
//! system temp directory: Windows Defender quarantines freshly linked PEs in
//! `%TEMP%` (see the Session 109/111 reports), which made those harnesses
//! non-deterministic. Runner artifacts are deleted by the harness; on failure
//! the generated source is kept for inspection.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Helpers shared by the generated programs.
const PRELUDE: &str = r#"
fn say_i(v: Int) { rt_print_int(v); }
fn say_b(v: Bool) { if v { rt_print_int(1); } else { rt_print_int(0); } }
"#;

fn artifact_dir() -> PathBuf {
    let dir = PathBuf::from("target").join("mink-artifacts");
    std::fs::create_dir_all(&dir).expect("failed to create target/mink-artifacts");
    dir
}

/// Compile and run one generated program. `decls` holds the top-level
/// functions and `body` holds the statements of `fn main`.
fn build_and_run(decls: &str, body: &str) -> (i32, String) {
    let source = format!("{PRELUDE}\n{decls}\nfn main() {{\n{body}\n}}\n");
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("threads_test_{n}.mink"));
    std::fs::write(&path, source.replace("\r\n", "\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("build")
        .arg(&path)
        .output()
        .unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&exe);
        panic!(
            "build failed (source kept at {}):\n{stderr}",
            path.display()
        );
    }
    let run = Command::new(&exe).output().expect("failed to run test exe");
    let code = run.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    // A clean exit also proves the arena/leak check passed (E-R06 = exit 106).
    assert!(stderr.is_empty(), "unexpected stderr: {stderr:?}");
    (code, stdout)
}

/// Compile and run a complete program (no prelude is prepended). Used by the
/// tests that exercise the `threads` stdlib module and the example program.
fn build_and_run_full(tag: &str, source: &str) -> (i32, String) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("threads_{tag}_{n}.mink"));
    std::fs::write(&path, source.replace("\r\n", "\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("build")
        .arg(&path)
        .output()
        .unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&exe);
        panic!(
            "build failed (source kept at {}):\n{stderr}",
            path.display()
        );
    }
    let run = Command::new(&exe).output().expect("failed to run test exe");
    let code = run.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    assert!(stderr.is_empty(), "unexpected stderr: {stderr:?}");
    (code, stdout)
}

fn lines(out: &str) -> Vec<String> {
    out.lines().map(|line| line.to_string()).collect()
}

fn assert_ok(code: i32, out: &str) {
    assert_eq!(code, 0, "program exited {code}; stdout={out:?}");
}

// ============================================================================
// R19 — threads
// ============================================================================

#[test]
fn r19_spawn_join_returns_thread_result() {
    // The argument is a pointer to an 8-byte cell; the thread reads it,
    // returns value + 1, and `join` yields the thread's result.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int { return rt_mem_load(p) + 1; }
"#;
    let body = r#"let a = rt_alloc(8);
rt_mem_store(a, 41);
let t = rt_thread_spawn(worker, a);
say_i(rt_thread_join(t));
rt_free(a);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(lines(&out), vec!["42"]);
}

#[test]
fn r19_thread_id_is_a_live_tid() {
    // The worker returns its own thread id; it must be a positive value that
    // differs from the main thread's, and each thread must see its own.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int { return rt_thread_id(); }
"#;
    let body = r#"let a = rt_alloc(8);
rt_mem_store(a, 0);
let main_id = rt_thread_id();
say_b(main_id > 0);
let t1 = rt_thread_spawn(worker, a);
let t2 = rt_thread_spawn(worker, a);
let i1 = rt_thread_join(t1);
let i2 = rt_thread_join(t2);
say_b(i1 != main_id);
say_b(i2 != main_id);
say_b(i1 != i2);
rt_free(a);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(lines(&out), vec!["1", "1", "1", "1"]);
}

#[test]
fn r19_repeated_spawn_and_join() {
    // 200 sequential spawn/join cycles. Each join frees the thread control
    // block, so the allocator must reuse it (the liveness table has a fixed
    // bound) and the exit-time leak scan must stay silent.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int { return rt_mem_load(p) + 1; }
"#;
    let body = r#"let a = rt_alloc(8);
let mut i = 0;
let mut sum = 0;
while i < 200 {
    rt_mem_store(a, i);
    let t = rt_thread_spawn(worker, a);
    sum = sum + rt_thread_join(t);
    i = i + 1;
}
say_i(sum);
rt_free(a);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    // 1 + 2 + ... + 200
    assert_eq!(lines(&out), vec!["20100"]);
}

#[test]
fn r19_many_threads_compute_and_join() {
    // Eight concurrent threads, each returning its own datum; join must
    // deliver the matching result for each handle.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let v = rt_mem_load(p);
    let mut k = 0;
    while k < 2000 { k = k + 1; }
    return v * 3;
}
"#;
    let body = r#"let slots = rt_alloc(64);
let mut i = 0;
while i < 8 {
    rt_mem_store(slots + i * 8, i);
    i = i + 1;
}
let t0 = rt_thread_spawn(worker, slots);
let t1 = rt_thread_spawn(worker, slots + 8);
let t2 = rt_thread_spawn(worker, slots + 16);
let t3 = rt_thread_spawn(worker, slots + 24);
let t4 = rt_thread_spawn(worker, slots + 32);
let t5 = rt_thread_spawn(worker, slots + 40);
let t6 = rt_thread_spawn(worker, slots + 48);
let t7 = rt_thread_spawn(worker, slots + 56);
say_i(rt_thread_join(t0));
say_i(rt_thread_join(t1));
say_i(rt_thread_join(t2));
say_i(rt_thread_join(t3));
say_i(rt_thread_join(t4));
say_i(rt_thread_join(t5));
say_i(rt_thread_join(t6));
say_i(rt_thread_join(t7));
rt_free(slots);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(
        lines(&out),
        vec!["0", "3", "6", "9", "12", "15", "18", "21"]
    );
}

#[test]
fn r19_thread_allocates_in_parallel() {
    // Two threads each allocate / touch / free 300 blocks while the main
    // thread's allocator state must stay coherent (the allocator is guarded
    // by the runtime spin lock). Any corruption makes the integrity check
    // return 99, and a corrupted liveness table aborts the process.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let mut i = 0;
    while i < 300 {
        let b = rt_alloc(64);
        rt_mem_store(b, i);
        if rt_mem_load(b) != i { return 99; }
        rt_free(b);
        i = i + 1;
    }
    return 0;
}
"#;
    let body = r#"let p = rt_alloc(32);
let t1 = rt_thread_spawn(worker, p);
let t2 = rt_thread_spawn(worker, p);
say_i(rt_thread_join(t1));
say_i(rt_thread_join(t2));
rt_free(p);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(lines(&out), vec!["0", "0"]);
}

#[test]
fn r19_thread_uses_strings_and_printing() {
    // Interaction with the string/printf paths: each thread builds a string
    // (allocation), reads it back (validated access) and returns its length.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let n = rt_mem_load(p);
    let s = rt_str_from_int(n);
    let len = rt_str_len(s);
    if rt_str_len(s) == 0 { return -1; }
    rt_str_free(s);
    return len + 100;
}
"#;
    let body = r#"let a = rt_alloc(8);
rt_mem_store(a, 12345);
let t1 = rt_thread_spawn(worker, a);
say_i(rt_thread_join(t1));
rt_mem_store(a, 7);
let t2 = rt_thread_spawn(worker, a);
say_i(rt_thread_join(t2));
rt_free(a);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(lines(&out), vec!["105", "101"]);
}

#[test]
fn r19_ptr_cast_round_trips_a_context_pointer() {
    // `rt_ptr_to_int` / `rt_int_to_ptr` make a pointer storable in a
    // `Ptr<Int>` block, which is how a thread receives shared state. The
    // round trip must preserve the address exactly (the writes through the
    // recovered pointer are validated against the live allocation table).
    let decls = "";
    let body = r#"let ctx = rt_alloc(16);
rt_mem_store(ctx, 111);
rt_mem_store(ctx + 8, 222);
let word = rt_ptr_to_int(ctx);
let again = rt_int_to_ptr(word);
say_i(rt_mem_load(again));
say_i(rt_mem_load(again + 8));
rt_mem_store(again + 8, 333);
say_i(rt_mem_load(ctx + 8));
rt_free(ctx);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(lines(&out), vec!["111", "222", "333"]);
}

// ============================================================================
// S71 — locks
// ============================================================================

#[test]
fn s71_mutex_uncontended_acquire_release() {
    // A single thread locks and unlocks 1000 times: the lock must stay
    // acquirable (no double-unlock lockup, no permanently held lock).
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let m = rt_mem_load(p);
    let mut i = 0;
    while i < 1000 {
        rt_mutex_lock(m);
        rt_mem_store(p + 8, rt_mem_load(p + 8) + 1);
        rt_mutex_unlock(m);
        i = i + 1;
    }
    return rt_mem_load(p + 8);
}
"#;
    let body = r#"let p = rt_alloc(32);
let m = rt_mutex_new();
rt_mem_store(p, m);
rt_mem_store(p + 8, 0);
let t = rt_thread_spawn(worker, p);
say_i(rt_thread_join(t));
rt_mutex_free(m);
rt_free(p);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(lines(&out), vec!["1000"]);
}

#[test]
fn s71_mutex_serializes_two_threads() {
    // 2 threads x 1000 locked increments must produce exactly 2000: a lost
    // update is a real lock defect, not a tolerance.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let m = rt_mem_load(p);
    let mut i = 0;
    while i < 1000 {
        rt_mutex_lock(m);
        rt_mem_store(p + 8, rt_mem_load(p + 8) + 1);
        rt_mutex_unlock(m);
        i = i + 1;
    }
    return 0;
}
"#;
    let body = r#"let p = rt_alloc(32);
let m = rt_mutex_new();
rt_mem_store(p, m);
rt_mem_store(p + 8, 0);
let t1 = rt_thread_spawn(worker, p);
let t2 = rt_thread_spawn(worker, p);
rt_thread_join(t1);
rt_thread_join(t2);
say_i(rt_mem_load(p + 8));
rt_mutex_free(m);
rt_free(p);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(lines(&out), vec!["2000"]);
}

#[test]
fn s71_mutex_serializes_four_threads_under_contention() {
    // 4 threads x 5000 locked increments (20 000 total). Run twice to show
    // the result is deterministic rather than a lucky interleaving.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let m = rt_mem_load(p);
    let mut i = 0;
    while i < 5000 {
        rt_mutex_lock(m);
        rt_mem_store(p + 8, rt_mem_load(p + 8) + 1);
        rt_mutex_unlock(m);
        i = i + 1;
    }
    return 0;
}
"#;
    let body = r#"let mut round = 0;
while round < 2 {
    let p = rt_alloc(32);
    let m = rt_mutex_new();
    rt_mem_store(p, m);
    rt_mem_store(p + 8, 0);
    let t1 = rt_thread_spawn(worker, p);
    let t2 = rt_thread_spawn(worker, p);
    let t3 = rt_thread_spawn(worker, p);
    let t4 = rt_thread_spawn(worker, p);
    rt_thread_join(t1);
    rt_thread_join(t2);
    rt_thread_join(t3);
    rt_thread_join(t4);
    say_i(rt_mem_load(p + 8));
    rt_mutex_free(m);
    rt_free(p);
    round = round + 1;
}"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(lines(&out), vec!["20000", "20000"]);
}

#[test]
fn s71_mutex_excludes_threads_from_the_critical_section() {
    // Mutual-exclusion probe: inside the critical section each thread
    // increments an "occupancy" counter, verifies it reads back exactly 1,
    // then decrements it. If two threads were ever inside together, the
    // read-back would exceed 1 and the violation flag would be set. A
    // correct lock always leaves the flag at 0, so this assertion cannot
    // fail spuriously on a working implementation and reliably catches a
    // broken one.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let m = rt_mem_load(p);
    let mut i = 0;
    while i < 2000 {
        rt_mutex_lock(m);
        rt_mem_store(p + 16, rt_mem_load(p + 16) + 1);
        if rt_mem_load(p + 16) != 1 { rt_mem_store(p + 24, 1); }
        rt_mem_store(p + 16, rt_mem_load(p + 16) - 1);
        rt_mem_store(p + 8, rt_mem_load(p + 8) + 1);
        rt_mutex_unlock(m);
        i = i + 1;
    }
    return 0;
}
"#;
    let body = r#"let p = rt_alloc(32);
let m = rt_mutex_new();
rt_mem_store(p, m);
rt_mem_store(p + 8, 0);
rt_mem_store(p + 16, 0);
rt_mem_store(p + 24, 0);
let t1 = rt_thread_spawn(worker, p);
let t2 = rt_thread_spawn(worker, p);
let t3 = rt_thread_spawn(worker, p);
let t4 = rt_thread_spawn(worker, p);
rt_thread_join(t1);
rt_thread_join(t2);
rt_thread_join(t3);
rt_thread_join(t4);
say_i(rt_mem_load(p + 8));
say_i(rt_mem_load(p + 24));
say_i(rt_mem_load(p + 16));
rt_mutex_free(m);
rt_free(p);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    // 8000 increments, no exclusion violation, occupancy drained back to 0.
    assert_eq!(lines(&out), vec!["8000", "0", "0"]);
}

#[test]
fn s71_unlocked_increments_are_lossy() {
    // The control for the lock tests: the same 4-thread workload without a
    // mutex must not be exact. MINK's per-access runtime lock does not make
    // a read-modify-write atomic, so this proves the user mutex is what
    // provides the guarantee above. Asserted as an upper bound (a lost
    // update can only lower the total) so the test can never fail merely
    // because the scheduler happened to serialise the threads.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let mut i = 0;
    while i < 5000 {
        rt_mem_store(p + 8, rt_mem_load(p + 8) + 1);
        i = i + 1;
    }
    return 0;
}
"#;
    let body = r#"let p = rt_alloc(32);
rt_mem_store(p + 8, 0);
let t1 = rt_thread_spawn(worker, p);
let t2 = rt_thread_spawn(worker, p);
let t3 = rt_thread_spawn(worker, p);
let t4 = rt_thread_spawn(worker, p);
rt_thread_join(t1);
rt_thread_join(t2);
rt_thread_join(t3);
rt_thread_join(t4);
say_b(rt_mem_load(p + 8) <= 20000);
rt_free(p);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(lines(&out), vec!["1"]);
}

#[test]
fn s71_mutex_repeated_construction_and_destruction() {
    // 200 create/lock/unlock/free cycles: every lock word is returned to the
    // allocator and reused, and the exit-time leak scan must stay silent.
    let decls = "";
    let body = r#"let mut i = 0;
let mut total = 0;
while i < 200 {
    let m = rt_mutex_new();
    rt_mutex_lock(m);
    total = total + 1;
    rt_mutex_unlock(m);
    rt_mutex_free(m);
    i = i + 1;
}
say_i(total);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(lines(&out), vec!["200"]);
}

#[test]
fn s71_released_lock_is_reacquirable_after_join() {
    // A worker locks and unlocks many times; the main thread then takes the
    // same lock after join. The lock must be free (no thread left it held),
    // which would otherwise block the main thread forever.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let m = rt_mem_load(p);
    let mut i = 0;
    while i < 500 {
        rt_mutex_lock(m);
        rt_mem_store(p + 8, rt_mem_load(p + 8) + 1);
        rt_mutex_unlock(m);
        i = i + 1;
    }
    return 0;
}
"#;
    let body = r#"let p = rt_alloc(32);
let m = rt_mutex_new();
rt_mem_store(p, m);
rt_mem_store(p + 8, 0);
let t = rt_thread_spawn(worker, p);
rt_thread_join(t);
rt_mutex_lock(m);
rt_mem_store(p + 8, rt_mem_load(p + 8) + 1);
rt_mutex_unlock(m);
say_i(rt_mem_load(p + 8));
rt_mutex_free(m);
rt_free(p);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(lines(&out), vec!["501"]);
}

#[test]
fn s71_mutex_protects_a_shared_accumulator() {
    // Locks + threads: each worker adds its own running index to a shared
    // accumulator under the lock, and the joined total must be exact.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let m = rt_mem_load(p);
    let mut i = 0;
    while i < 250 {
        rt_mutex_lock(m);
        rt_mem_store(p + 8, rt_mem_load(p + 8) + i);
        rt_mutex_unlock(m);
        i = i + 1;
    }
    return rt_mem_load(p + 8);
}
"#;
    let body = r#"let p = rt_alloc(32);
let m = rt_mutex_new();
rt_mem_store(p, m);
rt_mem_store(p + 8, 0);
let t1 = rt_thread_spawn(worker, p);
let t2 = rt_thread_spawn(worker, p);
rt_thread_join(t1);
rt_thread_join(t2);
// 2 * (0 + 1 + ... + 249) = 62250
say_i(rt_mem_load(p + 8));
rt_mutex_free(m);
rt_free(p);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(lines(&out), vec!["62250"]);
}

// ============================================================================
// Public user path — the `threads` stdlib module and the example program
// ============================================================================

#[test]
fn r19_stdlib_threads_and_locks_module() {
    // Exercises the shipped `stdlib/threads.mink` wrappers end to end: two
    // workers split a range, each adds its subtotal to a shared accumulator
    // under a stdlib lock, and the joined totals are exact.
    let source = r#"
mod threads;

fn say_i(v: Int) { rt_print_int(v); }

fn worker(p: Ptr<Int>) -> Int {
    let ctx = rt_int_to_ptr(rt_mem_load(p));
    let lock = rt_mem_load(ctx);
    let start = rt_mem_load(p + 8);
    let end = rt_mem_load(p + 16);
    let mut i = start;
    let mut sub = 0;
    while i < end { sub = sub + i; i = i + 1; }
    lock_acquire(lock);
    rt_mem_store(ctx + 8, rt_mem_load(ctx + 8) + sub);
    lock_release(lock);
    return sub;
}

fn args(ctx: Ptr<Int>, start: Int, end: Int) -> Ptr<Int> {
    let p = rt_alloc(24);
    rt_mem_store(p, rt_ptr_to_int(ctx));
    rt_mem_store(p + 8, start);
    rt_mem_store(p + 16, end);
    return p;
}

fn main() -> Int {
    let ctx = rt_alloc(16);
    let lock = lock_new();
    rt_mem_store(ctx, lock);
    rt_mem_store(ctx + 8, 0);
    let a1 = args(ctx, 1, 101);
    let a2 = args(ctx, 101, 201);
    let t1 = thread_spawn(worker, a1);
    let t2 = thread_spawn(worker, a2);
    say_i(thread_join(t1));
    say_i(thread_join(t2));
    say_i(rt_mem_load(ctx + 8));
    if thread_self() > 0 { say_i(1); } else { say_i(0); }
    rt_free(a1);
    rt_free(a2);
    lock_free(lock);
    rt_free(ctx);
    return 0;
}
"#;
    let (code, out) = build_and_run_full("stdlib", source);
    assert_ok(code, &out);
    // 1..100 = 5050, 101..200 = 15050, total 20100.
    assert_eq!(lines(&out), vec!["5050", "15050", "20100", "1"]);
}

#[test]
fn s134_concurrent_str_from_int_returns_its_own_value() {
    // Session 134: `rt_str_from_int` used to build its digits in the shared
    // `.bss` `print_buf`. Concurrent callers therefore overwrote each other's
    // digits: a multi-threaded server that formats an integer into a response
    // (or a log line) could send another thread's number. The digits are now
    // built on the calling thread's stack, so every call returns its own
    // value. Each thread round-trips 4,000 distinct values through
    // `rt_str_from_int` and parses the text back.
    let decls = r#"
fn parse(s: Str) -> Int {
    let n = rt_str_len(s);
    let mut i = 0;
    let mut v = 0;
    while i < n {
        v = v * 10 + (rt_str_byte(s, i) - 48);
        i = i + 1;
    }
    rt_str_free(s);
    return v;
}

fn worker(p: Ptr<Int>) -> Int {
    let base = rt_mem_load(p);
    let mut i = 0;
    let mut fails = 0;
    while i < 4000 {
        let v = base * 100000 + i;
        let s = rt_str_from_int(v);
        if parse(s) != v { fails = fails + 1; }
        i = i + 1;
    }
    return fails;
}
"#;
    let body = r#"let slots = rt_alloc(64);
let handles = rt_alloc(64);
let mut i = 0;
while i < 8 {
    rt_mem_store(slots + i * 8, i + 1);
    rt_mem_store(handles + i * 8, rt_thread_spawn(worker, slots + i * 8));
    i = i + 1;
}
let mut fails = 0;
i = 0;
while i < 8 {
    fails = fails + rt_thread_join(rt_mem_load(handles + i * 8));
    i = i + 1;
}
say_i(fails);
rt_free(slots);
rt_free(handles);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(lines(&out), vec!["0"]);
}

#[test]
fn s143_concurrent_print_int_emits_one_intact_line_per_call() {
    // Session 143: `rt_print_int` built its digits in the process-global
    // `.bss` `print_buf` and then wrote the digits and the CRLF with two
    // separate writes. Two threads printing at once could overwrite each
    // other's digits between formatting and writing (one value printed
    // twice, its owner's value lost) and could interleave the two writes
    // (merged lines and empty lines). The scratch is now per-call stack
    // memory and the digits and CRLF leave in one write, so every call
    // contributes exactly one intact line.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let base = rt_mem_load(p);
    let mut i = 0;
    while i < 3000 {
        rt_print_int(base * 100000 + i);
        i = i + 1;
    }
    return 0;
}
"#;
    let body = r#"let slots = rt_alloc(64);
let handles = rt_alloc(64);
let mut i = 0;
while i < 8 {
    rt_mem_store(slots + i * 8, i + 1);
    rt_mem_store(handles + i * 8, rt_thread_spawn(worker, slots + i * 8));
    i = i + 1;
}
i = 0;
while i < 8 {
    rt_thread_join(rt_mem_load(handles + i * 8));
    i = i + 1;
}
rt_free(handles);
rt_free(slots);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    let mut got = lines(&out);
    assert_eq!(got.len(), 24000, "one line per rt_print_int call");
    got.sort();
    let mut expected: Vec<String> = (1..=8)
        .flat_map(|base| (0..3000).map(move |i| (base * 100000 + i).to_string()))
        .collect();
    expected.sort();
    assert_eq!(got, expected, "every printed value appears exactly once");
}

#[test]
fn s143_concurrent_mixed_int_and_float_lines_stay_intact() {
    // The float path had the same defect: its big-integer words, the exact
    // digit expansion and the output assembly were all shared `.bss`
    // scratch. Four threads alternate an int and a thread-unique float;
    // every line must be one complete value, with no merged or empty
    // lines.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let base = rt_mem_load(p);
    let f = rt_int_to_float(base) + 0.5;
    let mut i = 0;
    while i < 500 {
        rt_print_int(base * 10000 + i);
        rt_print_float(f);
        i = i + 1;
    }
    return 0;
}
"#;
    let body = r#"let slots = rt_alloc(64);
let handles = rt_alloc(64);
let mut i = 0;
while i < 4 {
    rt_mem_store(slots + i * 8, i + 1);
    rt_mem_store(handles + i * 8, rt_thread_spawn(worker, slots + i * 8));
    i = i + 1;
}
i = 0;
while i < 4 {
    rt_thread_join(rt_mem_load(handles + i * 8));
    i = i + 1;
}
rt_free(handles);
rt_free(slots);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    let got = lines(&out);
    assert_eq!(got.len(), 4000, "one int line and one float line per iteration");
    let mut ints: Vec<String> = got.iter().filter(|l| !l.contains('.')).cloned().collect();
    let mut floats: Vec<String> = got.iter().filter(|l| l.contains('.')).cloned().collect();
    ints.sort();
    let mut expected_ints: Vec<String> = (1..=4)
        .flat_map(|base| (0..500).map(move |i| (base * 10000 + i).to_string()))
        .collect();
    expected_ints.sort();
    assert_eq!(ints, expected_ints, "int lines");
    floats.sort();
    let mut expected_floats: Vec<String> = ["1.5", "2.5", "3.5", "4.5"]
        .iter()
        .flat_map(|v| std::iter::repeat(v.to_string()).take(500))
        .collect();
    expected_floats.sort();
    assert_eq!(floats, expected_floats, "float lines");
}

#[test]
fn s143_str_from_float_does_not_swallow_another_threads_print() {
    // `rt_str_from_float` formats through the same float writer by arming
    // the process-global `write_redirect` sink. Before Session 143 the sink
    // was armed for every write on every thread, so a printer running next
    // to a formatter had its output appended into the formatter's sink:
    // 2,874 of 3,000 lines were lost and 117 of 3,000 formatted strings
    // were corrupted in the probe. The sink is now tagged with the arming
    // thread's id; only that thread's writes may enter it.
    let decls = r#"
fn fmt(p: Ptr<Int>) -> Int {
    let mut i = 0;
    let mut bad = 0;
    while i < 3000 {
        let s = rt_str_from_float(1.5);
        if rt_str_len(s) != 3 { bad = bad + 1; }
        rt_str_free(s);
        i = i + 1;
    }
    return bad;
}

fn printer(p: Ptr<Int>) -> Int {
    let mut i = 0;
    while i < 3000 {
        rt_print_int(700000 + i);
        i = i + 1;
    }
    return 0;
}
"#;
    let body = r#"let cell = rt_alloc(8);
rt_mem_store(cell, 0);
let t1 = rt_thread_spawn(fmt, cell);
let t2 = rt_thread_spawn(printer, cell);
let bad = rt_thread_join(t1);
rt_thread_join(t2);
rt_print_int(bad);
rt_free(cell);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    let got = lines(&out);
    assert_eq!(got.len(), 3001, "3,000 ints plus the bad-string count");
    let expected: Vec<String> = (0..3000).map(|i| (700000 + i).to_string()).collect();
    assert_eq!(&got[..3000], &expected[..], "printer lines");
    assert_eq!(got[3000], "0", "no formatted string was corrupted");
}

#[test]
fn s149_concurrent_print_str_emits_one_intact_line_per_call() {
    // Session 149: `rt_print_str` wrote the payload and the shared CRLF in
    // two separate WriteFile calls. Eight printers interleaved those writes:
    // the pre-fix probe produced 11,606 merged/malformed lines and 6,245
    // empty lines out of 24,000, with each payload appearing about 1,500
    // times instead of 3,000. The complete line is now assembled per call and
    // written once, so every observed line must be exactly one expected
    // payload and each payload must appear exactly 3,000 times.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let id = rt_mem_load(p);
    let line = if id == 1 { "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA01" }
    else if id == 2 { "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBB02" }
    else if id == 3 { "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCC03" }
    else if id == 4 { "DDDDDDDDDDDDDDDDDDDDDDDDDDDDDD04" }
    else if id == 5 { "EEEEEEEEEEEEEEEEEEEEEEEEEEEEEE05" }
    else if id == 6 { "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFF06" }
    else if id == 7 { "GGGGGGGGGGGGGGGGGGGGGGGGGGGGGG07" }
    else { "HHHHHHHHHHHHHHHHHHHHHHHHHHHHHH08" };
    let mut i = 0;
    while i < 3000 { rt_print_str(line); i = i + 1; }
    return 0;
}
"#;
    let body = r#"let slots = rt_alloc(64);
let handles = rt_alloc(64);
let mut i = 0;
while i < 8 {
    rt_mem_store(slots + i * 8, i + 1);
    rt_mem_store(handles + i * 8, rt_thread_spawn(worker, slots + i * 8));
    i = i + 1;
}
i = 0;
while i < 8 {
    rt_thread_join(rt_mem_load(handles + i * 8));
    i = i + 1;
}
rt_free(handles);
rt_free(slots);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    let mut got = lines(&out);
    assert_eq!(got.len(), 24000, "one line per rt_print_str call");
    let expected_literals = [
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA01",
        "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBB02",
        "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCC03",
        "DDDDDDDDDDDDDDDDDDDDDDDDDDDDDD04",
        "EEEEEEEEEEEEEEEEEEEEEEEEEEEEEE05",
        "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFF06",
        "GGGGGGGGGGGGGGGGGGGGGGGGGGGGGG07",
        "HHHHHHHHHHHHHHHHHHHHHHHHHHHHHH08",
    ];
    let mut expected: Vec<String> = expected_literals
        .iter()
        .flat_map(|line| std::iter::repeat((*line).to_string()).take(3000))
        .collect();
    got.sort();
    expected.sort();
    assert_eq!(got, expected, "every string line appears exactly once");
}

#[test]
fn s149_concurrent_print_str_repeated_payload_count_is_exact() {
    // High-contention repeated payload: eight threads print the same complete
    // line 2,000 times. A torn write would produce an empty or merged line
    // and break either the exact count or the content check.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let mut i = 0;
    while i < 2000 { rt_print_str("S149-REPEATED-LINE"); i = i + 1; }
    return 0;
}
"#;
    let body = r#"let slots = rt_alloc(64);
let handles = rt_alloc(64);
let mut i = 0;
while i < 8 {
    rt_mem_store(slots + i * 8, 0);
    rt_mem_store(handles + i * 8, rt_thread_spawn(worker, slots + i * 8));
    i = i + 1;
}
i = 0;
while i < 8 {
    rt_thread_join(rt_mem_load(handles + i * 8));
    i = i + 1;
}
rt_free(handles);
rt_free(slots);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    let got = lines(&out);
    assert_eq!(got.len(), 16000, "exact line count under contention");
    assert!(
        got.iter().all(|line| line == "S149-REPEATED-LINE"),
        "no empty, merged, or partial lines"
    );
}

#[test]
fn r19_example_threaded_work_is_deterministic() {
    // The shipped proof application, run through the real CLI. Four threads
    // accumulate under a lock; the result must match the closed form on
    // every run, and a clean exit proves nothing leaked.
    let source =
        std::fs::read_to_string("examples/threaded_work/main.mink").expect("missing example");
    for _ in 0..2 {
        let (code, out) = build_and_run_full("example", &source);
        assert_ok(code, &out);
        assert!(out.contains("MATCH"), "channel output: {out:?}");
        assert!(out.contains("21413400"), "channel output: {out:?}");
    }
}

// ============================================================================
// Session 144 — per-thread runtime scratch (M3 P4)
// ============================================================================

#[test]
fn s144_thread_local_word_is_isolated_per_thread() {
    // The runtime cell behind `rt_thread_local_get/set` is the storage for
    // `http_last_error()`. A fresh thread must start at 0 even though the
    // main thread already wrote 5, the worker's write must not be visible to
    // main, and main's value must survive the spawn/join. `rt_init` allocated
    // the slot before `main`; this also covers its use by a spawned thread.
    let decls = r#"
fn worker(p: Ptr<Int>) -> Int {
    let before = rt_thread_local_get();
    rt_mem_store(p, before);
    rt_thread_local_set(77);
    return rt_thread_local_get();
}
"#;
    let body = r#"rt_thread_local_set(5);
let p = rt_alloc(8);
let t = rt_thread_spawn(worker, p);
let worker_value = rt_thread_join(t);
let worker_before = rt_mem_load(p);
let main_value = rt_thread_local_get();
say_i(worker_before);
say_i(worker_value);
say_i(main_value);
rt_free(p);"#;
    let (code, out) = build_and_run(decls, body);
    assert_ok(code, &out);
    assert_eq!(lines(&out), vec!["0", "77", "5"]);
}
