#!/usr/bin/env python3
"""Execute one benchmark case against one implementation and emit a record.

    python benchmarks/m4/harness/run.py --workload w1_json --language python \
        --case validate_object --json
    python benchmarks/m4/harness/run.py --workload w5_manifest --language go \
        --case baseline_100 --json

Workloads that need a fixture server (W2) or a prepared work directory (W5)
get one for the duration of the call. Captures stdout, stderr, exit code and
declared artifacts, and compares them with the declarative golden case. No
timing is measured and no performance claim is made.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys

import common


def prepare_workdir(workload, case, language):
    """Create the W5 work directory: fixture tree, out/ and config.json."""
    work = common.work_dir(workload, case["id"], language)
    if os.path.isdir(work):
        shutil.rmtree(work)
    os.makedirs(os.path.join(work, "out"), exist_ok=True)

    prepare = case.get("prepare") or {}
    tree = prepare.get("tree")
    if tree:
        shutil.copytree(
            os.path.join(common.fixtures_dir(workload), tree),
            os.path.join(work, "tree"),
        )
    if prepare.get("raw_config") is not None:
        with open(os.path.join(work, "config.json"), "wb") as handle:
            handle.write(prepare["raw_config"].encode("utf-8"))
    elif prepare.get("config") is not None:
        with open(os.path.join(work, "config.json"), "w",
                  encoding="utf-8", newline="\n") as handle:
            json.dump(prepare["config"], handle, separators=(",", ":"))
    return work


def execute(language, workload, case, tokens=None):
    fixture_dir = common.fixtures_dir(workload)
    argv = common.resolve_argv(case["argv"], fixture_dir, tokens)
    command = common.run_command(language, workload)
    cwd = common.REPO_ROOT
    if workload == "w5_manifest":
        cwd = prepare_workdir(workload, case, language)

    try:
        completed = subprocess.run(
            command + argv,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except FileNotFoundError:
        # A blocked dependency must be reported as "not built", never as a
        # pass or as a crash of the harness.
        return {
            "workload": workload,
            "language": language,
            "case": case["id"],
            "scored": bool(case["scored"]),
            "argv": argv,
            "command": command[0],
            "cwd": cwd,
            "exit_code": None,
            "stdout": b"",
            "stderr": b"",
            "missing": True,
        }
    return {
        "workload": workload,
        "language": language,
        "case": case["id"],
        "scored": bool(case["scored"]),
        "argv": argv,
        "command": command[0],
        "cwd": cwd,
        "exit_code": completed.returncode,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
    }


def _expected_bytes(record, case, key, fixture_dir, tokens):
    filename = case.get(key + "_file")
    if filename:
        path = os.path.join(common.expected_directory(record["workload"]), filename)
        with open(path, "rb") as handle:
            return handle.read()
    return common.resolve_text(case[key], fixture_dir, tokens).encode("utf-8")


def _artifact_problems(record, case):
    problems = []
    expected_root = common.expected_directory(record["workload"])
    for relative in case.get("artifacts", []):
        parts = relative.split("/")
        produced = os.path.join(record.get("cwd", common.REPO_ROOT), *parts)
        expected = os.path.join(expected_root, case["id"], *parts)
        if not os.path.exists(expected):
            continue
        if not os.path.exists(produced):
            problems.append("missing artifact: %s" % relative)
            continue
        with open(produced, "rb") as handle:
            actual = handle.read()
        with open(expected, "rb") as handle:
            wanted = handle.read()
        if actual != wanted:
            problems.append("artifact differs: %s" % relative)
    return problems


def compare(record, case, fixture_dir, tokens=None):
    """Return a list of human-readable differences; empty means PASS."""
    problems = []
    if record.get("missing"):
        return ["application not built: %s" % record["command"]]
    if record["exit_code"] != case["exit"]:
        problems.append("exit %d != %d" % (record["exit_code"], case["exit"]))
    expected_stdout = _expected_bytes(record, case, "stdout", fixture_dir, tokens)
    expected_stderr = _expected_bytes(record, case, "stderr", fixture_dir, tokens)
    if record["stdout"] != expected_stdout:
        problems.append("stdout %r != %r" % (record["stdout"], expected_stdout))
    if record["stderr"] != expected_stderr:
        problems.append("stderr %r != %r" % (record["stderr"], expected_stderr))
    problems.extend(_artifact_problems(record, case))
    return problems


def main():
    parser = argparse.ArgumentParser(description="Run one M4 case")
    parser.add_argument("--workload", default="w1_json", choices=sorted(common.WORKLOADS))
    parser.add_argument("--language", required=True, choices=common.LANGUAGES)
    parser.add_argument("--case", required=True)
    parser.add_argument("--json", action="store_true", help="emit a JSON record")
    args = parser.parse_args()

    cases = common.load_cases(args.workload)
    by_id = {case["id"]: case for case in cases}
    if args.case not in by_id:
        raise SystemExit("unknown case: %s" % args.case)
    case = by_id[args.case]

    tokens = {}
    fixture_server = None
    if args.workload == "w2_http":
        import server as fixture_server_module
        fixture_server = fixture_server_module.FixtureServer().start()
        tokens = fixture_server.tokens()

    try:
        record = execute(args.language, args.workload, case, tokens)
    finally:
        if fixture_server is not None:
            fixture_server.stop()

    problems = compare(record, case, common.fixtures_dir(args.workload), tokens)

    if args.json:
        output = dict(record)
        output["stdout"] = record["stdout"].decode("utf-8", "backslashreplace")
        output["stderr"] = record["stderr"].decode("utf-8", "backslashreplace")
        output["status"] = "FAIL" if problems else "PASS"
        output["problems"] = problems
        print(json.dumps(output))
    else:
        print("%s %s / %s" % ("FAIL" if problems else "PASS", args.language, args.case))
        for problem in problems:
            print("   " + problem)

    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
