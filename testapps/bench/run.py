#!/usr/bin/env python3
"""Benchmark runner: the same four programs in MINK, Python, Go and Rust.

    python testapps/bench/run.py [--runs 5] [--mink path\\to\\mink.exe]

Each program is built once and run several times; the median wall time of a whole process (start to exit,
output included) is reported. Every language must print the same answer or the benchmark fails. Results are
written to testapps/bench/results.md and results.json. Nothing here is tuned: the programs are the plain
version of each algorithm in each language, written the way a beginner would write them.
"""
import argparse, json, os, platform, statistics, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
OUT = os.path.join(HERE, "out")
BENCHES = ["fib", "loop", "sieve", "collatz"]
WHAT = {
    "fib": "recursive Fibonacci of 32 (function calls)",
    "loop": "30 million iterations of integer arithmetic",
    "sieve": "sieve of Eratosthenes to 2 million (array reads and writes)",
    "collatz": "longest Collatz chain under 100,000 (branches and division)",
}


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def tool_version(cmd):
    try:
        out = sh(cmd)
        return (out.stdout or out.stderr).strip().splitlines()[0]
    except Exception:
        return None


def cpu_name():
    try:
        out = sh(["powershell", "-NoProfile", "-Command", "(Get-CimInstance Win32_Processor).Name"])
        return out.stdout.strip() or platform.processor()
    except Exception:
        return platform.processor()


def build(mink):
    os.makedirs(OUT, exist_ok=True)
    cmds = {}
    for b in BENCHES:
        m = os.path.join(HERE, "mink", b + ".mink")
        r = sh([mink, "build", m])
        if r.returncode != 0:
            sys.exit(f"mink could not build {b}: {r.stderr}")
        cmds[("MINK", b)] = [os.path.join(HERE, "mink", b + ".exe")]
        cmds[("Python", b)] = [sys.executable, os.path.join(HERE, "python", b + ".py")]
        if tool_version(["go", "version"]):
            exe = os.path.join(OUT, f"go_{b}.exe")
            r = sh(["go", "build", "-o", exe, os.path.join(HERE, "go", b + ".go")])
            if r.returncode != 0:
                sys.exit(f"go could not build {b}: {r.stderr}")
            cmds[("Go", b)] = [exe]
        exe = os.path.join(OUT, f"rust_{b}.exe")
        r = sh(["rustc", "-O", "-o", exe, os.path.join(HERE, "rust", b + ".rs")])
        if r.returncode != 0:
            sys.exit(f"rustc could not build {b}: {r.stderr}")
        cmds[("Rust", b)] = [exe]
    return cmds


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--mink", default=os.path.join(REPO, "target", "release", "mink.exe"))
    args = ap.parse_args()
    cmds = build(args.mink)
    langs = [l for l in ["MINK", "Python", "Go", "Rust"] if any(k[0] == l for k in cmds)]
    results = {}
    answers = {}
    for b in BENCHES:
        for lang in langs:
            times = []
            for _ in range(args.runs):
                t0 = time.perf_counter()
                r = subprocess.run(cmds[(lang, b)], capture_output=True, text=True)
                times.append((time.perf_counter() - t0) * 1000)
                if r.returncode != 0:
                    sys.exit(f"{lang} {b} exited {r.returncode}: {r.stderr}")
                out = r.stdout.split()
                answers.setdefault(b, out)
                if out != answers[b]:
                    sys.exit(f"{lang} {b} printed {out}, expected {answers[b]}")
            results[(lang, b)] = statistics.median(times)
            print(f"{b:8} {lang:7} {results[(lang, b)]:9.1f} ms", flush=True)
    info = {
        "cpu": cpu_name(),
        "os": platform.platform(),
        "mink": tool_version([args.mink, "--version"]),
        "python": tool_version([sys.executable, "--version"]),
        "go": tool_version(["go", "version"]),
        "rust": tool_version(["rustc", "--version"]),
        "runs": args.runs,
    }
    with open(os.path.join(HERE, "results.json"), "w") as f:
        json.dump({"info": info, "median_ms": {f"{l}/{b}": round(v, 1) for (l, b), v in results.items()}}, f, indent=2)
    lines = ["# Benchmark results", "", f"Median of {args.runs} runs, whole-process wall time in milliseconds (lower is faster).", ""]
    lines.append("| Benchmark | " + " | ".join(langs) + " | MINK vs Python |")
    lines.append("|---|" + "---|" * (len(langs) + 1))
    for b in BENCHES:
        row = [f"{results[(l, b)]:.0f}" for l in langs]
        ratio = results[("Python", b)] / results[("MINK", b)]
        lines.append(f"| {WHAT[b]} | " + " | ".join(row) + f" | {ratio:.1f}x faster |")
    lines += ["", "## Machine and tools", ""]
    for k, v in info.items():
        lines.append(f"- {k}: {v}")
    lines += [
        "",
        "## Notes",
        "",
        "- There was no C or C++ compiler on the test PC, so Go and Rust stand in as the compiled baselines.",
        "- The four programs are the plain version of each algorithm, not tuned. Rust was built with `rustc -O`; Go with the default `go build`.",
        "- Times include starting the process, so very short runs are dominated by start-up.",
        "- One machine, one run of the whole suite. Treat the numbers as a starting point, not a verdict.",
    ]
    with open(os.path.join(HERE, "results.md"), "w") as f:
        f.write("\n".join(lines) + "\n")
    print("\n" + "\n".join(lines))


if __name__ == "__main__":
    main()
