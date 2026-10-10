"""Python reference for the A1 fileinspect validation.

Same behaviour as validation/a1_fileinspect/main.mink so the two can be
compared field by field:

  fileinspect.py <path> [--grep PATTERN] [--max-bytes N] [--sha] [--lines]

Exit codes: 0 ok, 1 usage, 2 missing, 3 not a regular file, 4 over window,
5 short read.
"""
import hashlib
import os
import re
import sys

# Compare byte-for-byte with the MINK build: Python's text-mode streams would
# translate every "\n" into "\r\n" on Windows, MINK's rt_print_str does not
# (it appends CRLF only at the end of the string it is given).
try:
    sys.stdout.reconfigure(newline="")
    sys.stderr.reconfigure(newline="")
except AttributeError:
    pass

DEFAULT_MAX_BYTES = 1048576
USAGE = (
    "usage: fileinspect <path> [--grep PATTERN] [--max-bytes N] [--sha] [--lines]\r\n"
    "  FILEINSPECT_MAX_BYTES sets the default inspection window\r\n"
)


def emit(label, value, stream=sys.stdout):
    # rt_print_str appends CRLF to every string it writes, so each report
    # line is terminated the same way here.
    stream.write("%s: %s\r\n" % (label, value))


def fail_msg(label, detail=""):
    sys.stderr.write("fileinspect: %s%s\r\n" % (label, detail))


def main(argv):
    if len(argv) < 1:
        sys.stdout.write(USAGE)
        return 1

    max_bytes = DEFAULT_MAX_BYTES
    max_src = "default"
    want_sha = want_lines = False
    path = None
    pattern = None

    env = os.environ.get("FILEINSPECT_MAX_BYTES")
    if env:
        try:
            value = int(env.strip())
            if value > 0:
                max_bytes, max_src = value, "FILEINSPECT_MAX_BYTES"
        except ValueError:
            pass

    i = 0
    while i < len(argv):
        arg = argv[i]
        if arg == "--grep":
            if i + 1 >= len(argv):
                fail_msg("missing value for an option")
                sys.stdout.write(USAGE)
                return 1
            pattern = argv[i + 1]
            i += 2
        elif arg == "--max-bytes":
            if i + 1 >= len(argv):
                fail_msg("missing value for an option")
                sys.stdout.write(USAGE)
                return 1
            try:
                value = int(argv[i + 1])
            except ValueError:
                fail_msg("--max-bytes", " expects a positive integer")
                sys.stdout.write(USAGE)
                return 1
            if value > 0:
                max_bytes, max_src = value, "--max-bytes"
            i += 2
        elif arg == "--sha":
            want_sha = True
            i += 1
        elif arg == "--lines":
            want_lines = True
            i += 1
        elif arg == "--help":
            sys.stdout.write(USAGE)
            return 1
        elif path is None:
            path = arg
            i += 1
        else:
            fail_msg("only one path may be given")
            sys.stdout.write(USAGE)
            return 1

    if path is None:
        fail_msg("no path given")
        sys.stdout.write(USAGE)
        return 1

    if not os.path.exists(path):
        fail_msg("no such file: %s" % path)
        return 2
    if os.path.isdir(path):
        fail_msg("not a regular file: %s" % path)
        return 3

    size = os.path.getsize(path)
    if size > max_bytes:
        fail_msg(
            "refusing to inspect  is %d bytes over the inspection window of %d bytes; "
            "raise it with --max-bytes\n" % (size - max_bytes, max_bytes)
        )
        return 4

    with open(path, "rb") as fh:
        data = fh.read()
    if len(data) != size:
        fail_msg("short read: %d bytes but the file reports %d" % (len(data), size))
        return 5

    emit("path", path)
    emit("size", size)
    emit("extension", os.path.splitext(path)[1])
    emit("name", path[: len(path) - len(os.path.splitext(path)[1])])
    emit("window", max_bytes)
    emit("window_source", max_src)

    if want_sha:
        emit("sha256", hashlib.sha256(data).hexdigest())
    if want_lines:
        text = data.decode("utf-8", "replace")
        lines = text.split("\n")
        if lines and lines[-1] == "":
            lines.pop()
        emit("lines", len(lines))
        # The preview keeps the file's own bytes (CRLF stays CRLF), like the
        # MINK version, which slices the raw buffer.
        take = 0
        cut = 0
        pos = 0
        while take < 5 and pos < len(data):
            if data[pos : pos + 1] == b"\n":
                take += 1
                cut = pos
            pos += 1
        if pos >= len(data):
            cut = len(data)
        emit("preview", data[:cut].decode("utf-8", "replace"))
    if pattern is not None:
        text = data.decode("utf-8", "replace")
        found = re.findall(pattern, text)
        emit("grep_matches", len(found))
        # Anchored match at the start of the text (MINK `re_match_end`; note
        # that MINK `re_is_match` is search-anywhere, i.e. Python `re.search`).
        emit("grep_at_start", "yes" if re.match(pattern, text) else "no")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
