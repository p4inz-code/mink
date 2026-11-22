# Test apps plan (twelve small apps)

**Status:** Plan, second version. The owner has chosen where the apps live and how bugs are handled (2026-11-22). Nothing here is built yet.

## The idea

Twelve small apps. Each one is something a person would actually want, small enough to read in one
sitting, and each one tests a part of MINK that matters. Together they are the first things we show people:
one `.exe`, no runtime to install, starts instantly.

Every app also has to work as a test. It ends with exit code 0 and no leak report, it has tests, and
every problem it exposes goes into [`FRICTION_LOG.md`](FRICTION_LOG.md).

## What "frontend", "mobile" and "app" mean here

MINK compiles to native Windows programs. It does not compile to JavaScript or WebAssembly and it does
not run in a browser, and there is no Android or iOS target. So:

- **Backend:** MINK programs that serve HTTP, talk to a database, use sockets.
- **Frontend:** a web page (HTML, CSS, JavaScript) that a MINK program serves, builds or embeds in its own
  `.exe`. MINK is the server and the build tool. The page runs in the browser.
- **Web development:** tools that make and serve websites.
- **App development:** native Windows programs: a window, and a full-screen console program.
- **CLI:** command-line tools.
- **Mobile:** not possible with MINK today. We say so plainly.

## The twelve

| # | App | What it is | Area | What it tests in MINK | What people will see |
|---|---|---|---|---|---|
| 1 | **serve** | Static file server for a folder, like `python -m http.server` but one small `.exe` | Backend, web | HTTP server, file reading, content types, threads | Starts in milliseconds, serves a site from a single command |
| 2 | **todo** | A full web app in one `.exe`: REST API, SQLite and a single-page front end embedded in the program | Backend and frontend | HTTP server, SQLite, JSON, embedding files, routing | Run one file, open the browser, a working app |
| 3 | **sitegen** | Static site generator: Markdown and templates in, a folder of HTML out | Web, frontend | Text processing, files, templates, strings | Builds a 100-page site in well under a second |
| 4 | **jsonq** | A `jq`-style JSON query tool for the command line | CLI | JSON, parsing, arguments, big input | `curl ... | jsonq .items[0].name` in one small `.exe` |
| 5 | **fgrep** | Fast text search across a folder, with regular expressions and colour | CLI | Regex, directory walking, threads, large files | A speed number next to `findstr` |
| 6 | **dupfind** | Finds duplicate files by size and SHA-256 | CLI, filesystem | Hashing, directory walking, memory on large trees | Scans a big folder and prints what can be deleted |
| 7 | **paste** | A tiny pastebin and link shortener service | Backend | HTTP, SQLite, request parsing, IDs, limits | A service a person can run for a team in one command |
| 8 | **livelog** | Streams a log file to a web page live, as lines are written | Backend and frontend | Server-sent events, long connections, file following, threads | A browser page updating in real time |
| 9 | **mini** | A small expression language with a REPL | Language tooling | Structs, enums, `match`, recursion, ownership across many functions | Shows MINK can write a language |
| 10 | **widget** | A real Windows window: a clock or sticky note | App | Calling Win32, window and message loop, callbacks | A native window from a 100 KB program |
| 11 | **snake** | A terminal game with keyboard input and a game loop | App | Console input and output, timing, random numbers | The thing people share to show off a new language |
| 12 | **bench** | A speed suite: the same work in MINK, Python and C | Evidence | Compiler output, runtime speed, memory | A table of real timings, including where MINK loses |

### Coverage

| Area | Apps |
|---|---|
| Backend | 1, 2, 7, 8 |
| Frontend (served by MINK) | 2, 3, 8 |
| Web development | 1, 3 |
| App development | 10, 11 |
| CLI | 4, 5, 6, 9 |
| Evidence and speed | 12 |

## Order

1. **Risky first.** App 10 depends on whether MINK can hand Windows a callback for a window, and apps 2
   and 8 depend on how files get embedded and how long connections behave. We try those first with a
   throw-away spike each, and change the plan if the answer is no.
2. **Then the quick wins.** 4, 5, 6 and 1 are mostly standard library and make the first demo.
3. **Then the full-stack apps.** 2, 7, 8, then 3.
4. **Then the showpieces.** 9 and 11.
5. **Last, 12**, using the finished apps as the workload where it makes sense.

## Rules for every app

- Folder `testapps/<name>/` with a README: what it does, how to run it, results.
- Built and run with the installed release build.
- Exit code 0, no `E-R06` leak report, tests with `mink test`.
- Numbers recorded: lines of code, compile time, `.exe` size, run time on a fixed input, peak memory.
- Anything that goes wrong is logged; real bugs get a regression test and a fix on `next-1.0.6`.
- Builds and runs go through `tools/dev_guard.ps1` to stay inside the machine-heat limit.
- App text and READMEs are written in plain language.

## Decided

- The apps live in this repository under `testapps/`.
- A real bug is fixed straight away on `next-1.0.6`, with a regression test, then the app carries on.

## Still open

- Is this the right twelve? Tell me what to swap.
