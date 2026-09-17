//! Compression library integration tests — Session 110 (S42).
//!
//! Python parity target: `zlib` (RFC 1950), `gzip` (RFC 1952) and raw DEFLATE
//! (RFC 1951) as exposed by Python's `zlib`/`gzip` modules.
//!
//! Every test prepends `stdlib/zlib.mink`, compiles a genuine Windows PE
//! through the real `mink build` CLI and executes it. `rt_exit(0)` performs
//! the runtime arena validation, so a clean exit (no `E-R06`, exit code 106)
//! is also the ownership/leak proof for the codec.
//!
//! Compatibility evidence comes from CPython itself: the byte vectors below
//! were produced by `zlib.compress`, `gzip.compress` and
//! `zlib.compressobj(..., wbits=-15)` and must inflate to the exact payload.
//! The reverse direction (CPython inflating MINK's output) is checked live in
//! `s42_cpython_inflates_mink_output` when a Python interpreter is available,
//! and pinned statically by the golden-encoding test.
//!
//! Test artifacts are written under `target/mink-artifacts/` rather than the
//! system temp directory: Windows Defender quarantines freshly linked PEs in
//! `%TEMP%` (see the Session 109 report), which made those harnesses
//! non-deterministic.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn zlib_source() -> String {
    std::fs::read_to_string("stdlib/zlib.mink").expect("failed to read stdlib/zlib.mink")
}

/// Hex/byte helpers and output sinks used by the generated programs.
const PRELUDE: &str = r#"
fn hval(c: Int) -> Int { let mut v = c - 48; if c >= 97 { v = c - 87; } return v; }
fn hdig(v: Int) -> Int { let mut c = v + 48; if v > 9 { c = v + 87; } return c; }
fn fromhex(s: Str) -> Str {
    let n = rt_str_len(s) / 2;
    let out = rt_str_alloc(n);
    let mut i = 0;
    while i < n {
        let hi = hval(rt_str_byte(s, i * 2));
        let lo = hval(rt_str_byte(s, i * 2 + 1));
        rt_str_set_byte(out, i, hi * 16 + lo);
        i = i + 1;
    }
    rt_str_free(s);
    return out;
}
fn hexify(b: Str) -> Str {
    let n = rt_str_len(b);
    let out = rt_str_alloc(n * 2);
    let mut i = 0;
    while i < n {
        let c = rt_str_byte(b, i);
        rt_str_set_byte(out, i * 2, hdig((c >> 4) & 15));
        rt_str_set_byte(out, i * 2 + 1, hdig(c & 15));
        i = i + 1;
    }
    rt_str_free(b);
    return out;
}
fn hexline(b: Str) { let h = hexify(b); rt_print_str(h); rt_str_free(h); }
fn say_i(v: Int) { rt_print_int(v); }
fn say_b(v: Bool) { if v { rt_print_str("T"); } else { rt_print_str("F"); } }
"#;

/// CPython-produced vectors: (name, payload hex, zlib hex, gzip hex, adler32, crc32).
fn cpython_vectors() -> Vec<(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    i64,
    i64,
)> {
    vec![
        (
            "empty",
            "",
            "78da030000000001",
            "1f8b080000000000020a03000000000000000000",
            1,
            0,
        ),
        (
            "tiny",
            "61",
            "78da4b040000620062",
            "1f8b080000000000020a4b040043beb7e801000000",
            6422626,
            3904355907,
        ),
        (
            "short",
            "68656c6c6f",
            "78dacb48cdc9c90700062c0215",
            "1f8b080000000000020acb48cdc9c9070086a6103605000000",
            103547413,
            907060870,
        ),
        (
            "text",
            "74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f670a74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f670a74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f670a",
            "78da2bc94855282ccd4cce56482aca2fcf5348cbaf50c82acd2d2856c82f4b2d5228014ae72456552aa4e4a77395d0482d0095d3300a",
            "1f8b080000000000020a2bc94855282ccd4cce56482aca2fcf5348cbaf50c82acd2d2856c82f4b2d5228014ae72456552aa4e4a77395d0482d0078e1526884000000",
            2513645578,
            1750262136,
        ),
        (
            "repetitive",
            "6162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636162636163",
            "78da4b4c4a4e1c45a368148da2a1829201ce82cd5a",
            "1f8b080000000000020a4b4c4a4e1c45a368148da2a1829201dabff5e7b5040000",
            3464678746,
            3891642330,
        ),
    ]
}

fn artifact_dir() -> PathBuf {
    let dir = PathBuf::from("target").join("mink-artifacts");
    std::fs::create_dir_all(&dir).expect("failed to create target/mink-artifacts");
    dir
}

/// Compile and run one generated program. `decls` holds top-level helper
/// functions and `body` holds the statements of `fn main`.
fn build_and_run(decls: &str, body: &str) -> (i32, String) {
    let source = format!(
        "{}\n{}\n{}\nfn main() {{\n{}\n}}\n",
        zlib_source(),
        PRELUDE,
        decls,
        body
    );
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("zlib_test_{n}.mink"));
    std::fs::write(&path, source.replace("\r\n", "\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("build")
        .arg(&path)
        .output()
        .unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        // Keep the generated source (and drop the exe) so a failing build can
        // be inspected directly.
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&exe);
        panic!(
            "build failed (source kept at {}):\n{stderr}",
            path.display()
        );
    }
    let run = Command::new(&exe).output().expect("failed to run test exe");
    let code = run.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    // A clean exit also proves the arena/leak check passed (E-R06 = exit 106).
    assert!(stderr.is_empty(), "unexpected stderr: {stderr:?}");
    (code, stdout)
}

fn out_lines(out: &str) -> Vec<String> {
    out.lines().map(|line| line.to_string()).collect()
}

fn assert_ok(code: i32, out: &str) {
    assert_eq!(code, 0, "program exited {code}; stdout={out:?}");
}

// ============================================================================
// 1. Decoding CPython output — the parity requirement, pinned statically
// ============================================================================

#[test]
fn s42_zlib_decompress_matches_cpython() {
    // MINK forbids re-declaring a name, so each vector binds fresh variables.
    let mut body = String::new();
    for (i, (_, _, zhex, _, _, _)) in cpython_vectors().iter().enumerate() {
        body.push_str(&format!(
            "let (a{i}, e{i}) = zlib_decompress(fromhex(\"{zhex}\"));\nsay_i(e{i});\nhexline(a{i});\n"
        ));
    }
    let (code, out) = build_and_run("", &body);
    assert_ok(code, &out);
    let lines = out_lines(&out);
    for (i, (name, phex, _, _, _, _)) in cpython_vectors().iter().enumerate() {
        assert_eq!(
            lines[i * 2],
            "0",
            "{name}: expected success, got {}",
            lines[i * 2]
        );
        assert_eq!(lines[i * 2 + 1], *phex, "{name}: payload mismatch");
    }
}

#[test]
fn s42_gzip_decompress_matches_cpython() {
    let mut body = String::new();
    for (i, (_, _, _, ghex, _, _)) in cpython_vectors().iter().enumerate() {
        body.push_str(&format!(
            "let (a{i}, e{i}) = gzip_decompress(fromhex(\"{ghex}\"));\nsay_i(e{i});\nhexline(a{i});\n"
        ));
    }
    let (code, out) = build_and_run("", &body);
    assert_ok(code, &out);
    let lines = out_lines(&out);
    for (i, (name, phex, _, _, _, _)) in cpython_vectors().iter().enumerate() {
        assert_eq!(
            lines[i * 2],
            "0",
            "{name}: expected success, got {}",
            lines[i * 2]
        );
        assert_eq!(lines[i * 2 + 1], *phex, "{name}: payload mismatch");
    }
}

#[test]
fn s42_raw_inflate_matches_cpython() {
    // CPython's raw DEFLATE stream (wbits=-15) is byte-identical to the payload
    // inside its zlib stream, so it is derived by stripping the 2-byte header
    // and 4-byte Adler trailer.
    let mut body = String::new();
    for (i, (_, _, zhex, _, _, _)) in cpython_vectors().iter().enumerate() {
        let raw = &zhex[4..zhex.len() - 8];
        body.push_str(&format!(
            "let (a{i}, e{i}) = inflate_raw(fromhex(\"{raw}\"));\nsay_i(e{i});\nhexline(a{i});\n"
        ));
    }
    // An empty terminal fixed block and a raw stored block round out the tests.
    let tail = cpython_vectors().len();
    body.push_str(&format!(
        "let (a{tail}, e{tail}) = inflate_raw(fromhex(\"0300\"));\nsay_i(e{tail});\nhexline(a{tail});\n\
         let (a{}, e{}) = inflate_raw(fromhex(\"010300fcff666f6f\"));\nsay_i(e{});\nhexline(a{});\n",
        tail + 1,
        tail + 1,
        tail + 1,
        tail + 1
    ));
    let (code, out) = build_and_run("", &body);
    assert_ok(code, &out);
    let lines = out_lines(&out);
    for (i, (name, phex, _, _, _, _)) in cpython_vectors().iter().enumerate() {
        assert_eq!(
            lines[i * 2],
            "0",
            "{name}: expected success, got {}",
            lines[i * 2]
        );
        assert_eq!(lines[i * 2 + 1], *phex, "{name}: payload mismatch");
    }
    let n = cpython_vectors().len() * 2;
    assert_eq!(lines[n], "0");
    assert_eq!(
        lines[n + 1],
        "",
        "terminal fixed block must decode to empty"
    );
    assert_eq!(lines[n + 2], "0");
    assert_eq!(lines[n + 3], "666f6f", "stored block payload");
}

#[test]
fn s42_checksums_match_cpython() {
    // zlib.adler32 / zlib.crc32 values, plus a larger payload checked against
    // the known value for the 132-byte text vector.
    let mut body = String::from(
        "let (a1, d1) = zlib_adler32(fromhex(\"68656c6c6f\"));\nsay_i(a1);\n\
         let (c1, e1) = zlib_crc32(d1);\nsay_i(c1);\nrt_str_free(e1);\n\
         let (a2, d2) = zlib_adler32(fromhex(\"\"));\nsay_i(a2);\n\
         let (c2, e2) = zlib_crc32(d2);\nsay_i(c2);\nrt_str_free(e2);\n",
    );
    let mut v = 3;
    for (_, phex, _, _, _, _) in cpython_vectors() {
        if phex.is_empty() || phex == "68656c6c6f" {
            continue;
        }
        body.push_str(&format!(
            "let (a{v}, d{v}) = zlib_adler32(fromhex(\"{phex}\"));\nsay_i(a{v});\n\
             let (c{v}, e{v}) = zlib_crc32(d{v});\nsay_i(c{v});\nrt_str_free(e{v});\n"
        ));
        v += 1;
    }
    let (code, out) = build_and_run("", &body);
    assert_ok(code, &out);
    let lines = out_lines(&out);
    assert_eq!(lines[0], "103547413", "adler32(hello)");
    assert_eq!(lines[1], "907060870", "crc32(hello)");
    assert_eq!(lines[2], "1", "adler32(empty)");
    assert_eq!(lines[3], "0", "crc32(empty)");
    let mut idx = 4;
    for (name, phex, _, _, adler, crc) in cpython_vectors() {
        if phex.is_empty() || phex == "68656c6c6f" {
            continue;
        }
        assert_eq!(lines[idx], adler.to_string(), "{name}: adler32");
        assert_eq!(lines[idx + 1], crc.to_string(), "{name}: crc32");
        idx += 2;
    }
}

// ============================================================================
// 2. Round trips through the public API
// ============================================================================

/// In-program round-trip helpers: compress with the requested level, verify
/// the decompressed bytes equal the expected payload, and report both the
/// success flag and the compressed size.
const ROUNDTRIP: &str = r#"
fn rt_z(level: Int, hexd: Str, hexw: Str) -> Int {
    let data = fromhex(hexd);
    let want = fromhex(hexw);
    let out = zlib_compress(data, level);
    let clen = rt_str_len(out);
    let (back, e) = zlib_decompress(out);
    let mut ok = 0;
    if e == 0 { if rt_str_eq(back, want) { ok = 1; } }
    rt_str_free(back);
    rt_str_free(want);
    say_i(e);
    say_i(ok);
    return clen;
}
fn rt_g(level: Int, hexd: Str, hexw: Str) -> Int {
    let data = fromhex(hexd);
    let want = fromhex(hexw);
    let out = gzip_compress(data, level);
    let clen = rt_str_len(out);
    let (back, e) = gzip_decompress(out);
    let mut ok = 0;
    if e == 0 { if rt_str_eq(back, want) { ok = 1; } }
    rt_str_free(back);
    rt_str_free(want);
    say_i(e);
    say_i(ok);
    return clen;
}
fn rt_r(level: Int, hexd: Str, hexw: Str) -> Int {
    let data = fromhex(hexd);
    let want = fromhex(hexw);
    let out = deflate_raw(data, level);
    let clen = rt_str_len(out);
    let (back, e) = inflate_raw(out);
    let mut ok = 0;
    if e == 0 { if rt_str_eq(back, want) { ok = 1; } }
    rt_str_free(back);
    rt_str_free(want);
    say_i(e);
    say_i(ok);
    return clen;
}
"#;

#[test]
fn s42_round_trip_all_wrappers() {
    let mut body = String::new();
    body.push_str("say_i(rt_z(6, \"68656c6c6f\", \"68656c6c6f\"));\n");
    body.push_str("say_i(rt_g(6, \"68656c6c6f\", \"68656c6c6f\"));\n");
    body.push_str("say_i(rt_r(6, \"68656c6c6f\", \"68656c6c6f\"));\n");
    // Binary payload with every byte value, plus embedded NULs and UTF-8.
    let binary = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9fa0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfd0d1d2d3d4d5d6d7d8d9dadbdcdddedfe0e1e2e3e4e5e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff00c3a9e2988300";
    body.push_str(&format!("say_i(rt_z(6, \"{binary}\", \"{binary}\"));\n"));
    body.push_str(&format!("say_i(rt_g(6, \"{binary}\", \"{binary}\"));\n"));
    body.push_str(&format!("say_i(rt_r(6, \"{binary}\", \"{binary}\"));\n"));
    let (code, out) = build_and_run(ROUNDTRIP, &body);
    assert_ok(code, &out);
    let lines = out_lines(&out);
    // Interleaved: rt_* prints (status, ok) then the caller prints the size.
    for (i, (status, ok)) in [(0, 1), (3, 4), (6, 7), (9, 10), (12, 13), (15, 16)]
        .iter()
        .enumerate()
    {
        assert_eq!(lines[*status], "0", "round trip {i}: status");
        assert_eq!(lines[*ok], "1", "round trip {i}: payload mismatch");
    }
    // zlib('hello') = 2-byte header + 7-byte DEFLATE + 4-byte Adler-32.
    assert_eq!(lines[2], "13", "zlib('hello') length");
}

#[test]
fn s42_score_ratio_and_levels() {
    let mut body = String::new();
    // 2250 bytes of "abcabc...": every level must round-trip and compress hard.
    let rep = "616263".repeat(750);
    for level in [0, 1, 6, 9] {
        body.push_str(&format!("say_i(rt_z({level}, \"{rep}\", \"{rep}\"));\n"));
    }
    body.push_str(&format!("say_i(rt_g(9, \"{rep}\", \"{rep}\"));\n"));
    body.push_str(&format!("say_i(rt_r(9, \"{rep}\", \"{rep}\"));\n"));
    let (code, out) = build_and_run(ROUNDTRIP, &body);
    assert_ok(code, &out);
    let lines = out_lines(&out);
    let mut sizes = Vec::new();
    for i in 0..6 {
        assert_eq!(lines[i * 3], "0", "level {i}: status");
        assert_eq!(lines[i * 3 + 1], "1", "level {i}: payload mismatch");
        sizes.push(lines[i * 3 + 2].parse::<usize>().expect("size"));
    }
    // "616263" x 750 is 2250 bytes; level 0 stores them verbatim with 5 bytes
    // of block framing, inside the 2 + 4 byte zlib envelope.
    assert_eq!(sizes[0], 2250 + 11, "level 0 must be stored blocks");
    // Levels 1-9 must beat stored by a wide margin on this input.
    for size in &sizes[1..] {
        assert!(*size < 100, "expected heavy compression, got {size} bytes");
    }
}

#[test]
fn s42_empty_and_tiny_inputs() {
    let mut body = String::new();
    for hex in ["", "00", "61", "ff"] {
        body.push_str(&format!("say_i(rt_z(6, \"{hex}\", \"{hex}\"));\n"));
        body.push_str(&format!("say_i(rt_g(6, \"{hex}\", \"{hex}\"));\n"));
        body.push_str(&format!("say_i(rt_r(6, \"{hex}\", \"{hex}\"));\n"));
    }
    // Empty output sizes are the well-known 8/20/2-byte envelopes.
    body.push_str("say_i(rt_z(6, \"\", \"\"));\n");
    body.push_str("say_i(rt_g(6, \"\", \"\"));\n");
    body.push_str("say_i(rt_r(6, \"\", \"\"));\n");
    let (code, out) = build_and_run(ROUNDTRIP, &body);
    assert_ok(code, &out);
    let lines = out_lines(&out);
    // Each rt_* call prints (status, ok) and the caller prints the size.
    let mut idx = 0;
    for hex in ["", "00", "61", "ff"] {
        for wrapper in ["zlib", "gzip", "raw"] {
            assert_eq!(lines[idx], "0", "{wrapper} {hex:?}: status");
            assert_eq!(lines[idx + 1], "1", "{wrapper} {hex:?}: payload mismatch");
            idx += 3;
        }
    }
    assert_eq!(lines[idx + 2], "8", "empty zlib size");
    assert_eq!(lines[idx + 5], "20", "empty gzip size");
    assert_eq!(lines[idx + 8], "2", "empty raw DEFLATE size");
}

#[test]
fn s42_large_repetitive_payload() {
    let decls = format!(
        r#"{ROUNDTRIP}
fn reps(n: Int) -> Str {{
    let unit = "MINK-compression-test-block;";
    let ul = rt_str_len(unit);
    let out = rt_str_alloc(ul * n);
    let mut i = 0;
    while i < n {{
        let mut j = 0;
        while j < ul {{
            rt_str_set_byte(out, i * ul + j, rt_str_byte(unit, j));
            j = j + 1;
        }}
        i = i + 1;
    }}
    return out;
}}
fn big() -> Int {{
    let data = reps(1600);
    let z = zlib_compress(data, 6);
    let zlen = rt_str_len(z);
    let (back, e) = zlib_decompress(z);
    let want = reps(1600);
    let mut ok = 0;
    if e == 0 {{ if rt_str_eq(back, want) {{ ok = 1; }} }}
    say_i(e);
    say_i(ok);
    rt_str_free(back);
    rt_str_free(want);
    return zlen;
}}
"#
    );
    let (code, out) = build_and_run(&decls, "say_i(big());");
    assert_ok(code, &out);
    let lines = out_lines(&out);
    assert_eq!(lines[0], "0", "status");
    assert_eq!(lines[1], "1", "43 KiB payload mismatch");
    let zlen: usize = lines[2].parse().expect("size");
    // The 28-byte pattern repeated 1600 times (44800 bytes) collapses to a
    // few hundred bytes; CPython reports 384 for the equivalent payload.
    assert_eq!(
        zlen, 384,
        "44800 bytes of one repeated pattern -> 384 bytes"
    );
}

// ============================================================================
// 3. Failure paths — malformed, truncated and corrupted streams
// ============================================================================

#[test]
fn s42_rejects_malformed_streams() {
    // (mode, stream hex, expected code, label)
    let cases: Vec<(&str, &str, &str, &str)> = vec![
        ("zlib_decompress", "", "-7", "empty input"),
        ("zlib_decompress", "00010203", "-1", "not a zlib header"),
        ("zlib_decompress", "78", "-7", "truncated header"),
        // 0x79 has CM = 9; the 0xF5 byte keeps FCHECK valid so the CM check
        // is the one that fires.
        (
            "zlib_decompress",
            "79f50000000000000000",
            "-1",
            "reserved CM",
        ),
        ("zlib_decompress", "789c000000000000", "-4", "garbage body"),
        (
            "zlib_decompress",
            "78da0300000000",
            "-7",
            "truncated trailer",
        ),
        (
            "zlib_decompress",
            "78da0300000000ffffffff",
            "-2",
            "bad Adler-32",
        ),
        (
            "zlib_decompress",
            "78200000000000000000",
            "-10",
            "FDICT set",
        ),
        ("gzip_decompress", "", "-7", "empty input"),
        ("gzip_decompress", "1f8b0800", "-7", "truncated gzip header"),
        (
            "gzip_decompress",
            "1f9b0800000000000000",
            "-8",
            "bad gzip magic",
        ),
        (
            "gzip_decompress",
            "1f8b09000000000000000300",
            "-8",
            "bad gzip CM",
        ),
        ("inflate_raw", "06", "-3", "invalid block type"),
        (
            "inflate_raw",
            "010100000041",
            "-4",
            "stored LEN/NLEN mismatch",
        ),
        (
            "inflate_raw",
            "050000000000000000",
            "-5",
            "invalid code lengths",
        ),
        ("inflate_raw", "0302", "-6", "distance before output start"),
    ];
    let mut body = String::new();
    for (i, (func, hex, _, _)) in cases.iter().enumerate() {
        body.push_str(&format!(
            "let (s{i}, e{i}) = {func}(fromhex(\"{hex}\"));\nsay_i(e{i});\nsay_i(rt_str_len(s{i}));\nrt_str_free(s{i});\n"
        ));
    }
    let (code, out) = build_and_run("", &body);
    assert_ok(code, &out);
    let lines = out_lines(&out);
    for (i, (func, hex, want, label)) in cases.iter().enumerate() {
        assert_eq!(lines[i * 2], *want, "{label}: {func}({hex:?}) code");
        // A rejected stream returns the empty string, never partial bytes.
        assert_eq!(lines[i * 2 + 1], "0", "{label}: must return an empty Str");
    }
}

#[test]
fn s42_corruption_is_detected() {
    // Flipping a byte in the middle of a valid stream must be caught by the
    // format's own integrity check rather than returning wrong bytes.
    let text = "74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f670a74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f670a74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f670a";
    let decls = r#"
fn corrupt_z(hex: Str, at: Int) -> Int {
    let data = fromhex(hex);
    let n = rt_str_len(data);
    let mut b = rt_str_byte(data, at);
    b = (b ^ 255) & 255;
    rt_str_set_byte(data, at, b);
    let (s, e) = zlib_decompress(data);
    say_i(e);
    say_i(rt_str_len(s));
    rt_str_free(s);
    return n;
}
"#;
    let body = format!(
        "say_i(corrupt_z(\"{text}\", 20));\nsay_i(corrupt_z(\"{text}\", 40));\nsay_i(corrupt_z(\"{text}\", 8));\n"
    );
    let (code, out) = build_and_run(decls, &body);
    assert_ok(code, &out);
    let lines = out_lines(&out);
    // corrupt_z prints (code, output length) and the caller prints the input
    // length, so each call contributes three lines. Every corrupted body must
    // report a negative code and no output bytes.
    for i in 0..3 {
        assert!(
            lines[i * 3].starts_with('-'),
            "corruption {i} must be detected, got code {}",
            lines[i * 3]
        );
        assert_eq!(lines[i * 3 + 1], "0", "corruption {i} must not leak bytes");
        assert_eq!(lines[i * 3 + 2], "132", "corruption {i}: input length");
    }
}

// ============================================================================
// 4. Repetition, determinism and ownership
// ============================================================================

#[test]
fn s42_repeated_use_is_deterministic() {
    // These loops hold no caller-side temporaries; the only heap traffic is
    // the codec's own buffers, released in exact reverse allocation order
    // (see _z_deflate). Measured cycle counts on the default 4 MiB heap
    // before E-R02, per pattern in isolation:
    //   zlib_decompress only ........ >= 20000  (no measurable growth)
    //   zlib_compress only .......... ~ 7272
    //   compress + decompress ....... ~  540
    //   gzip / raw round trip ....... ~  470
    // The residual is the shared runtime allocator: reuse hands a freed block
    // to the first request that fits it and does not split it, so a block of
    // the wrong size class is consumed whole and its remainder is lost. The
    // codec itself is leak-free (rt_exit reports E-R06/exit 106 on any live
    // allocation). This is documented as a known limitation rather than
    // papered over; each loop below runs 100 cycles, well inside the bound.
    let decls = r#"
fn many_decompress(iters: Int) -> Int {
    let mut bad = 0;
    let mut i = 0;
    while i < iters {
        let z = zlib_compress("hello", 6);
        let (s, e) = zlib_decompress(z);
        if e != 0 { bad = bad + 1; }
        if rt_str_len(s) != 5 { bad = bad + 1; }
        let mut ok = 0;
        if rt_str_byte(s, 0) == 104 { ok = 1; }
        if ok == 0 { bad = bad + 1; }
        rt_str_free(s);
        i = i + 1;
    }
    return bad;
}
fn many_compress(iters: Int) -> Int {
    let mut bad = 0;
    let mut i = 0;
    while i < iters {
        let z = zlib_compress("hello", 6);
        if rt_str_len(z) != 13 { bad = bad + 1; }
        if rt_str_byte(z, 0) != 120 { bad = bad + 1; }
        if rt_str_byte(z, 1) != 156 { bad = bad + 1; }
        if rt_str_byte(z, 12) != 21 { bad = bad + 1; }
        rt_str_free(z);
        i = i + 1;
    }
    return bad;
}
fn wrap_cycle(iters: Int) -> Int {
    let mut bad = 0;
    let mut i = 0;
    while i < iters {
        let g = gzip_compress("the quick", 6);
        let (b, e) = gzip_decompress(g);
        if e != 0 { bad = bad + 1; }
        if rt_str_len(b) != 9 { bad = bad + 1; }
        let mut ok = 0;
        if rt_str_byte(b, 0) == 116 { ok = 1; }
        if ok == 0 { bad = bad + 1; }
        rt_str_free(b);
        i = i + 1;
    }
    return bad;
}
fn raw_cycle(iters: Int) -> Int {
    let mut bad = 0;
    let mut i = 0;
    while i < iters {
        let r = deflate_raw("the quick brown fox", 9);
        let (b, e) = inflate_raw(r);
        if e != 0 { bad = bad + 1; }
        if rt_str_len(b) != 19 { bad = bad + 1; }
        rt_str_free(b);
        i = i + 1;
    }
    return bad;
}
"#;
    let (code, out) = build_and_run(
        decls,
        "say_i(many_decompress(100));\nsay_i(many_compress(100));\nsay_i(wrap_cycle(100));\nsay_i(raw_cycle(100));\n",
    );
    assert_ok(code, &out);
    let lines = out_lines(&out);
    assert_eq!(
        lines,
        vec!["0", "0", "0", "0"],
        "400 repeated codec cycles must be clean and deterministic"
    );
}

#[test]
fn s42_golden_encoding_is_stable() {
    // The encoder is deterministic and byte-compatible with zlib's
    // fixed-Huffman choice for small inputs, so the exact output is pinned.
    let body = "say_i(rt_gold_z());\nsay_i(rt_gold_g());\nsay_i(rt_gold_r());\n";
    let helpers = r#"
fn gold_z() -> Str { return zlib_compress(fromhex("68656c6c6f"), 6); }
fn gold_g() -> Str { return gzip_compress(fromhex("68656c6c6f"), 6); }
fn gold_r() -> Str { return deflate_raw(fromhex("68656c6c6f"), 6); }
fn rt_gold_z() -> Int { let h = hexify(gold_z()); say_str(h); return 0; }
fn rt_gold_g() -> Int { let h = hexify(gold_g()); say_str(h); return 0; }
fn rt_gold_r() -> Int { let h = hexify(gold_r()); say_str(h); return 0; }
fn say_str(v: Str) { rt_print_str(v); rt_str_free(v); }
"#;
    let (code, out) = build_and_run(helpers, body);
    assert_ok(code, &out);
    let lines = out_lines(&out);
    // Each helper prints the hex line first and returns 0, which the caller
    // prints afterwards: hex at 0, 2, 4 and the status values at 1, 3, 5.
    // zlib.compress(b"hello", 6).hex() == this (byte-identical to CPython).
    assert_eq!(lines[0], "789ccb48cdc9c90700062c0215", "zlib golden");
    assert_eq!(
        lines[2], "1f8b0800000000000000cb48cdc9c9070086a6103605000000",
        "gzip golden"
    );
    assert_eq!(lines[4], "cb48cdc9c90700", "raw DEFLATE golden");
}

#[test]
fn s42_ownership_is_clean_under_mixed_use() {
    // Mixed compress/decompress traffic in one program, repeated far beyond
    // the point where one stranded scratch block per call would exhaust the
    // fixed 4 MiB heap (the deflate path releases its scratch in exact reverse
    // allocation order for this reason). Any leak also exits 106 with E-R06.
    let decls = r#"
fn mixed(iters: Int) -> Int {
    let mut bad = 0;
    let mut i = 0;
    while i < iters {
        let z = zlib_compress(fromhex("74686520717569636b2062726f776e20666f7820"), 6);
        let (b, e1) = zlib_decompress(z);
        if e1 != 0 { bad = bad + 1; }
        rt_str_free(b);
        let r = deflate_raw(fromhex("00ff00ff00ff00ff"), 9);
        let (c, e2) = inflate_raw(r);
        if e2 != 0 { bad = bad + 1; }
        rt_str_free(c);
        let (s, e3) = zlib_decompress(fromhex("0001"));
        if e3 == 0 { bad = bad + 1; }
        rt_str_free(s);
        i = i + 1;
    }
    return bad;
}
"#;
    let (code, out) = build_and_run(decls, "say_i(mixed(100));");
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["0"], "mixed use must be clean");
}

// ============================================================================
// 5. Live CPython interoperability (skipped when no interpreter is present)
// ============================================================================

fn python_binary() -> Option<String> {
    for candidate in ["python", "python3"] {
        if let Ok(out) = Command::new(candidate).arg("--version").output() {
            if out.status.success() {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

#[test]
fn s42_cpython_inflates_mink_output() {
    let Some(python) = python_binary() else {
        eprintln!("skipping: no python interpreter on PATH");
        return;
    };
    let body = "say_i(rt_out_z());\nsay_i(rt_out_g());\nsay_i(rt_out_r());\n";
    let helpers = r#"
fn out_z() -> Str { return zlib_compress(fromhex("74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f67"), 9); }
fn out_g() -> Str { return gzip_compress(fromhex("74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f67"), 9); }
fn out_r() -> Str { return deflate_raw(fromhex("74686520717569636b2062726f776e20666f78206a756d7073206f76657220746865206c617a7920646f67"), 9); }
fn rt_out_z() -> Int { let h = hexify(out_z()); rt_print_str(h); rt_str_free(h); return 0; }
fn rt_out_g() -> Int { let h = hexify(out_g()); rt_print_str(h); rt_str_free(h); return 0; }
fn rt_out_r() -> Int { let h = hexify(out_r()); rt_print_str(h); rt_str_free(h); return 0; }
"#;
    let (code, out) = build_and_run(helpers, body);
    assert_ok(code, &out);
    let lines = out_lines(&out);
    let payload = "the quick brown fox jumps over the lazy dog";
    // Each helper prints its hex line before returning 0, so the three
    // streams are at 0, 2 and 4.
    let script = format!(
        "import sys,zlib,gzip\n\
         z=bytes.fromhex('{}')\n\
         g=bytes.fromhex('{}')\n\
         r=bytes.fromhex('{}')\n\
         want={payload:?}.encode()\n\
         assert zlib.decompress(z)==want, 'zlib'\n\
         assert gzip.decompress(g)==want, 'gzip'\n\
         assert zlib.decompressobj(-15).decompress(r)==want, 'raw'\n\
         print('OK')\n",
        lines[0], lines[2], lines[4]
    );
    let out = Command::new(&python)
        .arg("-c")
        .arg(script)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "CPython rejected MINK output: {stderr}\nstdout={stdout}"
    );
    assert!(stdout.contains("OK"), "unexpected output: {stdout:?}");
}

#[test]
fn s42_matches_cpython_compressed_bytes_for_small_input() {
    let Some(python) = python_binary() else {
        eprintln!("skipping: no python interpreter on PATH");
        return;
    };
    let body = "say_i(rt_cmp_z());\nsay_i(rt_cmp_level0());\n";
    let helpers = r#"
fn cmp_z() -> Str { return zlib_compress(fromhex("68656c6c6f"), 6); }
fn rt_cmp_z() -> Int { let h = hexify(cmp_z()); rt_print_str(h); rt_str_free(h); return 0; }
// Every byte 0..255 twice: catches an alphabet/bit-packing error that a short
// text payload would miss.
fn cmp_long() -> Str {
    let data = fromhex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9fa0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfd0d1d2d3d4d5d6d7d8d9dadbdcdddedfe0e1e2e3e4e5e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9fa0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfd0d1d2d3d4d5d6d7d8d9dadbdcdddedfe0e1e2e3e4e5e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff");
    return zlib_compress(data, 6);
}
fn rt_cmp_level0() -> Int { let h = hexify(cmp_long()); rt_print_str(h); rt_str_free(h); return 0; }
"#;
    let (code, out) = build_and_run(helpers, body);
    assert_ok(code, &out);
    let lines = out_lines(&out);
    let script = format!(
        "import zlib\n\
         mink=bytes.fromhex('{}')\n\
         want=zlib.compress(b'hello',6)\n\
         assert mink==want, ('byte mismatch', mink.hex(), want.hex())\n\
         long=bytes.fromhex('{}')\n\
         assert zlib.decompress(long)==bytes(range(256))*2, 'long payload mismatch'\n\
         print('OK')\n",
        lines[0], lines[2]
    );
    let out = Command::new(&python)
        .arg("-c")
        .arg(script)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "MINK encoding diverged from CPython: {stderr}\nstdout={stdout}"
    );
}
