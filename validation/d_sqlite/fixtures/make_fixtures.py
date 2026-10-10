#!/usr/bin/env python3
"""Create the D `dbshell` fixtures.

Run from anywhere:

    python validation/d_sqlite/fixtures/make_fixtures.py
"""
import os

HERE = os.path.dirname(os.path.abspath(__file__))

# The workload, one statement per line. The trailing two statements trigger
# the first error, which stops the run.
WORKLOAD = [
    "-- schema",
    "CREATE TABLE items (sku INT, name STR, qty INT)",
    "INSERT INTO items VALUES (1001, 'bolt', 240)",
    "INSERT INTO items VALUES (1002, 'nut', 12)",
    "INSERT INTO items (sku, name) VALUES (1003, 'washer')",
    "-- queries",
    "SELECT * FROM items",
    "SELECT COUNT(*) FROM items",
    "SELECT COUNT(*) FROM items WHERE qty = 12",
    "SELECT * FROM items WHERE qty < 50",
    "SELECT * FROM items WHERE name = 'nut'",
    "-- update then delete",
    "UPDATE items SET qty = 0 WHERE sku = 1001",
    "SELECT * FROM items WHERE qty = 0",
    "DELETE FROM items WHERE sku = 1002",
    "SELECT COUNT(*) FROM items",
    "-- a committed transaction survives",
    "BEGIN",
    "INSERT INTO items VALUES (1004, 'screw', 80)",
    "COMMIT",
    "SELECT COUNT(*) FROM items",
    "-- a rolled-back transaction leaves no trace",
    "BEGIN",
    "INSERT INTO items VALUES (1005, 'ghost', 5)",
    "ROLLBACK",
    "SELECT COUNT(*) FROM items",
    "-- the first error stops the run",
    "SELECT * FROM missing",
    "SELECT COUNT(*) FROM items",
]

ERROR_AT = "SELECT * FROM missing"
CLEAN = WORKLOAD[:WORKLOAD.index(ERROR_AT)]


def write(name: str, text: str) -> None:
    with open(os.path.join(HERE, name), "wb") as handle:
        handle.write(text.encode("utf-8"))


def main() -> None:
    write("inventory.sql", "\n".join(WORKLOAD) + "\n")
    write("clean.sql", "\n".join(CLEAN) + "\n")
    # CRLF line endings must behave identically (the runner trims CR).
    write("crlf.sql", "\r\n".join(CLEAN) + "\r\n")
    write("comments.sql", "-- a comment\n\n-- another\n")
    write("empty.sql", "")
    print(f"fixtures: 5 files under {HERE}")


if __name__ == "__main__":
    main()
