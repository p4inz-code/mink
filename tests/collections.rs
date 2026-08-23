//! Integration tests for the collection and iteration foundation
//! (Session 41): array iteration, Vec<T> runtime intrinsics.

use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn check_source(src: &str) -> mink::driver::CheckReport {
    let mut sources = mink::source::SourceMap::new();
    let id = sources.add(std::path::Path::new("test.mink"), src);
    let file = sources.get(id).unwrap();
    let parsed = mink::parser::parse(file);
    let mut mono_ast = parsed.ast().clone();
    mink::monomorphize::monomorphize(&mut mono_ast);
    let semantic = mink::semantics::analyze(&mono_ast);
    let types = mink::typecheck::check(&mono_ast, &semantic, &sources);
    mink::driver::CheckReport {
        source_id: id,
        token_count: parsed.token_count(),
        errors: parsed
            .lex_errors()
            .iter()
            .copied()
            .map(mink::driver::CheckError::Lex)
            .chain(
                parsed
                    .parse_errors()
                    .iter()
                    .copied()
                    .map(mink::driver::CheckError::Parse),
            )
            .chain(
                semantic
                    .errors()
                    .iter()
                    .cloned()
                    .map(mink::driver::CheckError::Semantic),
            )
            .chain(
                types
                    .errors()
                    .iter()
                    .cloned()
                    .map(mink::driver::CheckError::Type),
            )
            .collect(),
        semantic: Some(semantic),
        types: Some(types),
        ownership: None,
        hir: None,
        mir: None,
    }
}

fn native_exit_code(src: &str) -> i32 {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = format!("mink_coll_test_{n}.mink");
    let tmp = std::env::temp_dir().join(&name);
    std::fs::write(&tmp, src).unwrap();
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_mink"))
        .args(["build", tmp.to_str().unwrap()])
        .status()
        .expect("failed to run mink build");
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(tmp.with_extension("exe"));
        return -1;
    }
    let exe = tmp.with_extension("exe");
    let status = std::process::Command::new(&exe).status().unwrap();
    let code = status.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(&exe);
    code
}

// ===========================================================================
// Array iteration: `for x in arr`
// ===========================================================================

#[test]
fn array_for_sum_of_elements() {
    let src = r#"
fn main() -> Int {
    let arr = [10, 20, 30];
    let mut sum = 0;
    for x in arr {
        sum = sum + x;
    }
    return sum;
}"#;
    assert_eq!(native_exit_code(src), 60);
}

#[test]
fn array_for_count_matching() {
    let src = r#"
fn main() -> Int {
    let arr = [5, 10, 15, 20, 25];
    let mut count = 0;
    for x in arr {
        if x > 10 {
            count = count + 1;
        }
    }
    return count;
}"#;
    assert_eq!(native_exit_code(src), 3);
}

#[test]
fn array_for_zero_elements_contribute() {
    let src = r#"
fn main() -> Int {
    let arr = [0, 0, 0];
    let mut sum = 0;
    for x in arr {
        sum = sum + x;
    }
    return sum;
}"#;
    assert_eq!(native_exit_code(src), 0);
}

#[test]
fn array_for_single_element() {
    let src = r#"
fn main() -> Int {
    let arr = [42];
    let mut result = 0;
    for x in arr {
        result = x;
    }
    return result;
}"#;
    assert_eq!(native_exit_code(src), 42);
}

#[test]
fn array_for_bool_array() {
    let src = r#"
fn main() -> Int {
    let arr = [true, false, true, true, false];
    let mut count = 0;
    for x in arr {
        if x {
            count = count + 1;
        }
    }
    return count;
}"#;
    assert_eq!(native_exit_code(src), 3);
}

#[test]
fn array_for_with_break() {
    let src = r#"
fn main() -> Int {
    let arr = [1, 2, 3, 4, 5];
    let mut result = 0;
    for x in arr {
        if x == 3 {
            break;
        }
        result = result + x;
    }
    return result;
}"#;
    assert_eq!(native_exit_code(src), 3);
}

#[test]
fn array_for_with_continue() {
    let src = r#"
fn main() -> Int {
    let arr = [1, 2, 3, 4, 5];
    let mut sum = 0;
    for x in arr {
        if x == 3 {
            continue;
        }
        sum = sum + x;
    }
    return sum;
}"#;
    assert_eq!(native_exit_code(src), 12);
}

#[test]
fn array_for_nested_loops() {
    let src = r#"
fn main() -> Int {
    let arr = [2, 3, 4];
    let mut product = 1;
    for x in arr {
        let mut i = 0;
        while i < x {
            product = product * 2;
            i = i + 1;
        }
    }
    return product;
}"#;
    assert_eq!(native_exit_code(src), 512);
}

#[test]
fn array_for_check_passes() {
    let src = r#"
fn main() -> Int {
    let arr = [1, 2, 3];
    let mut sum = 0;
    for x in arr {
        sum = sum + x;
    }
    return sum;
}"#;
    let report = check_source(src);
    assert!(
        report.errors.is_empty(),
        "expected no errors, got {:?}",
        report.errors
    );
}

#[test]
fn array_for_type_error_on_non_iterable() {
    let src = r#"
fn main() -> Int {
    let x = 42;
    for y in x {
        return y;
    }
    return 0;
}"#;
    let report = check_source(src);
    assert!(
        !report.errors.is_empty(),
        "expected type error for non-iterable"
    );
}

// ===========================================================================
// Vec<T> runtime intrinsics (E2E)
// ===========================================================================

#[test]
fn vec_new_and_len() {
    let src = r#"
fn main() -> Int {
    let v = rt_vec_new(10);
    let len = rt_vec_len(v);
    rt_vec_free(v);
    return len;
}"#;
    assert_eq!(native_exit_code(src), 0);
}

#[test]
fn vec_push_and_len() {
    let src = r#"
fn main() -> Int {
    let mut v = rt_vec_new(10);
    v = rt_vec_push(v, 42);
    v = rt_vec_push(v, 99);
    let len = rt_vec_len(v);
    rt_vec_free(v);
    return len;
}"#;
    assert_eq!(native_exit_code(src), 2);
}

#[test]
fn vec_push_and_get() {
    let src = r#"
fn main() -> Int {
    let mut v = rt_vec_new(10);
    v = rt_vec_push(v, 100);
    v = rt_vec_push(v, 200);
    v = rt_vec_push(v, 300);
    let e0 = rt_vec_get(v, 0);
    let e1 = rt_vec_get(v, 1);
    let e2 = rt_vec_get(v, 2);
    rt_vec_free(v);
    return e0 + e1 + e2;
}"#;
    assert_eq!(native_exit_code(src), 600);
}

#[test]
fn vec_growth_reallocates() {
    let src = r#"
fn main() -> Int {
    let mut v = rt_vec_new(2);
    v = rt_vec_push(v, 10);
    v = rt_vec_push(v, 20);
    v = rt_vec_push(v, 30);
    v = rt_vec_push(v, 40);
    let len = rt_vec_len(v);
    let e0 = rt_vec_get(v, 0);
    let e3 = rt_vec_get(v, 3);
    rt_vec_free(v);
    return len + e0 + e3;
}"#;
    assert_eq!(native_exit_code(src), 54);
}

// ===========================================================================
// Determinism
// ===========================================================================

#[test]
fn array_iteration_deterministic() {
    let src = r#"
fn main() -> Int {
    let arr = [1, 2, 3, 4, 5];
    let mut sum = 0;
    for x in arr {
        sum = sum + x;
    }
    return sum;
}"#;
    let r1 = native_exit_code(src);
    let r2 = native_exit_code(src);
    assert_eq!(r1, 15);
    assert_eq!(r2, 15);
}
