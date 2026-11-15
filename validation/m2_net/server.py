#!/usr/bin/env python3
"""Loopback fixture server for the M2 networking workload.

One TCP port echoes every byte it receives (so the 1 MiB upload comes back
unchanged), one HTTP port answers `GET /data` with a fixed body, and one UDP
port echoes each datagram. Started by verify.py; it can also be run by hand:

    python validation/m2_net/server.py 34101 34102 34103
"""
import socket
import sys
import threading

BODY = b"mink-http-ok\n"
RESPONSE = (
    b"HTTP/1.1 200 OK\r\n"
    b"Content-Type: text/plain\r\n"
    b"Content-Length: %d\r\n"
    b"Connection: close\r\n"
    b"\r\n" % len(BODY)
) + BODY


def echo_client(conn):
    try:
        while True:
            chunk = conn.recv(65536)
            if not chunk:
                return
            conn.sendall(chunk)
    except OSError:
        pass
    finally:
        try:
            conn.close()
        except OSError:
            pass


def tcp_server(port, started):
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", port))
    srv.listen(64)
    started.set()
    while True:
        conn, _ = srv.accept()
        threading.Thread(target=echo_client, args=(conn,), daemon=True).start()


def http_client(conn):
    try:
        request = b""
        while b"\r\n\r\n" not in request:
            chunk = conn.recv(4096)
            if not chunk:
                return
            request += chunk
        conn.sendall(RESPONSE)
    except OSError:
        pass
    finally:
        try:
            conn.close()
        except OSError:
            pass


def http_server(port, started):
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", port))
    srv.listen(64)
    started.set()
    while True:
        conn, _ = srv.accept()
        threading.Thread(target=http_client, args=(conn,), daemon=True).start()


def udp_server(port, started):
    srv = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", port))
    started.set()
    while True:
        data, addr = srv.recvfrom(65536)
        srv.sendto(data, addr)


def main(argv):
    if len(argv) != 3:
        print("usage: server.py <tcp-port> <http-port> <udp-port>")
        return 2
    tcp, http, udp = (int(p) for p in argv)
    ready = []
    for target, port in ((tcp_server, tcp), (http_server, http), (udp_server, udp)):
        event = threading.Event()
        ready.append(event)
        threading.Thread(target=target, args=(port, event), daemon=True).start()
    for event in ready:
        event.wait(5.0)
    print("ready", flush=True)
    threading.Event().wait()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
