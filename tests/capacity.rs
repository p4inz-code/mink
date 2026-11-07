//! Session 204 — runtime capacity, allocation limits and handle exhaustion.
//!
//! The three documented ceilings are exercised at their boundaries without
//! reproducing the historical 1 GiB failure:
//!
//! - an allocation request above the 1 GiB arena (`E-R02`/102) fails
//!   immediately, before any memory is committed, and a near-`i64::MAX` size
//!   cannot wrap into a small allocation;
//! - one 8 MiB block can be written at both ends, freed and reused;
//! - 1,024 simultaneously live allocations are accepted (`MAX_LIVE_ALLOCS`);
//!   the 1,025th is `E-R03`/103; freeing one and allocating again succeeds;
//! - 65,536 reuses of one block exhaust its 16-bit handle generation. The
//!   block is retired instead of wrapping to generation 0, so a stale handle
//!   issued for its first occupant stays stale (`E-R05`/105) instead of
//!   silently aliasing the new occupant.
//!
//! Every program is compiled by the real CLI and executed as a native PE;
//! clean exits also run the runtime leak check.

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn artifact_dir() -> PathBuf {
    let dir = PathBuf::from("target").join("mink-artifacts").join("s204");
    std::fs::create_dir_all(&dir).expect("failed to create target/mink-artifacts/s204");
    dir
}

/// Compiles `source` and returns the executable path. The source is deleted;
/// on a build failure it is kept for inspection.
fn build(tag: &str, source: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("cap_{tag}_{n}.mink"));
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
    let exe = build(tag, body);
    let (code, out, err) = run(&exe);
    let _ = std::fs::remove_file(&exe);
    (code, out, err)
}

fn first_int(s: &str) -> Option<i64> {
    s.split_whitespace().find_map(|t| t.parse::<i64>().ok())
}

/// A 2 GiB request is above the 1 GiB arena and fails before committing.
#[test]
fn cap01_request_above_arena_ceiling_is_er02() {
    let body = r#"
fn main() {
    let p = rt_alloc(2147483648);
    rt_free(p);
    rt_exit(0);
}
"#;
    let (code, out, err) = build_and_run("above_arena", body);
    assert_eq!(
        code, 102,
        "a 2 GiB request must be E-R02, got {code} stdout={out:?} stderr={err:?}"
    );
    assert!(
        err.contains("E-R02"),
        "expected E-R02 diagnostics, got {err:?}"
    );
}

/// The largest positive size cannot wrap the alignment arithmetic into a
/// small successful allocation.
#[test]
fn cap02_huge_size_does_not_wrap() {
    let body = r#"
fn main() {
    let p = rt_alloc(9223372036854775807);
    rt_free(p);
    rt_exit(0);
}
"#;
    let (code, out, err) = build_and_run("huge", body);
    assert_eq!(
        code, 102,
        "an i64::MAX request must be E-R02, got {code} stdout={out:?} stderr={err:?}"
    );
    assert!(
        err.contains("E-R02"),
        "expected E-R02 diagnostics, got {err:?}"
    );
}

/// An 8 MiB block round-trips at both ends after free-list reuse.
#[test]
fn cap03_eight_mib_block_roundtrip_and_reuse() {
    let body = r#"
fn main() {
    let s = rt_str_alloc(8388608);
    rt_str_set_byte(s, 0, 65);
    rt_str_set_byte(s, 8388607, 90);
    let n = rt_str_len(s);
    let a = rt_str_byte(s, 0);
    let b = rt_str_byte(s, 8388607);
    rt_str_free(s);
    let t = rt_str_alloc(8388608);
    rt_str_set_byte(t, 0, 7);
    let c = rt_str_byte(t, 0);
    rt_str_free(t);
    if n != 8388608 { rt_exit(2); }
    if a != 65 { rt_exit(3); }
    if b != 90 { rt_exit(4); }
    if c != 7 { rt_exit(5); }
    rt_exit(0);
}
"#;
    let (code, out, err) = build_and_run("eight_mib", body);
    assert_eq!(
        code, 0,
        "8 MiB round trip failed: stdout={out:?} stderr={err:?}"
    );
}

/// 1,024 simultaneously live allocations are accepted and all can be freed.
#[test]
fn cap04_live_table_accepts_1024() {
    let body = r#"
fn chain(n: Int) -> Int {
    if n == 0 {
        return 0;
    }
    let p = rt_alloc(16);
    let r = chain(n - 1);
    rt_free(p);
    return r + 1;
}

fn main() {
    let total = chain(1024);
    rt_print_int(total);
    rt_exit(0);
}
"#;
    let (code, out, err) = build_and_run("table_1024", body);
    assert_eq!(
        code, 0,
        "1,024 live allocations must succeed: stdout={out:?} stderr={err:?}"
    );
    assert_eq!(first_int(&out), Some(1024), "freed count: {out:?}");
}

/// While 1,024 allocations are live, the next one is `E-R03`/103.
#[test]
fn cap05_1025th_live_allocation_is_er03() {
    let body = r#"
fn chain(n: Int) -> Int {
    if n == 0 {
        let extra = rt_alloc(16);
        rt_free(extra);
        return 0;
    }
    let p = rt_alloc(16);
    let r = chain(n - 1);
    rt_free(p);
    return r;
}

fn main() {
    chain(1024);
    rt_exit(0);
}
"#;
    let (code, out, err) = build_and_run("table_1025", body);
    assert_eq!(
        code, 103,
        "the 1,025th live allocation must be E-R03, got {code} stdout={out:?} stderr={err:?}"
    );
    assert!(
        err.contains("E-R03"),
        "expected E-R03 diagnostics, got {err:?}"
    );
}

/// Freeing one block at the limit lets the next allocation reuse its slot.
#[test]
fn cap06_slot_reuse_at_the_limit() {
    let body = r#"
fn chain(n: Int) -> Int {
    if n == 0 {
        let p = rt_alloc(16);
        rt_free(p);
        let q = rt_alloc(16);
        rt_free(q);
        return 0;
    }
    let p = rt_alloc(16);
    let r = chain(n - 1);
    rt_free(p);
    return r + 1;
}

fn main() {
    let total = chain(1023);
    rt_print_int(total);
    rt_exit(0);
}
"#;
    let (code, out, err) = build_and_run("slot_reuse", body);
    assert_eq!(
        code, 0,
        "free-and-reallocate at the table limit failed: stdout={out:?} stderr={err:?}"
    );
    assert_eq!(first_int(&out), Some(1023), "freed count: {out:?}");
}

/// Generation exhaustion retires the block: a stale handle issued for its
/// first occupant must not alias the generation-0/1 occupant after 65,536
/// reuses.
#[test]
fn cap07_generation_exhaustion_retires_the_slot() {
    let body = r#"
fn main() {
    let first = rt_vec_new(4);
    let h = rt_vec_push(first, 11);
    rt_vec_free(h);
    let mut i = 0;
    while i < 65535 {
        let v = rt_vec_new(4);
        rt_vec_free(v);
        i = i + 1;
    }
    let live = rt_vec_new(4);
    let x = rt_vec_get(first, 0);
    rt_print_int(x);
    rt_vec_free(live);
    rt_exit(0);
}
"#;
    let (code, out, err) = build_and_run("generation_exhaustion", body);
    assert_eq!(
        code, 105,
        "a stale handle must stay stale after generation exhaustion, got \
         {code} stdout={out:?} stderr={err:?}"
    );
    assert!(
        err.contains("E-R05"),
        "expected E-R05 diagnostics, got {err:?}"
    );
}
