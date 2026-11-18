#!/usr/bin/env python3
"""M7 Phase E — child-stdin boundary check.

Builds the echo child and the probe with the repository compiler, runs the
probe in this directory, and checks the delivered-byte curve against the
documented contract in `stdlib/process.mink`:

    offered <= 4096  ->  every byte is delivered   (captured = offered + 2)
    offered >  4096  ->  exactly 4096 are delivered (captured = 4098)

The trailing +2 is the CRLF that the child's `rt_print_str` appends.

    python validation/phase_e/stdin/verify.py [path/to/mink.exe]
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(os.path.dirname(HERE)))
DEFAULT_MINK = os.path.join(REPO, "target", "release", "mink.exe")


def build(compiler, source):
    proc = subprocess.run([compiler, "build", source],
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if proc.returncode != 0:
        raise SystemExit("build failed for %s:\n%s"
                         % (source, proc.stderr.decode("utf-8", "replace")))


def main(argv):
    compiler = argv[1] if len(argv) > 1 else DEFAULT_MINK
    if not os.path.exists(compiler):
        print("missing compiler %s (build it with `cargo build --release`)"
              % compiler)
        return 1
    build(compiler, os.path.join(HERE, "child.mink"))
    build(compiler, os.path.join(HERE, "probe.mink"))

    proc = subprocess.run([os.path.join(HERE, "probe.exe")], cwd=HERE,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          timeout=300)
    if proc.returncode != 0:
        print("probe failed: exit %d\n%s"
              % (proc.returncode, proc.stderr.decode("utf-8", "replace")))
        return 1

    text = proc.stdout.decode("ascii", "replace").replace("\r\n", "\n")
    cases = {}
    offered = code = None
    for line in text.splitlines():
        if line == "probe":
            offered = code = None
        elif line.startswith("offered="):
            offered = int(line.split("=", 1)[1])
        elif line.startswith("code="):
            code = int(line.split("=", 1)[1])
        elif line.startswith("captured="):
            captured = int(line.split("=", 1)[1])
            if offered is None or code is None:
                print("malformed transcript:\n%s" % text)
                return 1
            cases[offered] = (code, captured)

    expected = {}
    for n in (0, 1, 4095, 4096, 4097, 5000, 65536):
        delivered = n if n <= 4096 else 4096
        expected[n] = (0, delivered + 2)

    failures = 0
    print("%8s %9s %9s %9s" % ("offered", "delivered", "expected", "code"))
    for n in sorted(expected):
        if n not in cases:
            print("%8d %9s %9s %9s" % (n, "MISSING", "-", "-"))
            failures += 1
            continue
        code, captured = cases[n]
        delivered = captured - 2 if captured >= 2 else 0
        want_code, want_captured = expected[n]
        ok = (code == want_code and captured == want_captured)
        print("%8d %9d %9d %9d%s"
              % (n, delivered, want_captured - 2, code, "" if ok else "  FAIL"))
        if not ok:
            failures += 1

    if failures:
        print("\n%d case(s) failed" % failures)
        return 1
    print("\nthe documented 4,096-byte cap holds; longer input is truncated "
          "silently and the child still exits 0")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
