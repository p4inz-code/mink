#!/usr/bin/env python3
"""Author the W3 golden transcripts from the M1-verified renderer.

    python benchmarks/m4/expected/w3_sqlite/make_expected.py

This is an oracle, not one of the five benchmark applications. It applies each
fixture script to Python's `sqlite3` and renders results in the M1/MINK format
(`OK`, `OK <n>`, a bare COUNT, tab-separated rows with LF terminators and
`NULL`). The M1 cross-check (validation/d_sqlite, 10/10) established that this
rendering matches MINK's engine; MINK and C# SQLite then confirm the bytes
independently in the M4 gate.

Generated files: `<case>.stdout` next to this script.
"""
import os
import sqlite3
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
M4_DIR = os.path.dirname(os.path.dirname(HERE))
FIXTURES = os.path.join(M4_DIR, "fixtures", "w3_sqlite")

CASES = [
    ("baseline_100", "baseline_100.sql"),
    ("scale_200", "scale_200.sql"),
    ("probe_500", "probe_500.sql"),
    ("clean", "clean.sql"),
    ("error_stops", "error_stops.sql"),
    ("crlf", "crlf.sql"),
    ("comments", "comments.sql"),
    ("empty", "empty.sql"),
]


def normalize_error(message):
    if "no such table" in message:
        return "no such table"
    if "no such column" in message:
        return "unknown column in WHERE"
    return message


def render(cursor, statement):
    kind = statement.split(None, 1)[0].upper() if statement else ""
    if kind == "SELECT":
        if "COUNT(" in statement.upper():
            row = cursor.fetchone()
            return str(row[0]) if row is not None else "0"
        chunks = []
        for row in cursor.fetchall():
            cells = ["NULL" if value is None else str(value) for value in row]
            chunks.append("\t".join(cells) + "\n")
        return "".join(chunks)
    if kind in ("UPDATE", "DELETE"):
        return "OK " + str(cursor.rowcount)
    return "OK"


def transcript(script):
    connection = sqlite3.connect(":memory:")
    connection.isolation_level = None
    cursor = connection.cursor()
    output = []
    with open(script, "rb") as handle:
        text = handle.read().decode("utf-8")
    for raw in text.split("\n"):
        line = raw.strip()
        if line == "" or line.startswith("--"):
            continue
        output.append("sql> " + line)
        try:
            cursor.execute(line)
            result = render(cursor, line)
        except sqlite3.Error as error:
            result = "ERROR: " + normalize_error(str(error))
        output.append(result)
        if result.startswith("ERROR:"):
            break
    connection.close()
    return "\r\n".join(output) + ("\r\n" if output else "")


def main():
    written = 0
    for case, script in CASES:
        text = transcript(os.path.join(FIXTURES, script))
        path = os.path.join(HERE, case + ".stdout")
        with open(path, "wb") as handle:
            handle.write(text.encode("utf-8"))
        written += 1
        print("  %-14s %7d bytes" % (case, len(text.encode("utf-8"))))
    print("expected: %d transcripts" % written)
    return 0


if __name__ == "__main__":
    sys.exit(main())
