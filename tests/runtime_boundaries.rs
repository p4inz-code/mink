//! M7 Phase E — runtime lifecycle and failure-boundary conformance corpus.
//!
//! Phase D pinned the allocator's *reuse* behaviour (mixed-size churn, splits,
//! coalescing). This corpus pins the boundaries around it: generation
//! retirement, the 1 GiB arena ceiling, the 1,024-live-allocation table, a
//! fragmented free list, and concurrent mixed-size traffic. Every case is a
//! real Windows PE compiled by the CLI and executed as a separate process, so
//! a clean exit also runs the runtime leak check (`E-R06`).
//!
//! The cases are deliberately about failure *behaviour*, not just success:
//! exhausting the arena must be `E-R02`/102, exhausting the liveness table
//! `E-R03`/103, and a stale collection handle must stay `E-R05`/105 however
//! many times its block was reused. Address comparisons are used instead of
//! arena offsets where a block can live in the growth region: growth blocks are
//! served by a separate `VirtualAlloc` region whose address need not be above
//! the `.bss` prefix, so `ptr - base` is only meaningful inside the prefix.

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn artifact_dir() -> PathBuf {
    let dir = PathBuf::from("target")
        .join("mink-artifacts")
        .join("runtime_boundaries");
    std::fs::create_dir_all(&dir).expect("failed to create the artifact directory");
    dir
}

/// Compiles `source`, returning the executable path. On a build failure the
/// source is kept for inspection.
fn build(tag: &str, source: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = artifact_dir().join(format!("boundary_{tag}_{n}.mink"));
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

fn build_and_run(tag: &str, body: &str) -> (i32, String, String) {
    let source = format!("fn main() {{\n{body}\n}}\n");
    let exe = build(tag, &source);
    let (code, out, err) = run(&exe);
    let _ = std::fs::remove_file(&exe);
    (code, out, err)
}

fn build_and_run_source(tag: &str, source: &str) -> (i32, String, String) {
    let exe = build(tag, source);
    let (code, out, err) = run(&exe);
    let _ = std::fs::remove_file(&exe);
    (code, out, err)
}

fn ints(out: &str) -> Vec<i64> {
    out.split_whitespace()
        .filter_map(|t| t.parse::<i64>().ok())
        .collect()
}

fn assert_clean(tag: &str, code: i32, out: &str, err: &str) {
    assert_eq!(code, 0, "{tag}: exit {code}; stdout={out:?} stderr={err:?}");
    assert!(err.is_empty(), "{tag}: unexpected stderr: {err:?}");
}

/// The largest single block a fresh heap can serve: the growth region
/// (`HEAP_MAX_BYTES - HEAP_BSS_BYTES`). A block that would straddle the
/// `.bss`/growth boundary is relocated to the start of the growth region, so
/// the whole `.bss` prefix tail is unavailable to it.
const MAX_BLOCK: i64 = 1024 * 1024 * 1024 - 4 * 1024 * 1024;

// ============================================================================
// 1. Generation retirement
// ============================================================================

/// A 64-byte block is reused until its 16-bit generation is exhausted, then
/// retired rather than reissued. 400,000 churn cycles therefore consume exactly
/// seven distinct blocks — six retirements — and every cycle's contents
/// round-trip. The block address never moves backwards: a retired block is
/// never handed out again.
#[test]
fn pe01_generation_retirement_is_bounded_and_monotonic() {
    let body = r#"
    let marker = rt_alloc(16);
    let base = rt_ptr_to_int(marker);
    let mut changes = 0;
    let mut prev = 0 - 1;
    let mut maxoff = 0;
    let mut bad = 0;
    let mut sum = 0;
    let mut i = 0;
    while i < 400000 {
        let p = rt_alloc(64);
        rt_mem_store(p, i);
        let off = rt_ptr_to_int(p) - base;
        if off != prev {
            changes = changes + 1;
            if off < prev { bad = bad + 1; }
            prev = off;
            if off > maxoff { maxoff = off; }
        }
        let v = rt_mem_load(p);
        if v != i { bad = bad + 1; }
        sum = sum + v;
        rt_free(p);
        i = i + 1;
    }
    rt_print_int(changes);
    rt_print_int(maxoff);
    rt_print_int(bad);
    rt_print_int(sum);
    rt_free(marker);
    rt_exit(0);"#;
    let (code, out, err) = build_and_run("generation_cadence", body);
    assert_clean("generation retirement cadence", code, &out, &err);
    // 400,000 = 6 full 65,535-reuse lifetimes + a remainder: 7 blocks, 6
    // retirements, each new block 64 bytes above the previous one.
    assert_eq!(
        ints(&out),
        vec![7, 400, 0, 79_999_800_000],
        "generation retirement must be exactly bounded: {out:?}"
    );
}

/// After *two* retirement boundaries the original collection handle must still
/// be rejected: a retired block is never linked into the free list, so no later
/// occupant can alias the stale generation.
#[test]
fn pe02_stale_handle_after_two_retirements_stays_stale() {
    let body = r#"
    let first = rt_vec_new(4);
    let h = rt_vec_push(first, 11);
    rt_vec_free(h);
    let mut i = 0;
    while i < 140000 {
        let v = rt_vec_new(4);
        rt_vec_free(v);
        i = i + 1;
    }
    let live = rt_vec_new(4);
    let x = rt_vec_get(first, 0);
    rt_print_int(x);
    rt_vec_free(live);
    rt_exit(0);"#;
    let (code, out, err) = build_and_run("stale_two_boundaries", body);
    assert_eq!(
        code, 105,
        "a stale handle must stay stale across every retirement; stdout={out:?} stderr={err:?}"
    );
    assert!(err.contains("E-R05"), "expected E-R05 diagnostics: {err:?}");
}

// ============================================================================
// 2. Arena ceiling
// ============================================================================

/// A single block of the whole growth region (1020 MiB) is the largest the
/// heap serves. Both ends are writable, the block is reused at the same address
/// after release, and a following small allocation is split off its front —
/// the arena is still consistent at its maximum.
#[test]
fn pe03_max_block_round_trip_and_reuse() {
    let body = format!(
        r#"
    let marker = rt_alloc(16);
    let big = {MAX_BLOCK};
    let a = rt_alloc(big);
    rt_mem_store(a, 111);
    rt_mem_store(a + big - 16, 222);
    let head = rt_ptr_to_int(a);
    rt_free(a);
    let b = rt_alloc(big);
    let mut same = 0;
    if rt_ptr_to_int(b) == head {{ same = 1; }}
    rt_mem_store(b, 333);
    let mut ends = 0;
    if rt_mem_load(b) == 333 {{ ends = 1; }}
    rt_print_int(same);
    rt_print_int(ends);
    rt_free(b);
    let small = rt_alloc(16);
    let mut front = 0;
    if rt_ptr_to_int(small) == head {{ front = 1; }}
    rt_print_int(front);
    rt_free(small);
    rt_free(marker);
    rt_exit(0);"#
    );
    let (code, out, err) = build_and_run("max_block", &body);
    assert_clean("maximum block round trip", code, &out, &err);
    assert_eq!(
        ints(&out),
        vec![1, 1, 1],
        "the 1020 MiB block must round-trip, be reused in place and split at its front: {out:?}"
    );
}

/// One more 16-byte block after the maximum does not fit: the boundary is a
/// deterministic `E-R02`, not a wrap or an overlapping allocation.
#[test]
fn pe04_arena_ceiling_exhaustion_is_er02() {
    let body = format!(
        r#"
    let big = rt_alloc({MAX_BLOCK});
    rt_mem_store(big, 1);
    let extra = rt_alloc(16);
    rt_mem_store(extra, 2);
    rt_free(extra);
    rt_free(big);
    rt_exit(0);"#
    );
    let (code, out, err) = build_and_run("ceiling_exhausted", &body);
    assert_eq!(
        code, 102,
        "allocation past 1 GiB must be E-R02; stdout={out:?} stderr={err:?}"
    );
    assert!(err.contains("E-R02"), "expected E-R02 diagnostics: {err:?}");
}

/// Two half-arena blocks cannot coexist even though each is half of the 1 GiB
/// logical ceiling. The first would straddle the 4 MiB `.bss`/growth boundary,
/// so it is placed at the start of the growth region and the prefix tail is
/// unusable; the second then crosses `HEAP_MAX_BYTES`. This 4 MiB (0.39%)
/// capacity cost is recorded behaviour, and it fails loudly.
#[test]
fn pe05_prefix_straddle_bounds_two_half_arena_blocks() {
    let body = r#"
    let a = rt_alloc(536870912);
    rt_mem_store(a, 1);
    let b = rt_alloc(536870912);
    rt_mem_store(b, 2);
    rt_print_int(rt_mem_load(a) + rt_mem_load(b));
    rt_free(a);
    rt_free(b);
    rt_exit(0);"#;
    let (code, out, err) = build_and_run("two_halves", body);
    assert_eq!(
        code, 102,
        "two 512 MiB blocks must not exceed the usable arena; stdout={out:?} stderr={err:?}"
    );
    assert!(err.contains("E-R02"), "expected E-R02 diagnostics: {err:?}");
}

// ============================================================================
// 3. Live-allocation table
// ============================================================================

/// With all 1,024 slots live, freeing 511 mixed blocks and allocating 511 new
/// ones must succeed by reusing their slots; the 1,025th live allocation is
/// then `E-R03`/103. Contents of the live half are untouched throughout.
#[test]
fn pe06_live_allocation_limit_reuses_freed_slots() {
    let body = r#"
    let marker = rt_alloc(16);
    let mut v = rt_vec_new(2048);
    let mut i = 0;
    while i < 1022 {
        let p = rt_alloc(16);
        rt_mem_store(p, i + 1);
        v = rt_vec_push(v, rt_ptr_to_int(p));
        i = i + 1;
    }
    // 1,024 live: marker + vector + 1,022 blocks.
    let mut j = 0;
    while j < 1022 {
        if j % 2 == 0 {
            let p = rt_int_to_ptr(rt_vec_get(v, j));
            if rt_mem_load(p) != j + 1 { rt_exit(2); }
            rt_free(p);
        }
        j = j + 1;
    }
    // 511 holes: the next 511 allocations must consume their slots.
    let mut k = 0;
    while k < 511 {
        let p = rt_alloc(16);
        rt_mem_store(p, 900000 + k);
        v = rt_vec_push(v, rt_ptr_to_int(p));
        k = k + 1;
    }
    let mut checks = 0;
    let mut m = 0;
    while m < 511 {
        let p = rt_int_to_ptr(rt_vec_get(v, 1022 + m));
        if rt_mem_load(p) == 900000 + m { checks = checks + 1; }
        m = m + 1;
    }
    rt_print_int(checks);
    // 1,024 live again: this one must be E-R03.
    let overflow = rt_alloc(16);
    rt_mem_store(overflow, 1);
    rt_exit(0);"#;
    let (code, out, err) = build_and_run("table_limit", body);
    assert_eq!(
        code, 103,
        "the 1,025th live allocation must be E-R03; stdout={out:?} stderr={err:?}"
    );
    assert!(err.contains("E-R03"), "expected E-R03 diagnostics: {err:?}");
    assert_eq!(
        ints(&out),
        vec![511],
        "all 511 replacement blocks must be readable: {out:?}"
    );
}

// ============================================================================
// 4. Free-list behaviour under fragmentation
// ============================================================================

/// 128 adjacent 4 KiB blocks, every other one freed: 64 non-adjacent holes.
/// The next 64 blocks of 4 KiB are all served from those holes (a first-fit
/// walk of a fragmented list), the live neighbours are untouched, a 256 KiB
/// request cannot use the holes and bumps, and after everything is released the
/// coalesced region serves it from the freed area.
#[test]
fn pe07_fragmented_free_list_first_fit_and_coalescing() {
    let body = r#"
    let marker = rt_alloc(16);
    let mut v = rt_vec_new(300);
    let mut i = 0;
    let mut max_addr = 0;
    while i < 128 {
        let p = rt_alloc(4096);
        rt_mem_store(p, i + 1);
        rt_mem_store(p + 4080, i + 1);
        let a = rt_ptr_to_int(p);
        if a > max_addr { max_addr = a; }
        v = rt_vec_push(v, a);
        i = i + 1;
    }
    let mut j = 0;
    while j < 128 {
        if j % 2 == 0 { rt_free(rt_int_to_ptr(rt_vec_get(v, j))); }
        j = j + 1;
    }
    let mut k = 0;
    let mut reused = 0;
    while k < 64 {
        let p = rt_alloc(4096);
        rt_mem_store(p, 7000 + k);
        if rt_ptr_to_int(p) <= max_addr { reused = reused + 1; }
        v = rt_vec_push(v, rt_ptr_to_int(p));
        k = k + 1;
    }
    let mut intact = 0;
    let mut m = 1;
    while m < 128 {
        let p = rt_int_to_ptr(rt_vec_get(v, m));
        if rt_mem_load(p) == m + 1 && rt_mem_load(p + 4080) == m + 1 { intact = intact + 1; }
        m = m + 2;
    }
    let big = rt_alloc(262144);
    rt_mem_store(big, 424242);
    let mut fresh = 0;
    if rt_ptr_to_int(big) > max_addr { fresh = 1; }
    rt_print_int(reused);
    rt_print_int(intact);
    rt_print_int(fresh);
    rt_free(big);
    let mut n = 1;
    while n < 128 {
        rt_free(rt_int_to_ptr(rt_vec_get(v, n)));
        n = n + 2;
    }
    let mut q = 128;
    while q < 192 {
        rt_free(rt_int_to_ptr(rt_vec_get(v, q)));
        q = q + 1;
    }
    rt_vec_free(v);
    let again = rt_alloc(262144);
    rt_mem_store(again + 262128, 99);
    let mut back = 0;
    if rt_ptr_to_int(again) == rt_ptr_to_int(marker) + 16 { back = 1; }
    if rt_mem_load(again + 262128) != 99 { back = 0; }
    rt_print_int(back);
    rt_free(again);
    rt_free(marker);
    rt_exit(0);"#;
    let (code, out, err) = build_and_run("fragmented", body);
    assert_clean("fragmented free list", code, &out, &err);
    assert_eq!(
        ints(&out),
        vec![64, 64, 1, 1],
        "hole reuse, neighbour integrity, bump for the large block and coalesced reuse: {out:?}"
    );
}

// ============================================================================
// 5. Concurrency
// ============================================================================

/// Four threads churn three sizes each under the runtime spin lock while the
/// main thread starts and joins them. Every block is written at both ends and
/// re-read before release, so a double-handed-out block, a torn free-list link
/// or a lost wakeup surfaces as a mismatch or a hang rather than a pass. Exit 0
/// also proves the leak check found nothing.
#[test]
fn pe08_concurrent_mixed_size_alloc_free_integrity() {
    let source = r#"
mod threads;

// [0] iterations, [8] mismatches, [16] completed
fn worker(p: Ptr<Int>) -> Int {
    let n = rt_mem_load(p);
    let mut i = 0;
    let mut bad = 0;
    while i < n {
        let sa = 2048 + (i % 8) * 2048;
        let a = rt_alloc(sa);
        let b = rt_alloc(64);
        let c = rt_alloc(4096);
        rt_mem_store(a, i + 1);
        rt_mem_store(a + sa - 16, i + 2);
        rt_mem_store(b, i + 3);
        rt_mem_store(c, i + 4);
        rt_mem_store(c + 4080, i + 5);
        if rt_mem_load(a) != i + 1 { bad = bad + 1; }
        if rt_mem_load(a + sa - 16) != i + 2 { bad = bad + 1; }
        if rt_mem_load(b) != i + 3 { bad = bad + 1; }
        if rt_mem_load(c) != i + 4 { bad = bad + 1; }
        if rt_mem_load(c + 4080) != i + 5 { bad = bad + 1; }
        if i % 2 == 0 {
            rt_free(a); rt_free(c); rt_free(b);
        } else {
            rt_free(b); rt_free(c); rt_free(a);
        }
        i = i + 1;
    }
    rt_mem_store(p + 8, bad);
    rt_mem_store(p + 16, i);
    return bad;
}

fn main() {
    let mut handles = rt_vec_new(8);
    let mut args = rt_vec_new(8);
    let mut w = 0;
    while w < 4 {
        let arg = rt_alloc(24);
        rt_mem_store(arg, 20000);
        rt_mem_store(arg + 8, 0);
        rt_mem_store(arg + 16, 0);
        args = rt_vec_push(args, rt_ptr_to_int(arg));
        handles = rt_vec_push(handles, thread_spawn(worker, arg));
        w = w + 1;
    }
    let mut joined = 0;
    let mut bad = 0;
    let mut done = 0;
    let mut k = 0;
    while k < 4 {
        bad = bad + thread_join(rt_vec_get(handles, k));
        done = done + rt_mem_load(rt_int_to_ptr(rt_vec_get(args, k)) + 16);
        rt_free(rt_int_to_ptr(rt_vec_get(args, k)));
        joined = joined + 1;
        k = k + 1;
    }
    rt_vec_free(args);
    rt_vec_free(handles);
    rt_print_int(joined);
    rt_print_int(done);
    rt_print_int(bad);
    rt_exit(0);
}
"#;
    let (code, out, err) = build_and_run_source("concurrent_mixed", source);
    assert_clean("concurrent mixed-size traffic", code, &out, &err);
    assert_eq!(
        ints(&out),
        vec![4, 80_000, 0],
        "all four workers must join with every block intact: {out:?}"
    );
}
