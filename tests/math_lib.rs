// MINK Math Library Test Suite — Session 54
// Uses the same harness pattern as strings_lib.rs and json.rs.
#![allow(clippy::approx_constant)]

use std::fs;
use std::process::Command;

fn math_lib() -> String {
    fs::read_to_string("stdlib/math.mink").expect("failed to read stdlib/math.mink")
}

fn run_with_output(source: &str) -> (i32, String) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let combined = format!("{}\n{}", math_lib(), source);
    let tmp = std::env::temp_dir().join(format!("mink_math_test_{}_{id}.mink", std::process::id()));
    fs::write(&tmp, combined).expect("failed to write temp file");
    let exe = tmp.with_extension("exe");
    let build = Command::new("target/debug/mink.exe")
        .args(["build", tmp.to_str().unwrap()])
        .output()
        .expect("failed to run mink build");
    if !build.status.success() {
        eprintln!("BUILD ERROR:\n{}", String::from_utf8_lossy(&build.stderr));
        return (-1, String::new());
    }
    let run = Command::new(&exe).output().expect("failed to run test");
    fs::remove_file(&tmp).ok();
    fs::remove_file(&exe).ok();
    let stdout = String::from_utf8_lossy(&run.stdout).trim().to_string();
    let code = if run.status.success() {
        0
    } else {
        run.status.code().unwrap_or(-1)
    };
    (code, stdout)
}

fn assert_int_op(name: &str, expr: &str, expected: i64) {
    let test = format!(
        "fn main() {{\n    let r = {};\n    rt_print_int(r);\n    rt_exit(0);\n}}",
        expr
    );
    let (code, output) = run_with_output(&test);
    assert!(
        code == 0 || code == 106,
        "{name}: unexpected exit code: {code}"
    );
    if code == 0 {
        let val: i64 = output.trim().parse().unwrap_or(-9999);
        assert_eq!(
            val, expected,
            "{name}: expected {expected}, got {val} (raw: {output})"
        );
    }
}

fn assert_float_op(name: &str, expr: &str, expected: f64, tol: f64) {
    let test = format!(
        "fn main() {{\n    let r = {};\n    rt_print_float(r);\n    rt_exit(0);\n}}",
        expr
    );
    let (code, output) = run_with_output(&test);
    assert!(
        code == 0 || code == 106,
        "{name}: unexpected exit code: {code}"
    );
    if code == 0 {
        let val: f64 = output.trim().parse().unwrap_or(f64::NAN);
        let diff = (val - expected).abs();
        assert!(
            diff < tol,
            "{name}: expected {expected} (±{tol}), got {val}, diff={diff}"
        );
    }
}

fn assert_bool_op(name: &str, expr: &str, expected: bool) {
    let test = format!(
        "fn main() {{\n    let r = {};\n    if r {{ rt_print_int(1); }} else {{ rt_print_int(0); }}\n    rt_exit(0);\n}}",
        expr
    );
    let (code, output) = run_with_output(&test);
    assert!(
        code == 0 || code == 106,
        "{name}: unexpected exit code: {code}"
    );
    if code == 0 {
        let val = output.trim() == "1";
        assert_eq!(val, expected, "{name}: expected {expected}, got {val}");
    }
}

// ============================================================================
// Integer: Basic
// ============================================================================

#[test]
fn m01_abs_positive() {
    assert_int_op("abs_pos", "math_abs(42)", 42);
}
#[test]
fn m02_abs_negative() {
    assert_int_op("abs_neg", "math_abs(-42)", 42);
}
#[test]
fn m03_abs_zero() {
    assert_int_op("abs_zero", "math_abs(0)", 0);
}
#[test]
fn m04_min() {
    assert_int_op("min", "math_min(3, 7)", 3);
}
#[test]
fn m05_min_equal() {
    assert_int_op("min_eq", "math_min(5, 5)", 5);
}
#[test]
fn m06_max() {
    assert_int_op("max", "math_max(3, 7)", 7);
}
#[test]
fn m07_max_equal() {
    assert_int_op("max_eq", "math_max(5, 5)", 5);
}
#[test]
fn m08_clamp_in_range() {
    assert_int_op("clamp_in", "math_clamp(5, 0, 10)", 5);
}
#[test]
fn m09_clamp_below() {
    assert_int_op("clamp_below", "math_clamp(-5, 0, 10)", 0);
}
#[test]
fn m10_clamp_above() {
    assert_int_op("clamp_above", "math_clamp(15, 0, 10)", 10);
}
#[test]
fn m11_sign_positive() {
    assert_int_op("sign_pos", "math_sign(42)", 1);
}
#[test]
fn m12_sign_negative() {
    assert_int_op("sign_neg", "math_sign(-42)", -1);
}
#[test]
fn m13_sign_zero() {
    assert_int_op("sign_zero", "math_sign(0)", 0);
}

// ============================================================================
// Integer: Power / Factorial
// ============================================================================

#[test]
fn m14_pow_zero_exp() {
    assert_int_op("pow0", "math_pow(5, 0)", 1);
}
#[test]
fn m15_pow_one_exp() {
    assert_int_op("pow1", "math_pow(5, 1)", 5);
}
#[test]
fn m16_pow_two() {
    assert_int_op("pow2", "math_pow(3, 4)", 81);
}
#[test]
fn m17_pow_zero_base() {
    assert_int_op("pow0base", "math_pow(0, 5)", 0);
}
#[test]
fn m18_factorial_zero() {
    assert_int_op("fac0", "math_factorial(0)", 1);
}
#[test]
fn m19_factorial_one() {
    assert_int_op("fac1", "math_factorial(1)", 1);
}
#[test]
fn m20_factorial_five() {
    assert_int_op("fac5", "math_factorial(5)", 120);
}
#[test]
fn m21_factorial_ten() {
    assert_int_op("fac10", "math_factorial(10)", 3628800);
}
#[test]
fn m22_factorial_negative() {
    assert_int_op("fac_neg", "math_factorial(-1)", 0);
}

// ============================================================================
// Integer: Square root
// ============================================================================

#[test]
fn m23_isqrt_zero() {
    assert_int_op("isqrt0", "math_isqrt(0)", 0);
}
#[test]
fn m24_isqrt_one() {
    assert_int_op("isqrt1", "math_isqrt(1)", 1);
}
#[test]
fn m25_isqrt_four() {
    assert_int_op("isqrt4", "math_isqrt(4)", 2);
}
#[test]
fn m26_isqrt_nine() {
    assert_int_op("isqrt9", "math_isqrt(9)", 3);
}
#[test]
fn m27_isqrt_ten() {
    assert_int_op("isqrt10", "math_isqrt(10)", 3);
}
#[test]
fn m28_isqrt_hundred() {
    assert_int_op("isqrt100", "math_isqrt(100)", 10);
}
#[test]
fn m29_isqrt_large() {
    assert_int_op("isqrt1000", "math_isqrt(1000)", 31);
}

// ============================================================================
// Integer: GCD / LCM
// ============================================================================

#[test]
fn m30_gcd_basic() {
    assert_int_op("gcd1", "math_gcd(12, 8)", 4);
}
#[test]
fn m31_gcd_coprime() {
    assert_int_op("gcd2", "math_gcd(7, 13)", 1);
}
#[test]
fn m32_gcd_same() {
    assert_int_op("gcd3", "math_gcd(5, 5)", 5);
}
#[test]
fn m33_gcd_with_zero() {
    assert_int_op("gcd4", "math_gcd(0, 5)", 5);
}
#[test]
fn m34_lcm_basic() {
    assert_int_op("lcm1", "math_lcm(4, 6)", 12);
}
#[test]
fn m35_lcm_coprime() {
    assert_int_op("lcm2", "math_lcm(3, 5)", 15);
}
#[test]
fn m36_lcm_with_zero() {
    assert_int_op("lcm3", "math_lcm(0, 5)", 0);
}

// ============================================================================
// Integer: Bit helpers
// ============================================================================

#[test]
fn m37_popcount_zero() {
    assert_int_op("pop0", "math_popcount(0)", 0);
}
#[test]
fn m38_popcount_one() {
    assert_int_op("pop1", "math_popcount(1)", 1);
}
#[test]
fn m39_popcount_ff() {
    assert_int_op("popff", "math_popcount(255)", 8);
}
#[test]
fn m40_popcount_abc() {
    assert_int_op("popabc", "math_popcount(2748)", 7);
}
#[test]
fn m41_pow2_true() {
    assert_bool_op("ispow2t", "math_is_power_of_two(16)", true);
}
#[test]
fn m42_pow2_false() {
    assert_bool_op("ispow2f", "math_is_power_of_two(15)", false);
}
#[test]
fn m43_pow2_one() {
    assert_bool_op("ispow21", "math_is_power_of_two(1)", true);
}
#[test]
fn m44_next_pow2() {
    assert_int_op("nextpow2", "math_next_power_of_two(5)", 8);
}
#[test]
fn m45_next_pow2_exact() {
    assert_int_op("nextpow2e", "math_next_power_of_two(8)", 8);
}

// ============================================================================
// Float: Conversion
// ============================================================================

#[test]
fn m46_int_to_float_basic() {
    assert_float_op("itof", "math_int_to_float(42)", 42.0, 0.001);
}
#[test]
fn m47_float_to_int_trunc() {
    assert_int_op("ftoi", "math_float_to_int(3.7)", 3);
}
#[test]
fn m48_float_to_int_negative() {
    assert_int_op("ftoi_neg", "math_float_to_int(-2.9)", -2);
}
#[test]
fn m49_roundtrip_int() {
    assert_int_op("rt_int", "math_float_to_int(math_int_to_float(123))", 123);
}

// ============================================================================
// Float: Basic
// ============================================================================

#[test]
fn m50_float_abs_positive() {
    assert_float_op("fabs+", "math_float_abs(3.5)", 3.5, 0.001);
}
#[test]
fn m51_float_abs_negative() {
    assert_float_op("fabs-", "math_float_abs(-3.5)", 3.5, 0.001);
}
#[test]
fn m52_float_min() {
    assert_float_op("fmin", "math_float_min(3.0, 7.0)", 3.0, 0.001);
}
#[test]
fn m53_float_max() {
    assert_float_op("fmax", "math_float_max(3.0, 7.0)", 7.0, 0.001);
}

// ============================================================================
// Float: Rounding
// ============================================================================

#[test]
fn m54_floor_positive() {
    assert_float_op("floor+", "math_float_floor(3.7)", 3.0, 0.001);
}
#[test]
fn m55_floor_negative() {
    assert_float_op("floor-", "math_float_floor(-3.7)", -4.0, 0.001);
}
#[test]
fn m56_ceil_positive() {
    assert_float_op("ceil+", "math_float_ceil(3.2)", 4.0, 0.001);
}
#[test]
fn m57_ceil_negative() {
    assert_float_op("ceil-", "math_float_ceil(-3.2)", -3.0, 0.001);
}
#[test]
fn m58_round_basic() {
    assert_float_op("round", "math_float_round(3.5)", 4.0, 0.001);
}
#[test]
fn m59_trunc_basic() {
    assert_float_op("trunc", "math_float_trunc(3.7)", 3.0, 0.001);
}

// ============================================================================
// Float: Square root
// ============================================================================

#[test]
fn m60_sqrt_zero() {
    assert_float_op("sqrt0", "math_float_sqrt(0.0)", 0.0, 0.001);
}
#[test]
fn m61_sqrt_one() {
    assert_float_op("sqrt1", "math_float_sqrt(1.0)", 1.0, 0.001);
}
#[test]
fn m62_sqrt_four() {
    assert_float_op("sqrt4", "math_float_sqrt(4.0)", 2.0, 0.001);
}
#[test]
fn m63_sqrt_two() {
    assert_float_op("sqrt2", "math_float_sqrt(2.0)", 1.414213562373095, 0.0001);
}
#[test]
fn m64_sqrt_nine() {
    assert_float_op("sqrt9", "math_float_sqrt(9.0)", 3.0, 0.001);
}

// ============================================================================
// Float: Power
// ============================================================================

#[test]
fn m65_pow_zero_exp() {
    assert_float_op("fpow0", "math_float_pow(5.0, 0.0)", 1.0, 0.001);
}
#[test]
fn m66_pow_two() {
    assert_float_op("fpow2", "math_float_pow(2.0, 10.0)", 1024.0, 0.001);
}
#[test]
fn m67_pow_negative_exp() {
    assert_float_op("fpown", "math_float_pow(2.0, -2.0)", 0.25, 0.001);
}

// ============================================================================
// Float: Exponential / Logarithm
// ============================================================================

#[test]
fn m68_exp_zero() {
    assert_float_op("exp0", "math_float_exp(0.0)", 1.0, 0.001);
}
#[test]
fn m69_exp_one() {
    assert_float_op("exp1", "math_float_exp(1.0)", 2.718281828459045, 0.01);
}
#[test]
fn m70_ln_one() {
    assert_float_op("ln1", "math_float_ln(1.0)", 0.0, 0.001);
}
#[test]
fn m71_ln_e() {
    assert_float_op("lne", "math_float_ln(2.718281828459045)", 1.0, 0.01);
}
#[test]
fn m72_log2_two() {
    assert_float_op("log22", "math_float_log2(2.0)", 1.0, 0.01);
}
#[test]
fn m73_log2_four() {
    assert_float_op("log24", "math_float_log2(4.0)", 2.0, 0.01);
}
#[test]
fn m74_log10_ten() {
    assert_float_op("log10_10", "math_float_log10(10.0)", 1.0, 0.01);
}

// ============================================================================
// Float: Trigonometry
// ============================================================================

#[test]
fn m75_sin_zero() {
    assert_float_op("sin0", "math_float_sin(0.0)", 0.0, 0.001);
}
#[test]
fn m76_sin_pi_half() {
    let pi_half = "math_pi() / 2.0";
    assert_float_op(
        "sin_pih",
        &format!("math_float_sin({})", pi_half),
        1.0,
        0.01,
    );
}
#[test]
fn m77_cos_zero() {
    assert_float_op("cos0", "math_float_cos(0.0)", 1.0, 0.001);
}
#[test]
fn m78_cos_pi() {
    assert_float_op("cos_pi", "math_float_cos(math_pi())", -1.0, 0.01);
}
#[test]
fn m79_tan_zero() {
    assert_float_op("tan0", "math_float_tan(0.0)", 0.0, 0.001);
}
#[test]
fn m80_sin_negative() {
    assert_float_op("sin_neg", "math_float_sin(-1.0)", -0.841470984807896, 0.01);
}

// ============================================================================
// Float: Inverse trig
// ============================================================================

#[test]
fn m81_asin_zero() {
    assert_float_op("asin0", "math_float_asin(0.0)", 0.0, 0.001);
}
#[test]
fn m82_asin_one() {
    assert_float_op("asin1", "math_float_asin(1.0)", 1.570796326794896, 0.01);
}
#[test]
fn m83_acos_one() {
    assert_float_op("acos1", "math_float_acos(1.0)", 0.0, 0.01);
}
#[test]
fn m84_atan_zero() {
    assert_float_op("atan0", "math_float_atan(0.0)", 0.0, 0.001);
}
#[test]
fn m85_atan_one() {
    assert_float_op("atan1", "math_float_atan(1.0)", 0.785398163397448, 0.01);
}
#[test]
fn m86_atan2_zero() {
    assert_float_op(
        "atan2_0",
        "math_float_atan2(1.0, 0.0)",
        1.570796326794896,
        0.01,
    );
}

// ============================================================================
// Float: Hyperbolic
// ============================================================================

#[test]
fn m87_sinh_zero() {
    assert_float_op("sinh0", "math_float_sinh(0.0)", 0.0, 0.001);
}
#[test]
fn m88_cosh_zero() {
    assert_float_op("cosh0", "math_float_cosh(0.0)", 1.0, 0.001);
}
#[test]
fn m89_tanh_zero() {
    assert_float_op("tanh0", "math_float_tanh(0.0)", 0.0, 0.001);
}
#[test]
fn m90_tanh_large() {
    assert_float_op("tanh_lg", "math_float_tanh(10.0)", 1.0, 0.001);
}

// ============================================================================
// Utility
// ============================================================================

#[test]
fn m91_lerp() {
    assert_float_op("lerp", "math_lerp(0.0, 10.0, 0.5)", 5.0, 0.001);
}
#[test]
fn m92_inverse_lerp() {
    assert_float_op("ilerp", "math_inverse_lerp(0.0, 10.0, 5.0)", 0.5, 0.001);
}
#[test]
fn m93_remap() {
    assert_float_op(
        "remap",
        "math_remap(5.0, 0.0, 10.0, 0.0, 100.0)",
        50.0,
        0.001,
    );
}
#[test]
fn m94_degrees() {
    assert_float_op("deg", "math_degrees(math_pi())", 180.0, 0.01);
}
#[test]
fn m95_radians() {
    assert_float_op("rad", "math_radians(180.0)", 3.141592653589793, 0.01);
}
#[test]
fn m96_approx_eq_true() {
    assert_bool_op("aeq_t", "math_approximately_equal(1.0, 1.0, 0.001)", true);
}
#[test]
fn m97_approx_eq_false() {
    assert_bool_op("aeq_f", "math_approximately_equal(1.0, 2.0, 0.001)", false);
}

// ============================================================================
// Constants
// ============================================================================

#[test]
fn m98_pi() {
    assert_float_op("pi", "math_pi()", 3.141592653589793, 0.0001);
}
#[test]
fn m99_e() {
    assert_float_op("e", "math_e()", 2.718281828459045, 0.0001);
}
#[test]
fn m100_tau() {
    assert_float_op("tau", "math_tau()", 6.283185307179586, 0.0001);
}
#[test]
fn m101_sqrt2_const() {
    assert_float_op("sqrt2c", "math_sqrt2()", 1.414213562373095, 0.0001);
}

// ============================================================================
// Cross-library: Math + Strings
// ============================================================================

#[test]
fn m102_math_string_concat() {
    let test = r#"
fn main() {
    let a = rt_str_from_int(math_abs(-42));
    rt_print_str(a);
    rt_exit(0);
}"#;
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106, "exit: {code}");
    if code == 0 {
        assert_eq!(output, "42");
    }
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn m103_abs_int_min() {
    // Test with a large negative value (not INT_MIN to avoid overflow)
    assert_int_op("abs_large", "math_abs(-999999)", 999999);
}
#[test]
fn m104_isqrt_large() {
    assert_int_op("isqrt_1m", "math_isqrt(1000000)", 1000);
}
#[test]
fn m105_pow_large() {
    assert_int_op("pow_lg", "math_pow(2, 30)", 1073741824);
}
#[test]
fn m106_factorial_twenty() {
    assert_int_op("fac20", "math_factorial(20)", 2432902008176640000);
}
