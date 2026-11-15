#!/usr/bin/env python3
"""M2 / C cross-check: `netstress` against its Python reference.

Starts the loopback fixture server (server.py) on three free ports, then runs
each case for both implementations and compares stdout, stderr and exit code.

    target/release/mink.exe build validation/m2_net/main.mink
    python validation/m2_net/verify.py
"""
import os
import socket
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
MINK_EXE = os.path.join(HERE, "main.exe")
PYTHON_REF = os.path.join(HERE, "python_ref.py")
SERVER = os.path.join(HERE, "server.py")


def free_ports(n):
    socks, ports = [], []
    for _ in range(n):
        s = socket.socket()
        s.bind(("127.0.0.1", 0))
        socks.append(s)
        ports.append(s.getsockname()[1])
    for s in socks:
        s.close()
    return ports


def wait_ready(proc, timeout=10.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        line = proc.stdout.readline()
        if line.strip() == b"ready":
            return True
        if proc.poll() is not None:
            return False
    return False


def run(cmd):
    proc = subprocess.run(cmd, cwd=HERE, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, timeout=300)
    return proc.stdout, proc.stderr, proc.returncode


def main():
    if not os.path.exists(MINK_EXE):
        print("missing %s; build validation/m2_net/main.mink first" % MINK_EXE)
        return 1
    tcp_port, http_port, udp_port = free_ports(3)
    server = subprocess.Popen([sys.executable, SERVER, str(tcp_port),
                               str(http_port), str(udp_port)],
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    try:
        if not wait_ready(server):
            print("fixture server did not start")
            return 1
        cases = [
            ("bulk", ["bulk", str(tcp_port)]),
            ("repeat_200", ["repeat", str(tcp_port), "200"]),
            ("repeat_bad_count", ["repeat", str(tcp_port), "0"]),
            ("udp", ["udp", str(udp_port)]),
            ("http", ["http", str(http_port)]),
            ("refused", ["refused", str(http_port + 1)]),
            ("timeout", ["timeout"]),
            ("no_args", []),
            ("bad_port", ["bulk", "notaport"]),
            ("unknown", ["frobnicate", str(tcp_port)]),
            ("option_looking", ["-x"]),
        ]
        passed = failed = 0
        for name, args in cases:
            mink = run([MINK_EXE] + args)
            py = run([sys.executable, PYTHON_REF] + args)
            problems = []
            if mink[0] != py[0]:
                problems.append("stdout differs:\n  mink=%r\n  py  =%r"
                                % (mink[0][:400], py[0][:400]))
            if mink[1] != py[1]:
                problems.append("stderr differs:\n  mink=%r\n  py  =%r"
                                % (mink[1][:400], py[1][:400]))
            if mink[2] != py[2]:
                problems.append("exit differs: mink=%d py=%d" % (mink[2], py[2]))
            if problems:
                print("FAIL %s" % name)
                for problem in problems:
                    print("   " + problem.replace("\n", "\n   "))
                failed += 1
            else:
                print("ok   %s (exit %d)" % (name, mink[2]))
                passed += 1
        print("\n%d/%d cases match the Python reference" % (passed, passed + failed))
        return 1 if failed else 0
    finally:
        server.kill()


if __name__ == "__main__":
    sys.exit(main())
