#!/usr/bin/env python3
"""Build and run the Session 176 exercises in disposable directories.

Usage:
    python benchmarks/m6/session176/verify.py
    python benchmarks/m6/session176/verify.py --compiler <path-to-mink.exe>

Every case is copied to its own temporary directory, built there, executed,
and compared byte-for-byte after CRLF normalization. Exit codes are checked.
"""

import argparse
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]

CASES = [
    ("01_hello", "01_hello.mink", [], 0, "42\n"),
    ("02_variables", "02_variables.mink", [], 0, "3\n2.5\n1\nmink\n"),
    ("03_operators", "03_operators.mink", [], 0,
     "10\n4\n21\n2\n1\n14\n3\n1\n1\n"),
    ("04_conditionals", "04_conditionals.mink", [], 0, "-1\n0\n1\n"),
    ("05_loops", "05_loops.mink", [], 0, "10\n15\n3\n"),
    ("06_functions", "06_functions.mink", [], 0, "9\n9\n"),
    ("07_returns", "07_returns.mink", [], 0, "-1\n0\n1\n10\n20\n"),
    ("08_strings", "08_strings.mink", [], 0, "mink-lang\n9\n109\n176\n"),
    ("09_collections", "09_collections.mink", [], 0, "100\n2\n200\n1\n"),
    ("10_imports", "10_imports.mink", [], 0, "module-ok\n9\n0\n"),
    ("program_a_calculator", "program_a_calculator.mink", [], 0,
     "add,sub,mul,div,neg\n90\n78\n504\n14\n-78\ndiv0=error\n"),
    ("program_b_default", "program_b_data_processor.mink", [], 0,
     "count,sum,min,max,mean_x100\n8\n360\n10\n80\n4500\n"),
    ("program_b_empty", "program_b_data_processor.mink", ["empty"], 0,
     "count,sum,min,max,mean_x100\n0\n0\n-1\n-1\n0\n"),
    ("program_b_single", "program_b_data_processor.mink", ["single"], 0,
     "count,sum,min,max,mean_x100\n1\n10\n10\n10\n1000\n"),
    ("program_c_utility", "program_c_modular_utility.mink", [], 0,
     "write,read,exists,size,missing_len,missing_size,bad_write,removed,cleanup\n"
     "8\nhello-m6\n1\n8\n0\n-1\n-1\n0\n1\n"),
]


def normalize(data):
    return data.decode("utf-8", "replace").replace("\r\n", "\n")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--compiler", default=str(REPO / "target" / "release" / "mink.exe"))
    args = parser.parse_args()

    compiler = Path(args.compiler)
    if not compiler.is_file():
        print("compiler not found: %s" % compiler)
        return 2

    work_root = REPO / "tmp" / "m6_session176_work"
    work_root.mkdir(parents=True, exist_ok=True)

    passed = 0
    for name, source, extra_args, want_exit, want_stdout in CASES:
        with tempfile.TemporaryDirectory(prefix=name + "_", dir=str(work_root)) as td:
            work = Path(td)
            src = work / source
            shutil.copy2(HERE / source, src)

            build_start = time.perf_counter()
            build = subprocess.run(
                [str(compiler), "build", src.name],
                cwd=str(work), capture_output=True, timeout=120
            )
            build_s = time.perf_counter() - build_start
            if build.returncode != 0:
                print("FAIL %-22s build exit %d" % (name, build.returncode))
                sys.stdout.write(normalize(build.stdout))
                sys.stderr.write(normalize(build.stderr))
                continue

            exe = work / (src.stem + ".exe")
            if not exe.is_file():
                print("FAIL %-22s build produced no executable" % name)
                continue

            run_start = time.perf_counter()
            run = subprocess.run(
                [str(exe)] + extra_args,
                cwd=str(work), capture_output=True, timeout=60
            )
            run_s = time.perf_counter() - run_start
            got_stdout = normalize(run.stdout)
            got_stderr = normalize(run.stderr)

            if run.returncode != want_exit or got_stdout != want_stdout:
                print("FAIL %-22s exit %d (want %d) build=%.3fs run=%.3fs"
                      % (name, run.returncode, want_exit, build_s, run_s))
                print("  stdout: %r" % got_stdout)
                print("  want:   %r" % want_stdout)
                if got_stderr:
                    print("  stderr: %r" % got_stderr)
                continue

            passed += 1
            print("PASS %-22s exit %d build=%.3fs run=%.3fs"
                  % (name, run.returncode, build_s, run_s))

    print("RESULT %d/%d passed" % (passed, len(CASES)))
    return 0 if passed == len(CASES) else 1


if __name__ == "__main__":
    sys.exit(main())
