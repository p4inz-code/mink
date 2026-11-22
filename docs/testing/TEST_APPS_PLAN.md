# Test apps plan

**Status:** Plan, for the owner to approve. Nothing here is built yet.

## Why

A field check of 1.0.5 showed that small programs work and found one real bug. Small programs hide the
problems that show up when code gets bigger: ownership rules across many functions, module layout,
speed, memory, and missing library pieces. So we build real, medium-sized apps, one per area MINK says
it is for, and write down everything that gets in the way.

## How each app is run

- Lives in `testapps/<name>/` with its own README: what it does, how to run it, results.
- Built and run with the installed release build, not a development build.
- Must end with exit code 0 and no `E-R06` leak report.
- Has tests (`mink test`) for its main logic.
- Every problem goes in one list, [`FRICTION_LOG.md`](FRICTION_LOG.md), with: what happened, the smallest
  program that shows it, and whether it is a bug, a missing feature or a rough edge.
- A real bug gets a regression test and a fix on the `next-1.0.6` branch.
- Builds and runs go through `tools/dev_guard.ps1` to stay inside the machine-heat limit.

## The apps

Ordered by risk, lowest first.

### Tier 1: quick, mostly the standard library

| # | App | What it is | Areas it covers | Done when |
|---|---|---|---|---|
| 1 | **logscan** | Command-line log analyser: reads a large log, counts levels, finds the top error messages, writes a report as text, CSV or JSON | CLI arguments, big files, strings, regex, maps, sorting, CSV and JSON output | Handles a 100 MB log without running out of memory; output checked against a Python script |
| 2 | **calc** | Expression language with a REPL: tokenizer, parser, evaluator, variables, functions | Structs, enums, `match`, recursion, ownership across many functions, tests | 20 or more expressions pass; syntax errors give clear messages |
| 3 | **backup** | Folder backup tool: walks a folder, hashes files, compresses changed ones into a zip, skips unchanged ones | Filesystem, directory walking, SHA-256, zlib and zip, timing | Second run on an unchanged folder copies nothing; archive opens in Windows Explorer |

### Tier 2: several parts working together

| # | App | What it is | Areas it covers | Done when |
|---|---|---|---|---|
| 4 | **notes** | Notes web service: add, list and delete notes over HTTP, stored in SQLite, answers in JSON | HTTP server, SQLite, JSON, threads, request parsing, a client for its own tests | 1,000 requests from a test client with no crash, no leak and correct data |
| 5 | **taskrun** | Task runner: reads a JSON file of tasks and dependencies, runs them as processes in parallel, reports time and failures | Processes and pipes, environment, threads, JSON, error handling | A 10-task graph runs in dependency order; a failing task stops its dependents |
| 6 | **chat** | TCP chat server and client with several clients at once | Sockets, concurrency, locks, timeouts, closing cleanly | 20 clients exchange messages with none lost; shutting down leaves no open sockets |

### Tier 3: measurement and ecosystem

| # | App | What it is | Areas it covers | Done when |
|---|---|---|---|---|
| 7 | **bench** | Speed suite: loops, sorting, hashing, string building, collections, with the same work written in Python and C | Compiler output quality, runtime speed and memory | A table of real timings. Any claim of "faster than X" has a number behind it, and "slower" is recorded too |
| 8 | **pkgdemo** | Two small packages and an app that installs and uses them | `mink install`, `mod` lookup, name clashes, shadowing diagnostics, package layout | Install, update and remove work; a clash is reported clearly |

### Spikes: find out first, then decide

| Spike | Question | Result decides |
|---|---|---|
| **Window** | Can a MINK program create a real Windows window with its own message loop, meaning can MINK hand Windows a callback? | Whether desktop apps are possible now, or need runtime work |
| **HTTPS** | Does a fresh program reach a real HTTPS site using the standard library's TLS? | Whether web clients work outside the local machine |
| **Big data** | How does MINK behave with a 1 GB file and with millions of small allocations? | Memory limits to document |

## What we measure for every app

Lines of code, time to compile, size of the `.exe`, run time on a fixed input, peak memory, and the
number of times a compiler message was unclear. These go in the app's README, so the numbers are not
guessed.

## Order of work

1. Spikes first, because they can change what the later apps are.
2. Tier 1, then Tier 2, then Tier 3.
3. After each app: update the friction log, fix what is a bug, and decide what to change in the
   language or library.

## Decisions needed from the owner

1. Is this the right list, and anything to add, drop or move?
2. Where should apps live: this repository under `testapps/`, or a separate repository?
3. Should real bugs found this way be fixed straight away on `next-1.0.6`, or only logged until the
   round is finished?
