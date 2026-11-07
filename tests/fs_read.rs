//! Session 203 — filesystem read correctness.
//!
//! `rt_fs_read` used to size the result from `GetFileSize`, then ignore
//! `ReadFile`'s BOOL and transferred-byte count: a failed or short read
//! returned an owned string whose length claimed the full file size, exposing
//! the uninitialised or stale tail of the allocation. These tests pin the
//! observable contract:
//!
//! - a normal read returns exactly the file's bytes and length;
//! - an empty file and a missing file return an owned empty string;
//! - repeated reads are stable and leave nothing live;
//! - a read that cannot succeed (a byte-range lock held on the file) returns
//!   an owned empty string, not a full-length buffer of uninitialised bytes,
//!   and the same executable reads correctly once the lock is released.
//!
//! The lock test is the deterministic reproduction of the failure path: Win32
//! byte-range locks are honoured by `ReadFile` even though the reader opened
//! the file normally, so no race or test-only runtime seam is needed.

#![cfg(windows)]

use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn artifact_dir() -> PathBuf {
    let dir = PathBuf::from("target").join("mink-artifacts").join("s203");
    std::fs::create_dir_all(&dir).expect("failed to create target/mink-artifacts/s203");
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
    let path = artifact_dir().join(format!("fsr_{tag}_{n}.mink"));
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

/// Concatenates `stdlib/filesystem.mink` with `body`, builds and runs it.
fn build_and_run(tag: &str, body: &str) -> (i32, String, String) {
    let full = format!("{}\n{body}", read_lib("filesystem"));
    let exe = build(tag, &full);
    let (code, out, err) = run(&exe);
    let _ = std::fs::remove_file(&exe);
    (code, out, err)
}

fn first_int(s: &str) -> Option<i64> {
    s.split_whitespace().find_map(|t| t.parse::<i64>().ok())
}

fn all_ints(s: &str) -> Vec<i64> {
    s.split_whitespace()
        .filter_map(|t| t.parse::<i64>().ok())
        .collect()
}

fn temp_file(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    artifact_dir().join(format!("fsr_{tag}_{}_{n}.bin", std::process::id()))
}

/// Small binary file: exact length and exact first/last bytes survive.
#[test]
fn fsr01_small_file_read_exact() {
    let path = temp_file("small");
    let data: Vec<u8> = (0..37u32).map(|i| (i * 7 + 3) as u8).collect();
    std::fs::write(&path, &data).unwrap();
    let body = format!(
        r#"
fn main() {{
    let r = fs_read("{p}");
    rt_print_int(rt_str_len(r));
    rt_print_int(rt_str_byte(r, 0));
    rt_print_int(rt_str_byte(r, 36));
    rt_str_free(r);
    rt_exit(0);
}}
"#,
        p = mink_path(&path)
    );
    let (code, out, err) = build_and_run("small", &body);
    let _ = std::fs::remove_file(&path);
    assert_eq!(code, 0, "small read failed: stdout={out:?} stderr={err:?}");
    assert_eq!(
        all_ints(&out),
        vec![37, data[0] as i64, data[36] as i64],
        "small file read mismatch: {out:?}"
    );
}

/// An empty file yields a length-0 owned string that can be freed.
#[test]
fn fsr02_empty_file_is_owned_empty() {
    let path = temp_file("empty");
    std::fs::write(&path, b"").unwrap();
    let body = format!(
        r#"
fn main() {{
    let r = fs_read("{p}");
    rt_print_int(rt_str_len(r));
    rt_str_free(r);
    rt_exit(0);
}}
"#,
        p = mink_path(&path)
    );
    let (code, out, err) = build_and_run("empty", &body);
    let _ = std::fs::remove_file(&path);
    assert_eq!(code, 0, "empty read failed: stdout={out:?} stderr={err:?}");
    assert_eq!(first_int(&out), Some(0), "empty file length: {out:?}");
}

/// A 256 KiB binary file is read completely and byte-exactly (checksum).
#[test]
fn fsr03_large_file_read_exact() {
    let path = temp_file("large");
    let data: Vec<u8> = (0..262_144u32)
        .map(|i| (i.wrapping_mul(31).wrapping_add(7) % 256) as u8)
        .collect();
    std::fs::write(&path, &data).unwrap();
    let expect_sum: i64 = data.iter().map(|&b| b as i64).sum::<i64>() % 251;
    let body = format!(
        r#"
fn main() {{
    let r = fs_read("{p}");
    let n = rt_str_len(r);
    rt_print_int(n);
    let mut i = 0;
    let mut sum = 0;
    while i < n {{
        sum = (sum + rt_str_byte(r, i)) % 251;
        i = i + 1;
    }}
    rt_print_int(sum);
    rt_print_int(rt_str_byte(r, 0));
    rt_print_int(rt_str_byte(r, n - 1));
    rt_str_free(r);
    rt_exit(0);
}}
"#,
        p = mink_path(&path)
    );
    let (code, out, err) = build_and_run("large", &body);
    let _ = std::fs::remove_file(&path);
    assert_eq!(code, 0, "large read failed: stdout={out:?} stderr={err:?}");
    assert_eq!(
        all_ints(&out),
        vec![
            data.len() as i64,
            expect_sum,
            data[0] as i64,
            data[data.len() - 1] as i64
        ],
        "large file read mismatch: {out:?}"
    );
}

/// Reading the same file 100 times returns the same exact bytes each time.
#[test]
fn fsr04_repeated_reads_are_stable() {
    let path = temp_file("repeat");
    let data: Vec<u8> = (0..37u32).map(|i| (i * 7 + 3) as u8).collect();
    std::fs::write(&path, &data).unwrap();
    let body = format!(
        r#"
fn main() {{
    let mut i = 0;
    let mut bad = 0;
    while i < 100 {{
        let r = fs_read("{p}");
        if rt_str_len(r) != 37 {{ bad = bad + 1; }}
        if rt_str_byte(r, 0) != 3 {{ bad = bad + 1; }}
        if rt_str_byte(r, 36) != 255 {{ bad = bad + 1; }}
        rt_str_free(r);
        i = i + 1;
    }}
    rt_print_int(bad);
    rt_exit(0);
}}
"#,
        p = mink_path(&path)
    );
    let (code, out, err) = build_and_run("repeat", &body);
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        code, 0,
        "repeated read failed: stdout={out:?} stderr={err:?}"
    );
    assert_eq!(
        first_int(&out),
        Some(0),
        "repeated read mismatches: {out:?}"
    );
}

/// A missing file yields an owned empty string (the documented result).
#[test]
fn fsr05_missing_file_is_owned_empty() {
    let path = temp_file("missing");
    let body = format!(
        r#"
fn main() {{
    let r = fs_read("{p}");
    rt_print_int(rt_str_len(r));
    rt_str_free(r);
    rt_exit(0);
}}
"#,
        p = mink_path(&path)
    );
    let (code, out, err) = build_and_run("missing", &body);
    assert_eq!(
        code, 0,
        "missing read failed: stdout={out:?} stderr={err:?}"
    );
    assert_eq!(first_int(&out), Some(0), "missing file length: {out:?}");
}

/// Win32 byte-range locks, used to force `ReadFile` to fail deterministically.
mod win {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LockFile(h_file: *mut c_void, lo: u32, hi: u32, n_lo: u32, n_hi: u32) -> i32;
        fn UnlockFile(h_file: *mut c_void, lo: u32, hi: u32, n_lo: u32, n_hi: u32) -> i32;
    }

    /// Locks bytes `0..len` of an open handle. Returns false on failure.
    pub fn lock_file(handle: *mut c_void, len: u32) -> bool {
        unsafe { LockFile(handle, 0, 0, len, 0) != 0 }
    }

    pub fn unlock_file(handle: *mut c_void, len: u32) {
        unsafe {
            UnlockFile(handle, 0, 0, len, 0);
        }
    }
}

/// The regression for the Session 203 defect: a `ReadFile` that fails must
/// not be reported as a full-length string full of uninitialised bytes.
///
/// A byte-range lock held by this test makes `ReadFile` fail with
/// `ERROR_LOCK_VIOLATION` while `GetFileSize` still succeeds. Before the fix
/// the program printed the file size (4096) and returned stale heap bytes;
/// after it prints 0, frees the owned empty string and exits 0. Releasing the
/// lock and running the same executable again reads the full file.
#[test]
fn fsr06_failed_read_returns_owned_empty_and_recovers() {
    let path = temp_file("locked");
    let data = vec![0x5Au8; 4096];
    std::fs::write(&path, &data).unwrap();
    let body = format!(
        r#"
fn main() {{
    let r = fs_read("{p}");
    rt_print_int(rt_str_len(r));
    rt_str_free(r);
    rt_exit(0);
}}
"#,
        p = mink_path(&path)
    );
    let source = format!("{}\n{body}", read_lib("filesystem"));
    let exe = build("locked", &source);

    let file = std::fs::File::open(&path).expect("open the test file");
    let handle = file.as_raw_handle() as *mut c_void;
    assert!(
        win::lock_file(handle, data.len() as u32),
        "LockFile could not lock the test file"
    );

    let (code, out, err) = run(&exe);
    assert_eq!(
        code, 0,
        "the locked read must exit cleanly (leak check included): stderr={err:?}"
    );
    assert_eq!(
        first_int(&out),
        Some(0),
        "a failing ReadFile must return an owned empty Str, not {} bytes over \
         uninitialised memory (stderr={err:?})",
        data.len()
    );

    win::unlock_file(handle, data.len() as u32);
    drop(file);

    let (code, out, err) = run(&exe);
    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(&path);
    assert_eq!(code, 0, "the unlocked read failed: stderr={err:?}");
    assert_eq!(
        first_int(&out),
        Some(data.len() as i64),
        "after releasing the lock the same executable must read the whole \
         file (stderr={err:?})"
    );
}
