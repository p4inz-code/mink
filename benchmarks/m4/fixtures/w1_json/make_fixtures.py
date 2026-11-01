#!/usr/bin/env python3
"""Create the deterministic M4 W1 `jsontool` fixtures.

Run from the repository root:

    python benchmarks/m4/fixtures/w1_json/make_fixtures.py

Every fixture is written with exact bytes: compact JSON, no trailing newline.
This mirrors the M1 `validation/c_json` fixture convention and keeps the
golden bytes independent of the host's text-mode newline translation.

The fixture set is fixed. `records.json` is the baseline-size document
(~2.7 KiB, inside MINK's documented ~3.5 KiB JSON envelope); the 4 KiB/5 KiB
boundary probes called out in M4_WORKLOAD_SELECTION.md are deliberately not
created here.
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))

# A config-like object: string, integer, array, bool, null.
OBJECT = {
    "name": "widget",
    "qty": 12,
    "tags": ["a", "b"],
    "active": True,
    "meta": None,
}

# An array of objects for the `sum` command.
ARRAY = [
    {"id": 1, "qty": 3},
    {"id": 2, "qty": 5},
    {"id": 3, "qty": 7},
]

NESTED = {
    "outer": {"inner": [1, 2, 3]},
    "list": [{"v": 10}, {"v": 20}],
}

# Whitespace-laden input for the `compact` command.
SPACED = {
    "service": "gateway",
    "enabled": True,
    "retries": 3,
    "hosts": ["alpha", "beta"],
    "limits": {"cpu": 2, "memory": 512},
}

# A 50-record array, the baseline-size document. `qty` cycles deterministically.
RECORDS = [
    {
        "id": i,
        "qty": (i * 7) % 13,
        "name": "item-%02d" % i,
        "tag": "alpha" if i % 2 else "beta",
    }
    for i in range(1, 51)
]

TEXT_FIXTURES = {
    # Invalid documents.
    "broken.json": '{"a": }',
    "trailing.json": '{"a": 1} extra',
    "empty.json": "",
}

JSON_FIXTURES = {
    "object.json": OBJECT,
    "array.json": ARRAY,
    "number.json": 42,
    "string.json": "hello",
    "bool.json": True,
    "null.json": None,
    "nested.json": NESTED,
    "records.json": RECORDS,
    # Raw UTF-8 bytes, not \u escapes: MINK passes multibyte bytes through and
    # only decodes ASCII \u escapes.
    "utf8.json": {"greeting": "caf\u00e9", "city": "Z\u00fcrich"},
}


def dump(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False)


def main():
    written = {}
    for name, value in JSON_FIXTURES.items():
        written[name] = dump(value)
    for name in ("escaped.json",):
        # Escapes are part of the fixture bytes, so they are written literally
        # rather than through json.dumps of a decoded value.
        written[name] = '{"msg":"line1\\nline2 \\"quoted\\" \\\\ back"}'
    # Pretty-printed so `compact` has whitespace to remove.
    written["spaced.json"] = json.dumps(SPACED, indent=2, ensure_ascii=False) + "\n"
    written.update(TEXT_FIXTURES)

    for name, text in sorted(written.items()):
        with open(os.path.join(HERE, name), "wb") as handle:
            handle.write(text.encode("utf-8"))

    print("fixtures: %d files under %s" % (len(written), HERE))
    print("records.json bytes: %d" % len(written["records.json"].encode("utf-8")))
    print("records qty sum: %d" % sum(r["qty"] for r in RECORDS))
    print("spaced compact: " + dump(SPACED))


if __name__ == "__main__":
    main()
