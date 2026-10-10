"""Deterministic fixtures for the A1 fileinspect validation.

Writes a small log file, a CRLF file, a file with no trailing newline, a
UTF-8 file, an empty file and a 3 MiB file (for the window test).
"""
import os

HERE = os.path.dirname(os.path.abspath(__file__))


def write(name, data):
    with open(os.path.join(HERE, name), "wb") as fh:
        fh.write(data)
    return os.path.join(HERE, name)


log = b"".join(
    b"2026-10-01T00:00:%02dZ INFO  worker started pid=%d\n" % (i % 60, 1000 + i)
    for i in range(20)
)
log += b"2026-10-01T00:00:59Z ERROR disk full on /dev/sda1\n"

write("app.log", log)
write("crlf.txt", b"alpha\r\nbeta\r\ngamma\r\n")
write("no_trailing_newline.txt", b"one\ntwo\nthree")
write("utf8.txt", "héllo wörld — ünïcode ✓\n".encode("utf-8"))
write("empty.txt", b"")

# 3 MiB payload for the inspection-window test.
big = b"".join(b"line %08d padding padding padding\n" % i for i in range(90000))
write("big.txt", big)
print("fixtures:", len(os.listdir(HERE)), "entries")
