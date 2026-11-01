#!/usr/bin/env python3
"""Author the W4 golden transcripts from the M1-verified split and rendering.

    python benchmarks/m4/expected/w4_pool/make_expected.py

The ranges, join order and closed-form check are the M1 `poolrun` contract.
This oracle is not one of the five benchmark applications; it reproduces the
M1 reference renderer so the golden bytes are independent of any port.
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

SCORED = [
    ("squares_100_4", ["squares", "100", "4"]),
    ("squares_1024_16", ["squares", "1024", "16"]),
    ("squares_10000_8", ["squares", "10000", "8"]),
    ("squares_10000_16", ["squares", "10000", "16"]),
    ("pipeline_100_4", ["pipeline", "100", "4"]),
    ("pipeline_1000_3", ["pipeline", "1000", "3"]),
    ("pipeline_999_7", ["pipeline", "999", "7"]),
    ("pipeline_10000_8", ["pipeline", "10000", "8"]),
]

SUPPORTING = [
    ("squares_1_1", ["squares", "1", "1"]),
    ("squares_97_5", ["squares", "97", "5"]),
    ("squares_10000_1", ["squares", "10000", "1"]),
    ("squares_10000_4", ["squares", "10000", "4"]),
    ("squares_999_7", ["squares", "999", "7"]),
    ("pipeline_1_1", ["pipeline", "1", "1"]),
    ("pipeline_97_5", ["pipeline", "97", "5"]),
    ("pipeline_10000_1", ["pipeline", "10000", "1"]),
    ("pipeline_10000_4", ["pipeline", "10000", "4"]),
]


def ranges_for(n, workers):
    chunk = n // workers
    parts = []
    for k in range(workers):
        start = k * chunk + 1
        end = (k + 1) * chunk
        if k == workers - 1:
            end = n
        parts.append((start, end))
    return parts


def expected_squares(n):
    return n * (n + 1) * (2 * n + 1) // 6


def subtotal(start, end):
    if end < start:
        return 0
    return sum(i * i for i in range(start, end + 1))


def render(mode, n, workers):
    lines = ["workers=" + str(workers), "n=" + str(n)]
    parts = ranges_for(n, workers)
    values = [subtotal(start, end) for start, end in parts]
    if mode == "squares":
        for index, value in enumerate(values):
            lines.append("subtotal[%d]=%d" % (index, value))
        total = sum(values)
        expected = expected_squares(n)
        lines.append("locked-total=" + str(total))
        lines.append("joined-total=" + str(total))
        lines.append("expected=" + str(expected))
    else:
        for index, value in enumerate(values):
            lines.append("stage1[%d]=%d" % (index, value))
        total = sum(values)
        expected = expected_squares(n)
        lines.append("total=" + str(total))
        lines.append("expected=" + str(expected))
    return "\r\n".join(lines) + "\r\n"


def main():
    written = 0
    for case, argv in SCORED + SUPPORTING:
        mode, n, workers = argv[0], int(argv[1]), int(argv[2])
        path = os.path.join(HERE, case + ".stdout")
        with open(path, "wb") as handle:
            handle.write(render(mode, n, workers).encode("utf-8"))
        written += 1
    print("expected: %d transcripts under %s" % (written, HERE))
    return 0


if __name__ == "__main__":
    sys.exit(main())
