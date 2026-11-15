#!/usr/bin/env python3
"""Python reference implementation of the M2 `bigio` utility.

Mirrors `validation/m2_bigio/main.mink` byte-for-byte:

    bigio gen   <path> <mib> <pattern>
    bigio scan  <path>
    bigio copy  <src> <dst>
    bigio stdin
    bigio grow  <steps>
    bigio cycles <n>
    bigio json  <path> <field>
    bigio clean <path>

Conventions mirrored from MINK:
  * every stdout/stderr line ends with CRLF (`rt_print_str`/`rt_stderr_write`);
  * `rt_argv` excludes the program name, and any argument starting with `-`
    is a usage error;
  * exit codes: 1 usage, 2 file error, 3 bad JSON.
"""
import hashlib
import json
import os
import sys

USAGE = "usage: bigio <gen|scan|copy|stdin|grow|cycles|json|clean> ..."


def w_line(text: str) -> None:
    sys.stdout.buffer.write(text.encode("utf-8") + b"\r\n")


def w_err(text: str) -> None:
    sys.stderr.buffer.write(text.encode("utf-8") + b"\r\n")


def fail(label: str, code: int) -> int:
    w_err("bigio: " + label)
    return code


def report_bytes(data: bytes) -> None:
    lines = 0
    digits = 0
    for i, b in enumerate(data):
        if b == 10:
            lines += 1
        if 48 <= b <= 57:
            digits = (digits + (b - 48)) % 1000003
    w_line("bytes=" + str(len(data)))
    w_line("lines=" + str(lines))
    w_line("digits=" + str(digits))
    w_line("sha256=" + hashlib.sha256(data).hexdigest())


def cmd_gen(path: str, mib: int, pattern: str) -> int:
    target = mib * 1024 * 1024
    if not pattern or target <= 0:
        return fail("usage: bigio gen <path> <mib> <pattern>", 1)
    pat = pattern.encode("utf-8")
    body = (pat * (target // len(pat) + 1))[:target]
    with open(path, "wb") as fh:
        fh.write(body)
    w_line("wrote=" + str(target))
    w_line("size=" + str(os.path.getsize(path)))
    return 0


def cmd_scan(path: str) -> int:
    if not os.path.exists(path):
        return fail("file does not exist", 2)
    if not os.path.isfile(path):
        return fail("not a regular file", 2)
    with open(path, "rb") as fh:
        data = fh.read()
    report_bytes(data)
    return 0


def cmd_copy(src: str, dst: str) -> int:
    if not os.path.exists(src):
        return fail("file does not exist", 2)
    if not os.path.isfile(src):
        return fail("not a regular file", 2)
    with open(src, "rb") as fh:
        data = fh.read()
    with open(dst, "wb") as fh:
        fh.write(data)
    w_line("wrote=" + str(len(data)))
    w_line("size=" + str(os.path.getsize(dst)))
    w_line("dst-sha256=" + hashlib.sha256(data).hexdigest())
    return 0


def cmd_stdin() -> int:
    report_bytes(sys.stdin.buffer.read())
    return 0


def cmd_grow(steps: int) -> int:
    if steps <= 0 or steps > 24:
        return fail("usage: bigio grow <1..24>", 1)
    s = b"0123456789abcdef"
    for _ in range(steps):
        s = s + s
    w_line("len=" + str(len(s)))
    w_line("sha256=" + hashlib.sha256(s).hexdigest())
    return 0


def cmd_cycles(n: int) -> int:
    if n <= 0 or n > 200000:
        return fail("usage: bigio cycles <1..200000>", 1)
    chunk = b"0123456789abcdef" * 4096
    checksum = 0
    for i in range(n):
        checksum = (checksum + chunk[i % 65536]) % 1000003
    w_line("cycles=" + str(n))
    w_line("checksum=" + str(checksum))
    return 0


def cmd_json(path: str, field: str) -> int:
    if not os.path.exists(path):
        return fail("file does not exist", 2)
    if not os.path.isfile(path):
        return fail("not a regular file", 2)
    with open(path, "rb") as fh:
        raw = fh.read()
    try:
        doc = json.loads(raw.decode("utf-8"))
    except (ValueError, UnicodeDecodeError):
        return fail("invalid JSON document", 3)
    if not isinstance(doc, list):
        return fail("root value is not an array", 3)
    total = 0
    found = 0
    for item in doc:
        if isinstance(item, dict):
            if field in item:
                found += 1
                total += int(item[field])
    w_line("records=" + str(len(doc)))
    w_line("matches=" + str(found))
    w_line("sum=" + str(total))
    return 0


def cmd_clean(path: str) -> int:
    if not os.path.exists(path):
        return fail("file does not exist", 2)
    os.remove(path)
    w_line("removed=0")
    return 0


def main() -> int:
    args = sys.argv[1:]
    pos = []
    bad = False
    for arg in args:
        if arg.startswith("-"):
            bad = True
        else:
            pos.append(arg)

    if bad:
        return fail("this tool has no options", 1)
    if not pos:
        return fail(USAGE, 1)

    cmd, rest = pos[0], pos[1:]
    if cmd == "gen":
        if len(rest) != 3:
            return fail("usage: bigio gen <path> <mib> <pattern>", 1)
        try:
            mib = int(rest[1])
        except ValueError:
            mib = 0
        return cmd_gen(rest[0], mib, rest[2])
    if cmd == "scan":
        if len(rest) != 1:
            return fail("usage: bigio scan <path>", 1)
        return cmd_scan(rest[0])
    if cmd == "copy":
        if len(rest) != 2:
            return fail("usage: bigio copy <src> <dst>", 1)
        return cmd_copy(rest[0], rest[1])
    if cmd == "stdin":
        if rest:
            return fail("usage: bigio stdin", 1)
        return cmd_stdin()
    if cmd == "grow":
        if len(rest) != 1:
            return fail("usage: bigio grow <1..24>", 1)
        try:
            steps = int(rest[0])
        except ValueError:
            steps = 0
        return cmd_grow(steps)
    if cmd == "cycles":
        if len(rest) != 1:
            return fail("usage: bigio cycles <1..200000>", 1)
        try:
            n = int(rest[0])
        except ValueError:
            n = 0
        return cmd_cycles(n)
    if cmd == "json":
        if len(rest) != 2:
            return fail("usage: bigio json <path> <field>", 1)
        return cmd_json(rest[0], rest[1])
    if cmd == "clean":
        if len(rest) != 1:
            return fail("usage: bigio clean <path>", 1)
        return cmd_clean(rest[0])
    return fail("unknown command", 1)


if __name__ == "__main__":
    sys.exit(main())
