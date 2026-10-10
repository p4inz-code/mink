#!/usr/bin/env python3
"""Python reference implementation of the H `manifest` utility.

Mirrors `validation/h_manifest/main.mink` byte-for-byte:

    manifest <config.json>

The config is `{"root": ..., "out": ..., "extensions": [...]}`; the tool walks
`root`, selects files whose extension is in the list (case-insensitive),
hashes each with SHA-256, writes a `path,size,sha256,ext` CSV (first 16 hex
characters of the digest, LF terminators) and prints a summary.

Deliberate mirrors of MINK conventions (see record.md):
  * `rt_print_str` appends CRLF, so every line is written with `b"\\r\\n"`;
  * `rt_argc`/`rt_argv` exclude the program name, and any argument starting
    with `-` is a usage error (the tool has no options);
  * relative paths use `/` separators and sort in byte order;
  * there is no exception type: failures are messages plus exit codes.
"""
import csv
import hashlib
import io
import json
import os
import sys


def w_err(text: str) -> None:
    sys.stderr.buffer.write(text.encode("utf-8") + b"\r\n")


def w_line(text: str) -> None:
    sys.stdout.buffer.write(text.encode("utf-8") + b"\r\n")


def fail_msg(label: str) -> int:
    w_err("manifest: " + label)
    return 1


def fail_file(label: str, path: str) -> int:
    w_err("manifest: " + label + path)
    return 1


def selected(rel: str, extensions) -> bool:
    ext = os.path.splitext(rel)[1]
    if not ext:
        return False
    return any(ext.lower() == wanted.lower() for wanted in extensions)


def main(argv) -> int:
    for arg in argv:
        if arg.startswith("-"):
            fail_msg("this tool has no options")
            return 1
    if len(argv) != 1:
        fail_msg("usage: manifest <config.json>")
        return 1

    path = argv[0]
    if not os.path.exists(path):
        fail_file("config does not exist: ", path)
        return 2
    if not os.path.isfile(path):
        fail_file("not a regular file: ", path)
        return 2
    with open(path, "rb") as handle:
        raw = handle.read()
    try:
        config = json.loads(raw.decode("utf-8"))
    except (ValueError, UnicodeDecodeError):
        fail_msg("config is not valid JSON")
        return 3
    if not isinstance(config, dict):
        fail_msg("config must be a JSON object")
        return 3
    for key in ("root", "out", "extensions"):
        if key not in config:
            fail_msg("config needs root, out and extensions")
            return 3
    root = config["root"]
    out = config["out"]
    extensions = config["extensions"]
    if not isinstance(root, str) or not isinstance(out, str) \
            or not isinstance(extensions, list):
        fail_msg("config types must be string, string, array")
        return 3
    if not os.path.exists(root):
        fail_file("root does not exist: ", root)
        return 4

    files = []
    for base, _dirs, names in os.walk(root):
        for name in names:
            full = os.path.join(base, name)
            rel = os.path.relpath(full, root).replace(os.sep, "/")
            if selected(rel, extensions):
                files.append(rel)
    files.sort()
    if not files:
        fail_msg("no file matched the extensions")
        return 5

    rows = [["path", "size", "sha256", "ext"]]
    total_bytes = 0
    for rel in files:
        full = os.path.join(root, *rel.split("/"))
        with open(full, "rb") as handle:
            data = handle.read()
        total_bytes += len(data)
        rows.append([rel, str(len(data)),
                     hashlib.sha256(data).hexdigest()[:16],
                     os.path.splitext(rel)[1]])

    buffer = io.StringIO()
    writer = csv.writer(buffer, lineterminator="\n")
    writer.writerows(rows)
    try:
        with open(out, "wb") as handle:
            handle.write(buffer.getvalue().encode("utf-8"))
    except OSError:
        fail_file("cannot write report: ", out)
        return 4

    w_line("files=" + str(len(files)))
    w_line("bytes=" + str(total_bytes))
    w_line("first=" + files[0])
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
