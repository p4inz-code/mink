#!/usr/bin/env python3
"""Create the A3 `textgrep` fixtures.

Run from anywhere:

    python validation/a3_textgrep/fixtures/make_fixtures.py

Writes into `validation/a3_textgrep/fixtures/`:

  app.log          8 lines (LF), one blank line, mixed levels, digits
  plain.txt        6 lines starting with words (anchors, a comment)
  crlf.txt         3 lines terminated with CRLF
  no_newline.txt   two lines, the last with no trailing LF
  empty.txt        0 bytes
  only_newlines.txt two LF bytes (two empty lines)
  unicode.txt      UTF-8 multibyte content with ASCII markers
"""
import os

HERE = os.path.dirname(os.path.abspath(__file__))

APP_LOG = (
    b"2026-10-01T00:00:00Z INFO  worker started pid=1000\n"
    b"2026-10-01T00:00:01Z DEBUG reading config\n"
    b"2026-10-01T00:00:02Z INFO  connected to db host=localhost\n"
    b"2026-10-01T00:00:03Z WARN  retry 1 of 3\n"
    b"2026-10-01T00:00:04Z ERROR failed to fetch rows code=42\n"
    b"\n"
    b"2026-10-01T00:00:05Z ERROR failed again code=43\n"
    b"2026-10-01T00:00:06Z INFO  done\n"
)

PLAIN = (
    b"INFO starting\n"
    b"INFO still going\n"
    b"warning: careful\n"
    b"ERROR stopped\n"
    b"# comment line\n"
    b"INFO done\n"
)

CRLF = (
    b"alpha error one\r\n"
    b"beta ok\r\n"
    b"Gamma ERROR two\r\n"
)

NO_NEWLINE = (
    b"first line here\n"
    b"error at the end"
)

ONLY_NEWLINES = b"\n\n"

UNICODE = "café naïve\nERROR: ünïcödé\nplain line\n".encode("utf-8")


def main() -> None:
    files = {
        "app.log": APP_LOG,
        "plain.txt": PLAIN,
        "crlf.txt": CRLF,
        "no_newline.txt": NO_NEWLINE,
        "empty.txt": b"",
        "only_newlines.txt": ONLY_NEWLINES,
        "unicode.txt": UNICODE,
    }
    for name, data in files.items():
        with open(os.path.join(HERE, name), "wb") as handle:
            handle.write(data)
    total = sum(len(data) for data in files.values())
    print(f"fixtures: {len(files)} files, {total} bytes under {HERE}")


if __name__ == "__main__":
    main()
