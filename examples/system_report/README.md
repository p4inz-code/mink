# MINK System Report (Windows example)

A small, self-contained MINK application that gathers basic system
information and writes it as a JSON report file (`report.json`).

This is the official Windows shipment example: it proves the complete
user workflow — install MINK, write MINK source, build with the `mink`
CLI, and run a standalone executable that needs nothing else.

## What it demonstrates

- Process information via `rt_process_id`
- Time information via `rt_time_millis`
- Filesystem reads (`rt_fs_get_cwd`, `rt_fs_exists`, `rt_fs_file_size`)
- String processing (`rt_str_len`, `rt_str_from_int`, `rt_str_concat`)
- Building structured JSON output and writing it with `rt_fs_write`
- Clean memory ownership (`rt_str_free` on every owned string)

The program is fully self-contained: it uses only runtime intrinsics,
so no standard-library modules or other files are required.

## Requirements

- MINK >= 1.0.1 (Windows x86_64)
- Installed via `npm install -g mink` (or the release zip)

## Build

```text
mink check main.mink
mink build main.mink
```

This produces one standalone executable: `main.exe`.

## Run

```text
mink run main.mink
```

or run the built executable directly:

```text
main.exe
```

Expected output:

```text
MINK system report written to report.json (N bytes)  pid=<pid> exe_exists=1
```

The program then exits with code 0 and leaves a `report.json` file in
the current directory, for example:

```json
{"pid":1234,"now_ms":567890,"cwd_len":42,"exe_size":19456}
```

## Produce a standalone .exe

```text
mink build main.mink
```

`main.exe` is a single self-contained executable. You can copy it to
any directory — even a machine without MINK, Rust, Node.js, or npm —
and run it there. It depends only on the Windows system libraries.

## Ownership note

Every `rt_str_*` function that returns a `Str` returns a new owned
string which the caller must release with `rt_str_free`. The example
follows this rule for every returned string, so it exits cleanly
through the leak checker.