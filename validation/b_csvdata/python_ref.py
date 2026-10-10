#!/usr/bin/env python3
"""Python reference implementation of the B `csvdata` utility.

Mirrors `validation/b_csvdata/main.mink` byte-for-byte:

    csvdata <select|sort|agg|write> <file> [args]

Deliberate mirrors of MINK conventions (see record.md):
  * records are decoded/encoded with the stdlib CSV layer (comma, double
    quote, doubled-quote escape, CR/LF/CRLF terminators, embedded newlines
    inside quotes are data);
  * blank records are skipped, and the first non-blank record is the header
    that defines the column count; any other field count is a ragged-row
    error (exit 4);
  * `rt_print_str` appends CRLF without translating embedded newlines, so
    re-encoded rows are written as `writerow` text + `b"\\r\\n"`;
  * `rt_argc`/`rt_argv` exclude the program name; any argument starting with
    `-` is a usage error (the tool has no options);
  * there is no exception type: errors are status codes and stderr messages.
"""
import csv
import io
import os
import sys

USAGE = "usage: csvdata <select|sort|agg|write> <file> [args]"


def w_err(text: str) -> None:
    sys.stderr.buffer.write(text.encode("utf-8") + b"\r\n")


def fail_file(label: str, path: str) -> None:
    w_err("csvdata: " + label + path)


def fail_msg(label: str) -> None:
    w_err("csvdata: " + label)


def parse_int(text: str):
    """Mirror `str_parse_int` for the option subset: signed integers only."""
    value = text.strip()
    if not value:
        return None
    body = value[1:] if value[0] in "+-" else value
    if not body or not body.isdigit():
        return None
    try:
        return int(value, 10)
    except ValueError:
        return None


def load_rows(path: str):
    """Return (rows, status): rows is a list of field lists, status 0 ok."""
    if not os.path.exists(path):
        fail_file("input file does not exist: ", path)
        return None, 2
    if not os.path.isfile(path):
        fail_file("not a regular file: ", path)
        return None, 2
    with open(path, "r", encoding="utf-8", newline="") as handle:
        rows = [row for row in csv.reader(handle) if len(row) != 0]
    if not rows:
        return [], 0
    cols = len(rows[0])
    for row in rows[1:]:
        if len(row) != cols:
            fail_msg("ragged row: field count differs from the header")
            return None, 4
    return rows, 0


def encode_row(row) -> bytes:
    buffer = io.StringIO()
    writer = csv.writer(buffer, lineterminator="\r\n")
    writer.writerow(row)
    return buffer.getvalue().encode("utf-8")


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

    if command == "select":
        if len(argv) != 4:
            fail_msg("usage: csvdata select <file> <col> <substr>")
            return 1
        col = parse_int(argv[2])
        if col is None:
            fail_msg("column index must be an integer")
            return 1
        if col < 0:
            fail_msg("column index must not be negative")
            return 1
        rows, status = load_rows(path)
        if status != 0:
            return status
        if not rows:
            return 3
        cols = len(rows[0])
        if col >= cols:
            fail_file("column out of range for: ", path)
            return 4
        if len(rows) <= 1:
            return 3
        needle = argv[3]
        found = 0
        out = sys.stdout.buffer
        for row in rows[1:]:
            if needle in row[col]:
                out.write(encode_row(row))
                found += 1
        out.flush()
        return 0 if found > 0 else 3

    if command == "sort":
        if len(argv) != 3:
            fail_msg("usage: csvdata sort <file> <col>")
            return 1
        col = parse_int(argv[2])
        if col is None:
            fail_msg("column index must be an integer")
            return 1
        if col < 0:
            fail_msg("column index must not be negative")
            return 1
        rows, status = load_rows(path)
        if status != 0:
            return status
        if not rows:
            return 3
        cols = len(rows[0])
        if col >= cols:
            fail_file("column out of range for: ", path)
            return 4
        if len(rows) <= 1:
            return 3
        out = sys.stdout.buffer
        for row in sorted(rows[1:], key=lambda r: r[col]):
            out.write(encode_row(row))
        out.flush()
        return 0

    if command == "agg":
        if len(argv) != 4:
            fail_msg("usage: csvdata agg <file> <group-col> <value-col>")
            return 1
        gcol = parse_int(argv[2])
        vcol = parse_int(argv[3])
        if gcol is None or vcol is None:
            fail_msg("column index must be an integer")
            return 1
        if gcol < 0:
            fail_msg("column index must not be negative")
            return 1
        if vcol < 0:
            fail_msg("column index must not be negative")
            return 1
        rows, status = load_rows(path)
        if status != 0:
            return status
        if not rows:
            return 3
        cols = len(rows[0])
        if gcol >= cols or vcol >= cols:
            fail_file("column out of range for: ", path)
            return 4
        if len(rows) <= 1:
            return 3
        totals = {}
        for row in rows[1:]:
            value = parse_int(row[vcol])
            if value is None:
                value = 0
            totals[row[gcol]] = totals.get(row[gcol], 0) + value
        out = sys.stdout.buffer
        for key in sorted(totals):
            out.write(f"{key}:{totals[key]}".encode("utf-8") + b"\r\n")
        out.flush()
        return 0 if totals else 3

    if command == "write":
        if len(argv) != 3:
            fail_msg("usage: csvdata write <file> <out>")
            return 1
        rows, status = load_rows(path)
        if status != 0:
            return status
        if not rows:
            return 3
        body = bytearray()
        for row in rows:
            body.extend(encode_row(row))
        try:
            with open(argv[2], "wb") as handle:
                handle.write(bytes(body))
        except OSError:
            fail_file("cannot write output file: ", argv[2])
            return 5
        return 0

    fail_msg(USAGE)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
