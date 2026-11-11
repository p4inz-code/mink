#!/bin/bash
# Reproducible measurements for deferred cleanup (sessions 271).
#
# Usage: benchmarks/defer/measure.sh [path/to/mink.exe]
#
# Each case is a standalone program compiled with the release compiler. The
# script reports, per case: the native executable size in bytes and the
# `mink build` wall time (minimum of 3 runs, which is dominated by codegen and
# linking rather than by anything defer-specific). The two loop cases also run
# the produced executable 3 times and report the minimum wall time.
#
# Nothing here is a microbenchmark of the optimizer: it measures what the
# feature costs in generated code size for a given number of registrations and
# exit paths, and whether a capture costs anything at run time beyond the
# ordinary alloc/free it performs.

set -u
root="$(cd "$(dirname "$0")/../.." && pwd)"
mink="${1:-$root/target/release/mink.exe}"
work="$root/tmp/defer_bench"
rm -rf "$work"
mkdir -p "$work"

now_ns() { date +%s%N; }
ms() { echo $(( ($2 - $1) / 1000000 )); }

emit() { # emit <name> <source>
    printf '%s\n' "$2" > "$work/$1.mink"
}

# --- cases -----------------------------------------------------------------

emit plain 'fn main() -> Int {
  let s = rt_str_alloc(64);
  rt_print_int(rt_str_len(s));
  rt_str_free(s);
  return 0;
}'

emit one_defer 'fn main() -> Int {
  let s = rt_str_alloc(64);
  defer { rt_print_str("cleanup"); }
  rt_str_free(s);
  return 0;
}'

emit three_defers 'fn main() -> Int {
  let a = rt_str_alloc(64);
  let b = rt_str_alloc(64);
  let c = rt_str_alloc(64);
  rt_print_int(rt_str_len(a) + rt_str_len(b) + rt_str_len(c));
  defer move (a) { rt_str_free(a); }
  defer move (b) { rt_str_free(b); }
  defer move (c) { rt_str_free(c); }
  return 0;
}'

emit nested 'fn main() -> Int {
  let outer = rt_str_alloc(64);
  defer move (outer) { rt_str_free(outer); }
  if true {
    let mid = rt_str_alloc(64);
    defer move (mid) { rt_str_free(mid); }
    if true {
      let inner = rt_str_alloc(64);
      rt_print_int(rt_str_len(inner));
      defer move (inner) { rt_str_free(inner); }
    }
  }
  return 0;
}'

emit returns2 'fn pick(c: Bool) -> Int {
  let s = rt_str_alloc(64);
  defer move (s) { rt_str_free(s); }
  if c { return 1; }
  return 2;
}
fn main() -> Int { return pick(true) + pick(false); }'

emit returns4 'fn pick(c: Int) -> Int {
  let s = rt_str_alloc(64);
  defer move (s) { rt_str_free(s); }
  if c == 0 { return 1; }
  if c == 1 { return 2; }
  if c == 2 { return 3; }
  return 4;
}
fn main() -> Int { return pick(0) + pick(1) + pick(2) + pick(3); }'

emit returns8 'fn pick(c: Int) -> Int {
  let s = rt_str_alloc(64);
  defer move (s) { rt_str_free(s); }
  if c == 0 { return 1; }
  if c == 1 { return 2; }
  if c == 2 { return 3; }
  if c == 3 { return 4; }
  if c == 4 { return 5; }
  if c == 5 { return 6; }
  if c == 6 { return 7; }
  return 8;
}
fn main() -> Int {
  return pick(0) + pick(1) + pick(2) + pick(3) + pick(4) + pick(5) + pick(6) + pick(7);
}'

emit loop_capture 'fn main() -> Int {
  let mut i = 0;
  let mut total = 0;
  while i < 1000000 {
    i += 1;
    let s = rt_str_alloc(16);
    defer move (s) { total += rt_str_len(s); rt_str_free(s); }
  }
  rt_print_int(total);
  return 0;
}'

emit loop_inline 'fn main() -> Int {
  let mut i = 0;
  let mut total = 0;
  while i < 1000000 {
    i += 1;
    let s = rt_str_alloc(16);
    total += rt_str_len(s);
    rt_str_free(s);
  }
  rt_print_int(total);
  return 0;
}'

# --- measurement -----------------------------------------------------------

printf '%-16s %12s %12s %12s\n' case exe_bytes build_ms run_ms
for name in plain one_defer three_defers nested returns2 returns4 returns8 loop_capture loop_inline; do
    src="$work/$name.mink"
    exe="$work/$name.exe"
    best_build=""
    for _ in 1 2 3; do
        rm -f "$exe"
        start=$(now_ns)
        if ! "$mink" build "$src" > "$work/$name.build.log" 2>&1; then
            echo "$name: build failed"; cat "$work/$name.build.log"; break
        fi
        elapsed=$(ms "$start" "$(now_ns)")
        if [ -z "$best_build" ] || [ "$elapsed" -lt "$best_build" ]; then best_build=$elapsed; fi
    done
    [ -f "$exe" ] || continue
    size=$(stat -c %s "$exe")
    run_ms="-"
    case "$name" in
        loop_*)
            best_run=""
            for _ in 1 2 3; do
                start=$(now_ns)
                "$exe" > "$work/$name.run.log" 2>&1
                elapsed=$(ms "$start" "$(now_ns)")
                if [ -z "$best_run" ] || [ "$elapsed" -lt "$best_run" ]; then best_run=$elapsed; fi
            done
            run_ms=$best_run
            ;;
    esac
    printf '%-16s %12s %12s %12s\n' "$name" "$size" "$best_build" "$run_ms"
done

echo
echo "loop outputs:"
for name in loop_capture loop_inline; do
    [ -f "$work/$name.run.log" ] && printf '  %-14s %s\n' "$name" "$(tr -d '\r' < "$work/$name.run.log")"
done
