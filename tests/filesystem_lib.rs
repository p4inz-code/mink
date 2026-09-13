//! Filesystem library integration tests — Session 56
//!
//! Tests path operations, file I/O, directory operations, and library integration.
//!
//! V1 OWNERSHIP: user function calls consume Str params (callees must free
//! consumed strings; freeing image-region literals is a safe no-op since
//! Session 93, so wrappers can free their parameters unconditionally).
//! Historical note: pre-Session-90 FS wrappers crashed when called in
//! sequence (stack-frame register clobbers). That limitation no longer
//! reproduces as of Session 94 (wrappers in sequence are covered by
//! `fs_wrappers_sequence_write_read_roundtrip` below); the intrinsic-level
//! tests remain for reference and precise diagnostics.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fs_lib() -> String {
    std::fs::read_to_string("stdlib/filesystem.mink")
        .expect("failed to read stdlib/filesystem.mink")
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_fs_test_{n}_{name}.mink"));
    std::fs::write(&path, content.replace("\r\n", "\n")).unwrap();
    path
}

fn build_and_run(test_body: &str) -> (i32, String) {
    let lib = fs_lib();
    let source = format!("{}\n{}", lib, test_body);
    let path = temp_source("test", &source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        return (-1, format!("{}{}", stdout, stderr));
    }
    let run = Command::new(&exe).output().expect("failed to run test exe");
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let code = run.status.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (code, format!("{}\n{}", stdout, stderr))
}

fn first_int(output: &str) -> i64 {
    output
        .lines()
        .next()
        .unwrap_or("0")
        .trim()
        .parse()
        .unwrap_or(0)
}

fn all_ints(output: &str) -> Vec<i64> {
    output
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("runtime error") && !l.contains("memory leak"))
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

fn assert_success(code: i32, out: &str) {
    assert!(
        code == 0 || code == 106,
        "unexpected exit code: {} — {}",
        code,
        out
    );
}

// ============================================================
// PATH JOIN
// ============================================================

#[test]
fn p01_path_join_basic() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let j = path_join("foo", "bar");
    rt_print_int(rt_str_len(j));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 7, "foo/bar = 7: {}", out);
}

#[test]
fn p02_path_join_empty_b() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let j = path_join("hello", "");
    rt_print_int(rt_str_len(j));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 5, "hello = 5: {}", out);
}

#[test]
fn p03_path_join_absolute_b() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let j = path_join("foo", "/bar");
    rt_print_int(rt_str_len(j));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 4, "/bar = 4: {}", out);
}

#[test]
fn p04_path_join_trailing_slash() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let j = path_join("foo/", "bar");
    rt_print_int(rt_str_len(j));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 7, "foo/bar = 7: {}", out);
}

#[test]
fn p32_path_join_empty_a() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let j = path_join("", "bar");
    rt_print_int(rt_str_len(j));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 3, "bar = 3: {}", out);
}

#[test]
fn p33_path_join_both_empty() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let j = path_join("", "");
    rt_print_int(rt_str_len(j));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 0, "empty = 0: {}", out);
}

#[test]
fn p34_path_join_deep() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = path_join("a", "b");
    let j = path_join(a, "c");
    rt_print_int(rt_str_len(j));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 5, "a/b/c = 5: {}", out);
}

// ============================================================
// PATH PARENT
// ============================================================

#[test]
fn p05_path_parent_basic() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let p = path_parent("foo/bar.txt");
    rt_print_int(rt_str_len(p));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 3, "foo = 3: {}", out);
}

#[test]
fn p06_path_parent_no_slash() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let p = path_parent("file.txt");
    rt_print_int(rt_str_len(p));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 1, ". = 1: {}", out);
}

#[test]
fn p07_path_parent_root() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let p = path_parent("/foo");
    rt_print_int(rt_str_len(p));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 1, "/ = 1: {}", out);
}

// ============================================================
// PATH FILENAME
// ============================================================

#[test]
fn p08_path_filename_basic() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let f = path_filename("foo/bar.txt");
    rt_print_int(rt_str_len(f));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 7, "bar.txt = 7: {}", out);
}

#[test]
fn p09_path_filename_no_slash() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let f = path_filename("file.txt");
    rt_print_int(rt_str_len(f));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 8, "file.txt = 8: {}", out);
}

// ============================================================
// PATH EXTENSION
// ============================================================

#[test]
fn p10_path_extension_basic() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let e = path_extension("foo/bar.txt");
    rt_print_int(rt_str_len(e));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 4, ".txt = 4: {}", out);
}

#[test]
fn p11_path_extension_none() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let e = path_extension("Makefile");
    rt_print_int(rt_str_len(e));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 0, "empty = 0: {}", out);
}

// ============================================================
// PATH STEM
// ============================================================

#[test]
fn p12_path_stem_basic() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let s = path_stem("foo/bar.txt");
    rt_print_int(rt_str_len(s));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 7, "bar (stem of bar.txt) = 7: {}", out);
}

// ============================================================
// PATH IS ABSOLUTE / RELATIVE
// ============================================================

#[test]
fn p13_path_is_absolute_slash() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    if path_is_absolute("/foo") { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 1, "/foo is absolute: {}", out);
}

#[test]
fn p14_path_is_absolute_drive() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    if path_is_absolute("C:/foo") { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 1, "C:/foo is absolute: {}", out);
}

#[test]
fn p15_path_is_relative() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    if path_is_relative("foo/bar") { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 1, "foo/bar is relative: {}", out);
}

// ============================================================
// PATH WITH EXTENSION
// ============================================================

#[test]
fn p16_path_with_extension() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let r = path_with_extension("foo/bar.txt", ".rs");
    rt_print_int(rt_str_len(r));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 10, "foo/bar.rs = 10: {}", out);
}

// ============================================================
// PATH CLASSIFICATION
// ============================================================

#[test]
fn p30_path_classification() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    if path_is_absolute("/foo") { rt_print_int(1); } else { rt_print_int(0); }
    if path_is_absolute("C:/foo") { rt_print_int(1); } else { rt_print_int(0); }
    if path_is_absolute("foo/bar") { rt_print_int(1); } else { rt_print_int(0); }
    if path_is_absolute("") { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![1, 1, 0, 0], "absolute: {}", out);
}

#[test]
fn p31_path_has_extension() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    if path_has_extension("foo.txt", ".txt") { rt_print_int(1); } else { rt_print_int(0); }
    if path_has_extension("foo.txt", ".rs") { rt_print_int(1); } else { rt_print_int(0); }
    if path_has_extension("foo", ".txt") { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![1, 0, 0], "has_ext: {}", out);
}

// ============================================================
// FILE I/O — wrapper-level round-trip (see fs_wrappers_sequence test)
// ============================================================

#[test]
fn p20_fs_exists_file() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    if rt_fs_exists("Cargo.toml") { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 1, "exists: {}", out);
}

#[test]
fn p21_fs_exists_nonexistent() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    if rt_fs_exists("no_such_file_xyz.txt") { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 0, "nonexistent: {}", out);
}

#[test]
fn p22_fs_file_size() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let sz = rt_fs_file_size("Cargo.toml");
    rt_print_int(sz);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert!(first_int(&out) > 0, "size > 0: {}", out);
}

#[test]
fn p24_fs_write_read_delete() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let data = rt_str_from_int(42);
    rt_fs_write("test_rw.txt", data);
    let sz = rt_fs_file_size("test_rw.txt");
    rt_print_int(sz);
    let content = rt_fs_read("test_rw.txt");
    let len = rt_str_len(content);
    rt_print_int(len);
    rt_fs_remove_file("test_rw.txt");
    let gone = rt_fs_exists("test_rw.txt");
    if gone { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(
        all_ints(&out),
        vec![2, 2, 0],
        "write/read/delete: {:?}",
        out
    );
}

#[test]
fn p25_fs_write_read_large() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let part1 = rt_str_from_int(111);
    let part2 = rt_str_from_int(222);
    let data = rt_str_alloc(7);
    let mut i = 0;
    while i < 3 {
        rt_str_set_byte(data, i, rt_str_byte(part1, i));
        i = i + 1;
    }
    rt_str_set_byte(data, 3, 45);
    i = 0;
    while i < 3 {
        rt_str_set_byte(data, 4 + i, rt_str_byte(part2, i));
        i = i + 1;
    }
    rt_fs_write("test_large.txt", data);
    let content = rt_fs_read("test_large.txt");
    rt_print_int(rt_str_len(content));
    rt_fs_remove_file("test_large.txt");
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 7, "111-222 = 7: {}", out);
}

#[test]
fn p26_fs_copy_file() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let data = rt_str_from_int(99);
    rt_fs_write("test_cpy_src.txt", data);
    let c = rt_fs_copy("test_cpy_src.txt", "test_cpy_dst.txt");
    rt_print_int(c);
    let sz = rt_fs_file_size("test_cpy_dst.txt");
    rt_print_int(sz);
    rt_fs_remove_file("test_cpy_src.txt");
    rt_fs_remove_file("test_cpy_dst.txt");
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![0, 2], "copy: {}", out);
}

#[test]
fn p27_fs_move_file() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let data = rt_str_from_int(77);
    rt_fs_write("test_mv_src.txt", data);
    let m = rt_fs_move("test_mv_src.txt", "test_mv_dst.txt");
    rt_print_int(m);
    if rt_fs_exists("test_mv_src.txt") { rt_print_int(1); } else { rt_print_int(0); }
    if rt_fs_exists("test_mv_dst.txt") { rt_print_int(1); } else { rt_print_int(0); }
    rt_fs_remove_file("test_mv_dst.txt");
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![0, 0, 1], "move: {}", out);
}

#[test]
fn p28_fs_create_remove_dir() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let d = rt_fs_create_dir("test_mkdir_s56");
    rt_print_int(d);
    if rt_fs_exists("test_mkdir_s56") { rt_print_int(1); } else { rt_print_int(0); }
    let r = rt_fs_remove_dir("test_mkdir_s56");
    rt_print_int(r);
    if rt_fs_exists("test_mkdir_s56") { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![0, 1, 0, 0], "mkdir: {}", out);
}

#[test]
fn p42_fs_get_cwd() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let cwd = rt_fs_get_cwd();
    let len = rt_str_len(cwd);
    rt_print_int(len);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert!(first_int(&out) > 0, "cwd length > 0: {}", out);
}

#[test]
fn p43_fs_write_read_cycles() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let d1 = rt_str_from_int(1);
    rt_fs_write("test_cycles.txt", d1);
    let r1 = rt_fs_read("test_cycles.txt");
    rt_print_int(rt_str_len(r1));
    let d2 = rt_str_from_int(22);
    rt_fs_write("test_cycles.txt", d2);
    let r2 = rt_fs_read("test_cycles.txt");
    rt_print_int(rt_str_len(r2));
    let d3 = rt_str_from_int(333);
    rt_fs_write("test_cycles.txt", d3);
    let r3 = rt_fs_read("test_cycles.txt");
    rt_print_int(rt_str_len(r3));
    rt_fs_remove_file("test_cycles.txt");
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![1, 2, 3], "cycles: {}", out);
}

#[test]
fn p44_path_join_multi_segment() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = path_join("src", "lib");
    let b = path_join(a, "core");
    let c = path_join(b, "main.mink");
    rt_print_int(rt_str_len(c));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 22, "src/lib/core/main.mink = 22: {}", out);
}

#[test]
fn p45_fs_many_operations() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let data = rt_str_from_int(42);
    rt_fs_write("test_many.txt", data);
    let s1 = rt_fs_file_size("test_many.txt");
    rt_print_int(s1);
    let data2 = rt_str_from_int(9999);
    rt_fs_write("test_many.txt", data2);
    let s2 = rt_fs_file_size("test_many.txt");
    rt_print_int(s2);
    rt_fs_remove_file("test_many.txt");
    let s3 = rt_fs_file_size("test_many.txt");
    rt_print_int(s3);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let vals = all_ints(&out);
    assert_eq!(vals[0], 2, "size 1: {}", out);
    assert_eq!(vals[1], 4, "size 2: {}", out);
}
// ---------------------------------------------------------------------------
// Session 94: wrapper-level file I/O round-trip. The pre-Session-90 stack
// frame issue that made fs_* wrappers crash in sequence no longer
// reproduces; this test locks the user-facing wrapper path.
// ---------------------------------------------------------------------------

#[test]
fn fs_wrappers_sequence_write_read_roundtrip() {
    let dir = std::env::temp_dir().join(format!("mink_fs_s94_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("w.txt").to_str().unwrap().replace('\\', "/");
    let body = format!(
        r#"
fn main() {{
    let w = fs_write("{path}", "wrapper-roundtrip");
    let r = fs_read("{path}");
    let l = rt_str_len(r);
    let mut good = true;
    if w != 17 {{ good = false; }}
    if l != 17 {{ good = false; }}
    let exp = "wrapper-roundtrip";
    let mut i = 0;
    while i < l {{
        if rt_str_byte(r, i) != rt_str_byte(exp, i) {{ good = false; }}
        i = i + 1;
    }}
    rt_str_free(r);
    let d = fs_remove_file("{path}");
    if good == true {{
        if d == 0 || d == 1 {{ rt_exit(0); }}
    }}
    rt_exit(1);
}}
"#
    );
    let (code, out) = build_and_run(&body);
    let _ = std::fs::remove_dir_all(&dir);
    assert_success(code, &out);
    assert_eq!(code, 0, "wrapper sequence must exit 0: {out}");
}

// ---------------------------------------------------------------------------
// Session 108 (S28): directory enumeration — os.scandir()/os.listdir() parity.
// ---------------------------------------------------------------------------

/// Creates a fixture tree under the temp directory:
///   `<dir>/a.txt`, `<dir>/b.txt`, `<dir>/sub/`, `<dir>/empty/`,
///   `<dir>/with space/inner.txt`
/// and returns the root path rendered with forward slashes.
fn dir_fixture(tag: &str) -> (std::path::PathBuf, String) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mink_fs_dir_{tag}_{}_{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::create_dir_all(dir.join("empty")).unwrap();
    std::fs::create_dir_all(dir.join("with space")).unwrap();
    std::fs::write(dir.join("a.txt"), b"a").unwrap();
    std::fs::write(dir.join("b.txt"), b"b").unwrap();
    std::fs::write(dir.join("with space").join("inner.txt"), b"i").unwrap();
    let rendered = dir.to_str().unwrap().replace('\\', "/");
    (dir, rendered)
}

/// MINK helper: count the entries the enumeration yields, or -1 when the
/// directory cannot be opened. Frees every name and closes the handle.
const DIR_COUNT_HELPER: &str = r#"
fn count_entries(dir: Str) -> Int {
    let h = fs_dir_open(dir);
    if h == 0 { return -1; }
    let mut n = 0;
    let mut done = 0;
    while done == 0 {
        let name = fs_dir_next(h);
        if rt_str_len(name) == 0 { done = 1; } else { n = n + 1; }
        rt_str_free(name);
    }
    fs_dir_close(h);
    return n;
}
"#;

#[test]
fn d01_dir_enumerates_entries() {
    let (dir, root) = dir_fixture("count");
    let body = format!(
        r#"
fn main() {{
    rt_print_int(count_entries("{root}"));
    rt_exit(0);
}}
"#
    );
    let (code, out) = build_and_run(&format!("{DIR_COUNT_HELPER}\n{body}"));
    let _ = std::fs::remove_dir_all(&dir);
    assert_success(code, &out);
    assert_eq!(first_int(&out), 5, "5 entries (dot entries skipped): {out}");
}

#[test]
fn d02_dir_empty_directory() {
    let (dir, root) = dir_fixture("empty");
    let body = format!(
        r#"
fn main() {{
    rt_print_int(count_entries("{root}/empty"));
    rt_exit(0);
}}
"#
    );
    let (code, out) = build_and_run(&format!("{DIR_COUNT_HELPER}\n{body}"));
    let _ = std::fs::remove_dir_all(&dir);
    assert_success(code, &out);
    assert_eq!(first_int(&out), 0, "empty directory: {out}");
}

#[test]
fn d03_dir_open_missing_returns_zero() {
    let body = r#"
fn main() {
    let h = fs_dir_open("no_such_directory_xyz_108");
    rt_print_int(h);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(body);
    assert_success(code, &out);
    assert_eq!(first_int(&out), 0, "missing directory opens as 0: {out}");
}

#[test]
fn d04_dir_null_handle_is_inert() {
    let body = r#"
fn main() {
    let s = fs_dir_next(0);
    rt_print_int(rt_str_len(s));
    rt_str_free(s);
    rt_print_int(fs_dir_close(0));
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(body);
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![0, -1], "null handle: {out}");
}

#[test]
fn d05_dir_trailing_separator() {
    let (dir, root) = dir_fixture("slash");
    let body = format!(
        r#"
fn main() {{
    rt_print_int(count_entries("{root}/"));
    rt_exit(0);
}}
"#
    );
    let (code, out) = build_and_run(&format!("{DIR_COUNT_HELPER}\n{body}"));
    let _ = std::fs::remove_dir_all(&dir);
    assert_success(code, &out);
    assert_eq!(first_int(&out), 5, "trailing separator: {out}");
}

#[test]
fn d06_dir_path_with_spaces() {
    let (dir, root) = dir_fixture("spaces");
    let body = format!(
        r#"
fn main() {{
    rt_print_int(count_entries("{root}/with space"));
    rt_exit(0);
}}
"#
    );
    let (code, out) = build_and_run(&format!("{DIR_COUNT_HELPER}\n{body}"));
    let _ = std::fs::remove_dir_all(&dir);
    assert_success(code, &out);
    assert_eq!(first_int(&out), 1, "path with spaces: {out}");
}

#[test]
fn d07_dir_two_handles_interleaved() {
    let (dir, root) = dir_fixture("interleave");
    let body = format!(
        r#"
fn main() {{
    let h1 = fs_dir_open("{root}");
    let h2 = fs_dir_open("{root}/empty");
    let a = fs_dir_next(h1);
    let b = fs_dir_next(h2);
    rt_print_int(rt_str_len(b));
    rt_str_free(b);
    let alen = rt_str_len(a);
    rt_str_free(a);
    if alen > 0 {{ rt_print_int(1); }} else {{ rt_print_int(0); }}
    let mut n = 1;
    let mut done = 0;
    while done == 0 {{
        let x = fs_dir_next(h1);
        if rt_str_len(x) == 0 {{ done = 1; }} else {{ n = n + 1; }}
        rt_str_free(x);
    }}
    rt_print_int(n);
    rt_print_int(fs_dir_close(h1));
    rt_print_int(fs_dir_close(h2));
    rt_exit(0);
}}
"#
    );
    let (code, out) = build_and_run(&body);
    let _ = std::fs::remove_dir_all(&dir);
    assert_success(code, &out);
    assert_eq!(
        all_ints(&out),
        vec![0, 1, 5, 0, 0],
        "interleaved handles: {out}"
    );
}

#[test]
fn d08_dir_repeated_open_close_stress() {
    let (dir, root) = dir_fixture("stress");
    let body = format!(
        r#"
fn main() {{
    let mut bad = 0;
    let mut i = 0;
    while i < 300 {{
        let h = fs_dir_open("{root}");
        if h == 0 {{ bad = bad + 1; }}
        let mut c = 0;
        let mut done = 0;
        while done == 0 {{
            let nm = fs_dir_next(h);
            if rt_str_len(nm) == 0 {{ done = 1; }} else {{ c = c + 1; }}
            rt_str_free(nm);
        }}
        if c != 5 {{ bad = bad + 1; }}
        let r = fs_dir_close(h);
        if r != 0 {{ bad = bad + 1; }}
        i = i + 1;
    }}
    rt_print_int(bad);
    rt_exit(0);
}}
"#
    );
    let (code, out) = build_and_run(&body);
    let _ = std::fs::remove_dir_all(&dir);
    assert_success(code, &out);
    assert_eq!(first_int(&out), 0, "300 open/enumerate/close cycles: {out}");
}

#[test]
fn d09_dir_recursive_walk() {
    let (dir, root) = dir_fixture("walk");
    let body = format!(
        r#"
fn main() {{
    let h = fs_dir_open("{root}");
    if h == 0 {{ rt_print_int(-1); rt_exit(1); }}
    let mut n = 0;
    let mut done = 0;
    while done == 0 {{
        let name = fs_dir_next(h);
        let l = rt_str_len(name);
        if l == 0 {{ done = 1; }} else {{ n = n + 1; }}
        // Build `<root>/<name>` and descend one level. Directories are
        // enumerated; file paths fail to open and contribute nothing.
        let child = path_join(path_join("{root}", ""), name);
        if l != 0 {{
            let ch = rt_dir_open(child);
            let mut cdone = 0;
            while cdone == 0 {{
                let cn = rt_dir_next(ch);
                if rt_str_len(cn) == 0 {{ cdone = 1; }} else {{ n = n + 1; }}
                rt_str_free(cn);
            }}
            rt_dir_close(ch);
        }}
        rt_str_free(child);
    }}
    fs_dir_close(h);
    rt_print_int(n);
    rt_exit(0);
}}
"#
    );
    let (code, out) = build_and_run(&body);
    let _ = std::fs::remove_dir_all(&dir);
    assert_success(code, &out);
    assert_eq!(
        first_int(&out),
        6,
        "walk counts 5 root entries plus `with space/inner.txt`: {out}"
    );
}

#[test]
fn d10_dir_many_entries() {
    let (dir, root) = dir_fixture("many");
    for i in 0..40 {
        std::fs::write(dir.join(format!("f{i:02}.dat")), b"x").unwrap();
    }
    let body = format!(
        r#"
fn main() {{
    rt_print_int(count_entries("{root}"));
    rt_exit(0);
}}
"#
    );
    let (code, out) = build_and_run(&format!("{DIR_COUNT_HELPER}\n{body}"));
    let _ = std::fs::remove_dir_all(&dir);
    assert_success(code, &out);
    assert_eq!(first_int(&out), 45, "40 files + 5 fixture entries: {out}");
}

/// Ownership: an enumeration handle that is never closed is a live
/// allocation, so the leak checker must reject the program (E-R06,
/// exit 106). This proves `fs_dir_open` participates in MINK's ownership
/// model instead of leaking silently.
#[test]
fn d11_dir_unclosed_handle_is_a_leak() {
    let (dir, root) = dir_fixture("leak");
    let body = format!(
        r#"
fn main() {{
    let h = fs_dir_open("{root}");
    rt_print_int(h);
    rt_exit(0);
}}
"#
    );
    let (code, out) = build_and_run(&body);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(code, 106, "unclosed handle must leak-check: {out}");
    assert!(out.contains("E-R06"), "expected E-R06 leak report: {out}");
}
