#!/usr/bin/env python3
"""Independent verifier for the M6 Session 180 `invsync` application.

Builds `invsync.mink` with a MINK compiler, runs each case in its own
disposable working directory, and checks exact stdout, stderr, exit code and
the generated `report.txt`. It never writes inside the repository: the build
source copy and every case directory live under a temporary directory that is
removed on exit.

Usage:
    python benchmarks/m6/session180/verify.py
    python benchmarks/m6/session180/verify.py --compiler <path-to-mink>

Default compiler is the repository build at target/release/mink.exe. The
repository compiler resolves `stdlib/` from the working directory, so the
build runs with cwd set to the repository root.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[2]

GOOD_CSV = (
    "sku,category,qty,price\n"
    "bolt,fasteners,40,3\n"
    "nut,fasteners,2,2\n"
    "washer,hardware,0,1\n"
    "screw,hardware,80,1\n"
)

GOOD_REPORT = (
    "inventory report\n"
    "rows=4 total_qty=122 total_value=204\n"
    "category=fasteners qty=42\n"
    "category=hardware qty=80\n"
)

GOOD_STDOUT = (
    "invsync: ok\n"
    "rows=4 total_qty=122 total_value=204\n"
)

HEADER_ONLY_REPORT = (
    "inventory report\n"
    "rows=0 total_qty=0 total_value=0\n"
)


def norm(text: str) -> str:
    return text.replace("\r\n", "\n")


class Failure(Exception):
    pass


def build(compiler: Path, work: Path) -> Path:
    src = work / "invsync.mink"
    shutil.copyfile(HERE / "invsync.mink", src)
    exe = src.with_suffix(".exe" if os.name == "nt" else "")
    result = subprocess.run(
        [str(compiler), "build", str(src)],
        cwd=str(REPO_ROOT),
        capture_output=True,
        text=True,
    )
    if result.returncode != 0 or not exe.exists():
        raise Failure(
            "build failed\n"
            f"  command: {compiler} build {src} (cwd {REPO_ROOT})\n"
            f"  exit: {result.returncode}\n"
            f"  stdout: {result.stdout.strip()}\n"
            f"  stderr: {result.stderr.strip()}"
        )
    return exe


def run_case(
    exe: Path,
    work: Path,
    name: str,
    files: dict[str, str],
    args: list[str],
    expected_code: int,
    expected_stdout: str,
    expected_stderr: str,
    expected_report: str | None,
    report_blocker: bool = False,
) -> None:
    case = work / name
    case.mkdir()
    for filename, content in files.items():
        (case / filename).write_text(content, encoding="utf-8", newline="")
    if report_blocker:
        (case / "report.txt").mkdir()

    result = subprocess.run(
        [str(exe)] + args,
        cwd=str(case),
        capture_output=True,
        text=True,
    )
    problems: list[str] = []
    if result.returncode != expected_code:
        problems.append(f"exit code: expected {expected_code}, got {result.returncode}")
    if norm(result.stdout) != expected_stdout:
        problems.append(f"stdout: expected {expected_stdout!r}, got {norm(result.stdout)!r}")
    if norm(result.stderr) != expected_stderr:
        problems.append(f"stderr: expected {expected_stderr!r}, got {norm(result.stderr)!r}")

    report_path = case / "report.txt"
    if expected_report is None:
        if report_path.exists() and not report_blocker:
            problems.append("report.txt: expected no report, but one was written")
    elif report_blocker:
        problems.append("report.txt: expected the blocked directory to remain")
    else:
        if not report_path.exists():
            problems.append("report.txt: missing")
        else:
            with open(report_path, "r", encoding="utf-8", newline="") as fh:
                actual = fh.read()
            if norm(actual) != expected_report:
                problems.append(f"report.txt: expected {expected_report!r}, got {norm(actual)!r}")

    if problems:
        raise Failure(f"case {name} failed:\n  " + "\n  ".join(problems))
    print(f"  PASS: {name}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--compiler",
        default=str(REPO_ROOT / "target" / "release" / ("mink.exe" if os.name == "nt" else "mink")),
    )
    args = parser.parse_args()
    compiler = Path(args.compiler)
    if not compiler.exists():
        print(f"compiler not found: {compiler}", file=sys.stderr)
        return 2

    work = Path(tempfile.mkdtemp(prefix="mink-s180-"))
    try:
        exe = build(compiler, work)
        print(f"built with {compiler}")
        run_case(exe, work, "normal", {"good.csv": GOOD_CSV}, ["good.csv"], 0, GOOD_STDOUT, "", GOOD_REPORT)
        run_case(
            exe, work, "header_only", {"h.csv": "sku,category,qty,price\n"}, ["h.csv"],
            0, "invsync: ok\nrows=0 total_qty=0 total_value=0\n", "", HEADER_ONLY_REPORT,
        )
        run_case(
            exe, work, "empty_file", {"empty.csv": ""}, ["empty.csv"],
            3, "", "invsync: input is empty\n", None,
        )
        run_case(
            exe, work, "bad_field_count", {"bad.csv": "sku,category,qty,price\na,b,1\n"}, ["bad.csv"],
            3, "", "invsync: invalid row 2\n", None,
        )
        run_case(
            exe, work, "bad_number", {"bad.csv": "sku,category,qty,price\na,b,x,1\n"}, ["bad.csv"],
            3, "", "invsync: invalid row 2\n", None,
        )
        run_case(
            exe, work, "negative_qty", {"bad.csv": "sku,category,qty,price\na,b,-1,1\n"}, ["bad.csv"],
            3, "", "invsync: invalid row 2\n", None,
        )
        run_case(
            exe, work, "bad_header", {"bad.csv": "sku,qty,price\na,b,1\n"}, ["bad.csv"],
            3, "", "invsync: missing or invalid header\n", None,
        )
        run_case(
            exe, work, "missing_file", {}, ["absent.csv"],
            4, "", "invsync: cannot read input file\n", None,
        )
        run_case(
            exe, work, "usage", {}, [],
            2, "", "usage: invsync <input.csv>\n", None,
        )
        run_case(
            exe, work, "write_failure", {"in.csv": "sku,category,qty,price\na,b,1,1\n"}, ["in.csv"],
            5, "", "invsync: cannot write report.txt\n", None, report_blocker=True,
        )
        # Boundary: duplicate categories aggregate, zero qty is valid, CRLF input.
        boundary_csv = (
            "sku,category,qty,price\r\n"
            "a,alpha,0,999\r\n"
            "b,alpha,1000000,1\r\n"
            "c,beta,7,0\r\n"
        )
        boundary_report = (
            "inventory report\n"
            "rows=3 total_qty=1000007 total_value=1000000\n"
            "category=alpha qty=1000000\n"
            "category=beta qty=7\n"
        )
        run_case(
            exe, work, "boundary", {"b.csv": boundary_csv}, ["b.csv"],
            0, "invsync: ok\nrows=3 total_qty=1000007 total_value=1000000\n",
            "", boundary_report,
        )
        # Two positional arguments is a usage error, not a silent second-file read.
        run_case(
            exe, work, "extra_arg", {"good.csv": GOOD_CSV}, ["good.csv", "good.csv"],
            2, "", "usage: invsync <input.csv>\n", None,
        )
    except Failure as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1
    finally:
        shutil.rmtree(work, ignore_errors=True)

    print("all invsync cases passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
