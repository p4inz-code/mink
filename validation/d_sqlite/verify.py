#!/usr/bin/env python3
"""D cross-check harness: run the MINK build and the Python `sqlite3`
reference on identical scripts and compare stdout, stderr and the exit code
byte-for-byte.

Build first, then run from the repository root:

    target/debug/mink.exe build validation/d_sqlite/main.mink
    python validation/d_sqlite/verify.py
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
MINK_EXE = os.path.join(HERE, "main.exe")
PYTHON_REF = os.path.join(HERE, "python_ref.py")
FIXTURES = os.path.join(HERE, "fixtures")


def fixture(name):
    return os.path.join(FIXTURES, name)


def run_case(name, args):
    results = {}
    for who in ("mink", "py"):
        cmd = [MINK_EXE] + args if who == "mink" else \
            [sys.executable, PYTHON_REF] + args
        proc = subprocess.run(cmd, cwd=REPO, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE)
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
              f"`target/debug/mink.exe build validation/d_sqlite/main.mink`")
        return 1
    subprocess.run([sys.executable,
                    os.path.join(FIXTURES, "make_fixtures.py")], check=True)

    cases = [
        ("inventory_stops_on_error", [fixture("inventory.sql")]),
        ("clean_workload", [fixture("clean.sql")]),
        ("crlf_workload", [fixture("crlf.sql")]),
        ("comments_only", [fixture("comments.sql")]),
        ("empty_script", [fixture("empty.sql")]),
        ("missing_file", [fixture("nope.sql")]),
        ("dir_as_file", [FIXTURES]),
        ("no_args", []),
        ("two_positionals", [fixture("clean.sql"), "extra"]),
        ("option_looking_arg", ["-x"]),
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
