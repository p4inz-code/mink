//! Integration tests for the session 23 milestone: **sub-word aggregate
//! layout correctness** (the bool-packing defect).
//!
//! A struct that packs booleans into byte offsets 1..7 immediately before
//! an integer field used to read those booleans as `false` (and storing a
//! boolean corrupted the integer's top bytes): the emitter placed sub-word
//! pieces *mirrored* (`word0 - b`) while full-word fields occupy stacked
//! qwords (`[word0 - 8k, word0 - 8k + 7]`), so a sub-word piece at offsets
//! 1..7 landed inside the qword of the following integer field. The slot
//! image is now uniformly chunked — byte `b` of a value lives at
//! `word0 - 8*(b/8) + (b%8)`, normal byte order within each 8-byte chunk,
//! chunks stacked downward — so packed booleans survive integer stores,
//! integer stores no longer clobber booleans, and statics/copies/params/
//! returns/place chains/references agree on one byte layout.
//!
//! The rules under test are documented in
//! `docs/implementation/AGGREGATE_TYPES_IMPLEMENTATION.md` (session 23
//! section) and `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use mink::backend;
use mink::driver;
use mink::runtime::layout;
use mink::source::SourceMap;

/// A unique per-call suffix, so tests running in parallel never share a
/// temp source file.
static COUNTER: AtomicU32 = AtomicU32::new(0);

fn unique_source(kind: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "mink_bool_packing_{kind}_{}_{n}.mink",
        std::process::id()
    ))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parses, semantically analyzes, and type-checks `src`, returning the
/// semantic result and the type result (for front-end-level assertions).
fn check_src(
    src: &str,
) -> (
    SourceMap,
    mink::semantics::SemanticResult,
    mink::typecheck::TypeResult,
) {
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("test.mink"), src);
    let file = sources.get(id).expect("the file just added");
    let parsed = mink::parser::parse(file);
    assert!(
        parsed.is_valid(),
        "test source must lex and parse cleanly\nlex errors: {:?}\nparse errors: {:?}",
        parsed.lex_errors(),
        parsed.parse_errors()
    );
    let (ast, lex_errors, parse_errors) = parsed.into_parts();
    assert!(lex_errors.is_empty() && parse_errors.is_empty());
    let semantic = mink::semantics::analyze(&ast);
    let types = mink::typecheck::check(&ast, &semantic, &sources);
    (sources, semantic, types)
}

/// Runs the front end on `src` and lowers the optimized MIR into backend
/// instructions, asserting every stage is clean.
fn lower_backend(src: &str) -> (mink::mir::MirProgram, mink::backend::BProgram) {
    let mut sources = SourceMap::new();
    let path = unique_source("test");
    std::fs::write(&path, src).unwrap();
    let report = driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(
        report.errors.is_empty(),
        "the source itself must be valid MINK: {:?}",
        report.errors
    );
    let optimized = report.mir.expect("clean program lowers to MIR");
    let program = backend::lower(&optimized, &sources)
        .unwrap_or_else(|errors| panic!("clean MIR must lower: {errors:?}"));
    backend::verify(&program).expect("lowering must produce valid instructions");
    (optimized, program)
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("mink_bool_packing_{}_{name}", std::process::id()));
    std::fs::write(&path, content).unwrap();
    path
}

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

fn run(exe: &PathBuf) -> (i32, Vec<u8>) {
    let output = Command::new(exe).output().unwrap();
    let stdout = output.stdout;
    // The native runtime writes `\n`; the Windows console layer may hand
    // text-mode CRLF through pipes, so normalize before comparing.
    let stdout = if stdout.contains(&b'\r') {
        stdout
            .iter()
            .copied()
            .filter(|b| *b != b'\r')
            .collect::<Vec<u8>>()
    } else {
        stdout
    };
    (output.status.code().unwrap_or(-1), stdout)
}

/// The trimmed stdout of a program that must exit 0.
fn output_of(source: &str) -> String {
    let exe = build(source);
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0, "program must exit 0");
    String::from_utf8_lossy(&stdout).trim().to_string()
}

// ---------------------------------------------------------------------------
// Layout stays deterministic C-style (the fix is in emission, not layout)
// ---------------------------------------------------------------------------

#[test]
fn packed_bool_layout_offsets_are_unchanged() {
    // The deterministic C-style layout is authoritative and must not be
    // changed to dodge the emission defect: booleans stay packed at byte
    // offsets 1..7 and the integer follows at the next 8-byte boundary.
    let (_sources, semantic, types) = check_src(
        "struct F { a: Bool, b: Bool, c: Bool, d: Int }
         fn main() { let f = F { a: true, b: true, c: true, d: 1 }; return 0; }",
    );
    assert!(types.errors().is_empty());
    let table = types.types();
    let symbol = semantic
        .symbols()
        .iter()
        .find(|s| s.name == "f")
        .expect("binding `f` registered");
    let f_ty = types.symbol_type(symbol.id).expect("`f` is typed");
    let struct_id = table.struct_id(f_ty).expect("`f` is a struct");
    let struct_layout = layout::struct_layout(struct_id, table).expect("layout resolves");
    assert_eq!(struct_layout.size, 16);
    assert_eq!(struct_layout.align, 8);
    assert_eq!(struct_layout.fields[0].offset, 0); // a: Bool
    assert_eq!(struct_layout.fields[1].offset, 1); // b: Bool
    assert_eq!(struct_layout.fields[2].offset, 2); // c: Bool
    assert_eq!(struct_layout.fields[3].offset, 8); // d: Int
}

#[test]
fn bool_array_stride_stays_one_byte() {
    let (_sources, semantic, types) = check_src(
        "struct G { bs: [Bool; 3], d: Int }
         fn main() { let g = G { bs: [true, false, true], d: 1 }; return 0; }",
    );
    assert!(types.errors().is_empty());
    let table = types.types();
    let symbol = semantic
        .symbols()
        .iter()
        .find(|s| s.name == "g")
        .expect("binding `g` registered");
    let g_ty = types.symbol_type(symbol.id).expect("`g` is typed");
    let array_ty = match table.kind(g_ty) {
        Some(mink::typecheck::TypeKind::Struct(id)) => {
            let info = table.struct_info(*id).expect("struct G");
            match table.kind(info.fields[0].ty) {
                Some(mink::typecheck::TypeKind::Array { .. }) => info.fields[0].ty,
                other => panic!("`bs` must be an array type, got {other:?}"),
            }
        }
        other => panic!("`g` must be a struct, got {other:?}"),
    };
    let array_layout = layout::array_layout(array_ty, table).expect("layout resolves");
    assert_eq!(array_layout.elem_size, 1, "booleans keep stride 1");
    assert_eq!(array_layout.len, 3);
    assert_eq!(array_layout.size, 3);
}

// ---------------------------------------------------------------------------
// Static images stay in normal byte order (byte `b` at `base + b`)
// ---------------------------------------------------------------------------

#[test]
fn struct_static_images_pack_bools_then_ints_in_normal_order() {
    // The data region holds the value's bytes in normal order; the slot
    // image is chunked, and the two are bridged byte-exactly. The image
    // for a packed-bool struct must show the booleans at bytes 0..2 and
    // the integer at bytes 8..15.
    let (_mir, program) = lower_backend(
        "struct F { a: Bool, b: Bool, c: Bool, d: Int }
         const S = F { a: true, b: true, c: true, d: 7 };
         fn main() { return S.d; }",
    );
    let static_binding = &program.statics[0];
    assert_eq!(static_binding.ty, backend::BType::Struct);
    assert_eq!(
        static_binding.bytes,
        vec![
            1, 1, 1, 0, 0, 0, 0, 0, // a, b, c packed, then padding
            7, 0, 0, 0, 0, 0, 0, 0, // d
        ]
    );
}

#[test]
fn struct_static_with_bool_array_field_keeps_normal_order() {
    let (_mir, program) = lower_backend(
        "struct G { bs: [Bool; 3], d: Int }
         const S = G { bs: [true, false, true], d: 5 };
         fn main() { return S.d; }",
    );
    let static_binding = &program.statics[0];
    assert_eq!(
        static_binding.bytes,
        vec![
            1, 0, 1, 0, 0, 0, 0, 0, // the bool array, then padding
            5, 0, 0, 0, 0, 0, 0, 0, // d
        ]
    );
}

// ---------------------------------------------------------------------------
// Native: packed booleans and following integers coexist (the core defect)
// ---------------------------------------------------------------------------

#[test]
fn native_packed_bools_survive_integer_store() {
    // The exact session-22 limitation: `a`, `b`, `c` at offsets 0, 1, 2
    // and `d` at offset 8. Storing `d` used to clobber `b` and `c`
    // (they read `false` afterwards); now every field keeps its value.
    let out = output_of(
        "struct F { a: Bool, b: Bool, c: Bool, d: Int }
         fn main() {
             let mut f = F { a: true, b: true, c: true, d: 0 };
             f.d = 7;
             let mut n = 0;
             if f.a { n = n + 1; }
             if f.b { n = n + 2; }
             if f.c { n = n + 4; }
             rt_print_int(n * 100 + f.d);
             return;
         }",
    );
    // n = 7, so 7 * 100 + 7
    assert_eq!(out, "707");
}

#[test]
fn native_bool_store_does_not_corrupt_following_integer() {
    // The reverse direction: storing a packed boolean must not change the
    // integer that follows it (a byte store used to land inside the
    // integer's qword).
    let out = output_of(
        "struct F { a: Bool, b: Bool, c: Bool, d: Int }
         fn main() {
             let mut f = F { a: false, b: false, c: false, d: 7 };
             f.b = true;
             f.c = true;
             rt_print_int(f.d * 100);
             if f.b { rt_print_int(1); } else { rt_print_int(0); }
             if f.c { rt_print_int(1); } else { rt_print_int(0); }
             return;
         }",
    );
    assert_eq!(out, "700\n1\n1");
}

#[test]
fn native_packed_bools_at_every_offset_1_through_7() {
    // Booleans packed into every byte offset 0..7 immediately before an
    // integer field: each one must read back true after the integer is
    // stored, and the integer must read back intact after each boolean is
    // stored.
    let out = output_of(
        "struct F {
             a0: Bool, a1: Bool, a2: Bool, a3: Bool,
             a4: Bool, a5: Bool, a6: Bool, a7: Bool,
             d: Int
         }
         fn main() {
             let mut f = F { a0: true, a1: true, a2: true, a3: true,
                             a4: true, a5: true, a6: true, a7: true, d: 0 };
             f.d = 255;
             let mut n = 0;
             if f.a0 { n = n + 1; }
             if f.a1 { n = n + 2; }
             if f.a2 { n = n + 4; }
             if f.a3 { n = n + 8; }
             if f.a4 { n = n + 16; }
             if f.a5 { n = n + 32; }
             if f.a6 { n = n + 64; }
             if f.a7 { n = n + 128; }
             rt_print_int(n);
             rt_print_int(f.d);
             f.a5 = false;
             rt_print_int(f.d);
             f.d = 1000;
             if f.a5 { rt_print_int(1); } else { rt_print_int(0); }
             return;
         }",
    );
    // n = 255, d = 255; a5 reset to false without touching d; d = 1000;
    // a5 still false.
    assert_eq!(out, "255\n255\n255\n0");
}

#[test]
fn native_packed_bools_in_second_chunk() {
    // Booleans at offsets 8..15 (the second chunk) before an integer at
    // offset 16, plus a trailing boolean after the integer.
    let out = output_of(
        "struct F { a: Int, b: Bool, c: Bool, d: Int, e: Bool }
         fn main() {
             let mut f = F { a: 1, b: true, c: true, d: 0, e: true };
             f.d = 42;
             let mut n = 0;
             if f.b { n = n + 2; }
             if f.c { n = n + 4; }
             if f.e { n = n + 16; }
             rt_print_int(n + f.a * 100 + f.d);
             return;
         }",
    );
    // n = 22, so 22 + 100 + 42
    assert_eq!(out, "164");
}

#[test]
fn native_packed_bool_place_store_reaches_the_field() {
    // Place-store chains (`f.b = v`, `f.c = v`) through packed booleans
    // write exactly the field's byte, leaving the following integer alone.
    let out = output_of(
        "struct F { a: Bool, b: Bool, c: Bool, d: Int }
         fn main() {
             let mut f = F { a: false, b: false, c: false, d: 9 };
             f.b = true;
             f.c = true;
             rt_print_int(f.d);
             if f.b { rt_print_int(1); } else { rt_print_int(0); }
             if f.c { rt_print_int(1); } else { rt_print_int(0); }
             f.d = 77;
             if f.b { rt_print_int(1); } else { rt_print_int(0); }
             return;
         }",
    );
    assert_eq!(out, "9\n1\n1\n1");
}

#[test]
fn native_packed_bools_survive_whole_value_copies() {
    // A whole-value copy moves chunks as qwords; the packed booleans ride
    // inside their chunk, so the copy reads back identically and mutating
    // the copy leaves the original's integer (and booleans) intact.
    let out = output_of(
        "struct F { a: Bool, b: Bool, c: Bool, d: Int }
         fn main() {
             let mut f = F { a: true, b: true, c: true, d: 11 };
             let mut g = f;
             g.d = 22;
             g.b = false;
             let mut n = 0;
             if f.a { n = n + 1; }
             if f.b { n = n + 2; }
             if f.c { n = n + 4; }
             let mut m = 0;
             if g.a { m = m + 1; }
             if g.b { m = m + 2; }
             if g.c { m = m + 4; }
             rt_print_int(f.d * 100 + n);
             rt_print_int(g.d * 100 + m);
             return;
         }",
    );
    // f: d=11, n=7 -> 1107; g: d=22, m=5 (b false) -> 2205
    assert_eq!(out, "1107\n2205");
}

#[test]
fn native_packed_bools_pass_and_return_through_slots() {
    // Packed booleans survive argument passing (word-wise pushes) and
    // aggregate returns (caller-allocated return slot).
    let out = output_of(
        "struct F { a: Bool, b: Bool, c: Bool, d: Int }
         fn read(f) {
             let mut n = 0;
             if f.a { n = n + 1; }
             if f.b { n = n + 2; }
             if f.c { n = n + 4; }
             return n * 100 + f.d;
         }
         fn make() { return F { a: true, b: true, c: false, d: 6 }; }
         fn main() {
             let f = F { a: true, b: false, c: true, d: 5 };
             rt_print_int(read(f));
             rt_print_int(read(make()));
             return;
         }",
    );
    // read(f): n=5, d=5 -> 505; read(make()): n=3, d=6 -> 306
    assert_eq!(out, "505\n306");
}

// ---------------------------------------------------------------------------
// Native: module-scope statics with packed booleans
// ---------------------------------------------------------------------------

#[test]
fn native_packed_bool_static_reads_all_fields() {
    let out = output_of(
        "struct F { a: Bool, b: Bool, c: Bool, d: Int }
         const S = F { a: true, b: true, c: true, d: 7 };
         fn main() {
             let mut n = 0;
             if S.a { n = n + 1; }
             if S.b { n = n + 2; }
             if S.c { n = n + 4; }
             rt_print_int(n * 100 + S.d);
             return;
         }",
    );
    // n = 7, so 7 * 100 + 7
    assert_eq!(out, "707");
}

#[test]
fn native_packed_bool_mutable_static_read_modify_write() {
    // A mutable module binding with packed booleans is written through
    // read-modify-write place stores; both the booleans and the integer
    // must read back correctly.
    let out = output_of(
        "struct F { a: Bool, b: Bool, c: Bool, d: Int }
         let mut S = F { a: false, b: false, c: false, d: 0 };
         fn main() {
             S.b = true;
             S.c = true;
             S.d = 31;
             let mut n = 0;
             if S.a { n = n + 1; }
             if S.b { n = n + 2; }
             if S.c { n = n + 4; }
             rt_print_int(n * 100 + S.d);
             return;
         }",
    );
    // n = 6, so 6 * 100 + 31
    assert_eq!(out, "631");
}

// ---------------------------------------------------------------------------
// Native: bool arrays inside structs, before an integer field
// ---------------------------------------------------------------------------

#[test]
fn native_bool_array_field_does_not_corrupt_following_integer() {
    let out = output_of(
        "struct G { bs: [Bool; 3], d: Int }
         fn main() {
             let mut g = G { bs: [true, true, true], d: 0 };
             g.d = 13;
             let mut n = 0;
             if g.bs[0] { n = n + 1; }
             if g.bs[1] { n = n + 2; }
             if g.bs[2] { n = n + 4; }
             rt_print_int(n * 100 + g.d);
             g.bs[1] = false;
             rt_print_int(g.d);
             return;
         }",
    );
    // n = 7, so 7 * 100 + 13; then d unchanged after a bool element store.
    assert_eq!(out, "713\n13");
}

#[test]
fn native_bool_array_elements_across_chunk_boundary() {
    // A `[Bool; N]` array longer than one chunk: elements at indices 0..7
    // live in the first chunk and indices 8..15 in the second, so element
    // addressing must apply the chunk correction.
    let out = output_of(
        "fn main() {
             let mut a = [true, false, true, false, true, false, true, false,
                          true, false, true, false, true, false, true, false];
             let mut n = 0;
             if a[0] { n = n + 1; }
             if a[2] { n = n + 4; }
             if a[4] { n = n + 16; }
             if a[6] { n = n + 64; }
             if a[8] { n = n + 256; }
             if a[10] { n = n + 1024; }
             if a[12] { n = n + 4096; }
             if a[14] { n = n + 16384; }
             rt_print_int(n);
             a[9] = true;
             a[15] = true;
             let mut m = 0;
             if a[8] { m = m + 1; }
             if a[9] { m = m + 2; }
             if a[15] { m = m + 4; }
             rt_print_int(m);
             return;
         }",
    );
    // Even indices read true in both chunks: n = 1+4+16+64+256+1024+4096+
    // 16384 = 21845; after storing a[9] and a[15] (odd indices of the
    // second chunk), m = 1+2+4 = 7.
    assert_eq!(out, "21845\n7");
}

#[test]
fn native_bool_array_bounds_check_still_traps_across_chunks() {
    // The chunk correction is applied after the bounds check; an
    // out-of-range index still traps with E-R10 (exit status 110).
    let exe = build(
        "fn main() {
             let a = [true, false, true, false, true];
             let mut i = 5;
             if a[i] { rt_print_int(1); }
             return 0;
         }",
    );
    let (code, _) = run(&exe);
    assert_eq!(code, 110, "an out-of-range bool index must trap E-R10");
}

// ---------------------------------------------------------------------------
// Native: nested structs and enum payloads with packed booleans
// ---------------------------------------------------------------------------

#[test]
fn native_nested_struct_with_packed_bools() {
    let out = output_of(
        "struct F { a: Bool, b: Bool, c: Bool, d: Int }
         struct Outer { x: Int, inner: F }
         fn main() {
             let mut o = Outer { x: 3, inner: F { a: true, b: true, c: true, d: 0 } };
             o.inner.d = 41;
             o.inner.b = false;
             let mut n = 0;
             if o.inner.a { n = n + 1; }
             if o.inner.b { n = n + 2; }
             if o.inner.c { n = n + 4; }
             rt_print_int(o.x * 1000 + n * 10 + o.inner.d);
             return;
         }",
    );
    // n = 5, so 3 * 1000 + 5 * 10 + 41
    assert_eq!(out, "3091");
}

#[test]
fn native_enum_payload_struct_with_packed_bools() {
    let out = output_of(
        "struct F { a: Bool, b: Bool, c: Bool, d: Int }
         enum E { A(F), B }
         fn main() {
             let e = E::A(F { a: true, b: true, c: true, d: 8 });
             let mut n = 0;
             match e {
                 E::A(p) => {
                     if p.a { n = n + 1; }
                     if p.b { n = n + 2; }
                     if p.c { n = n + 4; }
                     rt_print_int(n * 100 + p.d);
                 },
                 E::B => { rt_print_int(0); }
             }
             return;
         }",
    );
    // n = 7, so 7 * 100 + 8
    assert_eq!(out, "708");
}

// ---------------------------------------------------------------------------
// Native: references to packed fields
// ---------------------------------------------------------------------------

#[test]
fn native_reference_to_packed_bool_field_reads_and_writes() {
    // `&f.b` (b at byte offset 1) must compute the chunk-corrected
    // address: the reference deref reads and writes the boolean byte, and
    // neither direction touches the following integer.
    let out = output_of(
        "struct F { a: Bool, b: Bool, c: Bool, d: Int }
         fn flip(r) { if *r { *r = false; } else { *r = true; } }
         fn main() {
             let mut f = F { a: true, b: true, c: true, d: 9 };
             flip(&mut f.b);
             flip(&mut f.c);
             let mut n = 0;
             if f.a { n = n + 1; }
             if f.b { n = n + 2; }
             if f.c { n = n + 4; }
             rt_print_int(f.d * 100 + n);
             return;
         }",
    );
    // b flipped to false, c flipped to false: n = 1; d untouched.
    assert_eq!(out, "901");
}

#[test]
fn native_reference_to_integer_after_packed_bools() {
    // `&f.d` (d at byte offset 8) must still resolve to the integer's
    // qword even when packed booleans precede it.
    let out = output_of(
        "struct F { a: Bool, b: Bool, c: Bool, d: Int }
         fn bump(r) { *r = *r + 1; }
         fn main() {
             let mut f = F { a: true, b: true, c: true, d: 6 };
             bump(&mut f.d);
             let mut n = 0;
             if f.a { n = n + 1; }
             if f.b { n = n + 2; }
             if f.c { n = n + 4; }
             rt_print_int(f.d * 100 + n);
             return;
         }",
    );
    // d bumped to 7, n = 7 -> 707
    assert_eq!(out, "707");
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn native_packed_bool_programs_emit_byte_identical_images() {
    let src = "struct F { a: Bool, b: Bool, c: Bool, d: Int }
               const S = F { a: true, b: true, c: true, d: 7 };
               fn make() { return F { a: true, b: true, c: true, d: 5 }; }
               fn main() {
                   let f = make();
                   let mut n = 0;
                   if f.b { n = n + 2; }
                   if S.c { n = n + 4; }
                   rt_print_int(n);
                   return;
               }";
    let exe1 = build(src);
    let exe2 = build(src);
    assert_eq!(
        std::fs::read(&exe1).unwrap(),
        std::fs::read(&exe2).unwrap(),
        "identical sources produce identical images"
    );
    let (code1, out1) = run(&exe1);
    let (code2, out2) = run(&exe2);
    assert_eq!(code1, code2);
    assert_eq!(out1, out2);
    assert_eq!(code1, 0);
    // f.b (offset 1, via the return slot) and S.c (offset 2, via the
    // static data region) both read true: n = 6.
    assert_eq!(String::from_utf8_lossy(&out1).trim(), "6");
}
