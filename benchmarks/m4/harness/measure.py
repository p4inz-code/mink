#!/usr/bin/env python3
"""M4 measurement driver: implements the locked protocol, computes no verdict.

    python benchmarks/m4/harness/measure.py --workload w1_json --plan
    python benchmarks/m4/harness/measure.py --workload w1_json --runtime
    python benchmarks/m4/harness/measure.py --workload w1_json --startup
    python benchmarks/m4/harness/measure.py --workload w1_json --clean-build

Protocol (locked in `M4_COMPARISON_METHODOLOGY.md`):

* runtime: 2 warm-up runs, then 15 measured runs, per language and scored case;
* startup: 30 measured runs of the artifact with no arguments (the tool prints
  its usage and exits), per language;
* clean build: 5 builds into a benchmark-owned, emptied build directory;
* language order is shuffled with a recorded seed;
* every measured run must still match the golden exit code and stdout hash;
* raw samples are appended to `benchmarks/m4/results/raw/<workload>/*.jsonl`.

The driver never deletes a sample, never averages percentages and never emits
a ranking or a composite score. Correctness is a hard precondition: it refuses
to time a workload unless the recorded correctness gate is fully green for the
requested languages and cases.

`--plan` prints what would run and exits without launching anything.
"""
import argparse
import ctypes
import ctypes.wintypes
import hashlib
import json
import os
import random
import shutil
import subprocess
import sys
import time

import common
import run
import stats

WARMUPS = 2
MEASURED_RUNS = 15
STARTUP_RUNS = 30
CLEAN_BUILDS = 5
DEFAULT_SEED = 20261101


class _ProcessMemoryCounters(ctypes.Structure):
    _fields_ = [
        ("cb", ctypes.wintypes.DWORD),
        ("PageFaultCount", ctypes.wintypes.DWORD),
        ("PeakWorkingSetSize", ctypes.c_size_t),
        ("WorkingSetSize", ctypes.c_size_t),
        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
        ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
        ("PagefileUsage", ctypes.c_size_t),
        ("PeakPagefileUsage", ctypes.c_size_t),
    ]


def _win32_api():
    """Child CPU time and peak working set, on Windows only.

    Both are read from the process handle Python already holds, so no extra
    tool and no sampling thread is needed. Returns (cpu_ms, peak_rss_bytes) or
    (None, None) when the platform or the handle is unavailable; a missing
    measurement is recorded as null, never guessed.
    """
    if sys.platform != "win32":
        return None
    try:
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        psapi = ctypes.WinDLL("psapi", use_last_error=True)
    except OSError:
        return None
    return kernel32, psapi


def _sample_process(handle):
    api = _win32_api()
    if api is None or handle is None:
        return None, None
    kernel32, psapi = api
    cpu_ms = None
    peak_rss = None
    try:
        creation = ctypes.wintypes.FILETIME()
        exit_time = ctypes.wintypes.FILETIME()
        kernel = ctypes.wintypes.FILETIME()
        user = ctypes.wintypes.FILETIME()
        if kernel32.GetProcessTimes(handle, ctypes.byref(creation),
                                    ctypes.byref(exit_time),
                                    ctypes.byref(kernel), ctypes.byref(user)):
            ticks = ((kernel.dwHighDateTime << 32) | kernel.dwLowDateTime) \
                + ((user.dwHighDateTime << 32) | user.dwLowDateTime)
            cpu_ms = ticks / 10000.0
    except (OSError, ValueError):
        cpu_ms = None
    try:
        counters = _ProcessMemoryCounters()
        counters.cb = ctypes.sizeof(_ProcessMemoryCounters)
        if psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
            peak_rss = int(counters.PeakWorkingSetSize)
    except (OSError, ValueError):
        peak_rss = None
    return cpu_ms, peak_rss


def _time_once(command, cwd):
    started = time.perf_counter_ns()
    process = subprocess.Popen(command, cwd=cwd, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE)
    stdout, stderr = process.communicate()
    elapsed_ms = (time.perf_counter_ns() - started) / 1e6
    cpu_ms, peak_rss = _sample_process(getattr(process, "_handle", None))
    return {
        "duration_ms": elapsed_ms,
        "cpu_ms": cpu_ms,
        "peak_rss_bytes": peak_rss,
        "exit_code": process.returncode,
        "stdout": stdout,
        "stderr": stderr,
    }


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _correctness_records(workload):
    path = os.path.join(common.RAW_ROOT, workload, "correctness-%s.jsonl" % workload)
    if not os.path.exists(path):
        return None, path
    records = []
    with open(path, "r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                records.append(json.loads(line))
    return records, path


def check_gate(workload, languages, cases):
    """Refuse to measure unless every requested case passed in every language."""
    records, path = _correctness_records(workload)
    if records is None:
        return ["no correctness record at %s; run validate.py first" % path]
    problems = []
    for language in languages:
        for case in cases:
            match = [record for record in records
                     if record["language"] == language and record["case"] == case["id"]]
            if not match:
                problems.append("no correctness record for %s / %s" % (language, case["id"]))
            elif match[0]["status"] != "PASS":
                problems.append("%s / %s is FAIL in the correctness gate"
                                % (language, case["id"]))
    return problems


def _case_argv(case):
    return case["argv"]


def _write(directory, filename, record):
    os.makedirs(directory, exist_ok=True)
    with open(os.path.join(directory, filename), "a", encoding="utf-8") as handle:
        handle.write(json.dumps(record) + "\n")


def measure_runtime(workload, languages, cases, seed):
    directory = os.path.join(common.RAW_ROOT, workload)
    filename = "runtime-%s.jsonl" % workload
    path = os.path.join(directory, filename)
    if os.path.exists(path):
        os.remove(path)

    rng = random.Random(seed)
    order = list(languages)
    rng.shuffle(order)
    print("language order (seed %d): %s" % (seed, ", ".join(order)))
    total = 0
    for case in cases:
        order = list(languages)
        rng.shuffle(order)
        print("case %s: %s" % (case["id"], ", ".join(order)))
        for position, language in enumerate(order):
            tokens = {}
            server = None
            if workload == "w2_http":
                import server as fixture_server_module
                server = fixture_server_module.FixtureServer().start()
                tokens = server.tokens()
            try:
                argv = common.resolve_argv(_case_argv(case), common.fixtures_dir(workload), tokens)
                command = common.run_command(language, workload) + argv
                cwd = common.REPO_ROOT
                if workload == "w5_manifest":
                    cwd = run.prepare_workdir(workload, case, language)
                for index in range(WARMUPS + MEASURED_RUNS):
                    warmup = index < WARMUPS
                    sample = _time_once(command, cwd)
                    problems = run.compare(
                        {"workload": workload, "language": language,
                         "case": case["id"], "argv": argv,
                         "exit_code": sample["exit_code"],
                         "stdout": sample["stdout"], "stderr": sample["stderr"],
                         "cwd": cwd},
                        case, common.fixtures_dir(workload), tokens)
                    record = {
                        "workload": workload,
                        "case": case["id"],
                        "language": language,
                        "phase": "runtime",
                        "run_index": index,
                        "warmup": warmup,
                        "order_position": position,
                        "seed": seed,
                        "duration_ms": sample["duration_ms"],
                        "cpu_ms": sample["cpu_ms"],
                        "peak_rss_bytes": sample["peak_rss_bytes"],
                        "exit_code": sample["exit_code"],
                        "stdout_sha256": _sha(sample["stdout"]),
                        "correctness": "PASS" if not problems else "FAIL",
                    }
                    _write(directory, filename, record)
                    total += 1
                    if problems:
                        raise SystemExit(
                            "measured run broke correctness: %s / %s: %s"
                            % (language, case["id"], "; ".join(problems)))
            finally:
                if server is not None:
                    server.stop()
    print("runtime samples: %d -> %s" % (total, path))
    return 0


def measure_startup(workload, languages, seed):
    directory = os.path.join(common.RAW_ROOT, workload)
    filename = "startup-%s.jsonl" % workload
    path = os.path.join(directory, filename)
    if os.path.exists(path):
        os.remove(path)

    rng = random.Random(seed)
    order = list(languages)
    rng.shuffle(order)
    print("startup order (seed %d): %s" % (seed, ", ".join(order)))
    for language in order:
        command = common.run_command(language, workload)
        for index in range(STARTUP_RUNS):
            sample = _time_once(command, common.REPO_ROOT)
            _write(directory, filename, {
                "workload": workload,
                "case": "startup_no_args",
                "language": language,
                "phase": "startup",
                "run_index": index,
                "warmup": False,
                "seed": seed,
                "duration_ms": sample["duration_ms"],
                "cpu_ms": sample["cpu_ms"],
                "peak_rss_bytes": sample["peak_rss_bytes"],
                "exit_code": sample["exit_code"],
                "stdout_sha256": _sha(sample["stdout"]),
                "correctness": "not-applicable",
            })
    print("startup samples: %d -> %s" % (len(order) * STARTUP_RUNS, path))
    return 0


def measure_clean_build(workload, languages, seed):
    import build as build_module

    directory = os.path.join(common.RAW_ROOT, workload)
    filename = "clean-build-%s.jsonl" % workload
    path = os.path.join(directory, filename)
    if os.path.exists(path):
        os.remove(path)

    rng = random.Random(seed)
    order = list(languages)
    rng.shuffle(order)
    print("clean-build order (seed %d): %s" % (seed, ", ".join(order)))
    for position, language in enumerate(order):
        if language == "python":
            print("%s: no build step, nothing to time" % language)
            continue
        target = common.build_dir(language, workload)
        for index in range(CLEAN_BUILDS):
            started = time.perf_counter_ns()
            # Only a benchmark-owned build directory is removed.
            shutil.rmtree(target, ignore_errors=True)
            completed = subprocess.run(
                [sys.executable, os.path.join(common.HARNESS_DIR, "build.py"),
                 "--workload", workload, "--language", language],
                cwd=common.REPO_ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            elapsed_ms = (time.perf_counter_ns() - started) / 1e6
            _write(directory, filename, {
                "workload": workload,
                "case": "clean_build",
                "language": language,
                "phase": "clean-build",
                "run_index": index,
                "warmup": False,
                "order_position": position,
                "seed": seed,
                "duration_ms": elapsed_ms,
                "exit_code": completed.returncode,
                "correctness": "PASS" if completed.returncode == 0 else "FAIL",
                "log_tail": completed.stdout.decode("utf-8", "replace")[-400:],
            })
            if completed.returncode != 0:
                raise SystemExit("clean build failed for %s" % language)
    print("clean-build records -> %s" % path)
    return 0


def main():
    parser = argparse.ArgumentParser(description="M4 measurement driver")
    parser.add_argument("--workload", default="w1_json", choices=sorted(common.WORKLOADS))
    parser.add_argument("--language", default="all",
                        choices=("all",) + common.LANGUAGES)
    parser.add_argument("--case", action="append", default=None,
                        help="limit to a scored case id (repeatable)")
    parser.add_argument("--plan", action="store_true",
                        help="print the protocol and exit without measuring")
    parser.add_argument("--runtime", action="store_true")
    parser.add_argument("--startup", action="store_true")
    parser.add_argument("--clean-build", dest="clean_build", action="store_true")
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED)
    args = parser.parse_args()

    languages = common.LANGUAGES if args.language == "all" else (args.language,)
    scored = [case for case in common.load_cases(args.workload) if case["scored"]]
    if args.case:
        wanted = set(args.case)
        scored = [case for case in scored if case["id"] in wanted]
        missing = wanted - {case["id"] for case in scored}
        if missing:
            raise SystemExit("unknown or unscored case(s): %s" % ", ".join(sorted(missing)))

    if args.plan:
        order = list(languages)
        random.Random(args.seed).shuffle(order)
        print("workload: %s (%s)" % (args.workload, common.WORKLOADS[args.workload]))
        print("languages: %s" % ", ".join(order))
        print("scored cases: %s" % ", ".join(case["id"] for case in scored))
        print("runtime: %d warm-ups + %d measured runs per case/language"
              % (WARMUPS, MEASURED_RUNS))
        print("startup: %d runs of the no-argument invocation per language"
              % STARTUP_RUNS)
        print("clean build: %d builds into %s"
              % (CLEAN_BUILDS, os.path.relpath(common.BUILD_ROOT, common.REPO_ROOT)))
        print("raw JSONL: %s" % os.path.relpath(common.RAW_ROOT, common.REPO_ROOT))
        print("materiality: runtime ratio %.2f and disjoint IQR; startup %.1f ms and %.0f%%;"
              % (stats.RUNTIME_RATIO, stats.STARTUP_ABSOLUTE_MS,
                 stats.STARTUP_RELATIVE * 100))
        print("             memory 1 MiB and %.0f%%; no outlier deletion; no composite score"
              % (stats.MEMORY_RELATIVE * 100))
        print("gate: correctness records must be PASS for every listed case/language")
        print("nothing was launched")
        return 0

    if not (args.runtime or args.startup or args.clean_build):
        parser.error("choose --runtime, --startup, --clean-build or --plan")

    if args.runtime or args.startup:
        problems = check_gate(args.workload, languages, scored)
        if problems:
            print("measurement refused: correctness gate is not green")
            for problem in problems[:20]:
                print("  " + problem)
            return 2

    if args.runtime:
        measure_runtime(args.workload, languages, scored, args.seed)
    if args.startup:
        measure_startup(args.workload, languages, args.seed)
    if args.clean_build:
        measure_clean_build(args.workload, languages, args.seed)
    return 0


if __name__ == "__main__":
    sys.exit(main())
