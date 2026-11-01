#!/usr/bin/env python3
"""Execute one benchmark case against one implementation and emit a record.

    python benchmarks/m4/harness/run.py --workload w1_json --language python \
        --case validate_object --json
    python benchmarks/m4/harness/run.py --workload w2_http --language go \
        --case chunked --json

The executor starts the W2 fixture server in-process when the workload needs
one, captures stdout, stderr and the exit code, and compares them with the
declarative golden case. It performs no timing and makes no performance claim.
"""
import argparse
import json
import subprocess
import sys

import common


def execute(language, workload, case, tokens=None):
    fixture_dir = common.fixtures_dir(workload)
    argv = common.resolve_argv(case["argv"], fixture_dir, tokens)
    command = common.run_command(language, workload)

    completed = subprocess.run(
        command + argv,
        cwd=common.REPO_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return {
        "workload": workload,
        "language": language,
        "case": case["id"],
        "scored": bool(case["scored"]),
        "argv": argv,
        "command": command[0],
        "exit_code": completed.returncode,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
    }


def compare(record, case, fixture_dir, tokens=None):
    """Return a list of human-readable differences; empty means PASS."""
    problems = []
    if record["exit_code"] != case["exit"]:
        problems.append("exit %d != %d" % (record["exit_code"], case["exit"]))
    expected_stdout = common.resolve_text(
        case["stdout"], fixture_dir, tokens
    ).encode("utf-8")
    expected_stderr = common.resolve_text(
        case["stderr"], fixture_dir, tokens
    ).encode("utf-8")
    if record["stdout"] != expected_stdout:
        problems.append("stdout %r != %r" % (record["stdout"], expected_stdout))
    if record["stderr"] != expected_stderr:
        problems.append("stderr %r != %r" % (record["stderr"], expected_stderr))
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
