//! Permanent ownership regressions for the standard library (Session 125).
//!
//! A user function that takes a `Str` parameter **consumes** it: the caller's
//! binding is dead after the call, so the callee owns the only reference and
//! must release it. These tests pin that contract at the module level by
//! passing **heap-allocated** `Str` values in (`rt_str_concat` allocates; a
//! string literal is immortal and would hide the defect) and then requiring the
//! process to exit **0**.
//!
//! Exit 106 is `E-R06`, the exit-time leak check. Suites that assert
//! "0 or 106" tolerate the leak; these assert 0 exactly, so a regressed
//! release in any module below fails the build rather than quietly passing.
//!
//! Each case is a full program whose only import is the stdlib module under
//! test, so the module loader resolves it from the bundled `stdlib/`.

use std::path::PathBuf;
use std::process::Command;

/// Build and run a stdlib program; returns (exit code, stdout).
fn run_program(name: &str, body: &str) -> (i32, String) {
    let path: PathBuf = std::env::temp_dir().join(format!("mink_own_{}_{name}.mink", std::process::id()));
    std::fs::write(&path, body).expect("write temp source");
    let build = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("build")
        .arg(&path)
        .output()
        .expect("run mink build");
    let exe = path.with_extension("exe");
    if !build.status.success() {
        let err = String::from_utf8_lossy(&build.stderr);
        let _ = std::fs::remove_file(&path);
        panic!("build failed for {name}:\n{err}");
    }
    assert!(exe.exists(), "no executable produced for {name}");
    let run = Command::new(&exe).output().expect("run test program");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    let code = run.status.code().unwrap_or(-1);
    (
        code,
        String::from_utf8_lossy(&run.stdout).trim().to_string(),
    )
}

/// Assert the program exited 0 — no leak (106), no fault, no assertion failure.
#[track_caller]
fn assert_clean(name: &str, body: &str) {
    let (code, out) = run_program(name, body);
    assert_eq!(code, 0, "{name}: expected a clean exit, got {code} (stdout: {out})");
}

#[test]
fn encoding_releases_heap_inputs() {
    // Every `Str`-taking entry point of the encoding layer, each given a heap
    // string. The decoded tuples and the Vec must be released by the caller;
    // the inputs by the callee.
    assert_clean(
        "encoding",
        r#"
mod encoding;

fn main() -> Int {
    let h = hex_encode(rt_str_concat("ab", "c"));
    let hu = hex_encode_upper(rt_str_concat("ab", "c"));
    if rt_str_len(h) != 6 { rt_exit(2); }
    rt_str_free(h);
    rt_str_free(hu);
    let d = hex_decode_alloc(rt_str_concat("6162", "63"));
    rt_str_free(d.0);
    let b = base64_encode(rt_str_concat("ab", "c"));
    rt_str_free(b);
    let bu = base64_url_encode(rt_str_concat("ab", "c"));
    rt_str_free(bu);
    let bd = base64_decode_alloc(rt_str_concat("YWJj", ""));
    rt_str_free(bd.0);
    let bud = base64_url_decode_alloc(rt_str_concat("YWJj", ""));
    rt_str_free(bud.0);
    let ok = utf8_validate(rt_str_concat("ab", "c"));
    if ok { } else { rt_exit(3); }
    let n = utf8_char_count(rt_str_concat("ab", "c"));
    if n != 3 { rt_exit(4); }
    let codes = utf8_decode(rt_str_concat("ab", "c"));
    let e = utf8_encode(codes);
    rt_str_free(e);
    let ca = utf8_char_at(rt_str_concat("ab", "c"), 0);
    let bi = utf8_byte_index(rt_str_concat("ab", "c"), 1);
    if ca != 97 || bi != 1 { rt_exit(5); }
    let sl = utf8_slice(rt_str_concat("ab", "c"), 0, 1);
    rt_str_free(sl);
    rt_exit(0);
}
"#,
    );
}

#[test]
fn strings_releases_heap_inputs() {
    // The whole `Str`-taking surface of `strings`, including the entry points
    // that return an extra owned `Str` alongside their result (the caller
    // frees that one; the inputs belong to the callee).
    assert_clean(
        "strings",
        r#"
mod strings;

fn main() -> Int {
    let c = str_cmp(rt_str_concat("a", ""), rt_str_concat("b", ""));
    if c >= 0 { rt_exit(2); }
    let a1 = str_index_of(rt_str_concat("he", "llo"), rt_str_concat("l", "l"));
    rt_str_free(a1.1);
    let a2 = str_last_index_of(rt_str_concat("he", "llo"), rt_str_concat("l", "l"));
    rt_str_free(a2.1);
    let a3 = str_contains(rt_str_concat("he", "llo"), rt_str_concat("el", ""));
    rt_str_free(a3.1);
    let a4 = str_starts_with(rt_str_concat("he", "llo"), rt_str_concat("he", ""));
    rt_str_free(a4.1);
    let a5 = str_ends_with(rt_str_concat("he", "llo"), rt_str_concat("lo", ""));
    rt_str_free(a5.1);
    let a6 = str_count(rt_str_concat("aaa", ""), rt_str_concat("a", ""));
    rt_str_free(a6.1);
    let a7 = str_is_numeric(rt_str_concat("1", "2"));
    rt_str_free(a7.1);
    let a8 = str_is_alpha(rt_str_concat("ab", ""));
    rt_str_free(a8.1);
    let a9 = str_is_alphanumeric(rt_str_concat("a1", ""));
    rt_str_free(a9.1);
    let s = str_sub(rt_str_concat("abc", "def"), 1, 3);
    rt_str_free(s);
    let t = str_trim(rt_str_concat("  x", " "));
    rt_str_free(t);
    let ts = str_trim_start(rt_str_concat("  x", " "));
    rt_str_free(ts);
    let te = str_trim_end(rt_str_concat("  x", " "));
    rt_str_free(te);
    let u = str_to_upper(rt_str_concat("ab", "c"));
    rt_str_free(u);
    let l = str_to_lower(rt_str_concat("AB", "C"));
    rt_str_free(l);
    let rp = str_repeat(rt_str_concat("a", ""), 3);
    rt_str_free(rp);
    let pl = str_pad_left(rt_str_concat("a", ""), 4, 32);
    rt_str_free(pl);
    let pr = str_pad_right(rt_str_concat("a", ""), 4, 32);
    rt_str_free(pr);
    let rv = str_reverse(rt_str_concat("ab", ""));
    rt_str_free(rv);
    let r1 = str_replace(rt_str_concat("ab", "c"), rt_str_concat("b", ""), rt_str_concat("X", ""));
    rt_str_free(r1);
    let r2 = str_replace_all(rt_str_concat("ab", "c"), rt_str_concat("b", ""), rt_str_concat("X", ""));
    rt_str_free(r2);
    let j2 = str_join_2(rt_str_concat("a", ""), rt_str_concat("b", ""));
    rt_str_free(j2);
    let j3 = str_join_3(rt_str_concat("a", ""), rt_str_concat("b", ""), rt_str_concat("c", ""));
    rt_str_free(j3);
    let parts = str_split(rt_str_concat("a,b", ""), rt_str_concat(",", ""));
    let jv = str_join(parts, rt_str_concat("-", ""));
    rt_str_free(jv);
    let ca = str_char_at(rt_str_concat("ab", ""), 0);
    rt_str_free(ca.1);
    let pi = str_parse_int(rt_str_concat("1", "2"));
    if pi.0 != 12 { rt_exit(11); }
    let pf = str_parse_float(rt_str_concat("1.", "5"));
    if pf.1 == false { rt_exit(12); }
    rt_exit(0);
}
"#,
    );
}

#[test]
fn http_releases_heap_inputs() {
    // The response/request parsers and URL helpers, all given heap strings.
    // None of these touches the network: they are pure text handling.
    assert_clean(
        "http",
        r#"
mod http;

fn resp() -> Str {
    return rt_str_concat("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n", "hi");
}

fn main() -> Int {
    if http_response_ok(resp()) == false { rt_exit(2); }
    let p = http_parse_int_from_bytes(rt_str_concat("HTTP/1.1 2", "00 OK"), 9, 12);
    if p != 200 { rt_exit(3); }
    let sub = http_sub_bytes(rt_str_concat("HTTP/1.1 ", "200 OK"), 9, 12);
    rt_str_free(sub);
    let ch = rt_str_from_char(65);
    rt_str_free(ch);
    let host = http_url_host(rt_str_concat("http://local", "host:8080/x"));
    if rt_str_len(host) != 9 { rt_exit(4); }
    rt_str_free(host);
    let port = http_url_port(rt_str_concat("http://host", ":8080/x"));
    if port != 8080 { rt_exit(5); }
    let path = http_url_path(rt_str_concat("http://host", "/index.html"));
    rt_str_free(path);
    let req = http_request(rt_str_concat("GE", "T"), rt_str_concat("/in", "dex"));
    rt_str_free(req);
    let get = http_get(rt_str_concat("/in", "dex"));
    rt_str_free(get);
    let added = http_add_header(rt_str_concat("GET / HTTP/1.1\r\n\r\n", ""), rt_str_concat("Ho", "st"), rt_str_concat("h", "v"));
    let fin = http_finalize_request(added);
    rt_str_free(fin);
    let h = http_header(resp(), rt_str_concat("Content-Le", "ngth"));
    if rt_str_len(h) != 1 { rt_exit(6); }
    rt_str_free(h);
    let cl = http_content_length(resp());
    if cl != 2 { rt_exit(7); }
    let code = http_status_code(resp());
    if code != 200 { rt_exit(8); }
    let text = http_status_text(resp());
    if rt_str_len(text) != 2 { rt_exit(9); }
    rt_str_free(text);
    let body = http_body(resp());
    if rt_str_len(body) != 2 { rt_exit(10); }
    rt_str_free(body);
    rt_exit(0);
}
"#,
    );
}

#[test]
fn assert_releases_heap_arguments() {
    // The string assertions consume both arguments; on the success path they
    // must release them or the process leaks (E-R06).
    assert_clean(
        "assert",
        r#"
mod assert;

fn main() -> Int {
    assert_eq_str(rt_str_concat("ab", "c"), rt_str_concat("ab", "c"));
    assert_ne_str(rt_str_concat("ab", "c"), rt_str_concat("ab", "d"));
    rt_exit(0);
}
"#,
    );
}

#[test]
fn hashing_releases_heap_inputs() {
    // The hashing entry points move their input and release it; a heap input
    // proves it (a literal would be immortal and hide the leak).
    assert_clean(
        "hashing",
        r#"
mod hashing;

fn main() -> Int {
    let a = hash_fnv1a(rt_str_concat("ab", "c"));
    let b = hash_djb2(rt_str_concat("ab", "c"));
    if a == 0 && b == 0 { rt_exit(2); }
    if hash_digest_equal(rt_str_concat("ab", "c"), rt_str_concat("ab", "c")) == false { rt_exit(3); }
    if hash_digest_equal(rt_str_concat("ab", "c"), rt_str_concat("ab", "d")) { rt_exit(4); }
    let d1 = hash_sha256(rt_str_concat("ab", "c"));
    let d2 = hash_sha256(rt_str_concat("ab", "c"));
    if rt_str_len(d1) != 64 { rt_exit(5); }
    if hash_digest_equal(d1, d2) == false { rt_exit(6); }
    rt_exit(0);
}
"#,
    );
}

#[test]
fn crypto_releases_heap_inputs() {
    // HMAC/HKDF take several owned strings and return owned ones. `key` is
    // longer than the 64-byte HMAC block here, so the internal
    // `hash_sha256(key)` path runs with a copy and the original is released
    // exactly once.
    assert_clean(
        "crypto",
        r#"
mod crypto;

fn long_key() -> Str {
    let k = rt_str_concat("0123456789abcdef0123456789abcdef", "0123456789abcdef0123456789abcdef");
    return k;
}

fn main() -> Int {
    if crypto_verify(rt_str_concat("ab", "c"), rt_str_concat("ab", "c")) == false { rt_exit(2); }
    let mac = hmac_sha256(long_key(), rt_str_concat("me", "ssage"));
    if rt_str_len(mac) != 64 { rt_exit(3); }
    rt_str_free(mac);
    let prk = hkdf_extract(rt_str_concat("sa", "lt"), rt_str_concat("ik", "m"));
    let okm = hkdf_expand(prk, rt_str_concat("in", "fo"), 32);
    if rt_str_len(okm) != 32 { rt_exit(4); }
    rt_str_free(okm);
    let full = hkdf_sha256(rt_str_concat("ik", "m"), rt_str_concat("sa", "lt"), rt_str_concat("in", "fo"), 32);
    if rt_str_len(full) != 32 { rt_exit(5); }
    rt_str_free(full);
    let mac2 = hmac_sha256(rt_str_concat("k", "ey"), rt_str_concat("me", "ssage"));
    rt_str_free(mac2);
    let z = crypto_secure_zero_str(rt_str_concat("ab", "c"));
    rt_str_free(z);
    // The random sources need their one-time library load, exactly like the
    // network layer needs net_init() before any socket call.
    if crypto_init() != 0 { rt_exit(6); }
    let r = crypto_random_bytes(8);
    if rt_str_len(r) != 8 { rt_exit(7); }
    rt_str_free(r);
    let rh = crypto_random_hex(8);
    if rt_str_len(rh) != 16 { rt_exit(8); }
    rt_str_free(rh);
    rt_exit(0);
}
"#,
    );
}

#[test]
fn process_releases_heap_command() {
    // `process_is_valid_cmd` consumes its command string. The `run` variants
    // launch a real child process, which the ownership contract covers
    // separately; here only the pure predicate is exercised so the test stays
    // deterministic.
    assert_clean(
        "process",
        r#"
mod process;

fn main() -> Int {
    if process_is_valid_cmd(rt_str_concat("cm", "d")) == false { rt_exit(2); }
    if process_is_valid_cmd(rt_str_concat("", "")) { rt_exit(3); }
    rt_exit(0);
}
"#,
    );
}

#[test]
fn process_run_variants_release_heap_inputs() {
    // The `run` variants consume their `Str` parameters: a heap command and a
    // heap input must both be released by the callee. A missing release fails
    // the exit-time leak check with E-R06 (106). The child processes here are
    // tiny and deterministic (`echo`, `findstr`), so the test stays fast.
    assert_clean(
        "process_run_input",
        r#"
mod process;

fn main() -> Int {
    if process_run(rt_str_concat("echo ", "owned")) != 0 { rt_exit(2); }
    let cmd = rt_str_concat("findstr ", "owned");
    let input = rt_str_concat("owned ", "line\n");
    if process_run_input(cmd, input) != 0 { rt_exit(3); }
    rt_exit(0);
}
"#,
    );
}

#[test]
fn zlib_releases_heap_inputs() {
    // Compression consumes the input and returns an owned Str; decompression
    // consumes the compressed buffer and returns the payload plus a status.
    assert_clean(
        "zlib",
        r#"
mod zlib;

fn main() -> Int {
    let packed = zlib_compress(rt_str_concat("hello ", "zlib world"), 6);
    if rt_str_len(packed) == 0 { rt_exit(2); }
    let back = zlib_decompress(packed);
    if back.1 != 0 { rt_exit(3); }
    if rt_str_len(back.0) != 16 { rt_exit(4); }
    rt_str_free(back.0);
    let raw = deflate_raw(rt_str_concat("hello ", "zlib world"), 6);
    let rawback = inflate_raw(raw);
    rt_str_free(rawback.0);
    let gz = gzip_compress(rt_str_concat("hello ", "zlib world"), 6);
    let gzback = gzip_decompress(gz);
    rt_str_free(gzback.0);
    let a = zlib_adler32(rt_str_concat("ab", "c"));
    rt_str_free(a.1);
    let c = zlib_crc32(rt_str_concat("ab", "c"));
    rt_str_free(c.1);
    rt_exit(0);
}
"#,
    );
}

#[test]
fn tls_releases_heap_arguments() {
    // `tls_connect` / `https_client_get` consume the host name, the CA path
    // and the URL. Both calls below fail offline — an unresolvable name and a
    // malformed URL — so no network is touched, but both still have to
    // release everything they consumed.
    assert_clean(
        "tls",
        r#"
mod tls;

fn main() -> Int {
    let err = rt_alloc(8);
    rt_mem_store(err, 0);
    let h = tls_connect(rt_str_concat("mink-no-such", "-host.invalid"), 9, rt_str_concat("no-such", ".pem"), err);
    rt_free(err);
    let body = https_client_get(rt_str_concat("not-a-url", ""), rt_str_concat("no-such", ".pem"));
    if rt_str_len(body) != 0 { rt_exit(2); }
    rt_str_free(body);
    rt_exit(0);
}
"#,
    );
}

#[test]
fn conditional_exit_control_flow_is_preserved() {
    // A guard whose condition is **true** takes the branch, so `rt_exit`
    // runs while the heap string is still live and the exit-time leak scan
    // correctly reports `E-R06` (106). The statements *after* the `if` are
    // unreachable in that case and must not run.
    //
    // A guard whose condition is **false** falls through, so the
    // post-guard release runs and the process exits 0. Together the two
    // halves pin the branch: taken exits 106 without printing the
    // continuation, untaken prints it and exits cleanly.
    let (taken, taken_out) = run_program(
        "cond_taken",
        r#"
fn main() -> Int {
    let a = rt_str_concat("ab", "c");
    if rt_str_len(a) != 99 { rt_print_str("TAKEN"); rt_exit(5); }
    rt_print_str("CONT");
    rt_str_free(a);
    rt_exit(0);
}
"#,
    );
    assert_eq!(taken, 106, "a taken guard must exit while `a` is still live");
    assert_eq!(taken_out, "TAKEN", "the taken branch must run and stop there");

    let (untaken, untaken_out) = run_program(
        "cond_untaken",
        r#"
fn main() -> Int {
    let a = rt_str_concat("ab", "c");
    if rt_str_len(a) == 99 { rt_print_str("TAKEN"); rt_exit(5); }
    rt_print_str("CONT");
    rt_str_free(a);
    rt_exit(0);
}
"#,
    );
    assert_eq!(untaken, 0, "an untaken guard must reach the release and exit 0");
    assert_eq!(untaken_out, "CONT", "the continuation after the guard must run");
}