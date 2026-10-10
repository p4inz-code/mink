#!/usr/bin/env python3
"""A2 cross-check harness: run the MINK build and the Python reference on
identical inputs and compare stdout, stderr, exit code and the resulting
directory trees byte-for-byte.

Run from the repository root:

    python validation/a2_treeops/verify.py
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
FIXTURE_SRC = os.path.join(HERE, "fixtures", "src")


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


def build_case(case, src_exists, empty_src, dst_is_file, src_subdir):
    """Create the per-implementation source/destination paths."""
    if src_exists is False:
        src = os.path.join(case, "missing_src")
    elif src_exists == "file":
        src = os.path.join(case, "file_src")
        if not os.path.exists(src):
            with open(src, "wb") as handle:
                handle.write(b"i am a file\n")
    elif empty_src:
        src = os.path.join(case, "empty_src")
        os.makedirs(src, exist_ok=True)
    else:
        src = os.path.join(case, "src")
        fixture = os.path.join(FIXTURE_SRC, "a") if src_subdir else FIXTURE_SRC
        shutil.copytree(fixture, src)
    dst = os.path.join(case, "dst")
    if dst_is_file and not os.path.exists(dst):
        with open(dst, "wb") as handle:
            handle.write(b"not a directory\n")
    return src, dst


def run_case(name, args, src_exists=True, empty_src=False, dst_is_file=False,
             extra_positional=False, run_twice=False, compare_src=False,
             no_positionals=False, src_subdir=False):
    base = os.path.join(WORK, name)
    if os.path.isdir(base):
        shutil.rmtree(base)

    results = {}
    trees = {}
    for who in ("mink", "py"):
        case = os.path.join(base, who)
        os.makedirs(case)
        src, dst = build_case(case, src_exists, empty_src, dst_is_file,
                              src_subdir)
        if no_positionals:
            argv = list(args)
        elif extra_positional:
            argv = list(args) + [src, dst, src]
        else:
            argv = list(args) + [src, dst]
        cmd = [MINK_EXE] + argv if who == "mink" else \
            [sys.executable, PYTHON_REF] + argv
        attempts = 2 if run_twice else 1
        for attempt in range(attempts):
            proc = subprocess.run(cmd, cwd=REPO, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE)
            results[(who, attempt)] = (proc.stdout, proc.stderr,
                                       proc.returncode)
        trees[who] = (tree_map(dst), tree_map(src) if compare_src else None)

    problems = []
    attempts = 2 if run_twice else 1
    for attempt in range(attempts):
        mink = results[("mink", attempt)]
        py = results[("py", attempt)]
        tag = "" if attempt == 0 else f" (run {attempt + 1})"
        if mink[0] != py[0]:
            problems.append(f"stdout{tag} differs:\n"
                            f"  mink={mink[0]!r}\n  py  ={py[0]!r}")
        if mink[1] != py[1]:
            problems.append(f"stderr{tag} differs:\n"
                            f"  mink={mink[1]!r}\n  py  ={py[1]!r}")
        if mink[2] != py[2]:
            problems.append(f"exit{tag} differs: mink={mink[2]} py={py[2]}")

    for index, label in ((0, "destination"), (1, "source")):
        mink_tree, py_tree = trees["mink"][index], trees["py"][index]
        if mink_tree is None or py_tree is None:
            continue
        if mink_tree != py_tree:
            only_mink = sorted(set(mink_tree) - set(py_tree))
            only_py = sorted(set(py_tree) - set(mink_tree))
            changed = sorted(k for k in set(mink_tree) & set(py_tree)
                             if mink_tree[k] != py_tree[k])
            problems.append(f"{label} tree differs: only_mink={only_mink} "
                            f"only_py={only_py} changed={changed}")

    if problems:
        print(f"FAIL {name}")
        for problem in problems:
            print("   " + problem.replace("\n", "\n   "))
        return False
    print(f"ok   {name} (exit {results[('mink', 0)][2]})")
    return True


def main():
    subprocess.run([sys.executable,
                    os.path.join(HERE, "fixtures", "make_fixtures.py")],
                   check=True)

    default_ext = ["--ext", ".log,.txt,.mink,.json,.csv"]
    cases = [
        ("default_copy", dict(args=default_ext)),
        ("repeat_execution", dict(args=[], run_twice=True)),
        ("dry_run", dict(args=["--dry-run"])),
        ("move", dict(args=["--move"], compare_src=True)),
        ("ext_png_bin", dict(args=["--ext", ".png,.bin"])),
        ("no_match", dict(args=["--ext", ".xyz"])),
        ("limit_3", dict(args=["--limit", "3"])),
        ("missing_src", dict(args=[], src_exists=False)),
        ("src_is_file", dict(args=[], src_exists="file")),
        ("no_args", dict(args=[], no_positionals=True)),
        ("too_many_positionals", dict(args=[], extra_positional=True)),
        ("ext_missing_value", dict(args=["--ext"], no_positionals=True)),
        ("limit_missing_value", dict(args=["--limit"], no_positionals=True)),
        ("limit_invalid_value", dict(args=["--limit", "abc"])),
        ("move_and_dry_run", dict(args=["--move", "--dry-run"],
                                  compare_src=True)),
        ("empty_src", dict(args=[], empty_src=True)),
        ("dst_is_file", dict(args=[], dst_is_file=True)),
        ("src_subdir_prefix", dict(args=default_ext, src_subdir=True)),
    ]

    passed = failed = 0
    for name, kwargs in cases:
        args = kwargs.pop("args")
        if run_case(name, args, **kwargs):
            passed += 1
        else:
            failed += 1

    print(f"\n{passed}/{passed + failed} cases match the Python reference")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
