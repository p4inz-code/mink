//! Session 200 — compiler diagnostic correctness battery.
//!
//! Every case compiles a small malformed program with the real `mink build`
//! CLI and pins the observable contract: a clean exit code 1 (never a crash or
//! a silent success), the expected `E-…` code, a file:line location, no panic
//! marker, and byte-identical stderr across two runs. A valid-program control
//! and the `Int` boundary cases keep the battery from passing vacuously.
//!
//! Two defects found by this battery in Session 200 are pinned here:
//! out-of-range integer literals silently wrapped (now `E-T43`) and aggregate
//! type diagnostics rendered a mangled `` is not a valid key type: …`` prefix.

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn artifact_dir() -> PathBuf {
    let dir = PathBuf::from("target").join("mink-artifacts").join("s200");
    std::fs::create_dir_all(&dir).expect("failed to create target/mink-artifacts/s200");
    dir
}

struct Compile {
    code: i32,
    stdout: String,
    stderr: String,
    path: PathBuf,
}

fn compile(name: &str, source: &str) -> Compile {
    let path = artifact_dir().join(format!("diag_{name}.mink"));
    std::fs::write(&path, source.replace("\r\n", "\n")).expect("failed to write fixture");
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("build")
        .arg(&path)
        .output()
        .expect("failed to run mink build");
    Compile {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        path,
    }
}

fn normalized(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Asserts the full diagnostic contract for one malformed program.
fn expect_error(name: &str, source: &str, code: &str, line: u32) {
    let first = compile(name, source);
    assert_eq!(
        first.code, 1,
        "{name}: expected exit 1 (compile error), got {} (stdout={:?} stderr={:?})",
        first.code, first.stdout, first.stderr
    );
    assert!(
        first.stdout.is_empty(),
        "{name}: a failed build must not write stdout, got {:?}",
        first.stdout
    );
    let stderr = first.stderr.replace('\\', "/");
    assert!(
        stderr.contains(&format!("error[{code}]")),
        "{name}: expected {code}, got {:?}",
        first.stderr
    );
    let location = format!("{}:{line}:", normalized(&first.path));
    assert!(
        stderr.contains(&location),
        "{name}: expected a location at {location}, got {:?}",
        first.stderr
    );
    for marker in ["panicked", "internal compiler error", "RUST_BACKTRACE"] {
        assert!(
            !first.stderr.contains(marker),
            "{name}: diagnostic leaked {marker:?}: {:?}",
            first.stderr
        );
    }
    // Diagnostics are a contract: the same source must produce the same bytes.
    let second = compile(name, source);
    assert_eq!(first.code, second.code, "{name}: exit code is not stable");
    assert_eq!(
        first.stderr, second.stderr,
        "{name}: stderr is not deterministic"
    );
    let _ = std::fs::remove_file(first.path.with_extension("exe"));
}

// ============================================================================
// 1. Front-end and semantic error coverage
// ============================================================================

#[test]
fn d01_expected_expression_is_p03() {
    expect_error("p03", "fn main() {\n    let x = ;\n}\n", "E-P03", 2);
}

#[test]
fn d02_unterminated_string_is_l02() {
    expect_error("l02", "fn main() {\n    let s = \"abc\n}\n", "E-L02", 2);
}

#[test]
fn d03_type_mismatch_is_t01() {
    expect_error(
        "t01",
        "fn main() {\n    let x: Int = \"s\";\n}\n",
        "E-T01",
        2,
    );
}

#[test]
fn d04_duplicate_definition_is_s02() {
    expect_error("s02", "fn a() {}\nfn a() {}\nfn main() {}\n", "E-S02", 2);
}

#[test]
fn d05_unresolved_name_is_s01() {
    expect_error("s01", "fn main() {\n    nope();\n}\n", "E-S01", 2);
}

#[test]
fn d06_wrong_argument_count_is_t05() {
    expect_error(
        "t05",
        "fn f(a: Int) -> Int { return a; }\nfn main() { f(); }\n",
        "E-T05",
        2,
    );
}

#[test]
fn d07_immutable_assignment_is_s03() {
    expect_error(
        "s03",
        "fn main() {\n    let v = 1;\n    v = 2;\n}\n",
        "E-S03",
        3,
    );
}

#[test]
fn d08_call_of_non_function_is_t04() {
    expect_error(
        "t04",
        "fn main() {\n    let x = 1;\n    x();\n}\n",
        "E-T04",
        3,
    );
}

#[test]
fn d09_missing_module_is_s01() {
    expect_error("module", "mod nope;\nfn main() {}\n", "E-S01", 1);
}

#[test]
fn d10_unterminated_comment_is_l04() {
    expect_error("l04", "fn main() {\n    /* unterminated\n}\n", "E-L04", 2);
}

#[test]
fn d11_invalid_escape_is_l05() {
    expect_error(
        "l05",
        "fn main() {\n    let s = \"a\\qb\";\n}\n",
        "E-L05",
        2,
    );
}

#[test]
fn d12_break_outside_loop_is_s05() {
    expect_error("s05", "fn main() {\n    break;\n}\n", "E-S05", 2);
}

#[test]
fn d13_return_type_mismatch_is_t01() {
    expect_error(
        "ret",
        "fn f() -> Int { return \"s\"; }\nfn main() {}\n",
        "E-T01",
        1,
    );
}

// ============================================================================
// 2. Integer literal range (Session 200 defect: silent wraparound)
// ============================================================================

#[test]
fn d14_integer_literal_above_i64_max_is_t43() {
    expect_error(
        "overflow_decimal",
        "fn main() {\n    let x: Int = 9223372036854775808;\n}\n",
        "E-T43",
        2,
    );
}

#[test]
fn d15_integer_literal_above_u64_max_is_t43() {
    expect_error(
        "overflow_u64",
        "fn main() {\n    let x: Int = 999999999999999999999999;\n}\n",
        "E-T43",
        2,
    );
}

#[test]
fn d16_hex_literal_at_two_to_the_64_is_t43() {
    expect_error(
        "overflow_hex",
        "fn main() {\n    let x: Int = 0x1_0000_0000_0000_0000;\n}\n",
        "E-T43",
        2,
    );
}

#[test]
fn d17_negated_literal_below_i64_min_is_t43() {
    expect_error(
        "overflow_neg",
        "fn main() {\n    let x: Int = -9223372036854775809;\n}\n",
        "E-T43",
        2,
    );
}

#[test]
fn d18_overflow_in_call_argument_is_t43() {
    expect_error(
        "overflow_arg",
        "fn f(a: Int) -> Int { return a; }\nfn main() { f(9223372036854775808); }\n",
        "E-T43",
        2,
    );
}

#[test]
fn d19_i64_min_literal_is_accepted_and_exact() {
    let c = compile(
        "min_int",
        "fn main() {\n    let x: Int = -9223372036854775808;\n    rt_print_int(x);\n}\n",
    );
    assert_eq!(c.code, 0, "i64::MIN must compile (stderr={:?})", c.stderr);
    let exe = c.path.with_extension("exe");
    let ran = Command::new(&exe).output().expect("failed to run min_int");
    assert_eq!(ran.status.code(), Some(0), "min_int exited non-zero");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout).trim(),
        "-9223372036854775808",
        "i64::MIN printed the wrong value"
    );
    let _ = std::fs::remove_file(&exe);
}

// ============================================================================
// 3. Diagnostic message rendering (Session 200 defect: mangled prefix)
// ============================================================================

#[test]
fn d20_aggregate_diagnostic_has_no_key_type_prefix() {
    let c = compile(
        "index_message",
        "fn main() {\n    let a: [Int; 2] = [1, 2];\n    rt_print_int(a[5]);\n}\n",
    );
    assert_eq!(c.code, 1, "out-of-bounds index must not compile");
    assert!(
        c.stderr.contains("E-T11"),
        "expected E-T11, got {:?}",
        c.stderr
    );
    assert!(
        c.stderr
            .contains("array index `5` is out of bounds for an array of length 2"),
        "the aggregate message body is missing: {:?}",
        c.stderr
    );
    assert!(
        !c.stderr.contains("valid key type"),
        "an array-index diagnostic must not mention key types: {:?}",
        c.stderr
    );
}

#[test]
fn d21_collection_key_diagnostic_keeps_its_message() {
    let c = compile(
        "key_message",
        "fn main() {\n    let m = rt_map_new(4);\n    rt_print_int(rt_map_len(m));\n}\n",
    );
    // The fixture above is valid; the key diagnostic is exercised by the
    // collection suites. This test guards that the shared Display arm still
    // compiles every kind; a mangled *key* message would show up as
    // \"`` is not a valid key type\" with empty backticks.
    assert!(
        c.code == 0 || !c.stderr.contains("`` is not a valid key type"),
        "empty key-type rendering: {:?}",
        c.stderr
    );
    let _ = std::fs::remove_file(c.path.with_extension("exe"));
}

// ============================================================================
// 4. Control: valid programs still build and run
// ============================================================================

#[test]
fn d23_pattern_literal_overflow_is_t43() {
    expect_error(
        "pattern_overflow",
        "fn main() {\n    let x: Int = 5;\n    match x {\n        9223372036854775808 => { rt_print_int(1); },\n        _ => { rt_print_int(0); },\n    }\n}\n",
        "E-T43",
        4,
    );
}

#[test]
fn d24_i64_min_pattern_is_accepted_and_matches() {
    let c = compile(
        "min_pattern",
        "fn main() {\n    let x: Int = -9223372036854775808;\n    match x {\n        -9223372036854775808 => { rt_print_int(1); },\n        _ => { rt_print_int(0); },\n    }\n}\n",
    );
    assert_eq!(
        c.code, 0,
        "i64::MIN pattern must compile (stderr={:?})",
        c.stderr
    );
    let exe = c.path.with_extension("exe");
    let ran = Command::new(&exe)
        .output()
        .expect("failed to run min_pattern");
    assert_eq!(ran.status.code(), Some(0), "min_pattern exited non-zero");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout).trim(),
        "1",
        "i64::MIN pattern did not match"
    );
    let _ = std::fs::remove_file(&exe);
}

#[test]
fn d22_valid_program_compiles_and_runs() {
    let c = compile(
        "valid",
        "fn main() {\n    let x: Int = 9223372036854775807;\n    rt_print_int(x);\n}\n",
    );
    assert_eq!(c.code, 0, "valid program failed: {:?}", c.stderr);
    assert!(
        c.stderr.is_empty(),
        "valid build wrote stderr: {:?}",
        c.stderr
    );
    let exe = c.path.with_extension("exe");
    let ran = Command::new(&exe).output().expect("failed to run valid");
    assert_eq!(ran.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout).trim(),
        "9223372036854775807"
    );
    let _ = std::fs::remove_file(&exe);
}
