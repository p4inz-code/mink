"""Deterministic local TLS endpoint for the MINK S62 regression suite.

Modes:
  file server (default)   HTTPS GET of files under --root
  --echo N                after the handshake, read exactly N bytes and echo
                          them back (binary round-trip without HTTP framing)

The certificate/key come from the committed fixtures in this directory
(`<name>.pem`, `<name>.key.pem`). The script binds 127.0.0.1:0, prints the
chosen port on stdout, then serves until killed.
"""

import argparse
import os
import socket
import ssl
import sys
import threading
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *args):  # keep the harness output clean
        pass


def echo_loop(ctx, listener):
    conn, _ = listener.accept()
    try:
        tls = ctx.wrap_socket(conn, server_side=True)
        chunk = tls.recv(65536)
        if chunk:
            tls.sendall(chunk)
        tls.close()
    except Exception:
        pass
    finally:
        conn.close()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cert", default="good.pem")
    ap.add_argument("--key", default="good.key.pem")
    ap.add_argument("--root", default=".")
    ap.add_argument("--certdir", default=HERE)
    ap.add_argument("--expect", type=int, default=-1, help="echo mode: exact byte count")
    ap.add_argument("--connections", type=int, default=0, help="stop after N (0 = forever)")
    args = ap.parse_args()

    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(
        os.path.join(args.certdir, args.cert), os.path.join(args.certdir, args.key)
    )

    if args.expect >= 0:
        listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(("127.0.0.1", 0))
        listener.listen(8)
        print(listener.getsockname()[1], flush=True)
        while True:
            echo_loop(ctx, listener)
    else:
        os.chdir(args.root)
        httpd = ThreadingHTTPServer(("127.0.0.1", 0), QuietHandler)
        httpd.daemon_threads = True
        httpd.socket = ctx.wrap_socket(httpd.socket, server_side=True)
        print(httpd.server_address[1], flush=True)
        httpd.serve_forever()


if __name__ == "__main__":
    main()
