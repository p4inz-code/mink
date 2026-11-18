# M7 Phase E — child-stdin boundary (Windows x86_64)

Date: 2026-11-18. Release compiler built from this tree (`mink 1.0.5`,
unpublished).

## Question

M2 limitation L1 recorded that `process_run_input` delivered 4,096 of the
5,000 bytes offered. Phase E had to decide whether that is a documented API
behaviour or a defect, and whether the API needs a write-all operation.

## Method

`probe.mink` offers 0, 1, 4,095, 4,096, 4,097, 5,000 and 65,536 bytes to
`child.exe`, a MINK program that reads stdin to EOF and prints it back.
`child.exe`'s `rt_print_str` appends CRLF, so `captured = delivered + 2`.
`verify.py` builds both programs, runs the probe and checks the curve against
the documented contract.

```
target/release/mink.exe build validation/phase_e/stdin/probe.mink   # (verify.py does this)
python validation/phase_e/stdin/verify.py
```

## Result

| Offered | Delivered | Child exit | Verdict |
|--------:|----------:|-----------:|---------|
| 0 | 0 | 0 | whole input |
| 1 | 1 | 0 | whole input |
| 4,095 | 4,095 | 0 | whole input |
| 4,096 | 4,096 | 0 | whole input (exactly one pipe buffer) |
| 4,097 | 4,096 | 0 | truncated at the buffer |
| 5,000 | 4,096 | 0 | truncated at the buffer |
| 65,536 | 4,096 | 0 | truncated at the buffer |

`verify.py` exits 0 on this table.

## Analysis

- The truncation is not a buffering accident or a short-write bug: the write
  loop advances by the bytes `WriteFile` reports written and stops at the cap.
  It is the documented `STDIN_PIPE_LIMIT` (4,096, one default pipe buffer).
- The cap exists so the parent never blocks in a synchronous `WriteFile`
  against a child that has not started reading stdin yet. At 4,096 bytes the
  write always fits the pipe buffer and returns; a larger write could deadlock
  against a child that first floods its own stdout. `tests/process_lib.rs`
  pins that non-deadlock property (`s125_child_that_ignores_stdin_does_not_deadlock`,
  8 KiB offered to a child that never reads).
- The child sees EOF after the cap, exactly as from a redirected file, and
  exits 0; the parent's return value is the child's exit code, so truncation
  is not reported.

## Classification and decision

**Intentional design constraint**, not a defect. The API contract in
`stdlib/process.mink` now states that longer input is truncated silently, and
that a file redirect (`process_run("prog < file")`) or several children are
the supported route for larger input.

A write-all operation is **not** added in M7 Phase E: doing it safely needs a
concurrent drain (a feeder thread or overlapped I/O) so a child that writes
before it reads cannot deadlock, plus its own failure and lifecycle tests —
a new concurrency surface, not a small correction. No validated workload needs
more than one pipe buffer of child input.
