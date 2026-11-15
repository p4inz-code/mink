#!/usr/bin/env python3
"""M2 / A cross-check: run `bigio` and its Python reference on identical
inputs, comparing stdout, stderr and the exit code byte-for-byte.

Each implementation gets its own work directory (`work/mink`, `work/py`) so
file-producing cases cannot race each other; `{W}` in a case's arguments is
replaced with that directory.

Build first, then run from the repository root:

    target/release/mink.exe build validation/m2_bigio/main.mink
    python validation/m2_bigio/verify.py
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
WORK = os.path.join(HERE, "work")

RECORDS = 12000


def fixtures_for(side):
    root = os.path.join(WORK, side)
    os.makedirs(root, exist_ok=True)
    doc = [{"id": i, "value": (i * 7919) % 10007, "name": "r%d" % i}
           for i in range(RECORDS)]
    with open(os.path.join(root, "records.json"), "w", encoding="utf-8") as fh:
        json.dump(doc, fh, separators=(",", ":"))
    with open(os.path.join(root, "object.json"), "w", encoding="utf-8") as fh:
        json.dump({"a": 1}, fh)
    with open(os.path.join(root, "bad.json"), "w", encoding="utf-8") as fh:
        fh.write('{"a": [1, 2,')


def run(program, args, stdin):
    try:
        proc = subprocess.run([program] + args, cwd=REPO, input=stdin,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                              timeout=300)
        return proc.stdout, proc.stderr, proc.returncode
    except subprocess.TimeoutExpired:
        return b"", b"<timeout>", -1


def run_case(name, args, stdin=b""):
    results = {}
    for side, program, prefix in (
            ("mink", MINK_EXE, []),
            ("py", sys.executable, [PYTHON_REF])):
        fixtures_for(side)
        root = os.path.join(WORK, side)
        side_args = [a.replace("{W}", root) for a in args]
        results[side] = run(program, prefix + side_args, stdin)

    mink, py = results["mink"], results["py"]
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
        return False
    print("ok   %s (exit %d)" % (name, mink[2]))
    return True


def main():
    if not os.path.exists(MINK_EXE):
        print("missing %s; build first with "
              "`target/release/mink.exe build validation/m2_bigio/main.mink`" % MINK_EXE)
        return 1
    for side in ("mink", "py"):
        fixtures_for(side)

    stream = b"".join(b"line-%05d\n" % i for i in range(6000))  # ~78 KiB
    big_stream = b"0123456789abcdef" * 65536  # 1 MiB

    cases = [
        ("gen_5mib", ["gen", "{W}/gen5.bin", "5", "0123456789abcdef"], b""),
        ("scan_gen5", ["scan", "{W}/gen5.bin"], b""),
        ("copy_gen5", ["copy", "{W}/gen5.bin", "{W}/copy.bin"], b""),
        ("scan_copy", ["scan", "{W}/copy.bin"], b""),
        ("scan_missing", ["scan", "{W}/nope.bin"], b""),
        ("stdin_78k", ["stdin"], stream),
        ("stdin_1mib", ["stdin"], big_stream),
        ("grow_16", ["grow", "16"], b""),
        ("grow_20", ["grow", "20"], b""),
        ("grow_zero", ["grow", "0"], b""),
        ("grow_too_many", ["grow", "25"], b""),
        ("cycles_1000", ["cycles", "1000"], b""),
        ("cycles_20000", ["cycles", "20000"], b""),
        ("json_12k", ["json", "{W}/records.json", "value"], b""),
        ("json_missing_field", ["json", "{W}/records.json", "absent"], b""),
        ("json_not_array", ["json", "{W}/object.json", "value"], b""),
        ("json_bad", ["json", "{W}/bad.json", "value"], b""),
        ("json_missing_file", ["json", "{W}/nope.json", "value"], b""),
        ("clean_copy", ["clean", "{W}/copy.bin"], b""),
        ("clean_again", ["clean", "{W}/copy.bin"], b""),
        ("no_args", [], b""),
        ("unknown", ["frobnicate"], b""),
        ("option_looking", ["-x"], b""),
        ("stdin_extra_arg", ["stdin", "x"], b""),
        ("gen_short_args", ["gen", "{W}/x.bin"], b""),
    ]

    passed = failed = 0
    for name, args, stdin in cases:
        if run_case(name, args, stdin):
            passed += 1
        else:
            failed += 1

    shutil.rmtree(WORK, ignore_errors=True)
    print("\n%d/%d cases match the Python reference" % (passed, passed + failed))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
