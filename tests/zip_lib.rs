//! ZIP archive integration tests — Windows Python parity S44.
//!
//! Every test prepends `stdlib/zip.mink`, builds a genuine Windows PE through
//! the real `mink build` CLI, and runs it. `rt_exit(0)` performs the
//! arena/leak check, so a clean exit proves ownership is sound.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn zip_source() -> String {
    std::fs::read_to_string("stdlib/zip.mink").expect("failed to read stdlib/zip.mink")
}

fn artifact_dir() -> PathBuf {
    let dir = PathBuf::from("target").join("mink-artifacts");
    std::fs::create_dir_all(&dir).expect("failed to create target/mink-artifacts");
    dir
}

fn build_and_run(body: &str) -> (i32, String) {
    let source = format!("{}\nfn main() -> Int {{\n{}\n}}\n", zip_source(), body);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("zip_test_{n}.mink"));
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

fn assert_ok(code: i32, out: &str) {
    assert!(code == 0, "exit={code}, out={out}");
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn s44_empty_archive_round_trip() {
    let (c, o) = build_and_run(
        r#"
    let a = zip_new();
    let arc = zip_finish(a);
    if rt_str_len(arc) != 22 { return 1; }
    let (n, back) = zip_read_count(arc);
    if n != 0 { return 2; }
    rt_str_free(back);
    return 0;"#,
    );
    assert_ok(c, &o);
}

#[test]
fn s44_single_stored_entry() {
    let (c, o) = build_and_run(
        r#"
    let a = zip_new();
    let b = zip_add_stored(a, "hi.txt", "hello");
    if zip_count(b) != 1 { return 1; }
    let arc = zip_finish(b);
    let (n, back) = zip_read_count(arc);
    if n != 1 { return 2; }
    let v = zip_list(back);
    let cnt = rt_vec_len(v);
    rt_vec_free(v);
    if cnt != 1 { return 3; }
    return 0;"#,
    );
    assert_ok(c, &o);
}

#[test]
fn s44_deflate_and_stored_multi_entry() {
    let (c, o) = build_and_run(
        r#"
    let a = zip_new();
    let b = zip_add(a, "a.txt", "alpha");
    let c = zip_add_stored(b, "b.bin", "raw");
    let d = zip_add_dir(c, "sub");
    let e = zip_add(d, "sub/c.txt", "nested");
    if zip_count(e) != 4 { return 1; }
    let arc = zip_finish(e);
    let (n, back) = zip_read_count(arc);
    if n != 4 { return 2; }
    let v = zip_list(back);
    let cnt = rt_vec_len(v);
    rt_vec_free(v);
    if cnt != 4 { return 3; }
    return 0;"#,
    );
    assert_ok(c, &o);
}

#[test]
fn s44_entry_data_round_trip() {
    let (c, o) = build_and_run(
        r#"
    let a = zip_add(zip_new(), "x.txt", "hello world");
    let arc = zip_finish(a);
    let (n, back) = zip_read_count(arc);
    if n != 1 { return 1; }
    let r = zip_entry_data(back, 0);
    if r.1 != 0 { return 2; }
    if rt_str_len(r.0) != 11 { return 3; }
    rt_str_free(r.0);
    return 0;"#,
    );
    assert_ok(c, &o);
}

#[test]
fn s44_repeated_build_cycles_leak_free() {
    let (c, o) = build_and_run(
        r#"
    let mut ok = 0;
    while ok < 20 {
        let a = zip_add(zip_new(), "f.txt", "payload");
        let arc = zip_finish(a);
        let (n, back) = zip_read_count(arc);
        rt_str_free(back);
        if n != 1 { return 1; }
        ok = ok + 1;
    }
    return 0;"#,
    );
    assert_ok(c, &o);
}

#[test]
fn s44_deterministic_golden_bytes() {
    let (c, o) = build_and_run(
        r#"
    let a1 = zip_add(zip_new(), "x.txt", "alpha");
    let arc1 = zip_finish(a1);
    let a2 = zip_add(zip_new(), "x.txt", "alpha");
    let arc2 = zip_finish(a2);
    let l1 = rt_str_len(arc1);
    let l2 = rt_str_len(arc2);
    if l1 != l2 { return 1; }
    let mut i = 0;
    while i < l1 {
        if rt_str_byte(arc1, i) != rt_str_byte(arc2, i) { return 2; }
        i = i + 1;
    }
    rt_str_free(arc1);
    rt_str_free(arc2);
    return 0;"#,
    );
    assert_ok(c, &o);
}

#[test]
fn s44_safe_path_predicates() {
    let (c, o) = build_and_run(
        r#"
    if zip_is_safe_path("ok.txt") == false { return 1; }
    if zip_is_safe_path("../bad.txt") == true { return 2; }
    if zip_is_safe_path("a\\..\\b.txt") == true { return 3; }
    if zip_is_safe_path("/abs.txt") == true { return 4; }
    if zip_is_safe_path("C:\\x") == true { return 5; }
    if zip_is_safe_path("a:b") == true { return 6; }
    if zip_is_safe_path("") == true { return 7; }
    return 0;"#,
    );
    assert_ok(c, &o);
}

#[test]
fn s44_empty_payload_round_trip() {
    let (c, o) = build_and_run(
        r#"
    let a = zip_add_stored(zip_new(), "empty.bin", "");
    let arc = zip_finish(a);
    let (n, back) = zip_read_count(arc);
    if n != 1 { return 1; }
    let r = zip_entry_data(back, 0);
    if r.1 != 0 || rt_str_len(r.0) != 0 { return 2; }
    rt_str_free(r.0);
    return 0;"#,
    );
    assert_ok(c, &o);
}

#[test]
fn s44_binary_payload_round_trip() {
    let (c, o) = build_and_run(
        r#"
    let a = zip_add_stored(zip_new(), "bin.dat", "\x00\x01\x02\x03\x04");
    let arc = zip_finish(a);
    let (n, back) = zip_read_count(arc);
    if n != 1 { return 1; }
    let r = zip_entry_data(back, 0);
    if r.1 != 0 || rt_str_len(r.0) != 5 { return 2; }
    if rt_str_byte(r.0, 0) != 0 || rt_str_byte(r.0, 4) != 4 { return 3; }
    rt_str_free(r.0);
    return 0;"#,
    );
    assert_ok(c, &o);
}

#[test]
fn s44_stress_30_entries_capacity_growth() {
    let (c, o) = build_and_run(
        r#"
    let mut a = zip_new();
    let mut i = 0;
    while i < 30 {
        a = zip_add(a, "file.txt", "payload payload payload");
        i = i + 1;
    }
    if zip_count(a) != 30 { return 1; }
    let arc = zip_finish(a);
    let (n, back) = zip_read_count(arc);
    if n != 30 { return 2; }
    let v = zip_list(back);
    let cnt = rt_vec_len(v);
    rt_vec_free(v);
    if cnt != 30 { return 3; }
    return 0;"#,
    );
    assert_ok(c, &o);
}
