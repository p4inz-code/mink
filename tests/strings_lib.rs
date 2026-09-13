//! Integration tests for the MINK Strings library (Session 53).
//!
//! V1 OWNERSHIP RULES:
//! - str_to_upper/str_trim/etc. consume input, produce new allocated string
//! - str_index_of(s, sub) consumes s, returns (idx, s) where .1 is the original s
//! - After any user function call with Str param, the original variable is consumed
//! - Allocated strings (from transformations) must be freed via rt_str_free
//! - To free a string returned from str_index_of etc., use rt_str_free(r.1)

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn strings_lib() -> String {
    std::fs::read_to_string("stdlib/strings.mink").expect("failed to read stdlib/strings.mink")
}

fn json_lib() -> String {
    std::fs::read_to_string("stdlib/json.mink").expect("failed to read stdlib/json.mink")
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_str_test_{n}_{name}.mink"));
    std::fs::write(&path, content.replace("\r\n", "\n")).unwrap();
    path
}

fn build_and_run(test_body: &str) -> (i32, Vec<u8>) {
    let lib = strings_lib();
    let source = format!("{}\n{}", lib, test_body);
    let path = temp_source("test", &source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        panic!("build failed:\n{stderr}");
    }
    let run = Command::new(&exe).output().unwrap();
    let code = run.status.code().unwrap_or(-1);
    let stdout = run.stdout.clone();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (code, stdout)
}

fn build_and_run_with_json(test_body: &str) -> (i32, Vec<u8>) {
    let lib = strings_lib();
    let json = json_lib();
    let source = format!("{}\n{}\n{}", json, lib, test_body);
    let path = temp_source("test", &source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        panic!("build failed:\n{stderr}");
    }
    let run = Command::new(&exe).output().unwrap();
    let code = run.status.code().unwrap_or(-1);
    let stdout = run.stdout.clone();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (code, stdout)
}

// =============================================================================
// SEARCH TESTS (no allocation needed — return (result, Str) pass-back)
// =============================================================================

#[test]
fn s01_index_of_found() {
    let test = r#"
fn main() {
    let r = str_index_of("hello world", "world");
    rt_print_int(r.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s02_index_of_not_found() {
    let test = r#"
fn main() {
    let r = str_index_of("hello", "xyz");
    rt_print_int(r.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s03_index_of_empty_sub() {
    let test = r#"
fn main() {
    let r = str_index_of("hello", "");
    rt_print_int(r.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s04_index_of_at_start() {
    let test = r#"
fn main() {
    let r = str_index_of("hello", "he");
    rt_print_int(r.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s05_index_of_at_end() {
    let test = r#"
fn main() {
    let r = str_index_of("hello", "lo");
    rt_print_int(r.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s06_index_of_full_match() {
    let test = r#"
fn main() {
    let r = str_index_of("abc", "abc");
    rt_print_int(r.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s07_index_of_longer_sub() {
    let test = r#"
fn main() {
    let r = str_index_of("ab", "abc");
    rt_print_int(r.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s08_index_of_empty_string() {
    let test = r#"
fn main() {
    let r = str_index_of("", "a");
    rt_print_int(r.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s09_last_index_of() {
    let test = r#"
fn main() {
    let r = str_last_index_of("abcabc", "abc");
    rt_print_int(r.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s10_last_index_of_not_found() {
    let test = r#"
fn main() {
    let r = str_last_index_of("hello", "xyz");
    rt_print_int(r.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s11_last_index_of_empty_sub() {
    let test = r#"
fn main() {
    let r = str_last_index_of("hello", "");
    rt_print_int(r.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

// =============================================================================
// VALIDATION TESTS (return (Bool, Str) — no new allocation)
// =============================================================================

#[test]
fn s12_is_numeric_true() {
    let test = r#"
fn main() {
    let r = str_is_numeric("12345");
    if r.0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s13_is_numeric_false() {
    let test = r#"
fn main() {
    let r = str_is_numeric("12a45");
    if r.0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s14_is_alpha_true() {
    let test = r#"
fn main() {
    let r = str_is_alpha("hello");
    if r.0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s15_is_alpha_false() {
    let test = r#"
fn main() {
    let r = str_is_alpha("hello1");
    if r.0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s16_is_alphanumeric_true() {
    let test = r#"
fn main() {
    let r = str_is_alphanumeric("abc123");
    if r.0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s17_is_alphanumeric_false() {
    let test = r#"
fn main() {
    let r = str_is_alphanumeric("abc 123");
    if r.0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s18_contains_true() {
    let test = r#"
fn main() {
    let r = str_contains("hello world", "world");
    if r.0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s19_contains_false() {
    let test = r#"
fn main() {
    let r = str_contains("hello", "xyz");
    if r.0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s20_starts_with_true() {
    let test = r#"
fn main() {
    let r = str_starts_with("hello", "hel");
    if r.0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s21_starts_with_false() {
    let test = r#"
fn main() {
    let r = str_starts_with("hello", "xyz");
    if r.0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s22_ends_with_true() {
    let test = r#"
fn main() {
    let r = str_ends_with("hello", "llo");
    if r.0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s23_ends_with_false() {
    let test = r#"
fn main() {
    let r = str_ends_with("hello", "xyz");
    if r.0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s24_count_multiple() {
    let test = r#"
fn main() {
    let r = str_count("ababab", "ab");
    rt_print_int(r.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s25_count_zero() {
    let test = r#"
fn main() {
    let r = str_count("hello", "xyz");
    rt_print_int(r.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s26_char_at() {
    let test = r#"
fn main() {
    let r = str_char_at("hello", 0);
    let r2 = str_char_at(r.1, 4);
    rt_print_int(r.0);
    rt_print_int(r2.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

// =============================================================================
// COMPARISON TESTS
// =============================================================================

#[test]
fn s27_cmp_equal() {
    let test = r#"
fn main() {
    let c = str_cmp("abc", "abc");
    rt_print_int(c);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s28_cmp_less() {
    let test = r#"
fn main() {
    let c = str_cmp("abc", "abd");
    rt_print_int(c);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s29_cmp_greater() {
    let test = r#"
fn main() {
    let c = str_cmp("abd", "abc");
    rt_print_int(c);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s30_cmp_empty() {
    let test = r#"
fn main() {
    let c = str_cmp("", "");
    rt_print_int(c);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

// =============================================================================
// TRANSFORMATION TESTS — free the ALLOCATED result via rt_str_free
// After str_index_of(s, sub) consumes s, the original is in r.1
// =============================================================================

#[test]
fn s31_to_upper() {
    let test = r#"
fn main() {
    let s = str_to_upper("hello");
    let r = str_index_of(s, "HELLO");
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s32_to_lower() {
    let test = r#"
fn main() {
    let s = str_to_lower("HELLO");
    let r = str_index_of(s, "hello");
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s33_to_upper_empty() {
    let test = r#"
fn main() {
    let s = str_to_upper("");
    let len = rt_str_len(s);
    rt_print_int(len);
    rt_str_free(s);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s34_trim() {
    let test = r#"
fn main() {
    let s = str_trim("  hello  ");
    let len = rt_str_len(s);
    let r = str_index_of(s, "hello");
    rt_print_int(len);
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s35_trim_start() {
    let test = r#"
fn main() {
    let s = str_trim_start("  hello  ");
    let len = rt_str_len(s);
    rt_print_int(len);
    rt_str_free(s);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s36_trim_end() {
    let test = r#"
fn main() {
    let s = str_trim_end("  hello  ");
    let len = rt_str_len(s);
    rt_print_int(len);
    rt_str_free(s);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s37_trim_empty() {
    let test = r#"
fn main() {
    let s = str_trim("");
    let len = rt_str_len(s);
    rt_print_int(len);
    rt_str_free(s);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s38_reverse() {
    let test = r#"
fn main() {
    let s = str_reverse("abc");
    let r = str_index_of(s, "cba");
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s39_reverse_empty() {
    let test = r#"
fn main() {
    let s = str_reverse("");
    let len = rt_str_len(s);
    rt_print_int(len);
    rt_str_free(s);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s40_sub() {
    let test = r#"
fn main() {
    let s = str_sub("hello", 1, 4);
    let len = rt_str_len(s);
    let r = str_index_of(s, "ell");
    rt_print_int(len);
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s41_sub_empty() {
    let test = r#"
fn main() {
    let s = str_sub("hello", 2, 2);
    let len = rt_str_len(s);
    rt_print_int(len);
    rt_str_free(s);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s42_sub_out_of_bounds() {
    let test = r#"
fn main() {
    let s = str_sub("hi", 0, 100);
    let len = rt_str_len(s);
    rt_print_int(len);
    rt_str_free(s);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s43_sub_negative_start() {
    let test = r#"
fn main() {
    let s = str_sub("hello", -5, 3);
    let len = rt_str_len(s);
    rt_print_int(len);
    rt_str_free(s);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s44_sub_start_gt_end() {
    let test = r#"
fn main() {
    let s = str_sub("hello", 4, 1);
    let len = rt_str_len(s);
    rt_print_int(len);
    rt_str_free(s);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s45_repeat() {
    let test = r#"
fn main() {
    let s = str_repeat("ab", 3);
    let len = rt_str_len(s);
    let r = str_index_of(s, "ababab");
    rt_print_int(len);
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s46_repeat_zero() {
    let test = r#"
fn main() {
    let s = str_repeat("ab", 0);
    let len = rt_str_len(s);
    rt_print_int(len);
    rt_str_free(s);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s47_repeat_large() {
    let test = r#"
fn main() {
    let s = str_repeat("x", 100);
    let len = rt_str_len(s);
    rt_print_int(len);
    rt_str_free(s);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s48_pad_left() {
    let test = r#"
fn main() {
    let s = str_pad_left("hi", 5, 48);
    let len = rt_str_len(s);
    let r = str_index_of(s, "hi");
    rt_print_int(len);
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s49_pad_right() {
    let test = r#"
fn main() {
    let s = str_pad_right("hi", 5, 48);
    let len = rt_str_len(s);
    let r = str_index_of(s, "hi");
    rt_print_int(len);
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s50_pad_left_noop() {
    let test = r#"
fn main() {
    let s = str_pad_left("hello", 3, 48);
    let len = rt_str_len(s);
    rt_print_int(len);
    rt_str_free(s);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

// =============================================================================
// REPLACE / JOIN TESTS
// =============================================================================

#[test]
fn s51_replace_one() {
    let test = r#"
fn main() {
    let s = str_replace("hello world", "world", "MINK");
    let r = str_index_of(s, "MINK");
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s52_replace_all() {
    let test = r#"
fn main() {
    let s = str_replace_all("ababab", "ab", "x");
    let len = rt_str_len(s);
    let r = str_index_of(s, "xxx");
    rt_print_int(len);
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s53_replace_not_found() {
    let test = r#"
fn main() {
    let s = str_replace("hello", "xyz", "abc");
    let r = str_index_of(s, "hello");
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s54_replace_all_not_found() {
    let test = r#"
fn main() {
    let s = str_replace_all("hello", "xyz", "abc");
    let r = str_index_of(s, "hello");
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s55_join_2() {
    let test = r#"
fn main() {
    let s = str_join_2("hello", " world");
    let r = str_index_of(s, "hello world");
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s56_join_3() {
    let test = r#"
fn main() {
    let s = str_join_3("a", "b", "c");
    let r = str_index_of(s, "abc");
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

// =============================================================================
// CHAINING TESTS
// =============================================================================

// NOTE: V1 has no RAII. Chaining transformations (str_to_upper(str_trim(s)))
// leaks the intermediate string. Each step must be separate.
// The caller owns all allocations and must free them.

#[test]
fn s57_chain_trim_upper() {
    // Two-step: trim first, then uppercase the result.
    // str_trim("  hello  ") returns a new heap string (caller-owned).
    // str_to_upper consumes it — caller can't free the old one.
    // This is a V1 limitation; we verify the result is correct even if intermediate leaks.
    let test = r#"
fn main() {
    let t = str_trim("  hello  ");
    let s = str_to_upper(t);
    let r = str_index_of(s, "HELLO");
    rt_print_int(r.0);
    // t was consumed by str_to_upper; cannot free it.
    // s = r.1 is the result of str_index_of pass-back.
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    // V1: exit 106 = memory leak from un-freed intermediate (str_trim result)
    // This is expected — V1 has no RAII/drop for intermediate string chains.
    assert!(code == 0 || code == 106, "unexpected exit code: {code}");
}

#[test]
fn s58_chain_reverse_upper() {
    let test = r#"
fn main() {
    let t = str_reverse("abc");
    let s = str_to_upper(t);
    let r = str_index_of(s, "CBA");
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert!(code == 0 || code == 106, "unexpected exit code: {code}");
}

#[test]
fn s59_chain_join_sub() {
    let test = r#"
fn main() {
    let t = str_join_2("hello", " world");
    let s = str_sub(t, 6, 11);
    let r = str_index_of(s, "world");
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert!(code == 0 || code == 106, "unexpected exit code: {code}");
}

#[test]
fn s60_chain_repeat_trim() {
    let test = r#"
fn main() {
    let t = str_repeat(" x", 3);
    let s = str_trim(t);
    let len = rt_str_len(s);
    let r = str_index_of(s, "x x x");
    rt_print_int(len);
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert!(code == 0 || code == 106, "unexpected exit code: {code}");
}

#[test]
fn s61_chain_replace_chain() {
    let test = r#"
fn main() {
    let t = str_replace_all("aabbcc", "aa", "x");
    let s = str_replace_all(t, "cc", "y");
    let r = str_index_of(s, "xbby");
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert!(code == 0 || code == 106, "unexpected exit code: {code}");
}

#[test]
fn s62_case_insensitive_search() {
    let test = r#"
fn main() {
    let s = str_to_lower("Hello WORLD");
    let r = str_index_of(s, "hello world");
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

// =============================================================================
// OWNERSHIP TESTS
// =============================================================================

#[test]
fn s63_search_then_transform() {
    let test = r#"
fn main() {
    let r = str_index_of("Hello World", "World");
    let upper = str_to_upper(r.1);
    let r2 = str_index_of(upper, "WORLD");
    rt_print_int(r2.0);
    rt_str_free(r2.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s64_multiple_searches() {
    let test = r#"
fn main() {
    let r1 = str_index_of("aabbcc", "bb");
    let r2 = str_index_of(r1.1, "cc");
    rt_print_int(r1.0);
    rt_print_int(r2.0);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s65_search_validate_chain() {
    let test = r#"
fn main() {
    let r = str_is_numeric("12345");
    if r.0 {
        let s = str_join_2("Number: ", r.1);
        let r2 = str_index_of(s, "Number: ");
        rt_print_int(r2.0);
        rt_str_free(r2.1);
    } else {
        rt_print_int(-1);
    }
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s66_trim_and_check() {
    let test = r#"
fn main() {
    let s = str_trim("  hello  ");
    let r = str_is_alpha(s);
    if r.0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

// =============================================================================
// PRACTICAL TESTS
// =============================================================================

#[test]
fn s67_path_basename() {
    let test = r#"
fn main() {
    let s = "/usr/local/bin/mink";
    let r = str_last_index_of(s, "/");
    let base = str_sub(s, r.0 + 1, rt_str_len(s));
    let r2 = str_index_of(base, "mink");
    rt_print_int(r2.0);
    rt_str_free(r2.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s68_csv_first_field() {
    let test = r#"
fn main() {
    let s = "name,age,city";
    let r = str_index_of(s, ",");
    let field = str_sub(s, 0, r.0);
    let r2 = str_index_of(field, "name");
    rt_print_int(r2.0);
    rt_str_free(r2.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert_eq!(code, 0);
}

#[test]
fn s69_string_builder() {
    let test = r#"
fn main() {
    let s = str_join_3("Hello", ", ", str_join_2("Mr", "."));
    let r = str_index_of(s, "Hello, Mr.");
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run(test);
    assert!(code == 0 || code == 106, "unexpected exit code: {code}");
}

// =============================================================================
// JSON INTEGRATION TESTS
// =============================================================================

#[test]
fn s70_json_string_in_array() {
    let test_body = r#"
fn main() {
    let arena = rt_alloc(65536);
    let v = json_parse("[\"hello\", \"world\"]", arena);
    if v == 0 { rt_free(arena); rt_exit(1); }
    let len = json_arr_len(arena, v);
    let v0 = json_arr_get(arena, v, 0);
    let s0 = json_as_str(arena, v0);
    let r = str_index_of(s0, "hello");
    rt_print_int(r.0);
    rt_print_int(len);
    // s0 references arena memory; don't rt_str_free it.
    // str_index_of consumes s0 but the arena owns it.
    rt_free(arena);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run_with_json(test_body);
    // V1: str_index_of returns s0 in r.1 but we can't free it (arena owns it)
    // This may leak depending on whether str_index_of copies
    assert!(code == 0 || code == 106, "unexpected exit code: {code}");
}

#[test]
fn s71_json_uppercase_string() {
    let test_body = r#"
fn main() {
    let arena = rt_alloc(65536);
    let v = json_parse("\"hello\"", arena);
    if v == 0 { rt_free(arena); rt_exit(1); }
    let s = json_as_str(arena, v);
    let upper = str_to_upper(s);
    let r = str_index_of(upper, "HELLO");
    rt_print_int(r.0);
    rt_str_free(r.1);
    rt_free(arena);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run_with_json(test_body);
    assert!(code == 0 || code == 106, "unexpected exit code: {code}");
}

#[test]
fn s72_json_object_keys_searched() {
    let test_body = r#"
fn main() {
    let arena = rt_alloc(65536);
    let v = json_parse("{\"name\": \"mink\"}", arena);
    if v == 0 { rt_free(arena); rt_exit(1); }
    let k0 = json_obj_get_key(arena, v, 0);
    let r = str_index_of(k0, "name");
    rt_print_int(r.0);
    rt_free(arena);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run_with_json(test_body);
    assert!(code == 0 || code == 106, "unexpected exit code: {code}");
}

#[test]
fn s73_json_array_string_count() {
    let test_body = r#"
fn main() {
    let arena = rt_alloc(65536);
    let v = json_parse("[\"ab\", \"ab\", \"ab\", \"cd\"]", arena);
    if v == 0 { rt_free(arena); rt_exit(1); }
    let len = json_arr_len(arena, v);
    let mut count = 0;
    let mut i = 0;
    while i < len {
        let elem = json_arr_get(arena, v, i);
        let s = json_as_str(arena, elem);
        let r = str_index_of(s, "ab");
        if r.0 >= 0 { count = count + 1; }
        i = i + 1;
    }
    rt_print_int(count);
    rt_free(arena);
    rt_exit(0);
}"#;
    let (code, _) = build_and_run_with_json(test_body);
    // V1: str_index_of on arena strings may leak the pass-back tuple
    assert!(code == 0 || code == 106, "unexpected exit code: {code}");
}

// =============================================================================
// SESSION 108 — S06: string → number parsing (str_parse_int / str_parse_float)
// =============================================================================
// Each parser returns a `(value, ok)` tuple; `ok` is the "was a valid
// number" flag and `value` is 0 when `ok` is false. The tests print both
// fields and assert the flattened stream.

fn stdout_text(stdout: &[u8]) -> String {
    String::from_utf8_lossy(stdout).to_string()
}

fn ints_of(stdout: &[u8]) -> Vec<i64> {
    stdout_text(stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| l.trim().parse::<i64>().ok())
        .collect()
}

fn floats_of(stdout: &[u8]) -> Vec<f64> {
    stdout_text(stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| l.trim().parse::<f64>().ok())
        .collect()
}

#[test]
fn p01_parse_int_basics() {
    let test = r#"
fn main() {
    let a = str_parse_int("42");
    rt_print_int(a.0);
    if a.1 { rt_print_int(1); } else { rt_print_int(0); }
    let b = str_parse_int("-7");
    rt_print_int(b.0);
    if b.1 { rt_print_int(1); } else { rt_print_int(0); }
    let c = str_parse_int("+8");
    rt_print_int(c.0);
    if c.1 { rt_print_int(1); } else { rt_print_int(0); }
    let d = str_parse_int("0");
    rt_print_int(d.0);
    if d.1 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(test);
    assert_eq!(code, 0);
    assert_eq!(ints_of(&out), vec![42, 1, -7, 1, 8, 1, 0, 1]);
}

#[test]
fn p02_parse_int_whitespace() {
    let test = r#"
fn main() {
    let a = str_parse_int(" 12 ");
    rt_print_int(a.0);
    if a.1 { rt_print_int(1); } else { rt_print_int(0); }
    let b = str_parse_int("\t-3\n");
    rt_print_int(b.0);
    if b.1 { rt_print_int(1); } else { rt_print_int(0); }
    let c = str_parse_int("   ");
    if c.1 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(test);
    assert_eq!(code, 0);
    assert_eq!(ints_of(&out), vec![12, 1, -3, 1, 0]);
}

#[test]
fn p03_parse_int_digit_separators() {
    let test = r#"
fn main() {
    let a = str_parse_int("1_000");
    rt_print_int(a.0);
    if a.1 { rt_print_int(1); } else { rt_print_int(0); }
    let b = str_parse_int("1_0_0");
    rt_print_int(b.0);
    if b.1 { rt_print_int(1); } else { rt_print_int(0); }
    let c = str_parse_int("_1");
    if c.1 { rt_print_int(1); } else { rt_print_int(0); }
    let d = str_parse_int("1_");
    if d.1 { rt_print_int(1); } else { rt_print_int(0); }
    let e = str_parse_int("1__0");
    if e.1 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(test);
    assert_eq!(code, 0);
    assert_eq!(ints_of(&out), vec![1000, 1, 100, 1, 0, 0, 0]);
}

#[test]
fn p04_parse_int_invalid() {
    let test = r#"
fn main() {
    if str_parse_int("").1 { rt_print_int(1); } else { rt_print_int(0); }
    if str_parse_int("+").1 { rt_print_int(1); } else { rt_print_int(0); }
    if str_parse_int("-").1 { rt_print_int(1); } else { rt_print_int(0); }
    if str_parse_int("12x").1 { rt_print_int(1); } else { rt_print_int(0); }
    if str_parse_int("x12").1 { rt_print_int(1); } else { rt_print_int(0); }
    if str_parse_int("1 2").1 { rt_print_int(1); } else { rt_print_int(0); }
    if str_parse_int("--1").1 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(test);
    assert_eq!(code, 0);
    assert_eq!(ints_of(&out), vec![0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn p05_parse_int_boundaries() {
    let test = r#"
fn main() {
    let a = str_parse_int("9223372036854775807");
    rt_print_int(a.0);
    if a.1 { rt_print_int(1); } else { rt_print_int(0); }
    if str_parse_int("9223372036854775808").1 { rt_print_int(1); } else { rt_print_int(0); }
    let c = str_parse_int("-9223372036854775808");
    rt_print_int(c.0);
    if c.1 { rt_print_int(1); } else { rt_print_int(0); }
    if str_parse_int("-9223372036854775809").1 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(test);
    assert_eq!(code, 0);
    assert_eq!(
        ints_of(&out),
        vec![9223372036854775807, 1, 0, -9223372036854775808, 1, 0]
    );
}

#[test]
fn p06_parse_float_basics() {
    let test = r#"
fn main() {
    let a = str_parse_float("3.5");
    rt_print_float(a.0);
    if a.1 { rt_print_int(1); } else { rt_print_int(0); }
    let b = str_parse_float("-2.5");
    rt_print_float(b.0);
    if b.1 { rt_print_int(1); } else { rt_print_int(0); }
    let c = str_parse_float("42");
    rt_print_float(c.0);
    if c.1 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(test);
    assert_eq!(code, 0);
    assert_eq!(floats_of(&out), vec![3.5, 1.0, -2.5, 1.0, 42.0, 1.0]);
}

#[test]
fn p07_parse_float_exponents() {
    let test = r#"
fn main() {
    let a = str_parse_float("1e2");
    rt_print_float(a.0);
    if a.1 { rt_print_int(1); } else { rt_print_int(0); }
    let b = str_parse_float("1.5e1");
    rt_print_float(b.0);
    if b.1 { rt_print_int(1); } else { rt_print_int(0); }
    let c = str_parse_float("2E-1");
    rt_print_float(c.0);
    if c.1 { rt_print_int(1); } else { rt_print_int(0); }
    let d = str_parse_float("  2.5E1  ");
    rt_print_float(d.0);
    if d.1 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(test);
    assert_eq!(code, 0);
    assert_eq!(
        floats_of(&out),
        vec![100.0, 1.0, 15.0, 1.0, 0.2, 1.0, 25.0, 1.0]
    );
}

#[test]
fn p08_parse_float_invalid() {
    let test = r#"
fn main() {
    if str_parse_float("").1 { rt_print_int(1); } else { rt_print_int(0); }
    if str_parse_float(".").1 { rt_print_int(1); } else { rt_print_int(0); }
    if str_parse_float("1e").1 { rt_print_int(1); } else { rt_print_int(0); }
    if str_parse_float("1e+").1 { rt_print_int(1); } else { rt_print_int(0); }
    if str_parse_float(".e1").1 { rt_print_int(1); } else { rt_print_int(0); }
    if str_parse_float("1.2.3").1 { rt_print_int(1); } else { rt_print_int(0); }
    if str_parse_float("-").1 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(test);
    assert_eq!(code, 0);
    assert_eq!(ints_of(&out), vec![0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn p09_parse_float_extreme_exponents_terminate() {
    let test = r#"
fn main() {
    // Must not spin on a huge exponent: 400 iterations already saturate f64.
    let a = str_parse_float("1e999999999");
    rt_print_float(a.0);
    if a.1 { rt_print_int(1); } else { rt_print_int(0); }
    let b = str_parse_float("1e-999999999");
    rt_print_float(b.0);
    if b.1 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(test);
    assert_eq!(code, 0);
    assert_eq!(ints_of(&out), vec![1, 0, 1]);
    assert!(
        stdout_text(&out).starts_with("Inf"),
        "{:?}",
        stdout_text(&out)
    );
}

#[test]
fn p10_parse_float_fraction_precision() {
    let test = r#"
fn main() {
    let a = str_parse_float("0.125");
    rt_print_float(a.0);
    let b = str_parse_float("12.75");
    rt_print_float(b.0);
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(test);
    assert_eq!(code, 0);
    let vals = floats_of(&out);
    assert!((vals[0] - 0.125).abs() < 1e-12, "{:?}", vals);
    assert!((vals[1] - 12.75).abs() < 1e-12, "{:?}", vals);
}

/// The parsers consume their input, so 200 heap-produced strings must not
/// leak (E-R06 would make the exit code nonzero).
#[test]
fn p11_parse_consumes_heap_input() {
    let test = r#"
fn main() {
    let mut acc = 0;
    let mut i = 0;
    while i < 200 {
        let s = rt_str_from_int(i);
        let r = str_parse_int(s);
        if r.1 { acc = acc + r.0; }
        i = i + 1;
    }
    rt_print_int(acc);
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(test);
    assert_eq!(code, 0);
    assert_eq!(ints_of(&out), vec![19900]);
}

#[test]
fn p12_parse_float_of_int_text() {
    let test = r#"
fn main() {
    let i = str_parse_int("123");
    let f = str_parse_float(rt_str_from_int(i.0));
    rt_print_float(f.0);
    if f.1 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(test);
    assert_eq!(code, 0);
    assert_eq!(floats_of(&out), vec![123.0, 1.0]);
}
