#!/usr/bin/env python3
"""F cross-check harness: run the MINK build and the Python reference on
identical inputs and compare stdout, stderr and the exit code byte-for-byte.

Build first, then run from the repository root:

    target/debug/mink.exe build validation/f_process/main.mink
    python validation/f_process/verify.py
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
MINK_EXE = os.path.join(HERE, "main.exe")
PYTHON_REF = os.path.join(HERE, "python_ref.py")

PROBE_ENV = "MINK_F_VALIDATION_PROBE"


def run_case(name, args):
    results = {}
    for who in ("mink", "py"):
        env = dict(os.environ)
        env.pop(PROBE_ENV, None)  # both runs start with the probe var unset
        cmd = [MINK_EXE] + args if who == "mink" else \
            [sys.executable, PYTHON_REF] + args
        proc = subprocess.run(cmd, cwd=REPO, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, env=env)
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
              f"`target/debug/mink.exe build validation/f_process/main.mink`")
        return 1

    cases = [
        ("argv_basic", ["argv", "one", "two words", "3"]),
        ("argv_empty", ["argv"]),
        ("argv_unicode", ["argv", "caf\u00e9"]),
        ("run_echo", ["run", "cmd /c echo hello"]),
        ("run_exit3", ["run", "cmd /c exit 3"]),
        ("run_pipe", ["run", "cmd /c echo one|cmd /c findstr one"]),
        ("runin_sort", ["runin", "cmd /c sort", "b\na\n"]),
        ("runin_empty", ["runin", "cmd /c sort", ""]),
        ("stamp_epoch0", ["stamp", "0", "%Y-%m-%d %H:%M:%S"]),
        ("stamp_iso", ["stamp", "1000000000", "%Y-%m-%dT%H:%M:%SZ"]),
        ("stamp_2023", ["stamp", "1700000000", "%Y/%m/%d %H:%M"]),
        ("env_probe", ["env", PROBE_ENV]),
        ("no_args", []),
        ("unknown_command", ["frobnicate"]),
        ("option_looking_arg", ["-x"]),
        ("run_missing_args", ["run"]),
        ("run_extra_args", ["run", "cmd /c echo a", "extra"]),
        ("stamp_bad_epoch", ["stamp", "abc", "%Y"]),
        ("env_extra_args", ["env", PROBE_ENV, "extra"]),
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
