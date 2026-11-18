#!/usr/bin/env python3
"""M7 Phase E — allocator contention measurement.

Runs `main.exe` for 1, 2, 4 and 8 worker threads, three times each, and reports
alloc/free pairs per second. The workload is a fixed-size mixed allocation loop
against the shared runtime heap, so the curve is the cost of the runtime's
single heap spin lock, not of the scheduler.

    target/release/mink.exe build validation/phase_e/concurrency/main.mink
    python validation/phase_e/concurrency/run.py

The build is part of the evidence: the printed throughput numbers are only
comparable on the same machine and with the same binary.
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
EXE = os.path.join(HERE, "main.exe")
WINDOW_MS = 2000
REPEATS = 3
THREADS = (1, 2, 4, 8)


def run(threads):
    proc = subprocess.run([EXE, str(threads), str(WINDOW_MS)],
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          timeout=120)
    out = proc.stdout.decode("ascii", "replace").split()
    if len(out) != 4:
        raise SystemExit("unexpected output from %s %d: %r (stderr %r)"
                         % (EXE, threads, proc.stdout, proc.stderr))
    joined, ops, bad, elapsed = (int(v) for v in out)
    if proc.returncode != 0 or joined != threads or bad != 0:
        raise SystemExit("run %d threads failed: exit=%d joined=%d bad=%d"
                         % (threads, proc.returncode, joined, bad))
    return ops, elapsed


def main():
    if not os.path.exists(EXE):
        print("missing %s; build it first" % EXE)
        return 1
    print("window=%dms repeats=%d" % (WINDOW_MS, REPEATS))
    print("%8s %14s %14s %10s" % ("threads", "median ops/s", "per-thread", "speedup"))
    base = None
    for threads in THREADS:
        samples = [run(threads) for _ in range(REPEATS)]
        rates = sorted(ops * 1000.0 / elapsed for ops, elapsed in samples)
        median = rates[len(rates) // 2]
        if base is None:
            base = median
        print("%8d %14.0f %14.0f %9.2fx"
              % (threads, median, median / threads, median / base))
    print("\nalloc/free pairs per second, median of %d runs; total throughput "
          "across all threads." % REPEATS)
    return 0


if __name__ == "__main__":
    sys.exit(main())
