#!/usr/bin/env python3
"""M2 / G: verify the integrated workload against independent Python checks.

The app stores 100 rows in SQLite, writes the same records as JSON, reads and
re-parses the file, and sums 1..1000 squared with four threads. This harness
re-computes every number in Python (JSON parsed with the standard library) and
checks the app's own report, plus the error paths.

    target/release/mink.exe build validation/m2_integrated/main.mink
    python validation/m2_integrated/verify.py
"""
import json
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
MINK_EXE = os.path.join(HERE, "main.exe")
WORK = os.path.join(HERE, "verify_work")

VALUE = lambda i: (i * 7919) % 10007  # noqa: E731 - mirrors main.mink


def run(args):
    proc = subprocess.run([MINK_EXE] + args, cwd=HERE,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return proc.stdout, proc.stderr, proc.returncode


def main():
    if not os.path.exists(MINK_EXE):
        print("missing %s; build validation/m2_integrated/main.mink first" % MINK_EXE)
        return 1
    shutil.rmtree(WORK, ignore_errors=True)
    os.makedirs(WORK)
    ok = True

    out, err, code = run([WORK])
    want_sum = sum(VALUE(i) for i in range(100))
    want_linear = sum(i * i for i in range(1, 1001))
    # 1..1000 split into four quarters, summed by four threads.
    want_lines = [
        b"sql_rows=100",
        b"value_sum=%d" % want_sum,
        b"json_records=100",
        b"json_sum=%d" % want_sum,
        b"thread_sum=%d" % want_linear,
    ]
    got = out.split(b"\r\n")
    if code != 0:
        ok = False
        print("FAIL exit %d stderr=%r" % (code, err[:300]))
    for line in want_lines:
        if line not in got:
            ok = False
            print("FAIL missing line %r in %r" % (line, out[:300]))

    path = os.path.join(WORK, "records.json")
    if not os.path.exists(path):
        ok = False
        print("FAIL records.json was not written")
    else:
        with open(path, "r", encoding="utf-8") as fh:
            doc = json.load(fh)
        if len(doc) != 100 or [r["value"] for r in doc] != [VALUE(i) for i in range(100)]:
            ok = False
            print("FAIL records.json does not match the expected records")
        else:
            print("ok   100 records round-tripped through SQL, JSON and a file")

    # A second run in the same directory must overwrite, not append.
    out2, err2, code2 = run([WORK])
    if code2 != 0 or out2 != out:
        ok = False
        print("FAIL rerun exit %d stdout=%r stderr=%r" % (code2, out2[:200], err2[:200]))
    else:
        print("ok   rerun in an existing directory is idempotent")

    for args, want in (([], 2), (["-x"], 2)):
        _, _, got_code = run(args)
        if got_code != want:
            ok = False
            print("FAIL %r exit %d, want %d" % (args, got_code, want))
    if ok:
        print("ok   usage errors report exit 2")

    shutil.rmtree(WORK, ignore_errors=True)
    print("\n%s" % ("PASS" if ok else "FAIL"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
