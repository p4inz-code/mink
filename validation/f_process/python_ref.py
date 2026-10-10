#!/usr/bin/env python3
"""Python reference implementation of the F `jobrun` utility.

Mirrors `validation/f_process/main.mink` byte-for-byte:

    jobrun argv <a> [b ...]
    jobrun run <command>
    jobrun runin <command> <input>
    jobrun stamp <epoch> <fmt>
    jobrun env <name>

Deliberate mirrors of MINK conventions (see record.md):
  * `rt_print_str` appends CRLF without translating embedded newlines, so
    every line (and a child's captured stdout/stderr) is written with an
    explicit `b"\\r\\n"`;
  * `rt_argc`/`rt_argv` exclude the program name; any argument starting with
    `-` is a usage error (the tool has no options);
  * `time_strftime` is UTC (`time.gmtime`);
  * there is no exception type: failures are a printed status, not a traceback.
"""
import os
import subprocess
import sys
import time

USAGE = "usage: jobrun <argv|run|runin|stamp|env> ..."


def w_err(text: str) -> None:
    sys.stderr.buffer.write(text.encode("utf-8") + b"\r\n")


def w_line(text: str) -> None:
    sys.stdout.buffer.write(text.encode("utf-8") + b"\r\n")


def fail_msg(label: str) -> int:
    w_err("jobrun: " + label)
    return 1


def ansi_bytes(text: str) -> bytes:
    """Mirror MINK's ANSI command line: `rt_argv` reads the argument bytes
    through the Windows ANSI code page, so a non-ASCII argument arrives as
    codepage bytes (Python's own `sys.argv` is Unicode via GetCommandLineW)."""
    try:
        return text.encode("mbcs")
    except (LookupError, UnicodeEncodeError):
        return text.encode("utf-8")


def cmd_argv(argv) -> int:
    w_line("argc=" + str(len(argv)))
    for index, value in enumerate(argv):
        sys.stdout.buffer.write(b"arg[" + str(index).encode("ascii") + b"]="
                                + ansi_bytes(value) + b"\r\n")
    return 0


def report_child(completed) -> int:
    w_line("exit=" + str(completed.returncode))
    out = completed.stdout
    err = completed.stderr
    w_line("stdout-len=" + str(len(out)))
    sys.stdout.buffer.write(b"stdout=" + out + b"\r\n")
    w_line("stderr-len=" + str(len(err)))
    sys.stdout.buffer.write(b"stderr=" + err + b"\r\n")
    return 0


def cmd_run(command: str) -> int:
    # shell=False mirrors the MINK runtime, which passes the command line to
    # CreateProcessA directly.
    completed = subprocess.run(command, shell=False, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE)
    return report_child(completed)


def cmd_runin(command: str, data: str) -> int:
    completed = subprocess.run(command, shell=False, input=data.encode(),
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return report_child(completed)


def cmd_stamp(epoch_text: str, fmt: str) -> int:
    try:
        epoch = int(epoch_text, 10)
    except ValueError:
        return fail_msg("usage: jobrun stamp <epoch> <fmt>")
    w_line("time=" + time.strftime(fmt, time.gmtime(epoch)))
    return 0


def cmd_env(name: str) -> int:
    raw = os.environ.get(name)
    w_line("value=" + (raw if raw else "(unset)"))
    w_line("has=" + ("true" if name in os.environ else "false"))
    os.environ[name] = "probe-value"
    w_line("after-set=" + os.environ[name])
    del os.environ[name]
    w_line("after-remove-has="
           + ("true" if name in os.environ else "false"))
    return 0


def main(argv) -> int:
    for arg in argv:
        if arg.startswith("-"):
            fail_msg("this tool has no options")
            return 1
    if len(argv) < 1:
        fail_msg(USAGE)
        return 1

    command = argv[0]
    if command == "argv":
        return cmd_argv(argv)
    if command == "run":
        if len(argv) != 2:
            return fail_msg("usage: jobrun run <command>")
        return cmd_run(argv[1])
    if command == "runin":
        if len(argv) != 3:
            return fail_msg("usage: jobrun runin <command> <input>")
        return cmd_runin(argv[1], argv[2])
    if command == "stamp":
        if len(argv) != 3:
            return fail_msg("usage: jobrun stamp <epoch> <fmt>")
        return cmd_stamp(argv[1], argv[2])
    if command == "env":
        if len(argv) != 2:
            return fail_msg("usage: jobrun env <name>")
        return cmd_env(argv[1])
    fail_msg(USAGE)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
