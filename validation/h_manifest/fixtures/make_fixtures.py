#!/usr/bin/env python3
"""Create the H `manifest` fixtures.

Run from anywhere:

    python validation/h_manifest/fixtures/make_fixtures.py
"""
import json
import os
import shutil

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "src")

FILES = {
    "readme.txt": b"manifest fixture\nsecond line\n",
    "empty.txt": b"",
    "skip.bin": b"\x00\x01\x02\x03not selected",
    "data/records.csv": b"id,name\n1,alpha\n2,beta\n",
    "data/notes.log": b"2026-10-01 INFO start\n2026-10-01 WARN slow\n",
    "sub dir/spaced name.txt": b"path with spaces\n",
    # Unicode is exercised in the file *content* (UTF-8 bytes), because the
    # MINK filesystem layer is ANSI (see record.md CLASSIFICATION): a
    # non-ASCII path name is reported in the system code page and a name the
    # code page cannot represent is looked up as `?` and reported missing.
    "sub dir/nested/unicode content.txt": "caf\u00e9 \u65e5\u672c\u8a9e\n".encode("utf-8"),
    "sub dir/nested/deep.txt": b"deep\n",
    "noext": b"no extension here",
}

CONFIGS = {
    "config.json": {
        "root": SRC,
        "out": "@OUT",
        "extensions": [".txt", ".log", ".csv"],
    },
    "config_no_match.json": {
        "root": SRC,
        "out": "@OUT",
        "extensions": [".zzz"],
    },
    "config_missing_root.json": {
        "root": os.path.join(SRC, "does-not-exist"),
        "out": "@OUT",
        "extensions": [".txt"],
    },
    "config_uppercase_ext.json": {
        "root": SRC,
        "out": "@OUT",
        "extensions": [".TXT"],
    },
}

BAD_CONFIGS = {
    "bad_json.json": '{"root": }',
    "missing_keys.json": '{"root": "x"}',
    "bad_types.json": '{"root": 1, "out": "o", "extensions": ".txt"}',
    "root_not_object.json": '[1, 2, 3]',
}


def main() -> None:
    # Start from a clean tree so renamed fixtures do not linger.
    shutil.rmtree(SRC, ignore_errors=True)
    for rel, data in FILES.items():
        path = os.path.join(SRC, *rel.split("/"))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as handle:
            handle.write(data)
    for name, cfg in CONFIGS.items():
        with open(os.path.join(HERE, name), "w", encoding="utf-8") as handle:
            json.dump(cfg, handle, ensure_ascii=False, indent=1)
    for name, text in BAD_CONFIGS.items():
        with open(os.path.join(HERE, name), "w", encoding="utf-8") as handle:
            handle.write(text)
    print(f"fixtures: {len(FILES)} files, {len(CONFIGS) + len(BAD_CONFIGS)} configs")


if __name__ == "__main__":
    main()
