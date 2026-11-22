//! Tests for `mink fmt`: expected layouts, the promises (same tokens, same comments,
//! idempotent), and every `.mink` file in the repository.

use super::*;
use std::fs;
use std::path::{Path, PathBuf};

fn fmt(src: &str) -> String {
    format_source(src).unwrap_or_else(|e| panic!("format failed: {e}\n--- input ---\n{src}"))
}

/// Formats and checks the promises that must hold for any input.
fn check(src: &str, expected: &str) {
    let got = fmt(src);
    assert_eq!(got, expected, "\n--- input ---\n{src}\n--- got ---\n{got}\n--- expected ---\n{expected}");
    assert_eq!(fmt(&got), got, "formatting must be idempotent");
}

// ---------------------------------------------------------------- layout

#[test]
fn already_canonical_source_is_unchanged() {
    let src = "fn main() -> Int {\n    let x = 1 + 2;\n    return x;\n}\n";
    check(src, src);
}

#[test]
fn indentation_is_four_spaces_per_level() {
    check(
        "fn main() {\nif true {\nlet x = 1;\n  if x == 1 {\n\t\trt_print_int(x);\n}\n}\n}\n",
        "fn main() {\n    if true {\n        let x = 1;\n        if x == 1 {\n            rt_print_int(x);\n        }\n    }\n}\n",
    );
}

#[test]
fn spacing_around_operators_commas_and_colons() {
    check(
        "fn add(a:Int,b:Int)->Int{return a+b*2;}\n",
        "fn add(a: Int, b: Int) -> Int { return a + b * 2; }\n",
    );
    check("fn f() { let x=( 1 + 2 ) ; }\n", "fn f() { let x = (1 + 2); }\n");
}

#[test]
fn one_line_blocks_keep_one_space_inside_the_braces() {
    check("fn f() { return 1; }\n", "fn f() { return 1; }\n");
    check("fn f() {return 1;}\n", "fn f() { return 1; }\n");
    check("fn f() { let a = 1; let b = 2; }\n", "fn f() {\n    let a = 1;\n    let b = 2;\n}\n");
    check("fn f() {}\n", "fn f() {}\n");
    check("fn f() {   }\n", "fn f() {}\n");
    check(
        "fn main() { let p = Point{x:1,y:2}; }\n",
        "fn main() {\n    let p = Point { x: 1, y: 2 };\n}\n",
    );
}

#[test]
fn unary_and_binary_minus_star_and_amp() {
    check(
        "fn f(a:Int,p:Ptr) { let x = -1; let y = a - -1; let z = a-b; let r = &a; let d = *p; let m = &mut a; let n = !x; }\n",
        "fn f(a: Int, p: Ptr) {\n    let x = -1;\n    let y = a - -1;\n    let z = a - b;\n    let r = &a;\n    let d = *p;\n    let m = &mut a;\n    let n = !x;\n}\n",
    );
    check("fn f() { return -x; }\n", "fn f() { return -x; }\n");
    check("fn f() { g(-1, - 2); }\n", "fn f() { g(-1, -2); }\n");
}

#[test]
fn calls_indexing_members_ranges_and_paths() {
    check(
        "fn f() { g ( a , b ) ; v [ 0 ] = x . y ; for i in 0 .. n { } let e = Color :: Red ; let q = h ( ) ? ; }\n",
        "fn f() {\n    g(a, b);\n    v[0] = x.y;\n    for i in 0..n {}\n    let e = Color::Red;\n    let q = h()?;\n}\n",
    );
}

#[test]
fn keywords_keep_a_space_before_parentheses() {
    check(
        "fn f(x:Int) { if(x > 1){return(x);} while(x < 3){x+=1;} }\n",
        "fn f(x: Int) {\n    if (x > 1) { return (x); }\n    while (x < 3) { x += 1; }\n}\n",
    );
}

#[test]
fn braces_stay_with_their_headers_and_else_with_its_brace() {
    check(
        "fn f(x:Int)\n{\nif x > 1\n{\nreturn 1;\n}\nelse\n{\nreturn 2;\n}\n}\n",
        "fn f(x: Int) {\n    if x > 1 {\n        return 1;\n    } else {\n        return 2;\n    }\n}\n",
    );
    check(
        "fn f(x:Int) {\n    if x > 1 { return 1; }\n    else if x > 0 { return 2; }\n    else { return 3; }\n}\n",
        "fn f(x: Int) {\n    if x > 1 { return 1; }\n    else if x > 0 { return 2; }\n    else { return 3; }\n}\n",
    );
}

#[test]
fn a_block_on_its_own_line_after_a_statement_is_left_alone() {
    check(
        "fn f() {\n    let x = 1;\n    {\n        let y = 2;\n    }\n}\n",
        "fn f() {\n    let x = 1;\n    {\n        let y = 2;\n    }\n}\n",
    );
}

#[test]
fn blank_lines_are_limited_and_trimmed() {
    check(
        "fn a() {}\n\n\n\n\nfn b() {\n\n\n    let x = 1;\n\n\n\n    let y = 2;\n\n}\n",
        "fn a() {}\n\nfn b() {\n    let x = 1;\n\n    let y = 2;\n}\n",
    );
}

#[test]
fn top_level_items_get_one_blank_line_between_them() {
    check("fn a() {}\nfn b() {}\nstruct S { x: Int }\n", "fn a() {}\n\nfn b() {}\n\nstruct S { x: Int }\n");
    check("fn a() {} fn b() {}\n", "fn a() {}\n\nfn b() {}\n");
}

#[test]
fn imports_and_constants_stay_grouped() {
    check("mod json;\nmod math;\nuse math::math_abs;\nlet LIMIT = 10;\nfn main() {}\n", "mod json;\nmod math;\nuse math::math_abs;\nlet LIMIT = 10;\nfn main() {}\n");
}

#[test]
fn trailing_whitespace_and_final_newlines() {
    check("fn f() {   \n    let x = 1;   \n}   \n\n\n\n", "fn f() {\n    let x = 1;\n}\n");
    check("fn f() {}", "fn f() {}\n");
    check("", "");
    check("\n\n\n", "");
}

#[test]
fn crlf_files_stay_crlf_and_a_bom_is_kept() {
    let out = fmt("fn f() {\r\nlet x = 1;\r\n}\r\n");
    assert_eq!(out, "fn f() {\r\n    let x = 1;\r\n}\r\n");
    let out = fmt("\u{feff}fn f() {}\n");
    assert_eq!(out, "\u{feff}fn f() {}\n");
}

// -------------------------------------------------------------- continuation

#[test]
fn continuation_lines_are_indented_one_level() {
    check(
        "fn f() {\nlet total = alpha +\nbeta +\ngamma;\nlet s = items\n.first()\n.name;\nif a &&\nb {\nreturn 1;\n}\n}\n",
        "fn f() {\n    let total = alpha +\n        beta +\n        gamma;\n    let s = items\n        .first()\n        .name;\n    if a &&\n        b {\n        return 1;\n    }\n}\n",
    );
}

#[test]
fn lines_inside_open_brackets_hang_from_the_line_that_opened_them() {
    check(
        "fn f() {\nlet v = g(\na,\nb\n);\nlet w = [\n1,\n2,\n];\nh(first, other(\nx,\ny\n));\n}\n",
        "fn f() {\n    let v = g(\n        a,\n        b\n    );\n    let w = [\n        1,\n        2,\n    ];\n    h(first, other(\n        x,\n        y\n    ));\n}\n",
    );
}

#[test]
fn closures_with_blocks_keep_the_body_one_level_in() {
    check(
        "fn f() {\nrun(|x| {\nreturn x + 1;\n});\n}\n",
        "fn f() {\n    run(|x| {\n        return x + 1;\n    });\n}\n",
    );
}

#[test]
fn match_arms_are_indented_inside_the_match() {
    check(
        "fn f(c:Cmd)->Int {\nreturn match c {\nCmd { kind: 0, count } => count + 1,\nCmd { kind, count } => kind *\n1000 + count,\n};\n}\n",
        "fn f(c: Cmd) -> Int {\n    return match c {\n        Cmd { kind: 0, count } => count + 1,\n        Cmd { kind, count } => kind *\n            1000 + count,\n    };\n}\n",
    );
}

// ------------------------------------------------------------------ comments

#[test]
fn comments_keep_their_text_and_move_to_the_right_indent() {
    check(
        "// header\n\n// second\nfn f() {\n// inside\nlet x = 1; // trailing\n   // before closer\n}\n",
        "// header\n\n// second\nfn f() {\n    // inside\n    let x = 1; // trailing\n    // before closer\n}\n",
    );
}

#[test]
fn a_trailing_comment_is_one_space_from_the_code() {
    check("fn f() {\n    let x = 1;      // note\n}\n", "fn f() {\n    let x = 1; // note\n}\n");
}

#[test]
fn block_comments_are_kept_whole() {
    check(
        "/* top\n   still top */\nfn f() { /* inline */ let x = 1; }\n",
        "/* top\n   still top */\nfn f() { /* inline */\n    let x = 1;\n}\n",
    );
}

#[test]
fn a_comment_between_an_item_and_the_next_gets_its_blank_line() {
    check(
        "fn a() {}\n// about b\nfn b() {}\n",
        "fn a() {}\n\n// about b\nfn b() {}\n",
    );
}

#[test]
fn comment_text_with_odd_characters_is_untouched() {
    check("// ünïcode ✓ \t tabs  and   spaces\nfn f() {}\n", "// ünïcode ✓ \t tabs  and   spaces\nfn f() {}\n");
}

// --------------------------------------------------- spacing the tokens cannot decide

#[test]
fn angle_brackets_and_bars_keep_the_authors_spacing() {
    check("fn id<T>(x: T) -> T {\n    return x;\n}\n", "fn id<T>(x: T) -> T {\n    return x;\n}\n");
    check("fn f(a:Int,b:Int) {\n    let c = a < b;\n    let d = a<b;\n    let g = |x| x;\n}\n", "fn f(a: Int, b: Int) {\n    let c = a < b;\n    let d = a<b;\n    let g = |x| x;\n}\n");
}

#[test]
fn tokens_are_never_merged() {
    // `a & &b` and `a = = b` would lex differently without their spaces.
    let out = fmt("fn f() { let x = a & &b; }\n");
    assert!(out.contains("a & &b"), "{out}");
}

// ----------------------------------------------------------------- strings

#[test]
fn string_and_char_literals_are_copied_exactly() {
    let src = "fn main() {\n    rt_print_str(\"  two  spaces,  and \\\"quotes\\\"  // not a comment\\n\");\n    let c = ' ';\n}\n";
    check(src, src);
}

// ----------------------------------------------------------------- failures

#[test]
fn lexical_errors_are_refused_with_a_line_number() {
    let err = format_source("fn f() {\n    let s = \"never closed;\n}\n").unwrap_err();
    match err {
        FormatError::Lex { line, .. } => assert_eq!(line, 2),
        other => panic!("expected a lex error, got {other:?}"),
    }
    assert!(format_source("fn f() { /* open\n").is_err());
}

#[test]
fn unbalanced_braces_still_format_without_panicking() {
    let _ = fmt("fn f() {\n}\n}\n}\n");
    let _ = fmt("fn f() {\n{{{\n");
    let _ = fmt(")))(((]][[");
}

// --------------------------------------------------------------- the corpus

fn corpus() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if !(name.starts_with('.') || ["target", "node_modules", "dist", "tmp", "soak_work"].contains(&name.as_str())) {
                    walk(&path, out);
                }
            } else if path.extension().is_some_and(|e| e == "mink") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(Path::new("."), &mut files);
    files.sort();
    files
}

#[test]
fn every_mink_file_in_the_repository_keeps_its_tokens_and_formats_idempotently() {
    let files = corpus();
    assert!(files.len() > 100, "the corpus should be large, found {}", files.len());
    let (mut formatted, mut refused) = (0, Vec::new());
    for file in &files {
        let Ok(source) = fs::read_to_string(file) else { continue };
        match format_source(&source) {
            Ok(once) => {
                formatted += 1;
                let twice = format_source(&once)
                    .unwrap_or_else(|e| panic!("{}: the result could not be formatted again: {e}", file.display()));
                assert_eq!(twice, once, "{}: formatting twice must equal formatting once", file.display());
            }
            Err(FormatError::Lex { .. }) => refused.push(file.display().to_string()),
            Err(error) => panic!("{}: {error}", file.display()),
        }
    }
    // Files that do not lex are deliberate fixtures for the compiler's own error tests.
    assert!(formatted > files.len() * 9 / 10, "only {formatted} of {} files formatted; refused: {refused:?}", files.len());
}

#[test]
fn the_formatted_corpus_still_checks_the_same() {
    // Stronger than tokens alone: every file that compiled cleanly before still does, with the same
    // number of diagnostics, after formatting.
    let mut checked = 0;
    for file in corpus().iter().filter(|p| p.starts_with("./stdlib") || p.starts_with("./examples")) {
        let Ok(source) = fs::read_to_string(file) else { continue };
        let Ok(formatted) = format_source(&source) else { continue };
        let before = diagnostics_of(&source);
        let after = diagnostics_of(&formatted);
        assert_eq!(before, after, "{}: formatting changed the diagnostics", file.display());
        checked += 1;
    }
    assert!(checked > 20, "only {checked} files were compared");
}

/// The number of diagnostics the single-file front end reports for `source`.
fn diagnostics_of(source: &str) -> usize {
    let dir = std::env::temp_dir().join(format!("mink_fmt_check_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("x.mink");
    fs::write(&path, source).unwrap();
    let mut sources = crate::source::SourceMap::new();
    let count = match crate::driver::check(&mut sources, &path) {
        Ok(report) => report.errors.len(),
        Err(crate::driver::BuildError::FrontEnd(report)) => report.errors.len(),
        Err(_) => usize::MAX,
    };
    let _ = fs::remove_file(&path);
    count
}

#[test]
fn a_short_single_statement_block_keeps_its_line_but_a_long_or_busy_one_does_not() {
    // Guard clauses stay compact.
    check(
        "fn f(n:Int)->Int {\n    if n < 0 { return 0; }\n    return n;\n}\n",
        "fn f(n: Int) -> Int {\n    if n < 0 { return 0; }\n    return n;\n}\n",
    );
    // A block with a comment, a nested block or two statements is written out.
    check(
        "fn f(n:Int) {\n    if n < 0 { /* x */ return; }\n}\n",
        "fn f(n: Int) {\n    if n < 0 { /* x */\n        return;\n    }\n}\n",
    );
    let long = format!("fn f() {{ let a_very_long_name = {}; }}\n", "1 + ".repeat(40) + "1");
    let out = fmt(&long);
    assert!(out.contains("{\n    let a_very_long_name"), "{out}");
}

#[test]
fn header_comments_keep_the_blank_line_before_the_first_item() {
    check("// header\n\nfn f() {}\n", "// header\n\nfn f() {}\n");
    check("// header\nfn f() {}\n", "// header\nfn f() {}\n");
}
