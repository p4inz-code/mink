#!/usr/bin/env python3
"""E cross-check harness: start local HTTP / TCP / UDP servers, then run the
MINK build and the Python reference against them and compare stdout, stderr
and the exit code byte-for-byte.

Build first, then run from the repository root:

    target/debug/mink.exe build validation/e_network/main.mink
    python validation/e_network/verify.py
"""
import http.server
import os
import socket
import subprocess
import sys
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
MINK_EXE = os.path.join(HERE, "main.exe")
PYTHON_REF = os.path.join(HERE, "python_ref.py")
HOST = "127.0.0.1"

BODY = b"hello from the test server\n"


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"

    def do_GET(self):
        if self.path == "/index.txt":
            self.send_response(200)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.send_header("Content-Length", str(len(BODY)))
            self.end_headers()
            self.wfile.write(BODY)
        elif self.path == "/empty.txt":
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.send_header("Content-Length", "0")
            self.end_headers()
        else:
            self.send_error(404)

    def log_message(self, *args):
        pass


def free_port():
    """A port that is free right now (bound then released)."""
    with socket.socket() as probe:
        probe.bind((HOST, 0))
        return probe.getsockname()[1]


def start_http_server():
    server = http.server.ThreadingHTTPServer((HOST, 0), Handler)
    port = server.server_address[1]
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return port, server


def start_tcp_echo():
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((HOST, 0))
    srv.listen(8)
    port = srv.getsockname()[1]

    def serve():
        while True:
            try:
                conn, _addr = srv.accept()
                data = conn.recv(4096)
                conn.sendall(data)
                conn.close()
            except OSError:
                return  # the harness closed the listener at the end

    threading.Thread(target=serve, daemon=True).start()
    return port, srv


def start_tls_server():
    """Start the repository's deterministic TLS fixture server and read the
    port it prints on stdout (see tests/tls/server.py and tests/tls_lib.rs)."""
    proc = subprocess.Popen(
        [sys.executable, os.path.join("tests", "tls", "server.py"),
         "--cert", "good.pem", "--key", "good.key.pem",
         "--certdir", os.path.join("tests", "tls"),
         "--root", os.path.join("validation", "e_network", "fixtures",
                                "tlsroot")],
        cwd=REPO, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL, text=True)
    deadline = time.time() + 30
    while time.time() < deadline:
        line = proc.stdout.readline().strip()
        if line:
            return proc, int(line)
    proc.kill()
    raise RuntimeError("TLS fixture server did not report a port")


def start_udp_echo():
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind((HOST, 0))
    port = sock.getsockname()[1]

    def serve():
        while True:
            try:
                data, addr = sock.recvfrom(4096)
                sock.sendto(data, addr)
            except OSError:
                return  # the harness closed the socket at the end

    threading.Thread(target=serve, daemon=True).start()
    return port, sock


def run_case(name, args):
    results = {}
    for who in ("mink", "py"):
        cmd = [MINK_EXE] + args if who == "mink" else \
            [sys.executable, PYTHON_REF] + args
        try:
            proc = subprocess.run(cmd, cwd=REPO, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, timeout=30)
        except subprocess.TimeoutExpired:
            print(f"FAIL {name}: {who} timed out")
            return False
        results[who] = (proc.stdout, proc.stderr, proc.returncode)

    problems = []
    if results["mink"][0] != results["py"][0]:
        problems.append(f"stdout differs:\n  mink={results['mink'][0]!r}\n"
                        f"  py  ={results['py'][0]!r}")
    if results["mink"][1] != results["py"][1]:
        problems.append(f"stderr differs:\n  mink={results['mink'][1]!r}\n"
                        f"  py  ={results['py'][1]!r}")
    if results["mink"][2] != results["py"][2]:
        problems.append(f"exit differs: mink={results['mink'][2]} "
                        f"py={results['py'][2]}")

    if problems:
        print(f"FAIL {name}")
        for problem in problems:
            print("   " + problem.replace("\n", "\n   "))
        return False
    print(f"ok   {name} (exit {results['mink'][2]})")
    return True


def main():
    if not os.path.exists(MINK_EXE):
        print(f"missing {MINK_EXE}; build first with "
              f"`target/debug/mink.exe build validation/e_network/main.mink`")
        return 1

    http_port, server = start_http_server()
    tls_proc, tls_port = start_tls_server()
    tcp_port, tcp_srv = start_tcp_echo()
    udp_port, udp_sock = start_udp_echo()
    closed_port = free_port()

    cases = [
        ("http_index", ["http", HOST, str(http_port), "/index.txt"]),
        ("http_empty_body", ["http", HOST, str(http_port), "/empty.txt"]),
        ("http_404", ["http", HOST, str(http_port), "/missing.txt"]),
        ("http_refused", ["http", HOST, str(closed_port), "/index.txt"]),
        ("https_index", ["https", "localhost", str(tls_port), "/index.html"]),
        ("https_404", ["https", "localhost", str(tls_port), "/missing.txt"]),
        ("https_refused", ["https", "localhost", str(closed_port),
                           "/index.html"]),
        ("tcp_echo", ["tcp", HOST, str(tcp_port), "ping-1234567890"]),
        ("tcp_second_connection",
         ["tcp", HOST, str(tcp_port), "second-round-trip"]),
        ("tcp_refused", ["tcp", HOST, str(closed_port), "nobody-home"]),
        ("udp_echo", ["udp", HOST, str(udp_port), "datagram-42"]),
        ("udp_second_datagram",
         ["udp", HOST, str(udp_port), "another-datagram"]),
        ("no_args", []),
        ("unknown_command", ["ftp", HOST, str(http_port), "/index.txt"]),
        ("bad_port_text", ["http", HOST, "abc", "/index.txt"]),
        ("zero_port", ["http", HOST, "0", "/index.txt"]),
        ("option_looking_arg", ["-x", HOST, str(http_port), "/index.txt"]),
        ("too_few_args", ["http", HOST]),
    ]

    passed = failed = 0
    for name, args in cases:
        if run_case(name, args):
            passed += 1
        else:
            failed += 1

    server.shutdown()
    tls_proc.kill()
    tls_proc.wait()
    tcp_srv.close()
    udp_sock.close()

    print(f"\n{passed}/{passed + failed} cases match the Python reference")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
