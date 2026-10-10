#!/usr/bin/env python3
"""Python reference implementation of the A2 `treeops` utility.

Mirrors `validation/a2_treeops/main.mink` behaviour byte-for-byte:

    treeops <src-dir> <dst-dir> [--ext .a,.b] [--move] [--dry-run] [--limit N]

Deliberate mirrors of MINK conventions (see record.md):
  * `rt_print_str` appends CRLF without translating embedded newlines, so
    the reference writes raw bytes terminated with `\r\n`;
  * `rt_argc`/`rt_argv` exclude the program name (argv[0] is the first
    user argument here too);
  * an unknown `--flag` is treated as a positional argument (MINK has no
    special handling for unknown options);
  * `--limit` is applied after sorting, so the selection is deterministic.
"""
import os
import sys

DEFAULT_EXT = ".log,.txt,.mink,.json,.csv"


def w(buf: bytearray, text: str) -> None:
    buf.extend(text.encode("utf-8") + b"\r\n")


def emit(prefix: str, rel: str, size: int, dry: int, out: bytearray) -> None:
    tag = "would process " if dry else "processed "
    out.extend((tag + rel + " " + str(size)).encode("utf-8") + b"\r\n")


def ext_selected(path: str, ext_list: str) -> bool:
    """Extension of `path` (lower-cased, including the dot) in `ext_list`."""
    ext = os.path.splitext(path)[1]
    if not ext:
        return False
    parts = ext_list.split(",")
    for part in parts:
        if part and part.lower() == ext.lower():
            return True
    return False


def walk(src_dir: str, out_dir: str, ext_list: str, state: dict,
         matches: list) -> None:
    """Recursively collect matching files; `state` holds the counters."""
    try:
        names = os.listdir(src_dir)
    except OSError:
        sys.stderr.buffer.write(b"treeops: cannot open directory\r\n")
        state["walk_failed"] = 1
        return
    for name in names:
        if name in (".", ".."):
            continue
        full = src_dir + "/" + name if src_dir else name
        if os.path.isdir(full):
            walk(full, out_dir, ext_list, state, matches)
        elif os.path.isfile(full):
            if ext_selected(full, ext_list):
                matches.append(full)
            else:
                state["skipped"] += 1
        else:
            # Not a regular file/dir: MINK counts it as a non-match too.
            if ext_selected(full, ext_list):
                matches.append(full)
            else:
                state["skipped"] += 1


def transfer(full: str, rel: str, dst: str, move_it: int, dry: int,
             state: dict, out: bytearray) -> None:
    try:
        size = os.path.getsize(full)
    except OSError:
        state["skipped"] += 1
        return
    target = dst + "/" + rel
    parent = os.path.dirname(target)
    if parent:
        os.makedirs(parent, exist_ok=True)
    moved = 0
    if not dry:
        try:
            if move_it:
                if os.path.exists(target):
                    os.remove(target)
                os.rename(full, target)
            else:
                os.makedirs(parent, exist_ok=True)
                with open(full, "rb") as src, open(target, "wb") as dstf:
                    dstf.write(src.read())
        except OSError:
            moved = -1
    emit("", rel, size, dry, out)
    state["matched"] += 1
    state["bytes"] += size
    if moved == -1:
        state["errors"] += 1


def main(argv: list) -> int:
    argc = len(argv)
    src_index = -1
    dst_index = -1
    ext_list = DEFAULT_EXT
    move_it = 0
    dry = 0
    limit = 1000000

    out = bytearray()
    err = bytearray()

    i = 0
    bad = 0
    while i < argc:
        arg = argv[i]
        next_i = i + 1
        if arg == "--ext":
            if i + 1 >= argc:
                bad = 1
            else:
                ext_list = argv[i + 1]
                next_i = i + 2
        elif arg == "--move":
            move_it = 1
        elif arg == "--dry-run":
            dry = 1
        elif arg == "--limit":
            if i + 1 >= argc:
                bad = 1
            else:
                value = argv[i + 1]
                try:
                    n = int(value, 10)
                    ok = str(n) == value or value.strip("-+").isdigit()
                except ValueError:
                    n, ok = 0, False
                if ok and n > 0:
                    limit = n
                next_i = i + 2
        else:
            if src_index < 0:
                src_index = i
            elif dst_index < 0:
                dst_index = i
            else:
                bad = 2
        i = next_i

    if bad == 1:
        w(err, "treeops: missing value for an option")
        sys.stderr.buffer.write(bytes(err))
        return 1
    if bad == 2:
        w(err, "treeops: too many positional arguments")
        sys.stderr.buffer.write(bytes(err))
        return 1
    usage = ("usage: treeops <src> <dst> [--ext .a,.b] [--move] "
             "[--dry-run] [--limit N]")
    if src_index < 0 or dst_index < 0:
        w(err, "treeops: " + usage)
        sys.stderr.buffer.write(bytes(err))
        return 1

    src = argv[src_index]
    dst = argv[dst_index]
    code = 0
    if not os.path.exists(src):
        w(err, "treeops: source does not exist: ")
        sys.stderr.buffer.write(bytes(err))
        code = 2
    if code == 0 and not os.path.isdir(src):
        w(err, "treeops: source is not a directory: ")
        sys.stderr.buffer.write(bytes(err))
        code = 2
    if code == 0:
        if not os.path.exists(dst):
            try:
                os.makedirs(dst, exist_ok=True)
            except OSError:
                code = 3
        elif not os.path.isdir(dst):
            code = 3

    state = {"matched": 0, "bytes": 0, "skipped": 0, "errors": 0,
             "walk_failed": 0}

    if code == 0:
        matches: list = []
        walk(src, dst, ext_list, state, matches)
        matches.sort()
        prefix_len = len(src)
        if prefix_len and not src.endswith("/"):
            prefix_len += 1
        n = len(matches)
        emit_n = n if limit >= n else limit
        for k in range(emit_n):
            full = matches[k]
            rel = full[prefix_len:]
            transfer(full, rel, dst, move_it, dry, state, out)

    if code == 0 and state["walk_failed"] == 1:
        code = 3
    if code == 0 and state["matched"] == 0:
        code = 4

    w(out, "matched: " + str(state["matched"]))
    w(out, "bytes: " + str(state["bytes"]))
    w(out, "skipped: " + str(state["skipped"]))
    w(out, "errors: " + str(state["errors"]))
    sys.stdout.buffer.write(bytes(out))
    sys.stdout.buffer.flush()
    return code


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
