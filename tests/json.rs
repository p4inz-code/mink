//! Integration tests for the MINK JSON library (Session 52).

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn json_lib() -> String {
    std::fs::read_to_string("stdlib/json.mink").expect("failed to read stdlib/json.mink")
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_json_test_{n}_{name}.mink"));
    std::fs::write(&path, content.replace("\r\n", "\n")).unwrap();
    path
}

fn build_and_run(test_body: &str) -> (i32, Vec<u8>) {
    let lib = json_lib();
    let source = format!("{}\n{}", lib, test_body);
    let path = temp_source("test", &source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        panic!("build failed:\n{stderr}");
    }
    let run = Command::new(&exe).output().unwrap();
    let code = run.status.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (code, run.stdout)
}

/// Escape a string for use as a MINK string literal
fn mink_str(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Assert that raw JSON text parses successfully
fn assert_parse_ok(json: &str) {
    let mink_literal = mink_str(json);
    let test = format!(
        r#"
fn main() -> Int {{
    let arena = rt_alloc(65536);
    let v = json_parse({mink_literal}, arena);
    if v == 0 {{ rt_free(arena); rt_exit(1); }}
    rt_free(arena);
    rt_exit(0);
}}"#
    );
    let (code, _) = build_and_run(&test);
    assert_eq!(
        code, 0,
        "expected parse of {json:?} to succeed (exit code: {code})"
    );
}

/// Assert that raw JSON text fails to parse
fn assert_parse_err(json: &str) {
    let mink_literal = mink_str(json);
    let test = format!(
        r#"
fn main() -> Int {{
    let arena = rt_alloc(65536);
    let v = json_parse({mink_literal}, arena);
    if v == 0 {{ rt_free(arena); rt_exit(0); }}
    rt_free(arena);
    rt_exit(1);
}}"#
    );
    let (code, _) = build_and_run(&test);
    assert_eq!(code, 0, "expected parse of {json:?} to fail");
}

// =========================================================================
// Primitives
// =========================================================================

#[test]
fn parse_null() {
    assert_parse_ok("null");
}

#[test]
fn parse_true() {
    assert_parse_ok("true");
}

#[test]
fn parse_false() {
    assert_parse_ok("false");
}

#[test]
fn parse_int_zero() {
    assert_parse_ok("0");
}

#[test]
fn parse_int_positive() {
    assert_parse_ok("42");
}

#[test]
fn parse_int_negative() {
    assert_parse_ok("-7");
}

#[test]
fn parse_int_large() {
    assert_parse_ok("1234567890");
}

#[test]
fn parse_empty_string() {
    assert_parse_ok("\"\"");
}

#[test]
fn parse_hello_string() {
    assert_parse_ok("\"hello\"");
}

#[test]
fn parse_empty_array() {
    assert_parse_ok("[]");
}

#[test]
fn parse_empty_object() {
    assert_parse_ok("{}");
}

// =========================================================================
// Invalid inputs
// =========================================================================

#[test]
fn reject_empty_input() {
    assert_parse_err("");
}

#[test]
fn reject_trailing_garbage() {
    assert_parse_err("true false");
}

#[test]
fn reject_trailing_char() {
    assert_parse_err("trueX");
}

#[test]
fn reject_unclosed_string() {
    assert_parse_err("\"hello");
}

#[test]
fn reject_unclosed_array() {
    assert_parse_err("[1, 2");
}

#[test]
fn reject_unclosed_object() {
    assert_parse_err("{\"a\": 1");
}

#[test]
fn reject_missing_colon() {
    assert_parse_err("{\"a\" 1}");
}

#[test]
fn reject_missing_value() {
    assert_parse_err("{\"a\":}");
}

#[test]
fn reject_trailing_comma_array() {
    assert_parse_err("[1,]");
}

#[test]
fn reject_trailing_comma_object() {
    assert_parse_err("{\"a\":1,}");
}

// =========================================================================
// Whitespace
// =========================================================================

#[test]
fn whitespace_before_value() {
    assert_parse_ok("  true");
}

#[test]
fn whitespace_after_value() {
    assert_parse_ok("true  ");
}

#[test]
fn whitespace_around_value() {
    assert_parse_ok("  \n  true  \n  ");
}

#[test]
fn whitespace_in_array() {
    assert_parse_ok("[ 1 , 2 , 3 ]");
}

#[test]
fn whitespace_in_object() {
    assert_parse_ok("{ \"a\" : 1 }");
}

// =========================================================================
// Strings with escapes
// =========================================================================

#[test]
fn string_with_newline_escape() {
    assert_parse_ok("\"a\\nb\"");
}

#[test]
fn string_with_tab_escape() {
    assert_parse_ok("\"a\\tb\"");
}

#[test]
fn string_with_backslash_escape() {
    assert_parse_ok("\"a\\\\b\"");
}

#[test]
fn string_with_quote_escape() {
    assert_parse_ok("\"a\\\"b\"");
}

#[test]
fn string_with_slash_escape() {
    assert_parse_ok("\"a\\/b\"");
}

// =========================================================================
// Adversarial
// =========================================================================

#[test]
fn single_brace() {
    assert_parse_err("{");
}

#[test]
fn single_bracket() {
    assert_parse_err("[");
}

#[test]
fn just_number_sign() {
    assert_parse_err("-");
}

#[test]
fn double_open_bracket() {
    assert_parse_err("[[1]");
}

#[test]
fn invalid_literal_nulll() {
    assert_parse_err("nulll");
}

#[test]
fn invalid_literal_truee() {
    assert_parse_err("truee");
}

// =========================================================================
// Serialization (parse → serialize → verify output, exit 0, no leak)
// =========================================================================

/// Assert that JSON parses, serializes back, produces expected output, exit 0
fn assert_ser(json: &str, expected: &str) {
    let mink_literal = mink_str(json);
    let expected_literal = mink_str(expected);
    let test = format!(
        r#"
fn main() -> Int {{
    let arena = rt_alloc(65536);
    let v = json_parse({mink_literal}, arena);
    if v == 0 {{ rt_free(arena); rt_exit(1); }}
    let s = json_to_str(arena, v);
    let exp = {expected_literal};
    let ok = rt_str_eq(s, exp);
    rt_str_free(s);
    rt_free(arena);
    if ok {{ rt_exit(0); }} else {{ rt_exit(2); }}
}}"#
    );
    let (code, _) = build_and_run(&test);
    assert_eq!(
        code, 0,
        "serialize({json:?}) failed (exit code: {code}), expected {expected:?}"
    );
}

#[test]
fn serialize_null() {
    assert_ser("null", "null");
}

#[test]
fn serialize_true() {
    assert_ser("true", "true");
}

#[test]
fn serialize_false() {
    assert_ser("false", "false");
}

#[test]
fn serialize_integer() {
    assert_ser("42", "42");
}

#[test]
fn serialize_negative_integer() {
    assert_ser("-7", "-7");
}

#[test]
fn serialize_empty_string() {
    assert_ser("\"\"", "\"\"");
}

#[test]
fn serialize_string() {
    assert_ser("\"hello\"", "\"hello\"");
}

#[test]
fn serialize_empty_array() {
    assert_ser("[]", "[]");
}

#[test]
fn serialize_array() {
    assert_ser("[1,2,3]", "[1,2,3]");
}

#[test]
fn serialize_nested_array() {
    assert_ser("[[1,2],[3,4]]", "[[1,2],[3,4]]");
}

#[test]
fn serialize_empty_object() {
    assert_ser("{}", "{}");
}

#[test]
fn serialize_object() {
    assert_ser("{\"a\":1}", "{\"a\":1}");
}

#[test]
fn serialize_nested_object() {
    assert_ser("{\"a\":{\"b\":2}}", "{\"a\":{\"b\":2}}");
}

#[test]
fn serialize_mixed_object() {
    assert_ser(
        "{\"name\":\"test\",\"value\":42,\"active\":true}",
        "{\"name\":\"test\",\"value\":42,\"active\":true}",
    );
}

#[test]
fn serialize_mixed_array() {
    assert_ser("[1,\"two\",true,null]", "[1,\"two\",true,null]");
}

#[test]
fn serialize_string_with_escapes() {
    assert_ser("{\"msg\":\"line1\\nline2\"}", "{\"msg\":\"line1\\nline2\"}");
}

/// Assert that parsing and serializing is idempotent
fn assert_roundtrip(json: &str) {
    let mink_literal = mink_str(json);
    let test = format!(
        r#"
fn main() -> Int {{
    let arena = rt_alloc(65536);
    let v1 = json_parse({mink_literal}, arena);
    if v1 == 0 {{ rt_free(arena); rt_exit(1); }}
    // Serialize twice from the same parsed value — must be identical
    let s1 = json_to_str(arena, v1);
    let len1 = rt_str_len(s1);
    let s2 = json_to_str(arena, v1);
    let len2 = rt_str_len(s2);
    let ok = len1 == len2;
    rt_str_free(s1);
    rt_str_free(s2);
    rt_free(arena);
    if ok {{ rt_exit(0); }} else {{ rt_exit(3); }}
}}"#
    );
    let (code, _) = build_and_run(&test);
    assert_eq!(code, 0, "roundtrip failed for {json:?} (exit code: {code})");
}

#[test]
fn roundtrip_null() {
    assert_roundtrip("null");
}

#[test]
fn roundtrip_bool() {
    assert_roundtrip("true");
    assert_roundtrip("false");
}

#[test]
fn roundtrip_number() {
    assert_roundtrip("42");
    assert_roundtrip("-7");
}

#[test]
fn roundtrip_string() {
    assert_roundtrip("\"hello\"");
}

#[test]
fn roundtrip_array() {
    assert_roundtrip("[1,2,3]");
}

#[test]
fn roundtrip_object() {
    assert_roundtrip("{\"a\":1,\"b\":\"two\"}");
}

#[test]
fn roundtrip_nested() {
    assert_roundtrip("{\"arr\":[1,{\"x\":2}],\"str\":\"hi\"}");
}

/// Assert multiple parses into same arena work correctly
fn assert_multi_parse(jsons: &[&str]) {
    let mut body = String::from(
        r#"
fn main() -> Int {
    let arena = rt_alloc(65536);
    let mut ok = true;
"#,
    );
    for (i, json) in jsons.iter().enumerate() {
        let lit = mink_str(json);
        body.push_str(&format!(
            "    let v{i} = json_parse({lit}, arena);\n    if v{i} == 0 {{ ok = false; }}\n"
        ));
    }
    body.push_str(
        r#"    rt_free(arena);
    if ok { rt_exit(0); } else { rt_exit(1); }
}"#,
    );
    let (code, _) = build_and_run(&body);
    assert_eq!(code, 0, "multi-parse failed (exit code: {code})");
}

#[test]
fn multiple_independent_parses() {
    assert_multi_parse(&["null", "true", "42", "\"abc\"", "[]", "{}"]);
}

// =========================================================================
// Session 127: insignificant whitespace
// =========================================================================

#[test]
fn object_members_allow_whitespace_between_tokens() {
    // The object parser consumed `,` without skipping the whitespace that
    // follows, so a pretty-printed document (`{ "a": 1, "b": 2 }`) was
    // rejected while its compact twin parsed — the very form `json.dump`
    // emits by default. Regression for the H validation milestone.
    assert_parse_ok(r#"{"a": 1, "b": 2}"#);
    assert_parse_ok(r#"{ "a" : 1 , "b" : 2 }"#);
    assert_parse_ok("{\n  \"a\": 1,\n  \"b\": 2\n}");
    assert_parse_ok("{\t\"a\":\t1,\t\"b\":\t2\t}");
    assert_parse_ok(r#"{"a": {"b": 1, "c": 2}, "d": [1, 2]}"#);
    assert_parse_ok("{\r\n  \"root\": \"x\",\r\n  \"out\": \"y\",\r\n  \"extensions\": [\".txt\", \".log\"]\r\n}");
}

#[test]
fn object_members_still_reject_garbage_after_comma() {
    // The whitespace fix must not make the parser permissive.
    assert_parse_err(r#"{"a": 1, }"#);
    assert_parse_err(r#"{"a": 1 "b": 2}"#);
    assert_parse_err(r#"{,}"#);
}

// =========================================================================
// Session 138: serializer output must not overlap the parsed value tree
// =========================================================================

/// A compact integer array (`[0,1,2,...]`) whose text is at most `max_len`
/// bytes and as close to it as the next element allows.
fn compact_integer_array(max_len: usize) -> String {
    let mut nums = Vec::new();
    let mut len = 2; // "[]"
    let mut n = 0;
    loop {
        let text = n.to_string();
        let add = text.len() + usize::from(!nums.is_empty());
        if len + add > max_len {
            break;
        }
        nums.push(text);
        len += add;
        n += 1;
    }
    format!("[{}]", nums.join(","))
}

/// Assert that JSON parses and serializes back byte-identically in a
/// caller-sized arena. The library's documented default is 64 KiB, but
/// `json_parse`/`json_to_str` take a caller-supplied `Ptr<Int>` and therefore
/// support larger arenas.
fn assert_ser_in_arena(json: &str, arena_bytes: usize) {
    let mink_literal = mink_str(json);
    let expected_literal = mink_str(json);
    let test = format!(
        r#"
fn main() -> Int {{
    let arena = rt_alloc({arena_bytes});
    let v = json_parse({mink_literal}, arena);
    if v == 0 {{ rt_free(arena); rt_exit(1); }}
    let s = json_to_str(arena, v);
    let exp = {expected_literal};
    let ok = rt_str_eq(s, exp);
    rt_str_free(s);
    rt_free(arena);
    if ok {{ rt_exit(0); }} else {{ rt_exit(2); }}
}}"#
    );
    let (code, _) = build_and_run(&test);
    assert_eq!(
        code, 0,
        "serialize in a {arena_bytes}-byte arena failed (exit code: {code})"
    );
}

#[test]
fn serialize_2047_byte_document_in_documented_arena() {
    let doc = compact_integer_array(2047);
    assert_eq!(doc.len(), 2047);
    assert_ser_in_arena(&doc, 65536);
}

#[test]
fn serialize_3499_byte_document_in_enlarged_arena() {
    let doc = compact_integer_array(3499);
    assert_eq!(doc.len(), 3499);
    // The parsed value tree ends below the arena's persistent write-pointer
    // slot but overlaps the old fixed output base (slot 7168). Before
    // Session 138 this document serialized to a shorter, wrong string with
    // `json_to_str` reporting no error; it must now round-trip exactly.
    assert_ser_in_arena(&doc, 1 << 20);
}

#[test]
fn parse_rejects_document_that_would_reach_the_write_pointer() {
    // A 4091-byte document's value tree crosses slot 8191, where the library
    // stores its persistent write pointer. A larger caller-supplied arena
    // keeps the writes in bounds, so before Session 138 the parse returned a
    // value whose bytes the pointer write had overwritten (silent middle
    // corruption). It must now be refused: parse returns 0, and no corrupt
    // value is exposed.
    let doc = compact_integer_array(4091);
    assert_eq!(doc.len(), 4091);
    let mink_literal = mink_str(&doc);
    let test = format!(
        r#"
fn main() -> Int {{
    let arena = rt_alloc(1048576);
    let v = json_parse({mink_literal}, arena);
    rt_free(arena);
    if v == 0 {{ rt_exit(0); }} else {{ rt_exit(3); }}
}}"#
    );
    let (code, _) = build_and_run(&test);
    assert_eq!(code, 0, "oversized parse must be refused (exit {code})");
}

// =========================================================================
// Session 174: caller-sized arena capacity
// =========================================================================

/// An object whose compact text is exactly `size` bytes (minimum 10).
fn padded_object(size: usize) -> String {
    assert!(size >= 10);
    format!("{{\"pad\":\"{}\"}}", "X".repeat(size - 10))
}

/// Parse and serialize with an explicit arena slot count. Returns the MINK
/// exit code: 0 round-trip, 1 parse refused, 2 serialized bytes differ.
fn run_capacity_parse(json: &str, arena_bytes: usize, slots: usize) -> i32 {
    let mink_literal = mink_str(json);
    let ptr_slot = slots - 1;
    let test = format!(
        r#"
fn main() -> Int {{
    let arena = rt_alloc({arena_bytes});
    rt_mem_store(arena + {ptr_slot} * 8, 0);
    let v = json_parse_in({mink_literal}, arena, {slots});
    if v == 0 {{ rt_free(arena); rt_exit(1); }}
    let s = json_to_str(arena, v);
    let exp = {mink_literal};
    let ok = rt_str_eq(s, exp);
    rt_str_free(s);
    rt_free(arena);
    if ok {{ rt_exit(0); }} else {{ rt_exit(2); }}
}}"#
    );
    build_and_run(&test).0
}

#[test]
fn parse_8192_byte_document_with_caller_capacity() {
    let doc = padded_object(8192);
    assert_eq!(doc.len(), 8192);
    // 256 KiB = 32768 slots; the write pointer lives in the final slot.
    assert_eq!(run_capacity_parse(&doc, 262144, 32768), 0);
}

#[test]
fn capacity_aware_parse_refuses_document_larger_than_arena() {
    let doc = padded_object(8192);
    assert_eq!(run_capacity_parse(&doc, 4096, 512), 1);
}
