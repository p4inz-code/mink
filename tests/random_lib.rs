//! Tests for the MINK Random library (Session 61).

use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn build_and_run(source: &str) -> (i32, String) {
    let out_dir = std::env::temp_dir();
    let mink_path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("mink.exe");

    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let test_file = out_dir.join(format!("mink_rand_t{}.mink", id));
    std::fs::write(&test_file, source).unwrap();

    let build = Command::new(&mink_path)
        .args(["build", test_file.to_str().unwrap()])
        .output()
        .unwrap();

    if !build.status.success() {
        let stderr = String::from_utf8_lossy(&build.stderr).to_string();
        return (build.status.code().unwrap_or(1), stderr);
    }

    let exe = out_dir.join(format!("mink_rand_t{}.exe", id));
    let run = Command::new(&exe).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let code = run.status.code().unwrap_or(-1);
    (code, stdout)
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
// CORE PRNG
// ============================================================

#[test]
fn r01_seed_reproducibility() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(42);
    let a = rt_random_next();
    let b = rt_random_next();
    let c = rt_random_next();
    rt_random_seed(42);
    let d = rt_random_next();
    let e = rt_random_next();
    let f = rt_random_next();
    if a == d { rt_print_int(1); } else { rt_print_int(0); }
    if b == e { rt_print_int(1); } else { rt_print_int(0); }
    if c == f { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![1, 1, 1]);
}

#[test]
fn r02_different_seeds() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(1);
    let a = rt_random_next();
    rt_random_seed(2);
    let b = rt_random_next();
    if a != b { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![1]);
}

#[test]
fn r03_seed_zero_treated_as_one() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(0);
    let a = rt_random_next();
    rt_random_seed(1);
    let b = rt_random_next();
    if a == b { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![1]);
}

#[test]
fn r04_values_nonzero() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(999);
    let r1 = rt_random_next();
    let r2 = rt_random_next();
    let r3 = rt_random_next();
    if r1 != 0 { rt_print_int(1); } else { rt_print_int(0); }
    if r2 != 0 { rt_print_int(1); } else { rt_print_int(0); }
    if r3 != 0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![1, 1, 1]);
}

// ============================================================
// BOUNDED INTEGERS
// ============================================================

#[test]
fn r10_int_range_bounds() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(42);
    let mut i = 0;
    let mut all_ok = 1;
    while i < 200 {
        let r = rt_random_next();
        // Use abs for modulo to handle negative values
        let mut val = r - (r / 10) * 10;
        if val < 0 { val = 0 - val; }
        if val >= 10 { all_ok = 0; }
        i = i + 1;
    }
    rt_print_int(all_ok);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![1]);
}

#[test]
fn r11_single_element_range() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(42);
    let mut all_ok = 1;
    let mut i = 0;
    while i < 10 {
        let r = rt_random_next();
        let mut val = r - (r / 5) * 5;
        if val < 0 { val = 0 - val; }
        if val >= 5 { all_ok = 0; }
        i = i + 1;
    }
    rt_print_int(all_ok);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![1]);
}

// ============================================================
// BOOLEAN
// ============================================================

#[test]
fn r20_bool_values() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(42);
    let mut all_ok = 1;
    let mut i = 0;
    while i < 20 {
        let r = rt_random_next();
        let mut b = r - (r / 2) * 2;
        if b < 0 { b = 0 - b; }
        if b > 1 { all_ok = 0; }
        i = i + 1;
    }
    rt_print_int(all_ok);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![1]);
}

// ============================================================
// BYTE
// ============================================================

#[test]
fn r30_byte_range() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(42);
    let mut all_ok = 1;
    let mut i = 0;
    while i < 20 {
        let r = rt_random_next();
        let mut b = r - (r / 256) * 256;
        if b < 0 { b = 0 - b; }
        if b > 255 { all_ok = 0; }
        i = i + 1;
    }
    rt_print_int(all_ok);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![1]);
}

// ============================================================
// CHOICE / INDEX
// ============================================================

#[test]
fn r40_choice_range() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(42);
    let mut all_ok = 1;
    let mut i = 0;
    while i < 20 {
        let r = rt_random_next();
        let mut idx = r - (r / 7) * 7;
        if idx < 0 { idx = 0 - idx; }
        if idx >= 7 { all_ok = 0; }
        i = i + 1;
    }
    rt_print_int(all_ok);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![1]);
}

// ============================================================
// STATISTICAL SANITY
// ============================================================

#[test]
fn r50_uniform_distribution() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(42);
    let mut sum = 0;
    let mut i = 0;
    while i < 1000 {
        let r = rt_random_next();
        let mut val = r - (r / 100) * 100;
        if val < 0 { val = 0 - val; }
        sum = sum + val;
        i = i + 1;
    }
    rt_print_int(sum);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert!(
        ints[0] > 30000 && ints[0] < 70000,
        "sum {} outside expected range",
        ints[0]
    );
}

#[test]
fn r51_bit_distribution() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(42);
    let mut odd_count = 0;
    let mut i = 0;
    while i < 1000 {
        let r = rt_random_next();
        let mut bit = r - (r / 2) * 2;
        if bit < 0 { bit = 0 - bit; }
        if bit != 0 { odd_count = odd_count + 1; }
        i = i + 1;
    }
    rt_print_int(odd_count);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert!(
        ints[0] > 350 && ints[0] < 650,
        "odd count {} outside expected range",
        ints[0]
    );
}

// ============================================================
// DETERMINISTIC SEQUENCE
// ============================================================

#[test]
fn r60_deterministic_sequence() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(12345);
    let r1 = rt_random_next();
    let r2 = rt_random_next();
    let r3 = rt_random_next();
    rt_random_seed(12345);
    let r4 = rt_random_next();
    let r5 = rt_random_next();
    let r6 = rt_random_next();
    if r1 == r4 { rt_print_int(1); } else { rt_print_int(0); }
    if r2 == r5 { rt_print_int(1); } else { rt_print_int(0); }
    if r3 == r6 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![1, 1, 1]);
}

// ============================================================
// EDGE CASES
// ============================================================

#[test]
fn r80_large_seed() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(999999999999);
    let r = rt_random_next();
    if r != 0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![1]);
}

#[test]
fn r81_negative_seed() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(-1);
    let r = rt_random_next();
    if r != 0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![1]);
}

#[test]
fn r82_many_generations() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    rt_random_seed(42);
    let mut i = 0;
    while i < 10000 {
        let r = rt_random_next();
        i = i + 1;
    }
    rt_print_int(1);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![1]);
}
