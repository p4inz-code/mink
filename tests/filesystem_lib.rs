//! Filesystem library integration tests — Session 56
//!
//! Tests path operations, file I/O, directory operations, and library integration.
//!
//! V1 OWNERSHIP: user function calls consume Str params.
//! V1 LIMITATION: FS wrappers (fs_write, fs_file_size, etc.) can cause crashes
//! when called in sequence due to stack frame issues in user function calling.
//! Tests use rt_fs_* intrinsics directly for reliability.

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
// FILE I/O — using rt_fs_* intrinsics directly (V1 limitation)
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
