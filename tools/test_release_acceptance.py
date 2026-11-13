#!/usr/bin/env python3
"""Failure-injection tests for tools/release_acceptance.py.

The tests use a fake cargo executable so no MINK build or test is run. They
exercise the runner's reporting, verdict and exit-code behaviour, including
the distinction between ignored and failed tests.

Run with:
    python tools/test_release_acceptance.py
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
REPO = TOOLS.parent
RUNNER = TOOLS / "release_acceptance.py"

FAKE_CARGO = r'''
import os
import pathlib
import sys

args = sys.argv[1:]
mode = os.environ.get("FAKE_CARGO_MODE", "ok")
target = pathlib.Path(os.environ["CARGO_TARGET_DIR"])


def suite_output(failing=False, ignored=True):
    print("     Running unittests src\\lib.rs (target\\release\\deps\\mink.exe)")
    print("running 2 tests")
    print("test lib_a ... ok")
    print("test lib_b ... ok")
    print("test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s")
    print("     Running tests\\adversarial.rs (target\\release\\deps\\adversarial.exe)")
    print("running 3 tests")
    print("test adv_a ... ok")
    if failing:
        print("test adv_b ... FAILED")
        print("test adv_c ... ok")
        print("test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s")
    else:
        print("test adv_b ... ok")
        print("test adv_c ... ok")
        print("test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s")
    print("     Running tests\\soak.rs (target\\release\\deps\\soak.exe)")
    print("running 2 tests")
    print("test soak01 ... ok")
    if ignored:
        print("test soak02 ... ignored")
    print("test result: ok. 1 passed; 0 failed; %d ignored; 0 measured; 0 filtered out; finished in 0.03s" % (1 if ignored else 0))
    print("   Doc-tests mink")
    print("running 0 tests")
    print("test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s")


if args[:1] == ["build"]:
    if mode == "build_fail":
        print("error[E0001]: injected build failure", file=sys.stderr)
        sys.exit(42)
    if mode == "no_artifact":
        print("Finished `release` profile [optimized]")
        sys.exit(0)
    out = target / "release"
    out.mkdir(parents=True, exist_ok=True)
    (out / "mink.exe").write_text("fake mink binary\n", encoding="utf-8")
    print("Compiling mink v1.0.5")
    print("Finished `release` profile [optimized] target(s) in 0.01s")
    sys.exit(0)

if args[:1] == ["test"] and "--no-run" in args:
    if mode == "test_compile_fail":
        print("error: could not compile `mink` (test \"adversarial\")", file=sys.stderr)
        sys.exit(101)
    print("Finished `release` profile [optimized] target(s)")
    sys.exit(0)

if args[:1] == ["test"]:
    if mode == "bad_output":
        print("cargo produced no suite output")
        sys.exit(0)
    suite_output(failing=(mode == "test_fail"))
    sys.exit(101 if mode == "test_fail" else 0)

print("unsupported fake cargo invocation: %r" % (args,), file=sys.stderr)
sys.exit(64)
'''


class AcceptanceRunnerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory(prefix="mink-acceptance-")
        self.root = Path(self.tmp.name)
        self.fake = self.root / "fake_cargo.py"
        self.fake.write_text(FAKE_CARGO, encoding="utf-8")
        self.out = self.root / "reports"

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def run_tool(self, mode: str, out: Path | None = None, session: str = "test", extra: list[str] | None = None, cargo: str | None = None):
        env = os.environ.copy()
        env["MINK_ACCEPTANCE_CARGO"] = cargo if cargo is not None else f"{Path(sys.executable).as_posix()} {self.fake.as_posix()}"
        env["FAKE_CARGO_MODE"] = mode
        command = [
            sys.executable,
            str(RUNNER),
            "--repo",
            str(REPO),
            "--out",
            str(out or self.out),
            "--session",
            session,
            "--label",
            "selftest",
            "--allow-dirty",
        ]
        command.extend(extra or [])
        return subprocess.run(command, capture_output=True, text=True, env=env, timeout=180)

    def report_dirs(self, out: Path | None = None) -> list[Path]:
        root = (out or self.out) / "test"
        return sorted(path for path in root.iterdir() if path.is_dir()) if root.exists() else []

    def load(self, run: Path) -> dict:
        return json.loads((run / "report.json").read_text(encoding="utf-8"))

    def test_success_generates_reports_and_pass(self) -> None:
        result = self.run_tool("ok")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        runs = self.report_dirs()
        self.assertEqual(len(runs), 1)
        report = self.load(runs[0])
        self.assertEqual(report["verdict"], "PASS")
        self.assertEqual(report["exit_code"], 0)
        self.assertEqual(report["schema_version"], 1)
        self.assertEqual(report["repository"]["head"], subprocess.run(["git", "-C", str(REPO), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip())
        self.assertTrue(report["artifact"]["found"])
        artifact = Path(report["artifact"]["path"])
        self.assertEqual(report["artifact"]["size_bytes"], artifact.stat().st_size)
        self.assertEqual(report["artifact"]["sha256"], hashlib.sha256(artifact.read_bytes()).hexdigest())
        totals = report["tests"]["totals"]
        self.assertEqual(report["tests"]["totals"]["total_suites"], 4)
        self.assertEqual((totals["passed"], totals["failed"], totals["ignored"]), (6, 0, 1))
        self.assertEqual(totals["ignored_tests"], [{"suite": "soak", "test": "soak02"}])
        self.assertTrue((runs[0] / "report.md").is_file())
        self.assertTrue((runs[0] / "logs" / "build.log").is_file())
        self.assertTrue((runs[0] / "logs" / "test-compile.log").is_file())
        self.assertTrue((runs[0] / "logs" / "tests.log").is_file())
        markdown = (runs[0] / "report.md").read_text(encoding="utf-8")
        self.assertIn("**Verdict: PASS**", markdown)
        self.assertIn("soak02", markdown)

    def test_build_failure_propagates_and_still_reports(self) -> None:
        result = self.run_tool("build_fail")
        self.assertEqual(result.returncode, 42, result.stdout + result.stderr)
        report = self.load(self.report_dirs()[0])
        self.assertEqual(report["verdict"], "FAIL")
        self.assertEqual(report["exit_code"], 42)
        build = report["stages"][0]
        self.assertEqual(build["exit_code"], 42)
        self.assertEqual(build["status"], "fail")
        self.assertEqual([stage["status"] for stage in report["stages"][1:]], ["skipped", "skipped"])
        self.assertIsNone(report["tests"])
        self.assertTrue(any("build exited 42" in item for item in report["failures"]))

    def test_test_failure_is_distinguished_from_ignored(self) -> None:
        result = self.run_tool("test_fail")
        self.assertEqual(result.returncode, 101, result.stdout + result.stderr)
        report = self.load(self.report_dirs()[0])
        totals = report["tests"]["totals"]
        self.assertEqual(report["verdict"], "FAIL")
        self.assertEqual(totals["failed"], 1)
        self.assertEqual(totals["ignored"], 1)
        self.assertEqual(totals["passed"], 5)
        self.assertEqual(report["tests"]["failed_tests"], [{"suite": "adversarial", "test": "adv_b"}])
        self.assertEqual(totals["ignored_tests"], [{"suite": "soak", "test": "soak02"}])

    def test_ignored_test_alone_does_not_fail(self) -> None:
        result = self.run_tool("ok")
        self.assertEqual(result.returncode, 0)
        report = self.load(self.report_dirs()[0])
        self.assertEqual(report["tests"]["totals"]["ignored"], 1)
        self.assertEqual(report["tests"]["totals"]["failed"], 0)
        self.assertEqual(report["verdict"], "PASS")

    def test_missing_tool_is_blocked_and_reported(self) -> None:
        result = self.run_tool("ok", cargo="mink-acceptance-no-such-cargo-xyz")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        report = self.load(self.report_dirs()[0])
        self.assertEqual(report["verdict"], "BLOCKED")
        self.assertEqual(report["stages"], [])
        self.assertIn("cargo", report["limitations"][0])
        self.assertTrue((self.report_dirs()[0] / "report.md").is_file())

    def test_missing_binary_is_partial(self) -> None:
        result = self.run_tool("no_artifact")
        self.assertEqual(result.returncode, 3, result.stdout + result.stderr)
        report = self.load(self.report_dirs()[0])
        self.assertEqual(report["verdict"], "PARTIAL")
        self.assertFalse(report["artifact"]["found"])
        self.assertTrue(any("artifact was not found" in item for item in report["limitations"]))

    def test_unparsable_test_output_is_partial(self) -> None:
        result = self.run_tool("bad_output")
        self.assertEqual(result.returncode, 3)
        report = self.load(self.report_dirs()[0])
        self.assertEqual(report["verdict"], "PARTIAL")
        self.assertEqual(report["tests"]["totals"]["total_suites"], 0)

    def test_test_compile_failure_skips_regression(self) -> None:
        result = self.run_tool("test_compile_fail")
        self.assertEqual(result.returncode, 101)
        report = self.load(self.report_dirs()[0])
        self.assertEqual(report["verdict"], "FAIL")
        self.assertEqual(report["stages"][1]["status"], "fail")
        self.assertEqual(report["stages"][2]["status"], "skipped")
        self.assertIsNone(report["tests"])

    def test_repeated_runs_preserve_previous_reports(self) -> None:
        first = self.run_tool("ok")
        self.assertEqual(first.returncode, 0)
        runs = self.report_dirs()
        self.assertEqual(len(runs), 1)
        original = (runs[0] / "report.json").read_bytes()
        second = self.run_tool("ok")
        self.assertEqual(second.returncode, 0)
        runs = self.report_dirs()
        self.assertEqual(len(runs), 2)
        self.assertEqual((runs[0] / "report.json").read_bytes(), original)
        self.assertNotEqual(runs[0].name, runs[1].name)

    def test_paths_with_spaces(self) -> None:
        spaced = self.root / "reports with spaces"
        fake_dir = self.root / "fake cargo dir"
        fake_dir.mkdir()
        fake = fake_dir / "fake_cargo.py"
        fake.write_text(FAKE_CARGO, encoding="utf-8")
        cargo = f'"{Path(sys.executable).as_posix()}" "{fake.as_posix()}"'
        result = self.run_tool("ok", out=spaced, cargo=cargo)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        runs = self.report_dirs(spaced)
        self.assertEqual(len(runs), 1)
        report = self.load(runs[0])
        self.assertEqual(report["verdict"], "PASS")
        self.assertIn("reports with spaces", str(runs[0]))
        self.assertIn("fake cargo dir", report["stages"][0]["command_line"])
        self.assertTrue((runs[0] / "report.md").is_file())

    def test_tracked_changes_mark_partial(self) -> None:
        env = os.environ.copy()
        env["MINK_ACCEPTANCE_CARGO"] = f"{Path(sys.executable).as_posix()} {self.fake.as_posix()}"
        env["FAKE_CARGO_MODE"] = "ok"
        repo = self.root / "dirty repo"
        repo.mkdir()
        (repo / "Cargo.toml").write_text("[package]\nname = \"fake\"\nversion = \"0.1.0\"\n", encoding="utf-8")
        subprocess.run(["git", "init", "-q", str(repo)], check=True)
        subprocess.run(["git", "-C", str(repo), "add", "Cargo.toml"], check=True)
        subprocess.run(["git", "-C", str(repo), "-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "init"], check=True)
        (repo / "Cargo.toml").write_text("[package]\nname = \"fake\"\nversion = \"0.2.0\"\n", encoding="utf-8")
        command = [sys.executable, str(RUNNER), "--repo", str(repo), "--out", str(self.out), "--session", "test"]
        result = subprocess.run(command, capture_output=True, text=True, env=env, timeout=180)
        self.assertEqual(result.returncode, 3, result.stdout + result.stderr)
        report = self.load(self.report_dirs()[0])
        self.assertEqual(report["verdict"], "PARTIAL")
        self.assertTrue(any("tracked files differ" in item for item in report["limitations"]))


if __name__ == "__main__":
    unittest.main(verbosity=2)
