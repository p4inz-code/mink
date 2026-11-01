#!/usr/bin/env python3
"""Author the W5 golden outputs and report files.

    python benchmarks/m4/expected/w5_manifest/make_expected.py

Independent oracle (not one of the five applications). It walks the committed
fixture tree, filters by extension the way the contract specifies, hashes with
hashlib, renders the CSV with the standard library, and writes:

  <case>.stdout                 expected summary
  <case>/out/report.csv         expected artifact (successful cases only)

The fixture tree is copied into a scratch directory under the benchmark build
tree so relative paths match what the harness produces.
"""
import csv
import hashlib
import io
import os
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
M4_DIR = os.path.dirname(os.path.dirname(HERE))
FIXTURES = os.path.join(M4_DIR, "fixtures", "w5_manifest")
SCRATCH = os.path.join(M4_DIR, ".build", "expected-w5")

SUCCESS_CASES = [
    ("baseline_100", "tree_100", [".txt", ".csv"]),
    ("uppercase_ext", "tree_100", [".TXT"]),
    ("scale_400", "tree_400", [".txt", ".csv"]),
]

ERROR_CASES = {
    "no_match": ("manifest: no file matched the extensions\r\n", 5),
    "missing_root": ("manifest: root does not exist: missing-tree\r\n", 4),
    "bad_json": ("manifest: config is not valid JSON\r\n", 3),
    "missing_keys": ("manifest: config needs root, out and extensions\r\n", 3),
    "bad_types": ("manifest: config types must be string, string, array\r\n", 3),
    "root_not_object": ("manifest: config must be a JSON object\r\n", 3),
    "missing_config": ("manifest: config does not exist: config.json\r\n", 2),
    "directory_as_config": ("manifest: not a regular file: .\r\n", 2),
    "no_args": ("manifest: usage: manifest <config.json>\r\n", 1),
    "extra_args": ("manifest: usage: manifest <config.json>\r\n", 1),
    "option_looking_arg": ("manifest: this tool has no options\r\n", 1),
}


def selected(rel, extensions):
    ext = os.path.splitext(rel)[1]
    if not ext:
        return False
    return any(ext.lower() == wanted.lower() for wanted in extensions)


def run_tree(name, tree, extensions):
    work = os.path.join(SCRATCH, name)
    if os.path.isdir(work):
        shutil.rmtree(work)
    os.makedirs(os.path.join(work, "out"))
    shutil.copytree(os.path.join(FIXTURES, tree), os.path.join(work, "tree"))

    root = os.path.join(work, "tree")
    files = []
    for base, _dirs, names in os.walk(root):
        for filename in names:
            full = os.path.join(base, filename)
            rel = os.path.relpath(full, root).replace(os.sep, "/")
            if selected(rel, extensions):
                files.append(rel)
    files.sort()
    if not files:
        return None, None, 5

    rows = [["path", "size", "sha256", "ext"]]
    total_bytes = 0
    for rel in files:
        with open(os.path.join(root, *rel.split("/")), "rb") as handle:
            data = handle.read()
        total_bytes += len(data)
        rows.append([rel, str(len(data)),
                     hashlib.sha256(data).hexdigest()[:16],
                     os.path.splitext(rel)[1]])

    buffer = io.StringIO()
    writer = csv.writer(buffer, lineterminator="\n")
    writer.writerows(rows)
    report = buffer.getvalue()

    stdout = ("files=%d\r\nbytes=%d\r\nfirst=%s\r\n"
              % (len(files), total_bytes, files[0]))
    return stdout, report, 0


def main():
    written = 0
    for case, tree, extensions in SUCCESS_CASES:
        stdout, report, _code = run_tree(case, tree, extensions)
        with open(os.path.join(HERE, case + ".stdout"), "wb") as handle:
            handle.write(stdout.encode("utf-8"))
        directory = os.path.join(HERE, case, "out")
        os.makedirs(directory, exist_ok=True)
        with open(os.path.join(directory, "report.csv"), "wb") as handle:
            handle.write(report.encode("utf-8"))
        written += 1
        print("  %-16s %6d bytes stdout, %6d bytes report"
              % (case, len(stdout.encode("utf-8")), len(report.encode("utf-8"))))

    for case, (stderr, code) in ERROR_CASES.items():
        with open(os.path.join(HERE, case + ".stderr"), "wb") as handle:
            handle.write(stderr.encode("utf-8"))
        written += 1

    print("expected: %d case outputs under %s" % (written, HERE))
    return 0


if __name__ == "__main__":
    sys.exit(main())
