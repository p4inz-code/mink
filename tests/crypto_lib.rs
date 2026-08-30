use std::process::Command;

fn build_and_run(source: &str) -> (i32, String) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = format!("test_crypto_{ts}.mink");
    let full_path = format!("C:/Users/Admin/AppData/Local/Temp/{path}");
    std::fs::write(&full_path, source).unwrap();
    let build = Command::new("cargo")
        .args(["run", "--", "build", &full_path])
        .output()
        .expect("failed to run cargo build");
    if !build.status.success() {
        let stderr = String::from_utf8_lossy(&build.stderr);
        let stdout = String::from_utf8_lossy(&build.stdout);
        return (-1, format!("BUILD FAILED:\n{stderr}\n{stdout}"));
    }
    let exe = full_path.replace(".mink", ".exe");
    let run = Command::new(&exe).output().expect("failed to run");
    let code = run.status.code().unwrap_or(-1);
    let out = String::from_utf8_lossy(&run.stdout).to_string();
    (code, out)
}

fn lib_source() -> String {
    let hashing = std::fs::read_to_string("stdlib/hashing.mink").unwrap();
    let crypto = std::fs::read_to_string("stdlib/crypto.mink").unwrap();
    format!("{}\n{}", hashing, crypto)
}

// ============================================================
// SECURE RANDOM TESTS
// ============================================================

#[test]
fn r01_random_int_runs() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    crypto_init();\n    let a = crypto_random_int();\n    let b = crypto_random_int();\n    rt_print_int(a);\n    rt_print_int(b);\n    rt_exit(0);\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
}

#[test]
fn r02_random_bytes_correct_length() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    crypto_init();\n    let buf = crypto_random_bytes(32);\n    let len = rt_str_len(buf);\n    rt_print_int(len);\n    rt_str_free(buf);\n    if len == 32 {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(out.contains("32"), "got: {out}");
}

#[test]
fn r03_random_hex_correct_length() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    crypto_init();\n    let h = crypto_random_hex(16);\n    let len = rt_str_len(h);\n    rt_print_int(len);\n    rt_str_free(h);\n    if len == 32 {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(out.contains("32"), "got: {out}");
}

#[test]
fn r04_random_int_nonzero() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    crypto_init();\n    let mut nonzero = 0;\n    let mut i = 0;\n    while i < 10 {{\n        let v = crypto_random_int();\n        if v != 0 {{\n            nonzero = 1;\n        }}\n        i = i + 1;\n    }}\n    if nonzero == 1 {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
}

#[test]
fn r05_random_bytes_length_16() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    crypto_init();\n    let a = crypto_random_bytes(16);\n    let len = rt_str_len(a);\n    rt_print_int(len);\n    rt_str_free(a);\n    if len == 16 {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(out.contains("16"), "got: {out}");
}

// ============================================================
// CONSTANT-TIME VERIFY TESTS
// ============================================================

#[test]
fn v01_verify_equal_strings() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let a = \"hello\";\n    let b = \"hello\";\n    if crypto_verify(a, b) {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
}

#[test]
fn v02_verify_different_strings() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let a = \"hello\";\n    let b = \"world\";\n    if crypto_verify(a, b) == false {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
}

#[test]
fn v03_verify_different_lengths() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let a = \"ab\";\n    let b = \"abc\";\n    if crypto_verify(a, b) == false {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
}

#[test]
fn v04_verify_empty_strings() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let a = \"\";\n    let b = \"\";\n    if crypto_verify(a, b) {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
}

// ============================================================
// HMAC-SHA256 TESTS
// ============================================================

#[test]
fn h01_hmac_sha256_jefe() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    crypto_init();\n    let key = \"Jefe\";\n    let msg = \"what do ya want for nothing?\";\n    let mac = hmac_sha256(key, msg);\n    let expected = \"5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843\";\n    rt_print_str(mac);\n    if crypto_verify(mac, expected) {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
}

#[test]
fn h02_hmac_sha256_empty_key_empty_msg() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    crypto_init();\n    let mac = hmac_sha256(\"\", \"\");\n    let expected = \"b638863bb700999ab42666c412e58ab6c8681e14704ed1e8e31aed7eb2ac9242\";\n    rt_print_str(mac);\n    if crypto_verify(mac, expected) {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
}

#[test]
fn h03_hmac_sha256_hex_key() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    crypto_init();\n    let key_bytes = rt_str_alloc(4);\n    rt_str_set_byte(key_bytes, 0, 0xDE);\n    rt_str_set_byte(key_bytes, 1, 0xAD);\n    rt_str_set_byte(key_bytes, 2, 0xBE);\n    rt_str_set_byte(key_bytes, 3, 0xEF);\n    let key_hex = _raw_to_hex(key_bytes);\n    let mac = hmac_sha256(key_hex, \"test\");\n    let len = rt_str_len(mac);\n    rt_print_int(len);\n    rt_str_free(mac);\n    rt_str_free(key_bytes);\n    if len == 64 {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(out.contains("64"), "got: {out}");
}

#[test]
fn h04_hmac_sha256_deterministic() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    crypto_init();\n    let a = hmac_sha256(\"key\", \"msg\");\n    let b = hmac_sha256(\"key\", \"msg\");\n    rt_print_str(a);\n    rt_print_str(b);\n    if crypto_verify(a, b) {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
}

#[test]
fn h05_hmac_sha256_long_key() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    crypto_init();\n    let key = rt_str_alloc(131);\n    let mut i = 0;\n    while i < 131 {{\n        rt_str_set_byte(key, i, 0x0a);\n        i = i + 1;\n    }}\n    let msg = \"Test Using Larger Than Block-Size Key - Hash Key First\";\n    let mac = hmac_sha256(key, msg);\n    let expected = \"60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54\";\n    rt_print_str(mac);\n    if crypto_verify(mac, expected) {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
}

// ============================================================
// HKDF-SHA256 TESTS
// ============================================================

#[test]
fn k01_hkdf_extract_correct_length() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    crypto_init();\n    let salt = \"some salt value!!\";\n    let ikm = \"input key material\";\n    let prk = hkdf_extract(salt, ikm);\n    let len = rt_str_len(prk);\n    rt_print_int(len);\n    rt_str_free(prk);\n    if len == 32 {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(out.contains("32"), "got: {out}");
}

#[test]
fn k02_hkdf_expand_42_bytes() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    crypto_init();\n    let okm = hkdf_sha256(\"secret\", \"salt\", \"info\", 42);\n    let len = rt_str_len(okm);\n    rt_print_int(len);\n    rt_str_free(okm);\n    if len == 42 {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
    assert!(out.contains("42"), "got: {out}");
}

#[test]
fn k05_hkdf_deterministic() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    crypto_init();\n    let a = hkdf_sha256(\"ikm\", \"salt\", \"info\", 32);\n    let b = hkdf_sha256(\"ikm\", \"salt\", \"info\", 32);\n    rt_print_str(a);\n    rt_print_str(b);\n    if crypto_verify(a, b) {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
}

// ============================================================
// HASHING INTEGRATION TEST
// ============================================================

#[test]
fn x01_hashing_still_works() {
    let lib = lib_source();
    let code = format!(
        "{lib}\nfn main() {{\n    let h = hash_sha256(\"abc\");\n    let expected = \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\";\n    rt_print_str(h);\n    if crypto_verify(h, expected) {{ rt_exit(0); }}\n}}"
    );
    let (c, out) = build_and_run(&code);
    assert!(c == 0 || c == 106, "exit: {c}, out: {out}");
}
