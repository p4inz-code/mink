#!/usr/bin/env python3
"""Deterministic fixtures for the A2 treeops validation.

Creates `fixtures/src` with a realistic nested tree: matching and
non-matching extensions, an empty file, deep nesting, and names with
spaces. The tree is rebuilt from scratch on every run so the fixture is
byte-identical across machines.
"""
import os
import shutil

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "src")

# (relative path, content) — content is bytes.
FILES = [
    ("app.log", b"INFO boot\nINFO ready\nWARN disk low\n"),
    ("app.log.1", b"rotated log line\n"),
    ("notes.txt", b"hello world\nsecond line\n"),
    ("empty.txt", b""),
    ("data/report.csv", b"id,name,score\n1,alice,10\n2,bob,20\n"),
    ("data/nested/deep/report.json", b'{"ok": true, "n": 3}\n'),
    ("data/nested/deep/extra.txt", b"deep text\n"),
    ("data/nested/mid.log", b"mid log\n"),
    ("src/main.mink", b"fn main() -> Int {\n    return 0;\n}\n"),
    ("src/helper.mink", b"fn helper() -> Int {\n    return 1;\n}\n"),
    ("assets/logo.png", b"\x89PNG\r\n\x1a\nnot-a-real-png\n"),
    ("assets/blob.bin", b"\x00\x01\x02\x03\x04\x05\x06\x07"),
    ("with space/read me.txt", b"spaced file\n"),
    ("mixed/Mixed.TXT", b"upper case extension\n"),
    ("mixed/Mixed.LOG", b"upper case log\n"),
    ("only_dir/sub/.keep", b""),
    ("a/b/c/d/four_levels.log", b"deep log\n"),
    ("a/b/c/d/keep.txt", b"deep txt\n"),
]

DIRS = [
    "",
    "data",
    "data/nested",
    "data/nested/deep",
    "src",
    "assets",
    "with space",
    "mixed",
    "only_dir",
    "only_dir/sub",
    "a",
    "a/b",
    "a/b/c",
    "a/b/c/d",
]


def main() -> None:
    if os.path.isdir(SRC):
        shutil.rmtree(SRC)
    for d in DIRS:
        os.makedirs(os.path.join(SRC, d), exist_ok=True)
    for rel, content in FILES:
        path = os.path.join(SRC, rel.replace("/", os.sep))
        with open(path, "wb") as handle:
            handle.write(content)
    print(f"fixtures: {len(FILES)} files under {SRC}")


if __name__ == "__main__":
    main()
