#!/usr/bin/env python3
"""releasepack contract driver.

Checks the MINK executable against `oracle.py`, an independent implementation
of the frozen contract (section 7 of M5_APPLICATION_SELECTION.md). The archive
is inspected with Python's `zipfile`, not by re-implementing MINK's encoder.

Usage:
    python benchmarks/m5/releasepack/tests/driver.py
    python benchmarks/m5/releasepack/tests/driver.py --no-build
    python benchmarks/m5/releasepack/tests/driver.py --update-goldens
    python benchmarks/m5/releasepack/tests/driver.py --only build

Exit status is 0 only when every check passes.
"""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
import traceback
import zipfile
from pathlib import Path

import oracle

ROOT = Path(__file__).resolve().parents[4]
APP = ROOT / "benchmarks" / "m5" / "releasepack"
SOURCE = APP / "app" / "main.mink"
BUILD = APP / "build"
EXE = BUILD / "releasepack.exe"
FIXTURES = APP / "fixtures"
EXPECTED = APP / "expected"


class Suite:
    def __init__(self, only: str | None = None) -> None:
        self.passed = 0
        self.failed: list[tuple[str, str]] = []
        self.skipped: list[tuple[str, str]] = []
        self.only = only

    def check(self, name: str, fn) -> None:
        if self.only is not None and self.only.lower() not in name.lower():
            return
        try:
            fn()
        except SkipTest as exc:
            self.skipped.append((name, str(exc)))
            print(f"SKIP  {name}: {exc}")
        except Exception as exc:  # noqa: BLE001 - the driver reports failures
            detail = traceback.format_exc()
            self.failed.append((name, detail))
            print(f"FAIL  {name}: {exc}")
            print(detail)
        else:
            self.passed += 1
            print(f"PASS  {name}")


class SkipTest(Exception):
    pass


def expect(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


@contextlib.contextmanager
def fixture_work():
    with tempfile.TemporaryDirectory(prefix="releasepack_case_") as td:
        work = Path(td) / "work"
        shutil.copytree(FIXTURES / "basic", work)
        (work / "out").mkdir(exist_ok=True)
        yield work


def run(exe: Path, work: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [str(exe), *args], cwd=str(work), capture_output=True, timeout=60
    )


def expect_exit(
    exe: Path, work: Path, args: list[str], code: int, *, stdout: bytes = b""
) -> bytes:
    proc = run(exe, work, *args)
    expect(proc.returncode == code, f"exit {proc.returncode} != {code}; stderr={proc.stderr!r}")
    expect(proc.stdout == stdout, f"stdout {proc.stdout!r} != {stdout!r}")
    expect(proc.stderr.startswith(b"releasepack: "), f"stderr not prefixed: {proc.stderr!r}")
    expect(b"\r\n" in proc.stderr, "diagnostic must be CRLF-terminated")
    return proc.stderr


def base_config() -> dict:
    return json.loads((FIXTURES / "basic" / "config.json").read_text(encoding="utf-8"))


def write_config(work: Path, config: dict, name: str = "config.json") -> None:
    (work / name).write_text(json.dumps(config, indent=2) + "\n", encoding="ascii")


def build_basic(exe: Path, work: Path) -> tuple[subprocess.CompletedProcess, dict, Path]:
    proc = run(exe, work, "build", "config.json")
    model = oracle.build_model(work, base_config())
    return proc, model, work / "out" / "release.zip"


def check_archive(path: Path, model: dict) -> None:
    facts = oracle.archive_facts(path)
    expect(
        [f["name"] for f in facts] == model["paths"],
        f"entry order {[f['name'] for f in facts]} != {model['paths']}",
    )
    for fact, row in zip(facts, model["rows"]):
        expect(fact["name"] == row[0], "entry name")
        expect(fact["size"] == row[1], f"{row[0]}: size {fact['size']} != {row[1]}")
        expect(fact["sha256"] == row[2], f"{row[0]}: sha256 mismatch")
        expect(fact["date_time"] == oracle.FIXED_DATE_TIME, f"{row[0]}: timestamp not fixed")
        expect(fact["compress_type"] == zipfile.ZIP_DEFLATED, f"{row[0]}: not DEFLATE")
        expect(fact["flag_bits"] & 0x800 == 0x800, f"{row[0]}: UTF-8 name flag missing")


def golden(name: str) -> bytes:
    path = EXPECTED / name
    expect(path.exists(), f"missing golden {path}; run with --update-goldens")
    return path.read_bytes()


def build_exe() -> None:
    mink = ROOT / "target" / "release" / "mink.exe"
    if not mink.exists():
        mink = ROOT / "target" / "debug" / "mink.exe"
    expect(mink.exists(), f"MINK compiler not found under {ROOT / 'target'}")
    proc = subprocess.run(
        [str(mink), "build", str(SOURCE)], cwd=ROOT, capture_output=True, text=True
    )
    expect(
        proc.returncode == 0,
        f"mink build failed ({proc.returncode})\n{proc.stdout}\n{proc.stderr}",
    )
    produced = SOURCE.with_suffix(".exe")
    expect(produced.exists(), f"build did not produce {produced}")
    BUILD.mkdir(parents=True, exist_ok=True)
    shutil.copy2(produced, EXE)
    print(f"built {EXE.relative_to(ROOT)} ({EXE.stat().st_size} bytes)")


def update_goldens(exe: Path) -> None:
    with fixture_work() as work:
        proc, model, archive = build_basic(exe, work)
        expect(proc.returncode == 0, f"build exit {proc.returncode}: {proc.stderr!r}")
        (EXPECTED / "basic").mkdir(parents=True, exist_ok=True)
        (EXPECTED / "basic" / "manifest.csv").write_bytes(model["manifest"])
        (EXPECTED / "basic" / "archive.sha256").write_text(
            oracle.sha256(archive.read_bytes()) + "\n", encoding="ascii"
        )
        (EXPECTED / "basic" / "stdout.txt").write_bytes(
            b"files=8 bytes=196 archive_bytes={archive_bytes}\r\n"
        )
        help_proc = run(exe, work, "--help")
        expect(help_proc.returncode == 0, f"--help exit {help_proc.returncode}")
        (EXPECTED / "help.txt").write_bytes(help_proc.stdout)
    print(f"goldens written under {EXPECTED.relative_to(ROOT)}")


def run_suite(exe: Path, only: str | None = None) -> Suite:
    suite = Suite(only)

    # --- CLI ---------------------------------------------------------------
    def help_ok() -> None:
        with fixture_work() as work:
            proc = run(exe, work, "--help")
            expect(proc.returncode == 0, f"exit {proc.returncode}")
            expect(proc.stderr == b"", f"stderr {proc.stderr!r}")
            expect(proc.stdout == golden("help.txt"), "help golden")
            for needle in (
                b"releasepack build <config.json>",
                b"releasepack verify <archive.zip> <manifest.csv>",
                b"releasepack extract <archive.zip> <dest-dir>",
            ):
                expect(needle in proc.stdout, f"help missing {needle!r}")
            expect(proc.stdout.endswith(b"\r\n"), "help must end with CRLF")
            expect(b"\n" not in proc.stdout.replace(b"\r\n", b""), "bare LF in help")

    suite.check("--help exits 0 with exact golden bytes", help_ok)

    usage_cases = [
        ("no arguments", []),
        ("unknown command", ["pack"]),
        ("unknown option", ["--wat"]),
        ("help with argument", ["--help", "build"]),
        ("build without config", ["build"]),
        ("build with two configs", ["build", "a.json", "b.json"]),
        ("verify without arguments", ["verify"]),
        ("verify with one argument", ["verify", "a.zip"]),
        ("verify with three arguments", ["verify", "a.zip", "b.csv", "c"]),
        ("extract without arguments", ["extract"]),
        ("extract with one argument", ["extract", "a.zip"]),
        ("extract with three arguments", ["extract", "a.zip", "d", "e"]),
    ]

    def usage_case(args: list[str]) -> None:
        with fixture_work() as work:
            expect_exit(exe, work, args, 1)

    for label, args in usage_cases:
        suite.check(f"usage error: {label} -> 1", lambda args=args: usage_case(args))

    # --- build -------------------------------------------------------------
    def build_contract() -> None:
        with fixture_work() as work:
            proc, model, archive = build_basic(exe, work)
            expect(proc.returncode == 0, f"exit {proc.returncode}; stderr={proc.stderr!r}")
            expect(proc.stderr == b"", f"stderr {proc.stderr!r}")
            expect(archive.is_file(), "archive missing")
            archive_bytes = archive.stat().st_size
            expect(
                proc.stdout
                == oracle.expected_stdout(model["files"], model["bytes"], archive_bytes),
                f"stdout {proc.stdout!r}",
            )
            manifest = (work / "out" / "manifest.csv").read_bytes()
            expect(manifest == model["manifest"], "manifest differs from the oracle")
            expect(manifest == golden("basic/manifest.csv"), "manifest differs from golden")
            expect(
                oracle.parse_manifest(manifest) == model["rows"], "manifest rows differ"
            )
            check_archive(archive, model)
            expected_stdout = golden("basic/stdout.txt").replace(
                b"{archive_bytes}", str(archive_bytes).encode("ascii")
            )
            expect(proc.stdout == expected_stdout, "stdout golden")
            recorded = golden("basic/archive.sha256").strip().decode("ascii")
            expect(
                oracle.sha256(archive.read_bytes()) == recorded,
                f"archive hash differs from golden {recorded}",
            )
            expect(
                sorted(p.name for p in (work / "out").iterdir())
                == ["manifest.csv", "release.zip"],
                "unexpected output files",
            )

    suite.check("build: archive, manifest, stdout and goldens", build_contract)

    def determinism() -> None:
        with fixture_work() as work:
            cfg1 = base_config()
            cfg1["archive"] = "out/a.zip"
            cfg1["manifest"] = "out/a.csv"
            cfg2 = base_config()
            cfg2["archive"] = "out/b.zip"
            cfg2["manifest"] = "out/b.csv"
            write_config(work, cfg1, "a.json")
            write_config(work, cfg2, "b.json")
            first = run(exe, work, "build", "a.json")
            second = run(exe, work, "build", "b.json")
            expect(first.returncode == 0 and second.returncode == 0, "builds must succeed")
            a = (work / "out" / "a.zip").read_bytes()
            b = (work / "out" / "b.zip").read_bytes()
            expect(a == b, "identical inputs produced different archive bytes")
            expect(
                (work / "out" / "a.csv").read_bytes()
                == (work / "out" / "b.csv").read_bytes(),
                "identical inputs produced different manifests",
            )
            expect(
                first.stdout.replace(b"out/a.zip", b"out/x")
                == second.stdout.replace(b"out/b.zip", b"out/x"),
                "stdout differs between identical builds",
            )

    suite.check("two builds of identical trees are byte-identical", determinism)

    def empty_match() -> None:
        with fixture_work() as work:
            cfg = base_config()
            cfg["include_extensions"] = [".zzz"]
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 5)
            expect(not (work / "out" / "release.zip").exists(), "empty build wrote an archive")
            expect(not (work / "out" / "manifest.csv").exists(), "empty build wrote a manifest")

    suite.check("empty match set -> 5, no outputs", empty_match)

    def build_path_errors() -> None:
        with fixture_work() as work:
            cfg = base_config()
            cfg["root"] = "missing"
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 4)
            cfg = base_config()
            cfg["root"] = "tree/README.txt"
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 4)
            expect(not (work / "out" / "release.zip").exists(), "failed build wrote an archive")
            expect(not (work / "out" / "manifest.csv").exists(), "failed build wrote a manifest")

    suite.check("root missing or not a directory -> 4", build_path_errors)

    def parent_errors() -> None:
        with fixture_work() as work:
            cfg = base_config()
            cfg["archive"] = "missing/out.zip"
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 4)
            cfg = base_config()
            cfg["manifest"] = "missing/manifest.csv"
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 4)
            (work / "blocked").write_bytes(b"a file, not a directory")
            cfg = base_config()
            cfg["archive"] = "blocked/out.zip"
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 4)
            expect(not (work / "out" / "release.zip").exists(), "failed build wrote an archive")

    suite.check("missing or non-directory output parent -> 4", parent_errors)

    def output_inside_root() -> None:
        with fixture_work() as work:
            cfg = base_config()
            cfg["archive"] = "tree/out.zip"
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 4)
            expect(not (work / "tree" / "out.zip").exists(), "inside-root archive was written")
            cfg = base_config()
            cfg["manifest"] = "tree/sub/manifest.csv"
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 4)
            expect(not (work / "out" / "release.zip").exists(), "inside-root run wrote an archive")

    suite.check("output inside root -> 4, nothing written", output_inside_root)

    def unsafe_output_paths() -> None:
        with fixture_work() as work:
            cfg = base_config()
            cfg["root"] = "../tree"
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 4)
            cfg = base_config()
            cfg["root"] = str((work / "tree").resolve())
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 4)
            cfg = base_config()
            cfg["archive"] = str((work / "abs.zip").resolve())
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 4)
            cfg = base_config()
            cfg["archive"] = "out/../release.zip"
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 4)
            cfg = base_config()
            cfg["manifest"] = "out/../release.csv"
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 4)
            cfg = base_config()
            cfg["archive"] = "out/release.zip"
            cfg["manifest"] = "out/release.zip"
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 4)
            expect(not (work / "out" / "release.zip").exists(), "same-path run wrote output")

    suite.check("absolute or '..' paths -> 4", unsafe_output_paths)

    def missing_config() -> None:
        with fixture_work() as work:
            expect_exit(exe, work, ["build", "absent.json"], 2)
            expect_exit(exe, work, ["build", "tree"], 2)

    suite.check("config missing or not a regular file -> 2", missing_config)

    def invalid_config_syntax() -> None:
        with fixture_work() as work:
            (work / "config.json").write_text("{", encoding="ascii")
            expect_exit(exe, work, ["build", "config.json"], 3)
            (work / "config.json").write_text("[]", encoding="ascii")
            expect_exit(exe, work, ["build", "config.json"], 3)
            (work / "config.json").write_text(
                '{"root":"tree","root":"tree","archive":"out/a.zip",'
                '"manifest":"out/a.csv","include_extensions":[".txt"],'
                '"exclude_dirs":[]}',
                encoding="ascii",
            )
            expect_exit(exe, work, ["build", "config.json"], 3)

    suite.check("invalid JSON or non-object config -> 3", invalid_config_syntax)

    schema_cases = [
        ("missing key", lambda c: c.pop("exclude_dirs")),
        ("extra key", lambda c: c.__setitem__("extra", 1)),
        ("root wrong type", lambda c: c.__setitem__("root", 5)),
        ("root empty", lambda c: c.__setitem__("root", "")),
        ("archive wrong type", lambda c: c.__setitem__("archive", True)),
        ("archive empty", lambda c: c.__setitem__("archive", "")),
        ("manifest wrong type", lambda c: c.__setitem__("manifest", [])),
        ("manifest empty", lambda c: c.__setitem__("manifest", "")),
        ("extensions empty", lambda c: c.__setitem__("include_extensions", [])),
        ("extensions too many", lambda c: c.__setitem__("include_extensions", [".a"] * 9)),
        ("extension lacks dot", lambda c: c.__setitem__("include_extensions", ["txt"])),
        ("extension too long", lambda c: c.__setitem__("include_extensions", [".abcdefghi"])),
        ("extension non-alnum", lambda c: c.__setitem__("include_extensions", [".tx-t"])),
        ("excludes too many", lambda c: c.__setitem__("exclude_dirs", ["d"] * 9)),
        ("exclude empty", lambda c: c.__setitem__("exclude_dirs", [""])),
        ("exclude with slash", lambda c: c.__setitem__("exclude_dirs", ["a/b"])),
        ("exclude with colon", lambda c: c.__setitem__("exclude_dirs", ["a:b"])),
        ("extensions not an array", lambda c: c.__setitem__("include_extensions", ".txt")),
    ]

    def schema_case(mutate) -> None:
        with fixture_work() as work:
            cfg = base_config()
            mutate(cfg)
            write_config(work, cfg)
            expect_exit(exe, work, ["build", "config.json"], 3)

    for label, mutate in schema_cases:
        suite.check(f"schema: {label} -> 3", lambda mutate=mutate: schema_case(mutate))

    def unreadable_during_build() -> None:
        if sys.platform != "win32":
            raise SkipTest("Windows sharing lock needed")
        import ctypes
        from ctypes import wintypes

        with fixture_work() as work:
            target = work / "tree" / "README.txt"
            kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
            kernel32.CreateFileW.argtypes = [
                wintypes.LPCWSTR,
                wintypes.DWORD,
                wintypes.DWORD,
                wintypes.LPVOID,
                wintypes.DWORD,
                wintypes.DWORD,
                wintypes.HANDLE,
            ]
            kernel32.CreateFileW.restype = wintypes.HANDLE
            kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
            handle = kernel32.CreateFileW(str(target), 0x80000000, 0, None, 3, 0x80, None)
            if handle in (None, ctypes.c_void_p(-1).value):
                raise SkipTest("could not acquire an exclusive sharing lock")
            try:
                expect_exit(exe, work, ["build", "config.json"], 4)
                expect(not (work / "out" / "release.zip").exists(), "failed scan wrote an archive")
                expect(not (work / "out" / "manifest.csv").exists(), "failed scan wrote a manifest")
            finally:
                kernel32.CloseHandle(handle)

    suite.check("unreadable file during build -> 4, no outputs", unreadable_during_build)

    # --- verify ------------------------------------------------------------
    def verify_ok() -> None:
        with fixture_work() as work:
            proc, model, archive = build_basic(exe, work)
            expect(proc.returncode == 0, "build must succeed")
            verified = run(exe, work, "verify", "out/release.zip", "out/manifest.csv")
            expect(verified.returncode == 0, f"exit {verified.returncode}: {verified.stderr!r}")
            expect(verified.stdout == oracle.expected_verified(model["files"]), f"{verified.stdout!r}")
            expect(verified.stderr == b"", f"stderr {verified.stderr!r}")

    suite.check("verify accepts a matching archive and manifest", verify_ok)

    def verify_foreign_archive() -> None:
        import zipfile

        with fixture_work() as work:
            model = oracle.build_model(work, base_config())
            out = work / "out"
            out.mkdir(exist_ok=True)
            (out / "foreign.csv").write_bytes(model["manifest"])
            entries = [
                (rel, (work / "tree" / rel).read_bytes())
                for rel in reversed(model["paths"])
            ]
            oracle.make_archive(out / "foreign.zip", entries, zipfile.ZIP_STORED)
            proc = run(exe, work, "verify", "out/foreign.zip", "out/foreign.csv")
            expect(proc.returncode == 0, f"exit {proc.returncode}: {proc.stderr!r}")
            expect(proc.stdout == oracle.expected_verified(model["files"]), f"{proc.stdout!r}")
            expect(proc.stderr == b"", f"stderr {proc.stderr!r}")
            proc = run(exe, work, "extract", "out/foreign.zip", "dest")
            expect(proc.returncode == 0, f"exit {proc.returncode}: {proc.stderr!r}")
            expect(proc.stdout == oracle.expected_extracted(model["files"]), f"{proc.stdout!r}")
            for rel in model["paths"]:
                expect(
                    (work / "dest" / rel).read_bytes()
                    == (work / "tree" / rel).read_bytes(),
                    f"extracted bytes differ: {rel}",
                )

    suite.check("foreign stored, out-of-order archive verifies and extracts", verify_foreign_archive)

    def manifest_crlf() -> None:
        with fixture_work() as work:
            proc, model, archive = build_basic(exe, work)
            expect(proc.returncode == 0, "build must succeed")
            text = (work / "out" / "manifest.csv").read_bytes().replace(b"\n", b"\r\n")
            (work / "out" / "crlf.csv").write_bytes(text)
            proc = run(exe, work, "verify", "out/release.zip", "out/crlf.csv")
            expect(proc.returncode == 0, f"exit {proc.returncode}: {proc.stderr!r}")
            expect(proc.stdout == oracle.expected_verified(model["files"]), f"{proc.stdout!r}")
            expect(proc.stderr == b"", f"stderr {proc.stderr!r}")

    suite.check("CRLF manifest is accepted", manifest_crlf)

    def verify_missing() -> None:
        with fixture_work() as work:
            expect_exit(exe, work, ["verify", "absent.zip", "absent.csv"], 2)
            proc, model, archive = build_basic(exe, work)
            expect(proc.returncode == 0, "build must succeed")
            expect_exit(exe, work, ["verify", "out/release.zip", "absent.csv"], 2)
            expect_exit(exe, work, ["verify", "absent.zip", "out/manifest.csv"], 2)

    suite.check("missing archive or manifest -> 2", verify_missing)

    def manifest_schema_errors() -> None:
        with fixture_work() as work:
            proc, model, archive = build_basic(exe, work)
            expect(proc.returncode == 0, "build must succeed")
            original = (work / "out" / "manifest.csv").read_bytes()
            bad_documents = [
                ("bad header", b"path,bytes,hash\n"),
                ("two fields", b"path,size,sha256\nREADME.txt,39\n"),
                ("bad size", b"path,size,sha256\nREADME.txt,3x," + model["rows"][0][2].encode() + b"\n"),
                ("bad sha", b"path,size,sha256\nREADME.txt,39,deadbeef\n"),
                (
                    "duplicate path",
                    b"path,size,sha256\n"
                    + model["rows"][0][0].encode()
                    + b","
                    + str(model["rows"][0][1]).encode()
                    + b","
                    + model["rows"][0][2].encode()
                    + b"\n"
                    + model["rows"][0][0].encode()
                    + b","
                    + str(model["rows"][0][1]).encode()
                    + b","
                    + model["rows"][0][2].encode()
                    + b"\n",
                ),
            ]
            for label, data in bad_documents:
                (work / "out" / "manifest.csv").write_bytes(data)
                expect_exit(exe, work, ["verify", "out/release.zip", "out/manifest.csv"], 3)
            (work / "out" / "manifest.csv").write_bytes(original)

    suite.check("malformed manifests -> 3", manifest_schema_errors)

    def verify_mismatches() -> None:
        with fixture_work() as work:
            proc, model, archive = build_basic(exe, work)
            expect(proc.returncode == 0, "build must succeed")
            rows = list(model["rows"])

            # extra: an archive entry is absent from the manifest
            (work / "out" / "manifest.csv").write_bytes(oracle.manifest_bytes(rows[1:]))
            proc = run(exe, work, "verify", "out/release.zip", "out/manifest.csv")
            expect(proc.returncode == 6, f"extra: exit {proc.returncode}")
            expect(
                proc.stdout == oracle.mismatch_line(rows[0][0], "extra"),
                f"extra stdout {proc.stdout!r}",
            )

            # missing: a manifest row is absent from the archive
            ghost = ("ghost.txt", 3, oracle.sha256(b"abc"))
            (work / "out" / "manifest.csv").write_bytes(
                oracle.manifest_bytes(rows + [ghost])
            )
            proc = run(exe, work, "verify", "out/release.zip", "out/manifest.csv")
            expect(proc.returncode == 6, f"missing: exit {proc.returncode}")
            expect(
                proc.stdout == oracle.mismatch_line("ghost.txt", "missing"),
                f"missing stdout {proc.stdout!r}",
            )

            # size
            changed = list(rows)
            changed[1] = (changed[1][0], changed[1][1] + 1, changed[1][2])
            (work / "out" / "manifest.csv").write_bytes(oracle.manifest_bytes(changed))
            proc = run(exe, work, "verify", "out/release.zip", "out/manifest.csv")
            expect(proc.returncode == 6, f"size: exit {proc.returncode}")
            expect(
                proc.stdout == oracle.mismatch_line(changed[1][0], "size"),
                f"size stdout {proc.stdout!r}",
            )

            # sha256
            changed = list(rows)
            changed[2] = (changed[2][0], changed[2][1], "0" * 64)
            (work / "out" / "manifest.csv").write_bytes(oracle.manifest_bytes(changed))
            proc = run(exe, work, "verify", "out/release.zip", "out/manifest.csv")
            expect(proc.returncode == 6, f"sha: exit {proc.returncode}")
            expect(
                proc.stdout == oracle.mismatch_line(changed[2][0], "sha256"),
                f"sha stdout {proc.stdout!r}",
            )

            # multiple mismatches, sorted by path
            combined = list(rows[1:])  # README extra
            combined.append(ghost)  # ghost missing
            combined[1] = (combined[1][0], combined[1][1] + 5, combined[1][2])
            (work / "out" / "manifest.csv").write_bytes(oracle.manifest_bytes(combined))
            proc = run(exe, work, "verify", "out/release.zip", "out/manifest.csv")
            expect(proc.returncode == 6, f"combined: exit {proc.returncode}")
            expected = b"".join(
                sorted(
                    [
                        oracle.mismatch_line(rows[0][0], "extra"),
                        oracle.mismatch_line("ghost.txt", "missing"),
                        oracle.mismatch_line(combined[1][0], "size"),
                    ]
                )
            )
            expect(proc.stdout == expected, f"combined stdout {proc.stdout!r}")

    suite.check("verify reports extra/missing/size/sha256, sorted -> 6", verify_mismatches)

    def verify_corrupt() -> None:
        with fixture_work() as work:
            proc, model, archive = build_basic(exe, work)
            expect(proc.returncode == 0, "build must succeed")
            data = archive.read_bytes()
            archive.write_bytes(data[: max(1, len(data) // 3)])
            expect_exit(exe, work, ["verify", "out/release.zip", "out/manifest.csv"], 6)

    suite.check("corrupt archive on verify -> 6", verify_corrupt)

    # --- extract -----------------------------------------------------------
    def extract_ok() -> None:
        with fixture_work() as work:
            proc, model, archive = build_basic(exe, work)
            expect(proc.returncode == 0, "build must succeed")
            extracted = run(exe, work, "extract", "out/release.zip", "dest")
            expect(extracted.returncode == 0, f"exit {extracted.returncode}: {extracted.stderr!r}")
            expect(
                extracted.stdout == oracle.expected_extracted(model["files"]),
                f"stdout {extracted.stdout!r}",
            )
            expect(extracted.stderr == b"", f"stderr {extracted.stderr!r}")
            source = work / "tree"
            dest = work / "dest"
            expect(dest.is_dir(), "destination was not created")
            for rel in model["paths"]:
                expect((dest / rel).is_file(), f"missing extracted file {rel}")
                expect(
                    (dest / rel).read_bytes() == (source / rel).read_bytes(),
                    f"extracted bytes differ for {rel}",
                )
            extracted_files = {
                p.relative_to(dest).as_posix() for p in dest.rglob("*") if p.is_file()
            }
            expect(extracted_files == set(model["paths"]), f"extra files: {extracted_files}")

    suite.check("extract round-trips the tree byte-for-byte", extract_ok)

    def extract_errors() -> None:
        with fixture_work() as work:
            proc, model, archive = build_basic(exe, work)
            expect(proc.returncode == 0, "build must succeed")
            expect_exit(exe, work, ["extract", "absent.zip", "dest"], 2)
            expect_exit(exe, work, ["extract", "out/release.zip", "missing/dest"], 4)
            (work / "blocked").write_bytes(b"file")
            expect_exit(exe, work, ["extract", "out/release.zip", "blocked"], 4)
            data = archive.read_bytes()
            archive.write_bytes(data[: max(1, len(data) // 3)])
            expect_exit(exe, work, ["extract", "out/release.zip", "dest"], 6)

    suite.check("extract missing/destination/corrupt errors", extract_errors)

    def unsafe_archives() -> None:
        candidates = [
            ("traversal", [("../escape.txt", b"owned")]),
            ("absolute", [("/absolute.txt", b"owned")]),
            ("drive", [("C:/drive.txt", b"owned")]),
            ("nested traversal", [("dir/../../escape.txt", b"owned")]),
        ]
        for label, entries in candidates:
            with fixture_work() as work:
                archive = work / "out" / "evil.zip"
                oracle.make_archive(archive, entries)
                proc = run(exe, work, "extract", "out/evil.zip", "dest")
                expect(proc.returncode == 6, f"{label}: exit {proc.returncode}")
                expect(proc.stderr.startswith(b"releasepack: "), f"{label}: stderr")
                expect(
                    not (work / "escape.txt").exists()
                    and not (work / "absolute.txt").exists()
                    and not (work / "drive.txt").exists(),
                    f"{label}: wrote outside the destination",
                )
                if (work / "dest").exists():
                    written = [p for p in (work / "dest").rglob("*") if p.is_file()]
                    expect(written == [], f"{label}: partial tree written: {written}")

    suite.check("unsafe archive names -> 6, nothing written", unsafe_archives)

    # --- determinism / distribution ---------------------------------------
    def repeated_builds() -> None:
        snapshots = []
        for _ in range(3):
            with fixture_work() as work:
                proc, model, archive = build_basic(exe, work)
                expect(proc.returncode == 0, f"exit {proc.returncode}")
                snapshots.append(
                    (
                        oracle.sha256(archive.read_bytes()),
                        (work / "out" / "manifest.csv").read_bytes(),
                        proc.stdout,
                    )
                )
        for snapshot in snapshots[1:]:
            expect(snapshot == snapshots[0], "build output changed across identical runs")

    suite.check("three repeated builds are byte-identical", repeated_builds)

    def standalone_outside_tree() -> None:
        with tempfile.TemporaryDirectory(prefix="releasepack_bin_") as bd:
            with tempfile.TemporaryDirectory(prefix="releasepack_cwd_") as cd:
                bin_dir = Path(bd)
                cwd = Path(cd)
                exe2 = bin_dir / "releasepack.exe"
                shutil.copy2(exe, exe2)
                shutil.copytree(FIXTURES / "basic", cwd / "work")
                work = cwd / "work"
                (work / "out").mkdir(exist_ok=True)
                built = run(exe2, work, "build", "config.json")
                expect(built.returncode == 0, f"exit {built.returncode}: {built.stderr!r}")
                model = oracle.build_model(work, base_config())
                check_archive(work / "out" / "release.zip", model)
                verified = run(exe2, work, "verify", "out/release.zip", "out/manifest.csv")
                expect(verified.returncode == 0, f"verify exit {verified.returncode}")
                expect(
                    verified.stdout == oracle.expected_verified(model["files"]),
                    f"verify stdout {verified.stdout!r}",
                )
                extracted = run(exe2, work, "extract", "out/release.zip", "dest")
                expect(extracted.returncode == 0, f"extract exit {extracted.returncode}")
                expect(
                    (work / "dest" / "README.txt").read_bytes()
                    == (work / "tree" / "README.txt").read_bytes(),
                    "standalone extraction bytes",
                )
                expect(
                    not (bin_dir / "out").exists(),
                    "releasepack used the executable directory instead of the working directory",
                )

    suite.check("standalone exe outside the source tree", standalone_outside_tree)

    return suite


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--no-build", action="store_true", help="use the existing exe")
    parser.add_argument(
        "--update-goldens", action="store_true", help="rewrite expected/ from the oracle"
    )
    parser.add_argument("--only", help="run only checks whose name contains this text")
    args = parser.parse_args()

    if not args.no_build:
        build_exe()
    expect(EXE.exists(), f"missing {EXE}; run without --no-build")

    if args.update_goldens:
        update_goldens(EXE)

    suite = run_suite(EXE, only=args.only)
    total = suite.passed + len(suite.failed) + len(suite.skipped)
    print()
    print(
        f"summary: {suite.passed} passed, {len(suite.failed)} failed, "
        f"{len(suite.skipped)} skipped, {total} checks"
    )
    if suite.failed:
        print("\nfailures:")
        for name, message in suite.failed:
            print(f"  - {name}: {message}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
