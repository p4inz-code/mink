#!/usr/bin/env python3
"""Python reference implementation of the D `dbshell` utility.

Uses the real `sqlite3` module and re-renders its results in the format of the
MINK stdlib sqlite engine, so the two transcripts can be compared byte-for-byte
(see record.md):

    OK            successful write statement (CREATE / INSERT / BEGIN / ...)
    OK <n>        UPDATE / DELETE (rows affected)
    <n>           SELECT COUNT(*)
    v1\tv2\n      SELECT * (tab-separated, NULL as `NULL`, one LF per row)
    ERROR: ...    a statement the engine rejected

Deliberate mirrors of MINK conventions:
  * `rt_print_str` appends CRLF without translating embedded newlines, so each
    echoed line and each result block is written with an explicit `b"\\r\\n"`;
  * `rt_argc`/`rt_argv` exclude the program name; any argument starting with
    `-` is a usage error (the tool has no options);
  * statements are applied immediately unless inside an explicit
    BEGIN/COMMIT/ROLLBACK (`isolation_level = None`), matching the MINK
    engine's in-memory text blob;
  * the run stops at the first failing statement.
"""
import os
import sqlite3
import sys


def w_err(text: str) -> None:
    sys.stderr.buffer.write(text.encode("utf-8") + b"\r\n")


def w_out(text: str) -> None:
    sys.stdout.buffer.write(text.encode("utf-8") + b"\r\n")


def fail_msg(label: str) -> int:
    w_err("dbshell: " + label)
    return 1


def normalize_error(exc: sqlite3.Error) -> str:
    """Map a sqlite3 message onto the MINK engine's canonical message."""
    msg = str(exc)
    if "no such table" in msg:
        return "no such table"
    if "no such column" in msg:
        return "unknown column in WHERE"
    return msg


def render(cur: sqlite3.Cursor, statement: str) -> str:
    kind = statement.split(None, 1)[0].upper() if statement else ""
    if kind == "SELECT":
        if "COUNT(" in statement.upper():
            row = cur.fetchone()
            return str(row[0]) if row is not None else "0"
        chunks = []
        for row in cur.fetchall():
            cells = ["NULL" if value is None else str(value) for value in row]
            chunks.append("\t".join(cells) + "\n")
        return "".join(chunks)
    if kind in ("UPDATE", "DELETE"):
        return "OK " + str(cur.rowcount)
    return "OK"


def main(argv) -> int:
    for arg in argv:
        if arg.startswith("-"):
            fail_msg("this tool has no options")
            return 1
    if len(argv) != 1:
        fail_msg("usage: dbshell <script>")
        return 1

    path = argv[0]
    if not os.path.exists(path):
        w_err("dbshell: script does not exist: " + path)
        return 2
    if not os.path.isfile(path):
        w_err("dbshell: not a regular file: " + path)
        return 2

    with open(path, "rb") as handle:
        text = handle.read().decode("utf-8")
    lines = text.split("\n")

    conn = sqlite3.connect(":memory:")
    conn.isolation_level = None  # explicit BEGIN/COMMIT/ROLLBACK only
    cur = conn.cursor()

    code = 0
    for raw in lines:
        line = raw.strip()
        if line == "" or line.startswith("--"):
            continue
        w_out("sql> " + line)
        try:
            cur.execute(line)
            result = render(cur, line)
        except sqlite3.Error as exc:
            result = "ERROR: " + normalize_error(exc)
            code = 3
        w_out(result)
        if code == 3:
            break

    conn.close()
    return code


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
