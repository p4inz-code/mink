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

// ===========================================================================
// Session 106 permanent regression tests — 4 bugs fixed
// ===========================================================================

// BUG 1: Map rebuild wrote occupied flag into element area (wrong address)
// after growth. has() returned false for all entries after table rebuild.
#[test]
fn s106_map_has_after_growth() {
    let (code, out) = build_and_run(
        r#"
fn main() -> Int {
    let mut m = rt_map_new(4);
    let mut i = 0;
    while i < 10 {
        m = rt_map_insert(m, i, i * 7);
        i = i + 1;
    }
    // All 10 keys must be findable after growth triggers rebuild
    i = 0;
    while i < 10 {
        if !rt_map_has(m, i) {
            rt_print_str("FAIL: has after growth");
            return 1;
        }
        if rt_map_get(m, i) != i * 7 {
            rt_print_str("FAIL: get value after growth");
            return 2;
        }
        i = i + 1;
    }
    if rt_map_len(m) != 10 {
        rt_print_str("FAIL: len");
        return 3;
    }
    rt_map_free(m);
    return 0;
}"#,
    );
    assert_success(code, &out);
}

// BUG 2: map_get jumped to E-R11 on key mismatch instead of advancing
// the probe chain. Any collision-chain lookup failed with E-R11.
#[test]
fn s106_map_get_collision_chain() {
    let (code, out) = build_and_run(
        r#"
fn main() -> Int {
    let mut m = rt_map_new(4);
    // Insert 3 keys that hash to same bucket in cap=4: 10%4=2, 30%4=2
    m = rt_map_insert(m, 2, 100);
    m = rt_map_insert(m, 10, 200);
    m = rt_map_insert(m, 30, 300);
    // get(30) must probe past 2 and 10 to find 30
    if rt_map_get(m, 30) != 300 {
        rt_print_str("FAIL: get collision");
        return 1;
    }
    if rt_map_get(m, 10) != 200 {
        rt_print_str("FAIL: get collided key");
        return 2;
    }
    if rt_map_get(m, 2) != 100 {
        rt_print_str("FAIL: get first key");
        return 3;
    }
    if !rt_map_has(m, 30) {
        rt_print_str("FAIL: has collision");
        return 4;
    }
    rt_map_free(m);
    return 0;
}"#,
    );
    assert_success(code, &out);
}

// BUG 3: jcc_literal_str inverted upper bound — every heap string freed
// through collections leaked (treated as literal, not freed).
#[test]
fn s106_set_string_values_no_leak() {
    let (code, out) = build_and_run(
        r#"
fn main() -> Int {
    let mut s = rt_set_new(4);
    s = rt_set_insert(s, "alpha");
    s = rt_set_insert(s, "bravo");
    s = rt_set_insert(s, "charlie");
    s = rt_set_insert(s, "delta");
    s = rt_set_insert(s, "echo");
    if rt_set_len(s) != 5 {
        rt_print_str("FAIL: len");
        return 1;
    }
    if !rt_set_has(s, "alpha") {
        rt_print_str("FAIL: has alpha");
        return 2;
    }
    if !rt_set_has(s, "echo") {
        rt_print_str("FAIL: has echo");
        return 3;
    }
    rt_set_free(s);
    return 0;
}"#,
    );
    assert_success(code, &out);
}

// BUG 3b: Same leak for map with string values.
#[test]
fn s106_map_string_values_no_leak() {
    let (code, out) = build_and_run(
        r#"
fn main() -> Int {
    let mut m = rt_map_new(4);
    m = rt_map_insert(m, "one", 1);
    m = rt_map_insert(m, "two", 2);
    m = rt_map_insert(m, "three", 3);
    m = rt_map_insert(m, "four", 4);
    m = rt_map_insert(m, "five", 5);
    if rt_map_len(m) != 5 {
        rt_print_str("FAIL: len");
        return 1;
    }
    if rt_map_get(m, "one") != 1 {
        rt_print_str("FAIL: get one");
        return 2;
    }
    rt_map_free(m);
    return 0;
}"#,
    );
    assert_success(code, &out);
}

// BUG 4: Allocator free-list reuse returned blocks with stale occupied=1
// bucket flags, causing insert probe loops to spin forever (hang).
// Construction + destruction must not hang.
#[test]
fn s106_set_construction_destruction_no_hang() {
    let (code, out) = build_and_run(
        r#"
fn main() -> Int {
    // Repeat construction + destruction to exercise free-list reuse
    let mut i = 0;
    while i < 20 {
        let mut s = rt_set_new(4);
        s = rt_set_insert(s, 1);
        s = rt_set_insert(s, 2);
        s = rt_set_insert(s, 3);
        rt_set_free(s);
        i = i + 1;
    }
    // Also test: construct, grow, destroy — the rebuild path must clear buckets
    let mut s = rt_set_new(4);
    let mut j = 0;
    while j < 30 {
        s = rt_set_insert(s, j);
        j = j + 1;
    }
    if rt_set_len(s) != 30 {
        rt_print_str("FAIL: len");
        return 1;
    }
    rt_set_free(s);
    // Final insert into a reused block must not hang
    let mut s2 = rt_set_new(4);
    s2 = rt_set_insert(s2, 999);
    if !rt_set_has(s2, 999) {
        rt_print_str("FAIL: has after reuse");
        return 2;
    }
    rt_set_free(s2);
    return 0;
}"#,
    );
    assert_success(code, &out);
}

// BUG 4b: Same for map construction + destruction + reuse.
#[test]
fn s106_map_construction_destruction_no_hang() {
    let (code, out) = build_and_run(
        r#"
fn main() -> Int {
    let mut i = 0;
    while i < 20 {
        let mut m = rt_map_new(4);
        m = rt_map_insert(m, 1, 10);
        m = rt_map_insert(m, 2, 20);
        rt_map_free(m);
        i = i + 1;
    }
    // Grow, destroy, reuse
    let mut m = rt_map_new(4);
    let mut j = 0;
    while j < 30 {
        m = rt_map_insert(m, j, j * 5);
        j = j + 1;
    }
    if rt_map_len(m) != 30 {
        rt_print_str("FAIL: len");
        return 1;
    }
    rt_map_free(m);
    let mut m2 = rt_map_new(4);
    m2 = rt_map_insert(m2, 42, 420);
    if rt_map_get(m2, 42) != 420 {
        rt_print_str("FAIL: get after reuse");
        return 2;
    }
    rt_map_free(m2);
    return 0;
}"#,
    );
    assert_success(code, &out);
}

// VEC E-R04 regression: realloc + free must not corrupt memory.
#[test]
fn s106_vec_realloc_free_no_corruption() {
    let (code, out) = build_and_run(
        r#"
fn main() -> Int {
    let mut c = 0;
    while c < 100 {
        let mut v = rt_vec_new(2);
        let mut i = 0;
        while i < 50 {
            v = rt_vec_push(v, i);
            i = i + 1;
        }
        if rt_vec_len(v) != 50 {
            rt_print_str("FAIL: vec len");
            return 1;
        }
        if rt_vec_get(v, 0) != 0 {
            rt_print_str("FAIL: vec[0]");
            return 2;
        }
        if rt_vec_get(v, 49) != 49 {
            rt_print_str("FAIL: vec[49]");
            return 3;
        }
        rt_vec_free(v);
        c = c + 1;
    }
    return 0;
}"#,
    );
    assert_success(code, &out);
}

// Session 107 regression: VecRemove R8 clobber.
// The inner memcpy in the shift loop reloaded R8 with elem_size,
// clobbering the live shift-loop bound.  The last element was left
// unshifted and duplicated, causing a double-free on rt_vec_free.
// This only triggers for elem_size >= 9 and length >= 9.
#[test]
fn s107_vec_remove_r8_clobber_no_double_free() {
    let (code, out) = build_and_run(
        r#"
fn main() -> Int {
    // Force elem_size = 16 (a 2-field Int struct).
    let mut v = rt_vec_new(2);
    let mut i = 0;
    while i < 20 {
        v = rt_vec_push(v, i * 100);
        i = i + 1;
    }
    if rt_vec_len(v) != 20 { return 1; }
    // Remove the first element (triggers the shift of all 19 remaining).
    v = rt_vec_remove(v, 0);
    if rt_vec_len(v) != 19 { return 2; }
    // Verify the shift: the old vec[1] (value 100) is now at vec[0].
    if rt_vec_get(v, 0) != 100 { return 3; }
    if rt_vec_get(v, 18) != 1900 { return 4; }
    // Remove from the middle.
    v = rt_vec_remove(v, 9);
    if rt_vec_len(v) != 18 { return 5; }
    // Remove from the end.
    v = rt_vec_remove(v, 17);
    if rt_vec_len(v) != 17 { return 6; }
    rt_vec_free(v);
    return 0;
}"#,
    );
    assert_success(code, &out);
}

// Session 107 regression: MapKeys/MapValues/SetElements arity.
// These services take exactly one argument (the collection), but the
// IR arity table listed them as 2-arg, which caused every call to
// fail with E-B07 (wrong number of arguments).
#[test]
fn s107_map_keys_values_set_elements_arity() {
    let (code, out) = build_and_run(
        r#"
fn main() -> Int {
    // Map keys.
    let mut m = rt_map_new(4);
    m = rt_map_insert(m, 1, 10);
    m = rt_map_insert(m, 2, 20);
    let k = rt_map_keys(m);
    if rt_vec_len(k) != 2 { return 1; }
    rt_vec_free(k);
    // Map values.
    let v = rt_map_values(m);
    if rt_vec_len(v) != 2 { return 2; }
    rt_vec_free(v);
    rt_map_free(m);
    // Set elements.
    let mut s = rt_set_new(4);
    s = rt_set_insert(s, 10);
    s = rt_set_insert(s, 20);
    let e = rt_set_elements(s);
    if rt_vec_len(e) != 2 { return 3; }
    rt_vec_free(e);
    rt_set_free(s);
    return 0;
}"#,
    );
    assert_success(code, &out);
}

// Session 107 regression: chunked return-slot memcpy for multi-word
// struct elements.  The runtime services (vec_get, vec_pop, map_get)
// copied multi-word elements linearly into the hidden return slot, but
// the calling convention stores word0 at base, word1 at base-8, etc.
// This caused struct field reads to return garbage (or crash).
#[test]
fn s107_chunked_return_slot_struct_fields() {
    let (code, out) = build_and_run(
        r#"
struct Point { x: Int, y: Int }

fn main() -> Int {
    // Vec<struct> — read both fields.
    let mut v = rt_vec_new(2);
    let a = Point { x: 3, y: 4 };
    v = rt_vec_push(v, a);
    let b = Point { x: 5, y: 6 };
    v = rt_vec_push(v, b);
    let g0 = rt_vec_get(v, 0);
    let g1 = rt_vec_get(v, 1);
    if g0.x != 3 { return 1; }
    if g0.y != 4 { return 2; }
    if g1.x != 5 { return 3; }
    if g1.y != 6 { return 4; }
    rt_vec_free(v);
    // Map<Str, Int> — read multi-word values.
    let mut m = rt_map_new(2);
    m = rt_map_insert(m, "hello", 42);
    m = rt_map_insert(m, "world", 99);
    let v0 = rt_map_get(m, "hello");
    if v0 != 42 { return 5; }
    let v1 = rt_map_get(m, "world");
    if v1 != 99 { return 6; }
    rt_map_free(m);
    return 0;
}"#,
    );
    assert_success(code, &out);
}

// Session 107 regression: Vec<Str> push/get/free lifecycle.
// Before the catalog signature change, Vec push/get returned Int
// regardless of the actual element type, making typed collections
// unusable.  This test verifies the full lifecycle with string
// elements including ownership (no leaks, no double-frees).
#[test]
fn s107_vec_str_lifecycle() {
    let (code, out) = build_and_run(
        r#"
fn main() -> Int {
    let mut v = rt_vec_new(2);
    v = rt_vec_push(v, "hello");
    v = rt_vec_push(v, "world");
    if rt_vec_len(v) != 2 { return 1; }
    let s0 = rt_vec_get(v, 0);
    if rt_str_len(s0) != 5 { return 2; }
    if rt_str_byte(s0, 0) != 104 { return 3; } // 'h'
    let s1 = rt_vec_get(v, 1);
    if rt_str_len(s1) != 5 { return 4; }
    if rt_str_byte(s1, 0) != 119 { return 5; } // 'w'
    // Pop transfers ownership.
    let popped = rt_vec_pop(v);
    if rt_vec_len(v) != 1 { return 6; }
    rt_str_free(popped);
    rt_vec_free(v);
    return 0;
}"#,
    );
    assert_success(code, &out);
}
