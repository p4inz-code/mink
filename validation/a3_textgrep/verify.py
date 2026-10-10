#!/usr/bin/env python3
"""A3 cross-check harness: run the MINK build and the Python reference on
identical inputs and compare stdout, stderr, exit code and the produced
`--output` file (as a directory tree) byte-for-byte.

Build first, then run from the repository root:

    target/debug/mink.exe build validation/a3_textgrep/main.mink
    python validation/a3_textgrep/verify.py
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
    """{relative posix path: bytes} for every file under `root`."""
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
        argv = []
        for item in args:
            if item == "@OUT":
                argv.append(os.path.join(case, "out.txt"))
            else:
                argv.append(item)
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
              f"`target/debug/mink.exe build validation/a3_textgrep/main.mink`")
        return 1
    subprocess.run([sys.executable,
                    os.path.join(FIXTURES, "make_fixtures.py")], check=True)

    cases = [
        ("basic_error", ["ERROR", fixture("app.log")]),
        ("match_anywhere", ["code=4", fixture("app.log")]),
        ("anchored_caret", ["^INFO", fixture("plain.txt")]),
        ("anchored_dollar", ["done$", fixture("plain.txt")]),
        ("comment_anchor", ["^#", fixture("plain.txt")]),
        ("numbered", ["-n", "INFO", fixture("plain.txt")]),
        ("count", ["-c", "INFO", fixture("plain.txt")]),
        ("count_long", ["--count", "^INFO", fixture("plain.txt")]),
        ("invert", ["-v", "INFO", fixture("plain.txt")]),
        ("ignore_case", ["-i", "error", fixture("app.log")]),
        ("ignore_case_class", ["-i", "[a-z]+ing", fixture("plain.txt")]),
        ("max_count_2", ["-m", "2", "INFO", fixture("plain.txt")]),
        ("max_count_long", ["--max-count", "1", "INFO", fixture("plain.txt")]),
        ("regex_class_digits", ["[0-9]+", fixture("app.log")]),
        ("regex_dotstar", ["worker.*started", fixture("app.log")]),
        ("empty_pattern", ["", fixture("app.log")]),
        ("no_match", ["zzz", fixture("app.log")]),
        ("missing_file", ["x", fixture("nope.txt")]),
        ("dir_as_file", ["x", FIXTURES]),
        ("no_args", []),
        ("one_positional", ["INFO"]),
        ("too_many_positionals", ["INFO", fixture("app.log"), "extra"]),
        ("max_missing_value", ["-m"]),
        ("max_invalid", ["-m", "abc", "INFO", fixture("app.log")]),
        ("max_zero", ["-m", "0", "INFO", fixture("app.log")]),
        ("output_file", ["INFO", fixture("plain.txt"), "-o", "@OUT"]),
        ("output_count", ["-c", "INFO", fixture("plain.txt"), "-o", "@OUT"]),
        ("output_no_match", ["zzz", fixture("plain.txt"), "-o", "@OUT"]),
        ("no_trailing_newline", [".", fixture("no_newline.txt")]),
        ("crlf_error", ["ERROR", fixture("crlf.txt")]),
        ("empty_file", ["", fixture("empty.txt")]),
        ("only_newlines_empty_pattern", ["", fixture("only_newlines.txt")]),
        ("only_newlines_anchored", ["^x", fixture("only_newlines.txt")]),
        ("unicode_lines", ["line", fixture("unicode.txt")]),
        ("unicode_error", ["ERROR", fixture("unicode.txt")]),
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
