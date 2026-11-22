# What you can build with MINK today

A field check of MINK 1.0.5, done by writing small programs in different areas and running them with
the release build. Everything marked "ran" was run, and its output was read. Nothing here is a
promise about areas that were not tried.

## What ran

| Area | Program | What it did | Result |
|---|---|---|---|
| Command-line tool | a word counter | Built to `.exe`, given a file on the command line, printed lines, words and bytes | Ran. Printed 2, 5, 24 for a two-line file |
| Data processing | a CSV report | Split text into rows, parsed fields, summed `qty * price` | Ran. Total 1144, largest row 1, as expected |
| Configuration | [`examples/json_config`](../../examples/json_config/main.mink) | Parsed JSON, read a string and a number, wrote it back | Ran. Exit 0, no leaks |
| Security and storage | [`examples/hash_and_compress`](../../examples/hash_and_compress/main.mink) | SHA-256 of "mink", compressed a repeated buffer | Ran. The digest matches Python's; 64 bytes packed to 11 |
| Networking and threads | [`examples/web_loopback`](../../examples/web_loopback/main.mink) | A web server thread answered a request from the main thread over `127.0.0.1` | Ran. Status 200, body "hello from mink" |
| Talking to Windows | [`examples/winapi_info`](../../examples/winapi_info/main.mink) | Called user32 and kernel32 directly for the screen size and uptime | Ran. 2560 by 1440 on this PC |
| Database | [`examples/inventory_db`](../../examples/inventory_db/main.mink) | Tables, inserts, queries, a transaction | Ran (existing example) |
| Concurrency | `examples/threaded_work`, `examples/async_tasks` | Threads and tasks | Ran (existing examples) |
| Text tools | `examples/regex_tool`, `examples/freq_count`, `examples/log_util` | Regular expressions, counting, logging | Ran (existing examples) |
| System | `examples/sysinfo`, `examples/env_report`, `examples/fs_util` | Environment, files, processes | Ran (existing examples) |

## What it feels like to write

- **You free what you allocate.** Strings, buffers and vectors are manual. When a program ends with
  anything still allocated, MINK stops with `E-R06: memory leak: live allocations remain at exit` and
  exit code 106. The word counter printed the right answer and still ended with that error, because it
  did not free the argument string.
- **Library calls take their strings.** Most standard-library functions consume the string you pass
  them, so using it again is error `E-S10`. To keep a copy, make one first, for example
  `rt_str_concat(s, "")`.
- **The compiler's messages are specific.** Both mistakes above were caught with the file, line and
  name of the value.

## Rough edges found

| What | Detail |
|---|---|
| `mink run` takes no program arguments | To pass arguments, `mink build` first and run the `.exe` |
| The standard library is found next to the installed `mink.exe` or in `.\stdlib` | A repository build run from another folder says `module file 'strings.mink' not found`. Installed copies are fine |
| Calls to Windows functions with no arguments always returned 0 | **Fixed in this change.** `GetTickCount64`, `GetCurrentProcessId` and `GetLastError` now work. A function that could not be found now returns 0 instead of crashing |
| `argv` for `rt_sys_call` must hold four words | Even for calls that take fewer. Allocate at least 32 bytes |

## Not tried yet

- A real window with its own message loop. Windows needs a callback for that, and whether MINK can
  hand one over was not tested. Simple calls into Windows, as above, work.
- HTTPS and TLS in a new program, UDP, packages installed with `mink install`, and larger programs.

The next useful step is to write one larger program, for example a small notes server or a log
analyser, and keep a list of everything that gets in the way.
