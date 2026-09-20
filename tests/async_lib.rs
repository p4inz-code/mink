//! Async / task integration tests — Session 114 (R20 async / event loop,
//! S73 async/await + event loop).
//!
//! Python parity target: `async def` + `await` and an event loop that runs
//! coroutines concurrently (`asyncio.create_task` / `asyncio.gather`).
//!
//! Every test compiles a genuine Windows PE through the real `mink build` CLI
//! and executes it as a separate process. `rt_exit` performs the runtime arena
//! validation, so a clean exit (no `E-R06`, exit code 106) is also the
//! ownership/leak proof: every task control block and every kernel handle must
//! be released, and the leak scan runs at process exit.
//!
//! MINK's model: a task is `fn(arg: Int) -> Int` running on its own OS thread;
//! `async fn name(args) -> T { body }` declares the task body plus an ordinary
//! function that starts the task and returns its handle; `await handle` waits
//! for it and yields the task's result word.
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

/// Helpers shared by the generated programs. The `tasks` module import gives
/// the generated programs the shipped stdlib loop helpers (`task_pending`,
/// `task_run`, `task_await`, `task_stop`), which is the public surface a user
/// program reaches for.
const PRELUDE: &str = r#"
mod tasks;

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
    run_source("async", &source)
}

/// Compile and run a complete program (no prelude is prepended).
fn run_source(tag: &str, source: &str) -> (i32, String) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("async_{tag}_{n}.mink"));
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
    (code, format!("{stdout}\u{1}{stderr}"))
}

/// The stdout half of a `run_source` result.
fn out(result: &str) -> Vec<String> {
    result
        .split('\u{1}')
        .next()
        .unwrap_or("")
        .lines()
        .map(|line| line.to_string())
        .collect()
}

/// The stderr half of a `run_source` result.
fn err(result: &str) -> String {
    result.split('\u{1}').nth(1).unwrap_or("").to_string()
}

fn assert_ok(code: i32, result: &str) {
    assert_eq!(
        code,
        0,
        "program exited {code}; stdout={:?} stderr={:?}",
        out(result),
        err(result)
    );
    assert!(
        err(result).is_empty(),
        "unexpected stderr: {:?}",
        err(result)
    );
}

// ---------------------------------------------------------------------------
// R20: task lifecycle — create, run, collect, shutdown
// ---------------------------------------------------------------------------

#[test]
fn r20_spawn_await_result_and_handle_accounting() {
    // One task: the handle is visible while it is outstanding, the awaited
    // result is the task function's return value, and the loop is idle again
    // once the handle has been collected.
    let (code, result) = build_and_run(
        "async fn worker(x: Int) -> Int { return x + 1; }",
        "let t = worker(41);\nsay_i(task_pending());\nsay_i(await t);\nsay_i(task_pending());",
    );
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["1", "42", "0"]);
}

#[test]
fn r20_task_run_drains_the_loop() {
    // `task_run` collects every outstanding task and frees each control block;
    // a clean exit proves nothing was left live.
    let (code, result) = build_and_run(
        "fn worker(x: Int) -> Int { return x * 3; }",
        "let mut i = 0;\nwhile i < 4 {\n    rt_task_spawn(worker, i);\n    i = i + 1;\n}\nsay_i(rt_task_pending());\nsay_i(rt_task_run());\nsay_i(rt_task_pending());",
    );
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["4", "4", "0"]);
}

#[test]
fn r20_zero_tasks_is_a_no_op() {
    // Empty/minimum case: running the loop with nothing outstanding collects
    // nothing and does not block.
    let (code, result) = build_and_run("", "say_i(task_pending());\nsay_i(rt_task_run());");
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["0", "0"]);
}

#[test]
fn r20_repeated_spawn_and_await_cycles() {
    // 200 create/collect cycles: every handle is freed, every kernel handle is
    // closed, and the loop count returns to zero each time.
    let (code, result) = build_and_run(
        "fn worker(x: Int) -> Int { return x + 7; }\nfn tick(x: Int) -> Int { return 1; }",
        "let mut i = 0;\nlet mut sum = 0;\nwhile i < 200 {\n    let t = rt_task_spawn(tick, i);\n    sum = sum + await t;\n    i = i + 1;\n}\nsay_i(sum);\nsay_i(task_pending());",
    );
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["200", "0"]);
}

#[test]
fn r20_many_concurrent_tasks() {
    // 64 tasks live at once — the stress case for concurrent allocation,
    // thread creation and collection.
    let (code, result) = build_and_run(
        "fn worker(x: Int) -> Int { return x + 1; }",
        "let mut i = 0;\nwhile i < 64 {\n    rt_task_spawn(worker, i);\n    i = i + 1;\n}\nsay_i(rt_task_pending());\nsay_i(rt_task_run());\nsay_i(task_pending());",
    );
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["64", "64", "0"]);
}

#[test]
fn r20_task_stop_is_honored_by_run() {
    // `task_stop` makes `task_run` return without draining: the task is then
    // still outstanding, so the program collects it with `await` (a program
    // that did not would be reported as a leak rather than leaking silently).
    let (code, result) = build_and_run(
        "fn worker(x: Int) -> Int { return x; }",
        "let t = rt_task_spawn(worker, 5);\nrt_task_stop();\nsay_i(rt_task_run());\nsay_i(task_pending());\nsay_i(await t);\nsay_i(task_pending());",
    );
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["0", "1", "5", "0"]);
}

#[test]
fn r20_uncollected_task_is_a_leak() {
    // A spawn without a collection is the async counterpart of a thread spawn
    // without a join: the exit-time leak scan reports it (E-R06, exit 106)
    // instead of letting it pass silently.
    let (code, result) = build_and_run(
        "fn worker(x: Int) -> Int { return x; }",
        "rt_task_spawn(worker, 1);\nrt_sleep(50);",
    );
    assert_eq!(code, 106, "expected E-R06; stdout={:?}", out(&result));
    assert!(err(&result).contains("E-R06"), "stderr={:?}", err(&result));
}

#[test]
fn r20_double_collect_is_a_stable_error() {
    // Collecting the same handle twice is a stable runtime error, not a
    // use-after-free: the first await frees the control block, and the second
    // await fails the runtime's liveness validation on that word (E-R05)
    // before any dangling read.
    let (code, result) = build_and_run(
        "fn worker(x: Int) -> Int { return x * 2; }",
        "let t = rt_task_spawn(worker, 3);\nsay_i(await t);\nsay_i(await t);",
    );
    assert_eq!(code, 105, "expected E-R05; stdout={:?}", out(&result));
    assert!(err(&result).contains("E-R05"), "stderr={:?}", err(&result));
}

#[test]
fn r20_await_of_a_non_task_allocation_is_rejected() {
    // A live allocation that is not an outstanding task is `E-R13` (the loop
    // list is what makes a word a task handle), again without a fault.
    let (code, result) = build_and_run(
        "",
        "let block = rt_alloc(48);
let bogus = rt_ptr_to_int(block);
say_i(await bogus);",
    );
    assert_eq!(code, 113, "expected E-R13; stdout={:?}", out(&result));
    assert!(err(&result).contains("E-R13"), "stderr={:?}", err(&result));
}

// ---------------------------------------------------------------------------
// R20: concurrency behaviour — real parallelism and safe sharing
// ---------------------------------------------------------------------------

#[test]
fn r20_tasks_run_concurrently_with_the_spawner() {
    // The spawner's own work proceeds while the task runs: the task sleeps and
    // the spawner prints between the spawn and the await, so a synchronous
    // stand-in would order the output differently.
    let (code, result) = build_and_run(
        "fn slow(x: Int) -> Int { rt_sleep(120); say_i(100 + x); return x; }",
        "let t = rt_task_spawn(slow, 1);\nsay_i(7);\nsay_i(await t);",
    );
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["7", "101", "1"]);
}

#[test]
fn r20_four_tasks_share_state_under_a_lock() {
    // Four tasks, 1000 locked increments each: the locked total is exact, so
    // concurrent collection/allocation does not corrupt shared memory and the
    // lock really excludes.
    let (code, result) = build_and_run(
        "fn worker(arg: Int) -> Int {\n    let p = rt_int_to_ptr(arg);\n    let m = rt_mem_load(p);\n    let mut i = 0;\n    while i < 1000 {\n        rt_mutex_lock(m);\n        rt_mem_store(p + 8, rt_mem_load(p + 8) + 1);\n        rt_mutex_unlock(m);\n        i = i + 1;\n    }\n    return 0;\n}",
        "let p = rt_alloc(16);\nrt_mem_store(p, rt_mutex_new());\nrt_mem_store(p + 8, 0);\nlet mut i = 0;\nwhile i < 4 {\n    rt_task_spawn(worker, rt_ptr_to_int(p));\n    i = i + 1;\n}\nsay_i(rt_task_run());\nsay_i(rt_mem_load(p + 8));\nlet m = rt_mem_load(p);\nrt_mutex_free(m);\nrt_free(p);",
    );
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["4", "4000"]);
}

#[test]
fn r20_tasks_allocate_strings_safely() {
    // Tasks interact with the shared arena: each task builds and frees a
    // string, and the awaited results are exact.
    let (code, result) = build_and_run(
        "async fn label(x: Int) -> Int {\n    let s = rt_str_from_int(x);\n    let n = rt_str_len(s);\n    rt_str_free(s);\n    return n;\n}\nfn sum_digits(x: Int) -> Int {\n    let s = rt_str_from_int(x);\n    let n = rt_str_len(s);\n    rt_str_free(s);\n    return n;\n}",
        "let mut i = 0;\nlet mut digits = 0;\nwhile i < 100 {\n    let t = rt_task_spawn(sum_digits, i);\n    digits = digits + await t;\n    i = i + 1;\n}\nsay_i(digits);\nsay_i(task_pending());",
    );
    assert_ok(code, &result);
    // 0..9 -> 1 digit each (10), 10..99 -> 2 digits each (180).
    assert_eq!(out(&result), vec!["190", "0"]);
}

#[test]
fn r20_repeated_stress_runs_are_deterministic() {
    // The same multi-task workload, run five times, must produce identical
    // output every time (no scheduling-dependent results).
    let decls = "fn worker(arg: Int) -> Int {\n    let p = rt_int_to_ptr(arg);\n    let m = rt_mem_load(p);\n    let mut i = 0;\n    while i < 500 {\n        rt_mutex_lock(m);\n        rt_mem_store(p + 8, rt_mem_load(p + 8) + rt_mem_load(p + 16));\n        rt_mutex_unlock(m);\n        i = i + 1;\n    }\n    return rt_mem_load(p + 8);\n}";
    let body = "let p = rt_alloc(24);\nrt_mem_store(p, rt_mutex_new());\nrt_mem_store(p + 8, 0);\nrt_mem_store(p + 16, 3);\nlet mut i = 0;\nwhile i < 8 {\n    rt_task_spawn(worker, rt_ptr_to_int(p));\n    i = i + 1;\n}\nsay_i(rt_task_run());\nsay_i(rt_mem_load(p + 8));\nlet m = rt_mem_load(p);\nrt_mutex_free(m);\nrt_free(p);";
    let mut seen: Option<Vec<String>> = None;
    for _ in 0..5 {
        let (code, result) = build_and_run(decls, body);
        assert_ok(code, &result);
        let lines = out(&result);
        assert_eq!(lines, vec!["8", "12000"]);
        match &seen {
            Some(previous) => assert_eq!(previous, &lines, "non-deterministic output"),
            None => seen = Some(lines),
        }
    }
}

// ---------------------------------------------------------------------------
// S73: the language surface — `async fn` + `await`
// ---------------------------------------------------------------------------

#[test]
fn s73_await_direct_call_and_through_a_handle() {
    // `await f(x)` and `let h = f(x); await h` are the same task: the handle
    // form exposes the outstanding count in between.
    let (code, result) = build_and_run(
        "async fn double(x: Int) -> Int { return x * 2; }",
        "say_i(await double(21));\nlet h = double(4);\nsay_i(task_pending());\nsay_i(await h);",
    );
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["42", "1", "8"]);
}

#[test]
fn s73_zero_parameter_async_fn() {
    // A zero-parameter task: the handle producer contributes the unused word
    // itself, so no argument is passed and the result is still exact.
    let (code, result) = build_and_run(
        "async fn answer() -> Int { return 42; }",
        "let h = answer();\nsay_i(await h);\nsay_i(await answer());",
    );
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["42", "42"]);
}

#[test]
fn s73_multiple_async_fns_run_together() {
    // Several distinct async functions outstanding at once, awaited out of
    // spawn order: each result belongs to its own task.
    let (code, result) = build_and_run(
        "async fn square(x: Int) -> Int { return x * x; }\nasync fn plus(x: Int) -> Int { return x + 100; }\nasync fn neg(x: Int) -> Int { return 0 - x; }",
        "let a = square(3);\nlet b = plus(3);\nlet c = neg(3);\nsay_i(rt_task_pending());\nsay_i(await c);\nsay_i(await a);\nsay_i(await b);\nsay_i(rt_task_pending());",
    );
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["3", "-3", "9", "103", "0"]);
}

#[test]
fn s73_nested_async_tasks() {
    // A task spawns and awaits a child task from its own thread: collection is
    // safe from two threads because the loop list and the count are guarded.
    let (code, result) = build_and_run(
        "async fn inner(x: Int) -> Int { return x + 1; }\nasync fn outer(x: Int) -> Int {\n    let t = inner(x);\n    return x + await t;\n}",
        "say_i(await outer(30));\nsay_i(task_pending());",
    );
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["61", "0"]);
}

#[test]
fn s73_await_inside_control_flow() {
    // Awaits inside a loop and inside an if branch: the desugaring composes
    // with ordinary control flow and the ownership checker's scopes.
    let (code, result) = build_and_run(
        "async fn inc(x: Int) -> Int { return x + 1; }",
        "let mut i = 0;\nlet mut total = 0;\nwhile i < 10 {\n    total = total + await inc(i);\n    i = i + 1;\n}\nif total == 55 {\n    say_i(1);\n} else {\n    say_i(0);\n}\nlet mut j = 0;\nwhile j < 20 {\n    let t = inc(j);\n    total = total + await t;\n    j = j + 1;\n}\nsay_i(total);\nsay_i(task_pending());",
    );
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["1", "265", "0"]);
}

#[test]
fn s73_async_across_module_boundaries() {
    // An async fn declared in an imported module spawns, runs and returns
    // correctly; the generated task body and handle producer live in that
    // module's own item list.
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = artifact_dir();
    let module = dir.join(format!("async_mod_{n}.mink"));
    std::fs::write(
        &module,
        "pub async fn scale(x: Int) -> Int {\n    return x * 10;\n}\n\nasync fn offset(x: Int) -> Int {\n    return x + 1;\n}\n",
    )
    .unwrap();
    let source = format!(
        "mod async_mod_{n};\n\nfn main() {{\n    rt_print_int(await scale(4));\n    rt_print_int(scale_handle());\n    rt_print_int(await offset(9));\n}}\n\nfn scale_handle() -> Int {{\n    let h = scale(5);\n    return await h;\n}}\n"
    );
    let (code, result) = run_source("s73_module", &source);
    // The generated module file is removed by the harness on success.
    let _ = std::fs::remove_file(&module);
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["40", "50", "10"]);
}

#[test]
fn s73_async_fn_rejects_two_parameters() {
    // The task ABI is one word in and one word out, so the language rejects a
    // two-parameter async fn at parse time with a clear diagnostic instead of
    // compiling something that cannot work.
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("async_bad_{n}.mink"));
    std::fs::write(
        &path,
        "async fn two(a: Int, b: Int) -> Int { return a + b; }\nfn main() { }\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("check")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(!output.status.success(), "two-parameter async fn accepted");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-P31"), "stderr={stderr}");
}

#[test]
fn s73_bare_async_without_fn_is_rejected() {
    // `async` must introduce an `fn`; anything else is a parse error.
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("async_bad2_{n}.mink"));
    std::fs::write(&path, "async struct S {}\nfn main() { }\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("check")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(!output.status.success(), "`async struct` accepted");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-P30"), "stderr={stderr}");
}

// ---------------------------------------------------------------------------
// stdlib module and the shipped example
// ---------------------------------------------------------------------------

#[test]
fn r20_stdlib_tasks_module() {
    // The shipped `stdlib/tasks.mink` wrappers drive the same loop: spawn via
    // the runtime primitive, then collect with the module's helpers.
    let source = r#"
mod tasks;

fn worker(x: Int) -> Int { return x + 1; }

fn main() {
    let mut i = 0;
    while i < 3 {
        rt_task_spawn(worker, i);
        i = i + 1;
    }
    rt_print_int(task_pending());
    rt_print_int(task_run());
    let h = rt_task_spawn(worker, 5);
    rt_print_int(task_await(h));
    rt_print_int(task_pending());
}
"#;
    let (code, result) = run_source("s73_stdlib", source);
    assert_ok(code, &result);
    assert_eq!(out(&result), vec!["3", "3", "6", "0"]);
}

#[test]
fn r20_shipped_example_runs_repeatably() {
    // The real application proof: `examples/async_tasks/main.mink` runs four
    // async workers over disjoint slices of 0..1000, sums them under a lock
    // and checks the closed form. Two runs must print identical output.
    let example = PathBuf::from("examples")
        .join("async_tasks")
        .join("main.mink");
    assert!(example.exists(), "missing {}", example.display());

    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let copy = artifact_dir().join(format!("async_example_{n}.mink"));
    let source = std::fs::read_to_string(&example).unwrap();
    std::fs::write(&copy, source.replace("\r\n", "\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("build")
        .arg(&copy)
        .output()
        .unwrap();
    let exe = copy.with_extension("exe");
    assert!(
        output.status.success(),
        "example build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut previous: Option<String> = None;
    for _ in 0..2 {
        let run = Command::new(&exe).output().expect("failed to run example");
        let code = run.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&run.stdout).to_string();
        let stderr = String::from_utf8_lossy(&run.stderr).to_string();
        assert_eq!(code, 0, "example exited {code}; stderr={stderr:?}");
        assert!(stderr.is_empty(), "example stderr: {stderr:?}");
        let lines: Vec<String> = stdout.lines().map(|l| l.to_string()).collect();
        assert_eq!(
            lines,
            vec!["31125", "93625", "156125", "218625", "499500", "0"],
            "unexpected example output"
        );
        if let Some(prev) = &previous {
            assert_eq!(prev, &stdout, "example output changed between runs");
        }
        previous = Some(stdout);
    }

    let _ = std::fs::remove_file(&copy);
    let _ = std::fs::remove_file(&exe);
}
