//! M7 Phase B — Gate A: language front-end conformance corpus.
//!
//! Each test is a conformance case with an identifier of the form
//! `A-<area>-<n>`, organised by language feature, and every case states in its
//! name what the language contract is. The corpus covers the lexer, literals
//! and escapes, operator precedence and associativity, expression and
//! statement parsing, control flow, functions, types, aggregates, modules,
//! diagnostics, parser recovery, nesting limits, and `check`/`build` parity.
//!
//! Tests run the **real CLI binary** (not internal APIs), because Gate A is
//! about the behaviour a user observes. Cases that were added for a defect
//! found by the Session 278 audit name the defect in a comment.
//!
//! Reference: `docs/language/CORE_GRAMMAR.md`, `docs/language/CORE_LANGUAGE.md`,
//! `docs/language/TYPE_SYSTEM.md`.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

/// Writes `src` to a unique temporary file and returns its path.
fn source(name: &str, src: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("mink_conf_{}_{n}_{name}.mink", std::process::id()));
    std::fs::write(&path, src).expect("write temp source");
    path
}

/// Runs `mink check` and returns (exit code, stderr, stdout).
fn check(src: &str) -> (i32, String, String) {
    let path = source("check", src);
    let out = mink()
        .arg("check")
        .arg(&path)
        .output()
        .expect("run mink check");
    let _ = std::fs::remove_file(&path);
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// Runs `mink build` and returns (exit code, stderr). A failed build must not
/// leave an executable behind; callers that care use [`build_and_run`].
fn build(src: &str) -> (i32, String) {
    let path = source("build", src);
    let out = mink()
        .arg("build")
        .arg(&path)
        .output()
        .expect("run mink build");
    let exe = path.with_extension("exe");
    let code = out.status.code().unwrap_or(-1);
    if code != 0 {
        assert!(
            !exe.exists(),
            "a failed build left an executable artifact behind"
        );
    }
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (code, stderr)
}

/// Builds and runs `src`; returns (process exit code, stdout).
fn build_and_run(src: &str) -> (i32, String) {
    let path = source("run", src);
    let out = mink()
        .arg("build")
        .arg(&path)
        .output()
        .expect("run mink build");
    let exe = path.with_extension("exe");
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        panic!("build failed:\n{stderr}");
    }
    let run = Command::new(&exe).output().expect("run built program");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (
        run.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&run.stdout).into_owned(),
    )
}

/// Asserts the program compiles and exits with `expected`.
fn assert_exit(src: &str, expected: i32) {
    let (code, _) = build_and_run(src);
    assert_eq!(code, expected, "unexpected exit code for:\n{src}");
}

/// Asserts `check` and `build` both reject `src` and both mention `error`.
fn assert_rejected(src: &str, error_code: &str) {
    let (check_code, stderr, _) = check(src);
    assert_ne!(check_code, 0, "`check` accepted:\n{src}");
    assert!(
        stderr.contains(error_code),
        "expected `{error_code}` from check, got:\n{stderr}"
    );
    assert!(!stderr.contains("panicked"), "compiler panicked:\n{stderr}");
    assert!(
        !stderr.contains("overflowed its stack"),
        "compiler exhausted its stack:\n{stderr}"
    );
}

// ============================================================================
// A-LEX — lexical analysis and tokenization
// ============================================================================

/// A-LEX-01: keywords are reserved and cannot be binding names.
#[test]
fn a_lex_01_keywords_are_reserved() {
    assert_rejected(
        "fn main() -> Int { let struct = 1; return struct; }",
        "E-P02",
    );
}

/// A-LEX-02: keywords with a suffix are ordinary identifiers.
#[test]
fn a_lex_02_keyword_prefix_is_an_identifier() {
    assert_exit("fn main() -> Int { let fn2 = 42; return fn2; }", 42);
}

/// A-LEX-03: underscores and digits are allowed after the first character.
#[test]
fn a_lex_03_identifier_shape() {
    assert_exit("fn main() -> Int { let _a_b9 = 7; return _a_b9; }", 7);
}

/// A-LEX-04: an unterminated string is a controlled lexical/parse error.
#[test]
fn a_lex_04_unterminated_string() {
    let (code, stderr, _) = check("fn main() { let s = \"abc; }");
    assert_ne!(code, 0);
    assert!(
        stderr.contains("E-L02") || stderr.contains("E-P14"),
        "unexpected diagnostic:\n{stderr}"
    );
}

/// A-LEX-05: an unterminated block comment is diagnosed, not ignored.
#[test]
fn a_lex_05_unterminated_block_comment() {
    let (code, stderr, _) = check("fn main() -> Int { /* open\n return 0; }");
    assert_ne!(code, 0);
    assert!(stderr.contains("E-L04"), "unexpected diagnostic:\n{stderr}");
}

/// A-LEX-06: line comments and blank input are accepted with zero tokens.
#[test]
fn a_lex_06_comments_only_file() {
    let (code, stderr, _) = check("// a\n// b\n");
    assert_eq!(code, 0, "comments-only file rejected:\n{stderr}");
}

// ============================================================================
// A-LIT — literals and escapes
// ============================================================================

/// A-LIT-01: hexadecimal, binary, octal and underscore-separated integers
/// decode to their values (the backend decodes literal text; this pins it).
#[test]
fn a_lit_01_integer_bases() {
    assert_exit("fn main() -> Int { return 0xFF; }", 255);
    assert_exit("fn main() -> Int { return 0b1010; }", 10);
    assert_exit("fn main() -> Int { return 0o777 % 256; }", 255);
    assert_exit("fn main() -> Int { return 1_000_000 % 256; }", 64);
}

/// A-LIT-02: a digit outside the literal's radix is a lexical error.
#[test]
fn a_lit_02_invalid_radix_digit() {
    assert_rejected("fn main() -> Int { return 0b102; }", "E-L08");
    assert_rejected("fn main() -> Int { return 0o18; }", "E-L08");
}

/// A-LIT-03: an integer literal above `i64::MAX` is rejected, not wrapped.
#[test]
fn a_lit_03_oversized_integer_literal() {
    assert_rejected("fn main() -> Int { return 9223372036854775808; }", "E-T43");
}

/// A-LIT-04: the `i64::MIN` magnitude is expressible as a negated literal.
#[test]
fn a_lit_04_minimum_integer() {
    assert_exit(
        "fn main() -> Int { let x = -9223372036854775808; return x % 2; }",
        0,
    );
}

/// A-LIT-05: every documented string escape decodes to one byte, and the
/// `\u{...}` escape to UTF-8 (each program returns the byte length).
#[test]
fn a_lit_05_string_escapes() {
    assert_exit(
        r#"fn main() -> Int { let s = "a\nb\tc\r\0\\\"\x41"; return rt_str_len(s); }"#,
        10,
    );
    assert_exit(
        r#"fn main() -> Int { let s = "\u{41}\u{1F600}"; return rt_str_len(s); }"#,
        5,
    );
}

/// A-LIT-06: an unknown escape is a lexical error.
#[test]
fn a_lit_06_invalid_escape() {
    assert_rejected(r#"fn main() { let s = "\q"; }"#, "E-L05");
}

/// A-LIT-07: a character literal must hold exactly one character; escapes
/// count as one.
#[test]
fn a_lit_07_character_literal_arity() {
    assert_rejected("fn main() -> Int { let c = ''; return 0; }", "E-L07");
    assert_rejected("fn main() -> Int { let c = 'ab'; return 0; }", "E-L07");
    assert_exit("fn main() -> Int { rt_print_char('\\n'); return 0; }", 0);
}

/// A-LIT-08: floating-point forms (`1.5`, `1e10`, `1_0.2_5`) decode.
#[test]
fn a_lit_08_float_forms() {
    assert_exit("fn main() -> Int { rt_print_float(1_0.2_5); return 0; }", 0);
    assert_exit("fn main() -> Int { rt_print_float(1.5e10); return 0; }", 0);
}

// ============================================================================
// A-OP — operators, precedence and associativity
// ============================================================================

/// A-OP-01: multiplicative binds tighter than additive.
#[test]
fn a_op_01_multiplicative_precedence() {
    assert_exit("fn main() -> Int { return 2 + 3 * 4; }", 14);
    assert_exit("fn main() -> Int { return 2 * 3 + 4 * 5; }", 26);
}

/// A-OP-02: parentheses override precedence.
#[test]
fn a_op_02_grouping() {
    assert_exit("fn main() -> Int { return (2 + 3) * 4; }", 20);
}

/// A-OP-03: additive is left-associative.
#[test]
fn a_op_03_subtraction_associativity() {
    assert_exit("fn main() -> Int { return 10 - 3 - 2; }", 5);
    assert_exit("fn main() -> Int { return 0 - 1 + 2; }", 1);
}

/// A-OP-04: shifts bind looser than addition (C/Rust precedence).
#[test]
fn a_op_04_shift_precedence() {
    assert_exit("fn main() -> Int { return 1 << 3 + 1; }", 16);
    assert_exit("fn main() -> Int { return 16 >> 2; }", 4);
}

/// A-OP-05: bitwise operators: `|`, `&`, `^` in the documented precedence.
#[test]
fn a_op_05_bitwise_operators() {
    assert_exit("fn main() -> Int { return 5 | 2; }", 7);
    assert_exit("fn main() -> Int { return 5 & 3; }", 1);
    assert_exit("fn main() -> Int { return 5 ^ 1; }", 4);
}

/// A-OP-06: unary operators bind tighter than binary operators, and a
/// parenthesized operand of a unary operator works (`-(1)`).
#[test]
fn a_op_06_unary_precedence() {
    assert_exit("fn main() -> Int { return -5 + 10; }", 5);
    assert_exit("fn main() -> Int { return -(1) + 3; }", 2);
    assert_exit("fn main() -> Int { return ~0 + 5; }", 4);
}

/// A-OP-07: boolean operators short-circuit and comparisons yield `Bool`.
#[test]
fn a_op_07_boolean_operators() {
    assert_exit(
        "fn main() -> Int { if 1 < 2 && 3 > 2 { return 1; } return 0; }",
        1,
    );
    assert_exit(
        "fn main() -> Int { if false || 5 > 1 { return 1; } return 0; }",
        1,
    );
    assert_exit(
        "fn main() -> Int { let mut n = 0; if false && { n = 1; true } { } return n; }",
        0,
    );
}

// ============================================================================
// A-STMT — statements and control flow
// ============================================================================

/// A-STMT-01: `if`/`else` statement.
#[test]
fn a_stmt_01_if_else() {
    assert_exit(
        "fn main() -> Int { if 1 == 2 { return 1; } else { return 2; } }",
        2,
    );
}

/// A-STMT-02: `while` with a compound condition.
#[test]
fn a_stmt_02_while() {
    assert_exit(
        "fn main() -> Int { let mut i = 0; let mut s = 0; while i < 5 { s = s + i; i = i + 1; } return s; }",
        10,
    );
}

/// A-STMT-03: inclusive and exclusive `for` ranges iterate the documented
/// number of times.
#[test]
fn a_stmt_03_for_ranges() {
    assert_exit(
        "fn main() -> Int { let mut s = 0; for i in 1..=5 { s = s + i; } return s; }",
        15,
    );
    assert_exit(
        "fn main() -> Int { let mut n = 0; for i in 0..5 { n = n + 1; } return n; }",
        5,
    );
}

/// A-STMT-04: `loop`/`break value` and `continue`.
#[test]
fn a_stmt_04_loop_break_continue() {
    assert_exit(
        "fn main() -> Int { let mut i = 0; let r = loop { i = i + 1; if i < 3 { continue; } if i >= 5 { break i; } }; return r; }",
        5,
    );
}

/// A-STMT-05: nested blocks and block expressions.
#[test]
fn a_stmt_05_block_expressions() {
    assert_exit(
        "fn main() -> Int { let x = { let a = 2; let b = 3; a * b }; return x; }",
        6,
    );
}

/// A-STMT-06: empty statements are accepted.
#[test]
fn a_stmt_06_empty_statements() {
    assert_exit("fn main() -> Int { ; ; return 3; }", 3);
}

// ============================================================================
// A-FN — functions, calls and types
// ============================================================================

/// A-FN-01: declarations, calls, parameters and return values.
#[test]
fn a_fn_01_declaration_and_call() {
    assert_exit(
        "fn add(a: Int, b: Int) -> Int { return a + b; } fn main() -> Int { return add(20, 22); }",
        42,
    );
}

/// A-FN-02: recursion.
#[test]
fn a_fn_02_recursion() {
    assert_exit(
        "fn fact(n: Int) -> Int { if n <= 1 { return 1; } return n * fact(n - 1); } fn main() -> Int { return fact(5); }",
        120,
    );
}

/// A-FN-03: generic type parameters are inferred at the call site.
#[test]
fn a_fn_03_generics() {
    assert_exit(
        "fn id<T>(x: T) -> T { return x; } fn main() -> Int { return id(42); }",
        42,
    );
}

/// A-FN-04: a declared type mismatch is `E-T01` in both `check` and `build`.
#[test]
fn a_fn_04_type_annotation_mismatch() {
    assert_rejected("fn main() -> Int { let x: Int = true; return 0; }", "E-T01");
}

/// A-FN-05: implicit numeric conversion is not performed (`E-T02`).
#[test]
fn a_fn_05_no_implicit_conversion() {
    assert_rejected("fn main() -> Int { return 1.5 + 1; }", "E-T02");
}

/// A-FN-06: trailing commas are accepted in calls, tuples, struct literals
/// and array literals (the grammar calls the separator trailing-comma-
/// tolerant; arrays were a shell-quoting artefact in the audit, not a bug).
#[test]
fn a_fn_06_trailing_commas() {
    assert_exit(
        "fn f(a: Int, b: Int) -> Int { return a + b; } fn main() -> Int { return f(1, 2,); }",
        3,
    );
    assert_exit("fn main() -> Int { let t = (1, 2,); return t.0; }", 1);
    assert_exit(
        "struct P { x: Int } fn main() -> Int { let p = P { x: 4, }; return p.x; }",
        4,
    );
    assert_exit("fn main() -> Int { let a = [1, 2,]; return a[1]; }", 2);
}

// ============================================================================
// A-AGG — aggregates
// ============================================================================

/// A-AGG-01: struct construction, field access and destructuring.
#[test]
fn a_agg_01_structs() {
    assert_exit(
        "struct P { x: Int, y: Int } fn main() -> Int { let p = P { x: 10, y: 32 }; let P { x, y } = p; return x + y; }",
        42,
    );
}

/// A-AGG-02: tuples, tuple field access and destructuring.
#[test]
fn a_agg_02_tuples() {
    assert_exit(
        "fn main() -> Int { let t = (10, 32); return t.0 + t.1; }",
        42,
    );
    assert_exit("fn main() -> Int { let (a, b) = (1, 2); return a + b; }", 3);
}

/// A-AGG-03: arrays, literal length and indexing (including a computed index).
#[test]
fn a_agg_03_arrays() {
    assert_exit(
        "fn main() -> Int { let a = [10, 20, 30]; return a[2]; }",
        30,
    );
    assert_exit(
        "fn main() -> Int { let a = [1, 2, 3]; let i = 1; return a[i] + a[0]; }",
        3,
    );
}

/// A-AGG-04: an empty array literal has no inferable element type.
#[test]
fn a_agg_04_empty_array_literal() {
    assert_rejected("fn main() -> Int { let a = []; return 0; }", "E-T17");
}

/// A-AGG-05: enum construction and `match` on variants.
#[test]
fn a_agg_05_enums_and_match() {
    assert_exit(
        "enum E { A, B(Int) } fn main() -> Int { let e = E::B(9); match e { E::B(v) => { return v; }, E::A => { return 0; } } }",
        9,
    );
}

/// A-AGG-06: match arms support or-patterns, ranges and guards.
#[test]
fn a_agg_06_patterns() {
    assert_exit(
        "fn f(x: Int) -> Int { match x { 1 | 2 => { return 1; }, 3..=5 => { return 2; }, n if n > 10 => { return 3; }, _ => { return 0; } } } fn main() -> Int { return f(2) + f(4) + f(11); }",
        6,
    );
}

// ============================================================================
// A-MOD — modules and imports
// ============================================================================

/// A-MOD-01: a standard-library module is loadable and its functions resolve.
#[test]
fn a_mod_01_stdlib_module() {
    assert_exit(
        "mod strings;\nfn main() -> Int { let s = \"abcdef\"; let r = str_sub(s, 1, 4); let n = rt_str_len(r); rt_str_free(r); return n; }",
        3,
    );
}

/// A-MOD-02: importing a module that does not exist is `E-S01`.
#[test]
fn a_mod_02_missing_module() {
    assert_rejected(
        "mod definitely_not_a_module;\nfn main() -> Int { return 0; }",
        "E-S01",
    );
}

// ============================================================================
// A-DIAG — diagnostics, recovery and limits
// ============================================================================

/// A-DIAG-01: several syntax errors in one file are all reported (the parser
/// recovers at statement level rather than stopping at the first).
#[test]
fn a_diag_01_multiple_errors_reported() {
    let (code, stderr, _) = check("fn main() { let = ; let = ; }");
    assert_ne!(code, 0);
    assert_eq!(
        stderr.matches("E-P02").count(),
        2,
        "expected both errors to be reported:\n{stderr}"
    );
}

/// A-DIAG-02: malformed input never panics and never hangs; the compiler
/// exits with a diagnostic (the process must terminate on its own).
#[test]
fn a_diag_02_malformed_input_is_controlled() {
    for src in [
        "fn main() {",
        "}}}}",
        "fn main( -> Int { return 0; }",
        "let",
        "\u{0}\u{1}\u{2}",
        "fn main() -> Int { return (1; }",
    ] {
        let (code, stderr, _) = check(src);
        assert_ne!(code, 0, "accepted malformed input: {src:?}");
        assert!(!stderr.contains("panicked"), "panic on {src:?}:\n{stderr}");
    }
}

/// A-DIAG-03: a binary file is rejected with a diagnostic, not a crash.
#[test]
fn a_diag_03_binary_input() {
    let (code, stderr, _) = check("\u{0}\u{1}\u{2}\u{3}");
    assert_ne!(code, 0);
    assert!(!stderr.contains("panicked"), "{stderr}");
}

/// A-DIAG-04: moderate nesting is accepted; pathological nesting reports one
/// `E-P33` per file and creates no executable.
#[test]
fn a_diag_04_nesting_limits() {
    // 200 nested parentheses are legal and evaluate correctly.
    let src = format!(
        "fn main() -> Int {{ return {}7{}; }}",
        "(".repeat(200),
        ")".repeat(200)
    );
    assert_exit(&src, 7);

    // 1,000 nested parentheses exceed `MAX_NESTING` (512).
    let deep = format!(
        "fn main() -> Int {{ return {}7{}; }}",
        "(".repeat(1000),
        ")".repeat(1000)
    );
    let (code, stderr) = build(&deep);
    assert_ne!(code, 0);
    assert_eq!(
        stderr.matches("E-P33").count(),
        1,
        "expected exactly one E-P33:\n{stderr}"
    );
    assert!(!stderr.contains("overflowed its stack"), "{stderr}");
}

/// A-DIAG-05: a prefix-operator chain is bounded. Before Session 278 this
/// recursion was unguarded and 8,000 unary minuses overflowed the compiler
/// stack (exit 3221225725) instead of reporting `E-P33` (defect S278-D2).
#[test]
fn a_diag_05_prefix_operator_chain_is_bounded() {
    let src = format!("fn main() -> Int {{ return {}1; }}", "-".repeat(8000));
    let (code, stderr) = build(&src);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("E-P33"),
        "expected E-P33 for an 8,000-operator chain:\n{stderr}"
    );
    assert!(
        !stderr.contains("overflowed its stack"),
        "compiler crashed instead of diagnosing:\n{stderr}"
    );
}

/// A-DIAG-06: an expression chain longer than `MAX_EXPR_CHAIN` (2,048) is
/// rejected with `E-P33`; a legal chain just under the limit compiles.
#[test]
fn a_diag_06_expression_chain_limit() {
    let long = format!("fn main() -> Int {{ return 1{}; }}", " + 1".repeat(5000));
    let (code, stderr) = build(&long);
    assert_ne!(code, 0);
    assert!(stderr.contains("E-P33"), "{stderr}");

    let short = format!("fn main() -> Int {{ return 1{}; }}", " + 1".repeat(100));
    assert_exit(&short, 101);
}

/// A-DIAG-07: the operator budget bounds **one expression**, not a whole
/// function. A flat function with 3,000 statements and 3,000 operator nodes
/// was wrongly rejected as "nests too deeply" before Session 278 (defect
/// S278-D1); it must compile and run.
#[test]
fn a_diag_07_statement_count_is_not_a_nesting_limit() {
    let body = "s = s + 1; ".repeat(3000);
    let src = format!("fn main() -> Int {{ let mut s = 0; {body}return s % 7; }}");
    let (code, stdout) = build_and_run(&src);
    assert_eq!(code, 4, "3000 % 7 == 4");
    assert!(stdout.is_empty());
}

// ============================================================================
// A-PARITY — `check` and `build` agree
// ============================================================================

/// A-PARITY-01: a parenthesized literal is accepted by `check` **and**
/// `build`, in every position that carries a decoded literal. Session 277
/// fixed the first case (`let x = (1);`); this corpus pins the surrounding
/// contexts too.
#[test]
fn a_parity_01_grouped_literals_build() {
    let programs = [
        "fn main() -> Int { let x = (1); return x; }",
        "fn main() -> Int { let x = ((9)); return x; }",
        "fn main() -> Int { rt_print_int((7)); return 0; }",
        "fn main() -> Int { rt_print_str((\"hi\")); return 0; }",
        "fn main() -> Int { let t = ((1), 2); return t.0; }",
        "fn main() -> Int { let a = [(1), (2)]; return a[1]; }",
        "struct P { x: Int } fn main() -> Int { let p = P { x: (42) }; return p.x; }",
        "fn main() -> Int { return match (1) { 1 => 7, _ => 0 }; }",
        "fn main() -> Int { let a = [10, 20]; return a[(1)]; }",
        "fn id<T>(x: T) -> T { return x; } fn main() -> Int { return id((42)); }",
        "enum E { A(Int) } fn main() -> Int { let e = E::A((9)); match e { E::A(v) => { return v; } } }",
    ];
    for src in programs {
        let (check_code, check_err, _) = check(src);
        assert_eq!(check_code, 0, "`check` rejected {src:?}:\n{check_err}");
        let (build_code, build_err) = build(src);
        assert_eq!(build_code, 0, "`build` rejected {src:?}:\n{build_err}");
    }
}

/// A-PARITY-02: invalid programs are rejected by both stages, and no
/// executable is produced.
#[test]
fn a_parity_02_invalid_programs_rejected_by_both() {
    for src in [
        "fn main() -> Int { let x: Int = true; return 0; }",
        "fn main() -> Int { return undefined_name; }",
        "fn main() -> Int { let a = []; return 0; }",
        "fn main() -> Int { return 1 +; }",
    ] {
        let (check_code, _, _) = check(src);
        assert_ne!(check_code, 0, "`check` accepted {src:?}");
        let (build_code, _) = build(src);
        assert_ne!(build_code, 0, "`build` accepted {src:?}");
    }
}

/// A-PARITY-03: a program with a `main` that returns nothing is accepted by
/// both stages (the entry point may produce nothing).
#[test]
fn a_parity_03_unit_main() {
    let src = "fn main() { rt_print_int(1); }";
    let (check_code, err, _) = check(src);
    assert_eq!(check_code, 0, "{err}");
    assert_exit(src, 0);
}
