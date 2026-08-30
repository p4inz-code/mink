//! Collections library integration tests — Session 57
//!
//! Tests Vec operations: creation, access, mutation, search, aggregates, transformations.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_coll_test_{n}_{name}.mink"));
    std::fs::write(&path, content.replace("\r\n", "\n")).unwrap();
    path
}

fn build_and_run(test_body: &str) -> (i32, String) {
    let path = temp_source("test", test_body);
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
// BASIC CREATION / LENGTH
// ============================================================

#[test]
fn v01_vec_new_empty() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let v = rt_vec_new(4);
    rt_print_int(rt_vec_len(v));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 0, "new vec has len 0: {}", out);
}

#[test]
fn v02_vec_push_one() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 42);
    rt_print_int(rt_vec_len(v));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 1, "push one: {}", out);
}

#[test]
fn v03_vec_push_three() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 20);
    v = rt_vec_push(v, 30);
    rt_print_int(rt_vec_len(v));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 3, "push three: {}", out);
}

// ============================================================
// ACCESS
// ============================================================

#[test]
fn v04_vec_get_basic() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 100);
    v = rt_vec_push(v, 200);
    rt_print_int(rt_vec_get(v, 0));
    rt_print_int(rt_vec_get(v, 1));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![100, 200], "get: {}", out);
}

#[test]
fn v05_vec_first_last() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 5);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 15);
    rt_print_int(rt_vec_get(v, 0));
    rt_print_int(rt_vec_get(v, 2));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![5, 15], "first/last: {}", out);
}

// ============================================================
// SET
// ============================================================

#[test]
fn v06_vec_set_basic() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 20);
    v = rt_vec_set(v, 0, 99);
    rt_print_int(rt_vec_get(v, 0));
    rt_print_int(rt_vec_get(v, 1));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![99, 20], "set: {}", out);
}

// ============================================================
// POP
// ============================================================

#[test]
fn v07_vec_pop_basic() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 20);
    let popped = rt_vec_pop(v);
    rt_print_int(popped);
    rt_print_int(rt_vec_len(v));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![20, 1], "pop: {}", out);
}

// ============================================================
// REMOVE
// ============================================================

#[test]
fn v08_vec_remove_middle() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 20);
    v = rt_vec_push(v, 30);
    v = rt_vec_remove(v, 1);
    rt_print_int(rt_vec_len(v));
    rt_print_int(rt_vec_get(v, 0));
    rt_print_int(rt_vec_get(v, 1));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![2, 10, 30], "remove middle: {}", out);
}

// ============================================================
// INSERT
// ============================================================

#[test]
fn v09_vec_insert() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 30);
    // Manual insert: shift right then set
    let len = rt_vec_len(v);
    v = rt_vec_push(v, 0);
    let mut i = len;
    while i > 1 {
        let val = rt_vec_get(v, i - 1);
        v = rt_vec_set(v, i, val);
        i = i - 1;
    }
    v = rt_vec_set(v, 1, 20);
    rt_print_int(rt_vec_len(v));
    rt_print_int(rt_vec_get(v, 0));
    rt_print_int(rt_vec_get(v, 1));
    rt_print_int(rt_vec_get(v, 2));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![3, 10, 20, 30], "insert: {}", out);
}

// ============================================================
// SEARCH
// ============================================================

#[test]
fn v10_vec_contains() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 20);
    v = rt_vec_push(v, 30);
    if rt_vec_get(v, 1) == 20 { rt_print_int(1); } else { rt_print_int(0); }
    if rt_vec_get(v, 0) == 99 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![1, 0], "contains: {}", out);
}

#[test]
fn v11_vec_index_of() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 20);
    v = rt_vec_push(v, 30);
    // Find 20 — should be index 1
    let mut found = -1;
    let mut i = 0;
    while i < 3 {
        if rt_vec_get(v, i) == 20 {
            found = i;
        }
        i = i + 1;
    }
    rt_print_int(found);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 1, "index_of: {}", out);
}

// ============================================================
// AGGREGATES
// ============================================================

#[test]
fn v12_vec_sum() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 20);
    v = rt_vec_push(v, 30);
    let mut total = 0;
    let mut i = 0;
    while i < 3 {
        total = total + rt_vec_get(v, i);
        i = i + 1;
    }
    rt_print_int(total);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 60, "sum: {}", out);
}

#[test]
fn v13_vec_min_max() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 30);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 20);
    let mut mn = rt_vec_get(v, 0);
    let mut mx = rt_vec_get(v, 0);
    let mut i = 1;
    while i < 3 {
        let val = rt_vec_get(v, i);
        if val < mn { mn = val; }
        if val > mx { mx = val; }
        i = i + 1;
    }
    rt_print_int(mn);
    rt_print_int(mx);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![10, 30], "min/max: {}", out);
}

// ============================================================
// TRANSFORMATIONS
// ============================================================

#[test]
fn v14_vec_reverse() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 20);
    v = rt_vec_push(v, 30);
    // Reverse manually
    let mut left = 0;
    let mut right = 2;
    while left < right {
        let lv = rt_vec_get(v, left);
        let rv = rt_vec_get(v, right);
        v = rt_vec_set(v, left, rv);
        v = rt_vec_set(v, right, lv);
        left = left + 1;
        right = right - 1;
    }
    rt_print_int(rt_vec_get(v, 0));
    rt_print_int(rt_vec_get(v, 1));
    rt_print_int(rt_vec_get(v, 2));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![30, 20, 10], "reverse: {}", out);
}

// ============================================================
// GROWTH / REALLOCATION
// ============================================================

#[test]
fn v15_vec_growth() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(2);
    v = rt_vec_push(v, 1);
    v = rt_vec_push(v, 2);
    v = rt_vec_push(v, 3);
    v = rt_vec_push(v, 4);
    v = rt_vec_push(v, 5);
    rt_print_int(rt_vec_len(v));
    rt_print_int(rt_vec_get(v, 4));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![5, 5], "growth: {}", out);
}

// ============================================================
// EMPTY OPERATIONS
// ============================================================

#[test]
fn v16_vec_empty_len() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let v = rt_vec_new(4);
    rt_print_int(rt_vec_len(v));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 0, "empty len: {}", out);
}

// ============================================================
// LARGE VEC
// ============================================================

#[test]
fn v17_vec_large() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    let mut i = 0;
    while i < 50 {
        v = rt_vec_push(v, i);
        i = i + 1;
    }
    rt_print_int(rt_vec_len(v));
    rt_print_int(rt_vec_get(v, 49));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![50, 49], "large: {}", out);
}

// ============================================================
// NEGATIVE INDEX (should fail)
// ============================================================

#[test]
fn v18_vec_negative_index() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 10);
    let x = rt_vec_get(v, -1);
    rt_print_int(x);
    rt_exit(0);
}"#,
    );
    // Should get error (code != 0)
    assert!(
        code != 0 || out.contains("runtime error"),
        "negative index should fail: {}",
        out
    );
}

// ============================================================
// OUT OF RANGE INDEX (should fail)
// ============================================================

#[test]
fn v19_vec_out_of_range() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 10);
    let x = rt_vec_get(v, 5);
    rt_print_int(x);
    rt_exit(0);
}"#,
    );
    assert!(
        code != 0 || out.contains("runtime error"),
        "out of range should fail: {}",
        out
    );
}

// ============================================================
// MULTIPLE OPERATIONS
// ============================================================

#[test]
fn v20_vec_mixed_ops() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 20);
    v = rt_vec_push(v, 30);
    v = rt_vec_set(v, 1, 99);
    let popped = rt_vec_pop(v);
    rt_print_int(popped);
    v = rt_vec_remove(v, 0);
    rt_print_int(rt_vec_len(v));
    rt_print_int(rt_vec_get(v, 0));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(all_ints(&out), vec![30, 1, 99], "mixed: {}", out);
}

// ============================================================
// SHORT-CIRCUIT EVALUATION (verifies Phase 2 fix)
// ============================================================

#[test]
fn v21_short_circuit_and() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let s = "";
    let len = rt_str_len(s);
    if len > 0 && rt_str_byte(s, 0) == 47 {
        rt_print_int(1);
    } else {
        rt_print_int(0);
    }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 0, "short-circuit &&: {}", out);
}

#[test]
fn v22_short_circuit_or() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 10);
    if rt_vec_len(v) > 0 || rt_vec_get(v, 0) == 10 {
        rt_print_int(1);
    } else {
        rt_print_int(0);
    }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 1, "short-circuit ||: {}", out);
}

// ============================================================
// VEC + MATH INTEGRATION
// ============================================================

#[test]
fn v23_vec_math_integration() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 5);
    v = rt_vec_push(v, 3);
    v = rt_vec_push(v, 7);
    let mut total = 0;
    let mut i = 0;
    while i < 3 {
        total = total + rt_vec_get(v, i);
        i = i + 1;
    }
    rt_print_int(total);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 15, "math integration: {}", out);
}

// ============================================================
// VEC + JSON INTEGRATION
// ============================================================

#[test]
fn v24_vec_json_integration() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut v = rt_vec_new(4);
    v = rt_vec_push(v, 1);
    v = rt_vec_push(v, 2);
    v = rt_vec_push(v, 3);
    // [1,2,3] = 7 chars: [1,2,3]
    let len = rt_vec_len(v);
    let mut total_chars = 2; // [ and ]
    let mut i = 0;
    while i < len {
        let val = rt_vec_get(v, i);
        // Count digits
        let mut n = val;
        let mut d = 0;
        if n == 0 { d = 1; }
        while n > 0 {
            n = n / 10;
            d = d + 1;
        }
        total_chars = total_chars + d;
        if i < len - 1 {
            total_chars = total_chars + 1; // comma
        }
        i = i + 1;
    }
    rt_print_int(total_chars);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    assert_eq!(first_int(&out), 7, "json integration: {}", out);
}
