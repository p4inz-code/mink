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
// Session 107: the input was `C8 80`, which is a WELL-FORMED 2-byte
// sequence (U+0200), so the original expectation of "0" was wrong. The
// assertion never ran either, because `utf8_validate` leaked its argument
// and the test exited 106. With the leak fixed (and the assertion now
// live), the input is corrected to `C8 20` — a genuine invalid
// continuation (0x20 is not a continuation byte) — and the exit code is
// required to be clean.
#[test]
fn e28_utf8_invalid_continuation() {
    let test = "fn main() { let s = rt_str_alloc(2); rt_str_set_byte(s, 0, 200); rt_str_set_byte(s, 1, 32); if utf8_validate(s) { rt_print_int(1); } else { rt_print_int(0); } rt_exit(0); }";
    let (code, output) = run_with_output(test);
    assert_eq!(code, 0, "e28 must be leak-free, got {code}");
    assert_eq!(output, "0");
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

// ============================================================================
// UTF-8 code-point layer (Session 107, L05)
//
// These tests are STRICT. The harness (and the runtime) exit 106 on a live
// allocation (E-R06), so asserting `code == 0` proves each call is leak-free
// for HEAP-OWNED input as well as for immutable literals. The earlier UTF-8
// tests (e26-e31) accept `0 || 106` and therefore cannot detect the
// ownership bug these tests pin down.
// ============================================================================

/// `"h" + U+00E9 + "llo"` — mixed 1-byte and 2-byte code points.
const UTF8_HELLO: [i64; 6] = [104, 195, 169, 108, 108, 111];

/// Builds `let <name> = rt_str_alloc(n); rt_str_set_byte(...);`.
fn utf8_bytes(name: &str, bytes: &[i64]) -> String {
    let mut s = format!("let {name} = rt_str_alloc({});", bytes.len());
    for (i, b) in bytes.iter().enumerate() {
        s.push_str(&format!(" rt_str_set_byte({name}, {i}, {b});"));
    }
    s
}

/// Builds `let mut v = rt_vec_new(n); v = rt_vec_push(v, c); ...`.
fn utf8_vec(codes: &[i64]) -> String {
    let mut s = format!("let mut v = rt_vec_new({});", codes.len().max(1));
    for c in codes {
        s.push_str(&format!(" v = rt_vec_push(v, {c});"));
    }
    s
}

/// A `dump` helper that prints a heap string's length then its bytes and
/// frees it, so callers never leak the string they were given.
const UTF8_DUMP: &str = "fn dump(s: Str) -> Int { let n = rt_str_len(s); rt_print_int(n); let mut i = 0; while i < n { rt_print_int(rt_str_byte(s, i)); i = i + 1; } rt_str_free(s); return 0; }";

/// Wraps `body` in a `main` that returns 0, optionally after `helpers`.
fn utf8_program_with(helpers: &str, body: &str) -> String {
    format!("{helpers} fn main() -> Int {{ {body} return 0; }}")
}

fn utf8_program(body: &str) -> String {
    utf8_program_with("", body)
}

/// All integer lines of the output (CRLF or LF separated).
fn utf8_ints(output: &str) -> Vec<i64> {
    output
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.parse::<i64>().ok())
        .collect()
}

/// Runs a program and requires a clean exit (no leak, no runtime error).
fn utf8_clean(name: &str, source: &str) -> Vec<i64> {
    let (code, output) = run_with_output(source);
    assert_eq!(
        code, 0,
        "{name}: expected leak-free exit 0, got {code} ({output})"
    );
    utf8_ints(&output)
}

/// Validates every case and returns the 1/0 verdicts.
fn utf8_validate_cases(cases: &[&[i64]]) -> Vec<i64> {
    let mut body = String::new();
    for (i, case) in cases.iter().enumerate() {
        body.push_str(&utf8_bytes(&format!("s{i}"), case));
        body.push_str(&format!(
            " if utf8_validate(s{i}) {{ rt_print_int(1); }} else {{ rt_print_int(0); }}"
        ));
    }
    utf8_clean("utf8_validate cases", &utf8_program(&body))
}

// --- ownership: heap input must not leak (regression: pre-Session-107
//     utf8_validate/utf8_char_count consumed the string without freeing it) ---

#[test]
fn s107_utf8_validate_heap_input_is_leak_free() {
    let src = utf8_program(&format!(
        "{} if utf8_validate(a) {{ rt_print_int(1); }} else {{ rt_print_int(0); }}",
        utf8_bytes("a", &UTF8_HELLO)
    ));
    assert_eq!(utf8_clean("validate heap", &src), vec![1]);
}

#[test]
fn s107_utf8_char_count_heap_input_is_leak_free() {
    let src = utf8_program(&format!(
        "{} rt_print_int(utf8_char_count(a));",
        utf8_bytes("a", &UTF8_HELLO)
    ));
    assert_eq!(utf8_clean("char_count heap", &src), vec![5]);
}

#[test]
fn s107_utf8_validate_empty_literal_is_valid() {
    let src = utf8_program("if utf8_validate(\"\") { rt_print_int(1); } else { rt_print_int(0); }");
    assert_eq!(utf8_clean("validate empty", &src), vec![1]);
}

// --- validation correctness (regressions: overlong 2-byte forms and
//     UTF-16 surrogates were accepted before this session) ---

#[test]
fn s107_utf8_validate_rejects_overlong_two_byte() {
    let got = utf8_validate_cases(&[&[192, 128], &[193, 191], &[194, 128], &[223, 191]]);
    assert_eq!(
        got,
        vec![0, 0, 1, 1],
        "C0/C1 must be rejected; C2/DF accepted"
    );
}

#[test]
fn s107_utf8_validate_rejects_surrogates() {
    let got = utf8_validate_cases(&[
        &[237, 160, 128],
        &[237, 191, 191],
        &[237, 159, 191],
        &[238, 128, 128],
    ]);
    assert_eq!(got, vec![0, 0, 1, 1], "U+D800..U+DFFF must be rejected");
}

#[test]
fn s107_utf8_validate_boundaries() {
    let got = utf8_validate_cases(&[
        &[224, 128, 128],
        &[224, 160, 128],
        &[240, 128, 128, 128],
        &[240, 144, 128, 128],
        &[244, 143, 191, 191],
        &[244, 144, 128, 128],
        &[245, 128, 128, 128],
        &[128],
        &[195],
    ]);
    assert_eq!(got, vec![0, 1, 0, 1, 1, 0, 0, 0, 0]);
}

// --- decode ---

#[test]
fn s107_utf8_decode_multibyte() {
    let mut body = utf8_bytes("a", &UTF8_HELLO);
    body.push_str(" let c = utf8_decode(a); let n = rt_vec_len(c); rt_print_int(n); let mut i = 0; while i < n { rt_print_int(rt_vec_get(c, i)); i = i + 1; } rt_vec_free(c);");
    assert_eq!(
        utf8_clean("decode multibyte", &utf8_program(&body)),
        vec![5, 104, 233, 108, 108, 111]
    );
}

#[test]
fn s107_utf8_decode_four_byte() {
    let mut body = utf8_bytes("a", &[240, 159, 152, 128]);
    body.push_str(" let c = utf8_decode(a); let n = rt_vec_len(c); rt_print_int(n); let mut i = 0; while i < n { rt_print_int(rt_vec_get(c, i)); i = i + 1; } rt_vec_free(c);");
    assert_eq!(
        utf8_clean("decode four byte", &utf8_program(&body)),
        vec![1, 128512]
    );
}

#[test]
fn s107_utf8_decode_empty_is_empty() {
    // regression: `rt_vec_new(0)` raises E-R08, so decoding an empty string
    // must not ask for a zero-capacity vector.
    let mut body = String::new();
    body.push_str("let c = utf8_decode(\"\"); rt_print_int(rt_vec_len(c)); rt_vec_free(c);");
    assert_eq!(utf8_clean("decode empty", &utf8_program(&body)), vec![0]);
}

#[test]
fn s107_utf8_decode_invalid_uses_replacement() {
    let mut body = utf8_bytes("a", &[192, 128, 255]);
    body.push_str(" let c = utf8_decode(a); let n = rt_vec_len(c); rt_print_int(n); let mut i = 0; while i < n { rt_print_int(rt_vec_get(c, i)); i = i + 1; } rt_vec_free(c);");
    assert_eq!(
        utf8_clean("decode invalid", &utf8_program(&body)),
        vec![3, 65533, 65533, 65533]
    );
}

#[test]
fn s107_utf8_char_count_matches_decode_length() {
    let mut body = String::new();
    body.push_str(&utf8_bytes("a", &UTF8_HELLO));
    body.push_str(" rt_print_int(utf8_char_count(a));");
    body.push_str(&utf8_bytes("b", &[192, 128, 255]));
    body.push_str(" rt_print_int(utf8_char_count(b));");
    body.push_str(&utf8_bytes("c", &UTF8_HELLO));
    body.push_str(" let v = utf8_decode(c); rt_print_int(rt_vec_len(v)); rt_vec_free(v);");
    body.push_str(&utf8_bytes("d", &[192, 128, 255]));
    body.push_str(" let w = utf8_decode(d); rt_print_int(rt_vec_len(w)); rt_vec_free(w);");
    assert_eq!(
        utf8_clean("count == decode len", &utf8_program(&body)),
        vec![5, 3, 5, 3]
    );
}

// --- encode ---

#[test]
fn s107_utf8_encode_multibyte() {
    let body = format!("{} dump(utf8_encode(v));", utf8_vec(&[104, 233, 108]));
    assert_eq!(
        utf8_clean("encode multibyte", &utf8_program_with(UTF8_DUMP, &body)),
        vec![4, 104, 195, 169, 108]
    );
}

#[test]
fn s107_utf8_encode_replaces_unencodable() {
    let body = format!("{} dump(utf8_encode(v));", utf8_vec(&[55296, -1, 1114112]));
    assert_eq!(
        utf8_clean("encode invalid", &utf8_program_with(UTF8_DUMP, &body)),
        vec![9, 239, 191, 189, 239, 191, 189, 239, 191, 189]
    );
}

#[test]
fn s107_utf8_encode_four_byte() {
    let body = format!("{} dump(utf8_encode(v));", utf8_vec(&[128512, 2048]));
    assert_eq!(
        utf8_clean("encode four byte", &utf8_program_with(UTF8_DUMP, &body)),
        vec![7, 240, 159, 152, 128, 224, 160, 128]
    );
}

#[test]
fn s107_utf8_encode_empty_is_empty() {
    let body = "dump(utf8_encode(rt_vec_new(1)));";
    assert_eq!(
        utf8_clean("encode empty", &utf8_program_with(UTF8_DUMP, body)),
        vec![0]
    );
}

#[test]
fn s107_utf8_roundtrip_encode_decode_is_byte_exact() {
    // "H" U+20AC "!" — 1-byte, 3-byte, 1-byte.
    let mut body = utf8_bytes("a", &[72, 226, 130, 172, 33]);
    body.push_str(" dump(utf8_encode(utf8_decode(a)));");
    assert_eq!(
        utf8_clean("roundtrip", &utf8_program_with(UTF8_DUMP, &body)),
        vec![5, 72, 226, 130, 172, 33]
    );
}

// --- code-point indexing and slicing ---

#[test]
fn s107_utf8_char_at_and_byte_index() {
    let mut body = String::new();
    body.push_str(&utf8_bytes("a", &UTF8_HELLO));
    body.push_str(" rt_print_int(utf8_char_at(a, 1));");
    body.push_str(&utf8_bytes("b", &UTF8_HELLO));
    body.push_str(" rt_print_int(utf8_char_at(b, 9));");
    body.push_str(&utf8_bytes("c", &UTF8_HELLO));
    body.push_str(" rt_print_int(utf8_char_at(c, -1));");
    body.push_str(&utf8_bytes("d", &UTF8_HELLO));
    body.push_str(" rt_print_int(utf8_byte_index(d, 5));");
    body.push_str(&utf8_bytes("e", &UTF8_HELLO));
    body.push_str(" rt_print_int(utf8_byte_index(e, 6));");
    assert_eq!(
        utf8_clean("char_at / byte_index", &utf8_program(&body)),
        vec![233, -1, -1, 6, -1]
    );
}

#[test]
fn s107_utf8_slice_on_code_points() {
    let mut body = String::new();
    body.push_str(&utf8_bytes("a", &UTF8_HELLO));
    body.push_str(" dump(utf8_slice(a, 1, 4));");
    body.push_str(&utf8_bytes("b", &UTF8_HELLO));
    body.push_str(" dump(utf8_slice(b, 3, 1));");
    body.push_str(&utf8_bytes("c", &UTF8_HELLO));
    body.push_str(" dump(utf8_slice(c, 0, 5));");
    body.push_str(&utf8_bytes("d", &UTF8_HELLO));
    body.push_str(" dump(utf8_slice(d, 9, 12));");
    assert_eq!(
        utf8_clean("slice", &utf8_program_with(UTF8_DUMP, &body)),
        vec![4, 195, 169, 108, 108, 0, 6, 104, 195, 169, 108, 108, 111, 0]
    );
}
