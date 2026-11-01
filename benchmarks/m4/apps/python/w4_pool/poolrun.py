#!/usr/bin/env python3
"""Python reference implementation of the G `poolrun` utility.

Mirrors `validation/g_threads/main.mink` byte-for-byte:

    poolrun squares  <n> <workers>
    poolrun pipeline <n> <workers>

Both modes split 1..n into `workers` disjoint ranges with the same integer
division the MINK app uses, so every subtotal and the final total are
deterministic. Deliberate mirrors of MINK conventions (see record.md):
  * `rt_print_str` appends CRLF, so every line is written with an explicit
    `b"\\r\\n"`;
  * `rt_argc`/`rt_argv` exclude the program name; any argument starting with
    `-` is a usage error (the tool has no options);
  * results are printed in join order (subtotal[0] first), never in
    completion order.
"""
import sys
import threading

USAGE = "usage: poolrun <squares|pipeline> <n> <workers>"


def w_err(text: str) -> None:
    sys.stderr.buffer.write(text.encode("utf-8") + b"\r\n")


def w_line(text: str) -> None:
    sys.stdout.buffer.write(text.encode("utf-8") + b"\r\n")


def fail_msg(label: str) -> int:
    w_err("poolrun: " + label)
    return 1


def ranges_for(n: int, workers: int):
    chunk = n // workers
    parts = []
    for k in range(workers):
        start = k * chunk + 1
        end = (k + 1) * chunk
        if k == workers - 1:
            end = n
        parts.append((start, end))
    return parts


def expected_squares(n: int) -> int:
    return n * (n + 1) * (2 * n + 1) // 6


def run_squares(n: int, workers: int) -> int:
    parts = ranges_for(n, workers)
    lock = threading.Lock()
    accumulator = [0]
    subtotals = [0] * workers

    def worker(k: int, start: int, end: int) -> None:
        subtotal = sum(i * i for i in range(start, end + 1))
        with lock:
            accumulator[0] += subtotal
        subtotals[k] = subtotal

    threads = [threading.Thread(target=worker, args=(k, start, end))
               for k, (start, end) in enumerate(parts)]
    for thread in threads:
        thread.start()

    w_line("workers=" + str(workers))
    w_line("n=" + str(n))
    joined = 0
    for k, thread in enumerate(threads):
        thread.join()
        joined += subtotals[k]
        w_line("subtotal[" + str(k) + "]=" + str(subtotals[k]))

    expected = expected_squares(n)
    w_line("locked-total=" + str(accumulator[0]))
    w_line("joined-total=" + str(joined))
    w_line("expected=" + str(expected))
    if accumulator[0] != expected or joined != expected:
        return 3
    return 0


def run_pipeline(n: int, workers: int) -> int:
    parts = ranges_for(n, workers)
    slots = [0] * workers

    def mapper(k: int, start: int, end: int) -> None:
        slots[k] = sum(i * i for i in range(start, end + 1))

    threads = [threading.Thread(target=mapper, args=(k, start, end))
               for k, (start, end) in enumerate(parts)]
    for thread in threads:
        thread.start()

    w_line("workers=" + str(workers))
    w_line("n=" + str(n))
    for thread in threads:
        thread.join()  # join barrier: stage 1 completes before stage 2
    for k, value in enumerate(slots):
        w_line("stage1[" + str(k) + "]=" + str(value))

    total = sum(slots)  # stage 2: the reducer thread in the MINK app
    expected = expected_squares(n)
    w_line("total=" + str(total))
    w_line("expected=" + str(expected))
    if total != expected:
        return 3
    return 0


def main(argv) -> int:
    for arg in argv:
        if arg.startswith("-"):
            fail_msg("this tool has no options")
            return 1
    if len(argv) != 3:
        fail_msg(USAGE)
        return 1

    mode, count_text, workers_text = argv
    try:
        count = int(count_text, 10)
        workers = int(workers_text, 10)
    except ValueError:
        return fail_msg(USAGE)
    if count <= 0 or workers <= 0:
        return fail_msg(USAGE)

    if mode == "squares":
        return run_squares(count, workers)
    if mode == "pipeline":
        return run_pipeline(count, workers)
    return fail_msg(USAGE)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
