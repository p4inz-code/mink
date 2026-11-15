#!/usr/bin/env python3
"""M2 / D: drive the MINK HTTP server with concurrent clients.

Starts `main.exe serve <port> <n>` and then the server is expected to answer:
  /health  -> 200 with body "healthy"
  anything -> 404 with body "nope"

Each worker opens its own connection (Connection: close), so the run also
covers many accepts and a backlog of simultaneous connections.

    target/release/mink.exe build validation/m2_server/main.mink
    python validation/m2_server/verify.py
"""
import http.client
import os
import socket
import subprocess
import sys
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
MINK_EXE = os.path.join(HERE, "main.exe")


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def request(port, path):
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=15)
    try:
        conn.request("GET", path, headers={"Connection": "close"})
        resp = conn.getresponse()
        return resp.status, resp.read()
    finally:
        conn.close()


def run_scenario(port, total, workers, health_every):
    server = subprocess.Popen([MINK_EXE, "serve", str(port), str(total)],
                              cwd=HERE, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE)
    deadline = time.time() + 10
    while time.time() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=1):
                break
        except OSError:
            time.sleep(0.05)
    else:
        server.kill()
        print("FAIL server never listened")
        return False

    results = []
    lock = threading.Lock()

    per_worker = total // workers
    extra = total % workers

    def worker(index):
        local = []
        count = per_worker + (1 if index < extra else 0)
        for i in range(count):
            path = "/health" if (index + i) % health_every == 0 else "/missing"
            try:
                local.append((path,) + request(port, path))
            except Exception as exc:  # noqa: BLE001 - reported as a failure
                local.append((path, -1, str(exc).encode()))
        with lock:
            results.extend(local)

    threads = [threading.Thread(target=worker, args=(w,)) for w in range(workers)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    out, err = server.communicate(timeout=60)
    ok = True
    expected = total
    bad = [r for r in results if not (
        (r[0] == "/health" and r[1] == 200 and r[2] == b"healthy")
        or (r[0] == "/missing" and r[1] == 404 and r[2] == b"nope"))]
    if bad:
        ok = False
        print("FAIL %d wrong responses, first: %r" % (len(bad), bad[0]))
    if len(results) != expected:
        ok = False
        print("FAIL expected %d responses, got %d" % (expected, len(results)))
    if server.returncode != 0:
        ok = False
        print("FAIL server exit %d stderr=%r" % (server.returncode, err[:300]))
    want = ("served=%d\r\n" % total).encode()
    if out != want:
        ok = False
        print("FAIL stdout %r, want %r" % (out[:200], want))
    if ok:
        print("ok   %d requests over %d workers (%d health)" %
              (total, workers, sum(1 for r in results if r[0] == "/health")))
    return ok


def main():
    if not os.path.exists(MINK_EXE):
        print("missing %s; build validation/m2_server/main.mink first" % MINK_EXE)
        return 1
    ok = run_scenario(free_port(), 120, 16, 3)
    ok = run_scenario(free_port(), 40, 4, 2) and ok
    # usage errors
    for args, want in ((["serve"], 2), (["serve", "0", "5"], 2),
                       (["serve", "99999", "5"], 2), (["nope", "1", "1"], 2),
                       (["-x", "1", "1"], 2)):
        proc = subprocess.run([MINK_EXE] + args, cwd=HERE,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if proc.returncode != want:
            print("FAIL %r exit %d, want %d" % (args, proc.returncode, want))
            ok = False
    if ok:
        print("ok   usage errors report exit 2")
    print("\n%s" % ("PASS" if ok else "FAIL"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
