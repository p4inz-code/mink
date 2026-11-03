#!/usr/bin/env python3
"""Independent oracle for the frozen `releasepack` contract.

Written from section 7 of `docs/benchmarks/M5/M5_APPLICATION_SELECTION.md`.
It computes the expected file selection, sizes, SHA-256 digests and manifest
bytes from the fixture tree, and inspects a produced ZIP with Python's own
`zipfile` (an independent ZIP reader) rather than trusting the MINK encoder.

The archive *bytes* are not reproduced here on purpose: reproducing a DEFLATE
stream would just re-implement MINK's encoder. Instead the oracle checks what
the contract actually specifies — sorted entry names, sizes, digests, the fixed
1980-01-01 DOS epoch, DEFLATE entries and a manifest that matches all of them.
Determinism is checked by comparing two independent builds byte-for-byte.
"""

from __future__ import annotations

import csv
import hashlib
import io
import os
import zipfile
from pathlib import Path

MANIFEST_HEADER = ["path", "size", "sha256"]
FIXED_DATE_TIME = (1980, 1, 1, 0, 0, 0)


def selected_paths(root: Path, include_extensions, exclude_dirs) -> list[str]:
    """Contract selection: recursive walk, case-insensitive ext/exclude."""
    exts = {str(x).lower() for x in include_extensions}
    excludes = {str(x).lower() for x in exclude_dirs}
    found: list[str] = []
    for current, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(d for d in dirnames if d.lower() not in excludes)
        current_path = Path(current)
        for filename in sorted(filenames):
            index = filename.rfind(".")
            ext = filename[index:].lower() if index >= 0 else ""
            if ext not in exts:
                continue
            found.append((current_path / filename).relative_to(root).as_posix())
    found.sort(key=lambda item: item.encode("ascii"))
    return found


def build_model(work: Path, config: dict) -> dict:
    """Everything the build contract implies, from the fixture tree alone."""
    root = work / config["root"]
    paths = selected_paths(
        root, config["include_extensions"], config["exclude_dirs"]
    )
    rows: list[tuple[str, int, str]] = []
    total_bytes = 0
    for rel in paths:
        data = (root / rel).read_bytes()
        total_bytes += len(data)
        rows.append((rel, len(data), hashlib.sha256(data).hexdigest()))
    return {
        "rows": rows,
        "files": len(rows),
        "bytes": total_bytes,
        "manifest": manifest_bytes(rows),
        "paths": paths,
    }


def manifest_bytes(rows: list[tuple[str, int, str]]) -> bytes:
    """RFC-4180 CSV, header `path,size,sha256`, LF records."""
    out = io.StringIO(newline="")
    writer = csv.writer(out, lineterminator="\n")
    writer.writerow(MANIFEST_HEADER)
    for row in rows:
        writer.writerow(row)
    return out.getvalue().encode("ascii")


def parse_manifest(data: bytes) -> list[tuple[str, int, str]]:
    text = data.decode("ascii")
    reader = csv.reader(io.StringIO(text, newline=""))
    rows = list(reader)
    if not rows or rows[0] != MANIFEST_HEADER:
        raise AssertionError(f"bad manifest header: {rows[:1]!r}")
    out = []
    for row in rows[1:]:
        if len(row) != 3:
            raise AssertionError(f"bad manifest row: {row!r}")
        out.append((row[0], int(row[1]), row[2]))
    return out


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def archive_facts(path: Path) -> list[dict]:
    """Names, sizes, digests and metadata read by Python's `zipfile`."""
    facts = []
    with zipfile.ZipFile(path) as archive:
        for info in archive.infolist():
            payload = archive.read(info)
            facts.append(
                {
                    "name": info.filename,
                    "size": info.file_size,
                    "sha256": sha256(payload),
                    "date_time": info.date_time,
                    "compress_type": info.compress_type,
                    "flag_bits": info.flag_bits,
                    "payload": payload,
                }
            )
    return facts


def make_archive(
    path: Path,
    entries: list[tuple[str, bytes]],
    compression: int = zipfile.ZIP_DEFLATED,
) -> None:
    """Craft an archive with arbitrary (possibly unsafe) names."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w", compression=compression) as archive:
        for name, data in entries:
            archive.writestr(name, data)


def expected_stdout(files: int, total_bytes: int, archive_bytes: int) -> bytes:
    return (
        f"files={files} bytes={total_bytes} archive_bytes={archive_bytes}\r\n"
    ).encode("ascii")


def expected_verified(files: int) -> bytes:
    return f"verified files={files}\r\n".encode("ascii")


def expected_extracted(files: int) -> bytes:
    return f"extracted files={files}\r\n".encode("ascii")


def mismatch_line(path: str, reason: str) -> bytes:
    return f"MISMATCH {path} {reason}\r\n".encode("ascii")


# ---------------------------------------------------------------------------
# Self-check: the oracle's own CSV quoting and hashing.
# ---------------------------------------------------------------------------

def _self_check() -> None:
    rows = [("a,b.txt", 1, sha256(b"x")), ("plain.txt", 0, sha256(b""))]
    data = manifest_bytes(rows)
    assert data.split(b"\n")[1] == (
        b'"a,b.txt",1,' + sha256(b"x").encode("ascii")
    ), data
    assert parse_manifest(data) == rows
    assert sha256(b"") == (
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    )
    assert expected_stdout(1, 2, 3) == b"files=1 bytes=2 archive_bytes=3\r\n"


_self_check()
