//! CSV library integration tests — Session 108 (S36).
//!
//! Builds `stdlib/csv.mink` together with each test body through the real
//! `mink build` CLI and runs the resulting Windows PE, asserting decoded
//! fields, quoting behaviour, boundaries and ownership.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn csv_lib() -> String {
    std::fs::read_to_string("stdlib/csv.mink").expect("failed to read stdlib/csv.mink")
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_csv_test_{n}_{name}.mink"));
    std::fs::write(&path, content.replace("\r\n", "\n")).unwrap();
    path
}

fn build_and_run(test_body: &str) -> (i32, String) {
    let lib = csv_lib();
    let source = format!("{}\n{}", lib, test_body);
    let path = temp_source("test", &source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        return (-1, stderr);
    }
    let run = Command::new(&exe).output().expect("failed to run test exe");
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let code = run.status.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (code, format!("{stdout}\n{stderr}"))
}

fn ints_of(out: &str) -> Vec<i64> {
    out.lines()
        .filter(|l| !l.is_empty() && !l.contains("runtime error") && !l.contains("memory leak"))
        .filter_map(|l| l.trim().parse::<i64>().ok())
        .collect()
}

/// MINK helper emitted with every test: prints the field count, then each
/// field's length followed by its bytes.
const DUMP: &str = r#"
fn dump(row: Vec<Str>) {
    let n = rt_vec_len(row);
    rt_print_int(n);
    let mut i = 0;
    while i < n {
        let f = rt_vec_get(row, i);
        let fl = rt_str_len(f);
        rt_print_int(fl);
        let mut j = 0;
        while j < fl {
            rt_print_int(rt_str_byte(f, j));
            j = j + 1;
        }
        i = i + 1;
    }
}
"#;

#[test]
fn c01_parse_plain_row() {
    let body = r#"
fn main() {
    let r = csv_parse_row("a,b,c");
    dump(r);
    rt_vec_free(r);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(&format!("{DUMP}\n{body}"));
    assert_eq!(code, 0, "{out}");
    // 3 fields: a, b, c
    assert_eq!(ints_of(&out), vec![3, 1, 97, 1, 98, 1, 99], "{out}");
}

#[test]
fn c02_quoted_field_with_delimiter() {
    let body = r#"
fn main() {
    let r = csv_parse_row("\"a,b\",c");
    dump(r);
    rt_vec_free(r);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(&format!("{DUMP}\n{body}"));
    assert_eq!(code, 0, "{out}");
    // 2 fields: "a,b" (3 bytes: 97 44 98), "c"
    assert_eq!(ints_of(&out), vec![2, 3, 97, 44, 98, 1, 99], "{out}");
}

#[test]
fn c03_escaped_quotes() {
    let body = r#"
fn main() {
    let r = csv_parse_row("\"a\"\"b\"");
    dump(r);
    rt_vec_free(r);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(&format!("{DUMP}\n{body}"));
    assert_eq!(code, 0, "{out}");
    // 1 field: a"b (3 bytes: 97 34 98)
    assert_eq!(ints_of(&out), vec![1, 3, 97, 34, 98], "{out}");
}

#[test]
fn c04_empty_fields_preserved() {
    let body = r#"
fn main() {
    let r = csv_parse_row("a,,b,");
    dump(r);
    rt_vec_free(r);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(&format!("{DUMP}\n{body}"));
    assert_eq!(code, 0, "{out}");
    // 4 fields: a, "", b, ""
    assert_eq!(ints_of(&out), vec![4, 1, 97, 0, 1, 98, 0], "{out}");
}

#[test]
fn c05_record_terminators() {
    let body = r#"
fn main() {
    let a = csv_parse_row("a,b\n");
    dump(a);
    rt_vec_free(a);
    let b = csv_parse_row("x,y\r\n");
    dump(b);
    rt_vec_free(b);
    let c = csv_parse_row("p,q\r");
    dump(c);
    rt_vec_free(c);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(&format!("{DUMP}\n{body}"));
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        ints_of(&out),
        vec![2, 1, 97, 1, 98, 2, 1, 120, 1, 121, 2, 1, 112, 1, 113],
        "{out}"
    );
}

#[test]
fn c06_empty_input_has_no_fields() {
    let body = r#"
fn main() {
    let r = csv_parse_row("");
    dump(r);
    rt_print_int(csv_field_count(r));
    rt_vec_free(r);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(&format!("{DUMP}\n{body}"));
    assert_eq!(code, 0, "{out}");
    // The dump prints the field count (0) and then csv_field_count (0).
    assert_eq!(ints_of(&out), vec![0, 0], "{out}");
}

#[test]
fn c07_write_plain_row() {
    let body = r#"
fn main() {
    let v = rt_vec_new(2);
    let v1 = rt_vec_push(v, "a");
    let v2 = rt_vec_push(v1, "b");
    let w = csv_write_row(v2);
    rt_print_int(rt_str_len(w));
    let mut i = 0;
    while i < rt_str_len(w) {
        rt_print_int(rt_str_byte(w, i));
        i = i + 1;
    }
    rt_vec_free(v2);
    rt_str_free(w);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    assert_eq!(ints_of(&out), vec![3, 97, 44, 98], "{out}");
}

#[test]
fn c08_write_quotes_when_needed() {
    let body = r#"
fn main() {
    let v = rt_vec_new(3);
    let v1 = rt_vec_push(v, "x,y");
    let v2 = rt_vec_push(v1, "z");
    let v3 = rt_vec_push(v2, "a\"b");
    let w = csv_write_row(v3);
    rt_print_int(rt_str_len(w));
    let mut i = 0;
    while i < rt_str_len(w) {
        rt_print_int(rt_str_byte(w, i));
        i = i + 1;
    }
    rt_vec_free(v3);
    rt_str_free(w);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    // `"x,y",z,"a""b"` = 14 bytes
    let expected = vec![
        14, 34, 120, 44, 121, 34, 44, 122, 44, 34, 97, 34, 34, 98, 34,
    ];
    assert_eq!(ints_of(&out), expected, "{out}");
}

#[test]
fn c09_round_trip_preserves_fields() {
    let body = r#"
fn main() {
    let v = rt_vec_new(4);
    let v1 = rt_vec_push(v, "plain");
    let v2 = rt_vec_push(v1, "with,comma");
    let v3 = rt_vec_push(v2, "with\"quote");
    let v4 = rt_vec_push(v3, "");
    let back = csv_round_trip(v4);
    dump(back);
    rt_vec_free(v4);
    rt_vec_free(back);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(&format!("{DUMP}\n{body}"));
    assert_eq!(code, 0, "{out}");
    let expected = vec![
        4, // fields
        5, 112, 108, 97, 105, 110, // plain
        10, 119, 105, 116, 104, 44, 99, 111, 109, 109, 97, // with,comma
        10, 119, 105, 116, 104, 34, 113, 117, 111, 116, 101, // with"quote
        0,   // empty
    ];
    assert_eq!(ints_of(&out), expected, "{out}");
}

#[test]
fn c10_escape_and_field_count() {
    let body = r#"
fn main() {
    let a = csv_escape("plain");
    rt_print_int(rt_str_len(a));
    rt_str_free(a);
    let b = csv_escape("a,b");
    rt_print_int(rt_str_len(b));
    rt_print_int(rt_str_byte(b, 0));
    rt_print_int(rt_str_byte(b, rt_str_len(b) - 1));
    rt_str_free(b);
    let c = csv_escape("q\"q");
    rt_print_int(rt_str_len(c));
    rt_str_free(c);
    let row = csv_parse_row("a,b,c,d");
    rt_print_int(csv_field_count(row));
    rt_vec_free(row);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    // "plain" -> 5 bytes; "a,b" -> 5 bytes wrapped in quotes (34 ... 34);
    // "q\"q" -> 6 bytes (3 + 2 wrapping + 1 doubled quote); 4 fields.
    assert_eq!(ints_of(&out), vec![5, 5, 34, 34, 6, 4], "{out}");
}

#[test]
fn c11_many_fields() {
    let body = r#"
fn main() {
    // Build "3,3,...,3" (100 fields) while freeing each intermediate so
    // the ownership check stays clean.
    let mut s = rt_str_alloc(1);
    rt_str_set_byte(s, 0, 51); // '3'
    let mut i = 1;
    while i < 100 {
        let t = rt_str_concat(s, ",3");
        rt_str_free(s);
        s = t;
        i = i + 1;
    }
    let row = csv_parse_row(s);
    rt_print_int(rt_vec_len(row));
    let last = rt_vec_get(row, rt_vec_len(row) - 1);
    rt_print_int(rt_str_len(last));
    rt_vec_free(row);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    // 100 fields, each 1 byte.
    assert_eq!(ints_of(&out), vec![100, 1], "{out}");
}

#[test]
fn c12_ownership_repeated_parse_write() {
    let body = r#"
fn main() {
    let mut i = 0;
    while i < 100 {
        let text = rt_str_from_int(i);
        let line = rt_str_concat(text, ",tail");
        rt_str_free(text);
        let row = csv_parse_row(line);
        let w = csv_write_row(row);
        let back = csv_parse_row(w);
        rt_vec_free(row);
        rt_vec_free(back);
        i = i + 1;
    }
    rt_print_int(1);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    assert_eq!(ints_of(&out), vec![1], "{out}");
}

#[test]
fn c13_mid_field_quote_is_literal() {
    let body = r#"
fn main() {
    // A quote that does not start a field is data, matching the lenient
    // behaviour Python's csv reader exhibits for this input.
    let r = csv_parse_row("ab\"cd");
    dump(r);
    rt_vec_free(r);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(&format!("{DUMP}\n{body}"));
    assert_eq!(code, 0, "{out}");
    // 1 field: ab"cd (5 bytes: 97 98 34 99 100)
    assert_eq!(ints_of(&out), vec![1, 5, 97, 98, 34, 99, 100], "{out}");
}

#[test]
fn c14_embedded_newline_inside_quotes() {
    let body = r#"
fn main() {
    let r = csv_parse_row("\"line1\nline2\",z");
    dump(r);
    rt_vec_free(r);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(&format!("{DUMP}\n{body}"));
    assert_eq!(code, 0, "{out}");
    // 2 fields: the quoted field keeps its newline (11 bytes), then "z".
    assert_eq!(
        ints_of(&out),
        vec![
            2, 11, 108, 105, 110, 101, 49, 10, 108, 105, 110, 101, 50, 1, 122
        ],
        "{out}"
    );
}
