//! M7 Phase D — permanent mixed-size allocator conformance corpus.
//!
//! These tests pin the heap's *reuse* behaviour, not merely that programs
//! complete. Every program is compiled by the real CLI into a native Windows
//! PE and executed; a clean exit also runs the runtime leak check (`E-R06`).
//!
//! The central measurement is a block's **arena offset**, computed as
//! `rt_ptr_to_int(p) - base` where `base` is the address of a 16-byte marker
//! allocated before anything else. The marker is the first heap request, so it
//! occupies offset 0 and `base` is the arena base address; an offset therefore
//! says exactly how much of the arena a workload has consumed. That is what
//! distinguishes "the loop finished" from "the freed blocks were reused": the
//! original mixed-size defect completed small iteration counts and simply grew
//! the arena by a 64 KiB block per cycle until `E-R02`.
//!
//! For strings and collections the heap block is reached through an opaque
//! handle, so the offset of a following 16-byte **probe** allocation is used
//! instead: if the churn reused its blocks the probe keeps landing at the same
//! offset, and if it leaked the probe walks up with every round.
//!
//! Historical defect (M2-D1, closed here): first-fit reuse unlinked the chosen
//! block by overwriting `free_head` with that block's successor — which dropped
//! every free block ahead of it — and re-recorded the block at the smaller
//! requested size, discarding the remainder. A 64 KiB / 1 KiB churn therefore
//! lost ~64 KiB per iteration and died after ~16,000 cycles in the 1 GiB
//! arena. Every case below fails against that allocator and passes with
//! address-ordered first fit, exact splitting and adjacent coalescing.
//!
//! One documented bound interacts with these tests: Session 204 retires a
//! block after 65,536 reuses, because its 16-bit handle generation must never
//! wrap. Retirement is the only legitimate reason for churn to consume new
//! blocks, so the long churn case asserts *bounded* growth (one 64 KiB block
//! per 65,536 cycles) rather than growth-free behaviour.

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn artifact_dir() -> PathBuf {
    let dir = PathBuf::from("target")
        .join("mink-artifacts")
        .join("allocator_conformance");
    std::fs::create_dir_all(&dir).expect("failed to create the artifact directory");
    dir
}

/// Compiles `source`, returning the executable path. On a build failure the
/// source is kept for inspection.
fn build(tag: &str, source: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("alloc_{tag}_{n}.mink"));
    std::fs::write(&path, source.replace("\r\n", "\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("build")
        .arg(&path)
        .output()
        .expect("failed to run the compiler");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(path.with_extension("exe"));
        panic!(
            "build failed (source kept at {}):\n{stderr}",
            path.display()
        );
    }
    let _ = std::fs::remove_file(&path);
    path.with_extension("exe")
}

fn run(exe: &Path) -> (i32, String, String) {
    let out_path = exe.with_extension("out");
    let err_path = exe.with_extension("err");
    let output = Command::new(exe)
        .stdout(std::fs::File::create(&out_path).unwrap())
        .stderr(std::fs::File::create(&err_path).unwrap())
        .output()
        .expect("failed to run the compiled program");
    let stdout = std::fs::read_to_string(&out_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(&err_path).unwrap_or_default();
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&err_path);
    (output.status.code().unwrap_or(-1), stdout, stderr)
}

fn build_and_run_libs(tag: &str, libs: &[&str], body: &str) -> (i32, String, String) {
    let mut source = String::new();
    for lib in libs {
        let text = std::fs::read_to_string(format!("stdlib/{lib}.mink"))
            .unwrap_or_else(|e| panic!("failed to read stdlib/{lib}.mink: {e}"));
        source.push_str(&text);
        source.push('\n');
    }
    source.push_str("fn main() {\n");
    source.push_str(body);
    source.push_str("\n}\n");
    let exe = build(tag, &source);
    let (code, out, err) = run(&exe);
    let _ = std::fs::remove_file(&exe);
    (code, out, err)
}

fn build_and_run(tag: &str, body: &str) -> (i32, String, String) {
    build_and_run_libs(tag, &[], body)
}

/// Compiles a complete program source (with its own `mod`/`fn` preamble) and
/// runs it, returning `(exit code, stdout, stderr, elapsed seconds)`.
fn build_and_run_source(tag: &str, source: &str) -> (i32, String, String, f64) {
    let exe = build(tag, source);
    let started = std::time::Instant::now();
    let (code, out, err) = run(&exe);
    let elapsed = started.elapsed().as_secs_f64();
    let _ = std::fs::remove_file(&exe);
    (code, out, err, elapsed)
}

/// Every integer printed by a program, in order.
fn ints(out: &str) -> Vec<i64> {
    out.split_whitespace()
        .filter_map(|t| t.parse::<i64>().ok())
        .collect()
}

fn assert_clean(tag: &str, code: i32, out: &str, err: &str) {
    assert_eq!(code, 0, "{tag}: exit {code}; stdout={out:?} stderr={err:?}");
    assert!(err.is_empty(), "{tag}: unexpected stderr: {err:?}");
}

/// Program header that allocates the offset marker and exposes it as `base`.
///
/// `marker` stays live for the whole program so the address it pins remains the
/// arena base; each program frees it before exiting (a leak would be `E-R06`).
const MARKER: &str = r#"
let marker = rt_alloc(16);
let base = rt_ptr_to_int(marker);
"#;

// ============================================================================
// 1. Repeated 64 KiB / 1 KiB churn — the original M2-D1 reproduction
// ============================================================================

#[test]
fn ac01_mixed_size_churn_reuses_one_block() {
    // Each cycle reuses the block start twice (once as the 64 KiB block, once
    // as the 1 KiB block carved out of it), so the 16-bit generation reaches
    // retirement after ~32,768 cycles. 20,000 cycles therefore stays well
    // inside one block's life and the whole workload must live in one 64 KiB
    // block; the bounded-growth case below covers the retirement behaviour.
    let body = format!(
        r#"{MARKER}
    let mut i = 0;
    let mut first = -1;
    let mut last = -1;
    let mut max_off = -1;
    while i < 20000 {{
        let a = rt_alloc(65536);
        let ao = rt_ptr_to_int(a) - base;
        if first < 0 {{ first = ao; }}
        last = ao;
        if ao > max_off {{ max_off = ao; }}
        rt_mem_store(a, i);
        if rt_mem_load(a) != i {{ rt_exit(9); }}
        rt_free(a);
        let b = rt_alloc(1024);
        let bo = rt_ptr_to_int(b) - base;
        if bo > max_off {{ max_off = bo; }}
        rt_mem_store(b, 7);
        if rt_mem_load(b) != 7 {{ rt_exit(8); }}
        rt_free(b);
        i = i + 1;
    }}
    rt_free(marker);
    rt_print_int(first);
    rt_print_int(last);
    rt_print_int(max_off);
    rt_exit(0);"#
    );
    let (code, out, err) = build_and_run("churn", &body);
    assert_clean("64K/1K churn", code, &out, &err);
    assert_eq!(
        ints(&out),
        vec![16, 16, 16],
        "the 64 KiB block must be reused in place: every cycle has to land on \
         offset 16 (the marker is offsets 0..16), so the arena never grows; \
         stdout={out:?}"
    );
}

#[test]
fn ac01_long_churn_growth_is_bounded() {
    // 200,000 cycles used to consume >12 GiB of arena growth and die with
    // E-R02 after ~16,000 cycles. The only legitimate growth now is the
    // generation retirement of a reused block (one 64 KiB block per 65,536
    // reuses), so four blocks cover 200,000 cycles.
    let body = format!(
        r#"{MARKER}
    let mut i = 0;
    let mut max_off = 0;
    while i < 200000 {{
        let a = rt_alloc(65536);
        let ao = rt_ptr_to_int(a) - base;
        if ao > max_off {{ max_off = ao; }}
        rt_free(a);
        let b = rt_alloc(1024);
        let bo = rt_ptr_to_int(b) - base;
        if bo > max_off {{ max_off = bo; }}
        rt_free(b);
        i = i + 1;
    }}
    rt_free(marker);
    rt_print_int(max_off);
    rt_exit(0);"#
    );
    let (code, out, err) = build_and_run("longchurn", &body);
    assert_clean("long churn", code, &out, &err);
    let v = ints(&out);
    assert_eq!(v.len(), 1, "stdout={out:?}");
    assert!(
        v[0] < 512 * 1024,
        "200,000 cycles may only grow the arena through generation \
         retirement (four 64 KiB blocks); high-water offset was {}",
        v[0]
    );
}

// ============================================================================
// 2. Alternating small, medium and large allocations
// ============================================================================

#[test]
fn ac02_alternating_small_medium_large_stable() {
    let body = format!(
        r#"{MARKER}
    let mut round = 0;
    let mut a0 = -1;
    let mut b0 = -1;
    let mut c0 = -1;
    let mut a1 = -1;
    let mut b1 = -1;
    let mut c1 = -1;
    while round < 200 {{
        let small = rt_alloc(64);
        let medium = rt_alloc(4096);
        let large = rt_alloc(262144);
        if round == 1 {{
            a0 = rt_ptr_to_int(small) - base;
            b0 = rt_ptr_to_int(medium) - base;
            c0 = rt_ptr_to_int(large) - base;
        }}
        if round == 199 {{
            a1 = rt_ptr_to_int(small) - base;
            b1 = rt_ptr_to_int(medium) - base;
            c1 = rt_ptr_to_int(large) - base;
        }}
        rt_mem_store(large, round);
        rt_free(large);
        rt_free(small);
        rt_free(medium);
        round = round + 1;
    }}
    rt_free(marker);
    rt_print_int(a0);
    rt_print_int(b0);
    rt_print_int(c0);
    rt_print_int(a1);
    rt_print_int(b1);
    rt_print_int(c1);
    rt_exit(0);"#
    );
    let (code, out, err) = build_and_run("alternate", &body);
    assert_clean("alternating sizes", code, &out, &err);
    let v = ints(&out);
    assert_eq!(v.len(), 6, "stdout={out:?}");
    assert_eq!(
        &v[0..3],
        &v[3..6],
        "every round must land on the same three blocks; got {v:?}"
    );
}

// ============================================================================
// 3. Multiple sizes freed in a varied order
// ============================================================================

#[test]
fn ac03_varied_free_order_keeps_capacity() {
    let body = format!(
        r#"{MARKER}
    let a = rt_alloc(16);
    let b = rt_alloc(64);
    let c = rt_alloc(256);
    let d = rt_alloc(1024);
    let e = rt_alloc(4096);
    let extent = 16 + 64 + 256 + 1024 + 4096;
    rt_free(c);
    rt_free(a);
    rt_free(e);
    rt_free(b);
    rt_free(d);
    // Everything is free again; re-request in a different order and prove
    // every block comes out of the released extent instead of the bump cursor.
    let p = rt_alloc(4096);
    let q = rt_alloc(1024);
    let r = rt_alloc(256);
    let s = rt_alloc(64);
    let t = rt_alloc(16);
    let mut max = rt_ptr_to_int(p) - base;
    if rt_ptr_to_int(q) - base > max {{ max = rt_ptr_to_int(q) - base; }}
    if rt_ptr_to_int(r) - base > max {{ max = rt_ptr_to_int(r) - base; }}
    if rt_ptr_to_int(s) - base > max {{ max = rt_ptr_to_int(s) - base; }}
    if rt_ptr_to_int(t) - base > max {{ max = rt_ptr_to_int(t) - base; }}
    let mut inside = 0;
    if max >= 16 {{ if max < 16 + extent {{ inside = 1; }} }}
    rt_free(p);
    rt_free(q);
    rt_free(r);
    rt_free(s);
    rt_free(t);
    rt_free(marker);
    rt_print_int(max);
    rt_print_int(extent);
    rt_print_int(inside);
    rt_exit(0);"#
    );
    let (code, out, err) = build_and_run("freeorder", &body);
    assert_clean("varied free order", code, &out, &err);
    let v = ints(&out);
    assert_eq!(v.len(), 3, "stdout={out:?}");
    assert_eq!(
        v[2], 1,
        "every re-request must start inside the released {}-byte extent; the \
         highest start was offset {}",
        v[1], v[0]
    );
}

// ============================================================================
// 4. Adjacent free-block coalescing
// ============================================================================

#[test]
fn ac04_adjacent_free_blocks_coalesce() {
    let body = format!(
        r#"{MARKER}
    let a = rt_alloc(4096);
    let b = rt_alloc(4096);
    let c = rt_alloc(4096);
    let d = rt_alloc(4096);
    let a_off = rt_ptr_to_int(a) - base;
    let b_off = rt_ptr_to_int(b) - base;
    rt_free(b);
    rt_free(c);
    // b and c are adjacent, so the 8 KiB request must be served by merged
    // b+c rather than by bumping a new 8 KiB block.
    let merged = rt_alloc(8192);
    let m_off = rt_ptr_to_int(merged) - base;
    rt_mem_store(merged, 1);
    rt_mem_store(merged + 8184, 2);
    let mut ok = 0;
    if rt_mem_load(merged) == 1 {{ if rt_mem_load(merged + 8184) == 2 {{ ok = 1; }} }}
    rt_free(merged);
    rt_free(a);
    rt_free(d);
    rt_free(marker);
    rt_print_int(a_off);
    rt_print_int(b_off);
    rt_print_int(m_off);
    rt_print_int(ok);
    rt_exit(0);"#
    );
    let (code, out, err) = build_and_run("coalesce", &body);
    assert_clean("adjacent coalescing", code, &out, &err);
    let v = ints(&out);
    assert_eq!(v.len(), 4, "stdout={out:?}");
    assert_eq!(
        v[2], v[1],
        "the 8 KiB request must reuse the coalesced b+c block at offset {}",
        v[1]
    );
    assert_eq!(v[3], 1, "both ends of the merged block must be writable");
}

// ============================================================================
// 5. Non-adjacent free blocks must not coalesce
// ============================================================================

#[test]
fn ac05_non_adjacent_free_blocks_do_not_coalesce() {
    let body = format!(
        r#"{MARKER}
    let a = rt_alloc(4096);
    let b = rt_alloc(4096);
    let c = rt_alloc(4096);
    let a_off = rt_ptr_to_int(a) - base;
    let b_off = rt_ptr_to_int(b) - base;
    let c_off = rt_ptr_to_int(c) - base;
    rt_free(a);
    rt_free(c);
    // `b` is still live, so an 8 KiB request cannot be served from a+c.
    let big = rt_alloc(8192);
    let big_off = rt_ptr_to_int(big) - base;
    rt_mem_store(big, 3);
    let mut ok = 0;
    if rt_mem_load(b) == 0 {{ if rt_mem_load(big) == 3 {{ ok = 1; }} }}
    rt_free(big);
    rt_free(b);
    rt_free(marker);
    rt_print_int(a_off);
    rt_print_int(b_off);
    rt_print_int(c_off);
    rt_print_int(big_off);
    rt_print_int(ok);
    rt_exit(0);"#
    );
    let (code, out, err) = build_and_run("nonadjacent", &body);
    assert_clean("non-adjacent blocks", code, &out, &err);
    let v = ints(&out);
    assert_eq!(v.len(), 5, "stdout={out:?}");
    assert_ne!(
        v[3], v[0],
        "the 8 KiB block must not be placed where `a` was: `b` is live"
    );
    assert_ne!(v[3], v[2], "the 8 KiB block must not swallow `c` either");
    assert!(
        v[3] > v[2],
        "the request must come from fresh space above the live block ({v:?})"
    );
    assert_eq!(v[4], 1, "the live block must be untouched");
}

// ============================================================================
// 6. Reuse after a partial block split
// ============================================================================

#[test]
fn ac06_split_remainder_returns_to_the_pool() {
    let body = format!(
        r#"{MARKER}
    let whole = rt_alloc(65536);
    let whole_off = rt_ptr_to_int(whole) - base;
    rt_free(whole);
    // Carve a small block out of the big one...
    let small = rt_alloc(1024);
    let small_off = rt_ptr_to_int(small) - base;
    if small_off != whole_off {{ rt_exit(7); }}
    rt_free(small);
    // ...and the whole 64 KiB block must be available again, because the
    // 1 KiB sliver coalesced with the 63 KiB remainder.
    let again = rt_alloc(65536);
    let again_off = rt_ptr_to_int(again) - base;
    rt_free(again);
    // The same block must then serve two 32 KiB halves.
    let half1 = rt_alloc(32768);
    let h1 = rt_ptr_to_int(half1) - base;
    let half2 = rt_alloc(32768);
    let h2 = rt_ptr_to_int(half2) - base;
    rt_mem_store(half2, 5);
    let mut ok = 0;
    if rt_mem_load(half2) == 5 {{ ok = 1; }}
    rt_free(half1);
    rt_free(half2);
    rt_free(marker);
    rt_print_int(whole_off);
    rt_print_int(small_off);
    rt_print_int(again_off);
    rt_print_int(h1);
    rt_print_int(h2);
    rt_print_int(ok);
    rt_exit(0);"#
    );
    let (code, out, err) = build_and_run("split", &body);
    assert_clean("partial split", code, &out, &err);
    let v = ints(&out);
    assert_eq!(v.len(), 6, "stdout={out:?}");
    assert_eq!(v[1], v[0], "the small block is carved from the big one");
    assert_eq!(
        v[2], v[0],
        "the full 64 KiB block must be reusable after the split is released"
    );
    assert_eq!(
        v[3], v[0],
        "the first 32 KiB half comes from the same block"
    );
    assert_eq!(v[4], v[0] + 32768, "the second half is the remainder");
    assert_eq!(v[5], 1);
}

// ============================================================================
// 7. Repeated allocate/free across many size classes
// ============================================================================

#[test]
fn ac07_many_size_classes_stay_within_one_extent() {
    let body = format!(
        r#"{MARKER}
    let mut round = 0;
    let mut first_round_max = -1;
    let mut high = -1;
    while round < 2000 {{
        let mut max = -1;
        let s0 = rt_alloc(16);
        if rt_ptr_to_int(s0) - base > max {{ max = rt_ptr_to_int(s0) - base; }}
        rt_free(s0);
        let s1 = rt_alloc(32);
        if rt_ptr_to_int(s1) - base > max {{ max = rt_ptr_to_int(s1) - base; }}
        rt_free(s1);
        let s2 = rt_alloc(64);
        if rt_ptr_to_int(s2) - base > max {{ max = rt_ptr_to_int(s2) - base; }}
        rt_free(s2);
        let s3 = rt_alloc(128);
        if rt_ptr_to_int(s3) - base > max {{ max = rt_ptr_to_int(s3) - base; }}
        rt_free(s3);
        let s4 = rt_alloc(512);
        if rt_ptr_to_int(s4) - base > max {{ max = rt_ptr_to_int(s4) - base; }}
        rt_free(s4);
        let s5 = rt_alloc(2048);
        if rt_ptr_to_int(s5) - base > max {{ max = rt_ptr_to_int(s5) - base; }}
        rt_free(s5);
        let s6 = rt_alloc(8192);
        if rt_ptr_to_int(s6) - base > max {{ max = rt_ptr_to_int(s6) - base; }}
        rt_free(s6);
        let s7 = rt_alloc(32768);
        if rt_ptr_to_int(s7) - base > max {{ max = rt_ptr_to_int(s7) - base; }}
        rt_free(s7);
        let s8 = rt_alloc(131072);
        if rt_ptr_to_int(s8) - base > max {{ max = rt_ptr_to_int(s8) - base; }}
        rt_free(s8);
        if round == 0 {{ first_round_max = max; }}
        if max > high {{ high = max; }}
        round = round + 1;
    }}
    rt_free(marker);
    rt_print_int(first_round_max);
    rt_print_int(high);
    rt_exit(0);"#
    );
    let (code, out, err) = build_and_run("sizeclasses", &body);
    assert_clean("size classes", code, &out, &err);
    let v = ints(&out);
    assert_eq!(v.len(), 2, "stdout={out:?}");
    assert_eq!(
        v[0], v[1],
        "2,000 rounds over nine size classes must never raise the high-water \
         mark after the first round (first round={}, all rounds={})",
        v[0], v[1]
    );
    assert!(
        v[1] < 200_000,
        "the nine classes total ~175 KiB; got a high-water start of {}",
        v[1]
    );
}

// ============================================================================
// 8. Randomized deterministic allocation/free sequences
// ============================================================================

#[test]
fn ac08_randomized_sequence_is_bounded_and_clean() {
    let body = format!(
        r#"{MARKER}
    let mut rng = 88172645463325252;
    let mut slots = rt_vec_new(64);
    let mut i = 0;
    while i < 64 {{
        rt_vec_push(slots, 0);
        i = i + 1;
    }}
    let mut op = 0;
    let mut max_off = 0;
    while op < 40000 {{
        rng = (rng * 1103515245 + 12345) & 2147483647;
        let slot = rng % 64;
        let cur = rt_vec_get(slots, slot);
        if cur != 0 {{
            let p = rt_int_to_ptr(cur);
            rt_mem_store(p, op);
            rt_free(p);
            rt_vec_set(slots, slot, 0);
        }} else {{
            rng = (rng * 1103515245 + 12345) & 2147483647;
            let size = 16 + (rng % 8) * 2048;
            let p = rt_alloc(size);
            rt_mem_store(p, op);
            let off = rt_ptr_to_int(p) - base;
            if off > max_off {{ max_off = off; }}
            rt_vec_set(slots, slot, rt_ptr_to_int(p));
        }}
        op = op + 1;
    }}
    // Release everything the sequence left live.
    let mut j = 0;
    while j < 64 {{
        let cur = rt_vec_get(slots, j);
        if cur != 0 {{ rt_free(rt_int_to_ptr(cur)); }}
        j = j + 1;
    }}
    rt_vec_free(slots);
    // After the chaos, one large request must still be served.
    let after = rt_alloc(65536);
    rt_mem_store(after, 1);
    let mut ok = 0;
    if rt_mem_load(after) == 1 {{ ok = 1; }}
    rt_free(after);
    rt_free(marker);
    rt_print_int(max_off);
    rt_print_int(ok);
    rt_exit(0);"#
    );
    let (code, out, err) = build_and_run("random", &body);
    assert_clean("randomized sequence", code, &out, &err);
    let v = ints(&out);
    assert_eq!(v.len(), 2, "stdout={out:?}");
    assert!(
        v[0] < 8 * 1024 * 1024,
        "40,000 random ops over 64 slots of at most ~14 KiB must stay inside a \
         few MiB; high-water offset was {}",
        v[0]
    );
    assert_eq!(v[1], 1);
}

// ============================================================================
// 9. Near-capacity behaviour and deterministic exhaustion
// ============================================================================

#[test]
fn ac09_near_capacity_reuse_and_exhaustion() {
    // A 512 MiB block round-trips at both ends, is released, and is reused.
    let body = format!(
        r#"{MARKER}
    let big = rt_alloc(536870912);
    rt_mem_store(big, 42);
    rt_mem_store(big + 536870904, 43);
    let mut ok = 0;
    if rt_mem_load(big) == 42 {{ ok = 1; }}
    rt_free(big);
    let again = rt_alloc(536870912);
    let mut same = 0;
    if rt_ptr_to_int(again) == rt_ptr_to_int(big) {{ same = 1; }}
    rt_mem_store(again + 536870904, 44);
    let mut ok2 = 0;
    if rt_mem_load(again + 536870904) == 44 {{ ok2 = 1; }}
    rt_free(again);
    rt_free(marker);
    rt_print_int(ok);
    rt_print_int(same);
    rt_print_int(ok2);
    rt_exit(0);"#
    );
    let (code, out, err) = build_and_run("nearcap", &body);
    assert_clean("near capacity", code, &out, &err);
    assert_eq!(
        ints(&out),
        vec![1, 1, 1],
        "half-arena block round trip/reuse failed: {out:?}"
    );

    // A request above the ceiling fails before committing anything, and the
    // failure is the documented structured error (exit 102 = E-R02).
    let over = r#"
    let p = rt_alloc(2147483648);
    rt_free(p);
    rt_exit(0);"#;
    let (code, out, err) = build_and_run("overcap", over);
    assert_eq!(code, 102, "expected E-R02; stdout={out:?} stderr={err:?}");
    assert!(err.contains("E-R02"), "expected E-R02: {err:?}");
}

// ============================================================================
// 10. Invalid and duplicate free rejection
// ============================================================================

#[test]
fn ac10_duplicate_free_is_rejected() {
    let body = r#"
    let p = rt_alloc(64);
    rt_free(p);
    rt_free(p);
    rt_exit(0);"#;
    let (code, out, err) = build_and_run("doublefree", body);
    assert_eq!(code, 104, "expected E-R04; stdout={out:?} stderr={err:?}");
    assert!(err.contains("E-R04"), "expected E-R04: {err:?}");
}

#[test]
fn ac10_interior_free_is_rejected() {
    let body = r#"
    let p = rt_alloc(64);
    rt_free(p + 16);
    rt_exit(0);"#;
    let (code, out, err) = build_and_run("interiorfree", body);
    assert_eq!(code, 104, "expected E-R04; stdout={out:?} stderr={err:?}");
}

#[test]
fn ac10_misaligned_free_is_rejected() {
    let body = r#"
    let p = rt_alloc(64);
    rt_free(p + 8);
    rt_exit(0);"#;
    let (code, out, err) = build_and_run("misalignedfree", body);
    assert_eq!(code, 107, "expected E-R07; stdout={out:?} stderr={err:?}");
}

// ============================================================================
// 11. Interior-pointer access
// ============================================================================

#[test]
fn ac11_interior_access_is_validated_per_block() {
    let body = format!(
        r#"{MARKER}
    let p = rt_alloc(1024);
    rt_mem_store(p, 1);
    rt_mem_store(p + 512, 2);
    rt_mem_store(p + 1016, 3);
    let sum = rt_mem_load(p) + rt_mem_load(p + 512) + rt_mem_load(p + 1016);
    rt_free(p);
    // Reuse the block for a much smaller request: the recorded extent shrinks
    // with it, so interior access keeps tracking the live block, not the old
    // (reused) capacity.
    let q = rt_alloc(32);
    rt_mem_store(q, 5);
    let mut ok = 0;
    if rt_mem_load(q) == 5 {{ ok = 1; }}
    rt_free(q);
    rt_free(marker);
    rt_print_int(sum);
    rt_print_int(ok);
    rt_exit(0);"#
    );
    let (code, out, err) = build_and_run("interior", &body);
    assert_clean("interior access", code, &out, &err);
    assert_eq!(ints(&out), vec![6, 1], "interior words must round-trip");

    // One word past a live block is out of bounds.
    let body = r#"
    let p = rt_alloc(1024);
    rt_mem_store(p + 1024, 1);
    rt_exit(0);"#;
    let (code, out, err) = build_and_run("pastend", body);
    assert_eq!(code, 105, "expected E-R05; stdout={out:?} stderr={err:?}");
}

// ============================================================================
// 12. String allocation and mutation under churn
// ============================================================================

#[test]
fn ac12_string_churn_mutates_and_reuses() {
    let mut body = String::from(MARKER);
    body.push_str(
        r#"
    let mut i = 0;
    let mut max_off = 0;
    let mut bad = 0;
    while i < 20000 {
        let size = 16 + (i % 6) * 100;
        let s = rt_str_alloc(size);
        rt_str_set_byte(s, 0, 65);
        rt_str_set_byte(s, size - 1, 90);
        if rt_str_byte(s, 0) != 65 { bad = bad + 1; }
        if rt_str_byte(s, size - 1) != 90 { bad = bad + 1; }
        if rt_str_len(s) != size { bad = bad + 1; }
        rt_str_free(s);
        // The probe measures where the next block comes from: a leaking
        // string loop walks it up by the string size every round.
        let probe = rt_alloc(16);
        let off = rt_ptr_to_int(probe) - base;
        if off > max_off { max_off = off; }
        rt_free(probe);
        i = i + 1;
    }
    rt_free(marker);
    rt_print_int(bad);
    rt_print_int(max_off);
    rt_exit(0);"#,
    );
    let (code, out, err) = build_and_run("stringchurn", &body);
    assert_clean("string churn", code, &out, &err);
    let v = ints(&out);
    assert_eq!(v.len(), 2, "stdout={out:?}");
    assert_eq!(v[0], 0, "every string read/write must round-trip");
    assert!(
        v[1] < 64 * 1024,
        "six string size classes must share one extent; high-water {}",
        v[1]
    );
}

// ============================================================================
// 13. Collection allocation and mutation under churn
// ============================================================================

#[test]
fn ac13_collection_churn_keeps_values() {
    let mut body = String::from(MARKER);
    body.push_str(
        r#"
    let mut round = 0;
    let mut max_off = 0;
    let mut bad = 0;
    while round < 5000 {
        let mut v = rt_vec_new(4);
        let mut k = 0;
        while k < 32 {
            // `rt_vec_push` returns the (possibly moved) handle; keeping the
            // old one would make the next call a stale-handle access.
            v = rt_vec_push(v, k * 3);
            k = k + 1;
        }
        if rt_vec_len(v) != 32 { bad = bad + 1; }
        let mut j = 0;
        while j < 32 {
            if rt_vec_get(v, j) != j * 3 { bad = bad + 1; }
            j = j + 1;
        }
        rt_vec_free(v);
        let probe = rt_alloc(16);
        let off = rt_ptr_to_int(probe) - base;
        if off > max_off { max_off = off; }
        rt_free(probe);
        round = round + 1;
    }
    rt_free(marker);
    rt_print_int(bad);
    rt_print_int(max_off);
    rt_exit(0);"#,
    );
    let (code, out, err) = build_and_run("vecchurn", &body);
    assert_clean("collection churn", code, &out, &err);
    let v = ints(&out);
    assert_eq!(v.len(), 2, "stdout={out:?}");
    assert_eq!(v[0], 0, "collection contents must survive the reuse");
    assert!(
        v[1] < 1024 * 1024,
        "5,000 collection cycles must not grow the arena; high-water {}",
        v[1]
    );
}

// ============================================================================
// 14. Compression and ZIP compatibility under churn
// ============================================================================

#[test]
fn ac14_zlib_round_trip_after_churn() {
    let mut body = String::from(MARKER);
    body.push_str(
        r#"
    // Mixed-size churn first, so the codec runs on a heap whose free list has
    // been split and coalesced many times.
    let mut i = 0;
    while i < 5000 {
        let a = rt_alloc(65536);
        rt_free(a);
        let b = rt_alloc(1024);
        rt_free(b);
        i = i + 1;
    }
    let payload = rt_str_alloc(2048);
    let mut j = 0;
    while j < 2048 {
        rt_str_set_byte(payload, j, 65 + (j % 26));
        j = j + 1;
    }
    // Both codec entry points take ownership of their input, so the expected
    // payload is rebuilt for the comparison instead of being reused.
    let z = zlib_compress(payload, 6);
    let (back, e) = zlib_decompress(z);
    let expect = rt_str_alloc(2048);
    let mut j2 = 0;
    while j2 < 2048 {
        rt_str_set_byte(expect, j2, 65 + (j2 % 26));
        j2 = j2 + 1;
    }
    let mut same = 0;
    if rt_str_len(back) == 2048 {
        let mut k = 0;
        let mut good = 1;
        while k < 2048 {
            if rt_str_byte(back, k) != rt_str_byte(expect, k) { good = 0; }
            k = k + 1;
        }
        same = good;
    }
    rt_str_free(back);
    rt_str_free(expect);
    rt_free(marker);
    rt_print_int(e);
    rt_print_int(same);
    rt_exit(0);"#,
    );
    let (code, out, err) = build_and_run_libs("zlibchurn", &["zlib"], &body);
    assert_clean("zlib after churn", code, &out, &err);
    assert_eq!(
        ints(&out),
        vec![0, 1],
        "zlib round trip must survive a fragmented heap: {out:?}"
    );
}

/// Regression for the defect the phase-D allocator exposed: `_z_h_build`
/// indexed its 16-word length table with a code length it had already
/// rejected, and `_z_block_dynamic` built tables from a length arena whose
/// decode had failed. With capacity-preserving reuse that arena holds the
/// previous owner's data, so the combination was a live out-of-bounds access
/// (`E-R05`) instead of a reported decode error.
#[test]
fn ac14_zlib_malformed_stream_after_prior_decode() {
    let mut body = String::from(MARKER);
    body.push_str(
        r#"
    // A structurally-valid decode that fails on its truncated trailer leaves
    // the heap full of released arenas...
    let a = rt_str_alloc(7);
    rt_str_set_byte(a, 0, 120);
    rt_str_set_byte(a, 1, 218);
    rt_str_set_byte(a, 2, 3);
    rt_str_set_byte(a, 3, 0);
    rt_str_set_byte(a, 4, 0);
    rt_str_set_byte(a, 5, 0);
    rt_str_set_byte(a, 6, 0);
    let (s0, e0) = zlib_decompress(a);
    rt_str_free(s0);
    // ...which the malformed dynamic-block stream below must not trust.
    let b = rt_str_alloc(9);
    rt_str_set_byte(b, 0, 5);
    rt_str_set_byte(b, 1, 0);
    rt_str_set_byte(b, 2, 0);
    rt_str_set_byte(b, 3, 0);
    rt_str_set_byte(b, 4, 0);
    rt_str_set_byte(b, 5, 0);
    rt_str_set_byte(b, 6, 0);
    rt_str_set_byte(b, 7, 0);
    rt_str_set_byte(b, 8, 0);
    let (s1, e1) = inflate_raw(b);
    rt_str_free(s1);
    rt_free(marker);
    rt_print_int(e0);
    rt_print_int(e1);
    rt_exit(0);"#,
    );
    let (code, out, err) = build_and_run_libs("zlibmalformed", &["zlib"], &body);
    assert_clean("zlib malformed stream", code, &out, &err);
    assert_eq!(
        ints(&out),
        vec![-7, -5],
        "the decoder must report its structured errors, never touch memory \
         outside its arenas: {out:?}"
    );
}

#[test]
fn ac14_zip_round_trip_after_churn() {
    let mut body = String::from(MARKER);
    body.push_str(
        r#"
    let mut i = 0;
    while i < 5000 {
        let a = rt_alloc(32768);
        rt_free(a);
        let b = rt_alloc(512);
        rt_free(b);
        i = i + 1;
    }
    let a = zip_add(zip_new(), "x.txt", "hello world");
    let arc = zip_finish(a);
    let (n, back) = zip_read_count(arc);
    let d = zip_entry_data_by_name(back, "x.txt");
    let mut same = 0;
    if rt_str_eq(d.0, "hello world") { same = 1; }
    rt_str_free(d.0);
    rt_free(marker);
    rt_print_int(n);
    rt_print_int(d.1);
    rt_print_int(same);
    rt_exit(0);"#,
    );
    let (code, out, err) = build_and_run_libs("zipchurn", &["zip"], &body);
    assert_clean("zip after churn", code, &out, &err);
    assert_eq!(
        ints(&out),
        vec![1, 0, 1],
        "ZIP round trip must survive a fragmented heap: {out:?}"
    );
}

// ============================================================================
// 15. SQLite compatibility under churn
// ============================================================================

#[test]
fn ac15_sqlite_round_trip_after_churn() {
    let mut body = String::from(MARKER);
    body.push_str(
        r#"
    let mut i = 0;
    while i < 5000 {
        let a = rt_alloc(16384);
        rt_free(a);
        let b = rt_alloc(256);
        rt_free(b);
        i = i + 1;
    }
    let mut db = sql_new();
    let r1 = sql_exec(db, "CREATE TABLE users (id INT, name STR)");
    db = r1.1;
    rt_str_free(r1.0);
    let r2 = sql_exec(db, "INSERT INTO users VALUES (7, 'seven')");
    db = r2.1;
    rt_str_free(r2.0);
    let r3 = sql_exec(db, "SELECT * FROM users");
    db = r3.1;
    rt_print_str(r3.0);
    rt_str_free(r3.0);
    sql_free(db);
    rt_free(marker);
    rt_exit(0);"#,
    );
    let (code, out, err) = build_and_run_libs("sqlitechurn", &["sqlite"], &body);
    assert_clean("sqlite after churn", code, &out, &err);
    assert!(
        out.contains('7') && out.contains("seven"),
        "the row must round-trip after a fragmented heap: {out:?}"
    );
}

// ============================================================================
// 16. Long mixed-size soak with concurrent allocation activity
// ============================================================================

/// Deliberately not part of the default run: it keeps a machine busy for a
/// minute. Run it explicitly (Gate F):
///
/// ```text
/// cargo test --release --test allocator_conformance -- --ignored --nocapture
/// ```
///
/// Four worker threads and the main thread each churn mixed-size blocks out of
/// the shared heap until the same deadline passes. Every block is written at
/// both ends and re-read before it is released, so overlapping allocations or
/// a stale free-list entry would be counted as a failure rather than silently
/// passing. Exit 0 also runs the runtime leak check.
#[test]
#[ignore]
fn ac16_mixed_size_soak_60s() {
    let source = r#"
mod time;
mod threads;

// Worker argument block: [0] deadline, [8] completed iterations.
fn worker_mix(p: Ptr<Int>) -> Int {
    let deadline = rt_mem_load(p);
    let mut bad = 0;
    let mut i = 0;
    while time_millis() < deadline {
        let a = rt_alloc(65536);
        rt_mem_store(a, i);
        rt_mem_store(a + 65528, i);
        let b = rt_alloc(1024);
        rt_mem_store(b, i + 1);
        let c = rt_alloc(4096);
        rt_mem_store(c, i + 2);
        if rt_mem_load(a) != i { bad = bad + 1; }
        if rt_mem_load(a + 65528) != i { bad = bad + 1; }
        rt_free(a);
        if rt_mem_load(b) != i + 1 { bad = bad + 1; }
        rt_free(c);
        rt_free(b);
        i = i + 1;
    }
    rt_mem_store(p + 8, i);
    return bad;
}

fn main() {
    let marker = rt_alloc(16);
    let base = rt_ptr_to_int(marker);
    let deadline = time_millis() + 60000;
    let mut handles = rt_vec_new(8);
    let mut args = rt_vec_new(8);
    let mut w = 0;
    while w < 4 {
        let arg = rt_alloc(16);
        rt_mem_store(arg, deadline);
        rt_mem_store(arg + 8, 0);
        args = rt_vec_push(args, rt_ptr_to_int(arg));
        handles = rt_vec_push(handles, thread_spawn(worker_mix, arg));
        w = w + 1;
    }
    let mut rng = 123456789;
    let mut slots = rt_vec_new(64);
    let mut i = 0;
    while i < 64 { slots = rt_vec_push(slots, 0); i = i + 1; }
    let mut ops = 0;
    let mut bad = 0;
    let mut max_off = 0;
    while time_millis() < deadline {
        rng = (rng * 1103515245 + 12345) & 2147483647;
        let slot = rng % 64;
        let cur = rt_vec_get(slots, slot);
        if cur != 0 {
            let p = rt_int_to_ptr(cur);
            rt_mem_store(p, 0);
            rt_free(p);
            rt_vec_set(slots, slot, 0);
        } else {
            rng = (rng * 1103515245 + 12345) & 2147483647;
            let size = 16 + (rng % 64) * 1024;
            let p = rt_alloc(size);
            rt_mem_store(p, rng);
            rt_mem_store(p + size - 16, rng);
            if rt_mem_load(p + size - 16) != rng { bad = bad + 1; }
            let off = rt_ptr_to_int(p) - base;
            if off > max_off { max_off = off; }
            rt_vec_set(slots, slot, rt_ptr_to_int(p));
        }
        ops = ops + 1;
    }
    let mut j = 0;
    while j < 64 {
        let cur = rt_vec_get(slots, j);
        if cur != 0 { rt_free(rt_int_to_ptr(cur)); }
        j = j + 1;
    }
    let mut joined = 0;
    let mut worker_ops = 0;
    let mut worker_bad = 0;
    let mut k = 0;
    while k < 4 {
        worker_bad = worker_bad + thread_join(rt_vec_get(handles, k));
        worker_ops = worker_ops + rt_mem_load(rt_int_to_ptr(rt_vec_get(args, k)) + 8);
        rt_free(rt_int_to_ptr(rt_vec_get(args, k)));
        joined = joined + 1;
        k = k + 1;
    }
    rt_vec_free(slots);
    rt_vec_free(args);
    rt_vec_free(handles);
    rt_free(marker);
    rt_print_int(joined);
    rt_print_int(ops);
    rt_print_int(worker_ops);
    rt_print_int(bad + worker_bad);
    rt_print_int(max_off);
    rt_exit(0);
}
"#;
    let (code, out, err, elapsed) = build_and_run_source("soak60", source);
    assert_clean("60 s mixed soak", code, &out, &err);
    let v = ints(&out);
    assert_eq!(v.len(), 5, "stdout={out:?}");
    assert_eq!(v[0], 4, "all four worker threads must join");
    // The program's deadline is `time_millis() + 60_000` and the clock's tick
    // is truncated to whole milliseconds, so a full soak can finish a fraction
    // of a millisecond before this harness' 60 s wall mark (measured: 59.99 s).
    // 59.5 s still rules out an early exit while tolerating that tick rounding.
    assert!(
        elapsed >= 59.5,
        "the soak must run for its full 60 s window; it took {elapsed:.2}s"
    );
    assert!(v[1] > 0, "the main thread must have done work");
    assert!(v[2] > 0, "the workers must have done work");
    assert_eq!(v[3], 0, "every mixed-size block must round-trip unchanged");
    assert!(
        v[4] < 256 * 1024 * 1024,
        "generation retirement is the only legitimate growth; the main \
         thread's high-water offset was {} bytes",
        v[4]
    );
    println!(
        "60 s soak: main_ops={} worker_ops={} high_water={}B elapsed={elapsed:.2}s",
        v[1], v[2], v[4]
    );
}
