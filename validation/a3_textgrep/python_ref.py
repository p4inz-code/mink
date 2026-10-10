#!/usr/bin/env python3
"""Python reference implementation of the A3 `textgrep` utility.

Mirrors `validation/a3_textgrep/main.mink` byte-for-byte:

    textgrep [options] <pattern> <file>

Deliberate mirrors of MINK conventions (see record.md):
  * lines are split on LF only, and a trailing LF does not create an extra
    empty line (the MINK reference uses the stdlib `str_split` + drop-trailing);
  * `rt_print_str` appends CRLF without translating embedded newlines, so the
    reference writes raw bytes terminated with `b"\\r\\n"`;
  * `rt_argc`/`rt_argv` exclude the program name (argv[0] is the first user
    argument here too);
  * matching is the byte-oriented `re` subset, and `--ignore-case` lowers the
    pattern and the subject with ASCII case folding (`bytes.lower()`), which is
    exactly what the MINK reference does with `str_to_lower`;
  * regex errors cannot happen for the supported subset; unsupported patterns
    are escaped so the reference still runs (documented, not exercised).
"""
import os
import re
import sys

USAGE = "usage: textgrep [options] <pattern> <file>"


def w_err(text: str) -> None:
    sys.stderr.buffer.write(text.encode("utf-8") + b"\r\n")


def fail_file(label: str, path: str) -> None:
    w_err("textgrep: " + label + path)


def write_bytes(path: str, data: bytes) -> int:
    """Mirror `fs_write`: bytes written, or -1 on failure."""
    try:
        with open(path, "wb") as handle:
            handle.write(data)
        return len(data)
    except OSError:
        return -1


def parse_positive(value: str):
    """Mirror `str_parse_int` for the option-value subset we accept."""
    try:
        number = int(value, 10)
    except ValueError:
        return None
    return number


def main(argv) -> int:
    argc = len(argv)
    pattern_index = -1
    file_index = -1
    out_index = -1
    numbered = 0
    invert = 0
    count_mode = 0
    ignore_case = 0
    max_count = 0
    bad = 0

    i = 0
    while i < argc:
        arg = argv[i]
        next_i = i + 1
        if arg in ("-n", "--line-number"):
            numbered = 1
        elif arg in ("-v", "--invert"):
            invert = 1
        elif arg in ("-c", "--count"):
            count_mode = 1
        elif arg in ("-i", "--ignore-case"):
            ignore_case = 1
        elif arg in ("-m", "--max-count"):
            if i + 1 >= argc:
                bad = 1
            else:
                number = parse_positive(argv[i + 1])
                if number is None or number <= 0:
                    bad = 1
                else:
                    max_count = number
                next_i = i + 2
        elif arg in ("-o", "--output"):
            if i + 1 >= argc:
                bad = 1
            else:
                out_index = i + 1
                next_i = i + 2
        else:
            if pattern_index < 0:
                pattern_index = i
            elif file_index < 0:
                file_index = i
            else:
                bad = 2
        i = next_i

    if bad == 1:
        w_err("textgrep: missing or invalid value for an option")
        return 1
    if bad == 2:
        w_err("textgrep: too many positional arguments")
        return 1
    if pattern_index < 0 or file_index < 0:
        w_err("textgrep: " + USAGE)
        return 1

    pattern = argv[pattern_index]
    filepath = argv[file_index]
    code = 0
    if not os.path.exists(filepath):
        fail_file("input file does not exist: ", filepath)
        code = 2
    if code == 0 and not os.path.isfile(filepath):
        fail_file("not a regular file: ", filepath)
        code = 2

    if code == 0:
        with open(filepath, "rb") as handle:
            content = handle.read()
        parts = content.split(b"\n")
        effective = len(parts)
        if effective > 0 and len(parts[-1]) == 0:
            effective -= 1
        lines = parts[:effective]

        pat = pattern.encode("utf-8")
        if ignore_case:
            pat = pat.lower()
        try:
            rx = re.compile(pat)
        except re.error:
            rx = re.compile(re.escape(pat))

        selected = []
        for k in range(len(lines)):
            subject = lines[k]
            if ignore_case:
                subject = subject.lower()
            hit = rx.search(subject) is not None
            keep = (not hit) if invert else hit
            if keep:
                if numbered:
                    selected.append(str(k + 1).encode("utf-8") + b":" + lines[k])
                else:
                    selected.append(lines[k])
                if max_count > 0 and len(selected) >= max_count:
                    break

        count = len(selected)
        code = 0 if count > 0 else 3

        if count_mode:
            data = str(count).encode("utf-8")
            if out_index >= 0:
                wrote = write_bytes(argv[out_index], data + b"\n")
                if wrote < 0:
                    fail_file("cannot write output file: ", argv[out_index])
                    code = 4
            else:
                sys.stdout.buffer.write(data + b"\r\n")
        elif out_index >= 0:
            body = b"\n".join(selected)
            if body:
                body += b"\n"
            wrote = write_bytes(argv[out_index], body)
            if wrote < 0:
                fail_file("cannot write output file: ", argv[out_index])
                code = 4
        else:
            for line in selected:
                sys.stdout.buffer.write(line + b"\r\n")
        sys.stdout.buffer.flush()

    return code


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
