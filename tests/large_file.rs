//! Session 206 — large-file filesystem correctness.
//!
//! `rt_fs_file_size` and `rt_fs_read` used the 32-bit `GetFileSize` API, so a
//! file at or above 4 GiB silently reported (and read) only the low 32 bits of
//! its length. These tests pin the corrected contract:
//!
//! - `fs_file_size` reports the exact 64-bit length, including at and above
//!   4 GiB and exactly at the old `INVALID_FILE_SIZE` value;
//! - `fs_read` refuses a file larger than the 1 GiB arena with an owned empty
//!   string instead of silently returning a truncated prefix; it never
//!   allocates more than the arena ceiling;
//! - small files still read exactly.
//!
//! Test files are sparse: they are created with `SetEndOfFile` (`File::set_len`)
//! at the target length and only a small tail is written, so a 4 GiB test does
//! not consume 4 GiB of disk. Every file is removed by a guard even if a test
//! fails.

#![cfg(windows)]

use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

const GIB: u64 = 1024 * 1024 * 1024;

fn artifact_dir() -> PathBuf {
    let dir = PathBuf::from("target").join("mink-artifacts").join("s206");
    std::fs::create_dir_all(&dir).expect("failed to create target/mink-artifacts/s206");
    dir
}

fn mink_path(p: &Path) -> String {
    p.to_str().expect("non-UTF-8 path").replace('\\', "/")
}

fn build(tag: &str, source: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("lf_{tag}_{n}.mink"));
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

/// A sparse file in the artifact directory, removed on drop.
struct SparseFile {
    path: PathBuf,
    len: u64,
}

impl Drop for SparseFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Creates a file of exactly `len` bytes. A tail of `tail_len` recognizable
/// bytes is written at the end; the rest of the file is an unallocated hole.
fn sparse_file(tag: &str, len: u64, tail_len: usize) -> SparseFile {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("lf_{tag}_{}_{n}.bin", std::process::id()));
    let mut file = std::fs::File::create(&path).expect("create sparse file");
    file.set_len(len).expect("extend sparse file");
    if tail_len > 0 {
        let tail: Vec<u8> = (0..tail_len).map(|i| (i * 7 + 3) as u8).collect();
        file.seek(SeekFrom::End(-(tail_len as i64)))
            .expect("seek to tail");
        file.write_all(&tail).expect("write tail");
    }
    drop(file);
    let actual = std::fs::metadata(&path).expect("stat sparse file").len();
    assert_eq!(actual, len, "sparse file length");
    SparseFile { path, len }
}

/// A 4 GiB + 12,345-byte file reports its exact 64-bit size. The old
/// `GetFileSize` implementation returned the low DWORD (12,345).
#[test]
fn lf01_size_above_4_gib_is_exact() {
    let file = sparse_file("above", 4 * GIB + 12_345, 16);
    let body = format!(
        r#"
fn main() {{
    rt_print_int(fs_file_size("{p}"));
    rt_exit(0);
}}
"#,
        p = mink_path(&file.path)
    );
    let (code, out, err) = build_and_run("above_size", &body);
    assert_eq!(code, 0, "stdout={out:?} stderr={err:?}");
    assert_eq!(
        all_ints(&out),
        vec![file.len as i64],
        "size above 4 GiB must not be truncated to 32 bits: {out:?}"
    );
}

/// Reading a file larger than the arena is refused safely: an owned empty
/// string, never a silently truncated prefix.
#[test]
fn lf02_read_above_4_gib_is_refused_not_truncated() {
    let file = sparse_file("read_above", 4 * GIB + 12_345, 16);
    let body = format!(
        r#"
fn main() {{
    let r = fs_read("{p}");
    rt_print_int(rt_str_len(r));
    rt_str_free(r);
    rt_exit(0);
}}
"#,
        p = mink_path(&file.path)
    );
    let (code, out, err) = build_and_run("above_read", &body);
    assert_eq!(
        code, 0,
        "the refusal must still run the leak check: stdout={out:?} stderr={err:?}"
    );
    assert_eq!(
        all_ints(&out),
        vec![0],
        "a file above the arena must return an owned empty Str, not a \
         truncated prefix: {out:?}"
    );
}

/// Exactly 4 GiB is the value `GetFileSize` conflates with
/// `INVALID_FILE_SIZE`; it must report 4,294,967,296 and refuse the read.
#[test]
fn lf03_exactly_4_gib_is_exact_and_refused() {
    let file = sparse_file("exact", 4 * GIB, 0);
    let body = format!(
        r#"
fn main() {{
    rt_print_int(fs_file_size("{p}"));
    let r = fs_read("{p}");
    rt_print_int(rt_str_len(r));
    rt_str_free(r);
    rt_exit(0);
}}
"#,
        p = mink_path(&file.path)
    );
    let (code, out, err) = build_and_run("exact", &body);
    assert_eq!(code, 0, "stdout={out:?} stderr={err:?}");
    assert_eq!(
        all_ints(&out),
        vec![4 * GIB as i64, 0],
        "exactly 4 GiB must report its true size and refuse the read: {out:?}"
    );
}

/// 4 GiB - 1 is the largest value below the boundary and the value
/// `GetFileSize` returned for both success and failure. It must be exact and
/// its read (larger than the arena) refused.
#[test]
fn lf04_just_below_4_gib_is_exact_and_refused() {
    let file = sparse_file("below", 4 * GIB - 1, 0);
    let body = format!(
        r#"
fn main() {{
    rt_print_int(fs_file_size("{p}"));
    let r = fs_read("{p}");
    rt_print_int(rt_str_len(r));
    rt_str_free(r);
    rt_exit(0);
}}
"#,
        p = mink_path(&file.path)
    );
    let (code, out, err) = build_and_run("below", &body);
    assert_eq!(code, 0, "stdout={out:?} stderr={err:?}");
    assert_eq!(
        all_ints(&out),
        vec![(4 * GIB - 1) as i64, 0],
        "4 GiB - 1 must be exact and refused, not confused with an error: {out:?}"
    );
}

/// Small-file behavior is unchanged: exact size, exact bytes, exact tail.
#[test]
fn lf05_small_file_round_trip_unchanged() {
    let data: Vec<u8> = (0..65_536u32)
        .map(|i| (i.wrapping_mul(31).wrapping_add(7) % 256) as u8)
        .collect();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("lf_small_{}_{n}.bin", std::process::id()));
    std::fs::write(&path, &data).unwrap();
    let body = format!(
        r#"
fn main() {{
    rt_print_int(fs_file_size("{p}"));
    let r = fs_read("{p}");
    rt_print_int(rt_str_len(r));
    rt_print_int(rt_str_byte(r, 0));
    rt_print_int(rt_str_byte(r, rt_str_len(r) - 1));
    rt_str_free(r);
    rt_exit(0);
}}
"#,
        p = mink_path(&path)
    );
    let (code, out, err) = build_and_run("small", &body);
    let _ = std::fs::remove_file(&path);
    assert_eq!(code, 0, "stdout={out:?} stderr={err:?}");
    assert_eq!(
        all_ints(&out),
        vec![
            data.len() as i64,
            data.len() as i64,
            data[0] as i64,
            data[data.len() - 1] as i64,
        ],
        "small-file round trip changed: {out:?}"
    );
}
