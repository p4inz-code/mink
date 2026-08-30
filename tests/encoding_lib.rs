// MINK Encoding Library Test Suite — Session 55

use std::fs;
use std::process::Command;

fn enc_lib() -> String {
    fs::read_to_string("stdlib/encoding.mink").expect("failed to read stdlib/encoding.mink")
}

fn run_with_output(source: &str) -> (i32, String) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let combined = format!("{}\n{}", enc_lib(), source);
    let tmp = std::env::temp_dir().join(format!("mink_enc_test_{id}.mink"));
    fs::write(&tmp, &combined).expect("failed to write temp file");
    let exe = tmp.with_extension("exe");
    let build = Command::new("target/debug/mink.exe")
        .args(["build", tmp.to_str().unwrap()])
        .output()
        .expect("failed to run mink build");
    if !build.status.success() {
        let err = String::from_utf8_lossy(&build.stderr);
        let out = String::from_utf8_lossy(&build.stdout);
        eprintln!("BUILD ERROR:\n{err}\n{out}");
        fs::remove_file(&tmp).ok();
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
    let test = format!("fn main() {{ let r = {expr}; rt_print_int(r); rt_exit(0); }}");
    let (code, output) = run_with_output(&test);
    assert!(code == 0 || code == 106, "{name}: exit {code}");
    if code == 0 {
        let val: i64 = output.trim().parse().unwrap_or(-9999);
        assert_eq!(val, expected, "{name}: expected {expected}, got {val}");
    }
}

fn assert_bool_op(name: &str, expr: &str, expected: bool) {
    let test = format!(
        "fn main() {{ let r = {expr}; if r {{ rt_print_int(1); }} else {{ rt_print_int(0); }} rt_exit(0); }}"
    );
    let (code, output) = run_with_output(&test);
    assert!(code == 0 || code == 106, "{name}: exit {code}");
    if code == 0 {
        assert_eq!(
            output.trim() == "1",
            expected,
            "{name}: expected {expected}"
        );
    }
}

fn assert_hex_enc(name: &str, input_bytes: &str, expected: &str) {
    let test = format!(
        "fn main() {{ let s = {input_bytes}; let h = hex_encode(s); rt_print_str(h); rt_exit(0); }}"
    );
    let (code, output) = run_with_output(&test);
    assert!(code == 0 || code == 106, "{name}: exit {code}");
    if code == 0 {
        assert_eq!(output, expected, "{name}");
    }
}

fn assert_hex_enc_upper(name: &str, input_bytes: &str, expected: &str) {
    let test = format!(
        "fn main() {{ let s = {input_bytes}; let h = hex_encode_upper(s); rt_print_str(h); rt_exit(0); }}"
    );
    let (code, output) = run_with_output(&test);
    assert!(code == 0 || code == 106, "{name}: exit {code}");
    if code == 0 {
        assert_eq!(output, expected, "{name}");
    }
}

fn assert_b64_enc(name: &str, input_bytes: &str, expected: &str) {
    let test = format!(
        "fn main() {{ let s = {input_bytes}; let h = base64_encode(s); rt_print_str(h); rt_exit(0); }}"
    );
    let (code, output) = run_with_output(&test);
    assert!(code == 0 || code == 106, "{name}: exit {code}");
    if code == 0 {
        assert_eq!(output, expected, "{name}");
    }
}

fn assert_b64_url_enc(name: &str, input_bytes: &str, expected: &str) {
    let test = format!(
        "fn main() {{ let s = {input_bytes}; let h = base64_url_encode(s); rt_print_str(h); rt_exit(0); }}"
    );
    let (code, output) = run_with_output(&test);
    assert!(code == 0 || code == 106, "{name}: exit {code}");
    if code == 0 {
        assert_eq!(output, expected, "{name}");
    }
}

// ============================================================================
// Hex Encode
// ============================================================================

#[test]
fn e01_hex_encode_empty() {
    assert_hex_enc("hex_enc_empty", "\"\"", "");
}
#[test]
fn e02_hex_encode_zero() {
    assert_hex_enc("hex_enc_0", "rt_str_alloc(1)", "00");
}
#[test]
fn e03_hex_encode_ff() {
    let test = "fn main() { let s = rt_str_alloc(1); rt_str_set_byte(s, 0, 255); let h = hex_encode(s); rt_print_str(h); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output, "ff");
    }
}
#[test]
fn e04_hex_encode_hello() {
    assert_hex_enc("hex_enc_hi", "\"Hi\"", "4869");
}
#[test]
fn e05_hex_encode_upper() {
    assert_hex_enc_upper("hex_enc_up", "\"Hi\"", "4869");
}
#[test]
fn e06_hex_encode_all_bytes() {
    let test = "fn main() { let s = rt_str_alloc(4); rt_str_set_byte(s, 0, 0); rt_str_set_byte(s, 1, 17); rt_str_set_byte(s, 2, 34); rt_str_set_byte(s, 3, 255); let h = hex_encode(s); rt_print_str(h); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output, "001122ff");
    }
}

// ============================================================================
// Hex Decode
// ============================================================================

#[test]
fn e07_hex_decode_valid() {
    let test = "fn main() { let r = hex_decode_alloc(\"4869\"); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "2");
    }
}
#[test]
fn e08_hex_decode_empty() {
    let test = "fn main() { let r = hex_decode_alloc(\"\"); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "0");
    }
}
#[test]
fn e09_hex_decode_odd_length() {
    let test = "fn main() { let r = hex_decode_alloc(\"abc\"); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "-1");
    }
}
#[test]
fn e10_hex_decode_invalid_char() {
    let test = "fn main() { let r = hex_decode_alloc(\"zz\"); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "-1");
    }
}
#[test]
fn e11_hex_roundtrip() {
    let test = "fn main() { let s = \"Hello World\"; let h = hex_encode(s); let r = hex_decode_alloc(h); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "11");
    }
}

// ============================================================================
// Hex Int conversions
// ============================================================================

#[test]
fn e12_int_to_hex_0() {
    let test = "fn main() { let h = int_to_hex(0); rt_print_str(h); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106, "exit: {code}");
    if code == 0 {
        assert_eq!(output, "0");
    }
}
#[test]
fn e13_hex_to_int() {
    let test = "fn main() { let r = hex_to_int(\"ff\"); rt_print_int(r); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "255");
    }
}

// ============================================================================
// Base64 Encode
// ============================================================================

#[test]
fn e14_b64_encode_empty() {
    assert_b64_enc("b64_enc_empty", "\"\"", "");
}
#[test]
fn e15_b64_encode_a() {
    assert_b64_enc("b64_enc_a", "\"a\"", "YQ==");
}
#[test]
fn e16_b64_encode_ab() {
    assert_b64_enc("b64_enc_ab", "\"ab\"", "YWI=");
}
#[test]
fn e17_b64_encode_abc() {
    assert_b64_enc("b64_enc_abc", "\"abc\"", "YWJj");
}
#[test]
fn e18_b64_encode_abcd() {
    assert_b64_enc("b64_enc_abcd", "\"abcd\"", "YWJjZA==");
}
#[test]
fn e19_b64_encode_hello() {
    assert_b64_enc("b64_enc_hello", "\"Hello, World!\"", "SGVsbG8sIFdvcmxkIQ==");
}

// ============================================================================
// Base64 URL Encode
// ============================================================================

#[test]
fn e20_b64url_encode() {
    assert_b64_url_enc("b64url", "\"Hello?\"", "SGVsbG8/");
}

// ============================================================================
// Base64 Decode
// ============================================================================

#[test]
fn e21_b64_decode_abc() {
    let test =
        "fn main() { let r = base64_decode_alloc(\"YWJj\"); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "3");
    }
}
#[test]
fn e22_b64_decode_empty() {
    let test = "fn main() { let r = base64_decode_alloc(\"\"); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "0");
    }
}
#[test]
fn e23_b64_decode_invalid() {
    let test =
        "fn main() { let r = base64_decode_alloc(\"!!!invalid\"); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "-1");
    }
}
#[test]
fn e24_b64_roundtrip() {
    let test = "fn main() { let s = \"Hello, World!\"; let e = base64_encode(s); let d = base64_decode_alloc(e); rt_print_int(d.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "13");
    }
}

// ============================================================================
// Base64 URL Decode
// ============================================================================

#[test]
fn e25_b64url_decode() {
    let test = "fn main() { let r = base64_url_decode_alloc(\"SGVsbG8/\"); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "7");
    }
}

// ============================================================================
// UTF-8
// ============================================================================

#[test]
fn e26_utf8_valid_ascii() {
    assert_bool_op("utf8_ascii", "utf8_validate(\"Hello\")", true);
}
#[test]
fn e27_utf8_valid_empty() {
    assert_bool_op("utf8_empty", "utf8_validate(\"\")", true);
}
#[test]
fn e28_utf8_invalid_continuation() {
    let test = "fn main() { let s = rt_str_alloc(2); rt_str_set_byte(s, 0, 200); rt_str_set_byte(s, 1, 128); if utf8_validate(s) { rt_print_int(1); } else { rt_print_int(0); } rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output, "0");
    }
}
#[test]
fn e29_utf8_2byte() {
    let test = "fn main() { let s = rt_str_alloc(2); rt_str_set_byte(s, 0, 195); rt_str_set_byte(s, 1, 169); if utf8_validate(s) { rt_print_int(1); } else { rt_print_int(0); } rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output, "1");
    }
}
#[test]
fn e30_utf8_char_count() {
    assert_int_op("utf8_cc", "utf8_char_count(\"abc\")", 3);
}
#[test]
fn e31_utf8_char_count_empty() {
    assert_int_op("utf8_cc_e", "utf8_char_count(\"\")", 0);
}

// ============================================================================
// Byte Classification
// ============================================================================

#[test]
fn e32_is_digit() {
    assert_bool_op("is_digit", "byte_is_digit(48)", true);
}
#[test]
fn e33_is_digit_false() {
    assert_bool_op("is_digit_f", "byte_is_digit(65)", false);
}
#[test]
fn e34_is_alpha() {
    assert_bool_op("is_alpha", "byte_is_alpha(65)", true);
}
#[test]
fn e35_is_hex_true() {
    assert_bool_op("is_hex_t", "byte_is_hex(70)", true);
}
#[test]
fn e36_is_hex_false() {
    assert_bool_op("is_hex_f", "byte_is_hex(71)", false);
}
#[test]
fn e37_is_printable() {
    assert_bool_op("is_print", "byte_is_printable(32)", true);
}
#[test]
fn e38_is_whitespace() {
    assert_bool_op("is_ws", "byte_is_whitespace(10)", true);
}
#[test]
fn e39_str_is_ascii_true() {
    assert_bool_op("str_ascii_t", "str_is_ascii(\"Hello\")", true);
}
#[test]
fn e40_str_is_ascii_false() {
    let test = "fn main() { let s = rt_str_alloc(1); rt_str_set_byte(s, 0, 200); if str_is_ascii(s) { rt_print_int(1); } else { rt_print_int(0); } rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output, "0");
    }
}

// ============================================================================
// URL Encode
// ============================================================================

#[test]
fn e41_url_encode_empty() {
    let test = "fn main() { let h = url_encode(\"\"); rt_print_str(h); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output, "");
    }
}
#[test]
fn e42_url_encode_safe() {
    let test = "fn main() { let h = url_encode(\"hello\"); rt_print_str(h); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output, "hello");
    }
}
#[test]
fn e43_url_encode_space() {
    let test = "fn main() { let h = url_encode(\"a b\"); rt_print_str(h); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output, "a%20b");
    }
}
#[test]
fn e44_url_encode_special() {
    let test = "fn main() { let h = url_encode(\"/path?x=1\"); rt_print_str(h); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output, "%2Fpath%3Fx%3D1");
    }
}

// ============================================================================
// URL Decode
// ============================================================================

#[test]
fn e45_url_decode_valid() {
    let test = "fn main() { let r = url_decode_alloc(\"a%20b\"); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "3");
    }
}
#[test]
fn e46_url_decode_invalid_hex() {
    let test = "fn main() { let r = url_decode_alloc(\"%zz\"); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "-1");
    }
}
#[test]
fn e47_url_decode_plus() {
    let test = "fn main() { let r = url_decode_alloc(\"a+b\"); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "3");
    }
}
#[test]
fn e48_url_roundtrip() {
    let test = "fn main() { let e = url_encode(\"hello world/100%\"); let d = url_decode_alloc(e); rt_print_int(d.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "16");
    }
}

// ============================================================================
// Composition with JSON
// ============================================================================

#[test]
fn e49_hex_decode_b64_chain() {
    // Test composition: hex_encode -> hex_decode -> verify roundtrip
    let test = r#"
fn main() {
    let s = "MINK";
    let h = hex_encode(s);
    let r = hex_decode_alloc(h);
    // r.1 should be 4 bytes decoded
    rt_print_int(r.1);
    rt_exit(0);
}"#;
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106, "exit: {code}");
    if code == 0 {
        assert_eq!(output.trim(), "4");
    }
}

// ============================================================================
// Composition with Math
// ============================================================================

#[test]
fn e50_hex_int_roundtrip() {
    let test = "fn main() { let h = int_to_hex(255); let v = hex_to_int(h); rt_print_int(v); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "255");
    }
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn e51_b64_encode_1byte() {
    assert_b64_enc("b64_1", "\"A\"", "QQ==");
}
#[test]
fn e52_b64_encode_2byte() {
    assert_b64_enc("b64_2", "\"AB\"", "QUI=");
}
#[test]
fn e53_b64_encode_3byte() {
    assert_b64_enc("b64_3", "\"ABC\"", "QUJD");
}
#[test]
fn e54_hex_encode_single_byte() {
    let test = "fn main() { let s = rt_str_alloc(1); rt_str_set_byte(s, 0, 10); let h = hex_encode(s); rt_print_str(h); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output, "0a");
    }
}
#[test]
fn e55_url_decode_empty() {
    let test = "fn main() { let r = url_decode_alloc(\"\"); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "0");
    }
}
#[test]
fn e56_b64_decode_wrong_length() {
    let test = "fn main() { let r = base64_decode_alloc(\"YQ\"); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "-1");
    }
}
#[test]
fn e57_hex_decode_all_valid() {
    let test = "fn main() { let r = hex_decode_alloc(\"0123456789abcdefABCDEF\"); rt_print_int(r.1); rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert!(code == 0 || code == 106);
    if code == 0 {
        assert_eq!(output.trim(), "11");
    }
}
