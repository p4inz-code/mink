//! Session 209 — cross-subsystem correctness audit.
//!
//! Each program combines at least two subsystems in one live state:
//! filesystem reads, the collection runtime, the allocator/live table and the
//! ownership/leak checker. The cases target interactions that per-subsystem
//! suites cannot see:
//!
//! - a file's bytes stay intact while eight copies are live across recursive
//!   allocation and `Vec` churn;
//! - generation retirement survives unrelated filesystem allocation churn, so
//!   a stale handle stays `E-R05`;
//! - a failed `ReadFile` leaves an owned empty string and does not disturb
//!   collections or a subsequent successful read;
//! - exhausting the live allocation table while a file read is live fails
//!   deterministically with `E-R03`, not a crash or a truncated result.

#![cfg(windows)]

use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn artifact_dir() -> PathBuf {
    let dir = PathBuf::from("target").join("mink-artifacts").join("s209");
    std::fs::create_dir_all(&dir).expect("failed to create target/mink-artifacts/s209");
    dir
}

fn mink_path(p: &Path) -> String {
    p.to_str().expect("non-UTF-8 path").replace('\\', "/")
}

fn build(tag: &str, source: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("cs_{tag}_{n}.mink"));
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

fn read_lib(name: &str) -> String {
    std::fs::read_to_string(format!("stdlib/{name}.mink"))
        .unwrap_or_else(|e| panic!("failed to read stdlib/{name}.mink: {e}"))
}

fn run(exe: &Path) -> (i32, String, String) {
    let out_path = exe.with_extension("out");
    let err_path = exe.with_extension("err");
    let output = Command::new(exe)
        .stdout(std::fs::File::create(&out_path).unwrap())
        .stderr(std::fs::File::create(&err_path).unwrap())
        .output()
        .expect("failed to run test exe");
    let stdout = std::fs::read_to_string(&out_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(&err_path).unwrap_or_default();
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&err_path);
    (output.status.code().unwrap_or(-1), stdout, stderr)
}

fn build_and_run(tag: &str, body: &str) -> (i32, String, String) {
    let full = format!("{}\n{body}", read_lib("filesystem"));
    let exe = build(tag, &full);
    let (code, out, err) = run(&exe);
    let _ = std::fs::remove_file(&exe);
    (code, out, err)
}

fn all_ints(s: &str) -> Vec<i64> {
    s.split_whitespace()
        .filter_map(|t| t.parse::<i64>().ok())
        .collect()
}

fn fixture(tag: &str, len: usize) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("cs_{tag}_{}_{n}.bin", std::process::id()));
    let data: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
    std::fs::write(&path, &data).unwrap();
    path
}

mod win {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LockFile(h_file: *mut c_void, lo: u32, hi: u32, n_lo: u32, n_hi: u32) -> i32;
        fn UnlockFile(h_file: *mut c_void, lo: u32, hi: u32, n_lo: u32, n_hi: u32) -> i32;
    }

    pub fn lock_file(handle: *mut c_void, len: u32) -> bool {
        unsafe { LockFile(handle, 0, 0, len, 0) != 0 }
    }

    pub fn unlock_file(handle: *mut c_void, len: u32) {
        unsafe {
            UnlockFile(handle, 0, 0, len, 0);
        }
    }
}

/// Eight file reads stay live across recursive allocation and per-level `Vec`
/// churn; every copy's length and first/last byte are checked before release.
#[test]
fn cs01_file_reads_live_across_collection_churn() {
    let path = fixture("chain", 65_536);
    let body = format!(
        r#"
fn chain(n: Int) -> Int {{
    if n == 0 {{ return 0; }}
    let r = fs_read("{p}");
    let len = rt_str_len(r);
    let first = rt_str_byte(r, 0);
    let last = rt_str_byte(r, len - 1);
    let v = rt_vec_new(4);
    let v2 = rt_vec_push(v, len);
    let stored = rt_vec_get(v2, 0);
    rt_vec_free(v2);
    let inner = chain(n - 1);
    rt_str_free(r);
    if inner != 0 {{ return inner; }}
    if len != 65536 {{ return 901; }}
    if stored != len {{ return 902; }}
    if first != 3 {{ return 903; }}
    if last != 252 {{ return 904; }}
    return 0;
}}

fn main() {{
    let bad = chain(8);
    rt_print_int(bad);
    rt_exit(0);
}}
"#,
        p = mink_path(&path)
    );
    let (code, out, err) = build_and_run("chain", &body);
    let _ = std::fs::remove_file(&path);
    assert_eq!(code, 0, "stdout={out:?} stderr={err:?}");
    assert_eq!(all_ints(&out), vec![0], "cross checks failed: {out:?}");
}

/// Generation retirement is unaffected by filesystem allocation churn: a
/// freed `Vec` handle stays `E-R05` after eight file reads and frees.
#[test]
fn cs02_stale_handle_survives_filesystem_churn() {
    let path = fixture("stale", 4096);
    let body = format!(
        r#"
fn main() {{
    let v = rt_vec_new(4);
    let h = rt_vec_push(v, 7);
    rt_vec_free(h);
    let mut i = 0;
    while i < 8 {{
        let r = fs_read("{p}");
        rt_str_free(r);
        i = i + 1;
    }}
    rt_print_int(rt_vec_len(v));
    rt_exit(0);
}}
"#,
        p = mink_path(&path)
    );
    let (code, out, err) = build_and_run("stale", &body);
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        code, 105,
        "a stale handle must stay E-R05 after fs churn: stdout={out:?} stderr={err:?}"
    );
    assert!(err.contains("E-R05"), "expected E-R05: {err:?}");
}

/// A failed `ReadFile` yields an owned empty string; collections and a later
/// successful read in the same process are unaffected.
#[test]
fn cs03_failed_read_leaves_runtime_usable() {
    let locked = fixture("locked", 4096);
    let good = fixture("good", 4096);
    let body = format!(
        r#"
fn main() {{
    let a = fs_read("{lp}");
    let n = rt_str_len(a);
    let mut v = rt_vec_new(4);
    let mut i = 0;
    while i < 4 {{ v = rt_vec_push(v, i); i = i + 1; }}
    let mut ok = 0;
    if rt_vec_len(v) == 4 {{ ok = 1; }}
    rt_vec_free(v);
    let b = fs_read("{gp}");
    let m = rt_str_len(b);
    rt_str_free(a);
    rt_str_free(b);
    rt_print_int(n);
    rt_print_int(m);
    rt_print_int(ok);
    rt_exit(0);
}}
"#,
        lp = mink_path(&locked),
        gp = mink_path(&good)
    );
    let full = format!("{}\n{body}", read_lib("filesystem"));
    let exe = build("failed_read", &full);
    let file = std::fs::File::open(&locked).expect("open locked fixture");
    let handle = file.as_raw_handle() as *mut c_void;
    assert!(win::lock_file(handle, 4096), "LockFile failed");
    let (code, out, err) = run(&exe);
    win::unlock_file(handle, 4096);
    drop(file);
    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(&locked);
    let _ = std::fs::remove_file(&good);
    assert_eq!(code, 0, "stdout={out:?} stderr={err:?}");
    assert_eq!(
        all_ints(&out),
        vec![0, 4096, 1],
        "failed read disturbed later work: {out:?}"
    );
}

/// The live table fails deterministically (`E-R03`/103) while a file read is
/// live; it does not crash or truncate the read.
#[test]
fn cs04_table_exhaustion_with_live_file_data() {
    let path = fixture("exhaust", 65_536);
    let body = format!(
        r#"
fn eat(n: Int) -> Int {{
    if n == 0 {{
        let extra = rt_alloc(16);
        rt_free(extra);
        return 0;
    }}
    let p = rt_alloc(16);
    let r = eat(n - 1);
    rt_free(p);
    return r;
}}

fn main() {{
    let s = fs_read("{p}");
    if rt_str_len(s) != 65536 {{ rt_exit(2); }}
    eat(1024);
    rt_exit(0);
}}
"#,
        p = mink_path(&path)
    );
    let (code, out, err) = build_and_run("exhaust", &body);
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        code, 103,
        "the 1,025th live allocation must be E-R03: stdout={out:?} stderr={err:?}"
    );
    assert!(err.contains("E-R03"), "expected E-R03: {err:?}");
}
