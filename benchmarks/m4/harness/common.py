#!/usr/bin/env python3
"""Shared paths, workload metadata and language registry for the M4 harness.

Python 3.11 standard library only. No third-party package is used or needed.
"""
import json
import os
import sys

HARNESS_DIR = os.path.dirname(os.path.abspath(__file__))
M4_DIR = os.path.dirname(HARNESS_DIR)
REPO_ROOT = os.path.dirname(os.path.dirname(M4_DIR))

BUILD_ROOT = os.path.join(M4_DIR, ".build")
FIXTURES_ROOT = os.path.join(M4_DIR, "fixtures")
EXPECTED_ROOT = os.path.join(M4_DIR, "expected")
RESULTS_ROOT = os.path.join(M4_DIR, "results")
RAW_ROOT = os.path.join(RESULTS_ROOT, "raw")

MINK_EXE = os.path.join(REPO_ROOT, "target", "release", "mink.exe")

LANGUAGES = ("mink", "python", "rust", "go", "csharp")

# workload id -> canonical application/artifact name
WORKLOADS = {
    "w1_json": "jsontool",
    "w2_http": "netcheck",
}


def app_dir(language, workload):
    return os.path.join(M4_DIR, "apps", language, workload)


def build_dir(language, workload):
    return os.path.join(BUILD_ROOT, workload, language)


def fixtures_dir(workload):
    return os.path.join(FIXTURES_ROOT, workload)


def expected_cases_path(workload):
    return os.path.join(EXPECTED_ROOT, workload, "cases.json")


def load_cases(workload):
    with open(expected_cases_path(workload), "r", encoding="utf-8") as handle:
        document = json.load(handle)
    if document.get("workload") != workload:
        raise SystemExit(
            "expected-cases file is for %r, not %r"
            % (document.get("workload"), workload)
        )
    return document["cases"]


def _apply_tokens(text, tokens):
    for key, value in (tokens or {}).items():
        text = text.replace("{" + key + "}", str(value))
    return text


def resolve_argv(argv, fixture_dir, tokens=None):
    """Replace {fixtures}, {fixtures}/... and any {token} placeholder."""
    resolved = []
    for arg in argv:
        if arg == "{fixtures}":
            resolved.append(fixture_dir)
            continue
        if arg.startswith("{fixtures}/"):
            rest = arg[len("{fixtures}/"):]
            resolved.append(os.path.join(fixture_dir, *rest.split("/")))
            continue
        resolved.append(_apply_tokens(arg, tokens))
    return resolved


def resolve_text(text, fixture_dir, tokens=None):
    # `{fixtures}/x` must resolve to an OS-native path, not a mixed one, because
    # the implementations print the path argument exactly as received.
    text = text.replace("{fixtures}/", fixture_dir + os.sep).replace(
        "{fixtures}", fixture_dir
    )
    return _apply_tokens(text, tokens)


def run_command(language, workload):
    """Exact argv used to launch the built application (no program name for
    MINK: its argv excludes it and the harness passes user arguments only)."""
    name = WORKLOADS[workload]
    if language == "mink":
        return [os.path.join(build_dir(language, workload), name + ".exe")]
    if language == "python":
        return [sys.executable, os.path.join(app_dir(language, workload), name + ".py")]
    if language == "rust":
        return [os.path.join(build_dir(language, workload), "release", name + ".exe")]
    if language in ("go", "csharp"):
        return [os.path.join(build_dir(language, workload), name + ".exe")]
    raise SystemExit("unknown language: %r" % (language,))
