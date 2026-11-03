#!/usr/bin/env python3
"""hookbox contract driver.

The MINK executable is checked against `oracle.py`, an independent
implementation of the frozen contract in
`docs/benchmarks/M5/M5_APPLICATION_SELECTION.md` (section 6). The driver also
runs the CLI, lifecycle, restart-recovery, failure-cleanup and standalone
checks that the contract defines but that no single response can express.

Usage:
    python benchmarks/m5/hookbox/tests/driver.py
    python benchmarks/m5/hookbox/tests/driver.py --no-build
    python benchmarks/m5/hookbox/tests/driver.py --update-goldens

Exit status is 0 only when every check passes.
"""

from __future__ import annotations

import argparse
import contextlib
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import traceback
from pathlib import Path

import oracle

ROOT = Path(__file__).resolve().parents[4]
APP = ROOT / "benchmarks" / "m5" / "hookbox"
SOURCE = APP / "app" / "main.mink"
BUILD = APP / "build"
EXE = BUILD / "hookbox.exe"
FIXTURES = APP / "fixtures"
BODIES = FIXTURES / "bodies"
EXPECTED = APP / "expected"

EVENT_NAME = re.compile(r"^[0-9]{6}\.json$")


# ---------------------------------------------------------------------------
# Harness
# ---------------------------------------------------------------------------

class Suite:
    def __init__(self, only: str | None = None) -> None:
        self.passed = 0
        self.failed: list[tuple[str, str]] = []
        self.skipped: list[tuple[str, str]] = []
        self.only = only

    def check(self, name: str, fn) -> None:
        if self.only is not None and self.only.lower() not in name.lower():
            return
        try:
            fn()
        except SkipTest as exc:
            self.skipped.append((name, str(exc)))
            print(f"SKIP  {name}: {exc}")
        except Exception as exc:  # noqa: BLE001 - the driver reports failures
            detail = traceback.format_exc()
            self.failed.append((name, detail))
            print(f"FAIL  {name}: {exc}")
            print(detail)
        else:
            self.passed += 1
            print(f"PASS  {name}")


class SkipTest(Exception):
    pass


def expect(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


_ACTIVE: list["Server"] = []


@contextlib.contextmanager
def workdir():
    """A scratch directory that never outlives a still-running server.

    A failed assertion must not leave a hookbox process holding the directory
    as its cwd: that would turn one real failure into a confusing Windows
    sharing error during cleanup.
    """
    with tempfile.TemporaryDirectory(prefix="hookbox_case_") as td:
        try:
            yield Path(td)
        finally:
            for srv in list(_ACTIVE):
                srv.kill()
            _ACTIVE.clear()


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def http_request(
    method: str,
    target: str,
    body: bytes | None = None,
    *,
    content_length: bool = True,
    headers: tuple[str, ...] = (),
) -> bytes:
    lines = [f"{method} {target} HTTP/1.1", "Host: 127.0.0.1"]
    lines.extend(headers)
    if body is not None and content_length:
        lines.append(f"Content-Length: {len(body)}")
    head = ("\r\n".join(lines) + "\r\n\r\n").encode("ascii")
    return head + (body or b"")


class Server:
    def __init__(self, exe: Path, args: list[str], cwd: Path, port: int) -> None:
        self.port = port
        self.proc = subprocess.Popen(
            [str(exe), *args],
            cwd=str(cwd),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.finished = False
        _ACTIVE.append(self)

    def wait_listening(self, timeout: float = 20.0) -> "Server":
        deadline = time.time() + timeout
        last: Exception | None = None
        while time.time() < deadline:
            if self.proc.poll() is not None:
                out, err = self.proc.communicate()
                raise AssertionError(
                    f"hookbox exited {self.proc.returncode} before listening: "
                    f"stdout={out!r} stderr={err!r}"
                )
            try:
                with socket.create_connection(("127.0.0.1", self.port), timeout=0.5):
                    return self
            except OSError as exc:  # listener not up yet
                last = exc
                time.sleep(0.05)
        raise AssertionError(f"hookbox never listened on {self.port}: {last}")

    def request(self, raw: bytes, timeout: float = 10.0) -> bytes:
        return self.request_parts([raw], delay=0.0, timeout=timeout)

    def request_parts(
        self, parts: list[bytes], delay: float = 0.05, timeout: float = 10.0
    ) -> bytes:
        """Send one request in several TCP writes (read-reassembly check)."""
        with socket.create_connection(("127.0.0.1", self.port), timeout=timeout) as sock:
            for part in parts:
                sock.sendall(part)
                if delay > 0:
                    time.sleep(delay)
            try:
                sock.shutdown(socket.SHUT_WR)
            except OSError:
                pass
            chunks: list[bytes] = []
            while True:
                chunk = sock.recv(65536)
                if not chunk:
                    break
                chunks.append(chunk)
        return b"".join(chunks)

    def finish(self, timeout: float = 30.0) -> tuple[int, bytes, bytes]:
        expect(not self.finished, "server already finished")
        self.finished = True
        try:
            out, err = self.proc.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            out, err = self.proc.communicate()
            raise AssertionError(
                f"hookbox did not exit within {timeout}s: stdout={out!r} stderr={err!r}"
            )
        if self in _ACTIVE:
            _ACTIVE.remove(self)
        return int(self.proc.returncode or 0), out, err

    def kill(self) -> None:
        if not self.finished:
            if self.proc.poll() is None:
                self.proc.kill()
            self.proc.communicate()
            self.finished = True
        if self in _ACTIVE:
            _ACTIVE.remove(self)


def run_server(
    exe: Path,
    work: Path,
    *,
    max_requests: int | None,
    inbox: str = "inbox",
    port: int | None = None,
) -> tuple[Server, int]:
    if port is None:
        port = free_port()
    args = ["--port", str(port), "--inbox", inbox]
    if max_requests is not None:
        args += ["--max-requests", str(max_requests)]
    return Server(exe, args, work, port).wait_listening(), port


def parse_response(raw: bytes) -> tuple[bytes, int, bytes, dict[str, bytes], bytes]:
    head, sep, body = raw.partition(b"\r\n\r\n")
    expect(sep == b"\r\n\r\n", f"response has no header terminator: {raw!r}")
    lines = head.split(b"\r\n")
    expect(len(lines) >= 1, "empty response")
    parts = lines[0].split(b" ", 2)
    expect(len(parts) == 3, f"bad status line: {lines[0]!r}")
    headers: dict[str, bytes] = {}
    for line in lines[1:]:
        name, colon, value = line.partition(b":")
        expect(colon == b":", f"bad header line: {line!r}")
        headers[name.strip().lower().decode("ascii")] = value.strip()
    return parts[0], int(parts[1]), parts[2], headers, body


def check_response(
    raw: bytes,
    status: int,
    body: bytes,
    *,
    content_type: bytes | None = None,
) -> dict[str, bytes]:
    version, got_status, reason, headers, got_body = parse_response(raw)
    expect(version == b"HTTP/1.1", f"version {version!r}")
    expect(got_status == status, f"status {got_status} != {status}; body={got_body!r}")
    expect(
        reason.decode("ascii") == oracle.REASONS[status],
        f"reason {reason!r} != {oracle.REASONS[status]!r}",
    )
    expect(
        headers.get("content-length") == str(len(got_body)).encode("ascii"),
        f"Content-Length {headers.get('content-length')!r} != {len(got_body)}",
    )
    expect(headers.get("connection") == b"close", "Connection must be close")
    if content_type is not None:
        expect(
            headers.get("content-type") == content_type,
            f"Content-Type {headers.get('content-type')!r} != {content_type!r}",
        )
    expect(got_body == body, f"body {got_body!r} != expected {body!r}")
    return headers


def read_inbox(path: Path) -> dict[str, bytes]:
    expect(path.is_dir(), f"inbox missing: {path}")
    return {p.name: p.read_bytes() for p in sorted(path.iterdir()) if p.is_file()}


def body(name: str) -> bytes:
    return (BODIES / name).read_bytes()


def pad_body(size: int) -> bytes:
    """A valid JSON object of exactly `size` bytes: {"pad":"XXX..."}."""
    expect(size >= 10, "pad body needs at least 10 bytes")
    payload = b'{"pad":"' + b"X" * (size - 10) + b'"}'
    expect(len(payload) == size, f"pad body is {len(payload)} bytes, expected {size}")
    return payload


def expect_exit(
    exe: Path,
    work: Path,
    args: list[str],
    code: int,
    *,
    stdout: bytes = b"",
) -> bytes:
    proc = subprocess.run(
        [str(exe), *args], cwd=str(work), capture_output=True, timeout=30
    )
    expect(proc.returncode == code, f"exit {proc.returncode} != {code}; stderr={proc.stderr!r}")
    expect(proc.stdout == stdout, f"stdout {proc.stdout!r} != {stdout!r}")
    expect(proc.stderr.startswith(b"hookbox: "), f"stderr not prefixed: {proc.stderr!r}")
    expect(b"\r\n" in proc.stderr, "stderr diagnostic must be CRLF-terminated")
    return proc.stderr


def expect_listener_closed(port: int) -> None:
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=1.0):
            pass
    except OSError:
        return
    raise AssertionError(f"listener on {port} still accepts after exit")


def expect_clean_event(name: str, data: bytes, seq: int, parsed_body: bytes) -> None:
    expect(EVENT_NAME.match(name) is not None, f"event name {name!r} is not <6 digits>.json")
    expect(data == oracle.event_file(seq, parsed_body), f"{name} bytes differ from the oracle")
    expect(data.endswith(b"\n"), f"{name} must end with LF")
    expect(not data.endswith(b"\r\n"), f"{name} must not use CRLF")
    expect(data.count(b"\n") == 1, f"{name} must be one line")


def expect_no_events(path: Path) -> None:
    files = read_inbox(path)
    expect(files == {}, f"unexpected event files: {sorted(files)}")


# ---------------------------------------------------------------------------
# Build / goldens
# ---------------------------------------------------------------------------

def build_exe() -> None:
    mink = ROOT / "target" / "release" / "mink.exe"
    if not mink.exists():
        mink = ROOT / "target" / "debug" / "mink.exe"
    expect(mink.exists(), f"MINK compiler not found under {ROOT / 'target'}")
    proc = subprocess.run(
        [str(mink), "build", str(SOURCE)], cwd=ROOT, capture_output=True, text=True
    )
    expect(
        proc.returncode == 0,
        f"mink build failed ({proc.returncode})\n{proc.stdout}\n{proc.stderr}",
    )
    produced = SOURCE.with_suffix(".exe")
    expect(produced.exists(), f"build did not produce {produced}")
    BUILD.mkdir(parents=True, exist_ok=True)
    shutil.copy2(produced, EXE)
    print(f"built {EXE.relative_to(ROOT)} ({EXE.stat().st_size} bytes)")


def basic_event_bytes() -> tuple[bytes, bytes]:
    return (
        oracle.event_file(1, body("object.json")),
        oracle.event_file(2, body("nested.json")),
    )


def update_goldens(exe: Path) -> None:
    (EXPECTED / "basic").mkdir(parents=True, exist_ok=True)
    e1, e2 = basic_event_bytes()
    (EXPECTED / "basic" / "000001.json").write_bytes(e1)
    (EXPECTED / "basic" / "000002.json").write_bytes(e2)
    (EXPECTED / "basic" / "events_2.ndjson").write_bytes(
        oracle.events_payload([e1, e2])
    )
    (EXPECTED / "basic" / "stdout.txt").write_bytes(
        b"listening port={port}\r\nserved=3\r\n"
    )
    with workdir() as work:
        proc = subprocess.run(
            [str(exe), "--help"], cwd=str(work), capture_output=True, timeout=30
        )
        expect(proc.returncode == 0, f"--help exit {proc.returncode}")
        (EXPECTED / "help.txt").write_bytes(proc.stdout)
    print(f"goldens written under {EXPECTED.relative_to(ROOT)}")


def golden(name: str) -> bytes:
    path = EXPECTED / name
    expect(path.exists(), f"missing golden {path}; run with --update-goldens")
    return path.read_bytes()


# ---------------------------------------------------------------------------
# Checks
# ---------------------------------------------------------------------------

def run_suite(exe: Path, only: str | None = None) -> Suite:
    suite = Suite(only)

    # --- CLI ---------------------------------------------------------------
    def help_ok() -> None:
        with workdir() as work:
            proc = subprocess.run(
                [str(exe), "--help"], cwd=str(work), capture_output=True, timeout=30
            )
            expect(proc.returncode == 0, f"exit {proc.returncode}")
            expect(proc.stderr == b"", f"help wrote stderr: {proc.stderr!r}")
            expect(proc.stdout == golden("help.txt"), "help output differs from golden")
            for needle in (
                b"usage: hookbox --port <n> --inbox <dir>",
                b"--max-requests",
                b"POST /hooks",
                b"GET /health",
                b"GET /events?limit=N",
            ):
                expect(needle in proc.stdout, f"help missing {needle!r}")
            expect(proc.stdout.endswith(b"\r\n"), "help must end with CRLF")
            expect(b"\n" not in proc.stdout.replace(b"\r\n", b""), "help has a bare LF")

    suite.check("--help exits 0 with exact golden bytes", help_ok)

    usage_cases = [
        ("no arguments", []),
        ("unknown option", ["--wat"]),
        ("short help is not an option", ["-h"]),
        ("help with extra argument", ["--help", "extra"]),
        ("missing --port", ["--inbox", "inbox"]),
        ("missing --inbox", ["--port", "8080"]),
        ("--port without value", ["--port"]),
        ("--inbox without value", ["--port", "8080", "--inbox"]),
        ("--max-requests without value", ["--port", "8080", "--inbox", "inbox", "--max-requests"]),
        ("duplicate --port", ["--port", "8080", "--port", "8081", "--inbox", "inbox"]),
        ("duplicate --inbox", ["--port", "8080", "--inbox", "a", "--inbox", "b"]),
        ("duplicate --max-requests", ["--port", "8080", "--inbox", "inbox", "--max-requests", "1", "--max-requests", "2"]),
        ("stray positional", ["--port", "8080", "--inbox", "inbox", "extra"]),
        ("port 0", ["--port", "0", "--inbox", "inbox"]),
        ("port 65536", ["--port", "65536", "--inbox", "inbox"]),
        ("port not a number", ["--port", "abc", "--inbox", "inbox"]),
        ("port negative", ["--port", "-1", "--inbox", "inbox"]),
        ("port leading space", ["--port", " 80", "--inbox", "inbox"]),
        ("port underscore", ["--port", "1_0", "--inbox", "inbox"]),
        ("port plus sign", ["--port", "+80", "--inbox", "inbox"]),
        ("max-requests too large", ["--port", "8080", "--inbox", "inbox", "--max-requests", "1000001"]),
        ("max-requests not a number", ["--port", "8080", "--inbox", "inbox", "--max-requests", "abc"]),
        ("max-requests negative", ["--port", "8080", "--inbox", "inbox", "--max-requests", "-5"]),
        ("max-requests float", ["--port", "8080", "--inbox", "inbox", "--max-requests", "1.0"]),
        ("inbox value looks like an option", ["--port", "8080", "--inbox", "--max-requests"]),
        ("empty inbox value", ["--port", "8080", "--inbox", ""]),
    ]

    def usage_case(args: list[str]) -> None:
        with workdir() as work:
            expect_exit(exe, work, args, 1)
            expect(not (work / "inbox").exists(), "usage error must not create the inbox")

    for label, args in usage_cases:
        suite.check(f"usage error: {label} -> 1", lambda args=args: usage_case(args))

    # --- Startup failures --------------------------------------------------
    def inbox_parent_missing() -> None:
        with workdir() as work:
            expect_exit(
                exe, work, ["--port", str(free_port()), "--inbox", "missing/inbox"], 2
            )
            expect(not (work / "missing").exists(), "a one-level create must not make parents")

    suite.check("inbox parent missing -> 2", inbox_parent_missing)

    def inbox_is_a_file() -> None:
        with workdir() as work:
            (work / "inbox").write_bytes(b"not a directory")
            expect_exit(
                exe, work, ["--port", str(free_port()), "--inbox", "inbox"], 2
            )

    suite.check("inbox is a file -> 2", inbox_is_a_file)

    def inbox_not_writable() -> None:
        with workdir() as work:
            inbox = work / "inbox"
            inbox.mkdir()
            (inbox / ".hookbox-probe").mkdir()
            expect_exit(
                exe, work, ["--port", str(free_port()), "--inbox", "inbox"], 2
            )

    suite.check("inbox write probe blocked -> 2", inbox_not_writable)

    def port_in_use() -> None:
        with workdir() as work:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
                listener.bind(("127.0.0.1", 0))
                listener.listen(5)
                port = int(listener.getsockname()[1])
                expect_exit(
                    exe, work, ["--port", str(port), "--inbox", "inbox"], 3
                )

    suite.check("port already in use -> 3", port_in_use)

    # --- Core endpoint behavior -------------------------------------------
    def basic_contract() -> None:
        object_body = body("object.json")
        nested_body = body("nested.json")
        e1, e2 = basic_event_bytes()
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=3)
            check_response(
                srv.request(http_request("POST", "/hooks", object_body)),
                201,
                oracle.created_body(1),
            )
            check_response(
                srv.request(http_request("POST", "/hooks", nested_body)),
                201,
                oracle.created_body(2),
            )
            check_response(
                srv.request(http_request("GET", "/events?limit=2")),
                200,
                oracle.events_payload([e1, e2]),
                content_type=oracle.NDJSON_TYPE,
            )
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}; stderr={err!r}")
            expect(out == oracle.stdout_lines(port, 3), f"stdout {out!r}")
            expect(err == b"", f"stderr {err!r}")
            files = read_inbox(work / "inbox")
            expect(list(files) == ["000001.json", "000002.json"], f"files {sorted(files)}")
            expect_clean_event("000001.json", files["000001.json"], 1, object_body)
            expect_clean_event("000002.json", files["000002.json"], 2, nested_body)
            expect(files["000001.json"] == golden("basic/000001.json"), "event 1 golden")
            expect(files["000002.json"] == golden("basic/000002.json"), "event 2 golden")
            expect(
                oracle.events_payload([files["000001.json"], files["000002.json"]])
                == golden("basic/events_2.ndjson"),
                "NDJSON golden",
            )
            expected_stdout = golden("basic/stdout.txt").replace(
                b"{port}", str(port).encode("ascii")
            )
            expect(out == expected_stdout, "stdout golden")
            expect_listener_closed(port)
            expect(not (work / "inbox" / ".hookbox-probe").exists(), "probe file left behind")

    suite.check("basic contract: 2 events, /events, stdout, goldens", basic_contract)

    def compact_reserialization() -> None:
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=4)
            for name in ("spaced.json", "duplicate.json"):
                expect(
                    check_response(
                        srv.request(http_request("POST", "/hooks", body(name))),
                        201,
                        oracle.created_body(1 if name == "spaced.json" else 2),
                    )
                    is not None,
                    "response missing",
                )
            files = read_inbox(work / "inbox")
            expect_clean_event("000001.json", files["000001.json"], 1, body("spaced.json"))
            expect(
                files["000001.json"] == b'{"seq":1,"body":{"a":1,"b":[true,null]}}\n',
                f"spaced body was not compacted: {files['000001.json']!r}",
            )
            expect(
                files["000002.json"] == b'{"seq":2,"body":{"a":1,"a":2}}\n',
                f"duplicate keys were not preserved: {files['000002.json']!r}",
            )
            check_response(
                srv.request(http_request("GET", "/health")), 200, oracle.health_body(2)
            )
            check_response(
                srv.request(http_request("GET", "/events?limit=20")),
                200,
                oracle.events_payload(
                    [files["000001.json"], files["000002.json"]]
                ),
                content_type=oracle.NDJSON_TYPE,
            )
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}; stderr={err!r}")
            expect(out == oracle.stdout_lines(port, 4), f"stdout {out!r}")

    suite.check("compact re-serialization and duplicate keys", compact_reserialization)

    def rejected_bodies() -> None:
        rejected = [
            ("array root", body("array.json")),
            ("scalar root", body("scalar.json")),
            ("invalid JSON", body("invalid.json")),
            ("empty body", body("empty.json")),
            ("whitespace only", b"   \r\n\t "),
            ("truncated object", b'{"a":1'),
            ("trailing junk", b'{"a":1}x'),
        ]
        for label, payload in rejected:
            with workdir() as work:
                srv, port = run_server(exe, work, max_requests=1)
                check_response(
                    srv.request(http_request("POST", "/hooks", payload)),
                    400,
                    oracle.DOCUMENTED_BODIES[400],
                )
                code, out, err = srv.finish()
                expect(code == 0, f"{label}: exit {code}; stderr={err!r}")
                expect(out == oracle.stdout_lines(port, 1), f"{label}: stdout {out!r}")
                expect_no_events(work / "inbox")

    suite.check("non-object/invalid/empty bodies -> 400, no event", rejected_bodies)

    def missing_content_length() -> None:
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=1)
            raw = srv.request(http_request("POST", "/hooks", content_length=False))
            check_response(raw, 400, oracle.DOCUMENTED_BODIES[400])
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}; stderr={err!r}")
            expect_no_events(work / "inbox")

    suite.check("POST /hooks without Content-Length -> 400", missing_content_length)

    def unknown_and_wrong_method() -> None:
        # Method is checked first: a query string on /hooks is only reached by
        # a POST, because GET /hooks is already a wrong-method 405.
        known = [
            ("GET", "/nope", 404),
            ("GET", "/", 404),
            ("GET", "/hooks/", 404),
            ("GET", "/health/", 404),
            ("GET", "/events/", 404),
            ("GET", "/health?x=1", 404),
            ("POST", "/hooks?x=1", 404),
            ("GET", "/events?limit=1&x=1", 400),
        ]
        for method, target, status in known:
            with workdir() as work:
                srv, port = run_server(exe, work, max_requests=1)
                raw = srv.request(http_request(method, target))
                expected = (
                    oracle.FROZEN_BODIES[404]
                    if status == 404
                    else oracle.DOCUMENTED_BODIES[400]
                )
                check_response(raw, status, expected)
                srv.finish()
        wrong_method = [
            ("GET", "/hooks"),
            ("PUT", "/hooks"),
            ("OPTIONS", "/hooks"),
            ("POST", "/health"),
            ("DELETE", "/health"),
            ("PATCH", "/health"),
            ("POST", "/events"),
            ("HEAD", "/events"),
        ]
        for method, target in wrong_method:
            with workdir() as work:
                srv, port = run_server(exe, work, max_requests=1)
                check_response(
                    srv.request(http_request(method, target)),
                    405,
                    oracle.FROZEN_BODIES[405],
                )
                code, _, _ = srv.finish()
                expect(code == 0, f"{method} {target}: exit {code}")
                expect_no_events(work / "inbox")

    suite.check("unknown path -> 404 and wrong method -> 405", unknown_and_wrong_method)

    def event_limits() -> None:
        bad_limits = ["0", "21", "abc", "", "1.5", "-1", " 5"]
        for value in bad_limits:
            with workdir() as work:
                srv, port = run_server(exe, work, max_requests=1)
                check_response(
                    srv.request(http_request("GET", f"/events?limit={value}")),
                    400,
                    oracle.DOCUMENTED_BODIES[400],
                )
                srv.finish()
        for value in ("1", "20", "05"):
            with workdir() as work:
                srv, port = run_server(exe, work, max_requests=1)
                check_response(
                    srv.request(http_request("GET", f"/events?limit={value}")),
                    200,
                    b"",
                    content_type=oracle.NDJSON_TYPE,
                )
                srv.finish()

    suite.check("limit boundaries and malformed limits", event_limits)

    def empty_inbox_views() -> None:
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=3)
            check_response(
                srv.request(http_request("GET", "/health")), 200, oracle.health_body(0)
            )
            check_response(
                srv.request(http_request("GET", "/events")),
                200,
                b"",
                content_type=oracle.NDJSON_TYPE,
            )
            raw = srv.request(http_request("GET", "/nope"))
            check_response(raw, 404, oracle.FROZEN_BODIES[404])
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}; stderr={err!r}")
            expect(out == oracle.stdout_lines(port, 3), f"stdout {out!r}")

    suite.check("empty-inbox /health and /events", empty_inbox_views)

    def event_window() -> None:
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=28)
            events: list[bytes] = []
            for i in range(25):
                payload = b'{"n":%d}' % i
                check_response(
                    srv.request(http_request("POST", "/hooks", payload)),
                    201,
                    oracle.created_body(i + 1),
                )
                events.append(oracle.event_file(i + 1, payload))
            check_response(
                srv.request(http_request("GET", "/health")), 200, oracle.health_body(25)
            )
            check_response(
                srv.request(http_request("GET", "/events?limit=20")),
                200,
                oracle.events_payload(events[-20:]),
                content_type=oracle.NDJSON_TYPE,
            )
            check_response(
                srv.request(http_request("GET", "/events")),
                200,
                oracle.events_payload(events[-10:]),
                content_type=oracle.NDJSON_TYPE,
            )
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}; stderr={err!r}")
            expect(out == oracle.stdout_lines(port, 28), f"stdout {out!r}")
            files = read_inbox(work / "inbox")
            expect(len(files) == 25, f"expected 25 event files, got {len(files)}")
            expect(sorted(files) == [f"{i:06d}.json" for i in range(1, 26)], "event numbering")
            expect(len(set(files)) == 25, "duplicate event names")

    suite.check("25 events: numbering, default limit 10, limit 20", event_window)

    # --- JSON body boundary (the documented MINK stdlib limit) ------------
    def json_boundary_4096() -> None:
        payload = pad_body(4096)
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=1)
            check_response(
                srv.request(http_request("POST", "/hooks", payload)),
                201,
                oracle.created_body(1),
            )
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}; stderr={err!r}")
            files = read_inbox(work / "inbox")
            expect_clean_event("000001.json", files["000001.json"], 1, payload)

    suite.check("JSON body 4096 bytes -> 201 (largest accepted body)", json_boundary_4096)

    def json_boundary_refused(size: int) -> None:
        payload = pad_body(size)
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=1)
            check_response(
                srv.request(http_request("POST", "/hooks", payload)),
                400,
                oracle.DOCUMENTED_BODIES[400],
            )
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}; stderr={err!r}")
            expect(
                out == oracle.stdout_lines(port, 1),
                f"refused body must not change the served count: {out!r}",
            )
            expect_no_events(work / "inbox")

    suite.check(
        "JSON body 4097 bytes -> 400 (MINK stdlib arena limit)",
        lambda: json_boundary_refused(4097),
    )
    suite.check(
        "JSON body 8192 bytes -> 400 (contract maximum refused by MINK)",
        lambda: json_boundary_refused(8192),
    )

    def json_boundary_413() -> None:
        payload = pad_body(8193)
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=1)
            check_response(
                srv.request(http_request("POST", "/hooks", payload)),
                413,
                oracle.DOCUMENTED_BODIES[413],
            )
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}; stderr={err!r}")
            expect_no_events(work / "inbox")

    suite.check("JSON body 8193 bytes -> 413", json_boundary_413)

    def declared_oversize() -> None:
        # The declared length alone proves the body cannot be accepted; the
        # peer then closes early, so only part of the body ever arrives.
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=1)
            head = (
                b"POST /hooks HTTP/1.1\r\nHost: 127.0.0.1\r\n"
                b"Content-Length: 9000\r\n\r\n"
            )
            check_response(
                srv.request(head + b'{"a":1}'),
                413,
                oracle.DOCUMENTED_BODIES[413],
            )
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}; stderr={err!r}")
            expect_no_events(work / "inbox")

    suite.check("declared Content-Length over 8192 -> 413", declared_oversize)

    def split_tcp_request() -> None:
        # One request spread over three writes: the server must reassemble by
        # Content-Length rather than treat the first segment as the request.
        payload = b'{"split":true}'
        expect(len(payload) == 14, "fixture length")
        head = b"POST /hooks HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 14\r\n\r\n"
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=1)
            raw = srv.request_parts([head, payload[:5], payload[5:]])
            check_response(raw, 201, oracle.created_body(1))
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}; stderr={err!r}")
            files = read_inbox(work / "inbox")
            expect_clean_event("000001.json", files["000001.json"], 1, payload)

    suite.check("request split across three TCP writes is reassembled", split_tcp_request)

    def rejected_body_keeps_sequence() -> None:
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=2)
            check_response(
                srv.request(http_request("POST", "/hooks", body("array.json"))),
                400,
                oracle.DOCUMENTED_BODIES[400],
            )
            check_response(
                srv.request(http_request("POST", "/hooks", b'{"ok":1}')),
                201,
                oracle.created_body(1),
            )
            code, _, err = srv.finish()
            expect(code == 0, f"exit {code}; stderr={err!r}")
            files = read_inbox(work / "inbox")
            expect(list(files) == ["000001.json"], f"rejection consumed a sequence: {sorted(files)}")
            expect(files["000001.json"] == oracle.event_file(1, b'{"ok":1}'), "event bytes")

    suite.check("a rejected body does not advance the sequence", rejected_body_keeps_sequence)

    def startup_window_and_ignored_names() -> None:
        with workdir() as work:
            inbox = work / "inbox"
            inbox.mkdir()
            for i in range(1, 26):
                (inbox / f"{i:06d}.json").write_bytes(
                    oracle.event_file(i, b'{"n":%d}' % i)
                )
            # 000099.JSON is deliberately above the next sequence: it is not
            # an event (byte-exact lowercase ".json" required), and on a
            # case-insensitive filesystem writing 000099.json later would be
            # the same file. 000026 is the live target, so the probe must not
            # sit on it.
            ignored = {
                "000000.json": oracle.event_file(0, b'{"n":0}'),
                "1234567.json": b'{"seq":1234567,"body":{}}\n',
                "000099.JSON": b"uppercase extension is not an event",
                "notes.txt": b"not an event",
            }
            for name, data in ignored.items():
                (inbox / name).write_bytes(data)
            srv, port = run_server(exe, work, max_requests=3)
            check_response(
                srv.request(http_request("POST", "/hooks", b'{"n":26}')),
                201,
                oracle.created_body(26),
            )
            check_response(
                srv.request(http_request("GET", "/health")), 200, oracle.health_body(26)
            )
            window = [
                oracle.event_file(i, b'{"n":%d}' % i) for i in range(7, 27)
            ]
            check_response(
                srv.request(http_request("GET", "/events?limit=20")),
                200,
                oracle.events_payload(window),
                content_type=oracle.NDJSON_TYPE,
            )
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}; stderr={err!r}")
            files = read_inbox(inbox)
            for name, data in ignored.items():
                expect(files[name] == data, f"ignored entry {name} was modified")
            expect(
                files["000026.json"] == oracle.event_file(26, b'{"n":26}'),
                "new event must follow the startup maximum",
            )
            expect(
                "000000.json" in files
                and "1234567.json" in files
                and "000099.JSON" in files,
                "non-event names must be preserved",
            )

    suite.check(
        "25 existing events: newest-20 startup window, junk names ignored",
        startup_window_and_ignored_names,
    )

    # --- Lifecycle ---------------------------------------------------------
    def request_limit_counts_every_request() -> None:
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=3)
            check_response(
                srv.request(http_request("POST", "/hooks", b'{"a":1}')),
                201,
                oracle.created_body(1),
            )
            check_response(
                srv.request(http_request("GET", "/missing")),
                404,
                oracle.FROZEN_BODIES[404],
            )
            check_response(
                srv.request(http_request("GET", "/health")), 200, oracle.health_body(1)
            )
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}; stderr={err!r}")
            expect(out == oracle.stdout_lines(port, 3), f"stdout {out!r}")
            expect(err == b"", f"stderr {err!r}")
            expect_listener_closed(port)

    suite.check("max-requests counts 404s too, then exits 0", request_limit_counts_every_request)

    def request_limit_one() -> None:
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=1)
            check_response(
                srv.request(http_request("POST", "/hooks", b'{"one":1}')),
                201,
                oracle.created_body(1),
            )
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}")
            expect(out == oracle.stdout_lines(port, 1), f"stdout {out!r}")
            expect(sorted(read_inbox(work / "inbox")) == ["000001.json"], "exactly one event")

    suite.check("max-requests 1 serves exactly one request", request_limit_one)

    def unlimited_runs_until_killed() -> None:
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=0)
            expect(srv.proc.poll() is None, "--max-requests 0 must keep running")
            check_response(
                srv.request(http_request("POST", "/hooks", b'{"a":1}')),
                201,
                oracle.created_body(1),
            )
            expect(srv.proc.poll() is None, "--max-requests 0 must keep running after a request")
            srv.kill()

    suite.check("--max-requests 0 keeps running until killed", unlimited_runs_until_killed)

    def absent_limit_runs_until_killed() -> None:
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=None)
            expect(srv.proc.poll() is None, "absent --max-requests must keep running")
            check_response(
                srv.request(http_request("GET", "/health")), 200, oracle.health_body(0)
            )
            expect(srv.proc.poll() is None, "server must stay up")
            srv.kill()

    suite.check("absent --max-requests keeps running until killed", absent_limit_runs_until_killed)

    def stdout_shape() -> None:
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=2)
            srv.request(http_request("GET", "/health"))
            srv.request(http_request("GET", "/health"))
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}")
            expect(out.count(b"\r\n") == 2, f"stdout must be two CRLF lines: {out!r}")
            expect(b"\n" not in out.replace(b"\r\n", b""), "stdout has a bare LF")
            expect(out.startswith(b"listening port="), "stdout must start with the listening line")
            expect(err == b"", f"stderr {err!r}")

    suite.check("stdout is exactly the two CRLF status lines", stdout_shape)

    # --- Persistence and restart recovery ---------------------------------
    def sequence_recovery() -> None:
        with workdir() as work:
            inbox = work / "inbox"
            inbox.mkdir()
            (inbox / "000003.json").write_bytes(oracle.event_file(3, b'{"n":3}'))
            (inbox / "000007.json").write_bytes(oracle.event_file(7, b'{"n":7}'))
            (inbox / "notes.txt").write_bytes(b"ignore me")
            (inbox / "000008.txt").write_bytes(b"not an event")
            (inbox / "1.json").write_bytes(b'{"seq":1,"body":1}\n')
            srv, port = run_server(exe, work, max_requests=3)
            check_response(
                srv.request(http_request("POST", "/hooks", b'{"n":8}')),
                201,
                oracle.created_body(8),
            )
            check_response(
                srv.request(http_request("GET", "/health")), 200, oracle.health_body(3)
            )
            check_response(
                srv.request(http_request("GET", "/events?limit=10")),
                200,
                oracle.events_payload(
                    [
                        oracle.event_file(3, b'{"n":3}'),
                        oracle.event_file(7, b'{"n":7}'),
                        oracle.event_file(8, b'{"n":8}'),
                    ]
                ),
                content_type=oracle.NDJSON_TYPE,
            )
            code, out, err = srv.finish()
            expect(code == 0, f"exit {code}; stderr={err!r}")
            files = read_inbox(inbox)
            expect(
                sorted(files)
                == [
                    "000003.json",
                    "000007.json",
                    "000008.json",
                    "000008.txt",
                    "1.json",
                    "notes.txt",
                ],
                f"unexpected inbox contents: {sorted(files)}",
            )
            expect(files["000003.json"] == oracle.event_file(3, b'{"n":3}'), "existing event changed")
            expect(files["000007.json"] == oracle.event_file(7, b'{"n":7}'), "existing event changed")
            expect(files["000008.json"] == oracle.event_file(8, b'{"n":8}'), "recovered event")

    suite.check("sequence recovery: max(existing)+1, non-events ignored", sequence_recovery)

    def second_run_continues_numbering() -> None:
        with workdir() as work:
            srv1, port1 = run_server(exe, work, max_requests=1)
            check_response(
                srv1.request(http_request("POST", "/hooks", b'{"run":1}')),
                201,
                oracle.created_body(1),
            )
            code1, out1, _ = srv1.finish()
            expect(code1 == 0, f"first run exit {code1}")
            first = read_inbox(work / "inbox")["000001.json"]
            srv2, port2 = run_server(exe, work, max_requests=1)
            check_response(
                srv2.request(http_request("POST", "/hooks", b'{"run":2}')),
                201,
                oracle.created_body(2),
            )
            code2, out2, _ = srv2.finish()
            expect(code2 == 0, f"second run exit {code2}")
            files = read_inbox(work / "inbox")
            expect(files["000001.json"] == first, "first run's event was overwritten")
            expect(files["000002.json"] == oracle.event_file(2, b'{"run":2}'), "second event")

    suite.check("a second run in the same inbox continues numbering", second_run_continues_numbering)

    def repeated_runs_deterministic() -> None:
        snapshots = []
        for _ in range(3):
            with workdir() as work:
                srv, port = run_server(exe, work, max_requests=3)
                srv.request(http_request("POST", "/hooks", body("object.json")))
                srv.request(http_request("POST", "/hooks", body("nested.json")))
                srv.request(http_request("GET", "/events?limit=2"))
                code, out, err = srv.finish()
                expect(code == 0, f"exit {code}")
                snapshots.append(
                    (
                        out.replace(str(port).encode("ascii"), b"{port}"),
                        read_inbox(work / "inbox"),
                    )
                )
        for stdout, files in snapshots[1:]:
            expect(stdout == snapshots[0][0], "stdout changed across identical runs")
            expect(files == snapshots[0][1], "event bytes changed across identical runs")

    suite.check("three identical runs are byte-identical", repeated_runs_deterministic)

    # --- Failure cleanup ---------------------------------------------------
    def write_failure() -> None:
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=1)
            (work / "inbox").rename(work / "inbox-moved")
            check_response(
                srv.request(http_request("POST", "/hooks", b'{"a":1}')),
                500,
                oracle.DOCUMENTED_BODIES[500],
            )
            code, out, err = srv.finish()
            expect(code == 4, f"write failure must exit 4, got {code}; stderr={err!r}")
            expect(out == oracle.stdout_lines(port, None), f"stdout {out!r}")
            expect(b"cannot write event" in err, f"stderr {err!r}")
            expect(read_inbox(work / "inbox-moved") == {}, "a failed write left a partial event file")
            expect_listener_closed(port)

    suite.check("event write failure -> 500, exit 4, no partial event", write_failure)

    def events_read_failure() -> None:
        with workdir() as work:
            inbox = work / "inbox"
            inbox.mkdir()
            (inbox / "000001.json").write_bytes(oracle.event_file(1, b'{"a":1}'))
            srv, port = run_server(exe, work, max_requests=1)
            (inbox / "000001.json").unlink()
            check_response(
                srv.request(http_request("GET", "/events?limit=1")),
                500,
                oracle.DOCUMENTED_BODIES[500],
            )
            code, out, err = srv.finish()
            expect(code == 4, f"read failure must exit 4, got {code}; stderr={err!r}")
            expect(out == oracle.stdout_lines(port, None), f"stdout {out!r}")
            expect(err.startswith(b"hookbox: "), f"stderr {err!r}")
            expect_listener_closed(port)

    suite.check("event read failure during /events -> 500, exit 4", events_read_failure)

    def cleanup_after_success() -> None:
        with workdir() as work:
            srv, port = run_server(exe, work, max_requests=1)
            srv.request(http_request("GET", "/health"))
            srv.finish()
            left = [p.name for p in (work / "inbox").iterdir()]
            expect(left == [], f"successful run left files behind: {left}")

    suite.check("no probe or temp file remains after a clean run", cleanup_after_success)

    # --- Standalone distribution ------------------------------------------
    def standalone_outside_tree() -> None:
        with tempfile.TemporaryDirectory(prefix="hookbox_bin_") as bd:
            with tempfile.TemporaryDirectory(prefix="hookbox_cwd_") as cd:
                bin_dir = Path(bd)
                cwd = Path(cd)
                exe2 = bin_dir / "hookbox.exe"
                shutil.copy2(exe, exe2)
                srv, port = run_server(exe2, cwd, max_requests=2)
                check_response(
                    srv.request(http_request("POST", "/hooks", b'{"standalone":true}')),
                    201,
                    oracle.created_body(1),
                )
                check_response(
                    srv.request(http_request("GET", "/health")),
                    200,
                    oracle.health_body(1),
                )
                code, out, err = srv.finish()
                expect(code == 0, f"exit {code}; stderr={err!r}")
                expect(out == oracle.stdout_lines(port, 2), f"stdout {out!r}")
                expect(err == b"", f"stderr {err!r}")
                files = read_inbox(cwd / "inbox")
                expect(
                    files["000001.json"] == oracle.event_file(1, b'{"standalone":true}'),
                    "standalone event bytes",
                )
                expect(
                    not (bin_dir / "inbox").exists(),
                    "hookbox created the inbox next to the executable, not in cwd",
                )

    suite.check("standalone exe outside the source tree", standalone_outside_tree)

    def absolute_inbox_path() -> None:
        with tempfile.TemporaryDirectory(prefix="hookbox_cwd_") as cd:
            with tempfile.TemporaryDirectory(prefix="hookbox_inbox_") as idir:
                cwd = Path(cd)
                inbox = Path(idir) / "events"
                srv, port = run_server(exe, cwd, max_requests=1, inbox=str(inbox))
                check_response(
                    srv.request(http_request("POST", "/hooks", b'{"abs":1}')),
                    201,
                    oracle.created_body(1),
                )
                code, out, err = srv.finish()
                expect(code == 0, f"exit {code}; stderr={err!r}")
                expect(
                    read_inbox(inbox)["000001.json"] == oracle.event_file(1, b'{"abs":1}'),
                    "absolute inbox event bytes",
                )
                expect(not (cwd / "events").exists(), "absolute inbox was not used")

    suite.check("absolute inbox path is created and used", absolute_inbox_path)

    return suite


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--no-build", action="store_true", help="use the existing exe")
    parser.add_argument(
        "--update-goldens", action="store_true", help="rewrite expected/ from the oracle"
    )
    parser.add_argument("--only", help="run only checks whose name contains this text")
    args = parser.parse_args()

    if not args.no_build:
        build_exe()
    expect(EXE.exists(), f"missing {EXE}; run without --no-build")

    if args.update_goldens:
        update_goldens(EXE)

    suite = run_suite(EXE, only=args.only)
    total = suite.passed + len(suite.failed) + len(suite.skipped)
    print()
    print(
        f"summary: {suite.passed} passed, {len(suite.failed)} failed, "
        f"{len(suite.skipped)} skipped, {total} checks"
    )
    if suite.failed:
        print("\nfailures:")
        for name, message in suite.failed:
            print(f"  - {name}: {message}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
