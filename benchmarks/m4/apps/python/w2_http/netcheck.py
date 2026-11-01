#!/usr/bin/env python3
"""M4 W2 `netcheck` — Python 3.11 port.

Contract: `docs/benchmarks/M4/M4_SESSION_153.md`.

    netcheck <host> <port> <path>
"""
import http.client
import json
import sys

USAGE = "usage: netcheck <host> <port> <path>"


def write_out(text):
    sys.stdout.buffer.write(text.encode("utf-8") + b"\r\n")


def write_err(text):
    sys.stderr.buffer.write(text.encode("utf-8") + b"\r\n")


def fail(label, code):
    write_err("netcheck: " + label)
    return code


def fetch(host, port, path):
    connection = http.client.HTTPConnection(host, port, timeout=10)
    try:
        connection.request("GET", path)
        response = connection.getresponse()
        body = response.read()
        return response.status, body
    finally:
        connection.close()


def integral_value(body):
    """Return (parsed_ok, value_or_None) for a JSON object with `value`."""
    try:
        document = json.loads(body.decode("utf-8"))
    except (ValueError, UnicodeDecodeError):
        return False, None
    if isinstance(document, dict):
        value = document.get("value")
        if isinstance(value, int) and not isinstance(value, bool):
            return True, value
    return True, None


def main(argv):
    for arg in argv:
        if arg.startswith("-"):
            return fail("this tool has no options", 1)
    if len(argv) != 3:
        return fail(USAGE, 1)

    host, port_text, path = argv
    try:
        port = int(port_text)
    except ValueError:
        return fail(USAGE, 1)
    if port < 1 or port > 65535:
        return fail(USAGE, 1)

    try:
        status, body = fetch(host, port, path)
    except (OSError, http.client.HTTPException):
        return fail("connection failed", 2)

    parsed, value = integral_value(body)
    if status < 400:
        if not parsed:
            return fail("malformed JSON", 3)
        if value is None:
            return fail("missing value", 3)

    write_out("STATUS " + str(status))
    write_out("BODY " + str(len(body)))
    if value is not None:
        write_out("VALUE " + str(value))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
