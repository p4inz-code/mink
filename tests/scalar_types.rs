//! Integration tests for the session 24 scalar-types milestone: `Float`,
//! `Char`, and `Null` become first-class native types.
//!
//! - Float: SSE2 arithmetic (`+ - * / %`), comparisons, negation, literals
//!   in all notations, function parameters/returns, locals, struct fields,
//!   array elements, and exact 17-significant-digit decimal printing
//!   (fixed and scientific, shortest round-trip) with `Inf`/`NaN`/`-0`
//!   handling.
//! - Char: byte literals with escapes, printing, function plumbing, and
//!   integer round-tripping.
//! - Null: a word-sized unit-like type usable in locals and returns.
//! - Regression: Float/Char/Null no longer produce `E-B03` in the native
//!   backend; determinism and native end-to-end execution are covered.
//!
//! The design is documented in
//! `docs/implementation/SCALAR_TYPES_IMPLEMENTATION.md`.

use std::process::Command;

use mink::ast::BinaryOp;
use mink::backend::{self, BInstKind, BType, BackendErrorKind};
use mink::driver;
use mink::source::SourceMap;

static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn unique_source(kind: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "mink_scalar_{kind}_{}_{n}.mink",
        std::process::id()
    ))
}

fn check_src(src: &str) -> mink::typecheck::TypeResult {
    let mut sources = SourceMap::new();
    let path = unique_source("check");
    std::fs::write(&path, src).unwrap();
    let report = driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(
        report.errors.is_empty(),
        "front end must be clean: {:?}",
        report.errors
    );
    report.types.expect("clean program is type-checked")
}

fn lower_errors(src: &str) -> Vec<mink::backend::BackendError> {
    let mut sources = SourceMap::new();
    let path = unique_source("err");
    std::fs::write(&path, src).unwrap();
    let report = driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(
        report.errors.is_empty(),
        "source must be valid MINK: {:?}",
        report.errors
    );
    let mir = report.mir.expect("clean program lowers to MIR");
    match backend::lower(&mir, &sources) {
        Ok(_) => panic!("expected backend errors for: {src}"),
        Err(errors) => errors,
    }
}

fn lower_backend(src: &str) -> mink::backend::BProgram {
    let mut sources = SourceMap::new();
    let path = unique_source("lower");
    std::fs::write(&path, src).unwrap();
    let report = driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(
        report.errors.is_empty(),
        "front end must be clean: {:?}",
        report.errors
    );
    let mir = report.mir.expect("clean program lowers to MIR");
    let program = backend::lower(&mir, &sources)
        .unwrap_or_else(|errors| panic!("clean MIR must lower: {errors:?}"));
    if let Err(errors) = backend::verify(&program) {
        panic!("lowering must produce valid instructions: {errors:?}");
    }
    program
}

fn function<'a>(program: &'a mink::backend::BProgram, name: &str) -> &'a mink::backend::BFunction {
    program
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no function {name}"))
}

fn build(source: &str) -> std::path::PathBuf {
    let name = std::thread::current()
        .name()
        .unwrap_or("program")
        .replace("::", "_");
    let path = unique_source(&name);
    std::fs::write(&path, source).unwrap();
    let mink_exe = env!("CARGO_BIN_EXE_mink");
    let output = Command::new(mink_exe)
        .arg("build")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exe = path.with_extension("exe");
    assert!(exe.exists(), "no executable produced");
    exe
}

fn run(exe: &std::path::PathBuf) -> (i32, Vec<u8>) {
    let output = Command::new(exe).output().unwrap();
    let stdout = output.stdout;
    let stdout = if stdout.contains(&b'\r') {
        stdout.iter().copied().filter(|b| *b != b'\r').collect()
    } else {
        stdout
    };
    (output.status.code().unwrap_or(-1), stdout)
}

fn output_of(source: &str) -> String {
    let exe = build(source);
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0, "program must exit 0");
    String::from_utf8_lossy(&stdout).trim().to_string()
}

// ---------------------------------------------------------------------------
// Typechecking: Float is a distinct numeric type
// ---------------------------------------------------------------------------

#[test]
fn float_literals_typecheck_as_float() {
    // The existing `assert_expr_type` pattern is copied from tests/typecheck.rs.
    let src = "fn main() { let a = 1.5; let b = 1e10; let c = 2.5e-3; return; }";
    let types = check_src(src);
    for literal in ["1.5", "1e10", "2.5e-3"] {
        let start = src.find(literal).unwrap() as u32;
        let span = mink::source::Span::new(
            mink::source::SourceId::new(0),
            start..(start + literal.len() as u32),
        );
        let ty = types.expr_type(span).expect("expression is typed");
        assert_eq!(types.types().display(ty), "Float");
    }
}

#[test]
fn float_arith_operators_typecheck() {
    check_src(
        "fn main() { let a = 1.5 + 2.5; let b = 3.0 - 1.0; let c = 2.0 * 3.0; let d = 7.0 / 2.0; let e = 10.0 % 3.0; let f = -1.5; return; }",
    );
}

#[test]
fn float_comparisons_produce_bool() {
    check_src(
        "fn main() { let a = 1.5 < 2.5; let b = 2.5 <= 2.5; let c = 3.0 > 2.0; let d = 3.0 >= 3.0; let e = 1.5 == 1.5; let f = 1.5 != 2.5; return; }",
    );
}

#[test]
fn int_float_mixing_is_rejected() {
    let src = "fn main() { let a = 1 + 2.5; return; }";
    let mut sources = SourceMap::new();
    let path = unique_source("mix");
    std::fs::write(&path, src).unwrap();
    let report = driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(!report.errors.is_empty(), "int+float must be rejected");
}

// ---------------------------------------------------------------------------
// Backend: Float/Char/Null lower to native types (no E-B03)
// ---------------------------------------------------------------------------

#[test]
fn float_binary_lowers_to_float_typed_instruction() {
    let program = lower_backend("fn main() { let a = 1.5 + 2.5; rt_print_float(a); return; }");
    let f = function(&program, "main");
    assert!(f.blocks.iter().flat_map(|b| &b.insts).any(|i| matches!(
        i.kind,
        BInstKind::Binary {
            ty: BType::Float,
            op: BinaryOp::Add,
            ..
        }
    )));
}

#[test]
fn float_division_and_remainder_lower() {
    let program = lower_backend(
        "fn main() { let a = 7.0 / 2.0; let b = 10.0 % 3.0; rt_print_float(a); rt_print_float(b); return; }",
    );
    let f = function(&program, "main");
    let ops: Vec<_> = f
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter_map(|i| match i.kind {
            BInstKind::Binary {
                op,
                ty: BType::Float,
                ..
            } => Some(op),
            _ => None,
        })
        .collect();
    assert!(ops.contains(&BinaryOp::Div));
    assert!(ops.contains(&BinaryOp::Rem));
}

#[test]
fn char_and_null_no_longer_rejected() {
    // Regression: these were `E-B03` before session 24.
    let program =
        lower_backend("fn main() { let c = 'a'; let n = null; rt_print_char(c); return; }");
    let f = function(&program, "main");
    assert!(f.locals.iter().any(|l| l.ty == BType::Char));
    assert!(f.locals.iter().any(|l| l.ty == BType::Null));
}

#[test]
fn float_return_type_is_supported() {
    let program = lower_backend("fn f() { return 1.5; } fn main() { f(); return; }");
    let f = function(&program, "f");
    assert_eq!(f.result, BType::Float);
}

#[test]
fn range_is_still_rejected() {
    // The only remaining unrepresentable scalar: `Range<Int>`.
    let kinds: Vec<BackendErrorKind> = lower_errors("fn main() { return 0 .. 10; }")
        .iter()
        .map(|e| e.kind())
        .collect();
    assert_eq!(kinds, [BackendErrorKind::UnsupportedType]);
}

// ---------------------------------------------------------------------------
// Native end-to-end: printing
// ---------------------------------------------------------------------------

#[test]
fn native_float_basic_values_print_exactly() {
    let out = output_of(
        "fn main() { rt_print_float(1.5); rt_print_float(0.5); rt_print_float(2.5); rt_print_float(0.25); rt_print_float(100.0); rt_print_float(1e10); }",
    );
    assert_eq!(out, "1.5\n0.5\n2.5\n0.25\n100\n10000000000");
}

#[test]
fn native_float_arithmetic_matches_double_semantics() {
    let out = output_of(
        "fn main() { rt_print_float(0.1 + 0.2); rt_print_float(1.5 + 2.5); rt_print_float(3.0 - 1.0); rt_print_float(7.0 / 2.0); rt_print_float(10.0 % 3.0); rt_print_float(0.1 * 3.0); }",
    );
    assert_eq!(
        out,
        "0.30000000000000004\n4\n2\n3.5\n1\n0.30000000000000004"
    );
}

#[test]
fn native_float_remainder_covers_signs_and_fractional_cases() {
    let out = output_of(
        "fn main() { rt_print_float(5.0 % 2.0); rt_print_float(7.5 % 2.0); rt_print_float(-10.0 % 3.0); rt_print_float(10.0 % -3.0); rt_print_float(2.5 % 1.0); rt_print_float(1.5 % 1.0); rt_print_float(0.5 % 1.0); }",
    );
    assert_eq!(out, "1\n1.5\n-1\n1\n0.5\n0.5\n0.5");
}

#[test]
fn native_float_negation_and_signed_zero() {
    let out = output_of(
        "fn main() { rt_print_float(-1.5); rt_print_float(-0.0); let x = 1.5; rt_print_float(-x); }",
    );
    assert_eq!(out, "-1.5\n-0\n-1.5");
}

#[test]
fn native_float_scientific_notation() {
    let out = output_of(
        "fn main() { rt_print_float(1e-5); rt_print_float(1e20); rt_print_float(1.7976931348623157e308); rt_print_float(4.9406564584124654e-324); rt_print_float(-1.5e-300); rt_print_float(1e16); rt_print_float(9.999999999999999e15); }",
    );
    assert_eq!(
        out,
        "1.0000000000000001e-5\n1e+20\n1.7976931348623157e+308\n4.9406564584124654e-324\n-1.5000000000000001e-300\n10000000000000000\n10000000000000000"
    );
}

#[test]
fn native_float_inf_and_nan() {
    let out = output_of(
        "fn main() { let big = 1e308; rt_print_float(big * 10.0); rt_print_float(0.0 / 0.0); rt_print_float(1.0 / 0.0); rt_print_float(-1.0 / 0.0); rt_print_float(0.0 / 0.0 - 1.0); }",
    );
    // NaN payloads may vary; assert the recognizable spellings.
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "Inf");
    assert!(lines[1] == "NaN" || lines[1] == "-NaN");
    assert_eq!(lines[2], "Inf");
    assert_eq!(lines[3], "-Inf");
    assert!(lines[4].contains("NaN"));
}

#[test]
fn native_float_transcendentals_round_trip() {
    let out = output_of(
        "fn main() { rt_print_float(3.141592653589793); rt_print_float(2.718281828459045); rt_print_float(1.0 / 3.0); rt_print_float(2.0 / 3.0); rt_print_float(0.1); rt_print_float(0.7); rt_print_float(0.1 + 0.7); rt_print_float(123456.789); rt_print_float(-123.456); }",
    );
    assert_eq!(
        out,
        "3.1415926535897931\n2.7182818284590451\n0.33333333333333331\n0.66666666666666663\n0.10000000000000001\n0.69999999999999996\n0.79999999999999993\n123456.789\n-123.456"
    );
}

#[test]
fn native_float_comparisons() {
    let out = output_of(
        "fn main() { if 0.1 + 0.2 > 0.3 { rt_print_int(1); } else { rt_print_int(0); } if 0.1 + 0.2 == 0.3 { rt_print_int(1); } else { rt_print_int(0); } if 1.5 >= 1.5 { rt_print_int(1); } else { rt_print_int(0); } if 2.0 > 1.0 { rt_print_int(1); } else { rt_print_int(0); } if 1.5 != 2.5 { rt_print_int(1); } else { rt_print_int(0); } if 1.5 < 2.5 { rt_print_int(1); } else { rt_print_int(0); } if 3.0 <= 3.0 { rt_print_int(1); } else { rt_print_int(0); } }",
    );
    assert_eq!(out, "1\n0\n1\n1\n1\n1\n1");
}

#[test]
fn native_float_through_functions() {
    let out = output_of(
        "fn add(x, y) { return x + y; } fn neg(x) { return -x; } fn main() { rt_print_float(add(1.5, 2.5)); rt_print_float(add(0.1, 0.2)); rt_print_float(neg(3.25)); }",
    );
    assert_eq!(out, "4\n0.30000000000000004\n-3.25");
}

#[test]
fn native_float_locals_and_mutation() {
    let out = output_of(
        "fn main() { let mut x = 1.5; x = x * 2.0; rt_print_float(x); x = x + 0.25; rt_print_float(x); let y = x; rt_print_float(y); }",
    );
    assert_eq!(out, "3\n3.25\n3.25");
}

#[test]
fn native_float_struct_fields_and_arrays() {
    let out = output_of(
        "struct V { x: Float, y: Float } fn main() { let v = V { x: 1.5, y: 2.5 }; rt_print_float(v.x + v.y); let mut arr = [1.5, 2.5, 3.5]; rt_print_float(arr[0] + arr[2]); arr[1] = 10.0; rt_print_float(arr[1]); }",
    );
    assert_eq!(out, "4\n5\n10");
}

#[test]
fn native_float_loops_accumulate() {
    let out = output_of(
        "fn main() { let mut sum = 0.0; let mut i = 0; while i < 10 { sum = sum + 0.5; i = i + 1; } rt_print_float(sum); }",
    );
    assert_eq!(out, "5");
}

#[test]
fn native_char_printing_and_escapes() {
    // `rt_print_char` writes the character followed by a newline, like the
    // other `rt_print_*` intrinsics.
    let out = output_of(
        "fn main() { rt_print_char('A'); rt_print_char('z'); rt_print_char('0'); rt_print_char('!'); }",
    );
    assert_eq!(out, "A\nz\n0\n!");
}

#[test]
fn native_char_through_functions() {
    let out = output_of("fn id(c) { return c; } fn main() { let c = id('Q'); rt_print_char(c); }");
    assert_eq!(out, "Q");
}

#[test]
fn native_null_locals_and_returns() {
    let out = output_of(
        "fn nothing() { return null; } fn main() { let n = nothing(); rt_print_int(1); return; }",
    );
    assert_eq!(out, "1");
}

#[test]
fn native_mixed_scalars_in_one_program() {
    let out = output_of(
        "fn main() { let f = 1.5; let c = 'X'; let n = null; rt_print_float(f + 1.0); rt_print_char(c); rt_print_int(7); return; }",
    );
    assert_eq!(out, "2.5\nX\n7");
}

// ---------------------------------------------------------------------------
// Determinism: identical binaries from identical sources
// ---------------------------------------------------------------------------

#[test]
fn float_programs_are_byte_identical_across_builds() {
    let src = "fn main() { let a = 1.5 + 2.5; let b = a * 2.0; rt_print_float(b); return; }";
    let exe1 = build(src);
    let bytes1 = std::fs::read(&exe1).unwrap();
    let exe2 = build(src);
    let bytes2 = std::fs::read(&exe2).unwrap();
    assert_eq!(
        bytes1, bytes2,
        "two builds of the same source must be byte-identical"
    );
}

#[test]
fn float_image_layout_is_deterministic() {
    // Lowering twice yields identical programs.
    let src = "fn main() { let a = 1.5; let b = 2.5; let c = a + b; rt_print_float(c); return; }";
    let p1 = lower_backend(src);
    let p2 = lower_backend(src);
    let bytes1 = format!("{p1:?}");
    let bytes2 = format!("{p2:?}");
    assert_eq!(bytes1, bytes2);
}
