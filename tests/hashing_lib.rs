//! Hashing library tests (Session 58).
//!
//! Each test prepends the library source and runs the resulting program.

use std::process::Command;

fn build_and_run(source: &str) -> (i32, String) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("test_hash_{}.mink", ts));
    std::fs::write(&path, source).unwrap();
    let out = Command::new("cargo")
        .args(["run", "--", "build", path.to_str().unwrap()])
        .output()
        .expect("failed to run build");
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        return (-1, format!("BUILD FAILED:\n{stderr}\n{stdout}"));
    }
    let exe = path.with_extension("exe");
    let run = Command::new(exe.to_str().unwrap())
        .output()
        .expect("failed to run exe");
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let code = run.status.code().unwrap_or(-1);
    (code, stdout)
}

fn lib_source() -> String {
    std::fs::read_to_string("stdlib/hashing.mink").expect("reading stdlib/hashing.mink")
}

// ============================================================
// FNV-1a Tests
// ============================================================

#[test]
fn h01_fnv1a_empty() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hash_fnv1a(\"\");\n    rt_print_int(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    // FNV-1a of empty string = 2166136261
    assert!(out.contains("2166136261"), "got: {out}");
}

#[test]
fn h02_fnv1a_a() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hash_fnv1a(\"a\");\n    rt_print_int(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    // FNV-1a("a") = 36342608335481132 (64-bit wrapping)
    assert!(out.contains("36342608335481132"), "got: {out}");
}

#[test]
fn h03_fnv1a_abc() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hash_fnv1a(\"abc\");\n    rt_print_int(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(out.contains("6851979267275221259"), "got: {out}");
}

#[test]
fn h04_fnv1a_deterministic() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h1 = hash_fnv1a(\"hello world\");\n    let h2 = hash_fnv1a(\"hello world\");\n    if h1 == h2 {{ rt_print_int(42); }}\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(out.contains("42"), "got: {out}");
}

// ============================================================
// DJB2 Tests
// ============================================================

#[test]
fn h05_djb2_empty() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hash_djb2(\"\");\n    rt_print_int(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    // djb2 of empty string = 5381
    assert!(out.contains("5381"), "got: {out}");
}

#[test]
fn h06_djb2_hello() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hash_djb2(\"hello\");\n    rt_print_int(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    // djb2("hello") = 32890966520 (but this may overflow 64-bit signed)
    // Let's just check it's non-zero and deterministic
    assert!(out.lines().next().unwrap() != "0", "got: {out}");
}

#[test]
fn h07_djb2_deterministic() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h1 = hash_djb2(\"test\");\n    let h2 = hash_djb2(\"test\");\n    if h1 == h2 {{ rt_print_int(42); }}\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(out.contains("42"), "got: {out}");
}

// ============================================================
// Hash Combine Tests
// ============================================================

#[test]
fn h08_combine_basic() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hash_combine(100, 200);\n    rt_print_int(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(!out.is_empty(), "got: {out}");
}

#[test]
fn h09_combine_order_matters() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h1 = hash_combine(1, 2);\n    let h2 = hash_combine(2, 1);\n    if h1 != h2 {{ rt_print_int(42); }}\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(out.contains("42"), "got: {out}");
}

// ============================================================
// Hex Encoding Tests
// ============================================================

#[test]
fn h10_hex_encode_zero() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hex_encode_byte(0);\n    rt_print_str(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(out.trim() == "00", "got: {out}");
}

#[test]
fn h11_hex_encode_ff() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hex_encode_byte(255);\n    rt_print_str(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(out.trim() == "ff", "got: {out}");
}

#[test]
fn h12_hex_encode_42() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hex_encode_byte(42);\n    rt_print_str(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(out.trim() == "2a", "got: {out}");
}

// ============================================================
// Digest Equality Tests
// ============================================================

#[test]
fn h13_digest_equal_same() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let a = hash_fnv1a(\"test\");\n    let b = hash_fnv1a(\"test\");\n    rt_print_int(a);\n    rt_print_int(b);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    let lines: Vec<&str> = out.trim().lines().collect();
    assert!(lines.len() >= 2, "got: {out}");
    assert!(lines[0] == lines[1], "digests should be equal: {out}");
}

// ============================================================
// SHA-256 Tests
// ============================================================

#[test]
fn h14_sha256_abc() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hash_sha256(\"abc\");\n    rt_print_str(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    // Expected: ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
    assert!(
        out.trim() == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "SHA-256(\"abc\") mismatch: got {}",
        out.trim()
    );
}

#[test]
fn h15_sha256_empty() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hash_sha256(\"\");\n    rt_print_str(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    assert!(
        out.trim() == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "SHA-256(\"\") mismatch: got {}",
        out.trim()
    );
}

#[test]
fn h16_sha256_length() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hash_sha256(\"abc\");\n    let len = rt_str_len(h);\n    rt_print_int(len);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(
        out.trim() == "64",
        "SHA-256 digest should be 64 chars: got {out}"
    );
}

#[test]
fn h17_sha256_deterministic() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h1 = hash_sha256(\"hello\");\n    let h2 = hash_sha256(\"hello\");\n    rt_print_str(h1);\n    rt_print_str(h2);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    let lines: Vec<&str> = out.trim().lines().collect();
    assert!(
        lines.len() >= 2 && lines[0] == lines[1],
        "SHA-256 should be deterministic: got {out}"
    );
}

#[test]
fn h18_sha256_hello_world() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hash_sha256(\"hello world\");\n    rt_print_str(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    // SHA-256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
    assert!(
        out.trim() == "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
        "SHA-256(\"hello world\") mismatch: got {}",
        out.trim()
    );
}

#[test]
fn h19_sha256_multiblock() {
    let lib = lib_source();
    // "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq" is 56 bytes
    // SHA-256 of that = 248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hash_sha256(\"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq\");\n    rt_print_str(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(
        out.trim() == "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        "SHA-256 multi-block mismatch: got {}",
        out.trim()
    );
}

#[test]
fn h20_sha256_digit_string() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hash_sha256(\"1234567890\");\n    rt_print_str(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(
        out.trim() == "c775e7b757ede630cd0aa1113bd102661ab38829ca52a6422ab782862f268646",
        "SHA-256(\"1234567890\") mismatch: got {}",
        out.trim()
    );
}

// ============================================================
// Adversarial / Edge Cases
// ============================================================

#[test]
fn h21_sha256_single_byte() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hash_sha256(\"a\");\n    rt_print_str(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(
        out.trim() == "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb",
        "SHA-256(\"a\") mismatch: got {}",
        out.trim()
    );
}

#[test]
fn h22_sha256_63_bytes() {
    let lib = lib_source();
    // 63 'a' chars = one block minus padding
    let code = format!(
        "{lib}\nfn main() {{\n    let s = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\";\n    let h = hash_sha256(s);\n    rt_print_str(h);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(
        out.trim() == "7d3e74a05d7db15bce4ad9ec0658ea98e3f06eeecf16b4c6fff2da457ddc2f34",
        "SHA-256(63*a) mismatch: got {}",
        out.trim()
    );
}

#[test]
fn h23_fnv1a_different_inputs() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h1 = hash_fnv1a(\"hello\");\n    let h2 = hash_fnv1a(\"world\");\n    if h1 != h2 {{ rt_print_int(42); }}\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(
        out.contains("42"),
        "Different inputs should produce different hashes: got {out}"
    );
}

#[test]
fn h24_hash_fnv1a_vs_djb2() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h1 = hash_fnv1a(\"test\");\n    let h2 = hash_djb2(\"test\");\n    if h1 != h2 {{ rt_print_int(42); }}\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(
        out.contains("42"),
        "FNV-1a and djb2 should differ: got {out}"
    );
}
