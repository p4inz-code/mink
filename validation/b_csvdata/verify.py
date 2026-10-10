#!/usr/bin/env python3
"""B cross-check harness: run the MINK build and the Python reference on
identical inputs and compare stdout, stderr, exit code and the produced
output file (as a directory tree) byte-for-byte.

Build first, then run from the repository root:

    target/debug/mink.exe build validation/b_csvdata/main.mink
    python validation/b_csvdata/verify.py
"""
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
MINK_EXE = os.path.join(HERE, "main.exe")
PYTHON_REF = os.path.join(HERE, "python_ref.py")
WORK = os.path.join(HERE, "work")
FIXTURES = os.path.join(HERE, "fixtures")


def fixture(name):
    return os.path.join(FIXTURES, name)


def tree_map(root):
    result = {}
    if not os.path.isdir(root):
        return result
    for base, _dirs, files in os.walk(root):
        for name in files:
            full = os.path.join(base, name)
            rel = os.path.relpath(full, root).replace(os.sep, "/")
            with open(full, "rb") as handle:
                result[rel] = handle.read()
    return result


def run_case(name, args):
    base = os.path.join(WORK, name)
    if os.path.isdir(base):
        shutil.rmtree(base)
    results = {}
    trees = {}
    for who in ("mink", "py"):
        case = os.path.join(base, who)
        os.makedirs(case)
        argv = [os.path.join(case, "out.csv") if item == "@OUT" else item
                for item in args]
        cmd = [MINK_EXE] + argv if who == "mink" else \
            [sys.executable, PYTHON_REF] + argv
        proc = subprocess.run(cmd, cwd=REPO, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE)
        results[who] = (proc.stdout, proc.stderr, proc.returncode)
        trees[who] = tree_map(case)

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
    if trees["mink"] != trees["py"]:
        only_mink = sorted(set(trees["mink"]) - set(trees["py"]))
        only_py = sorted(set(trees["py"]) - set(trees["mink"]))
        changed = sorted(k for k in set(trees["mink"]) & set(trees["py"])
                         if trees["mink"][k] != trees["py"][k])
        problems.append(f"output tree differs: only_mink={only_mink} "
                        f"only_py={only_py} changed={changed}")

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
              f"`target/debug/mink.exe build validation/b_csvdata/main.mink`")
        return 1
    subprocess.run([sys.executable,
                    os.path.join(FIXTURES, "make_fixtures.py")], check=True)

    sales = fixture("sales.csv")
    notes = fixture("notes.csv")
    ragged = fixture("ragged.csv")
    header_only = fixture("header_only.csv")

    cases = [
        ("select_city", ["select", sales, "2", "Berlin"]),
        ("select_quoted_comma", ["select", sales, "1", "Acme"]),
        ("select_quoted_quotes", ["select", sales, "1", "Q"]),
        ("select_no_match", ["select", sales, "2", "Rome"]),
        ("select_notes_embedded_lf", ["select", notes, "1", "second"]),
        ("sort_customer", ["sort", sales, "1"]),
        ("sort_units_lexicographic", ["sort", sales, "3"]),
        ("sort_price_lexicographic", ["sort", sales, "4"]),
        ("sort_notes", ["sort", notes, "0"]),
        ("agg_city_units", ["agg", sales, "2", "3"]),
        ("agg_city_price_nonint", ["agg", sales, "2", "4"]),
        ("write_roundtrip", ["write", sales, "@OUT"]),
        ("write_notes", ["write", notes, "@OUT"]),
        ("write_header_only", ["write", header_only, "@OUT"]),
        ("ragged_select", ["select", ragged, "1", "ok"]),
        ("ragged_sort", ["sort", ragged, "1"]),
        ("ragged_write", ["write", ragged, "@OUT"]),
        ("header_only_select", ["select", header_only, "1", "x"]),
        ("header_only_sort", ["sort", header_only, "0"]),
        ("header_only_agg", ["agg", header_only, "0", "0"]),
        ("missing_file", ["select", fixture("nope.csv"), "0", "x"]),
        ("dir_as_file", ["select", FIXTURES, "0", "x"]),
        ("unknown_command", ["frobnicate", sales]),
        ("no_args", []),
        ("one_positional", ["select"]),
        ("select_missing_args", ["select", sales, "1"]),
        ("select_extra_args", ["select", sales, "1", "a", "b"]),
        ("col_not_int", ["select", sales, "x", "a"]),
        ("col_negative", ["select", sales, "-1", "a"]),
        ("col_out_of_range", ["select", sales, "9", "a"]),
        ("agg_col_out_of_range", ["agg", sales, "0", "9"]),
        ("option_looking_arg", ["-n", sales]),
        ("sort_missing_args", ["sort", sales]),
        ("agg_missing_args", ["agg", sales, "1"]),
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
