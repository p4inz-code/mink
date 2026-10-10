#!/usr/bin/env python3
"""C cross-check harness: run the MINK build and the Python reference on
identical inputs and compare stdout, stderr and the exit code byte-for-byte.

Build first, then run from the repository root:

    target/debug/mink.exe build validation/c_json/main.mink
    python validation/c_json/verify.py
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
              f"`target/debug/mink.exe build validation/c_json/main.mink`")
        return 1
    subprocess.run([sys.executable,
                    os.path.join(FIXTURES, "make_fixtures.py")], check=True)

    obj = fixture("object.json")
    arr = fixture("array.json")
    nested = fixture("nested.json")
    number = fixture("number.json")
    string = fixture("string.json")
    boolean = fixture("bool.json")
    null = fixture("null.json")
    empty_arr = fixture("empty_array.json")
    empty_obj = fixture("empty_object.json")
    escaped = fixture("escaped.json")
    unicode_f = fixture("unicode.json")
    broken = fixture("broken.json")
    trailing = fixture("trailing.json")
    empty = fixture("empty.json")

    cases = [
        ("validate_object", ["validate", obj]),
        ("validate_array", ["validate", arr]),
        ("validate_scalar", ["validate", number]),
        ("validate_broken", ["validate", broken]),
        ("validate_trailing", ["validate", trailing]),
        ("validate_empty", ["validate", empty]),
        ("type_object", ["type", obj]),
        ("type_array", ["type", arr]),
        ("type_number", ["type", number]),
        ("type_string", ["type", string]),
        ("type_bool", ["type", boolean]),
        ("type_null", ["type", null]),
        ("keys_object", ["keys", obj]),
        ("keys_empty_object", ["keys", empty_obj]),
        ("keys_on_array", ["keys", arr]),
        ("count_array", ["count", arr]),
        ("count_empty_array", ["count", empty_arr]),
        ("count_on_object", ["count", obj]),
        ("get_qty", ["get", obj, "qty"]),
        ("get_tags", ["get", obj, "tags"]),
        ("get_nested", ["get", nested, "outer"]),
        ("get_missing", ["get", obj, "zzz"]),
        ("get_on_array", ["get", arr, "x"]),
        ("sum_qty", ["sum", arr, "qty"]),
        ("sum_missing_field", ["sum", arr, "nope"]),
        ("sum_on_object", ["sum", obj, "qty"]),
        ("compact_nested", ["compact", nested]),
        ("compact_escaped", ["compact", escaped]),
        ("compact_unicode", ["compact", unicode_f]),
        ("compact_number", ["compact", number]),
        ("compact_empty_array", ["compact", empty_arr]),
        ("compact_empty_object", ["compact", empty_obj]),
        ("missing_file", ["validate", fixture("nope.json")]),
        ("dir_as_file", ["validate", FIXTURES]),
        ("no_args", []),
        ("one_positional", ["validate"]),
        ("unknown_command", ["frobnicate", obj]),
        ("option_looking_arg", ["-x", obj]),
        ("get_missing_args", ["get", obj]),
        ("get_extra_args", ["get", obj, "qty", "x"]),
        ("compact_extra_args", ["compact", obj, "x"]),
        ("unknown_command_missing_file", ["frobnicate", fixture("nope.json")]),
        ("unknown_command_broken_file", ["frobnicate", broken]),
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
