#!/usr/bin/env python3
"""Deterministic local HTTP/1.1 fixture server for M4 W2.

The server binds 127.0.0.1 on an ephemeral port, so W2 never depends on the
public internet or on a fixed port. It sends every response in several TCP
writes with a short pause between them: the client must handle a response
whose framing does not line up with packet boundaries, especially the chunked
body.

Endpoints:

  /ok      200, Content-Length, JSON body with an integral "value"
  /chunked 200, Transfer-Encoding: chunked, three chunks, same JSON body
  /notfound 404, text/html body
  /badjson 200, Content-Length, unparseable body
  /novalue 200, Content-Length, valid JSON without an integral "value"
  /empty   200, Content-Length: 0
  /close   accepts the request, sends nothing, closes
  /garbage accepts the request, sends bytes with no HTTP status line

`closed_port` is a port that was just bound and released, used for the
connection-refused case.
"""
import socket
import threading
import time

HOST = "127.0.0.1"

BODY_OK = b'{"value":42,"label":"netcheck"}'
BODY_404 = b"<h1>not found</h1>"
BODY_BAD = b"{oops,,,}"
BODY_NOVALUE = b'{"other":1}'

# A pause between TCP writes, long enough for the client to observe separate
# segments on loopback.
FRAGMENT_PAUSE = 0.02


def _response(status, content_type, body):
    head = (
        "HTTP/1.1 %s\r\nContent-Type: %s\r\nContent-Length: %d\r\n"
        "Connection: close\r\n\r\n" % (status, content_type, len(body))
    ).encode("ascii")
    return head, body


def _chunked_head():
    return (
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n"
        b"Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
    )


class FixtureServer:
    def __init__(self):
        self._listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._listener.bind((HOST, 0))
        self._listener.listen(16)
        self._listener.settimeout(0.2)
        self.port = self._listener.getsockname()[1]
        self.closed_port = _free_port()
        self._stopping = threading.Event()
        self._thread = None
        self._workers = []
        self._lock = threading.Lock()

    def tokens(self):
        return {"port": str(self.port), "closed_port": str(self.closed_port)}

    def start(self):
        self._thread = threading.Thread(target=self._accept_loop, daemon=True)
        self._thread.start()
        return self

    def stop(self):
        self._stopping.set()
        try:
            self._listener.close()
        except OSError:
            pass
        if self._thread is not None:
            self._thread.join(timeout=5)
        with self._lock:
            workers = list(self._workers)
        for worker in workers:
            worker.join(timeout=5)

    def __enter__(self):
        return self.start()

    def __exit__(self, *_exc):
        self.stop()

    def _accept_loop(self):
        while not self._stopping.is_set():
            try:
                connection, _address = self._listener.accept()
            except (socket.timeout, OSError):
                continue
            worker = threading.Thread(
                target=self._handle, args=(connection,), daemon=True
            )
            with self._lock:
                self._workers.append(worker)
            worker.start()

    def _handle(self, connection):
        try:
            connection.settimeout(10)
            request = self._read_request(connection)
            target = _request_target(request)
            self._respond(connection, target)
        except (OSError, ValueError):
            pass
        finally:
            try:
                connection.shutdown(socket.SHUT_WR)
            except OSError:
                pass
            try:
                connection.close()
            except OSError:
                pass

    @staticmethod
    def _read_request(connection):
        data = b""
        while b"\r\n\r\n" not in data and len(data) < 16384:
            chunk = connection.recv(4096)
            if not chunk:
                break
            data += chunk
        return data

    @staticmethod
    def _send(connection, data):
        connection.sendall(data)
        time.sleep(FRAGMENT_PAUSE)

    def _respond(self, connection, target):
        if target == "/ok":
            head, body = _response("200 OK", "application/json", BODY_OK)
            self._send(connection, head)
            self._send(connection, body[:10])
            self._send(connection, body[10:])
        elif target == "/chunked":
            self._send(connection, _chunked_head())
            for start, end in ((0, 10), (10, 20), (20, len(BODY_OK))):
                piece = BODY_OK[start:end]
                self._send(connection, b"%x\r\n" % len(piece) + piece + b"\r\n")
            self._send(connection, b"0\r\n\r\n")
        elif target == "/notfound":
            head, body = _response("404 Not Found", "text/html", BODY_404)
            self._send(connection, head)
            self._send(connection, body)
        elif target == "/badjson":
            head, body = _response("200 OK", "application/json", BODY_BAD)
            self._send(connection, head)
            self._send(connection, body)
        elif target == "/novalue":
            head, body = _response("200 OK", "application/json", BODY_NOVALUE)
            self._send(connection, head)
            self._send(connection, body)
        elif target == "/empty":
            head, body = _response("200 OK", "application/json", b"")
            self._send(connection, head)
        elif target == "/garbage":
            self._send(connection, b"NOT-HTTP\r\n\r\n")
        elif target == "/close":
            pass
        else:
            head, body = _response("404 Not Found", "text/html", BODY_404)
            self._send(connection, head)
            self._send(connection, body)


def _request_target(request):
    try:
        line = request.split(b"\r\n", 1)[0].decode("ascii", "replace")
        parts = line.split(" ")
        if len(parts) < 2:
            return ""
        target = parts[1]
        return target.split("?", 1)[0]
    except Exception:  # pragma: no cover - defensive
        return ""


def _free_port():
    probe = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        probe.bind((HOST, 0))
        return probe.getsockname()[1]
    finally:
        probe.close()


if __name__ == "__main__":
    with FixtureServer() as server:
        print("port=%d closed_port=%d" % (server.port, server.closed_port))
        try:
            while True:
                time.sleep(0.5)
        except KeyboardInterrupt:
            pass
