#!/usr/bin/env python3
"""repoaudit contract tests.

Python is an independent oracle here, not a shipped second implementation.
The oracle recomputes inventory, violations, summary and stdout from the frozen
contract, and the MINK executable must match it byte-for-byte.

Usage:
    python benchmarks/m5/repoaudit/tests/run_tests.py
    python benchmarks/m5/repoaudit/tests/run_tests.py --no-build
    python benchmarks/m5/repoaudit/tests/run_tests.py --update-goldens

Exit status is 0 only when every check passes.
"""

from __future__ import annotations

import argparse
import contextlib
import csv
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
APP_DIR = ROOT / "benchmarks" / "m5" / "repoaudit"
SOURCE = APP_DIR / "app" / "main.mink"
BUILD_DIR = APP_DIR / "build"
EXE = BUILD_DIR / "repoaudit.exe"
FIXTURES = APP_DIR / "fixtures"
EXPECTED = APP_DIR / "expected"

REPORT_NAMES = ("inventory.csv", "violations.csv", "summary.json")


# ---------------------------------------------------------------------------
# Oracle
# ---------------------------------------------------------------------------

def _marker_count(data: bytes, marker: bytes) -> int:
    if not marker:
        return 0
    count = 0
    pos = 0
    while True:
        found = data.find(marker, pos)
        if found < 0:
            return count
        count += 1
        pos = found + len(marker)


def _csv_bytes(header: list[str], rows: list[list[object]]) -> bytes:
    out = io.StringIO(newline="")
    writer = csv.writer(out, lineterminator="\n")
    writer.writerow(header)
    for row in rows:
        writer.writerow(row)
    return out.getvalue().encode("ascii")


def audit(config: dict, cwd: Path) -> dict:
    """Recompute the complete frozen output from the contract."""
    root = cwd / config["root"]
    exts = [str(x).lower() for x in config["include_extensions"]]
    excludes = {str(x).lower() for x in config["exclude_dirs"]}
    markers = [str(x).encode("ascii") for x in config["markers"]]
    max_file_bytes = int(config["max_file_bytes"])
    max_markers = int(config["max_markers"])

    selected: list[tuple[str, Path]] = []
    for current, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(
            d for d in dirnames if d.lower() not in excludes
        )
        current_path = Path(current)
        for filename in sorted(filenames):
            index = filename.rfind(".")
            ext = filename[index:].lower() if index >= 0 else ""
            if ext not in exts:
                continue
            rel = (current_path / filename).relative_to(root).as_posix()
            selected.append((rel, current_path / filename))
    selected.sort(key=lambda item: item[0].encode("ascii"))

    inventory: list[list[object]] = []
    violations: list[list[object]] = []
    total_bytes = 0
    total_lines = 0
    total_binary = 0
    total_todo = 0
    total_fixme = 0

    for rel, path in selected:
        size = path.stat().st_size
        total_bytes += size
        todo = 0
        fixme = 0
        binary = 0
        lines = -1
        if size > max_file_bytes:
            violations.append([rel, "file_too_large", f"limit={max_file_bytes}"])
        else:
            data = path.read_bytes()
            binary = 1 if b"\x00" in data[:4096] else 0
            if binary == 0:
                lines = 0 if not data else data.count(b"\n") + (
                    0 if data.endswith(b"\n") else 1
                )
                todo = _marker_count(data, markers[0]) if markers else 0
                fixme = _marker_count(data, markers[1]) if len(markers) > 1 else 0
                if todo + fixme > max_markers:
                    violations.append(
                        [rel, "too_many_markers", f"count={todo + fixme}"]
                    )
        inventory.append([rel, size, lines, binary, todo, fixme])
        if lines > 0:
            total_lines += lines
        total_binary += binary
        total_todo += todo
        total_fixme += fixme

    summary = {
        "files": len(inventory),
        "bytes": total_bytes,
        "lines": total_lines,
        "binary": total_binary,
        "todo": total_todo,
        "fixme": total_fixme,
        "violations": len(violations),
    }
    stdout = (
        f"files={summary['files']} bytes={summary['bytes']} "
        f"lines={summary['lines']} todo={summary['todo']} "
        f"fixme={summary['fixme']} violations={summary['violations']}\r\n"
    ).encode("ascii")
    reports = {
        "inventory.csv": _csv_bytes(
            ["path", "bytes", "lines", "binary", "todo", "fixme"], inventory
        ),
        "violations.csv": _csv_bytes(["path", "rule", "detail"], violations),
        "summary.json": json.dumps(summary, separators=(",", ":")).encode("ascii"),
    }
    return {
        "code": 6 if violations else 0,
        "stdout": stdout,
        "reports": reports,
        "summary": summary,
        "inventory": inventory,
        "violations": violations,
    }


# ---------------------------------------------------------------------------
# Runner
# ---------------------------------------------------------------------------

class Suite:
    def __init__(self) -> None:
        self.passed = 0
        self.failed: list[tuple[str, str]] = []
        self.skipped: list[tuple[str, str]] = []

    def check(self, name: str, fn) -> None:
        try:
            fn()
        except SkipTest as exc:
            self.skipped.append((name, str(exc)))
            print(f"SKIP  {name}: {exc}")
        except Exception as exc:  # noqa: BLE001 - test driver reports failures
            self.failed.append((name, str(exc)))
            print(f"FAIL  {name}: {exc}")
        else:
            self.passed += 1
            print(f"PASS  {name}")


class SkipTest(Exception):
    pass


def expect(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def build_exe() -> None:
    mink = ROOT / "target" / "release" / "mink.exe"
    if not mink.exists():
        mink = ROOT / "target" / "debug" / "mink.exe"
    expect(mink.exists(), f"MINK compiler not found under {ROOT / 'target'}")
    proc = subprocess.run(
        [str(mink), "build", str(SOURCE)],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    expect(
        proc.returncode == 0,
        f"mink build failed ({proc.returncode})\n{proc.stdout}\n{proc.stderr}",
    )
    produced = SOURCE.with_suffix(".exe")
    expect(produced.exists(), f"build did not produce {produced}")
    BUILD_DIR.mkdir(parents=True, exist_ok=True)
    shutil.copy2(produced, EXE)
    print(f"built {EXE.relative_to(ROOT)} ({EXE.stat().st_size} bytes)")


def invoke(exe: Path, cwd: Path, args: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(
        [str(exe), *args],
        cwd=str(cwd),
        capture_output=True,
        timeout=30,
    )


@contextlib.contextmanager
def fixture(name: str):
    with tempfile.TemporaryDirectory(prefix="repoaudit_") as td:
        work = Path(td) / "work"
        shutil.copytree(FIXTURES / name, work)
        yield work


def read_reports(out_dir: Path) -> dict[str, bytes]:
    return {name: (out_dir / name).read_bytes() for name in REPORT_NAMES}


def check_contract(exe: Path, work: Path, expected_name: str | None = None) -> dict:
    config = json.loads((work / "config.json").read_text(encoding="utf-8"))
    oracle = audit(config, work)
    proc = invoke(exe, work, ["config.json"])
    expect(
        proc.returncode == oracle["code"],
        f"exit {proc.returncode}, expected {oracle['code']}; stderr={proc.stderr!r}",
    )
    expect(proc.stdout == oracle["stdout"], f"stdout {proc.stdout!r} != {oracle['stdout']!r}")
    expect(proc.stderr == b"", f"unexpected stderr: {proc.stderr!r}")
    reports = read_reports(work / config["out"])
    for name in REPORT_NAMES:
        expect(
            reports[name] == oracle["reports"][name],
            f"{name} differs from oracle\n"
            f"app={reports[name]!r}\noracle={oracle['reports'][name]!r}",
        )
    if expected_name is not None:
        for name in REPORT_NAMES:
            golden = (EXPECTED / expected_name / name).read_bytes()
            expect(reports[name] == golden, f"{name} differs from committed golden")
        golden_stdout = (EXPECTED / expected_name / "stdout.txt").read_bytes()
        expect(proc.stdout == golden_stdout, "stdout differs from committed golden")
    return oracle


def expect_diagnostic(exe: Path, work: Path, args: list[str], code: int) -> bytes:
    proc = invoke(exe, work, args)
    expect(proc.returncode == code, f"exit {proc.returncode}, expected {code}")
    expect(proc.stdout == b"", f"stdout must be empty on fatal error, got {proc.stdout!r}")
    expect(proc.stderr.startswith(b"repoaudit: "), f"stderr not prefixed: {proc.stderr!r}")
    return proc.stderr


def write_config(work: Path, config: dict, name: str = "config.json") -> None:
    (work / name).write_text(
        json.dumps(config, indent=2) + "\n", encoding="ascii"
    )


def base_config() -> dict:
    return json.loads((FIXTURES / "basic" / "config.json").read_text(encoding="utf-8"))


def update_goldens() -> None:
    for name in ("basic", "clean", "empty"):
        work_root = FIXTURES / name
        config = json.loads((work_root / "config.json").read_text(encoding="utf-8"))
        result = audit(config, work_root)
        out = EXPECTED / name
        out.mkdir(parents=True, exist_ok=True)
        for report_name, data in result["reports"].items():
            (out / report_name).write_bytes(data)
        (out / "stdout.txt").write_bytes(result["stdout"])
        print(f"goldens written: {out.relative_to(ROOT)}")


# ---------------------------------------------------------------------------
# Checks
# ---------------------------------------------------------------------------

def run_suite(exe: Path) -> Suite:
    suite = Suite()

    def basic_contract() -> None:
        with fixture("basic") as work:
            check_contract(exe, work, "basic")

    def clean_contract() -> None:
        with fixture("clean") as work:
            check_contract(exe, work, "clean")

    def empty_contract() -> None:
        with fixture("empty") as work:
            check_contract(exe, work, "empty")

    suite.check("basic contract + committed goldens", basic_contract)
    suite.check("clean project exits 0", clean_contract)
    suite.check("empty tree has header-only reports", empty_contract)

    def no_match() -> None:
        with fixture("basic") as work:
            cfg = base_config()
            cfg["include_extensions"] = [".zzz"]
            write_config(work, cfg)
            result = check_contract(exe, work)
            expect(result["summary"]["files"] == 0, "expected no selected files")
            expect(result["reports"]["inventory.csv"] == b"path,bytes,lines,binary,todo,fixme\n", "unexpected inventory body")

    suite.check("no matching extension yields empty inventory", no_match)

    def content_rows() -> None:
        with fixture("basic") as work:
            result = check_contract(exe, work, "basic")
            rows = {row[0]: row for row in result["inventory"]}
            expected = {
                "src/app.mink": [5, 0, 3, 1],
                "src/no_final.txt": [2, 0, 1, 0],
                "src/empty.txt": [0, 0, 0, 0],
                "src/repeat.txt": [1, 0, 2, 0],
                "docs/clean.MD": [3, 0, 0, 0],
                "space dir/file with space.txt": [1, 0, 1, 0],
                "assets/binary.txt": [-1, 1, 0, 0],
                "assets/oversized.txt": [-1, 0, 0, 0],
                "assets/nul_at_4096.txt": [1, 0, 1, 0],
            }
            for path, (lines, binary, todo, fixme) in expected.items():
                expect(path in rows, f"missing inventory row {path}")
                row = rows[path]
                expect(
                    row[2:] == [lines, binary, todo, fixme],
                    f"{path} values {row[2:]} != {[lines, binary, todo, fixme]}",
                )
            for path in (
                "target/generated.mink",
                "nested/target/deep.mink",
                "extensionless",
                "notes.csv",
                "src/ignored.rs",
            ):
                expect(path not in rows, f"unexpected row {path}")

    suite.check("line, binary, marker and exclusion semantics", content_rows)

    def violation_rows() -> None:
        with fixture("basic") as work:
            result = check_contract(exe, work, "basic")
            expect(
                result["violations"] == [
                    ["assets/oversized.txt", "file_too_large", "limit=8192"],
                    ["src/app.mink", "too_many_markers", "count=4"],
                ],
                f"unexpected violations: {result['violations']}",
            )

    suite.check("violation ordering and details", violation_rows)

    def repeated_runs() -> None:
        with fixture("basic") as work:
            first = check_contract(exe, work, "basic")
            snapshots = []
            for _ in range(3):
                proc = invoke(exe, work, ["config.json"])
                expect(proc.returncode == 6, "repeat run must still exit 6")
                snapshots.append(
                    (
                        proc.stdout,
                        read_reports(work / "out"),
                    )
                )
            for stdout, reports in snapshots:
                expect(stdout == first["stdout"], "stdout changed across runs")
                expect(reports == first["reports"], "report bytes changed across runs")

    suite.check("three repeated runs are byte-identical", repeated_runs)

    def help_ok() -> None:
        with fixture("basic") as work:
            proc = invoke(exe, work, ["--help"])
            expect(proc.returncode == 0, f"help exit {proc.returncode}")
            expect(b"repoaudit" in proc.stdout, "help text missing")
            expect(proc.stderr == b"", "help wrote stderr")

    suite.check("--help exits 0 on stdout", help_ok)

    usage_cases = [
        ("missing argument", []),
        ("extra argument", ["config.json", "extra"]),
        ("unknown option", ["--wat"]),
        ("help with extra argument", ["--help", "extra"]),
    ]
    def usage_case(args: list[str]) -> None:
        with fixture("basic") as work:
            expect_diagnostic(exe, work, args, 1)

    for label, args in usage_cases:
        suite.check(
            f"usage error: {label}",
            lambda args=args: usage_case(args),
        )

    def missing_config() -> None:
        with fixture("basic") as work:
            expect_diagnostic(exe, work, ["absent.json"], 2)

    suite.check("config missing -> 2", missing_config)

    def config_is_dir() -> None:
        with fixture("basic") as work:
            expect_diagnostic(exe, work, ["tree"], 2)

    suite.check("config not a regular file -> 2", config_is_dir)

    def invalid_json() -> None:
        with fixture("basic") as work:
            (work / "config.json").write_text("{", encoding="ascii")
            expect_diagnostic(exe, work, ["config.json"], 3)

    suite.check("invalid JSON -> 3", invalid_json)

    def not_object() -> None:
        with fixture("basic") as work:
            (work / "config.json").write_text("[]", encoding="ascii")
            expect_diagnostic(exe, work, ["config.json"], 3)

    suite.check("non-object config -> 3", not_object)

    def duplicate_key() -> None:
        with fixture("basic") as work:
            (work / "config.json").write_text(
                '{"root":"tree","root":"tree","out":"out",'
                '"include_extensions":[".mink"],"exclude_dirs":[],'
                '"markers":["TODO"],"max_file_bytes":100,"max_markers":1}',
                encoding="ascii",
            )
            expect_diagnostic(exe, work, ["config.json"], 3)

    suite.check("duplicate config key -> 3", duplicate_key)

    schema_cases = [
        ("missing key", lambda c: c.pop("markers")),
        ("extra key", lambda c: c.__setitem__("extra", 1)),
        ("root wrong type", lambda c: c.__setitem__("root", 5)),
        ("root empty", lambda c: c.__setitem__("root", "")),
        ("out empty", lambda c: c.__setitem__("out", "")),
        ("extensions empty", lambda c: c.__setitem__("include_extensions", [])),
        ("extensions too many", lambda c: c.__setitem__("include_extensions", [".a"] * 9)),
        ("extension lacks dot", lambda c: c.__setitem__("include_extensions", ["mink"])),
        ("extension too long", lambda c: c.__setitem__("include_extensions", [".abcdefghi"])),
        ("extension non-alnum", lambda c: c.__setitem__("include_extensions", [".min-k"])),
        ("exclude with slash", lambda c: c.__setitem__("exclude_dirs", ["a/b"])),
        ("exclude empty name", lambda c: c.__setitem__("exclude_dirs", [""])),
        ("excludes too many", lambda c: c.__setitem__("exclude_dirs", ["d"] * 9)),
        ("markers empty", lambda c: c.__setitem__("markers", [])),
        ("markers too many", lambda c: c.__setitem__("markers", ["A"] * 5)),
        ("marker non-token", lambda c: c.__setitem__("markers", ["TO-DO"])),
        ("marker too long", lambda c: c.__setitem__("markers", ["A" * 17])),
        ("max_file_bytes zero", lambda c: c.__setitem__("max_file_bytes", 0)),
        ("max_file_bytes too high", lambda c: c.__setitem__("max_file_bytes", 1048577)),
        ("max_file_bytes float", lambda c: c.__setitem__("max_file_bytes", 1.5)),
        ("max_file_bytes string", lambda c: c.__setitem__("max_file_bytes", "5")),
        ("max_markers negative", lambda c: c.__setitem__("max_markers", -1)),
        ("max_markers too high", lambda c: c.__setitem__("max_markers", 1001)),
        ("max_markers string", lambda c: c.__setitem__("max_markers", "5")),
    ]

    def schema_case(mutate) -> None:
        with fixture("basic") as work:
            cfg = base_config()
            mutate(cfg)
            write_config(work, cfg)
            expect_diagnostic(exe, work, ["config.json"], 3)

    for label, mutate in schema_cases:
        suite.check(f"schema: {label} -> 3", lambda mutate=mutate: schema_case(mutate))

    def root_errors() -> None:
        with fixture("basic") as work:
            cfg = base_config()
            cfg["root"] = "missing"
            write_config(work, cfg)
            expect_diagnostic(exe, work, ["config.json"], 4)
            cfg = base_config()
            cfg["root"] = "tree/README.md"
            write_config(work, cfg)
            expect_diagnostic(exe, work, ["config.json"], 4)

    suite.check("root missing or not a directory -> 4", root_errors)

    def out_errors() -> None:
        with fixture("basic") as work:
            cfg = base_config()
            cfg["out"] = "missing_out"
            write_config(work, cfg)
            expect_diagnostic(exe, work, ["config.json"], 5)
            cfg = base_config()
            cfg["out"] = "tree/README.md"
            write_config(work, cfg)
            expect_diagnostic(exe, work, ["config.json"], 5)
            cfg = base_config()
            write_config(work, cfg)
            (work / "out" / "inventory.csv").mkdir()
            expect_diagnostic(exe, work, ["config.json"], 5)

    suite.check("out missing, not a directory, or report blocked -> 5", out_errors)

    def config_with_space() -> None:
        with fixture("basic") as work:
            shutil.copy2(work / "config.json", work / "policy config.json")
            proc = invoke(exe, work, ["policy config.json"])
            expect(proc.returncode == 6, f"exit {proc.returncode}")
            cfg = json.loads((work / "config.json").read_text(encoding="utf-8"))
            oracle = audit(cfg, work)
            expect(proc.stdout == oracle["stdout"], "stdout mismatch")
            expect(read_reports(work / "out") == oracle["reports"], "report mismatch")

    suite.check("config path containing a space", config_with_space)

    def out_with_space() -> None:
        with fixture("basic") as work:
            cfg = base_config()
            cfg["out"] = "report out"
            (work / "report out").mkdir()
            write_config(work, cfg)
            oracle = audit(cfg, work)
            proc = invoke(exe, work, ["config.json"])
            expect(proc.returncode == 6, f"exit {proc.returncode}")
            expect(read_reports(work / "report out") == oracle["reports"], "report mismatch")

    suite.check("out directory containing a space", out_with_space)

    def stdout_contract() -> None:
        with fixture("basic") as work:
            proc = invoke(exe, work, ["config.json"])
            expect(proc.stdout.endswith(b"\r\n"), "stdout must end with CRLF")
            expect(proc.stdout.count(b"\r\n") == 1, "stdout must be one line")
            expect(b"\n" not in proc.stdout.replace(b"\r\n", b""), "stdout has a bare LF")
            expect(proc.stdout.startswith(b"files="), "stdout prefix")

    suite.check("stdout is exactly one CRLF line", stdout_contract)

    def casing_and_boundaries() -> None:
        with fixture("basic") as work:
            (work / "tree" / "docs" / "lowercase.txt").write_bytes(
                b"todo fixme TODO\n"
            )
            cfg = base_config()
            cfg["exclude_dirs"] = ["TARGET"]
            cfg["max_file_bytes"] = 8193  # exactly the oversized fixture size
            cfg["max_markers"] = 4       # exactly src/app.mink's count
            write_config(work, cfg)
            result = check_contract(exe, work)
            rows = {row[0]: row for row in result["inventory"]}
            expect(result["code"] == 0, "inclusive boundaries must be clean")
            expect(result["violations"] == [], "boundary run produced violations")
            expect("target/generated.mink" not in rows, "exclude must be case-insensitive")
            expect("nested/target/deep.mink" not in rows, "nested exclude must be case-insensitive")
            expect(rows["docs/lowercase.txt"][4:] == [1, 0], "markers must be case-sensitive")
            expect(rows["assets/oversized.txt"][2] == 1, "equal max_file_bytes must be read")

    suite.check("case sensitivity and inclusive boundaries", casing_and_boundaries)

    def csv_quoting() -> None:
        with fixture("basic") as work:
            (work / "tree" / "docs" / "comma,name.txt").write_bytes(b"TODO\n")
            cfg = base_config()
            cfg["max_markers"] = 100
            write_config(work, cfg)
            result = check_contract(exe, work)
            expect(
                b'"docs/comma,name.txt"' in result["reports"]["inventory.csv"],
                "comma path was not CSV-quoted",
            )

    suite.check("CSV quoting for a comma path", csv_quoting)

    def unreadable_scan() -> None:
        if sys.platform != "win32":
            raise SkipTest("Windows sharing lock needed for scan-I/O test")
        import ctypes
        from ctypes import wintypes

        with fixture("basic") as work:
            target = work / "tree" / "src" / "app.mink"
            kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
            kernel32.CreateFileW.argtypes = [
                wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD,
                wintypes.HANDLE,
            ]
            kernel32.CreateFileW.restype = wintypes.HANDLE
            kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
            handle = kernel32.CreateFileW(
                str(target), 0x80000000, 0, None, 3, 0x80, None
            )
            invalid = ctypes.c_void_p(-1).value
            if handle in (None, invalid):
                raise SkipTest("could not acquire an exclusive sharing lock")
            try:
                expect_diagnostic(exe, work, ["config.json"], 7)
                for name in REPORT_NAMES:
                    expect(
                        not (work / "out" / name).exists(),
                        f"scan failure must not leave {name}",
                    )
            finally:
                kernel32.CloseHandle(handle)

    suite.check("unreadable file during scan -> 7", unreadable_scan)

    def standalone() -> None:
        with fixture("basic") as work, tempfile.TemporaryDirectory(prefix="repoaudit_bin_") as td:
            isolated = Path(td) / "bin"
            isolated.mkdir()
            exe2 = isolated / "repoaudit.exe"
            shutil.copy2(exe, exe2)
            proc = invoke(exe2, work, ["config.json"])
            expect(proc.returncode == 6, f"isolated exe exit {proc.returncode}")
            expect(proc.stderr == b"", f"isolated exe stderr {proc.stderr!r}")
            cfg = json.loads((work / "config.json").read_text(encoding="utf-8"))
            oracle = audit(cfg, work)
            expect(read_reports(work / "out") == oracle["reports"], "isolated reports differ")

    suite.check("standalone exe outside the source tree", standalone)

    return suite


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--no-build", action="store_true", help="use the existing exe")
    parser.add_argument("--update-goldens", action="store_true", help="rewrite expected/ from the oracle")
    args = parser.parse_args()

    if not args.no_build:
        build_exe()
    expect(EXE.exists(), f"missing {EXE}; run without --no-build")

    if args.update_goldens:
        update_goldens()

    suite = run_suite(EXE)
    total = suite.passed + len(suite.failed) + len(suite.skipped)
    print()
    print(
        f"summary: {suite.passed} passed, {len(suite.failed)} failed, "
        f"{len(suite.skipped)} skipped, {total} checks"
    )
    if suite.failed:
        print("\nfailures:")
        for name, message in suite.failed:
            print(f"  - {name}: {message}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
