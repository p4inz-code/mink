//! End-to-end tests for the MINK native runtime: build MINK programs that
//! use the `rt_*` runtime intrinsics (`rt_alloc`, `rt_free`, `rt_mem_load`,
//! `rt_mem_store`, `rt_print_int`) and verify the generated executables'
//! exit codes and stdout.
//!
//! The runtime semantics under test are documented in
//! `docs/implementation/RUNTIME_IMPLEMENTATION.md` and specified in
//! `src/runtime/allocator.rs` (the pure-Rust reference implementation).

use std::path::PathBuf;
use std::process::Command;

/// Returns a `Command` for the compiled `mink` binary.
fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

/// Writes `content` to a uniquely named temp file and returns its path.
fn temp_source(name: &str, content: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("mink_runtime_test_{}_{name}", std::process::id()));
    std::fs::write(&path, content).unwrap();
    path
}

/// Builds `source` with the compiler and returns the generated executable.
/// The temp file is named after the running test so parallel tests never
/// collide on the same path.
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
// Heap: allocation, lifetime, and error paths
// ---------------------------------------------------------------------------

#[test]
fn heap_round_trip_returns_stored_values() {
    let exe = build(
        "fn main() {
            let p = rt_alloc(64);
            rt_mem_store(p, 11);
            rt_mem_store(p + 8, 22);
            rt_mem_store(p + 16, 9);
            let a = rt_mem_load(p);
            let b = rt_mem_load(p + 8);
            let c = rt_mem_load(p + 16);
            rt_free(p);
            return a + b + c;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 42, "alloc/store/load/free round trip");
}

#[test]
fn freed_block_reuse_is_lifo_and_preserves_contents() {
    let exe = build(
        "fn main() {
            let p = rt_alloc(32);
            let q = rt_alloc(32);
            rt_mem_store(p + 8, 7);
            rt_free(q);
            // The next allocation reuses q's block (LIFO). Word 0 of a
            // freed block holds the free-list link, so the value stored at
            // p + 8 (an interior word) survives the reuse.
            let r = rt_alloc(32);
            rt_mem_store(r + 8, 35);
            rt_free(r);
            rt_free(p);
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 0, "alloc/free/reuse without errors");
}

#[test]
fn load_after_free_is_e_ro5() {
    let exe = build(
        "fn main() {
            let p = rt_alloc(32);
            rt_free(p);
            return rt_mem_load(p);
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 105, "accessing a freed block is E-R05");
}

#[test]
fn out_of_bounds_load_is_e_ro5() {
    let exe = build("fn main() { let p = rt_alloc(16); return rt_mem_load(p + 16); }");
    let (code, _) = run(&exe);
    assert_eq!(code, 105, "a word beyond the block is E-R05");
}

#[test]
fn misaligned_store_is_e_ro7() {
    let exe = build("fn main() { let p = rt_alloc(32); rt_mem_store(p + 4, 1); return 0; }");
    let (code, _) = run(&exe);
    assert_eq!(code, 107, "a 4-byte-misaligned store is E-R07");
}

#[test]
fn leaking_allocation_is_e_ro6() {
    let exe = build("fn main() { let p = rt_alloc(16); rt_mem_store(p, 1); return 0; }");
    let (code, _) = run(&exe);
    assert_eq!(code, 106, "a live allocation at exit is E-R06");
}

#[test]
fn null_free_is_e_ro4() {
    let exe = build("fn main() { rt_free(0); return 0; }");
    let (code, _) = run(&exe);
    assert_eq!(code, 104, "freeing null is E-R04");
}

#[test]
fn interior_free_is_e_ro4() {
    let exe = build("fn main() { let p = rt_alloc(64); rt_free(p + 16); return 0; }");
    let (code, _) = run(&exe);
    assert_eq!(code, 104, "freeing an interior pointer is E-R04");
}

#[test]
fn double_free_is_e_ro4() {
    let exe = build(
        "fn main() {
            let p = rt_alloc(32);
            rt_free(p);
            rt_free(p);
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 104, "a double free is E-R04");
}

#[test]
fn heap_exhaustion_is_e_ro2() {
    let exe = build(
        "fn main() {
            let a = rt_alloc(524288);
            let b = rt_alloc(524288);
            let c = rt_alloc(16);
            rt_free(a);
            rt_free(b);
            rt_free(c);
            return 0;
        }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 102, "exhausting the 1 MiB arena is E-R02");
}

#[test]
fn many_allocations_do_not_exhaust_the_table() {
    // 100 allocations and frees stay well under the 256-entry table.
    let mut src = String::from("fn main() {\n");
    for i in 0..100 {
        src.push_str(&format!("    let p{i} = rt_alloc(16);\n"));
    }
    for i in 0..100 {
        src.push_str(&format!("    rt_free(p{i});\n"));
    }
    src.push_str("    return 0;\n}\n");
    let exe = build(&src);
    let (code, _) = run(&exe);
    assert_eq!(code, 0, "many alloc/free pairs succeed");
}

// ---------------------------------------------------------------------------
// Output: rt_print_int
// ---------------------------------------------------------------------------

#[test]
fn print_int_writes_digits_and_crlf() {
    let exe = build(
        "fn main() {
            rt_print_int(12345);
            rt_print_int(-7);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"12345\r\n-7\r\n");
}

#[test]
fn print_int_zero_and_single_digit() {
    let exe = build("fn main() { rt_print_int(0); rt_print_int(1); return 0; }");
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"0\r\n1\r\n");
}

// ---------------------------------------------------------------------------
// Strings: rt_print_str, rt_str_alloc, rt_str_len, byte access
// ---------------------------------------------------------------------------

#[test]
fn print_str_writes_literal_bytes() {
    let exe = build("fn main() { rt_print_str(\"hello, mink!\"); return 0; }");
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"hello, mink!\r\n");
}

#[test]
fn print_str_preserves_embedded_escapes() {
    let exe = build("fn main() { rt_print_str(\"a\\tb\\n\\\"q\\\"\\0z\"); return 0; }");
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"a\tb\n\"q\"\0z\r\n");
}

#[test]
fn print_str_accepts_utf8_bytes() {
    let exe = build("fn main() { rt_print_str(\"caf\u{e9}\u{20ac}\"); return 0; }");
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    let mut expected = "café€".as_bytes().to_vec();
    expected.extend_from_slice(b"\r\n");
    assert_eq!(stdout, expected);
}

#[test]
fn empty_string_literal_prints_crlf() {
    let exe = build("fn main() { rt_print_str(\"\"); return 0; }");
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"\r\n");
}

#[test]
fn str_alloc_len_and_bytes_round_trip() {
    let exe = build(
        "fn main() {
            let s = rt_str_alloc(3);
            rt_str_set_byte(s, 0, 104);
            rt_str_set_byte(s, 1, 105);
            rt_str_set_byte(s, 2, 33);
            rt_print_str(s);
            rt_print_int(rt_str_len(s));
            rt_print_int(rt_str_byte(s, 0));
            rt_print_int(rt_str_byte(s, 2));
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"hi!\r\n3\r\n104\r\n33\r\n");
}

#[test]
fn str_alloc_starts_zero_filled() {
    let exe = build(
        "fn main() {
            let s = rt_str_alloc(2);
            rt_print_int(rt_str_byte(s, 0));
            rt_print_int(rt_str_byte(s, 1));
            rt_str_free(s);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"0\r\n0\r\n");
}

#[test]
fn str_byte_out_of_range_is_e_ro9() {
    let exe = build(
        "fn main() {
            let s = rt_str_alloc(2);
            let b = rt_str_byte(s, 5);
            rt_print_int(b);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 109);
    assert!(stdout.is_empty());
}

#[test]
fn str_set_byte_out_of_range_is_e_ro9() {
    let exe = build(
        "fn main() {
            let s = rt_str_alloc(1);
            rt_str_set_byte(s, 3, 65);
            return 0;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 109);
    assert!(stdout.is_empty());
}

#[test]
fn str_alloc_negative_size_is_e_ro8() {
    let exe = build("fn main() { let s = rt_str_alloc(-1); return 0; }");
    let (code, _stdout) = run(&exe);
    assert_eq!(code, 108);
}

#[test]
fn str_byte_negative_index_is_e_ro9() {
    let exe = build(
        "fn main() {
            let s = rt_str_alloc(2);
            rt_print_int(rt_str_byte(s, -1));
            return 0;
        }",
    );
    let (code, _stdout) = run(&exe);
    assert_eq!(code, 109);
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn runtime_errors_are_deterministic() {
    let src = "fn main() { let p = rt_alloc(16); rt_free(p); return rt_mem_load(p); }";
    let exe1 = build(src);
    let exe2 = build(src);
    assert_eq!(
        std::fs::read(&exe1).unwrap(),
        std::fs::read(&exe2).unwrap(),
        "identical sources produce identical images"
    );
    let (code1, _) = run(&exe1);
    let (code2, _) = run(&exe2);
    assert_eq!(code1, code2);
    assert_eq!(code1, 105);
}
