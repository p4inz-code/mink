#!/usr/bin/env python3
"""Create the C `jsontool` fixtures.

Run from anywhere:

    python validation/c_json/fixtures/make_fixtures.py
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))

FILES = {
    # A small config-like object (keys, number, array, bool, null).
    "object.json": {"name": "widget", "qty": 12, "tags": ["a", "b"],
                    "active": True, "meta": None},
    # An array of objects for the `sum` command.
    "array.json": [{"id": 1, "qty": 3}, {"id": 2, "qty": 5},
                   {"id": 3, "qty": 7}],
    # Nested structures.
    "nested.json": {"outer": {"inner": [1, 2, 3]},
                    "list": [{"v": 10}, {"v": 20}]},
    # Scalars at the root.
    "number.json": 42,
    "string.json": "hello",
    "bool.json": True,
    "null.json": None,
    # Empty containers.
    "empty_array.json": [],
    "empty_object.json": {},
    # Escapes in a string value (kept byte-transparent).
    "escaped.json": {"msg": "line1\nline2 \"quoted\" \\ back"},
    # Multibyte UTF-8 value (raw bytes, not \\u escapes).
    "unicode.json": {"greeting": "caf\u00e9"},
    # Invalid documents.
    "broken.json": '{"a": }',
    "trailing.json": '{"a": 1} extra',
    "empty.json": "",
}


def main() -> None:
    for name, value in FILES.items():
        if name in ("broken.json", "trailing.json", "empty.json"):
            text = value
        else:
            # Compact separators so the MINK serializer's compact output can
            # be compared with the Python reference's compact output.
            text = json.dumps(value, separators=(",", ":"), ensure_ascii=False)
        with open(os.path.join(HERE, name), "wb") as handle:
            handle.write(text.encode("utf-8"))
    print(f"fixtures: {len(FILES)} files under {HERE}")


if __name__ == "__main__":
    main()
