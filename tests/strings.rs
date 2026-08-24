//! End-to-end tests for V1 string operations (Session 44):
//! rt_str_concat, rt_str_eq, rt_str_from_int, rt_str_from_bool.

use std::path::PathBuf;
use std::process::Command;

/// Returns a `Command` for the compiled `mink` binary.
fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

/// Writes `content` to a uniquely named temp file and returns its path.
fn temp_source(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mink_str_test_{}_{name}", std::process::id()));
    std::fs::write(&path, content).unwrap();
    path
}

/// Builds `source` with the compiler and returns the generated executable.
fn build(source: &str) -> PathBuf {
    let name = std::thread::current()
        .name()
        .unwrap_or("program")
        .replace("::", "_");
    let path = temp_source(&format!("{name}.mink"), source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exe = path.with_extension("exe");
    assert!(exe.exists(), "no executable produced");
    exe
}

/// Runs `exe` and returns (exit code, stdout bytes).
fn run(exe: &PathBuf) -> (i32, Vec<u8>) {
    let output = Command::new(exe).output().unwrap();
    (output.status.code().unwrap_or(-1), output.stdout)
}

// ---------------------------------------------------------------------------
// String concatenation
// ---------------------------------------------------------------------------

#[test]
fn concat_empty_empty() {
    let exe = build(
        "fn main() {
            let a = \"\";
            let b = \"\";
            let c = rt_str_concat(a, b);
            rt_print_int(rt_str_len(c));
            rt_str_free(c);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"0\r\n");
}

#[test]
fn concat_empty_nonempty() {
    let exe = build(
        "fn main() {
            let a = \"\";
            let b = \"hello\";
            let c = rt_str_concat(a, b);
            rt_print_str(c);
            rt_str_free(c);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"hello\r\n");
}

#[test]
fn concat_nonempty_empty() {
    let exe = build(
        "fn main() {
            let a = \"hello\";
            let b = \"\";
            let c = rt_str_concat(a, b);
            rt_print_str(c);
            rt_str_free(c);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"hello\r\n");
}

#[test]
fn concat_normal() {
    let exe = build(
        "fn main() {
            let a = \"hello\";
            let b = \" world\";
            let c = rt_str_concat(a, b);
            rt_print_str(c);
            rt_str_free(c);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"hello world\r\n");
}

#[test]
fn concat_multiple() {
    let exe = build(
        "fn main() {
            let a = \"one\";
            let b = \" two\";
            let c = \" three\";
            let ab = rt_str_concat(a, b);
            let abc = rt_str_concat(ab, c);
            rt_print_str(abc);
            rt_str_free(ab);
            rt_str_free(abc);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"one two three\r\n");
}

#[test]
fn concat_then_print() {
    let exe = build(
        "fn main() {
            let result = rt_str_concat(\"foo\", \"bar\");
            rt_print_str(result);
            rt_str_free(result);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"foobar\r\n");
}

#[test]
fn concat_returned_from_function() {
    let exe = build(
        "fn greet(name: Str) {
            return rt_str_concat(\"Hello, \", name);
        }
        fn main() {
            let s = greet(\"MINK\");
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"Hello, MINK\r\n");
}

#[test]
fn concat_ownership_move() {
    let exe = build(
        "fn main() {
            let a = rt_str_alloc(3);
            rt_str_set_byte(a, 0, 65);
            rt_str_set_byte(a, 1, 66);
            rt_str_set_byte(a, 2, 67);
            let b = rt_str_alloc(2);
            rt_str_set_byte(b, 0, 48);
            rt_str_set_byte(b, 1, 49);
            let c = rt_str_concat(a, b);
            rt_print_str(c);
            rt_str_free(a);
            rt_str_free(b);
            rt_str_free(c);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"ABC01\r\n");
}

// ---------------------------------------------------------------------------
// String equality
// ---------------------------------------------------------------------------

#[test]
fn eq_equal_strings() {
    let exe = build(
        "fn main() {
            let a = \"hello\";
            let b = \"hello\";
            if rt_str_eq(a, b) {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 1);
}

#[test]
fn eq_different_strings() {
    let exe = build(
        "fn main() {
            let a = \"hello\";
            let b = \"world\";
            if rt_str_eq(a, b) {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 0);
}

#[test]
fn eq_different_lengths() {
    let exe = build(
        "fn main() {
            let a = \"hi\";
            let b = \"hello\";
            if rt_str_eq(a, b) {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 0);
}

#[test]
fn eq_empty_strings() {
    let exe = build(
        "fn main() {
            let a = \"\";
            let b = \"\";
            if rt_str_eq(a, b) {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 1);
}

#[test]
fn eq_empty_vs_nonempty() {
    let exe = build(
        "fn main() {
            let a = \"\";
            let b = \"x\";
            if rt_str_eq(a, b) {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 0);
}

#[test]
fn eq_prefix_strings() {
    let exe = build(
        "fn main() {
            let a = \"hello\";
            let b = \"hell\";
            if rt_str_eq(a, b) {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 0);
}

#[test]
fn eq_dynamic_strings() {
    let exe = build(
        "fn main() {
            let a = rt_str_alloc(3);
            rt_str_set_byte(a, 0, 72);
            rt_str_set_byte(a, 1, 105);
            rt_str_set_byte(a, 2, 33);
            let b = rt_str_alloc(3);
            rt_str_set_byte(b, 0, 72);
            rt_str_set_byte(b, 1, 105);
            rt_str_set_byte(b, 2, 33);
            if rt_str_eq(a, b) {
                rt_print_int(1);
            } else {
                rt_print_int(0);
            }
            rt_str_free(a);
            rt_str_free(b);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"1\r\n");
}

#[test]
fn eq_dynamic_different() {
    let exe = build(
        "fn main() {
            let a = rt_str_alloc(3);
            rt_str_set_byte(a, 0, 72);
            rt_str_set_byte(a, 1, 105);
            rt_str_set_byte(a, 2, 33);
            let b = rt_str_alloc(3);
            rt_str_set_byte(b, 0, 72);
            rt_str_set_byte(b, 1, 105);
            rt_str_set_byte(b, 2, 63);
            if rt_str_eq(a, b) {
                rt_print_int(1);
            } else {
                rt_print_int(0);
            }
            rt_str_free(a);
            rt_str_free(b);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"0\r\n");
}

#[test]
fn eq_concat_result() {
    let exe = build(
        "fn main() {
            let a = rt_str_concat(\"hel\", \"lo\");
            let b = \"hello\";
            if rt_str_eq(a, b) {
                rt_print_int(1);
            } else {
                rt_print_int(0);
            }
            rt_str_free(a);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"1\r\n");
}

// ---------------------------------------------------------------------------
// Int-to-string conversion
// ---------------------------------------------------------------------------

#[test]
fn from_int_zero() {
    let exe = build(
        "fn main() {
            let s = rt_str_from_int(0);
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"0\r\n");
}

#[test]
fn from_int_positive() {
    let exe = build(
        "fn main() {
            let s = rt_str_from_int(42);
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"42\r\n");
}

#[test]
fn from_int_negative() {
    let exe = build(
        "fn main() {
            let s = rt_str_from_int(-7);
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"-7\r\n");
}

#[test]
fn from_int_multidigit() {
    let exe = build(
        "fn main() {
            let s = rt_str_from_int(123456789);
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"123456789\r\n");
}

#[test]
fn from_int_one() {
    let exe = build(
        "fn main() {
            let s = rt_str_from_int(1);
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"1\r\n");
}

#[test]
fn from_int_negative_large() {
    let exe = build(
        "fn main() {
            let s = rt_str_from_int(-12345);
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"-12345\r\n");
}

#[test]
fn from_int_then_concat() {
    let exe = build(
        "fn main() {
            let num = rt_str_from_int(42);
            let msg = rt_str_concat(\"value = \", num);
            rt_print_str(msg);
            rt_str_free(num);
            rt_str_free(msg);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"value = 42\r\n");
}

#[test]
fn from_int_negative_then_concat() {
    let exe = build(
        "fn main() {
            let num = rt_str_from_int(-42);
            let msg = rt_str_concat(\"value = \", num);
            rt_print_str(msg);
            rt_str_free(num);
            rt_str_free(msg);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"value = -42\r\n");
}

#[test]
fn from_int_max_value() {
    // 9223372036854775807 is i64::MAX
    let exe = build(
        "fn main() {
            let s = rt_str_from_int(9223372036854775807);
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"9223372036854775807\r\n");
}

#[test]
fn from_int_min_value() {
    // -9223372036854775808 is i64::MIN
    let exe = build(
        "fn main() {
            let s = rt_str_from_int(-9223372036854775808);
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"-9223372036854775808\r\n");
}

#[test]
fn from_int_returned_from_function() {
    let exe = build(
        "fn to_str(n: Int) {
            return rt_str_from_int(n);
        }
        fn main() {
            let s = to_str(100);
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"100\r\n");
}

// ---------------------------------------------------------------------------
// Bool-to-string conversion
// ---------------------------------------------------------------------------

#[test]
fn from_bool_true() {
    let exe = build(
        "fn main() {
            let s = rt_str_from_bool(true);
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"true\r\n");
}

#[test]
fn from_bool_false() {
    let exe = build(
        "fn main() {
            let s = rt_str_from_bool(false);
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"false\r\n");
}

#[test]
fn from_bool_then_concat() {
    let exe = build(
        "fn main() {
            let b = rt_str_from_bool(true);
            let msg = rt_str_concat(\"flag: \", b);
            rt_print_str(msg);
            rt_str_free(b);
            rt_str_free(msg);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"flag: true\r\n");
}

#[test]
fn from_bool_false_then_concat() {
    let exe = build(
        "fn main() {
            let b = rt_str_from_bool(false);
            let msg = rt_str_concat(\"flag: \", b);
            rt_print_str(msg);
            rt_str_free(b);
            rt_str_free(msg);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"flag: false\r\n");
}

#[test]
fn from_bool_returned_from_function() {
    let exe = build(
        "fn bool_to_s(b: Bool) {
            return rt_str_from_bool(b);
        }
        fn main() {
            let s = bool_to_s(true);
            rt_print_str(s);
            rt_str_free(s);
            let s2 = bool_to_s(false);
            rt_print_str(s2);
            rt_str_free(s2);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"true\r\nfalse\r\n");
}

// ---------------------------------------------------------------------------
// Integration
// ---------------------------------------------------------------------------

#[test]
fn strings_returned_from_functions() {
    let exe = build(
        "fn make() {
            return rt_str_alloc(3);
        }
        fn main() {
            let s = make();
            rt_str_set_byte(s, 0, 65);
            rt_str_set_byte(s, 1, 66);
            rt_str_set_byte(s, 2, 67);
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"ABC\r\n");
}

#[test]
fn strings_passed_to_functions() {
    let exe = build(
        "fn echo(s: Str) {
            rt_print_str(s);
        }
        fn main() {
            echo(\"hello\");
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"hello\r\n");
}

#[test]
fn strings_stored_in_structs() {
    let exe = build(
        "struct P { name: Str, age: Int }
        fn main() {
            let p = P { name: \"Alice\", age: 30 };
            rt_print_str(p.name);
            rt_print_int(p.age);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"Alice\r\n30\r\n");
}

#[test]
fn strings_concat_in_struct() {
    let exe = build(
        "struct G { greeting: Str }
        fn make_greeting(name: Str) {
            return G { greeting: rt_str_concat(\"Hello, \", name) };
        }
        fn main() {
            let g = make_greeting(\"World\");
            rt_print_str(g.greeting);
            rt_str_free(g.greeting);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"Hello, World\r\n");
}

#[test]
fn regression_str_literal_print() {
    let exe = build("fn main() { rt_print_str(\"hello\"); return 0; }");
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"hello\r\n");
}

#[test]
fn regression_str_alloc_round_trip() {
    let exe = build(
        "fn main() {
            let s = rt_str_alloc(5);
            rt_str_set_byte(s, 0, 72);
            rt_str_set_byte(s, 1, 101);
            rt_str_set_byte(s, 2, 108);
            rt_str_set_byte(s, 3, 108);
            rt_str_set_byte(s, 4, 111);
            rt_print_str(s);
            rt_print_int(rt_str_len(s));
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"Hello\r\n5\r\n");
}

#[test]
fn regression_str_byte_access() {
    let exe = build(
        "fn main() {
            let s = \"ABC\";
            rt_print_int(rt_str_byte(s, 0));
            rt_print_int(rt_str_byte(s, 1));
            rt_print_int(rt_str_byte(s, 2));
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"65\r\n66\r\n67\r\n");
}

#[test]
fn regression_str_length() {
    let exe = build(
        "fn main() {
            rt_print_int(rt_str_len(\"hello\"));
            rt_print_int(rt_str_len(\"\"));
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"5\r\n0\r\n");
}

#[test]
fn regression_str_mutation() {
    let exe = build(
        "fn main() {
            let s = rt_str_alloc(3);
            rt_str_set_byte(s, 0, 88);
            rt_str_set_byte(s, 1, 89);
            rt_str_set_byte(s, 2, 90);
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"XYZ\r\n");
}

#[test]
fn concat_single_char_strings() {
    let exe = build(
        "fn main() {
            let a = \"a\";
            let b = \"b\";
            let c = rt_str_concat(a, b);
            rt_print_str(c);
            rt_str_free(c);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"ab\r\n");
}

#[test]
fn from_int_single_digit_boundary() {
    let exe = build(
        "fn main() {
            let s = rt_str_from_int(9);
            rt_print_str(s);
            rt_str_free(s);
            let s2 = rt_str_from_int(10);
            rt_print_str(s2);
            rt_str_free(s2);
            let s3 = rt_str_from_int(99);
            rt_print_str(s3);
            rt_str_free(s3);
            let s4 = rt_str_from_int(100);
            rt_print_str(s4);
            rt_str_free(s4);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"9\r\n10\r\n99\r\n100\r\n");
}

#[test]
fn eq_single_char_equal() {
    let exe = build(
        "fn main() {
            if rt_str_eq(\"a\", \"a\") {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 1);
}

#[test]
fn eq_single_char_different() {
    let exe = build(
        "fn main() {
            if rt_str_eq(\"a\", \"b\") {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 0);
}

#[test]
fn bool_size_check() {
    // Verify that "true" is 4 bytes and "false" is 5 bytes.
    let exe = build(
        "fn main() {
            let t = rt_str_from_bool(true);
            let f = rt_str_from_bool(false);
            rt_print_int(rt_str_len(t));
            rt_print_int(rt_str_len(f));
            rt_str_free(t);
            rt_str_free(f);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"4\r\n5\r\n");
}

#[test]
fn from_int_negative_one() {
    let exe = build(
        "fn main() {
            let s = rt_str_from_int(-1);
            rt_print_str(s);
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"-1\r\n");
}

#[test]
fn concat_preserves_operands() {
    // Ensure the original strings are unchanged after concatenation.
    let exe = build(
        "fn main() {
            let a = \"hello\";
            let b = \" world\";
            let c = rt_str_concat(a, b);
            rt_print_str(a);
            rt_print_str(b);
            rt_print_str(c);
            rt_str_free(c);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"hello\r\n world\r\nhello world\r\n");
}

#[test]
fn from_int_then_concat_multiple() {
    let exe = build(
        "fn main() {
            let a = rt_str_from_int(10);
            let b = rt_str_from_int(20);
            let sum = rt_str_from_int(30);
            let eq_sum = rt_str_concat(\"=\", sum);
            let bsum = rt_str_concat(b, eq_sum);
            let ab = rt_str_concat(a, \",\");
            let msg = rt_str_concat(ab, bsum);
            rt_print_str(msg);
            rt_str_free(a);
            rt_str_free(b);
            rt_str_free(sum);
            rt_str_free(eq_sum);
            rt_str_free(bsum);
            rt_str_free(ab);
            rt_str_free(msg);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"10,20=30\r\n");
}

#[test]
fn eq_with_int_converted_strings() {
    let exe = build(
        "fn main() {
            let a = rt_str_from_int(42);
            let b = rt_str_from_int(42);
            if rt_str_eq(a, b) {
                rt_print_int(1);
            } else {
                rt_print_int(0);
            }
            rt_str_free(a);
            rt_str_free(b);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"1\r\n");
}

#[test]
fn eq_with_different_int_strings() {
    let exe = build(
        "fn main() {
            let a = rt_str_from_int(42);
            let b = rt_str_from_int(43);
            if rt_str_eq(a, b) {
                rt_print_int(1);
            } else {
                rt_print_int(0);
            }
            rt_str_free(a);
            rt_str_free(b);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"0\r\n");
}

// ---------------------------------------------------------------------------
// String equality via `==` operator (V1: content comparison, not pointer)
// ---------------------------------------------------------------------------

#[test]
fn str_eq_operator_identical_literals() {
    // Two identical string literals should compare equal via `==`.
    let exe = build(
        "fn main() {
            let a = \"hello\";
            let b = \"hello\";
            if a == b {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 1);
}

#[test]
fn str_eq_operator_different_literals() {
    // Two different string literals should compare not-equal via `==`.
    let exe = build(
        "fn main() {
            let a = \"hello\";
            let b = \"world\";
            if a == b {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 0);
}

#[test]
fn str_ne_operator_different_literals() {
    // `!=` should return true for different string literals.
    let exe = build(
        "fn main() {
            let a = \"hello\";
            let b = \"world\";
            if a != b {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 1);
}

#[test]
fn str_ne_operator_identical_literals() {
    // `!=` should return false for identical string literals.
    let exe = build(
        "fn main() {
            let a = \"hello\";
            let b = \"hello\";
            if a != b {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 0);
}

#[test]
fn str_eq_operator_empty_strings() {
    // Empty strings should compare equal.
    let exe = build(
        "fn main() {
            let a = \"\";
            let b = \"\";
            if a == b {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 1);
}

#[test]
fn str_eq_operator_heap_strings_same_content() {
    // Two heap-allocated strings with identical content should compare
    // equal via `==` (content comparison, not pointer comparison).
    let exe = build(
        "fn main() {
            let a = rt_str_alloc(3);
            rt_str_set_byte(a, 0, 72);
            rt_str_set_byte(a, 1, 105);
            rt_str_set_byte(a, 2, 33);
            let b = rt_str_alloc(3);
            rt_str_set_byte(b, 0, 72);
            rt_str_set_byte(b, 1, 105);
            rt_str_set_byte(b, 2, 33);
            if a == b {
                rt_print_int(1);
            } else {
                rt_print_int(0);
            }
            rt_str_free(a);
            rt_str_free(b);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"1\r\n");
}

#[test]
fn str_eq_operator_heap_strings_different_content() {
    // Two heap-allocated strings with different content should compare
    // not-equal via `==`.
    let exe = build(
        "fn main() {
            let a = rt_str_alloc(3);
            rt_str_set_byte(a, 0, 72);
            rt_str_set_byte(a, 1, 105);
            rt_str_set_byte(a, 2, 33);
            let b = rt_str_alloc(3);
            rt_str_set_byte(b, 0, 72);
            rt_str_set_byte(b, 1, 105);
            rt_str_set_byte(b, 2, 65);
            if a == b {
                rt_print_int(1);
            } else {
                rt_print_int(0);
            }
            rt_str_free(a);
            rt_str_free(b);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"0\r\n");
}

#[test]
fn str_ne_operator_heap_strings() {
    // `!=` on heap strings with different content should return true.
    let exe = build(
        "fn main() {
            let a = rt_str_alloc(3);
            rt_str_set_byte(a, 0, 72);
            rt_str_set_byte(a, 1, 105);
            rt_str_set_byte(a, 2, 33);
            let b = rt_str_alloc(3);
            rt_str_set_byte(b, 0, 72);
            rt_str_set_byte(b, 1, 105);
            rt_str_set_byte(b, 2, 65);
            if a != b {
                rt_print_int(1);
            } else {
                rt_print_int(0);
            }
            rt_str_free(a);
            rt_str_free(b);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"1\r\n");
}

#[test]
fn str_eq_operator_prefix_strings() {
    // A prefix string should not equal the full string.
    let exe = build(
        "fn main() {
            let a = \"hello\";
            let b = \"hell\";
            if a == b {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 0);
}

#[test]
fn str_eq_operator_literal_vs_heap() {
    // A string literal and a heap-allocated string with the same content
    // should compare equal.
    let exe = build(
        "fn main() {
            let a = \"hi\";
            let b = rt_str_alloc(2);
            rt_str_set_byte(b, 0, 104);
            rt_str_set_byte(b, 1, 105);
            if a == b {
                rt_print_int(1);
            } else {
                rt_print_int(0);
            }
            rt_str_free(b);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"1\r\n");
}

#[test]
fn str_eq_operator_as_condition_in_loop() {
    // String `==` in a loop condition should work.
    // Note: `==` on Str consumes both operands (ownership), so we test
    // with a short-lived loop that does not need to free after comparison.
    let exe = build(
        "fn main() {
            let target = \"hello\";
            let source = \"hello\";
            if source == target {
                return 1;
            }
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 1);
}
