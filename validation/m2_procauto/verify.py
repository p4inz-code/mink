#!/usr/bin/env python3
"""M2 / B cross-check: compare `procauto` with its Python reference.

Both sides run with this directory as the working directory, because the app
launches its helper as `child.exe` (resolved relative to the child's working
directory, exactly as a user tool would).

    target/release/mink.exe build validation/m2_procauto/child.mink
    target/release/mink.exe build validation/m2_procauto/main.mink
    python validation/m2_procauto/verify.py
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
MINK_EXE = os.path.join(HERE, "main.exe")
CHILD_EXE = os.path.join(HERE, "child.exe")
PYTHON_REF = os.path.join(HERE, "python_ref.py")

CASES = [
    ("capture", ["capture"]),
    ("feed", ["feed"]),
    ("many", ["many"]),
    ("codes", ["codes"]),
    ("missing", ["missing"]),
    ("no_args", []),
    ("unknown", ["frobnicate"]),
    ("option_looking", ["-x"]),
]


def run(cmd):
    proc = subprocess.run(cmd, cwd=HERE, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, timeout=300)
    return proc.stdout, proc.stderr, proc.returncode


def main():
    for path in (MINK_EXE, CHILD_EXE):
        if not os.path.exists(path):
            print("missing %s; build child.mink and main.mink first" % path)
            return 1

    passed = failed = 0
    for name, args in CASES:
        mink = run([MINK_EXE] + args)
        py = run([sys.executable, PYTHON_REF] + args)
        problems = []
        if mink[0] != py[0]:
            problems.append("stdout differs:\n  mink=%r\n  py  =%r" % (mink[0][:400], py[0][:400]))
        if mink[1] != py[1]:
            problems.append("stderr differs:\n  mink=%r\n  py  =%r" % (mink[1][:400], py[1][:400]))
        if mink[2] != py[2]:
            problems.append("exit differs: mink=%d py=%d" % (mink[2], py[2]))
        if problems:
            print("FAIL %s" % name)
            for problem in problems:
                print("   " + problem.replace("\n", "\n   "))
            failed += 1
        else:
            print("ok   %s (exit %d)" % (name, mink[2]))
            passed += 1

    print("\n%d/%d cases match the Python reference" % (passed, passed + failed))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
