#!/usr/bin/env python3
"""Create the deterministic M4 W3 `dbshell` fixture scripts.

Run from the repository root:

    python benchmarks/m4/fixtures/w3_sqlite/make_fixtures.py

Every script is one SQL statement per line with LF terminators (except the
explicit CRLF case). The bulk rows are one INSERT per row because MINK's
engine has no multi-row INSERT; the non-bulk operations match the M4 contract.
"""
import os

HERE = os.path.dirname(os.path.abspath(__file__))

SCHEMA = "CREATE TABLE items (sku INT, name STR, qty INT, category STR)"


def insert_rows(count):
    lines = []
    for i in range(1, count + 1):
        lines.append(
            "INSERT INTO items VALUES (%d, 'item-%03d', %d, 'cat-%d')"
            % (1000 + i, i, (i * 7) % 97, i % 5)
        )
    return lines


def workload(count):
    """The W3 baseline/scale shape: schema, rows, then 13 non-bulk statements."""
    lines = [SCHEMA]
    lines += insert_rows(count)
    lines += [
        "SELECT COUNT(*) FROM items",
        "SELECT * FROM items WHERE category = 'cat-1'",
        "UPDATE items SET qty = 0 WHERE sku = 1001",
        "SELECT * FROM items WHERE sku = 1001",
        "DELETE FROM items WHERE sku = 1002",
        "SELECT COUNT(*) FROM items WHERE qty < 50",
        "BEGIN",
        "INSERT INTO items VALUES (1101, 'tx-item', 7, 'cat-2')",
        "COMMIT",
        "BEGIN",
        "INSERT INTO items VALUES (1102, 'rb-item', 8, 'cat-3')",
        "ROLLBACK",
        "SELECT COUNT(*) FROM items",
    ]
    return lines


def probe(count):
    lines = [SCHEMA]
    lines += insert_rows(count)
    lines.append("SELECT COUNT(*) FROM items")
    return lines


FIXTURES = {
    "baseline_100.sql": workload(100),
    "scale_200.sql": workload(200),
    "probe_500.sql": probe(500),
    "clean.sql": [
        "CREATE TABLE notes (id INT, body STR, weight INT, tag STR)",
        "INSERT INTO notes VALUES (1, 'alpha', 10, 'x')",
        "INSERT INTO notes VALUES (2, 'beta', 20, 'y')",
        "INSERT INTO notes VALUES (3, 'gamma', 30, 'x')",
        "SELECT COUNT(*) FROM notes",
        "SELECT * FROM notes WHERE tag = 'x'",
        "UPDATE notes SET weight = 99 WHERE id = 2",
        "SELECT * FROM notes WHERE id = 2",
        "DELETE FROM notes WHERE id = 3",
        "SELECT COUNT(*) FROM notes",
        "BEGIN",
        "INSERT INTO notes VALUES (4, 'delta', 40, 'z')",
        "COMMIT",
        "SELECT COUNT(*) FROM notes",
    ],
    "error_stops.sql": [
        "CREATE TABLE t (a INT, b STR)",
        "INSERT INTO t VALUES (1, 'one')",
        "SELECT * FROM missing",
        "INSERT INTO t VALUES (2, 'two')",
        "SELECT COUNT(*) FROM t",
    ],
    "comments.sql": [
        "-- a comment line",
        "",
        "-- another comment",
        "CREATE TABLE c (x INT, y STR)",
        "INSERT INTO c VALUES (1, 'a')",
        "SELECT COUNT(*) FROM c",
    ],
    "empty.sql": [],
}


def main():
    written = 0
    for name, lines in FIXTURES.items():
        text = "\n".join(lines)
        if lines:
            text += "\n"
        with open(os.path.join(HERE, name), "wb") as handle:
            handle.write(text.encode("utf-8"))
        written += 1

    # A CRLF copy of the clean script: the runner trims CR, so results match.
    with open(os.path.join(HERE, "clean.sql"), "rb") as handle:
        clean = handle.read().decode("utf-8")
    with open(os.path.join(HERE, "crlf.sql"), "wb") as handle:
        handle.write(clean.replace("\n", "\r\n").encode("utf-8"))
    written += 1

    print("fixtures: %d files under %s" % (written, HERE))
    for name in sorted(list(FIXTURES) + ["crlf.sql"]):
        path = os.path.join(HERE, name)
        with open(path, "rb") as handle:
            print("  %-18s %6d bytes" % (name, len(handle.read())))


if __name__ == "__main__":
    main()
