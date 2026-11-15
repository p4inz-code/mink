#!/usr/bin/env python3
"""Reference model for the M2 networking workload.

The numbers are derived from the same contract the MINK app implements and the
fixture server honours (see server.py): the TCP port echoes every byte, the
UDP port echoes each datagram, /data answers with a fixed 13-byte body, and a
closed port refuses the connection.
"""
import hashlib
import sys

BULK = 1048576
UDP_SIZE = 8192
HTTP_BODY = b"mink-http-ok\n"
UDP_PAYLOAD = bytes(65 + (i % 26) for i in range(UDP_SIZE))


def w(line):
    sys.stdout.buffer.write((line + "\r\n").encode())


def e(line):
    sys.stderr.buffer.write(line.encode())


def fail(msg):
    e("netstress: " + msg + "\r\n")
    return 2


def cmd_bulk(port):
    payload = b"Z" * BULK
    w("sent=%d" % BULK)
    w("recv=%d" % BULK)
    w("recv_sha=" + hashlib.sha256(payload).hexdigest())
    return 0


def cmd_repeat(port, n):
    if n <= 0 or n > 10000:
        return fail("repeat: count must be 1..10000")
    w("rounds=%d" % n)
    w("last_reply=4")
    return 0


def cmd_udp(port):
    w("sent=%d" % UDP_SIZE)
    w("recv=%d" % UDP_SIZE)
    w("recv_sha=" + hashlib.sha256(UDP_PAYLOAD).hexdigest())
    return 0


def cmd_http(port):
    w("status=200")
    w("body_len=%d" % len(HTTP_BODY))
    w("body_sha=" + hashlib.sha256(HTTP_BODY).hexdigest())
    return 0


def cmd_refused(port):
    w("dial=-1")
    return 0


def cmd_timeout():
    w("connect=-1")
    w("bounded=1")
    return 0


def main(argv):
    if not argv:
        return fail("usage: netstress <bulk|repeat|udp|http|refused|timeout> ...")
    if argv[0].startswith("-"):
        return fail("this tool has no options")
    cmd = argv[0]
    if cmd == "timeout":
        return cmd_timeout()
    if len(argv) < 2:
        return fail("missing port")
    try:
        port = int(argv[1])
    except ValueError:
        return fail("bad port")
    if port <= 0 or port > 65535:
        return fail("bad port")
    if cmd == "bulk":
        return cmd_bulk(port)
    if cmd == "repeat":
        if len(argv) < 3:
            return fail("repeat needs a count")
        try:
            n = int(argv[2])
        except ValueError:
            return fail("bad count")
        return cmd_repeat(port, n)
    if cmd == "udp":
        return cmd_udp(port)
    if cmd == "http":
        return cmd_http(port)
    if cmd == "refused":
        return cmd_refused(port)
    return fail("unknown command")


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
