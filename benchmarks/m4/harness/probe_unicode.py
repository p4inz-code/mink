#!/usr/bin/env python3
"""W5 Unicode-path probe: capability evidence, never a gate.

    python benchmarks/m4/harness/probe_unicode.py

Creates three UTF-8-named files (plus one ASCII control file) in a work
directory per language, runs `manifest` through the normal run command and
records exit code, stdout, stderr and the report bytes/hashes it produced.

The probe does not pass or fail, and nothing here changes a scored result.
MINK's filesystem layer uses ANSI Win32 APIs, so its behaviour on these names
is documented limitation evidence (see M4_SESSION_156.md), not a defect to be
worked around inside the benchmark.
"""
import json
import os
import shutil
import subprocess
import sys

import common

CASES = [
    ("na\u00efve caf\u00e9.txt", b"latin-1 supplement and latin small letter i with diaeresis\n"),
    ("\u65e5\u672c\u8a9e.txt", b"japanese name, utf-8 content\n"),
    ("plain.txt", b"ascii control file\n"),
]

OUT = os.path.join(common.RAW_ROOT, "w5_manifest", "unicode-probe.json")


def prepare(language):
    work = common.work_dir("w5_manifest", "unicode_probe", language)
    if os.path.isdir(work):
        shutil.rmtree(work)
    tree = os.path.join(work, "tree")
    os.makedirs(os.path.join(tree, "nested"), exist_ok=True)
    os.makedirs(os.path.join(work, "out"), exist_ok=True)
    for name, data in CASES:
        with open(os.path.join(tree, name), "wb") as handle:
            handle.write(data)
    with open(os.path.join(tree, "nested", "caf\u00e9 notes.txt"), "wb") as handle:
        handle.write(b"nested unicode name\n")
    with open(os.path.join(work, "config.json"), "w", encoding="utf-8",
              newline="\n") as handle:
        json.dump({"root": "tree", "out": "out/report.csv",
                   "extensions": [".txt"]}, handle)
    return work


def run_language(language):
    work = prepare(language)
    command = common.run_command(language, "w5_manifest") + ["config.json"]
    try:
        completed = subprocess.run(command, cwd=work, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE)
        exit_code = completed.returncode
        stdout = completed.stdout
        stderr = completed.stderr
    except FileNotFoundError:
        return {"language": language, "status": "not-built",
                "command": command[0]}

    report = os.path.join(work, "out", "report.csv")
    report_text = None
    if os.path.exists(report):
        with open(report, "rb") as handle:
            report_text = handle.read().decode("utf-8", "backslashreplace")
    directories = sorted(os.listdir(os.path.join(work, "tree")))
    return {
        "language": language,
        "status": "ran",
        "exit_code": exit_code,
        "stdout": stdout.decode("utf-8", "backslashreplace"),
        "stderr": stderr.decode("utf-8", "backslashreplace"),
        "report": report_text,
        "tree_entries_seen_by_os": directories,
        "unicode_names_round_trip": all(
            name in directories for name, _ in CASES if not name.isascii()
        ),
    }


def main():
    results = []
    for language in common.LANGUAGES:
        record = run_language(language)
        results.append(record)
        print("%-7s %s" % (language, record["status"]))
        if record["status"] == "ran":
            # ascii() keeps the console's code page out of the evidence path.
            print("  exit=%s stdout=%s stderr=%s"
                  % (record["exit_code"], ascii(record["stdout"]),
                     ascii(record["stderr"])))
            print("  report=%s" % ascii(record["report"]))

    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8") as handle:
        json.dump({
            "probe": "w5_manifest unicode-path",
            "gating": False,
            "note": "Capability evidence only. Not a scored case, not a timing run.",
            "files": [name for name, _ in CASES],
            "results": results,
        }, handle, indent=1)
    print("evidence: %s" % OUT)
    return 0


if __name__ == "__main__":
    sys.exit(main())
