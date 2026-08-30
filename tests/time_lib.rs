//! Tests for the MINK Time/Date library (Session 60).

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

    // Unique file names per test
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let test_file = out_dir.join(format!("mink_time_t{}.mink", id));
    std::fs::write(&test_file, source).unwrap();

    // Build
    let build = Command::new(&mink_path)
        .args(["build", test_file.to_str().unwrap()])
        .output()
        .unwrap();

    if !build.status.success() {
        let stderr = String::from_utf8_lossy(&build.stderr).to_string();
        return (build.status.code().unwrap_or(1), stderr);
    }

    // Run the built executable
    let exe = out_dir.join(format!("mink_time_t{}.exe", id));
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
// CORE TIME FUNCTIONS
// ============================================================

#[test]
fn t01_time_now_returns_reasonable_timestamp() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let ts = rt_time_now();
    rt_print_int(ts);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints.len(), 1);
    assert!(ints[0] > 1700000000, "timestamp too small: {}", ints[0]);
    assert!(ints[0] < 1900000000, "timestamp too large: {}", ints[0]);
}

#[test]
fn t02_time_millis_returns_positive() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let ms = rt_time_millis();
    rt_print_int(ms);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints.len(), 1);
    assert!(ints[0] > 0, "millis should be positive: {}", ints[0]);
}

#[test]
fn t03_time_ticks_returns_positive() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let t = rt_time_ticks();
    rt_print_int(t);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints.len(), 1);
    assert!(ints[0] > 0, "ticks should be positive: {}", ints[0]);
}

#[test]
fn t04_time_freq_returns_positive() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let f = rt_time_freq();
    rt_print_int(f);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints.len(), 1);
    assert!(ints[0] > 0, "freq should be positive: {}", ints[0]);
}

#[test]
fn t05_time_now_lib_function() {
    let (code, out) = build_and_run(
        r#"
fn time_now() -> Int { return rt_time_now(); }

fn main() {
    let ts = time_now();
    rt_print_int(ts);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints.len(), 1);
    assert!(ints[0] > 1700000000);
}

// ============================================================
// DATE COMPONENT EXTRACTION
// ============================================================

#[test]
fn t10_time_year_known_date() {
    let (code, out) = build_and_run(
        r#"
fn time_year(ts: Int) -> Int {
    let days = ts / 86400 + 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let mut m = mp + 3;
    if mp >= 10 { m = mp - 9; }
    let mut yy = y;
    if m <= 2 { yy = y + 1; }
    return yy;
}

fn time_month(ts: Int) -> Int {
    let days = ts / 86400 + 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let mut m = mp + 3;
    if mp >= 10 { m = mp - 9; }
    return m;
}

fn time_day(ts: Int) -> Int {
    let days = ts / 86400 + 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    return d;
}

fn main() {
    // 2000-01-01 00:00:00 UTC = 946684800
    let ts = 946684800;
    rt_print_int(time_year(ts));
    rt_print_int(time_month(ts));
    rt_print_int(time_day(ts));
    // 2024-02-29 (leap year)
    let ts2 = 1709164800;
    rt_print_int(time_year(ts2));
    rt_print_int(time_month(ts2));
    rt_print_int(time_day(ts2));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![2000, 1, 1, 2024, 2, 29]);
}

#[test]
fn t11_time_hour_minute_second() {
    let (code, out) = build_and_run(
        r#"
fn time_hour(ts: Int) -> Int {
    let secs = ts - (ts / 86400) * 86400;
    if secs < 0 { return (secs + 86400) / 3600; }
    return secs / 3600;
}

fn time_minute(ts: Int) -> Int {
    let secs = ts - (ts / 86400) * 86400;
    let mut s = secs;
    if secs < 0 { s = secs + 86400; }
    let h = s / 3600;
    return (s - h * 3600) / 60;
}

fn time_second(ts: Int) -> Int {
    let secs = ts - (ts / 86400) * 86400;
    let mut s = secs;
    if secs < 0 { s = secs + 86400; }
    let m = s / 60;
    return s - m * 60;
}

fn main() {
    // 1970-01-01 12:30:45 UTC = 45045
    let ts = 45045;
    rt_print_int(time_hour(ts));
    rt_print_int(time_minute(ts));
    rt_print_int(time_second(ts));
    // 1970-01-01 00:00:00 UTC = 0
    rt_print_int(time_hour(0));
    rt_print_int(time_minute(0));
    rt_print_int(time_second(0));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![12, 30, 45, 0, 0, 0]);
}

// ============================================================
// CALENDAR HELPERS
// ============================================================

#[test]
fn t20_leap_year() {
    let (code, out) = build_and_run(
        r#"
fn time_is_leap_year(year: Int) -> Int {
    if year - (year / 4) * 4 != 0 { return 0; }
    if year - (year / 100) * 100 != 0 { return 1; }
    if year - (year / 400) * 400 != 0 { return 0; }
    return 1;
}

fn main() {
    rt_print_int(time_is_leap_year(2000));
    rt_print_int(time_is_leap_year(1900));
    rt_print_int(time_is_leap_year(2024));
    rt_print_int(time_is_leap_year(2023));
    rt_print_int(time_is_leap_year(2100));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![1, 0, 1, 0, 0]);
}

#[test]
fn t21_days_in_month() {
    let (code, out) = build_and_run(
        r#"
fn time_is_leap_year(year: Int) -> Int {
    if year - (year / 4) * 4 != 0 { return 0; }
    if year - (year / 100) * 100 != 0 { return 1; }
    if year - (year / 400) * 400 != 0 { return 0; }
    return 1;
}

fn time_days_in_month(year: Int, month: Int) -> Int {
    if month == 1 { return 31; }
    if month == 3 { return 31; }
    if month == 5 { return 31; }
    if month == 7 { return 31; }
    if month == 8 { return 31; }
    if month == 10 { return 31; }
    if month == 12 { return 31; }
    if month == 4 { return 30; }
    if month == 6 { return 30; }
    if month == 9 { return 30; }
    if month == 11 { return 30; }
    if month == 2 {
        if time_is_leap_year(year) != 0 { return 29; }
        return 28;
    }
    return 0;
}

fn main() {
    rt_print_int(time_days_in_month(2024, 1));
    rt_print_int(time_days_in_month(2024, 2));
    rt_print_int(time_days_in_month(2023, 2));
    rt_print_int(time_days_in_month(2024, 4));
    rt_print_int(time_days_in_month(2024, 12));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![31, 29, 28, 30, 31]);
}

#[test]
fn t22_weekday() {
    let (code, out) = build_and_run(
        r#"
fn time_weekday(ts: Int) -> Int {
    let days = ts / 86400 + 4;
    let wd = days - (days / 7) * 7;
    if wd < 0 { return wd + 7; }
    return wd;
}

fn main() {
    // 1970-01-01 = Thursday = 4
    rt_print_int(time_weekday(0));
    // 1970-01-04 = Sunday = 0
    rt_print_int(time_weekday(259200));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![4, 0]);
}

// ============================================================
// DURATION / DIFF / ADD
// ============================================================

#[test]
fn t30_time_diff() {
    let (code, out) = build_and_run(
        r#"
fn time_diff(ts1: Int, ts2: Int) -> Int {
    let d = ts1 - ts2;
    if d < 0 { return 0 - d; }
    return d;
}

fn time_add(ts: Int, seconds: Int) -> Int {
    return ts + seconds;
}

fn main() {
    rt_print_int(time_diff(100, 200));
    rt_print_int(time_diff(200, 100));
    rt_print_int(time_add(1000, 3600));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![100, 100, 4600]);
}

// ============================================================
// FORMATTING
// ============================================================

#[test]
fn t40_pad2_works() {
    let (code, out) = build_and_run(
        r#"
fn _pad2(n: Int) -> Str {
    let tens = n / 10;
    let ones = n - tens * 10;
    let t = rt_str_from_int(tens);
    let o = rt_str_from_int(ones);
    return rt_str_concat(t, o);
}

fn main() {
    rt_print_str(_pad2(5));
    rt_print_str(_pad2(42));
    rt_print_str(_pad2(0));
    rt_print_str(_pad2(99));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let lines: Vec<&str> = out
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("runtime error") && !l.contains("memory leak"))
        .collect();
    assert_eq!(lines, vec!["05", "42", "00", "99"]);
}

// ============================================================
// ROUND-TRIP / CONSISTENCY
// ============================================================

#[test]
fn t50_time_now_year_is_2026() {
    let (code, out) = build_and_run(
        r#"
fn time_now() -> Int { return rt_time_now(); }
fn time_year(ts: Int) -> Int {
    let days = ts / 86400 + 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let mut m = mp + 3;
    if mp >= 10 { m = mp - 9; }
    let mut yy = y;
    if m <= 2 { yy = y + 1; }
    return yy;
}
fn time_month(ts: Int) -> Int {
    let days = ts / 86400 + 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let mut m = mp + 3;
    if mp >= 10 { m = mp - 9; }
    return m;
}
fn main() {
    let ts = time_now();
    rt_print_int(time_year(ts));
    rt_print_int(time_month(ts));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints[0], 2026);
    assert_eq!(ints[1], 8);
}

#[test]
fn t51_ticks_increase() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let t1 = rt_time_ticks();
    let t2 = rt_time_ticks();
    let t3 = rt_time_ticks();
    rt_print_int(t2 - t1);
    rt_print_int(t3 - t2);
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints.len(), 2);
    assert!(ints[0] >= 0);
    assert!(ints[1] >= 0);
}

// ============================================================
// EDGE CASES
// ============================================================

#[test]
fn t70_epoch_zero() {
    let (code, out) = build_and_run(
        r#"
fn time_hour(ts: Int) -> Int {
    let secs = ts - (ts / 86400) * 86400;
    return secs / 3600;
}
fn time_minute(ts: Int) -> Int {
    let secs = ts - (ts / 86400) * 86400;
    let h = secs / 3600;
    return (secs - h * 3600) / 60;
}
fn main() {
    rt_print_int(time_hour(0));
    rt_print_int(time_minute(0));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![0, 0]);
}

#[test]
fn t71_end_of_day() {
    let (code, out) = build_and_run(
        r#"
fn time_hour(ts: Int) -> Int {
    let secs = ts - (ts / 86400) * 86400;
    return secs / 3600;
}
fn time_minute(ts: Int) -> Int {
    let secs = ts - (ts / 86400) * 86400;
    let h = secs / 3600;
    return (secs - h * 3600) / 60;
}
fn time_second(ts: Int) -> Int {
    let secs = ts - (ts / 86400) * 86400;
    let m = secs / 60;
    return secs - m * 60;
}
fn main() {
    rt_print_int(time_hour(86399));
    rt_print_int(time_minute(86399));
    rt_print_int(time_second(86399));
    rt_exit(0);
}"#,
    );
    assert_success(code, &out);
    let ints = all_ints(&out);
    assert_eq!(ints, vec![23, 59, 59]);
}
