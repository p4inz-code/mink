#!/usr/bin/env python3
"""H cross-check harness: run the MINK build and the Python reference on
identical configs and compare stdout, stderr, the exit code and the produced
report file byte-for-byte.

Build first, then run from the repository root:

    target/debug/mink.exe build validation/h_manifest/main.mink
    python validation/h_manifest/verify.py
"""
import json
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
MINK_EXE = os.path.join(HERE, "main.exe")
PYTHON_REF = os.path.join(HERE, "python_ref.py")
FIXTURES = os.path.join(HERE, "fixtures")
WORK = os.path.join(HERE, "work")


def template(name):
    return os.path.join(FIXTURES, name)


def write_config(case, who, template_name):
    """Rewrite the template's `@OUT` for this side and return its path.

    The config itself lives outside the compared output directory so the two
    sides' config files (which differ only in the out path) are not part of
    the byte-for-byte tree comparison."""
    case_dir = os.path.join(WORK, case, who)
    os.makedirs(case_dir, exist_ok=True)
    with open(template(template_name), "r", encoding="utf-8") as handle:
        config = json.load(handle)
    config["out"] = os.path.join(case_dir, "report.csv")
    path = os.path.join(WORK, case, who + ".json")
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(config, handle, ensure_ascii=False)
    return path


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


def run_case(name, make_args):
    base = os.path.join(WORK, name)
    if os.path.isdir(base):
        shutil.rmtree(base)
    results = {}
    trees = {}
    for who in ("mink", "py"):
        case_dir = os.path.join(base, who)
        os.makedirs(case_dir, exist_ok=True)
        args = make_args(who)
        cmd = [MINK_EXE] + args if who == "mink" else \
            [sys.executable, PYTHON_REF] + args
        proc = subprocess.run(cmd, cwd=REPO, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE)
        results[who] = (proc.stdout, proc.stderr, proc.returncode)
        trees[who] = tree_map(case_dir)

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
              f"`target/debug/mink.exe build validation/h_manifest/main.mink`")
        return 1
    subprocess.run([sys.executable,
                    os.path.join(FIXTURES, "make_fixtures.py")], check=True)

    cases = [
        ("default", lambda who: [write_config("default", who, "config.json")]),
        ("uppercase_ext",
         lambda who: [write_config("uppercase_ext", who,
                                   "config_uppercase_ext.json")]),
        ("no_match",
         lambda who: [write_config("no_match", who, "config_no_match.json")]),
        ("missing_root",
         lambda who: [write_config("missing_root", who,
                                   "config_missing_root.json")]),
        ("bad_json", lambda who: [template("bad_json.json")]),
        ("missing_keys", lambda who: [template("missing_keys.json")]),
        ("bad_types", lambda who: [template("bad_types.json")]),
        ("root_not_object", lambda who: [template("root_not_object.json")]),
        ("missing_config", lambda who: [template("nope.json")]),
        ("dir_as_config", lambda who: [FIXTURES]),
        ("no_args", lambda who: []),
        ("extra_args", lambda who: [template("config.json"), "extra"]),
        ("option_looking_arg", lambda who: ["-x"]),
    ]

    passed = failed = 0
    for name, make_args in cases:
        if run_case(name, make_args):
            passed += 1
        else:
            failed += 1

    print(f"\n{passed}/{passed + failed} cases match the Python reference")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
