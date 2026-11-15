#!/usr/bin/env python3
"""Reference model for the M2 process-automation workload.

The reference does not spawn the MINK helper: it reproduces the same numbers
from the documented contract, so agreement means the MINK run really captured
what it claims.

Documented contract reproduced here:
  * `child emit <n> <code>` writes `n` copies of `x`, then the CRLF that
    `rt_print_str` appends, writes `ERR` to stderr and exits with `<code>`;
  * `process_run_input` delivers at most 4096 bytes of standard input
    (`STDIN_PIPE_LIMIT`), so the echo child receives `min(n, 4096)` bytes and
    prints them plus one CRLF;
  * a command that cannot be launched reports exit code 1 with empty stdout.
"""
import hashlib
import sys


def w(line):
    # Binary streams: the reference must emit the same bytes (CRLF) on every
    # platform, without text-mode newline translation.
    sys.stdout.buffer.write((line + "\r\n").encode())


def e(line):
    sys.stderr.buffer.write(line.encode())


CHILD_STDERR = b"ERR"
PIPE_LIMIT = 4096


def cmd_capture():
    out = b"x" * 65536 + b"\r\n"
    w("code=7")
    w("out_len=%d" % len(out))
    w("out_sha=" + hashlib.sha256(out).hexdigest())
    w("err_len=%d" % len(CHILD_STDERR))
    w("err_sha=" + hashlib.sha256(CHILD_STDERR).hexdigest())
    return 0


def cmd_feed():
    delivered = min(5000, PIPE_LIMIT)
    out = b"a" * delivered + b"\r\n"
    w("code=0")
    w("echo_len=%d" % len(out))
    w("echo_sha=" + hashlib.sha256(out).hexdigest())
    return 0


def cmd_many():
    w("runs=50")
    w("bytes=%d" % (50 * 18))
    w("last=18")
    return 0


def cmd_codes():
    w("code=42")
    w("ok_zero=true")
    w("ok_fail=false")
    return 0


def cmd_missing():
    w("code=1")
    w("out_len=0")
    return 0


def main(argv):
    if not argv:
        e("procauto: usage: procauto <capture|feed|many|codes|missing>")
        return 1
    if argv[0].startswith("-"):
        e("procauto: this tool has no options")
        return 1
    table = {
        "capture": cmd_capture,
        "feed": cmd_feed,
        "many": cmd_many,
        "codes": cmd_codes,
        "missing": cmd_missing,
    }
    fn = table.get(argv[0])
    if fn is None:
        e("procauto: unknown command")
        return 1
    return fn()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
