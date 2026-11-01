#!/usr/bin/env python3
"""Descriptive statistics and the locked M4 materiality rules.

    python benchmarks/m4/harness/stats.py --selftest

No comparative claim is made here. The functions implement exactly the rules
recorded in `M4_COMPARISON_METHODOLOGY.md`:

* report median, p95, min, max and IQR from the raw samples, never a mean of
  rounded values;
* runtime: a difference is clear only when the median ratio is >= 1.10 **and**
  the two IQRs are disjoint;
* startup: a difference is clear only when it is >= 2 ms **and** >= 10 %;
* memory: a difference is clear only when it is >= 1 MiB **and** >= 10 %.

A "no clear difference" outcome is a legitimate result and is not a tie in
favour of either implementation. No outlier is removed and no composite score
is computed.
"""
import sys

RUNTIME_RATIO = 1.10
STARTUP_ABSOLUTE_MS = 2.0
STARTUP_RELATIVE = 0.10
MEMORY_ABSOLUTE_BYTES = 1024 * 1024
MEMORY_RELATIVE = 0.10


def percentile(sorted_values, fraction):
    """Linear-interpolated percentile on an already sorted sequence."""
    if not sorted_values:
        raise ValueError("percentile of an empty sample")
    if len(sorted_values) == 1:
        return float(sorted_values[0])
    rank = (len(sorted_values) - 1) * fraction
    lower = int(rank)
    upper = min(lower + 1, len(sorted_values) - 1)
    weight = rank - lower
    return float(sorted_values[lower]) * (1.0 - weight) + float(sorted_values[upper]) * weight


def summarize(samples):
    """Median/p95/min/max/IQR of the measured samples, in their own units."""
    if not samples:
        raise ValueError("summarize of an empty sample")
    ordered = sorted(float(value) for value in samples)
    return {
        "n": len(ordered),
        "min": ordered[0],
        "p95": percentile(ordered, 0.95),
        "median": percentile(ordered, 0.50),
        "max": ordered[-1],
        "q1": percentile(ordered, 0.25),
        "q3": percentile(ordered, 0.75),
        "iqr": percentile(ordered, 0.75) - percentile(ordered, 0.25),
    }


def _iqr_overlap(left, right):
    return left["q1"] <= right["q3"] and right["q1"] <= left["q3"]


def _ratio(low, high):
    if low <= 0:
        return float("inf") if high > 0 else 1.0
    return high / low


def runtime_outcome(left, right):
    """Return (clear, text) for two runtime summaries (milliseconds)."""
    low, high = sorted((left["median"], right["median"]))
    ratio = _ratio(low, high)
    if ratio < RUNTIME_RATIO:
        return False, "no clear difference: median ratio %.3f < %.2f" % (ratio, RUNTIME_RATIO)
    if _iqr_overlap(left, right):
        return False, "no clear difference: IQRs overlap (ratio %.3f)" % ratio
    return True, "clear difference: median ratio %.3f, IQRs disjoint" % ratio


def startup_outcome(left, right):
    """Return (clear, text) for two startup summaries (milliseconds)."""
    difference = abs(left["median"] - right["median"])
    base = max(left["median"], right["median"], 1e-9)
    relative = difference / base
    if difference < STARTUP_ABSOLUTE_MS:
        return False, "no clear difference: %.2f ms < %.1f ms" % (
            difference, STARTUP_ABSOLUTE_MS)
    if relative < STARTUP_RELATIVE:
        return False, "no clear difference: %.1f%% < %.0f%%" % (
            relative * 100.0, STARTUP_RELATIVE * 100.0)
    return True, "clear difference: %.2f ms, %.1f%%" % (difference, relative * 100.0)


def memory_outcome(left, right):
    """Return (clear, text) for two peak-memory summaries (bytes)."""
    difference = abs(left["median"] - right["median"])
    base = max(left["median"], right["median"], 1.0)
    relative = difference / base
    if difference < MEMORY_ABSOLUTE_BYTES:
        return False, "no clear difference: %.2f MiB < 1 MiB" % (
            difference / (1024.0 * 1024.0))
    if relative < MEMORY_RELATIVE:
        return False, "no clear difference: %.1f%% < %.0f%%" % (
            relative * 100.0, MEMORY_RELATIVE * 100.0)
    return True, "clear difference: %.2f MiB, %.1f%%" % (
        difference / (1024.0 * 1024.0), relative * 100.0)


def _selftest():
    failures = []

    def check(label, condition):
        if not condition:
            failures.append(label)

    sample = list(range(1, 16))
    summary = summarize(sample)
    check("median", summary["median"] == 8.0)
    check("min/max", summary["min"] == 1.0 and summary["max"] == 15.0)
    check("p95", abs(summary["p95"] - 14.3) < 1e-9)
    check("iqr", abs(summary["iqr"] - 7.0) < 1e-9)
    check("n", summary["n"] == 15)

    # Runtime: ratio and IQR must both pass before a difference is called.
    clear, _ = runtime_outcome({"median": 100.0, "q1": 95.0, "q3": 105.0},
                               {"median": 130.0, "q1": 125.0, "q3": 135.0})
    check("runtime clear", clear)
    clear, _ = runtime_outcome({"median": 100.0, "q1": 95.0, "q3": 105.0},
                               {"median": 105.0, "q1": 80.0, "q3": 130.0})
    check("runtime small ratio", not clear)
    clear, _ = runtime_outcome({"median": 100.0, "q1": 90.0, "q3": 120.0},
                               {"median": 140.0, "q1": 110.0, "q3": 160.0})
    check("runtime overlapping IQR", not clear)

    # Startup: both the absolute and the relative threshold must be exceeded.
    clear, _ = startup_outcome({"median": 10.0}, {"median": 10.5})
    check("startup tiny", not clear)
    clear, _ = startup_outcome({"median": 100.0}, {"median": 105.0})
    check("startup below 10 percent", not clear)
    clear, _ = startup_outcome({"median": 10.0}, {"median": 20.0})
    check("startup clear", clear)

    # Memory: same conjunction, with a 1 MiB floor.
    megabyte = 1024 * 1024
    clear, _ = memory_outcome({"median": 10 * megabyte}, {"median": 10 * megabyte + 1000})
    check("memory tiny", not clear)
    clear, _ = memory_outcome({"median": 50 * megabyte}, {"median": 53 * megabyte})
    check("memory below 10 percent", not clear)
    clear, _ = memory_outcome({"median": 10 * megabyte}, {"median": 14 * megabyte})
    check("memory clear", clear)

    for label in failures:
        print("FAIL " + label)
    if failures:
        return 1
    print("stats selftest: PASS (17 assertions)")
    return 0


def main(argv):
    if argv and argv[0] == "--selftest":
        return _selftest()
    print(__doc__.strip())
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
