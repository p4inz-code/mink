#!/usr/bin/env python3
"""G cross-check harness: run the MINK build and the Python reference on
identical workloads and compare stdout, stderr and the exit code byte-for-byte.

Build first, then run from the repository root:

    target/debug/mink.exe build validation/g_threads/main.mink
    python validation/g_threads/verify.py
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
MINK_EXE = os.path.join(HERE, "main.exe")
PYTHON_REF = os.path.join(HERE, "python_ref.py")


def run_case(name, args):
    results = {}
    for who in ("mink", "py"):
        cmd = [MINK_EXE] + args if who == "mink" else \
            [sys.executable, PYTHON_REF] + args
        try:
            proc = subprocess.run(cmd, cwd=REPO, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, timeout=60)
        except subprocess.TimeoutExpired:
            print(f"FAIL {name}: {who} timed out")
            return False
        results[who] = (proc.stdout, proc.stderr, proc.returncode)

    problems = []
    if results["mink"][0] != results["py"][0]:
        problems.append(f"stdout differs:\n  mink={results['mink'][0]!r}\n"
                        f"  py  ={results['py'][0]!r}")
    if results["mink"][1] != results["py"][1]:
        problems.append(f"stderr differs:\n  mink={results['mink'][1]!r}\n"
                        f"  py  ={results['py'][1]!r}")
    if results["mink"][2] != results["py"][2]:
        problems.append(f"exit differs: mink={results['mink'][2]} "
                        f"py={results['py'][2]}")

    if problems:
        print(f"FAIL {name}")
        for problem in problems:
            print("   " + problem.replace("\n", "\n   "))
        return False
    print(f"ok   {name} (exit {results['mink'][2]})")
    return True


def main():
    if not os.path.exists(MINK_EXE):
        print(f"missing {MINK_EXE}; build first with "
              f"`target/debug/mink.exe build validation/g_threads/main.mink`")
        return 1

    cases = [
        ("squares_100_4", ["squares", "100", "4"]),
        ("squares_1_1", ["squares", "1", "1"]),
        ("squares_97_5", ["squares", "97", "5"]),
        ("squares_10000_8", ["squares", "10000", "8"]),
        ("squares_1024_16", ["squares", "1024", "16"]),
        ("pipeline_1_1", ["pipeline", "1", "1"]),
        ("pipeline_100_4", ["pipeline", "100", "4"]),
        ("pipeline_1000_3", ["pipeline", "1000", "3"]),
        ("pipeline_999_7", ["pipeline", "999", "7"]),
        ("no_args", []),
        ("missing_workers", ["squares", "100"]),
        ("extra_args", ["squares", "100", "4", "x"]),
        ("bad_n", ["squares", "abc", "4"]),
        ("bad_workers", ["squares", "100", "x"]),
        ("zero_n", ["squares", "0", "4"]),
        ("zero_workers", ["squares", "100", "0"]),
        ("unknown_mode", ["frobnicate", "100", "4"]),
        ("option_looking_arg", ["-x", "100", "4"]),
    ]

    passed = failed = 0
    for name, args in cases:
        if run_case(name, args):
            passed += 1
        else:
            failed += 1

    print(f"\n{passed}/{passed + failed} cases match the Python reference")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
