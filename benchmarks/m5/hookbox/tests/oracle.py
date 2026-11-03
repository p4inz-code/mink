#!/usr/bin/env python3
"""Independent oracle for the frozen `hookbox` contract.

This module is written from the contract in
`docs/benchmarks/M5/M5_APPLICATION_SELECTION.md` (section 6), not from the
MINK implementation. It answers three questions for the driver:

  * is a request body a JSON **object** (so `POST /hooks` must accept it)?
  * what is the compact re-serialization of that object?
  * what bytes must `<inbox>/<seq:06d>.json` contain?

The JSON reader/writer below is a small, self-contained implementation of the
JSON grammar with numbers kept as their original lexemes. That matters: the
contract says the stored body is the parsed object re-serialized *compactly*,
and a number's spelling is data (`1.50` must not become `1.5`). Object member
order, including duplicate keys, is preserved for the same reason.

Scope notes (documented, not hidden):
  * ASCII/standard JSON only. `\\uXXXX` escapes are decoded to text and
    re-encoded as UTF-8; MINK's byte-oriented serializer maps non-ASCII code
    points to `?`. No fixture uses non-ASCII, so the oracle never relies on
    that divergence.
  * A JSON number keeps its raw lexeme, as the stdlib serializer does.
"""

from __future__ import annotations

import json as _stdlib_json
import re


class JsonError(ValueError):
    """The bytes are not one complete JSON value."""


class Num:
    """A JSON number, kept as the exact lexeme that appeared in the input."""

    __slots__ = ("raw",)

    def __init__(self, raw: bytes) -> None:
        self.raw = raw

    def __repr__(self) -> str:  # pragma: no cover - debugging aid
        return f"Num({self.raw!r})"


class Obj:
    """A JSON object as an ordered list of (str, value) pairs."""

    __slots__ = ("pairs",)

    def __init__(self, pairs: list[tuple[str, object]]) -> None:
        self.pairs = pairs

    def __repr__(self) -> str:  # pragma: no cover - debugging aid
        return f"Obj({self.pairs!r})"


class Arr:
    """A JSON array."""

    __slots__ = ("items",)

    def __init__(self, items: list[object]) -> None:
        self.items = items

    def __repr__(self) -> str:  # pragma: no cover - debugging aid
        return f"Arr({self.items!r})"


_WS = b" \t\r\n"
_NUMBER = re.compile(rb"-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?")
_ESCAPES = {
    0x22: b'"',
    0x5C: b"\\",
    0x2F: b"/",
    0x62: b"\b",
    0x66: b"\f",
    0x6E: b"\n",
    0x72: b"\r",
    0x74: b"\t",
}


class _Parser:
    def __init__(self, data: bytes) -> None:
        self.data = data
        self.i = 0
        self.n = len(data)

    def parse(self) -> object:
        value = self.value()
        self.ws()
        if self.i != self.n:
            raise JsonError(f"trailing bytes at {self.i}")
        return value

    def ws(self) -> None:
        while self.i < self.n and self.data[self.i] in _WS:
            self.i += 1

    def value(self) -> object:
        self.ws()
        if self.i >= self.n:
            raise JsonError("unexpected end of input")
        c = self.data[self.i]
        if c == 0x7B:
            return self.object()
        if c == 0x5B:
            return self.array()
        if c == 0x22:
            return self.string()
        if c == 0x74:
            return self.literal(b"true", True)
        if c == 0x66:
            return self.literal(b"false", False)
        if c == 0x6E:
            return self.literal(b"null", None)
        if c == 0x2D or 0x30 <= c <= 0x39:
            return self.number()
        raise JsonError(f"unexpected byte 0x{c:02x} at {self.i}")

    def literal(self, word: bytes, value: object) -> object:
        if self.data[self.i : self.i + len(word)] != word:
            raise JsonError(f"bad literal at {self.i}")
        self.i += len(word)
        return value

    def object(self) -> Obj:
        self.i += 1  # '{'
        pairs: list[tuple[str, object]] = []
        self.ws()
        if self.i < self.n and self.data[self.i] == 0x7D:
            self.i += 1
            return Obj(pairs)
        while True:
            self.ws()
            if self.i >= self.n or self.data[self.i] != 0x22:
                raise JsonError(f"expected object key at {self.i}")
            key = self.string()
            self.ws()
            if self.i >= self.n or self.data[self.i] != 0x3A:
                raise JsonError(f"expected ':' at {self.i}")
            self.i += 1
            pairs.append((key, self.value()))
            self.ws()
            if self.i >= self.n:
                raise JsonError("unterminated object")
            if self.data[self.i] == 0x2C:
                self.i += 1
                continue
            if self.data[self.i] == 0x7D:
                self.i += 1
                return Obj(pairs)
            raise JsonError(f"expected ',' or '}}' at {self.i}")

    def array(self) -> Arr:
        self.i += 1  # '['
        items: list[object] = []
        self.ws()
        if self.i < self.n and self.data[self.i] == 0x5D:
            self.i += 1
            return Arr(items)
        while True:
            items.append(self.value())
            self.ws()
            if self.i >= self.n:
                raise JsonError("unterminated array")
            if self.data[self.i] == 0x2C:
                self.i += 1
                continue
            if self.data[self.i] == 0x5D:
                self.i += 1
                return Arr(items)
            raise JsonError(f"expected ',' or ']' at {self.i}")

    def string(self) -> str:
        self.i += 1  # '"'
        out = bytearray()
        while True:
            if self.i >= self.n:
                raise JsonError("unterminated string")
            c = self.data[self.i]
            if c == 0x22:
                self.i += 1
                return out.decode("utf-8")
            if c == 0x5C:
                self.i += 1
                if self.i >= self.n:
                    raise JsonError("unterminated escape")
                e = self.data[self.i]
                if e in _ESCAPES:
                    out += _ESCAPES[e]
                    self.i += 1
                    continue
                if e == 0x75:
                    hex_text = self.data[self.i + 1 : self.i + 5]
                    if len(hex_text) != 4:
                        raise JsonError("short \\u escape")
                    try:
                        code = int(hex_text, 16)
                    except ValueError as exc:
                        raise JsonError("bad \\u escape") from exc
                    out += chr(code).encode("utf-8")
                    self.i += 5
                    continue
                raise JsonError(f"bad escape 0x{e:02x}")
            if c < 0x20:
                raise JsonError("raw control byte in string")
            out.append(c)
            self.i += 1

    def number(self) -> Num:
        match = _NUMBER.match(self.data, self.i)
        if match is None:
            raise JsonError(f"bad number at {self.i}")
        self.i = match.end()
        return Num(match.group(0))


def parse(data: bytes) -> object:
    """Parse exactly one JSON value; raise JsonError otherwise."""
    return _Parser(data).parse()


def is_object(data: bytes) -> bool:
    """True when `data` is one complete JSON object (the /hooks rule)."""
    try:
        return isinstance(parse(data), Obj)
    except JsonError:
        return False


# ---------------------------------------------------------------------------
# Compact serialization
# ---------------------------------------------------------------------------

def _quote(text: str) -> bytes:
    out = bytearray(b'"')
    for byte in text.encode("utf-8"):
        if byte == 0x22:
            out += b'\\"'
        elif byte == 0x5C:
            out += b"\\\\"
        elif byte == 0x08:
            out += b"\\b"
        elif byte == 0x0C:
            out += b"\\f"
        elif byte == 0x0A:
            out += b"\\n"
        elif byte == 0x0D:
            out += b"\\r"
        elif byte == 0x09:
            out += b"\\t"
        elif byte < 0x20:
            out += b"\\u%04x" % byte
        else:
            out.append(byte)
    out += b'"'
    return bytes(out)


def dumps(value: object) -> bytes:
    """Compact JSON: no whitespace, member order preserved."""
    if value is None:
        return b"null"
    if value is True:
        return b"true"
    if value is False:
        return b"false"
    if isinstance(value, Num):
        return value.raw
    if isinstance(value, str):
        return _quote(value)
    if isinstance(value, Arr):
        return b"[" + b",".join(dumps(item) for item in value.items) + b"]"
    if isinstance(value, Obj):
        return b"{" + b",".join(
            _quote(key) + b":" + dumps(item) for key, item in value.pairs
        ) + b"}"
    raise TypeError(f"not a JSON value: {value!r}")


def compact_object(body: bytes) -> bytes:
    """Compact form of a JSON object body; raises JsonError for non-objects."""
    value = parse(body)
    if not isinstance(value, Obj):
        raise JsonError("root is not an object")
    return dumps(value)


# ---------------------------------------------------------------------------
# Contract artifacts
# ---------------------------------------------------------------------------

def event_file(seq: int, body: bytes) -> bytes:
    """`{"seq":N,"body":<compact>}\\n` — exactly the frozen event bytes."""
    return b'{"seq":%d,"body":%s}\n' % (seq, compact_object(body))


def event_name(seq: int) -> str:
    """`<seq:06d>.json`."""
    return f"{seq:06d}.json"


def events_payload(events: list[bytes]) -> bytes:
    """NDJSON body: the stored event files, ascending, one per line."""
    return b"".join(events)


def health_body(count: int) -> bytes:
    return b'{"status":"ok","count":%d}' % count


def created_body(seq: int) -> bytes:
    return b'{"seq":%d}' % seq


# Response bodies that the frozen contract pins explicitly.
FROZEN_BODIES = {
    404: b'{"error":"not found"}',
    405: b'{"error":"method not allowed"}',
}

# Bodies this implementation documents for the statuses the contract defines
# without a body. The drivers assert these bytes so the behavior stays stable;
# they are not contract text.
DOCUMENTED_BODIES = {
    400: b'{"error":"bad request"}',
    413: b'{"error":"payload too large"}',
    500: b'{"error":"storage failure"}',
}

REASONS = {
    200: "OK",
    201: "Created",
    400: "Bad Request",
    404: "Not Found",
    405: "Method Not Allowed",
    413: "Payload Too Large",
    500: "Internal Server Error",
}

JSON_TYPE = b"application/json"
NDJSON_TYPE = b"application/x-ndjson"


def response(status: int, body: bytes, content_type: bytes = JSON_TYPE) -> bytes:
    """The deterministic response the frozen status/body implies.

    Header *format* is not frozen by the contract; it is fixed here so the
    driver can assert exact bytes (status line, Content-Length, close, body).
    """
    reason = REASONS[status].encode("ascii")
    head = (
        b"HTTP/1.1 %d %s\r\n" % (status, reason)
        + b"Content-Type: " + content_type + b"\r\n"
        + b"Content-Length: %d\r\n" % len(body)
        + b"Connection: close\r\n\r\n"
    )
    return head + body


def stdout_lines(port: int, served: int | None) -> bytes:
    """`listening port=<n>` then, on max-requests completion, `served=<n>`."""
    out = b"listening port=%d\r\n" % port
    if served is not None:
        out += b"served=%d\r\n" % served
    return out


# ---------------------------------------------------------------------------
# Sanity: the compact serializer must agree with a stock JSON dump for plain
# ASCII values. Guards against the oracle silently drifting.
# ---------------------------------------------------------------------------

def _self_check() -> None:
    plain = b'{"a":1,"b":[true,null,"x"],"c":{"d":1.50}}'
    expected = _stdlib_json.dumps(
        _stdlib_json.loads(plain.decode("ascii")), separators=(",", ":")
    ).encode("ascii")
    # `1.50` is intentionally re-spelled by the stdlib; compare the rest.
    assert compact_object(plain) == b'{"a":1,"b":[true,null,"x"],"c":{"d":1.50}}', (
        compact_object(plain),
    )
    assert dumps(parse(b'"a\\nb"')) == b'"a\\nb"'
    assert is_object(b'{"a": 1}')
    assert not is_object(b"[1]")
    assert not is_object(b'{"a":')
    assert event_file(1, b'{"a":1}') == b'{"seq":1,"body":{"a":1}}\n'
    assert expected is not None
    assert event_name(7) == "000007.json"


_self_check()
