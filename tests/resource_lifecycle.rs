//! Runtime resource lifecycle tests — Session 198.
//!
//! The Session 198 audit asked a question the existing suites cannot answer:
//! not "does this operation work once", but "does repeating it, and repeating
//! its failure paths, leave the process with nothing behind". Each test here
//! compiles a MINK program that runs a bounded number of resource cycles and
//! samples the child's **OS handle count** while it runs
//! (`GetProcessHandleCount`), then asserts the count does not trend upward.
//! A Win32 handle leaked on an error path (an unclosed `CreateFileA`, pipe,
//! process or thread handle) shows up here even though the program itself
//! exits 0, because a kernel handle is invisible to the MINK heap leak scan.
//!
//! A clean exit code additionally proves the MINK-side lifecycle: exit 106
//! (`E-R06`) would fire if any runtime allocation was still live, and 104
//! (`E-R04`) if a release path freed the wrong thing.
//!
//! Artifacts live under `target/mink-artifacts/` rather than `%TEMP%`, for the
//! same Windows Defender reason documented in `tests/threads_lib.rs`.

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn artifact_dir() -> PathBuf {
    let dir = PathBuf::from("target").join("mink-artifacts").join("s198");
    std::fs::create_dir_all(&dir).expect("failed to create target/mink-artifacts/s198");
    dir
}

/// Forward-slashed, MINK-literal-safe rendering of a path.
fn mink_path(p: &Path) -> String {
    p.to_str().expect("non-UTF-8 path").replace('\\', "/")
}

/// Compiles `source` and returns the executable path. The source is deleted;
/// on a build failure it is kept for inspection.
fn build(tag: &str, source: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("rl_{tag}_{n}.mink"));
    std::fs::write(&path, source.replace("\r\n", "\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("build")
        .arg(&path)
        .output()
        .expect("failed to run the compiler");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        panic!(
            "build failed (source kept at {}):\n{stderr}",
            path.display()
        );
    }
    let _ = std::fs::remove_file(&path);
    path.with_extension("exe")
}

fn read_lib(name: &str) -> String {
    std::fs::read_to_string(format!("stdlib/{name}.mink"))
        .unwrap_or_else(|e| panic!("failed to read stdlib/{name}.mink: {e}"))
}

/// A finished child run plus the handle-count samples taken while it ran.
struct Outcome {
    code: i32,
    stdout: String,
    stderr: String,
    samples: Vec<u32>,
    elapsed: Duration,
}

/// Runs `exe`, sampling its OS handle count every `SAMPLE_INTERVAL` until it
/// exits, and fails the test if it outlives `budget`.
fn run_sampled(exe: &Path, budget: Duration) -> Outcome {
    const SAMPLE_INTERVAL: Duration = Duration::from_millis(10);

    let out_path = exe.with_extension("out");
    let err_path = exe.with_extension("err");
    let mut child = Command::new(exe)
        .stdout(std::fs::File::create(&out_path).unwrap())
        .stderr(std::fs::File::create(&err_path).unwrap())
        .spawn()
        .expect("failed to spawn test exe");
    let pid = child.id();

    let start = Instant::now();
    let mut samples = Vec::new();
    let status = loop {
        // A count of 0 means the OS was already tearing the process down;
        // it is not a measurement of the program's live handles.
        if let Some(h) = win::process_handles(pid).filter(|h| *h > 0) {
            samples.push(h);
        }
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if start.elapsed() > budget {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&exe);
            panic!(
                "child did not exit within {:?} — hang in a resource path",
                budget
            );
        }
        std::thread::sleep(SAMPLE_INTERVAL);
    };

    let code = status.code().unwrap_or(-1);
    let stdout = std::fs::read_to_string(&out_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(&err_path).unwrap_or_default();
    let el = start.elapsed();
    let _ = std::fs::remove_file(exe);
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&err_path);
    Outcome {
        code,
        stdout,
        stderr,
        samples,
        elapsed: el,
    }
}

/// The first non-zero integer printed by the program, or `None`.
fn first_int(stdout: &str) -> Option<i64> {
    stdout
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty() && l.parse::<i64>().is_ok())
        .and_then(|l| l.parse().ok())
}

/// Asserts the child left no OS handle behind: the last sample and the peak
/// may not exceed the warm-up baseline by more than a small fixed slack
/// (module loading can add a handle or two in the first milliseconds).
fn assert_flat_handles(o: &Outcome, label: &str) {
    let s = &o.samples;
    assert!(
        s.len() >= 3,
        "{label}: child finished too fast to sample ({:?}, {} samples); \
         the handle-leak check would be meaningless — raise the iteration count",
        o.elapsed,
        s.len()
    );
    let base = s[..3].iter().copied().max().unwrap_or(0);
    let last = *s.last().unwrap();
    let peak = *s.iter().max().unwrap();
    // Visible with `--nocapture`: the measured evidence for the session
    // record, not an assertion.
    println!(
        "{label}: {} samples / {:?}, baseline {base}, peak {peak}, final {last}",
        s.len(),
        o.elapsed
    );
    assert!(
        peak <= base + 6,
        "{label}: OS handle count grew from {base} to a peak of {peak} \
         (samples: {s:?}) — a Win32 handle is leaked on a repeated path"
    );
    assert!(
        last <= base + 4,
        "{label}: OS handle count ended at {last}, up from {base} \
         (samples: {s:?}) — a Win32 handle is leaked on a repeated path"
    );
}

/// Asserts a clean close-out: exit 0 (so no `E-R06` leak report and no
/// `E-R04` invalid free), no stderr, and a counter of 0 if the program
/// printed one.
fn assert_clean(o: &Outcome, label: &str) {
    assert_eq!(
        o.code, 0,
        "{label}: exited {} (stdout={:?}, stderr={:?})",
        o.code, o.stdout, o.stderr
    );
    assert!(
        o.stderr.is_empty(),
        "{label}: unexpected stderr: {:?}",
        o.stderr
    );
}

// ============================================================================
// Win32 handle counting
// ============================================================================

mod win {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn GetProcessHandleCount(process: *mut c_void, count: *mut u32) -> i32;
    }

    /// `PROCESS_QUERY_LIMITED_INFORMATION` — the least access that answers a
    /// handle count, and the one Windows grants across integrity levels.
    const QUERY_LIMITED: u32 = 0x1000;

    /// The number of OS handles open in `pid`, or `None` if the process has
    /// already gone. Uses the process handle and closes it immediately, so
    /// sampling does not perturb the parent's own count.
    pub fn process_handles(pid: u32) -> Option<u32> {
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
}

// ============================================================================
// 1. Filesystem: repeated successful create/write/read/size/enumerate/close
// ============================================================================

#[test]
fn rl01_filesystem_cycles_do_not_leak_os_handles() {
    let dir = artifact_dir().join("rl01");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("payload.txt");
    let source = format!(
        r#"
fn main() {{
    let path = "{file}";
    let dir = "{dir}";
    let mut bad = 0;
    let mut i = 0;
    while i < 3000 {{
        if fs_write(path, "0123456789") != 10 {{ bad = bad + 1; }}
        let s = fs_read(path);
        if rt_str_len(s) != 10 {{ bad = bad + 1; }}
        rt_str_free(s);
        if fs_file_size(path) != 10 {{ bad = bad + 1; }}
        if fs_exists(path) == false {{ bad = bad + 1; }}
        let h = fs_dir_open(dir);
        if h == 0 {{ bad = bad + 1; }}
        let mut more = 1;
        while more == 1 {{
            let e = fs_dir_next(h);
            if rt_str_len(e) == 0 {{ more = 0; }}
            rt_str_free(e);
        }}
        if fs_dir_close(h) != 0 {{ bad = bad + 1; }}
        i = i + 1;
    }}
    fs_remove_file(path);
    rt_print_int(bad);
    rt_exit(0);
}}
"#,
        file = mink_path(&file),
        dir = mink_path(&dir),
    );
    let full = format!("{}\n{source}", read_lib("filesystem"));
    let exe = build("fs_cycles", &full);
    let o = run_sampled(&exe, Duration::from_secs(120));
    let _ = std::fs::remove_dir_all(&dir);
    assert_clean(&o, "filesystem cycles");
    assert_eq!(
        first_int(&o.stdout),
        Some(0),
        "filesystem cycles reported failures (stdout={:?})",
        o.stdout
    );
    assert_flat_handles(&o, "filesystem cycles");
}

// ============================================================================
// 2. Filesystem: repeated *failures* — the paths that historically skipped
//    their CloseHandle
// ============================================================================

#[test]
fn rl02_failed_filesystem_operations_do_not_leak_os_handles() {
    let dir = artifact_dir().join("rl02");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let missing = dir.join("never_created.txt");
    let source = format!(
        r#"
fn main() {{
    let miss = "{miss}";
    let mut bad = 0;
    let mut i = 0;
    while i < 3000 {{
        if fs_exists(miss) != false {{ bad = bad + 1; }}
        if fs_file_size(miss) != -1 {{ bad = bad + 1; }}
        let s = fs_read(miss);
        if rt_str_len(s) != 0 {{ bad = bad + 1; }}
        rt_str_free(s);
        if fs_remove_file(miss) != 1 {{ bad = bad + 1; }}
        let h = fs_dir_open(miss);
        if h != 0 {{ bad = bad + 1; }}
        let e = fs_dir_next(h);
        if rt_str_len(e) != 0 {{ bad = bad + 1; }}
        rt_str_free(e);
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
    let exe = build("fs_failures", &full);
    let o = run_sampled(&exe, Duration::from_secs(120));
    let _ = std::fs::remove_dir_all(&dir);
    assert_clean(&o, "failed filesystem operations");
    assert_eq!(
        first_int(&o.stdout),
        Some(0),
        "failure paths returned unexpected values (stdout={:?})",
        o.stdout
    );
    assert_flat_handles(&o, "failed filesystem operations");
}

// ============================================================================
// 3. Process: repeated child spawn, capture and reap
// ============================================================================

#[test]
fn rl03_process_cycles_do_not_leak_os_handles() {
    let source = r#"
fn main() {
    let mut bad = 0;
    let mut i = 0;
    while i < 40 {
        let c = process_run("echo rl198");
        if c != 0 { bad = bad + 1; }
        if process_stdout_len() < 2 { bad = bad + 1; }
        i = i + 1;
    }
    rt_print_int(bad);
    rt_exit(0);
}
"#;
    let full = format!("{}\n{}\n{source}", read_lib("strings"), read_lib("process"));
    let exe = build("process_cycles", &full);
    let o = run_sampled(&exe, Duration::from_secs(180));
    assert_clean(&o, "process cycles");
    assert_eq!(
        first_int(&o.stdout),
        Some(0),
        "process cycles reported failures (stdout={:?})",
        o.stdout
    );
    assert_flat_handles(&o, "process cycles");
}

// ============================================================================
// 4. Threads: repeated spawn/join; the kernel thread handle must be closed
// ============================================================================

#[test]
fn rl04_thread_cycles_do_not_leak_os_handles() {
    let source = r#"
fn worker(p: Ptr<Int>) -> Int { return rt_mem_load(p) + 1; }

fn main() {
    let mut bad = 0;
    let mut i = 0;
    while i < 3000 {
        let a = rt_alloc(8);
        rt_mem_store(a, 7);
        let t = thread_spawn(worker, a);
        if thread_join(t) != 8 { bad = bad + 1; }
        rt_free(a);
        i = i + 1;
    }
    rt_print_int(bad);
    rt_exit(0);
}
"#;
    let full = format!("{source}\n{}", read_lib("threads"));
    let exe = build("thread_cycles", &full);
    let o = run_sampled(&exe, Duration::from_secs(120));
    assert_clean(&o, "thread cycles");
    assert_eq!(
        first_int(&o.stdout),
        Some(0),
        "thread cycles reported failures (stdout={:?})",
        o.stdout
    );
    assert_flat_handles(&o, "thread cycles");
}

// ============================================================================
// 5. Heap: repeated collection and raw allocation cycles leave nothing live
// ============================================================================

#[test]
fn rl05_allocation_cycles_leave_no_live_allocations() {
    let source = r#"
fn main() {
    let mut i = 0;
    let mut sum = 0;
    while i < 200000 {
        let v = rt_vec_new(4);
        rt_vec_push(v, i);
        rt_vec_push(v, i + 1);
        sum = sum + rt_vec_len(v);
        rt_vec_free(v);
        let p = rt_alloc(32);
        rt_free(p);
        i = i + 1;
    }
    rt_print_int(sum);
    rt_exit(0);
}
"#;
    let exe = build("alloc_cycles", source);
    let o = run_sampled(&exe, Duration::from_secs(120));
    assert_clean(&o, "allocation cycles");
    assert_eq!(
        first_int(&o.stdout),
        Some(400000),
        "allocation cycles produced the wrong total (stdout={:?})",
        o.stdout
    );
}

// ============================================================================
// 6. Runtime-owned views are rejected by the release path
// ============================================================================

// ============================================================================
// 6. Negative control: the sampler must *see* a deliberate handle leak, or
//    the flatness assertions above would prove nothing.
// ============================================================================

#[test]
fn rl06_detector_sees_a_deliberate_handle_leak() {
    let dir = artifact_dir().join("rl06");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // 400 enumerations are opened and never closed: 400 leaked FindFirstFile
    // handles and 400 leaked state blocks, so the exit scan reports E-R06 and
    // the handle count rises monotonically.
    let source = format!(
        r#"
fn main() {{
    let dir = "{dir}";
    let mut i = 0;
    while i < 400 {{
        let h = fs_dir_open(dir);
        if h == 0 {{ rt_exit(2); }}
        i = i + 1;
    }}
    rt_exit(0);
}}
"#,
        dir = mink_path(&dir),
    );
    let full = format!("{}\n{source}", read_lib("filesystem"));
    let exe = build("leak_control", &full);
    let o = run_sampled(&exe, Duration::from_secs(60));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        o.code, 106,
        "a program that forgets fs_dir_close must fail the leak check, got {}",
        o.code
    );
    assert!(!o.samples.is_empty(), "no handle samples were taken");
    let base = o.samples[0];
    let peak = *o.samples.iter().max().unwrap();
    assert!(
        peak >= base + 40,
        "the sampler did not observe a 400-handle leak (baseline {base}, peak {peak}, \
         samples {:?}) — the flatness assertions would be meaningless",
        o.samples
    );
    println!(
        "leak control: baseline {base}, peak {peak}, samples {:?}",
        o.samples
    );
}

#[test]
fn rl07_freeing_a_runtime_owned_capture_view_is_rejected() {
    let source = r#"
fn main() {
    process_run("echo rl198");
    let s = process_stdout();
    rt_str_free(s);
    rt_exit(0);
}
"#;
    let full = format!("{}\n{}\n{source}", read_lib("strings"), read_lib("process"));
    let exe = build("borrowed_free", &full);
    let o = run_sampled(&exe, Duration::from_secs(60));
    assert_eq!(
        o.code, 104,
        "freeing the runtime-owned stdout view must fault E-R04 (104), got {} stderr={:?}",
        o.code, o.stderr
    );
    assert!(
        o.stderr.contains("E-R04"),
        "expected E-R04 diagnostics, got {:?}",
        o.stderr
    );
}
