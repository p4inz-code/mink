#!/usr/bin/env python3
"""Python reference implementation of the E `netcheck` utility.

Mirrors `validation/e_network/main.mink` byte-for-byte against the same local
servers (see record.md):

    netcheck http  <host> <port> <path>
    netcheck https <host> <port> <path>
    netcheck tcp   <host> <port> <payload>
    netcheck udp   <host> <port> <payload>

Deliberate mirrors of MINK conventions:
  * `rt_print_str` appends CRLF without translating embedded newlines, so
    every line (and the HTTP body) is written with an explicit `b"\\r\\n"`;
  * `rt_argc`/`rt_argv` exclude the program name; any argument starting with
    `-` is a usage error (the tool has no options);
  * there is no exception type: failures are a message on stderr and exit 2.
"""
import http.client
import socket
import ssl
import sys

USAGE = "usage: netcheck <http|https|tcp|udp> <host> <port> <arg>"
# The explicit trust anchor used by `netcheck https`, mirroring the MINK app.
CA_PATH = "tests/tls/ca.pem"


def w_err(text: str) -> None:
    sys.stderr.buffer.write(text.encode("utf-8") + b"\r\n")


def w_line(text: str) -> None:
    sys.stdout.buffer.write(text.encode("utf-8") + b"\r\n")


def report_response(resp) -> int:
    body = resp.read()
    status = resp.status
    reason = resp.reason
    ctype = resp.getheader("Content-Type") or ""
    raw_clen = resp.getheader("Content-Length")
    clen = int(raw_clen) if raw_clen is not None else -1
    w_line("status=" + str(status))
    w_line("reason=" + reason)
    w_line("content-type=" + ctype)
    w_line("content-length=" + str(clen))
    sys.stdout.buffer.write(b"body=" + body + b"\r\n")
    return 0


def run_http(host: str, port: int, path: str) -> int:
    try:
        conn = http.client.HTTPConnection(host, port, timeout=10)
        conn.request("GET", path)
        resp = conn.getresponse()
    except OSError:
        w_err("netcheck: connection failed")
        return 2
    code = report_response(resp)
    conn.close()
    return code


def run_https(host: str, port: int, path: str) -> int:
    try:
        ctx = ssl.create_default_context(cafile=CA_PATH)
        conn = http.client.HTTPSConnection(host, port, timeout=10, context=ctx)
        conn.request("GET", path)
        resp = conn.getresponse()
    except (OSError, ssl.SSLError):
        w_err("netcheck: connection failed")
        return 2
    code = report_response(resp)
    conn.close()
    return code


def run_tcp(host: str, port: int, payload: str) -> int:
    data = payload.encode("utf-8")
    try:
        sock = socket.create_connection((host, port), timeout=10)
        sock.sendall(data)
        got = sock.recv(4096)
        sock.close()
    except OSError:
        w_err("netcheck: connect failed")
        return 2
    w_line("sent=" + str(len(data)))
    w_line("recv=" + str(len(got)))
    sys.stdout.buffer.write(b"echo=" + got + b"\r\n")
    return 0


def run_udp(host: str, port: int, payload: str) -> int:
    data = payload.encode("utf-8")
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.settimeout(10)
        sock.sendto(data, (host, port))
        got, _addr = sock.recvfrom(4096)
        sock.close()
    except OSError:
        w_err("netcheck: send failed")
        return 2
    w_line("sent=" + str(len(data)))
    w_line("recv=" + str(len(got)))
    sys.stdout.buffer.write(b"echo=" + got + b"\r\n")
    return 0


def main(argv) -> int:
    for arg in argv:
        if arg.startswith("-"):
            w_err("netcheck: this tool has no options")
            return 1
    if len(argv) != 4:
        w_err("netcheck: " + USAGE)
        return 1

    command, host, port_text, arg = argv
    try:
        port = int(port_text)
    except ValueError:
        port = 0
    if port <= 0:
        if command == "http":
            w_err("netcheck: usage: netcheck http <host> <port> <path>")
        elif command == "https":
            w_err("netcheck: usage: netcheck https <host> <port> <path>")
        elif command == "tcp":
            w_err("netcheck: usage: netcheck tcp <host> <port> <payload>")
        elif command == "udp":
            w_err("netcheck: usage: netcheck udp <host> <port> <payload>")
        else:
            w_err("netcheck: " + USAGE)
        return 1

    if command == "http":
        return run_http(host, port, arg)
    if command == "https":
        return run_https(host, port, arg)
    if command == "tcp":
        return run_tcp(host, port, arg)
    if command == "udp":
        return run_udp(host, port, arg)
    w_err("netcheck: " + USAGE)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
