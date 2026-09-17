//! Regex library integration tests — Session 109 (S02).
//!
//! Every test prepends `stdlib/re.mink`, builds a genuine Windows PE through
//! the real `mink build` CLI, and runs it. `rt_exit(0)` performs the runtime
//! leak check, so a clean exit also proves the arena/string ownership is
//! leak-free. No test-only shortcut bypasses the public `re_*` API.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn re_lib() -> String {
    std::fs::read_to_string("stdlib/re.mink").expect("failed to read stdlib/re.mink")
}

/// Small MINK output helpers emitted before each test body.
/// `rt_print_*` already terminate each value with CRLF, so the helpers add
/// nothing beyond the value and the free.
const PRELUDE: &str = r#"
fn say_i(v: Int) { rt_print_int(v); }
fn say_b(v: Bool) { if v { rt_print_str("T"); } else { rt_print_str("F"); } }
fn say_s(v: Str) { rt_print_str(v); rt_str_free(v); }
/// Print an element borrowed from a Vec without taking its ownership
/// (`rt_vec_free` releases the elements, so freeing here would double-free).
fn show_s(v: Str) { rt_print_str(v); }
"#;

fn build_and_run(body: &str) -> (i32, String) {
    let source = format!("{}\n{}\n{}\n", re_lib(), PRELUDE, body);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_re_test_{n}.mink"));
    std::fs::write(&path, source.replace("\r\n", "\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("build")
        .arg(&path)
        .output()
        .unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        panic!("build failed:\n{stderr}");
    }
    let run = Command::new(&exe).output().expect("failed to run test exe");
    let code = run.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
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
// Search / match / full-match fundamentals
// ============================================================================

#[test]
fn s02_search_finds_leftmost_digits() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let r = re_search("\\d+", "abc123def");
    say_i(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["3"]);
}

#[test]
fn s02_search_no_match_is_minus_one() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let r = re_search("\\d+", "abcdef");
    say_i(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["-1"]);
}

#[test]
fn s02_empty_pattern_matches_at_zero() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let r = re_search("", "abc");
    say_i(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["0"]);
}

#[test]
fn s02_empty_text_never_matches_nonempty_pattern() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let r = re_is_match("a", "");
    say_b(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["F"]);
}

#[test]
fn s02_anchors_start_and_end() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_is_match("^ab.*z$", "abXYZz");
    say_b(a.0);
    rt_str_free(a.1);
    let b = re_is_match("^b", "ab");
    say_b(b.0);
    rt_str_free(b.1);
    let c = re_is_match("a$", "ab");
    say_b(c.0);
    rt_str_free(c.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T", "F", "F"]);
}

#[test]
fn s02_match_is_anchored_at_start() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_match_end("[a-z]+", "hello world");
    say_i(a.0);
    rt_str_free(a.1);
    let b = re_match_end("[a-z]+", " hello");
    say_i(b.0);
    rt_str_free(b.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["5", "-1"]);
}

#[test]
fn s02_full_match_positive_and_negative() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_full_match("[a-z]+[0-9]+", "abc123");
    say_b(a.0);
    rt_str_free(a.1);
    let b = re_full_match("[a-z]+", "abc123");
    say_b(b.0);
    rt_str_free(b.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T", "F"]);
}

#[test]
fn s02_find_reports_span() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let r = re_find("[0-9]+", "xy42zz");
    say_i(r.0);
    say_i(r.1);
    rt_str_free(r.2);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["2", "4"]);
}

// ============================================================================
// Character classes
// ============================================================================

#[test]
fn s02_class_ranges_and_negation() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_full_match("[a-c]+", "abcabc");
    say_b(a.0);
    rt_str_free(a.1);
    let b = re_full_match("[a-c]+", "abcd");
    say_b(b.0);
    rt_str_free(b.1);
    let c = re_full_match("[^0-9]+", "hello");
    say_b(c.0);
    rt_str_free(c.1);
    let d = re_full_match("[^0-9]+", "he1lo");
    say_b(d.0);
    rt_str_free(d.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T", "F", "T", "F"]);
}

#[test]
fn s02_class_leading_bracket_and_dash_are_literal() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_full_match("[]a]+", "]a]");
    say_b(a.0);
    rt_str_free(a.1);
    let b = re_full_match("[a-]+", "a-a");
    say_b(b.0);
    rt_str_free(b.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T", "T"]);
}

#[test]
fn s02_shorthand_classes() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_full_match("\\w+", "abc_123");
    say_b(a.0);
    rt_str_free(a.1);
    let b = re_full_match("\\w+", "abc-123");
    say_b(b.0);
    rt_str_free(b.1);
    let c = re_full_match("\\W+", "---");
    say_b(c.0);
    rt_str_free(c.1);
    let d = re_full_match("\\S+", "abc");
    say_b(d.0);
    rt_str_free(d.1);
    let e = re_full_match("\\S+", "a c");
    say_b(e.0);
    rt_str_free(e.1);
    let f = re_full_match("\\D+", "abc");
    say_b(f.0);
    rt_str_free(f.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T", "F", "T", "T", "F", "T"]);
}

#[test]
fn s02_whitespace_class_matches_tab_newline_space() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let r = re_full_match("\\s+", " \t\r\n");
    say_b(r.0);
    rt_str_free(r.1);
    let w = re_full_match("\\s+", "x");
    say_b(w.0);
    rt_str_free(w.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T", "F"]);
}

// ============================================================================
// Quantifiers
// ============================================================================

#[test]
fn s02_star_plus_question() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_full_match("ab*c", "ac");
    say_b(a.0);
    rt_str_free(a.1);
    let b = re_full_match("ab*c", "abbbbc");
    say_b(b.0);
    rt_str_free(b.1);
    let c = re_full_match("ab+c", "ac");
    say_b(c.0);
    rt_str_free(c.1);
    let d = re_full_match("ab?c", "ac");
    say_b(d.0);
    rt_str_free(d.1);
    let e = re_full_match("ab?c", "abbc");
    say_b(e.0);
    rt_str_free(e.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T", "T", "F", "T", "F"]);
}

#[test]
fn s02_zero_width_star_match_at_start() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let r = re_search("x*", "abc");
    say_i(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["0"]);
}

#[test]
fn s02_brace_quantifiers() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_full_match("a{3}", "aaa");
    say_b(a.0);
    rt_str_free(a.1);
    let b = re_full_match("a{3}", "aa");
    say_b(b.0);
    rt_str_free(b.1);
    let c = re_full_match("a{2,4}", "aaa");
    say_b(c.0);
    rt_str_free(c.1);
    let d = re_full_match("a{2,4}", "aaaaa");
    say_b(d.0);
    rt_str_free(d.1);
    let e = re_full_match("a{2,}", "aaaaa");
    say_b(e.0);
    rt_str_free(e.1);
    let f = re_full_match("a{2,}", "a");
    say_b(f.0);
    rt_str_free(f.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T", "F", "T", "F", "T", "F"]);
}

#[test]
fn s02_literal_brace_is_not_a_quantifier() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let r = re_full_match("a{x}", "a{x}");
    say_b(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T"]);
}

#[test]
fn s02_lazy_quantifier_prefers_shortest() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let r = re_find("<.+?>", "<a><b>");
    say_i(r.0);
    say_i(r.1);
    rt_str_free(r.2);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["0", "3"]);
}

#[test]
fn s02_greedy_quantifier_prefers_longest() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let r = re_find("<.+>", "<a><b>");
    say_i(r.0);
    say_i(r.1);
    rt_str_free(r.2);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["0", "6"]);
}

// ============================================================================
// Groups and alternation
// ============================================================================

#[test]
fn s02_alternation() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_full_match("cat|dog", "dog");
    say_b(a.0);
    rt_str_free(a.1);
    let b = re_full_match("cat|dog", "cow");
    say_b(b.0);
    rt_str_free(b.1);
    let c = re_full_match("(cat|dog)s?", "dogs");
    say_b(c.0);
    rt_str_free(c.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T", "F", "T"]);
}

#[test]
fn s02_non_capturing_group() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_full_match("(?:ab)+", "ababab");
    say_b(a.0);
    rt_str_free(a.1);
    let b = re_full_match("(?:ab)+", "aba");
    say_b(b.0);
    rt_str_free(b.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T", "F"]);
}

#[test]
fn s02_nested_groups_and_alternation() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_full_match("((a|b)c)+", "acbc");
    say_b(a.0);
    rt_str_free(a.1);
    let b = re_full_match("((a|b)c)+", "acbd");
    say_b(b.0);
    rt_str_free(b.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T", "F"]);
}

// ============================================================================
// Escapes, dot, binaries
// ============================================================================

#[test]
fn s02_escaped_metacharacters_are_literal() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_full_match("\\.", ".");
    say_b(a.0);
    rt_str_free(a.1);
    let b = re_full_match("\\*", "*");
    say_b(b.0);
    rt_str_free(b.1);
    let c = re_full_match("\\[a\\]", "[a]");
    say_b(c.0);
    rt_str_free(c.1);
    let d = re_full_match("\\+", "x");
    say_b(d.0);
    rt_str_free(d.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T", "T", "T", "F"]);
}

#[test]
fn s02_dot_matches_any_byte_except_newline() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_full_match("a.c", "abc");
    say_b(a.0);
    rt_str_free(a.1);
    let b = re_full_match("a.c", "a\nc");
    say_b(b.0);
    rt_str_free(b.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T", "F"]);
}

#[test]
fn s02_utf8_bytes_are_byte_semantics() {
    // "é" is the two UTF-8 bytes C3 A9. Byte semantics: '.' matches one byte,
    // '..' matches two, and \w (ASCII) does not match either byte.
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_full_match("..", "é");
    say_b(a.0);
    rt_str_free(a.1);
    let b = re_full_match(".", "é");
    say_b(b.0);
    rt_str_free(b.1);
    let c = re_is_match("\\w", "é");
    say_b(c.0);
    rt_str_free(c.1);
    let d = re_is_match("\\x", "x");
    say_b(d.0);
    rt_str_free(d.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["T", "F", "F", "T"]);
}

// ============================================================================
// count / find_all / split / replace
// ============================================================================

#[test]
fn s02_count_non_overlapping() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let r = re_count("[0-9]+", "a1b22c333");
    say_i(r.0);
    rt_str_free(r.1);
    let z = re_count("[0-9]+", "none");
    say_i(z.0);
    rt_str_free(z.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["3", "0"]);
}

#[test]
fn s02_find_all_positions() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let r = re_find_all("[0-9]+", "a1b22c333");
    let n = rt_vec_len(r.0);
    say_i(n);
    let mut i = 0;
    while i < n {
        say_i(rt_vec_get(r.0, i));
        i = i + 1;
    }
    rt_vec_free(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["3", "1", "3", "6"]);
}

#[test]
fn s02_split_parts() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let r = re_split(",", "a,bb,ccc");
    let n = rt_vec_len(r.0);
    say_i(n);
    let mut i = 0;
    while i < n {
        show_s(rt_vec_get(r.0, i));
        i = i + 1;
    }
    rt_vec_free(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["3", "a", "bb", "ccc"]);
}

#[test]
fn s02_replace_all_and_first() {
    let (code, out) = build_and_run(
        r##"
fn main() {
    let a = re_replace("[0-9]+", "a1b22c", "#");
    say_s(a);
    let b = re_replace_first("[0-9]+", "a1b22c", "#");
    say_s(b);
    let c = re_replace("[0-9]+", "abcd", "#");
    say_s(c);
    rt_exit(0);
}"##,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["a#b#c", "a#b22c", "abcd"]);
}

// ============================================================================
// Malformed patterns must not crash
// ============================================================================

#[test]
fn s02_malformed_patterns_do_not_crash() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let a = re_is_match("[abc", "abc");
    say_b(a.0);
    rt_str_free(a.1);
    let b = re_is_match("(abc", "abc");
    say_b(b.0);
    rt_str_free(b.1);
    let c = re_is_match("a\\", "a");
    say_b(c.0);
    rt_str_free(c.1);
    let d = re_is_match("]", "]");
    say_b(d.0);
    rt_str_free(d.1);
    let e = re_is_match("[z-a]", "m");
    say_b(e.0);
    rt_str_free(e.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    // None of these may crash; the unclosed class matches its literal bytes.
    let lines = out_lines(&out);
    assert_eq!(lines.len(), 5);
}

#[test]
fn s02_pathological_backtracking_is_bounded() {
    // (a+)+b over a long run of 'a's with no 'b' is exponential for a naive
    // engine; the step budget must make it return "no match" quickly.
    let (code, out) = build_and_run(
        r#"
fn main() {
    let mut text = rt_str_alloc(32);
    let mut i = 0;
    while i < 31 {
        rt_str_set_byte(text, i, 97);
        i = i + 1;
    }
    rt_str_set_byte(text, 31, 33);
    let r = re_is_match("(a+)+b", text);
    say_b(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["F"]);
}

// ============================================================================
// Scale and ownership
// ============================================================================

#[test]
fn s02_large_input_search() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let big = rt_str_alloc(5000);
    let mut i = 0;
    while i < 5000 {
        rt_str_set_byte(big, i, 97);
        i = i + 1;
    }
    rt_str_set_byte(big, 4000, 55);
    let r = re_search("[0-9]+", big);
    say_i(r.0);
    rt_str_free(r.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["4000"]);
}

#[test]
fn s02_ownership_repeated_cycles_are_leak_free() {
    let (code, out) = build_and_run(
        r##"
fn main() {
    let mut i = 0;
    while i < 200 {
        let a = re_search("\\d+", "text 123");
        if a.0 != 5 { rt_exit(1); }
        rt_str_free(a.1);
        let b = re_replace("[0-9]+", "a1b2", "#");
        rt_str_free(b);
        let c = re_is_match("^[a-z]+$", "abc");
        if !c.0 { rt_exit(2); }
        rt_str_free(c.1);
        i = i + 1;
    }
    rt_exit(0);
}"##,
    );
    assert_ok(code, &out);
}

#[test]
fn s02_heap_subject_is_preserved_and_returned() {
    let (code, out) = build_and_run(
        r#"
fn main() {
    let text = rt_str_alloc(5);
    rt_str_set_byte(text, 0, 104);
    rt_str_set_byte(text, 1, 49);
    rt_str_set_byte(text, 2, 50);
    rt_str_set_byte(text, 3, 51);
    rt_str_set_byte(text, 4, 33);
    let r = re_search("[0-9]+", text);
    say_i(r.0);
    // The returned subject must still be the same intact heap string.
    say_i(rt_str_len(r.1));
    say_i(rt_str_byte(r.1, 0));
    rt_str_free(r.1);
    rt_exit(0);
}"#,
    );
    assert_ok(code, &out);
    assert_eq!(out_lines(&out), vec!["1", "5", "104"]);
}
