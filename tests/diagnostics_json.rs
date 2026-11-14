//! M7 Phase C — Gate C: machine-readable diagnostics conformance corpus.
//!
//! Every case runs the real CLI and asserts on the JSON it prints, so this
//! file covers the contract a tool consumes rather than the renderer's
//! internals: schema version, per-diagnostic source attribution, byte-accurate
//! spans, failure paths that are not compile errors, and byte-identical
//! output for the same input.
//!
//! The reported schema is a superset of the one that existed before Phase C
//! (fields were added; none were renamed or removed), which is why
//! `schema_version` starts at 1.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn unique_dir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mink_diag_{}_{n}_{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write(dir: &std::path::Path, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, source).expect("write temp source");
    path
}

struct CheckOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

impl CheckOutput {
    fn json(&self) -> &str {
        assert!(
            validate_json(&self.stdout),
            "stdout is not valid JSON:\n{}",
            self.stdout
        );
        &self.stdout
    }

    fn fails_with_code(&self, code: i32) {
        assert_eq!(
            self.code, code,
            "expected exit {code}, got {}; stdout:\n{}\nstderr:\n{}",
            self.code, self.stdout, self.stderr
        );
    }
}

fn check_json(path: &std::path::Path) -> CheckOutput {
    let output = mink()
        .arg("check")
        .arg(path)
        .arg("--json")
        .output()
        .expect("run the mink CLI");
    CheckOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

// ----------------------------------------------------------------------
// A minimal JSON validator.
//
// The compiler crate has no dependencies and its tests keep it that way, so
// "the output is machine-readable" is checked against the grammar rather than
// a substring search: an unescaped quote or a trailing comma has to fail here.
// ----------------------------------------------------------------------

fn validate_json(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    if !skip_ws(bytes, &mut i) || !value(bytes, &mut i) {
        return false;
    }
    skip_ws(bytes, &mut i) && i == bytes.len()
}

fn skip_ws(bytes: &[u8], i: &mut usize) -> bool {
    while *i < bytes.len() && matches!(bytes[*i], b' ' | b'\t' | b'\n' | b'\r') {
        *i += 1;
    }
    true
}

fn value(bytes: &[u8], i: &mut usize) -> bool {
    if *i >= bytes.len() {
        return false;
    }
    match bytes[*i] {
        b'{' => object(bytes, i),
        b'[' => array(bytes, i),
        b'"' => string(bytes, i),
        b't' => literal(bytes, i, "true"),
        b'f' => literal(bytes, i, "false"),
        b'n' => literal(bytes, i, "null"),
        b'-' | b'0'..=b'9' => number(bytes, i),
        _ => false,
    }
}

fn literal(bytes: &[u8], i: &mut usize, word: &str) -> bool {
    if bytes[*i..].starts_with(word.as_bytes()) {
        *i += word.len();
        true
    } else {
        false
    }
}

fn object(bytes: &[u8], i: &mut usize) -> bool {
    *i += 1; // '{'
    skip_ws(bytes, i);
    if *i < bytes.len() && bytes[*i] == b'}' {
        *i += 1;
        return true;
    }
    loop {
        skip_ws(bytes, i);
        if *i >= bytes.len() || bytes[*i] != b'"' || !string(bytes, i) {
            return false;
        }
        skip_ws(bytes, i);
        if *i >= bytes.len() || bytes[*i] != b':' {
            return false;
        }
        *i += 1;
        skip_ws(bytes, i);
        if !value(bytes, i) {
            return false;
        }
        skip_ws(bytes, i);
        match bytes.get(*i) {
            Some(b',') => *i += 1,
            Some(b'}') => {
                *i += 1;
                return true;
            }
            _ => return false,
        }
    }
}

fn array(bytes: &[u8], i: &mut usize) -> bool {
    *i += 1; // '['
    skip_ws(bytes, i);
    if *i < bytes.len() && bytes[*i] == b']' {
        *i += 1;
        return true;
    }
    loop {
        skip_ws(bytes, i);
        if !value(bytes, i) {
            return false;
        }
        skip_ws(bytes, i);
        match bytes.get(*i) {
            Some(b',') => *i += 1,
            Some(b']') => {
                *i += 1;
                return true;
            }
            _ => return false,
        }
    }
}

fn string(bytes: &[u8], i: &mut usize) -> bool {
    *i += 1; // '"'
    while *i < bytes.len() {
        match bytes[*i] {
            b'"' => {
                *i += 1;
                return true;
            }
            b'\\' => {
                *i += 1;
                match bytes.get(*i) {
                    Some(b'"') | Some(b'\\') | Some(b'/') | Some(b'b') | Some(b'f')
                    | Some(b'n') | Some(b'r') | Some(b't') => *i += 1,
                    Some(b'u') => {
                        *i += 1;
                        for _ in 0..4 {
                            match bytes.get(*i) {
                                Some(c) if c.is_ascii_hexdigit() => *i += 1,
                                _ => return false,
                            }
                        }
                    }
                    _ => return false,
                }
            }
            // Raw control characters are not legal inside a JSON string.
            c if c < 0x20 => return false,
            _ => *i += 1,
        }
    }
    false
}

fn number(bytes: &[u8], i: &mut usize) -> bool {
    if bytes.get(*i) == Some(&b'-') {
        *i += 1;
    }
    let start = *i;
    while *i < bytes.len() && bytes[*i].is_ascii_digit() {
        *i += 1;
    }
    if *i == start {
        return false;
    }
    // Optional fraction and exponent, so `1.5` and `1e9` stay valid.
    if bytes.get(*i) == Some(&b'.') {
        *i += 1;
        let frac = *i;
        while *i < bytes.len() && bytes[*i].is_ascii_digit() {
            *i += 1;
        }
        if *i == frac {
            return false;
        }
    }
    if matches!(bytes.get(*i), Some(b'e') | Some(b'E')) {
        *i += 1;
        if matches!(bytes.get(*i), Some(b'+') | Some(b'-')) {
            *i += 1;
        }
        let exp = *i;
        while *i < bytes.len() && bytes[*i].is_ascii_digit() {
            *i += 1;
        }
        if *i == exp {
            return false;
        }
    }
    true
}

/// Asserts that some diagnostic names a file ending in `suffix`.
///
/// Diagnostics carry the path the compiler used, which for a module is the
/// path it resolved, so the comparison is on the tail of that path.
fn assert_file_ends_with(json: &str, suffix: &str) {
    let needle = format!("{suffix}\"");
    assert!(
        json.contains(&needle),
        "expected a diagnostic whose file ends with {suffix}:\n{json}"
    );
}

/// Extracts the string value of the first `"key": "value"` occurrence.
fn string_field<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\": \"");
    let start = json.find(&needle)? + needle.len();
    let rest = &json[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

// ======================================================================
// C-JSON: schema shape
// ======================================================================

#[test]
fn c_json_success_reports_schema_version_and_empty_errors() {
    let dir = unique_dir("success");
    let path = write(&dir, "ok.mink", "fn main() { return 0; }\n");

    let out = check_json(&path);
    out.fails_with_code(0);
    let json = out.json();

    assert!(json.contains("\"schema_version\": 1"), "{json}");
    assert!(json.contains("\"success\": true"), "{json}");
    assert!(json.contains("\"exit_code\": 0"), "{json}");
    assert!(json.contains("\"files_checked\": 1"), "{json}");
    assert!(json.contains("\"errors\": []"), "{json}");
    assert!(json.contains("\"warnings\": []"), "{json}");
    // A successful check still reports which file was checked.
    assert!(json.contains("ok.mink"), "{json}");
    assert!(out.stderr.trim().is_empty(), "stderr: {}", out.stderr);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn c_json_failure_reports_error_stage_and_code() {
    let dir = unique_dir("stage");
    let path = write(&dir, "bad.mink", "fn main() {\n    return };\n}\n");

    let out = check_json(&path);
    out.fails_with_code(1);
    let json = out.json();

    assert!(json.contains("\"success\": false"), "{json}");
    assert!(json.contains("\"exit_code\": 1"), "{json}");
    assert!(json.contains("\"severity\": \"error\""), "{json}");
    // The stage is one of the pipeline stages; a syntax error cannot come
    // from semantic analysis.
    let stage = string_field(json, "stage").expect("stage field");
    assert!(
        matches!(stage, "parsing" | "lexing"),
        "syntax error reported at stage {stage}: {json}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn c_json_semantic_error_reports_semantics_stage() {
    let dir = unique_dir("sem");
    let path = write(
        &dir,
        "sem.mink",
        "fn main() {\n    let x = 1;\n    return y;\n}\n",
    );

    let out = check_json(&path);
    out.fails_with_code(1);
    let json = out.json();
    assert!(
        json.contains("\"stage\": \"semantics\""),
        "unresolved name should be a semantic error:\n{json}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ======================================================================
// C-SPAN: source attribution
// ======================================================================

#[test]
fn c_span_bytes_match_the_reported_source() {
    let dir = unique_dir("bytes");
    // The `y` on line 3 is the unresolved name; its byte offset must let a
    // tool slice exactly that identifier out of the file.
    let source = "fn main() {\n    let x = 1;\n    return y;\n}\n";
    let path = write(&dir, "bytes.mink", source);

    let out = check_json(&path);
    out.fails_with_code(1);
    let json = out.json();

    let start = json
        .split("\"start_byte\": ")
        .nth(1)
        .and_then(|rest| rest.split(',').next())
        .and_then(|n| n.trim().parse::<usize>().ok())
        .expect("start_byte field");
    let end = json
        .split("\"end_byte\": ")
        .nth(1)
        .and_then(|rest| rest.split(',').next())
        .and_then(|n| n.trim().parse::<usize>().ok())
        .expect("end_byte field");

    assert_eq!(&source[start..end], "y", "span must cover the identifier");
    assert!(json.contains("\"start_line\": 3"), "{json}");
    assert!(json.contains("\"start_column\": 12"), "{json}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn c_multi_module_errors_keep_their_own_file_and_position() {
    // Before Phase C every error was attributed to the root file, so a tool
    // jumping to `helper.mink:2:19` was sent to `main.mink` instead.
    let dir = unique_dir("multi");
    let main = write(
        &dir,
        "main.mink",
        "mod helper;\nfn main() {\n    return 0;\n}\n",
    );
    write(
        &dir,
        "helper.mink",
        "fn helper_fn() -> Int {\n    return missing_name;\n}\n",
    );

    let out = check_json(&main);
    out.fails_with_code(1);
    let json = out.json();

    assert_file_ends_with(json, "helper.mink");
    assert!(
        json.contains("\"start_line\": 2"),
        "error must keep its real line number:\n{json}"
    );
    assert!(
        json.contains("\"files_checked\": 2"),
        "both files were checked:\n{json}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn c_human_output_also_attributes_multi_module_errors() {
    let dir = unique_dir("human");
    let main = write(
        &dir,
        "main.mink",
        "mod helper;\nfn main() {\n    return 0;\n}\n",
    );
    write(
        &dir,
        "helper.mink",
        "fn helper_fn() -> Int {\n    return missing_name;\n}\n",
    );

    let output = mink().arg("check").arg(&main).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr.contains("helper.mink:2:12"),
        "human-readable output must name the module file too:\n{stderr}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn c_two_errors_in_one_run_are_both_reported() {
    let dir = unique_dir("two");
    let main = write(
        &dir,
        "main.mink",
        "mod a;\nmod b;\nfn main() {\n    return 0;\n}\n",
    );
    write(
        &dir,
        "a.mink",
        "fn a_fn() -> Int {\n    return nope_a;\n}\n",
    );
    write(
        &dir,
        "b.mink",
        "fn b_fn() -> Int {\n    return nope_b;\n}\n",
    );

    let out = check_json(&main);
    out.fails_with_code(1);
    let json = out.json();

    assert!(json.contains("nope_a"), "{json}");
    assert!(json.contains("nope_b"), "{json}");
    assert_file_ends_with(json, "a.mink");
    assert_file_ends_with(json, "b.mink");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn c_json_output_is_byte_identical_for_the_same_input() {
    let dir = unique_dir("det");
    let main = write(
        &dir,
        "main.mink",
        "mod a;\nmod b;\nfn main() {\n    return 0;\n}\n",
    );
    write(
        &dir,
        "a.mink",
        "fn a_fn() -> Int {\n    return nope_a;\n}\n",
    );
    write(
        &dir,
        "b.mink",
        "fn b_fn() -> Int {\n    return nope_b;\n}\n",
    );

    let first = check_json(&main);
    let second = check_json(&main);
    first.fails_with_code(1);
    assert_eq!(first.stdout, second.stdout, "diagnostics must be ordered");

    // Errors are ordered by file, then by byte offset: b.mink precedes
    // main.mink and a.mink precedes b.mink if both failed.
    let json = first.json();
    let a = json.find("a.mink\"").expect("a.mink");
    let b = json.find("b.mink\"").expect("b.mink");
    assert!(a < b, "errors should be ordered by file:\n{json}");

    let _ = std::fs::remove_dir_all(&dir);
}

// ======================================================================
// C-FAILURE: failures that are not compile errors
// ======================================================================

#[test]
fn c_json_reports_a_missing_file_instead_of_printing_nothing() {
    let dir = unique_dir("missing");
    let path = dir.join("absent.mink");

    let out = check_json(&path);
    out.fails_with_code(1);
    let json = out.json();

    assert!(json.contains("\"schema_version\": 1"), "{json}");
    assert!(json.contains("\"success\": false"), "{json}");
    assert!(json.contains("\"stage\": \"driver\""), "{json}");
    assert!(json.contains("\"code\": null"), "{json}");
    assert!(json.contains("\"span\": null"), "{json}");
    assert!(json.contains("absent.mink"), "{json}");
    // The prose belongs on stderr, not interleaved with the JSON.
    assert!(out.stdout.trim_start().starts_with('{'), "{}", out.stdout);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn c_json_reports_a_directory_argument() {
    let dir = unique_dir("dirarg");

    let out = check_json(&dir);
    out.fails_with_code(1);
    let json = out.json();
    assert!(json.contains("\"stage\": \"driver\""), "{json}");
    assert!(json.contains("is a directory"), "{json}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn c_json_handles_spaces_and_non_ascii_paths() {
    let dir = unique_dir("unicode").join("with space \u{fc}");
    std::fs::create_dir_all(&dir).unwrap();
    let path = write(
        &dir,
        "h\u{e9}llo \u{2603}.mink",
        "fn main() {\n    return nope;\n}\n",
    );

    let out = check_json(&path);
    out.fails_with_code(1);
    let json = out.json();
    assert!(json.contains("with space"), "{json}");
    assert!(json.contains("h\u{e9}llo"), "{json}");
    assert!(json.contains("nope"), "{json}");

    let _ = std::fs::remove_dir_all(&dir);
}

// ======================================================================
// C-COMPAT: the human-readable path is unchanged
// ======================================================================

#[test]
fn c_human_output_still_prints_success_sentence() {
    let dir = unique_dir("humanok");
    let path = write(&dir, "ok.mink", "fn main() { return 0; }\n");

    let output = mink().arg("check").arg(&path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("passed parsing"), "{stdout}");
    assert!(!stdout.contains("schema_version"), "{stdout}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn c_human_missing_file_error_stays_on_stderr() {
    let dir = unique_dir("humanmissing");
    let path = dir.join("absent.mink");

    let output = mink().arg("check").arg(&path).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("failed to read"), "{stderr}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("schema_version"),
        "human output must not change shape"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn c_explain_command_is_unaffected_by_the_schema_change() {
    let output = mink().arg("explain").arg("E-S01").output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("schema_version"), "{stdout}");
    assert!(stdout.to_lowercase().contains("module") || !stdout.trim().is_empty());
}
