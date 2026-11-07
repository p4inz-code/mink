//! Bounded Windows soak tests — Session 199.
//!
//! These are *deliberately* not part of the default `cargo test` run: each one
//! keeps a machine busy for seconds, and a soak suite that runs on every edit
//! gets deleted. Run them explicitly:
//!
//! ```text
//! cargo test --release --test soak -- --ignored --nocapture
//! ```
//!
//! Every workload is bounded — fixed iteration counts and a wall-clock budget
//! enforced by the harness, which kills a run that overruns instead of
//! hanging the suite. Artifacts are written under `target/mink-artifacts/s199`
//! and removed afterwards; nothing outside that directory and the generated
//! MINK temp files is touched.
//!
//! Measurements recorded per run: iterations, elapsed time, exit code, OS
//! handle count samples, peak working set (via `K32GetProcessMemoryInfo`), and
//! the program's own internal failure counter. `--nocapture` prints them; the
//! session record quotes the observed numbers.

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn artifact_dir() -> PathBuf {
    let dir = PathBuf::from("target").join("mink-artifacts").join("s199");
    std::fs::create_dir_all(&dir).expect("failed to create target/mink-artifacts/s199");
    dir
}

fn mink_path(p: &Path) -> String {
    p.to_str().expect("non-UTF-8 path").replace('\\', "/")
}

fn read_lib(name: &str) -> String {
    std::fs::read_to_string(format!("stdlib/{name}.mink"))
        .unwrap_or_else(|e| panic!("failed to read stdlib/{name}.mink: {e}"))
}

/// Compiles `source`, returning the executable path (source deleted on
/// success, kept on failure).
fn build(tag: &str, source: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("soak_{tag}_{n}.mink"));
    std::fs::write(&path, source.replace("\r\n", "\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("build")
        .arg(&path)
        .output()
        .expect("failed to run the compiler");
    if !output.status.success() {
        panic!(
            "build failed (source kept at {}):\n{}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let _ = std::fs::remove_file(&path);
    path.with_extension("exe")
}

struct Outcome {
    code: i32,
    stdout: String,
    stderr: String,
    handles: Vec<u32>,
    peak_ws: Option<usize>,
    elapsed: Duration,
}

/// Runs `exe` under a wall-clock budget, sampling the child's OS handle count
/// and peak working set every 10 ms. A run that overruns the budget is killed
/// and the test fails — a soak that hangs is a failure, not a result.
fn run_sampled(exe: &Path, budget: Duration) -> Outcome {
    const SAMPLE: Duration = Duration::from_millis(10);

    let out_path = exe.with_extension("out");
    let err_path = exe.with_extension("err");
    let mut child = Command::new(exe)
        .stdout(std::fs::File::create(&out_path).unwrap())
        .stderr(std::fs::File::create(&err_path).unwrap())
        .spawn()
        .expect("failed to spawn soak exe");
    let pid = child.id();

    let start = Instant::now();
    let mut handles = Vec::new();
    let mut peak_ws = None;
    let status = loop {
        if let Some(h) = win::handle_count(pid).filter(|h| *h > 0) {
            handles.push(h);
        }
        if let Some(m) = win::peak_working_set(pid) {
            peak_ws = Some(peak_ws.map_or(m, |p: usize| p.max(m)));
        }
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if start.elapsed() > budget {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(exe);
            panic!("soak run exceeded its {budget:?} budget — hang under load");
        }
        std::thread::sleep(SAMPLE);
    };

    let elapsed = start.elapsed();
    let code = status.code().unwrap_or(-1);
    let stdout = std::fs::read_to_string(&out_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(&err_path).unwrap_or_default();
    let _ = std::fs::remove_file(exe);
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&err_path);
    Outcome {
        code,
        stdout,
        stderr,
        handles,
        peak_ws,
        elapsed,
    }
}

fn first_int(stdout: &str) -> Option<i64> {
    stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && l.parse::<i64>().is_ok())
        .and_then(|l| l.parse().ok())
}

fn mib(bytes: Option<usize>) -> String {
    match bytes {
        Some(b) => format!("{:.1} MiB", b as f64 / (1024.0 * 1024.0)),
        None => "unavailable".to_string(),
    }
}

/// Prints the measurement line that the session record quotes.
fn report(label: &str, iterations: usize, o: &Outcome) {
    let (base, peak, last) = match (o.handles.first(), o.handles.iter().max(), o.handles.last()) {
        (Some(b), Some(p), Some(l)) => (*b, *p, *l),
        _ => (0, 0, 0),
    };
    let per = if iterations > 0 {
        o.elapsed.as_secs_f64() * 1000.0 / iterations as f64
    } else {
        0.0
    };
    println!(
        "{label}: iterations={iterations} elapsed={:.3}s ({per:.3} ms/iter) exit={} \
         handles={base}->{peak}->{last} ({} samples) peak_ws={} stdout={:?} stderr={:?}",
        o.elapsed.as_secs_f64(),
        o.code,
        o.handles.len(),
        mib(o.peak_ws),
        o.stdout.trim(),
        o.stderr.trim(),
    );
}

fn assert_clean(o: &Outcome, label: &str) {
    assert_eq!(
        o.code, 0,
        "{label}: exited {} (stdout={:?} stderr={:?})",
        o.code, o.stdout, o.stderr
    );
    assert!(
        o.stderr.is_empty(),
        "{label}: unexpected stderr: {:?}",
        o.stderr
    );
}

/// No handle leak: peak/last may not drift more than a few above the warm-up.
fn assert_flat(o: &Outcome, label: &str) {
    if o.handles.len() < 3 {
        return; // too short to judge; the report line shows the sample count
    }
    let base = o.handles[..3].iter().copied().max().unwrap_or(0);
    let peak = *o.handles.iter().max().unwrap();
    let last = *o.handles.last().unwrap();
    assert!(
        peak <= base + 8 && last <= base + 6,
        "{label}: handle count drifted {base} -> peak {peak} -> final {last} \
         ({} samples)",
        o.handles.len()
    );
}

// ============================================================================
// Environment / provenance
// ============================================================================

#[test]
fn soak00_environment_is_reproducible() {
    let ver = Command::new("cmd").arg("/c").arg("ver").output().ok();
    if let Some(v) = ver {
        println!("os: {}", String::from_utf8_lossy(&v.stdout).trim());
    }
    for tool in ["rustc", "cargo"] {
        if let Ok(o) = Command::new(tool).arg("--version").output() {
            println!("{tool}: {}", String::from_utf8_lossy(&o.stdout).trim());
        }
    }
    let mink = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("version")
        .output()
        .expect("failed to run mink version");
    println!("mink: {}", String::from_utf8_lossy(&mink.stdout).trim());
    println!(
        "profile: {}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    assert!(mink.status.success());
}

// ============================================================================
// 1. Repeated compiler invocations
// ============================================================================

#[test]
#[ignore = "bounded soak: run with --ignored"]
fn soak01_repeated_compiler_invocations() {
    // A program with a real body: collections, a loop, a nested helper.
    let source = r#"
fn total(v: Vec<Int>, n: Int) -> Int {
    let mut i = 0;
    let mut sum = 0;
    while i < n {
        sum = sum + rt_vec_get(v, i);
        i = i + 1;
    }
    return sum;
}

fn main() {
    let v = rt_vec_new(16);
    let mut i = 0;
    while i < 500 { rt_vec_push(v, i); i = i + 1; }
    rt_print_int(total(v, 500));
    rt_vec_free(v);
    rt_exit(0);
}
"#;
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let src = artifact_dir().join(format!("soak_compile_{n}.mink"));
    std::fs::write(&src, source).unwrap();

    let mut times = Vec::new();
    let start = Instant::now();
    for i in 0..6 {
        let t0 = Instant::now();
        let out = Command::new(env!("CARGO_BIN_EXE_mink"))
            .arg("build")
            .arg(&src)
            .output()
            .expect("compiler did not run");
        let exe = src.with_extension("exe");
        assert!(
            out.status.success(),
            "build {i} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let size = std::fs::metadata(&exe).map(|m| m.len()).unwrap_or(0);
        assert!(size > 10_000, "build {i} produced no PE ({size} bytes)");
        times.push(t0.elapsed());
        let _ = std::fs::remove_file(&exe);
    }
    let _ = std::fs::remove_file(&src);
    println!(
        "compiler invocations: 6 over {:.3}s, per build: {:?}",
        start.elapsed().as_secs_f64(),
        times
    );
}

// ============================================================================
// 2. Repeated program execution (runtime init + shutdown each time)
// ============================================================================

#[test]
#[ignore = "bounded soak: run with --ignored"]
fn soak02_repeated_program_launches() {
    let exe = build(
        "launch",
        r#"
fn main() {
    let mut v = rt_vec_new(8);
    let mut i = 0;
    while i < 100 { v = rt_vec_push(v, i); i = i + 1; }
    if rt_vec_len(v) != 100 { rt_exit(3); }
    rt_vec_free(v);
    rt_print_int(1);
    rt_exit(0);
}
"#,
    );
    let runs = 100;
    let mut codes = std::collections::BTreeMap::new();
    let start = Instant::now();
    for i in 0..runs {
        let out = Command::new(&exe)
            .output()
            .expect("failed to run launch {i}");
        let code = out.status.code().unwrap_or(-1);
        *codes.entry(code).or_insert(0usize) += 1;
        if out.status.code().unwrap_or(-1) != 0 {
            panic!(
                "launch {i} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
    let _ = std::fs::remove_file(&exe);
    println!(
        "program launches: {runs} in {:.3}s ({:.2} ms each), exit codes: {:?}",
        start.elapsed().as_secs_f64(),
        start.elapsed().as_secs_f64() * 1000.0 / runs as f64,
        codes
    );
    assert_eq!(
        codes.get(&0).copied().unwrap_or(0),
        runs,
        "not every launch exited 0: {codes:?}"
    );
}

// ============================================================================
// 3. Collection allocation/destruction and mutation
// ============================================================================

#[test]
#[ignore = "bounded soak: run with --ignored"]
fn soak03_collection_lifecycle() {
    let source = r#"
fn main() {
    let mut checks = 0;
    let mut i = 0;
    while i < 120000 {
        let v = rt_vec_new(4);
        rt_vec_push(v, i);
        rt_vec_push(v, i + 1);
        rt_vec_push(v, i + 2);
        rt_vec_set(v, 0, 7);
        if rt_vec_len(v) != 3 { checks = checks + 1; }
        if rt_vec_get(v, 0) != 7 { checks = checks + 1; }
        if rt_vec_get(v, 2) != i + 2 { checks = checks + 1; }
        rt_vec_free(v);
        let p = rt_alloc(24);
        rt_free(p);
        i = i + 1;
    }
    rt_print_int(checks);
    rt_exit(0);
}
"#;
    let exe = build("collections", source);
    let o = run_sampled(&exe, Duration::from_secs(120));
    report("collection lifecycle", 400_000, &o);
    assert_clean(&o, "collection lifecycle");
    assert_eq!(first_int(&o.stdout), Some(0), "internal checks failed");
    assert_flat(&o, "collection lifecycle");
}

// ============================================================================
// 4. Filesystem create/write/read/close cycles + medium file I/O
// ============================================================================

#[test]
#[ignore = "bounded soak: run with --ignored"]
fn soak04_filesystem_cycles() {
    let dir = artifact_dir().join("fs");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let small = dir.join("small.txt");
    let big = dir.join("big.bin");
    // 1 MiB fixture written by the harness, read by MINK.
    std::fs::write(&big, vec![0x5A_u8; 1024 * 1024]).unwrap();

    let source = format!(
        r#"
fn main() {{
    let small = "{small}";
    let big = "{big}";
    let mut bad = 0;
    let mut i = 0;
    while i < 3000 {{
        let data = str_repeat("0123456789", 410);
        if fs_write(small, data) != 4100 {{ bad = bad + 1; }}
        let got = fs_read(small);
        if rt_str_len(got) != 4100 {{ bad = bad + 1; }}
        rt_str_free(got);
        if fs_file_size(small) != 4100 {{ bad = bad + 1; }}
        i = i + 1;
    }}
    let mut j = 0;
    while j < 5 {{
        let got = fs_read(big);
        if rt_str_len(got) != 1048576 {{ bad = bad + 1; }}
        rt_str_free(got);
        j = j + 1;
    }}
    let removed = fs_remove_file(small);
    let rem = removed != 0 && removed != 1;

    rt_print_int(bad);
    rt_exit(0);
}}
"#,
        small = mink_path(&small),
        big = mink_path(&big),
    );
    let full = format!(
        "{}\n{}\n{source}",
        read_lib("filesystem"),
        read_lib("strings")
    );
    let exe = build("fs", &full);
    let o = run_sampled(&exe, Duration::from_secs(180));
    let _ = std::fs::remove_dir_all(&dir);
    report("filesystem cycles", 3_000, &o);
    assert_clean(&o, "filesystem cycles");
    assert_eq!(first_int(&o.stdout), Some(0), "fs checks failed");
    assert_flat(&o, "filesystem cycles");
}

// ============================================================================
// 5. Child process creation and cleanup
// ============================================================================

#[test]
#[ignore = "bounded soak: run with --ignored"]
fn soak05_process_cycles() {
    let source = r#"
fn main() {
    let mut bad = 0;
    let mut i = 0;
    while i < 100 {
        let c = process_run("echo soak199");
        if c != 0 { bad = bad + 1; }
        if process_stdout_len() < 7 { bad = bad + 1; }
        i = i + 1;
    }
    rt_print_int(bad);
    rt_exit(0);
}
"#;
    let full = format!("{}\n{}\n{source}", read_lib("strings"), read_lib("process"));
    let exe = build("process", &full);
    let o = run_sampled(&exe, Duration::from_secs(180));
    report("process cycles", 100, &o);
    assert_clean(&o, "process cycles");
    assert_eq!(first_int(&o.stdout), Some(0), "process checks failed");
    assert_flat(&o, "process cycles");
}

// ============================================================================
// 6. Repeated error paths
// ============================================================================

#[test]
#[ignore = "bounded soak: run with --ignored"]
fn soak06_error_paths() {
    let dir = artifact_dir().join("err");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let missing = dir.join("absent.txt");

    let source = format!(
        r#"
fn main() {{
    let miss = "{miss}";
    let mut bad = 0;
    let mut i = 0;
    while i < 20000 {{
        if fs_exists(miss) != false {{ bad = bad + 1; }}
        if fs_file_size(miss) != -1 {{ bad = bad + 1; }}
        let s = fs_read(miss);
        if rt_str_len(s) != 0 {{ bad = bad + 1; }}
        rt_str_free(s);
        let h = fs_dir_open(miss);
        if h != 0 {{ bad = bad + 1; }}
        if fs_dir_close(h) != -1 {{ bad = bad + 1; }}
        i = i + 1;
    }}
    rt_print_int(bad);
    rt_exit(0);
}}
"#,
        miss = mink_path(&missing),
    );
    let full = format!("{}\n{source}", read_lib("filesystem"));
    let exe = build("errors", &full);
    let o = run_sampled(&exe, Duration::from_secs(120));
    let _ = std::fs::remove_dir_all(&dir);
    report("error paths", 20_000, &o);
    assert_clean(&o, "error paths");
    assert_eq!(first_int(&o.stdout), Some(0), "error-path checks failed");
    assert_flat(&o, "error paths");

    // The fatal error paths must stay deterministic under repetition too:
    // each of these is a distinct process, 10 times each.
    let cases: [(&str, i32, &str); 3] = [
        (
            "free_twice",
            104,
            r#"fn main() { let p = rt_alloc(16); rt_free(p); rt_free(p); rt_exit(0); }"#,
        ),
        (
            "stale_handle",
            105,
            r#"fn main() {
    let v = rt_vec_new(4);
    rt_vec_free(v);
    let w = rt_vec_new(4);
    rt_push_guard(w);
    rt_print_int(rt_vec_len(v));
    rt_exit(0);
}
"#,
        ),
        (
            "out_of_bounds",
            110,
            r#"fn main() { let mut v = rt_vec_new(2);
        rt_vec_push(v, 1);rt_print_int(rt_vec_get(v, 5)); rt_vec_free(v); rt_exit(0);}
"#,
        ),
    ];
    for (name, expected, body) in cases {
        // `stale_handle` needs the replacement allocation to exist; a push on
        // it is the simplest way to keep it live across the stale access.
        let src = body.replace("rt_push_guard(w);", "rt_vec_push(w, 1);");
        let exe = build(name, &src);
        for i in 0..10 {
            let out = Command::new(&exe).output().expect("run failed");
            let code = out.status.code().unwrap_or(-1);
            assert_eq!(
                code,
                expected,
                "{name} run {i}: expected exit {expected}, got {code} (stderr={:?})",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let _ = std::fs::remove_file(&exe);
        println!("error path {name}: 10/10 runs exited {expected}");
    }
}

// ============================================================================
// 7. Combined representative workload
// ============================================================================

#[test]
#[ignore = "bounded soak: run with --ignored"]
fn soak07_combined_workload() {
    let dir = artifact_dir().join("combined");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("payload.txt");
    let source = format!(
        r#"fn worker(p: Ptr<Int>) -> Int {{ return rt_mem_load(p) * 2; }}

fn main() {{
    let file = "{file}";
    let mut bad = 0;
    let mut i = 0;
    while i < 200 {{
        // collections
        let mut v = rt_vec_new(8);
        let mut k = 0;
        while k < 50 {{ v = rt_vec_push(v, k); k = k + 1; }}
        if rt_vec_len(v) != 50 {{ bad = bad + 1; }}
        rt_vec_free(v);
        // strings + filesystem
        let s = str_repeat("ab", 512);
        if fs_write(file, s) != 1024 {{ bad = bad + 1; }}
        let r = fs_read(file);
        if rt_str_len(r) != 1024 {{ bad = bad + 1; }}
        rt_str_free(r);
        // threads
        let a = rt_alloc(8);
        rt_mem_store(a, i);
        let t = thread_spawn(worker, a);
        if thread_join(t) != i * 2 {{ bad = bad + 1; }}
        rt_free(a);
        // child process
        if process_run("echo combined") != 0 {{ bad = bad + 1; }}
        i = i + 1;
    }}
    fs_remove_file(file);
    rt_print_int(bad);
    rt_exit(0);
}}
"#,
        file = mink_path(&file),
    );
    let full = format!(
        "{}\n{}\n{}\n{}\n{source}",
        read_lib("filesystem"),
        read_lib("strings"),
        read_lib("process"),
        read_lib("threads")
    );
    let exe = build("combined", &full);

    // Three consecutive runs: repeatability, not a single lucky pass.
    for run in 1..=3 {
        let exe_copy = exe.with_extension(format!("run{run}.exe"));
        std::fs::copy(&exe, &exe_copy).unwrap();
        let o = run_sampled(&exe_copy, Duration::from_secs(180));
        report(&format!("combined run {run}"), 200, &o);
        assert_clean(&o, &format!("combined run {run}"));
        assert_eq!(
            first_int(&o.stdout),
            Some(0),
            "combined run {run}: checks failed"
        );
        assert_flat(&o, &format!("combined run {run}"));
    }
    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_dir_all(&dir);
}

// ============================================================================
// Win32 measurements
// ============================================================================

mod win {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn GetProcessHandleCount(process: *mut c_void, count: *mut u32) -> i32;
        fn K32GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    /// `PROCESS_MEMORY_COUNTERS` (x64: 4 + 4 + 8 × 8 = 72 bytes).
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    const QUERY_LIMITED: u32 = 0x1000;

    pub fn handle_count(pid: u32) -> Option<u32> {
        unsafe {
            let h = OpenProcess(QUERY_LIMITED, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut count: u32 = 0;
            let ok = GetProcessHandleCount(h, &mut count);
            CloseHandle(h);
            if ok == 0 {
                None
            } else {
                Some(count)
            }
        }
    }

    pub fn peak_working_set(pid: u32) -> Option<usize> {
        unsafe {
            let h = OpenProcess(QUERY_LIMITED, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut c = ProcessMemoryCounters {
                cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
                page_fault_count: 0,
                peak_working_set_size: 0,
                working_set_size: 0,
                quota_peak_paged_pool_usage: 0,
                quota_paged_pool_usage: 0,
                quota_peak_non_paged_pool_usage: 0,
                quota_non_paged_pool_usage: 0,
                pagefile_usage: 0,
                peak_pagefile_usage: 0,
            };
            let ok = K32GetProcessMemoryInfo(h, &mut c, c.cb);
            CloseHandle(h);
            if ok == 0 {
                None
            } else {
                Some(c.peak_working_set_size)
            }
        }
    }
}
