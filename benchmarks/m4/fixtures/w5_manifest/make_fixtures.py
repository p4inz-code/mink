#!/usr/bin/env python3
"""Create the deterministic M4 W5 `manifest` fixture trees.

    python benchmarks/m4/fixtures/w5_manifest/make_fixtures.py

ASCII path names only (MINK's filesystem layer is ANSI). UTF-8 content is
included, and a separate Unicode-path tree exists for the capability probe.
"""
import os
import shutil

HERE = os.path.dirname(os.path.abspath(__file__))

SPECIALS = {
    "readme.txt": b"manifest fixture\nsecond line\n",
    "empty.txt": b"",
    "data/records.csv": b"id,name\n1,alpha\n2,beta\n",
    "data/notes.log": b"2026-10-01 INFO start\n2026-10-01 WARN slow\n",
    "data/comma,name.txt": b"comma in the name\n",
    "sub dir/spaced name.txt": b"path with spaces\n",
    "sub dir/nested/deep.txt": b"deep\n",
    "sub dir/nested/unicode content.txt": "caf\u00e9 \u65e5\u672c\u8a9e\n".encode("utf-8"),
    "skip.bin": bytes(range(256)),
    "noext": b"no extension here",
}


def bulk(index):
    name = "bulk/g%03d" % index
    if index % 2 == 0:
        return name + ".txt", ("bulk %03d\n" % index).encode("ascii")
    return name + ".csv", ("bulk,%03d\n" % index).encode("ascii")


def write_tree(root, total_files):
    if os.path.isdir(root):
        shutil.rmtree(root)
    for rel, data in SPECIALS.items():
        path = os.path.join(root, *rel.split("/"))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as handle:
            handle.write(data)
    remaining = total_files - len(SPECIALS)
    for index in range(remaining):
        rel, data = bulk(index)
        path = os.path.join(root, *rel.split("/"))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as handle:
            handle.write(data)
    count = sum(len(files) for _base, _dirs, files in os.walk(root))
    if count != total_files:
        raise SystemExit("tree %s has %d files, expected %d"
                         % (root, count, total_files))
    return count


def main():
    print("tree_100:", write_tree(os.path.join(HERE, "tree_100"), 100))
    print("tree_400:", write_tree(os.path.join(HERE, "tree_400"), 400))

    unicode_root = os.path.join(HERE, "tree_unicode")
    if os.path.isdir(unicode_root):
        shutil.rmtree(unicode_root)
    os.makedirs(unicode_root)
    for name, data in (
        ("ascii.txt", b"plain\n"),
        ("na\u00efve caf\u00e9.txt", b"accented path\n"),
        ("\u65e5\u672c\u8a9e.txt", b"japanese path\n"),
    ):
        with open(os.path.join(unicode_root, name), "wb") as handle:
            handle.write(data)
    print("tree_unicode: 3 files (probe only)")


if __name__ == "__main__":
    main()
