#!/usr/bin/env python3
"""Run a workload's full correctness suite for every implementation.

    python benchmarks/m4/harness/validate.py --workload w1_json
    python benchmarks/m4/harness/validate.py --workload w2_http

Compares stdout, stderr and exit code byte-for-byte against the declarative
golden cases in `benchmarks/m4/expected/<workload>/cases.json`. W2 starts the
harness-owned fixture server for the duration of the run and stops it before
exiting. Machine-readable records go to `benchmarks/m4/results/raw/`.

Exit status is 0 only when every scored and supporting case passes in every
requested language.
"""
import argparse
import hashlib
import json
import os
import sys

import common
import run


def main():
    parser = argparse.ArgumentParser(description="M4 correctness gate")
    parser.add_argument("--workload", default="w1_json", choices=sorted(common.WORKLOADS))
    parser.add_argument("--language", default="all",
                        choices=("all",) + common.LANGUAGES)
    parser.add_argument("--quiet", action="store_true", help="summary only")
    parser.add_argument("--include-supporting", dest="supporting",
                        action="store_true", default=True)
    parser.add_argument("--scored-only", dest="supporting", action="store_false")
    args = parser.parse_args()

    languages = common.LANGUAGES if args.language == "all" else (args.language,)
    cases = common.load_cases(args.workload)
    if not args.supporting:
        cases = [case for case in cases if case["scored"]]
    fixture_dir = common.fixtures_dir(args.workload)

    fixture_server = None
    tokens = {}
    if args.workload == "w2_http":
        import server as fixture_server_module
        fixture_server = fixture_server_module.FixtureServer().start()
        tokens = fixture_server.tokens()

    try:
        records, passed, failed = _run_all(
            args, languages, cases, fixture_dir, tokens
        )
    finally:
        if fixture_server is not None:
            fixture_server.stop()

    _write_records(args.workload, records)
    _print_summary(args.workload, languages, cases, records, passed, failed)

    return 1 if sum(failed.values()) else 0


def _run_all(args, languages, cases, fixture_dir, tokens):
    records = []
    passed = {language: 0 for language in languages}
    failed = {language: 0 for language in languages}

    for language in languages:
        for case in cases:
            record = run.execute(language, args.workload, case, tokens)
            problems = run.compare(record, case, fixture_dir, tokens)
            status = "FAIL" if problems else "PASS"
            if problems:
                failed[language] += 1
                if not args.quiet:
                    print("%s %s / %s" % (status, language, case["id"]))
                    for problem in problems:
                        print("   " + problem)
            else:
                passed[language] += 1
                if not args.quiet:
                    print("%s %s / %s" % (status, language, case["id"]))
            records.append({
                "workload": args.workload,
                "language": language,
                "case": case["id"],
                "scored": bool(case["scored"]),
                "status": status,
                "exit_code": record["exit_code"],
                "expected_exit": case["exit"],
                "stdout_sha256": _sha(record["stdout"]),
                "stderr_sha256": _sha(record["stderr"]),
                "problems": problems,
            })
    return records, passed, failed


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _write_records(workload, records):
    directory = os.path.join(common.RAW_ROOT, workload)
    os.makedirs(directory, exist_ok=True)
    path = os.path.join(directory, "correctness-%s.jsonl" % workload)
    with open(path, "w", encoding="utf-8") as handle:
        for record in records:
            handle.write(json.dumps(record) + "\n")
    print("records: %s" % path)


def _print_summary(workload, languages, cases, records, passed, failed):
    print("\n%d cases x %d languages" % (len(cases), len(languages)))
    for language in languages:
        total = len(cases)
        print("  %-7s %d/%d passed, %d failed"
              % (language, passed[language], total, failed[language]))

    scored = [case for case in cases if case["scored"]]
    print("\nscored cases: %d" % len(scored))
    for case in scored:
        row = []
        for language in languages:
            match = [r for r in records
                     if r["language"] == language and r["case"] == case["id"]]
            row.append("PASS" if match and match[0]["status"] == "PASS" else "FAIL")
        print("  %-24s %s" % (case["id"], " ".join(
            "%s=%s" % (language, value) for language, value in zip(languages, row))))

    total_failures = sum(failed.values())
    print("\nGATE: %s" % ("PASS" if total_failures == 0 else "FAIL"))


if __name__ == "__main__":
    sys.exit(main())
