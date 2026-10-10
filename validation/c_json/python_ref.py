#!/usr/bin/env python3
"""Python reference implementation of the C `jsontool` utility.

Mirrors `validation/c_json/main.mink` byte-for-byte:

    jsontool <validate|type|keys|count|get|sum|compact> <file> [args]

Deliberate mirrors of MINK conventions (see record.md):
  * `rt_print_str` appends CRLF without translating embedded newlines, so
    stdout and stderr lines are written with an explicit `b"\\r\\n"`;
  * `rt_argc`/`rt_argv` exclude the program name; any argument starting with
    `-` is a usage error (the tool has no options);
  * the file is loaded and parsed *before* the command is dispatched, so a
    missing file or invalid JSON is reported even for an unknown command;
  * numbers keep their source text (the MINK parser is textual), and `sum`
    only adds integer-looking values.
"""
import json
import os
import sys

USAGE = ("usage: jsontool <validate|type|keys|count|get|sum|compact> "
         "<file> [args]")


def w_err(text: str) -> None:
    sys.stderr.buffer.write(text.encode("utf-8") + b"\r\n")


def w_out(text: str) -> None:
    sys.stdout.buffer.write(text.encode("utf-8") + b"\r\n")


def fail_msg(label: str) -> int:
    w_err("jsontool: " + label)
    return 1


def fail_file(label: str, path: str) -> int:
    w_err("jsontool: " + label + path)
    return 1


def fail_shape(label: str) -> int:
    """Shape/missing-member errors exit 4 (distinct from usage, exit 1)."""
    w_err("jsontool: " + label)
    return 4


def type_name(value) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "bool"
    if isinstance(value, (int, float)):
        return "number"
    if isinstance(value, str):
        return "string"
    if isinstance(value, list):
        return "array"
    if isinstance(value, dict):
        return "object"
    return "unknown"


def compact(value) -> str:
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False)


def main(argv) -> int:
    for arg in argv:
        if arg.startswith("-"):
            fail_msg("this tool has no options")
            return 1
    if len(argv) < 2:
        fail_msg(USAGE)
        return 1

    command = argv[0]
    path = argv[1]

    if not os.path.exists(path):
        fail_file("input file does not exist: ", path)
        return 2
    if not os.path.isfile(path):
        fail_file("not a regular file: ", path)
        return 2
    with open(path, "rb") as handle:
        raw = handle.read()
    try:
        doc = json.loads(raw.decode("utf-8"))
    except (ValueError, UnicodeDecodeError):
        fail_msg("invalid JSON")
        return 3

    if command == "validate":
        if len(argv) != 2:
            return fail_msg("usage: jsontool validate <file>")
        w_out("ok")
        return 0

    if command == "type":
        if len(argv) != 2:
            return fail_msg("usage: jsontool type <file>")
        w_out(type_name(doc))
        return 0

    if command == "keys":
        if len(argv) != 2:
            return fail_msg("usage: jsontool keys <file>")
        if not isinstance(doc, dict):
            return fail_shape("document is not an object")
        for key in doc:
            w_out(key)
        return 0

    if command == "count":
        if len(argv) != 2:
            return fail_msg("usage: jsontool count <file>")
        if not isinstance(doc, list):
            return fail_shape("document is not an array")
        w_out(str(len(doc)))
        return 0

    if command == "get":
        if len(argv) != 3:
            return fail_msg("usage: jsontool get <file> <key>")
        if not isinstance(doc, dict):
            return fail_shape("document is not an object")
        if argv[2] not in doc:
            return fail_shape("no such key: " + argv[2])
        w_out(compact(doc[argv[2]]))
        return 0

    if command == "sum":
        if len(argv) != 3:
            return fail_msg("usage: jsontool sum <file> <field>")
        if not isinstance(doc, list):
            return fail_shape("document is not an array")
        field = argv[2]
        total = 0
        for element in doc:
            if isinstance(element, dict) and field in element:
                value = element[field]
                if isinstance(value, int) and not isinstance(value, bool):
                    total += value
        w_out(str(total))
        return 0

    if command == "compact":
        if len(argv) != 2:
            return fail_msg("usage: jsontool compact <file>")
        w_out(compact(doc))
        return 0

    fail_msg(USAGE)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
