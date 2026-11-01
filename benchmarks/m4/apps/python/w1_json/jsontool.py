#!/usr/bin/env python3
"""M4 W1 `jsontool` — Python 3.11 port.

Contract: `docs/benchmarks/M4/M4_SESSION_151.md` (reduced from the M1
`validation/c_json` application).

Deliberate contract details carried over from M1:
  * canonical stdout/stderr is UTF-8 with explicit CRLF;
  * argv excludes the program name, and any `-`-prefixed argument is a usage
    error because the tool has no options;
  * the file is read and parsed before the command is dispatched, so a file
    or JSON error is reported even for an unknown command;
  * `sum` adds integral field values only; a record without the field, or a
    document that is not an array of objects, is handled as the contract says.
"""
import json
import os
import sys

USAGE = "usage: jsontool <validate|type|sum|compact> <file> [args]"


def write_out(text):
    sys.stdout.buffer.write(text.encode("utf-8") + b"\r\n")


def write_err(text):
    sys.stderr.buffer.write(text.encode("utf-8") + b"\r\n")


def fail_usage(text):
    write_err("jsontool: " + text)
    return 1


def fail_file(label, path):
    write_err("jsontool: " + label + path)
    return 2


def type_name(value):
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


def compact(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False)


def main(argv):
    for arg in argv:
        if arg.startswith("-"):
            write_err("jsontool: this tool has no options")
            return 1
    if len(argv) < 2:
        return fail_usage(USAGE)

    command = argv[0]
    path = argv[1]

    if not os.path.exists(path):
        return fail_file("input file does not exist: ", path)
    if not os.path.isfile(path):
        return fail_file("not a regular file: ", path)
    with open(path, "rb") as handle:
        raw = handle.read()
    try:
        doc = json.loads(raw.decode("utf-8"))
    except (ValueError, UnicodeDecodeError):
        write_err("jsontool: invalid JSON")
        return 3

    if command == "validate":
        if len(argv) != 2:
            return fail_usage("usage: jsontool validate <file>")
        write_out("ok")
        return 0

    if command == "type":
        if len(argv) != 2:
            return fail_usage("usage: jsontool type <file>")
        write_out(type_name(doc))
        return 0

    if command == "sum":
        if len(argv) != 3:
            return fail_usage("usage: jsontool sum <file> <field>")
        if not isinstance(doc, list):
            write_err("jsontool: document is not an array")
            return 4
        field = argv[2]
        total = 0
        for element in doc:
            if isinstance(element, dict) and field in element:
                value = element[field]
                if isinstance(value, int) and not isinstance(value, bool):
                    total += value
        write_out(str(total))
        return 0

    if command == "compact":
        if len(argv) != 2:
            return fail_usage("usage: jsontool compact <file>")
        write_out(compact(doc))
        return 0

    return fail_usage(USAGE)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
