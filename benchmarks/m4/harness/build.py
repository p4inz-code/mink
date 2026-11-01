#!/usr/bin/env python3
"""Build the M4 W1 implementations from their documented commands.

    python benchmarks/m4/harness/build.py --workload w1_json
    python benchmarks/m4/harness/build.py --workload w1_json --language rust

Build outputs go to `benchmarks/m4/.build/<workload>/<language>/` so no
shared compiler/dependency cache is touched and nothing outside benchmark
directories is deleted. Rust is built with `--offline`: W1's pinned crates
must already be present in the local cargo cache, and this harness never
downloads a dependency.
"""
import argparse
import os
import shutil
import subprocess
import sys

import common


class BuildError(Exception):
    pass


def _ensure_dir(path):
    os.makedirs(path, exist_ok=True)
    return path


def _run(command, cwd, env=None, label=""):
    print("$ " + " ".join(command))
    completed = subprocess.run(
        command, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT
    )
    text = completed.stdout.decode("utf-8", "replace")
    if text.strip():
        print(text.rstrip())
    if completed.returncode != 0:
        raise BuildError("%s failed with exit %d" % (label or command[0], completed.returncode))


def build_mink(workload):
    name = common.WORKLOADS[workload]
    source = os.path.join(common.app_dir("mink", workload), name + ".mink")
    destination = _ensure_dir(common.build_dir("mink", workload))
    copied = os.path.join(destination, name + ".mink")
    shutil.copyfile(source, copied)
    if not os.path.exists(common.MINK_EXE):
        raise BuildError("MINK compiler not found: %s" % common.MINK_EXE)
    # MINK resolves `mod` files against the invocation directory (the
    # repository root), not the source directory, so the build runs from the
    # repository root while the artifact stays in the benchmark build tree.
    _run([common.MINK_EXE, "build", copied], cwd=common.REPO_ROOT, label="mink build")
    artifact = os.path.join(destination, name + ".exe")
    if not os.path.exists(artifact):
        raise BuildError("expected artifact not produced: %s" % artifact)
    return artifact


def _rust_env():
    """Override the repository's `.cargo/config.toml` +crt-static flag.

    The repository forces a static CRT for every MSVC build under the project
    tree. The M4 environment baseline was captured with the default dynamic
    CRT (`rustc -O`, VCRUNTIME140 imports), so the benchmark build restores
    that configuration explicitly instead of silently changing the recorded
    build/distribution baseline.
    """
    env = dict(os.environ)
    env["RUSTFLAGS"] = ""
    env["CARGO_TARGET_DIR"] = ""
    return env


def build_rust(workload):
    name = common.WORKLOADS[workload]
    crate = common.app_dir("rust", workload)
    target = _ensure_dir(common.build_dir("rust", workload))
    _run(
        [
            "cargo",
            "build",
            "--release",
            "--offline",
            "--manifest-path",
            os.path.join(crate, "Cargo.toml"),
            "--target-dir",
            target,
        ],
        cwd=crate,
        env=_rust_env(),
        label="cargo build",
    )
    artifact = os.path.join(target, "release", name + ".exe")
    if not os.path.exists(artifact):
        raise BuildError("expected artifact not produced: %s" % artifact)
    return artifact


def build_go(workload):
    name = common.WORKLOADS[workload]
    app = common.app_dir("go", workload)
    destination = _ensure_dir(common.build_dir("go", workload))
    artifact = os.path.join(destination, name + ".exe")
    env = dict(os.environ)
    env["GOFLAGS"] = ""
    _run(
        ["go", "build", "-trimpath", "-buildvcs=false", "-o", artifact, "."],
        cwd=app,
        env=env,
        label="go build",
    )
    if not os.path.exists(artifact):
        raise BuildError("expected artifact not produced: %s" % artifact)
    return artifact


def build_csharp(workload):
    name = common.WORKLOADS[workload]
    app = common.app_dir("csharp", workload)
    destination = _ensure_dir(common.build_dir("csharp", workload))
    _run(
        [
            "dotnet",
            "publish",
            "-c",
            "Release",
            "-r",
            "win-x64",
            "--self-contained",
            "false",
            "-o",
            destination,
            "--nologo",
        ],
        cwd=app,
        label="dotnet publish",
    )
    artifact = os.path.join(destination, name + ".exe")
    if not os.path.exists(artifact):
        raise BuildError("expected artifact not produced: %s" % artifact)
    return artifact


BUILDERS = {
    "mink": build_mink,
    "rust": build_rust,
    "go": build_go,
    "csharp": build_csharp,
}


def build(language, workload):
    if language == "python":
        # No build step: the interpreter runs the committed source directly.
        return os.path.join(common.app_dir("python", workload),
                            common.WORKLOADS[workload] + ".py")
    return BUILDERS[language](workload)


def main():
    parser = argparse.ArgumentParser(description="Build M4 applications")
    parser.add_argument("--workload", default="w1_json", choices=sorted(common.WORKLOADS))
    parser.add_argument("--language", default="all",
                        choices=("all",) + common.LANGUAGES)
    args = parser.parse_args()

    languages = common.LANGUAGES if args.language == "all" else (args.language,)
    failures = []
    for language in languages:
        print("\n=== %s / %s ===" % (args.workload, language))
        try:
            artifact = build(language, args.workload)
            print("artifact: %s" % artifact)
        except (BuildError, OSError) as error:
            print("FAILED: %s" % error)
            failures.append((language, str(error)))

    if failures:
        for language, error in failures:
            print("FAIL %s: %s" % (language, error))
        return 1
    print("\nbuild: all %d implementations ready" % len(languages))
    return 0


if __name__ == "__main__":
    sys.exit(main())
