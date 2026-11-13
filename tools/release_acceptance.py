#!/usr/bin/env python3
"""Repository-native MINK release acceptance runner.

Runs the authoritative release build, release test compilation and the full
release regression in a fresh Cargo target directory, captures evidence, and
writes a JSON plus a Markdown report. The verdict is computed from the stage
results, not from the success of this script.

Exit codes:
    0  PASS
    1  FAIL with no usable stage exit code
    2  BLOCKED (a required prerequisite is unavailable)
    3  PARTIAL (execution completed, acceptance evidence is incomplete)
    *  the failing stage's own exit code (for example 42 or 101)

Usage:
    python tools/release_acceptance.py --session 276

Reports go to target/acceptance/<session>/<run-id>/ by default. Build output
goes to target/acceptance-work/<run-id>/cargo-target/. Both are git-ignored.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import time
from pathlib import Path

SCHEMA_VERSION = 1
EXIT_PASS = 0
EXIT_FAIL = 1
EXIT_BLOCKED = 2
EXIT_PARTIAL = 3
DEFAULT_TIMEOUT = 1800.0

RUNNING_UNITTEST_RE = re.compile(r"^\s*Running\s+unittests\s+(\S+)")
RUNNING_TEST_RE = re.compile(r"^\s*Running\s+tests[\\/](.+?)\s+\(")
DOC_TEST_RE = re.compile(r"^\s*Doc-tests\s+(\S+)")
RESULT_RE = re.compile(
    r"test result: (?P<status>ok|FAILED)\. "
    r"(?P<passed>\d+) passed; (?P<failed>\d+) failed; "
    r"(?P<ignored>\d+) ignored; (?P<measured>\d+) measured; "
    r"(?P<filtered>\d+) filtered out; finished in (?P<secs>[0-9.]+)s"
)
IGNORED_TEST_RE = re.compile(r"^\s*test\s+(.+?)\s+\.\.\.\s+ignored\b")
FAILED_TEST_RE = re.compile(r"^\s*test\s+(.+?)\s+\.\.\.\s+FAILED\b")


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def run_id_time() -> str:
    return dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def command_string(command: list[str]) -> str:
    return " ".join(shlex.quote(part) for part in command)


def resolve_command(value: str) -> list[str]:
    """Split an env-provided command line (used for test wrappers)."""
    return shlex.split(value, posix=True)


def executable_available(command: list[str]) -> bool:
    if not command:
        return False
    head = command[0]
    if os.path.sep in head or (os.altsep and os.altsep in head):
        return Path(head).is_file()
    return shutil.which(head) is not None


def capture(command: list[str], timeout: float = 60.0) -> dict:
    started = time.perf_counter()
    try:
        completed = subprocess.run(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            stdin=subprocess.DEVNULL,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=timeout,
        )
        output = completed.stdout or ""
        code = completed.returncode
    except subprocess.TimeoutExpired as exc:
        output = (exc.stdout or "") if isinstance(exc.stdout, str) else ""
        code = 124
    except OSError as exc:
        output = f"could not execute {command[0]}: {exc}"
        code = 126
    return {
        "exit_code": code,
        "output": output.strip(),
        "duration_seconds": round(time.perf_counter() - started, 3),
    }


def gather_environment(repo: Path, cargo: list[str], rustc: list[str], git: list[str]) -> dict:
    rustc_version = capture(rustc + ["--version"])
    rustc_verbose = capture(rustc + ["-vV"])
    cargo_version = capture(cargo + ["--version"])
    host = None
    for line in rustc_verbose["output"].splitlines():
        if line.startswith("host:"):
            host = line.split(":", 1)[1].strip()
    target = None
    for line in rustc_verbose["output"].splitlines():
        if line.startswith("LLVM version:"):
            target = line.split(":", 1)[1].strip()
    branch = capture(git + ["-C", str(repo), "rev-parse", "--abbrev-ref", "HEAD"])
    head = capture(git + ["-C", str(repo), "rev-parse", "HEAD"])
    status = capture(git + ["-C", str(repo), "status", "--porcelain=v1", "--untracked-files=all"])
    return {
        "cargo_version": cargo_version["output"] or None,
        "rustc_version": rustc_version["output"] or None,
        "host": host,
        "llvm": target,
        "branch": branch["output"] or None,
        "head": head["output"] or None,
        "status_entries": [line for line in status["output"].splitlines() if line.strip()],
        "status_exit_code": status["exit_code"],
    }


def cargo_configuration(repo: Path) -> dict:
    config_path = None
    for candidate in (repo / ".cargo" / "config.toml", repo / ".cargo" / "config"):
        if candidate.is_file():
            config_path = candidate
            break
    config_text = config_path.read_text(encoding="utf-8", errors="replace") if config_path else ""
    manifest_path = repo / "Cargo.toml"
    manifest = manifest_path.read_text(encoding="utf-8", errors="replace") if manifest_path.is_file() else ""
    profile_lines: list[str] = []
    in_profile = False
    for line in manifest.splitlines():
        stripped = line.strip()
        if stripped.startswith("["):
            in_profile = stripped == "[profile.release]"
        if in_profile:
            profile_lines.append(line.rstrip())
    profile_text = "\n".join(profile_lines).strip()
    return {
        "cargo_config_path": str(config_path.relative_to(repo)) if config_path else None,
        "cargo_config": config_text.strip() or None,
        "release_profile": profile_text or "Cargo default release profile (no [profile.release] overrides)",
        "dependencies": _dependency_count(manifest),
    }


def _dependency_count(manifest: str) -> int:
    count = 0
    section = None
    for line in manifest.splitlines():
        stripped = line.strip()
        if stripped.startswith("["):
            section = stripped
            continue
        if section in ("[dependencies]", "[build-dependencies]", "[dev-dependencies]") and "=" in stripped:
            count += 1
    return count


def unique_run_dir(parent: Path, base: str) -> Path:
    candidate = parent / base
    suffix = 2
    while candidate.exists():
        candidate = parent / f"{base}-{suffix}"
        suffix += 1
    candidate.mkdir(parents=True)
    return candidate


def run_stage(name: str, command: list[str], log_path: Path, env: dict, timeout: float) -> dict:
    started_at = utc_now()
    started = time.perf_counter()
    log_path.parent.mkdir(parents=True, exist_ok=True)
    status = "fail"
    code = 1
    output = ""
    with log_path.open("w", encoding="utf-8", newline="\n") as handle:
        handle.write(f"# command: {command_string(command)}\n")
        handle.write(f"# started_at: {started_at}\n")
        handle.flush()
        try:
            completed = subprocess.run(
                command,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                stdin=subprocess.DEVNULL,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=timeout,
            )
            output = completed.stdout or ""
            code = completed.returncode
            status = "pass" if code == 0 else "fail"
        except subprocess.TimeoutExpired as exc:
            output = (exc.stdout or "") if isinstance(exc.stdout, str) else ""
            code = 124
            status = "timeout"
        handle.write(output)
        handle.write(f"\n# exit_code: {code}\n")
    duration = round(time.perf_counter() - started, 3)
    return {
        "name": name,
        "command": command,
        "command_line": command_string(command),
        "exit_code": code,
        "status": status,
        "started_at": started_at,
        "finished_at": utc_now(),
        "duration_seconds": duration,
        "log": f"logs/{log_path.name}",
        "output": output,
    }


def parse_cargo_test(output: str) -> dict:
    suites: list[dict] = []
    current = None
    ignored: list[dict] = []
    failed_tests: list[dict] = []
    orphans = 0

    def close_current(status: str) -> None:
        nonlocal current
        if current is not None:
            current["status"] = status
            suites.append(current)
            current = None

    for line in output.splitlines():
        match = RUNNING_UNITTEST_RE.match(line)
        if match:
            close_current("no result")
            path = match.group(1).replace("\\", "/")
            current = {"name": _unit_name(path), "passed": None, "failed": None, "ignored": None, "duration_seconds": None}
            continue
        match = RUNNING_TEST_RE.match(line)
        if match:
            close_current("no result")
            stem = Path(match.group(1).replace("\\", "/")).stem
            current = {"name": stem, "passed": None, "failed": None, "ignored": None, "duration_seconds": None}
            continue
        match = DOC_TEST_RE.match(line)
        if match:
            close_current("no result")
            current = {"name": "doc_tests", "passed": None, "failed": None, "ignored": None, "duration_seconds": None}
            continue
        match = RESULT_RE.search(line)
        if match:
            if current is None:
                orphans += 1
                current = {"name": f"unattributed_{orphans}", "passed": None, "failed": None, "ignored": None, "duration_seconds": None}
            current["passed"] = int(match.group("passed"))
            current["failed"] = int(match.group("failed"))
            current["ignored"] = int(match.group("ignored"))
            current["duration_seconds"] = float(match.group("secs"))
            close_current("failed" if int(match.group("failed")) else "ok")
            continue
        match = IGNORED_TEST_RE.match(line)
        if match and current is not None:
            ignored.append({"suite": current["name"], "test": match.group(1).strip()})
            continue
        match = FAILED_TEST_RE.match(line)
        if match and current is not None:
            failed_tests.append({"suite": current["name"], "test": match.group(1).strip()})

    close_current("no result")
    unexecuted = [suite["name"] for suite in suites if suite["passed"] is None]
    complete = [suite for suite in suites if suite["passed"] is not None]
    totals = {
        "total_suites": len(suites),
        "completed_suites": len(complete),
        "passed": sum(suite["passed"] for suite in complete),
        "failed": sum(suite["failed"] for suite in complete),
        "ignored": sum(suite["ignored"] for suite in complete),
        "unexecuted_suites": unexecuted,
        "ignored_tests": ignored,
    }
    return {"suites": suites, "totals": totals, "failed_tests": failed_tests}


def _unit_name(path: str) -> str:
    if path.endswith("src/lib.rs"):
        return "lib_unit"
    if path.endswith("src/main.rs"):
        return "bin_main"
    return "unit_" + Path(path).stem


def verify_artifact(repo: Path, target_dir: Path) -> dict:
    candidates = [target_dir / "release" / "mink.exe", target_dir / "release" / "mink"]
    binary = next((path for path in candidates if path.is_file()), None)
    if binary is None:
        return {"found": False, "expected": [str(path.relative_to(repo)) if path.is_relative_to(repo) else str(path) for path in candidates]}
    stat = binary.stat()
    version = capture([str(binary), "--version"], timeout=30.0)
    return {
        "found": True,
        "path": str(binary),
        "relative_path": str(binary.relative_to(repo)) if binary.is_relative_to(repo) else str(binary),
        "size_bytes": stat.st_size,
        "sha256": sha256_file(binary),
        "modified_at": dt.datetime.fromtimestamp(stat.st_mtime, dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "version": version["output"].splitlines()[0] if version["exit_code"] == 0 and version["output"] else None,
        "version_exit_code": version["exit_code"],
    }


def decide(stages: list[dict], artifact: dict, tests: dict | None, tracked_changes: list[str], blocked: str | None, allow_dirty: bool) -> tuple[str, list[str], list[str]]:
    limitations: list[str] = []
    partial: list[str] = []
    failures: list[str] = []
    if blocked:
        limitations.append(blocked)
        return "BLOCKED", failures, limitations

    for stage in stages:
        if stage["status"] in ("fail", "timeout"):
            failures.append(f"{stage['name']} exited {stage['exit_code']}")

    if not artifact.get("found"):
        partial.append("release artifact was not found after a successful build stage")
        limitations.append(partial[-1])
    elif not artifact.get("version"):
        limitations.append("artifact version could not be read; size and SHA-256 were recorded")

    if tests is None:
        if not failures:
            partial.append("release regression did not execute")
            limitations.append(partial[-1])
    else:
        totals = tests["totals"]
        for failed_test in tests["failed_tests"]:
            failures.append(f"{failed_test['suite']}::{failed_test['test']} failed")
        if totals["total_suites"] == 0:
            partial.append("no cargo test result blocks were parsed")
            limitations.append(partial[-1])
        if totals["unexecuted_suites"]:
            partial.append("suites without a result block: " + ", ".join(totals["unexecuted_suites"]))
            limitations.append(partial[-1])
        if totals["failed"] and not tests["failed_tests"]:
            failures.append(f"{totals['failed']} test(s) failed but no individual test name was parsed")

    if tracked_changes and not allow_dirty:
        partial.append("tracked files differ from the committed tree; acceptance evidence does not describe a clean checkout")
        limitations.append(partial[-1])

    if failures:
        return "FAIL", failures, limitations
    if partial:
        return "PARTIAL", failures, limitations
    return "PASS", failures, limitations


def exit_code_for(verdict: str, stages: list[dict]) -> int:
    if verdict == "PASS":
        return EXIT_PASS
    if verdict == "BLOCKED":
        return EXIT_BLOCKED
    if verdict == "PARTIAL":
        return EXIT_PARTIAL
    for stage in stages:
        if stage["status"] in ("fail", "timeout"):
            if stage["exit_code"] not in (0, EXIT_BLOCKED, EXIT_PARTIAL):
                return stage["exit_code"]
            return EXIT_FAIL
    return EXIT_FAIL


def render_markdown(report: dict) -> str:
    lines: list[str] = []
    verdict = report["verdict"]
    lines.append(f"# MINK release acceptance -- session {report['session']}")
    lines.append("")
    lines.append(f"**Verdict: {verdict}**  (runner exit code {report['exit_code']})")
    lines.append("")
    lines.append(f"Generated {report['generated_at']} from `{report['repository']['head']}` on branch `{report['repository']['branch']}`.")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    lines.append("| Item | Value |")
    lines.append("| --- | --- |")
    lines.append(f"| Run id | `{report['run_id']}` |")
    lines.append(f"| Commit | `{report['repository']['head']}` |")
    lines.append(f"| Worktree | {'dirty' if report['repository']['dirty'] else 'clean'} |")
    lines.append(f"| Build | {_stage_cell(report, 'build')} |")
    lines.append(f"| Test compilation | {_stage_cell(report, 'test-compile')} |")
    lines.append(f"| Regression | {_stage_cell(report, 'test')} |")
    artifact = report["artifact"]
    if artifact.get("found"):
        lines.append(f"| Artifact | {artifact['size_bytes']:,} bytes, SHA-256 `{artifact['sha256']}` |")
    else:
        lines.append("| Artifact | missing |")
    tests = report.get("tests")
    if tests:
        totals = tests["totals"]
        lines.append(f"| Regression result | {totals['total_suites']} suites, {totals['passed']} passed, {totals['failed']} failed, {totals['ignored']} ignored |")
    lines.append(f"| Limitations | {len(report['limitations'])} |")
    lines.append("")
    lines.append("## Toolchain and configuration")
    lines.append("")
    lines.append("| Item | Value |")
    lines.append("| --- | --- |")
    lines.append(f"| rustc | {report['toolchain']['rustc_version'] or 'unavailable'} |")
    lines.append(f"| cargo | {report['toolchain']['cargo_version'] or 'unavailable'} |")
    lines.append(f"| Host | {report['toolchain']['host'] or 'unavailable'} |")
    lines.append(f"| LLVM | {report['toolchain'].get('llvm') or 'unavailable'} |")
    lines.append(f"| Cargo config | {report['configuration']['cargo_config_path'] or 'none'} |")
    lines.append(f"| Release profile | {report['configuration']['release_profile'].splitlines()[0]} |")
    lines.append(f"| Dependencies | {report['configuration']['dependencies']} |")
    lines.append(f"| Cargo target dir | `{report['configuration']['target_dir']}` |")
    lines.append("")
    lines.append("## Stages")
    lines.append("")
    lines.append("| Stage | Command | Exit | Duration | Log |")
    lines.append("| --- | --- | ---: | ---: | --- |")
    for stage in report["stages"]:
        if stage["command_line"]:
            command = stage["command_line"].replace("|", "\\|")
            log_cell = f"[`{stage['log']}`]({stage['log']})"
        else:
            command = "not executed"
            log_cell = "-"
        exit_cell = "-" if stage["exit_code"] is None else str(stage["exit_code"])
        lines.append(f"| {stage['name']} | `{command}` | {exit_cell} | {stage['duration_seconds']:.3f} s | {log_cell} |")
    lines.append("")
    lines.append("## Artifact")
    lines.append("")
    if artifact.get("found"):
        lines.append(f"- Path: `{artifact['relative_path']}`")
        lines.append(f"- Size: {artifact['size_bytes']:,} bytes")
        lines.append(f"- SHA-256: `{artifact['sha256']}`")
        lines.append(f"- Version: `{artifact['version'] or 'unavailable'}`")
        lines.append(f"- Modified: {artifact['modified_at']}")
    else:
        lines.append(f"- Not found. Expected one of: {', '.join('`' + item + '`' for item in artifact.get('expected', []))}")
    lines.append("")
    if tests:
        totals = tests["totals"]
        lines.append("## Regression")
        lines.append("")
        lines.append(f"{totals['total_suites']} suites, {totals['passed']} passed, {totals['failed']} failed, {totals['ignored']} ignored.")
        lines.append("")
        if totals["ignored_tests"]:
            lines.append("### Ignored tests")
            lines.append("")
            lines.append("| Suite | Test |")
            lines.append("| --- | --- |")
            for item in totals["ignored_tests"]:
                lines.append(f"| {item['suite']} | `{item['test']}` |")
            lines.append("")
        if report["failures"]:
            lines.append("### Failures")
            lines.append("")
            for item in report["failures"]:
                lines.append(f"- {item}")
            lines.append("")
        lines.append("### Suite results")
        lines.append("")
        lines.append("| Suite | Passed | Failed | Ignored | Status |")
        lines.append("| --- | ---: | ---: | ---: | --- |")
        for suite in tests["suites"]:
            if suite["passed"] is None:
                lines.append(f"| `{suite['name']}` | - | - | - | {suite['status']} |")
            else:
                lines.append(f"| `{suite['name']}` | {suite['passed']} | {suite['failed']} | {suite['ignored']} | {suite['status']} |")
        lines.append("")
    if report["failures"]:
        lines.append("## Failures")
        lines.append("")
        for item in report["failures"]:
            lines.append(f"- {item}")
        lines.append("")
    if report["limitations"]:
        lines.append("## Limitations")
        lines.append("")
        for item in report["limitations"]:
            lines.append(f"- {item}")
        lines.append("")
    lines.append("## Reproduction")
    lines.append("")
    lines.append("```")
    for command in report["reproduction"]:
        lines.append(command)
    lines.append("```")
    lines.append("")
    return "\n".join(lines)


def _stage_cell(report: dict, name: str) -> str:
    for stage in report["stages"]:
        if stage["name"] == name:
            if stage["exit_code"] is None:
                return "skipped"
            return f"{stage['status']} (exit {stage['exit_code']}, {stage['duration_seconds']:.3f} s)"
    return "skipped"


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run MINK release acceptance and write evidence reports.")
    parser.add_argument("--session", default="unlabelled", help="session identifier used in the report path and title")
    parser.add_argument("--label", default=None, help="optional label appended to the run id")
    parser.add_argument("--repo", default=str(Path(__file__).resolve().parent.parent), help="repository root")
    parser.add_argument("--out", default=None, help="report root (default: <repo>/target/acceptance)")
    parser.add_argument("--timeout", type=float, default=DEFAULT_TIMEOUT, help="per-stage timeout in seconds")
    parser.add_argument("--allow-dirty", action="store_true", help="do not mark tracked worktree changes as PARTIAL")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    repo = Path(args.repo).resolve()
    out_root = Path(args.out).resolve() if args.out else repo / "target" / "acceptance"
    out_root.mkdir(parents=True, exist_ok=True)

    cargo = resolve_command(os.environ.get("MINK_ACCEPTANCE_CARGO", "cargo"))
    rustc = resolve_command(os.environ.get("MINK_ACCEPTANCE_RUSTC", "rustc"))
    git = resolve_command(os.environ.get("MINK_ACCEPTANCE_GIT", "git"))

    blocked = None
    missing = []
    for name, command in (("cargo", cargo), ("rustc", rustc), ("git", git)):
        if not executable_available(command):
            missing.append(name)
    if missing:
        blocked = "required tool(s) unavailable on PATH: " + ", ".join(missing)
    elif not (repo / "Cargo.toml").is_file():
        blocked = f"no Cargo.toml under repository root {repo}"

    environment = gather_environment(repo, cargo, rustc, git) if not missing else {
        "cargo_version": None, "rustc_version": None, "host": None, "llvm": None,
        "branch": None, "head": None, "status_entries": [], "status_exit_code": None,
    }
    if environment["status_exit_code"] not in (0, None):
        blocked = blocked or "git status failed; the repository state cannot be verified"

    head = environment["head"] or "unknown"
    session = re.sub(r"[^A-Za-z0-9._-]+", "-", args.session.strip()).strip("-") or "unlabelled"
    session_dir = out_root / session
    base_id = f"{run_id_time()}-{head[:7]}"
    if args.label:
        safe_label = re.sub(r"[^A-Za-z0-9._-]+", "-", args.label).strip("-")
        if safe_label:
            base_id += f"-{safe_label}"
    run_dir = unique_run_dir(session_dir, base_id)
    work_root = repo / "target" / "acceptance-work"
    work_dir = unique_run_dir(work_root, run_dir.name)
    target_dir = work_dir / "cargo-target"

    stages: list[dict] = []
    artifact: dict = {"found": False}
    tests: dict | None = None
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(target_dir)
    reproduction = [
        f"CARGO_TARGET_DIR={shlex.quote(str(target_dir))} " + command_string(cargo + ["build", "--release"]),
        f"CARGO_TARGET_DIR={shlex.quote(str(target_dir))} " + command_string(cargo + ["test", "--release", "--no-run"]),
        f"CARGO_TARGET_DIR={shlex.quote(str(target_dir))} " + command_string(cargo + ["test", "--release", "--no-fail-fast"]),
        f"python tools/release_acceptance.py --session {session}" + (f" --label {args.label}" if args.label else ""),
    ]

    def skipped(name: str) -> dict:
        return {"name": name, "command": [], "command_line": "", "exit_code": None, "status": "skipped", "started_at": None, "finished_at": None, "duration_seconds": 0.0, "log": None, "output": ""}

    if not blocked:
        stages.append(run_stage("build", cargo + ["build", "--release"], run_dir / "logs" / "build.log", env, args.timeout))
        if stages[-1]["exit_code"] == 0:
            artifact = verify_artifact(repo, target_dir)
            stages.append(run_stage("test-compile", cargo + ["test", "--release", "--no-run"], run_dir / "logs" / "test-compile.log", env, args.timeout))
            if stages[-1]["exit_code"] == 0:
                stages.append(run_stage("test", cargo + ["test", "--release", "--no-fail-fast"], run_dir / "logs" / "tests.log", env, args.timeout))
                tests = parse_cargo_test(stages[-1]["output"])
            else:
                stages.append(skipped("test"))
        else:
            stages.append(skipped("test-compile"))
            stages.append(skipped("test"))

    tracked_changes = [line for line in environment["status_entries"] if not line.startswith("??")]
    untracked = [line[3:] for line in environment["status_entries"] if line.startswith("??")]
    verdict, failures, limitations = decide(stages, artifact, tests, tracked_changes, blocked, args.allow_dirty)
    if untracked:
        limitations.append(f"{len(untracked)} untracked path(s) present; they are not part of the acceptance evidence")
    code = exit_code_for(verdict, stages)

    report = {
        "schema_version": SCHEMA_VERSION,
        "session": session,
        "label": args.label,
        "run_id": run_dir.name,
        "generated_at": utc_now(),
        "verdict": verdict,
        "exit_code": code,
        "repository": {
            "root": str(repo),
            "branch": environment["branch"],
            "head": environment["head"],
            "dirty": bool(environment["status_entries"]),
            "tracked_changes": tracked_changes,
            "untracked": untracked,
        },
        "toolchain": {
            "cargo_version": environment["cargo_version"],
            "rustc_version": environment["rustc_version"],
            "host": environment["host"],
            "llvm": environment["llvm"],
        },
        "configuration": {
            **cargo_configuration(repo),
            "target_dir": str(target_dir),
            "work_dir": str(work_dir),
            "timeout_seconds": args.timeout,
        },
        "stages": [
            {
                "name": stage["name"],
                "command": stage["command"],
                "command_line": stage["command_line"],
                "exit_code": stage["exit_code"],
                "status": stage["status"],
                "started_at": stage["started_at"],
                "finished_at": stage["finished_at"],
                "duration_seconds": stage["duration_seconds"],
                "log": stage["log"],
            }
            for stage in stages
        ],
        "artifact": {key: value for key, value in artifact.items() if key != "found"} | {"found": artifact.get("found", False)},
        "tests": {"totals": tests["totals"], "suites": tests["suites"], "failed_tests": tests["failed_tests"]} if tests else None,
        "failures": failures,
        "limitations": limitations,
        "reproduction": reproduction,
    }

    (run_dir / "logs").mkdir(exist_ok=True)
    (run_dir / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8", newline="\n")
    (run_dir / "report.md").write_text(render_markdown(report), encoding="utf-8", newline="\n")
    (run_dir / "environment.txt").write_text(
        "\n".join(
            [
                f"generated_at={report['generated_at']}",
                f"run_id={report['run_id']}",
                f"verdict={verdict}",
                f"exit_code={code}",
                f"branch={environment['branch']}",
                f"head={environment['head']}",
                f"cargo={environment['cargo_version']}",
                f"rustc={environment['rustc_version']}",
                f"host={environment['host']}",
                f"target_dir={target_dir}",
            ]
        )
        + "\n",
        encoding="utf-8",
        newline="\n",
    )

    print(f"verdict: {verdict}")
    print(f"exit code: {code}")
    print(f"report: {run_dir / 'report.md'}")
    print(f"json:   {run_dir / 'report.json'}")
    if tests:
        totals = tests["totals"]
        print(f"regression: {totals['total_suites']} suites, {totals['passed']} passed, {totals['failed']} failed, {totals['ignored']} ignored")
    for failure in failures:
        print(f"failure: {failure}")
    for limitation in limitations:
        print(f"limitation: {limitation}")
    return code


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
