#!/usr/bin/env python3
"""Verify the behavior documented in docs/language/LANGUAGE_BASICS.md.

Usage:
    python benchmarks/m6/session177/verify.py
    python benchmarks/m6/session177/verify.py --compiler <path-to-mink.exe>

Every case is built and run in a disposable directory. The repository
standard library is copied beside each source so `mod filesystem;` resolves
without depending on the compiler's working directory. Complete MINK fences
from the guide are compiled as well.
"""

import argparse
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
GUIDE = REPO / "docs" / "language" / "LANGUAGE_BASICS.md"

HELPER = """pub fn double(x: Int) -> Int {
    return x * 2;
}

fn secret(x: Int) -> Int {
    return x + 100;
}
"""

CASES = [
    (
        "print_line_shape",
        {
            "main.mink": """fn main() {
    rt_print_str("a=");
    rt_print_int(7);
}
"""
        },
        [],
        0,
        "a=\n7\n",
        None,
    ),
    (
        "compose_line",
        {
            "main.mink": """fn main() -> Int {
    let number = rt_str_from_int(55);
    let line = rt_str_concat("total=", number);
    rt_print_str(line);
    rt_str_free(line);
    rt_str_free(number);
    return 0;
}
"""
        },
        [],
        0,
        "total=55\n",
        None,
    ),
    (
        "format_line",
        {
            "main.mink": """fn main() -> Int {
    let line = rt_str_format("{} + {} = {}", "2", "3", "5");
    rt_print_str(line);
    rt_str_free(line);
    return 0;
}
"""
        },
        [],
        0,
        "2 + 3 = 5\n",
        None,
    ),
    (
        "float_format",
        {
            "main.mink": """fn main() {
    rt_print_float(1.0);
    rt_print_float(0.5);
    rt_print_float(-0.0);
    rt_print_float(1e20);
    rt_print_float(1.0 / 0.0);
    rt_print_float(-1.0 / 0.0);
    rt_print_float(0.0 / 0.0);
}
"""
        },
        [],
        0,
        "1\n0.5\n-0\n1e+20\nInf\n-Inf\n-NaN\n",
        None,
    ),
    (
        "map_crossing",
        {
            "main.mink": """fn total(m: Map<Int, Int>) -> Int {
    return rt_map_get(m, 1) + rt_map_get(m, 2);
}

fn build() -> Map<Int, Int> {
    let mut m = rt_map_new(2);
    m = rt_map_insert(m, 1, 10);
    m = rt_map_insert(m, 2, 20);
    return m;
}

fn main() -> Int {
    let mut m = rt_map_new(2);
    m = rt_map_insert(m, 1, 10);
    m = rt_map_insert(m, 2, 20);
    rt_print_int(total(m));
    let built = build();
    rt_print_int(total(built));
    rt_map_free(m);
    rt_map_free(built);
    return 0;
}
"""
        },
        [],
        0,
        "30\n30\n",
        None,
    ),
    (
        "module_unqualified",
        {"helper.mink": HELPER, "main.mink": """mod helper;

fn main() -> Int {
    rt_print_int(double(4));
    return 0;
}
"""},
        [],
        0,
        "8\n",
        None,
    ),
    (
        "module_private_unqualified",
        {"helper.mink": HELPER, "main.mink": """mod helper;

fn main() -> Int {
    rt_print_int(secret(4));
    return 0;
}
"""},
        [],
        0,
        "104\n",
        None,
    ),
    (
        "module_use_private",
        {"helper.mink": HELPER, "main.mink": """mod helper;
use helper::secret;

fn main() -> Int {
    rt_print_int(secret(4));
    return 0;
}
"""},
        [],
        0,
        "104\n",
        None,
    ),
    (
        "borrow_string",
        {
            "main.mink": """fn length(s: &Str) -> Int {
    return rt_str_len(*s);
}

fn main() -> Int {
    let s = rt_str_concat("a", "b");
    rt_print_int(length(&s));
    rt_str_free(s);
    return 0;
}
"""
        },
        [],
        0,
        "2\n",
        None,
    ),
    (
        "filesystem_returns",
        {
            "main.mink": """mod filesystem;

fn main() -> Int {
    let written = fs_write("note.txt", "hello");
    let size = fs_file_size("note.txt");
    let text = fs_read("note.txt");
    let missing = fs_read("absent.txt");

    rt_print_int(written);
    rt_print_int(size);
    rt_print_int(rt_str_len(text));
    rt_print_int(rt_str_len(missing));
    rt_print_int(fs_file_size("absent.txt"));

    rt_str_free(text);
    rt_str_free(missing);
    rt_print_int(fs_remove_file("note.txt"));
    return 0;
}
"""
        },
        [],
        0,
        "5\n5\n5\n0\n-1\n0\n",
        None,
    ),
    (
        "leak_exit",
        {
            "main.mink": """fn main() -> Int {
    let s = rt_str_concat("a", "b");
    rt_print_str(s);
    return 0;
}
"""
        },
        [],
        106,
        "ab\n",
        "E-R06",
    ),
]


def normalize(data):
    return data.decode("utf-8", "replace").replace("\r\n", "\n")


def prepare(work, files):
    for name, text in files.items():
        (work / name).write_text(text, encoding="utf-8")
    for source in (REPO / "stdlib").glob("*.mink"):
        shutil.copy2(source, work / source.name)


def run_case(compiler, name, files, extra_args, want_exit, want_stdout, want_stderr):
    work_root = REPO / "tmp" / "m6_session177_work"
    work_root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=name + "_", dir=str(work_root)) as td:
        work = Path(td)
        prepare(work, files)
        build = subprocess.run(
            [str(compiler), "build", "main.mink"],
            cwd=str(work),
            capture_output=True,
            timeout=120,
        )
        if build.returncode != 0:
            print("FAIL %-28s build exit %d" % (name, build.returncode))
            sys.stdout.write(normalize(build.stdout))
            sys.stderr.write(normalize(build.stderr))
            return False

        exe = work / "main.exe"
        run = subprocess.run(
            [str(exe)] + extra_args,
            cwd=str(work),
            capture_output=True,
            timeout=60,
        )
        got_stdout = normalize(run.stdout)
        got_stderr = normalize(run.stderr)
        ok = run.returncode == want_exit and got_stdout == want_stdout
        if want_stderr is not None and want_stderr not in got_stderr:
            ok = False
        if not ok:
            print("FAIL %-28s exit %d (want %d)" % (name, run.returncode, want_exit))
            print("  stdout: %r" % got_stdout)
            print("  want:   %r" % want_stdout)
            if got_stderr:
                print("  stderr: %r" % got_stderr)
            return False
        print("PASS %-28s exit %d" % (name, run.returncode))
        return True


def compile_guide_fences(compiler):
    text = GUIDE.read_text(encoding="utf-8")
    blocks = re.findall(r"```mink\n(.*?)```", text, re.S)
    checked = 0
    failed = 0
    work_root = REPO / "tmp" / "m6_session177_work"
    work_root.mkdir(parents=True, exist_ok=True)
    for index, body in enumerate(blocks):
        if "fn main" not in body:
            continue
        checked += 1
        with tempfile.TemporaryDirectory(
            prefix="fence_%02d_" % index, dir=str(work_root)
        ) as td:
            work = Path(td)
            prepare(work, {"main.mink": body, "helper.mink": HELPER})
            build = subprocess.run(
                [str(compiler), "build", "main.mink"],
                cwd=str(work),
                capture_output=True,
                timeout=120,
            )
            if build.returncode != 0:
                failed += 1
                print("FAIL guide fence %02d build exit %d" % (index, build.returncode))
                sys.stdout.write(normalize(build.stdout))
                sys.stderr.write(normalize(build.stderr))
            else:
                print("PASS guide fence %02d" % index)
    return checked, failed


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--compiler", default=str(REPO / "target" / "release" / "mink.exe"))
    args = parser.parse_args()

    compiler = Path(args.compiler)
    if not compiler.is_file():
        print("compiler not found: %s" % compiler)
        return 2

    passed = 0
    for name, files, extra_args, want_exit, want_stdout, want_stderr in CASES:
        if run_case(compiler, name, files, extra_args, want_exit, want_stdout, want_stderr):
            passed += 1

    checked, failed = compile_guide_fences(compiler)
    print("RESULT %d/%d cases passed; %d/%d guide fences compiled"
          % (passed, len(CASES), checked - failed, checked))
    return 0 if passed == len(CASES) and failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
