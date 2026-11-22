# Why choose MINK? Features, limits, and where it can win

**Status:** Working paper, 2026-11-22. For the owner. Facts are from this repository and from measurements
on one PC; where something about another language is general knowledge and not measured here, it says so.

The question every visitor asks is "why would I use this instead of Python, Go, Rust or C++?" A language that
cannot answer it in one sentence does not get tried. This paper lists what MINK has, what it lacks, what we
measured, and where it can honestly win.

## 1. What MINK has today (verified)

| Area | What works |
|---|---|
| Compiler | Written in Rust, zero dependencies, no unsafe code. Writes the Windows `.exe` itself: no linker, no assembler. 5.1 MB program, 2 MB zipped. |
| Programs | A built program is a small native `.exe`: 73 KB for the four benchmark programs. |
| Errors | Every error has a code, a file and position, and an explanation (`mink explain`). 65 documented codes, including every runtime error. |
| Memory safety net | A built-in leak report at exit (E-R06), checked allocations, checked reads and writes, string and array bounds errors, division by zero. |
| Tools | `mink fmt` (no settings, verifies its own output), `mink lsp` (language server), VS Code extension, `mink test`, `mink upgrade`, side-by-side versions. |
| Windows | File association, icon, right-click Run / Check / Build, New menu, preview, per-user install with no administrator rights, clean removal. |
| Standard library | 27 modules: files, JSON, CSV, SQLite, HTTP client and server, TLS (Windows trust store), threads, tasks, regex, hashing, crypto, compression (zlib, zip), time, logging, environment, processes, random, math, strings, collections. |
| Windows API | Call any DLL function directly (`rt_sys_call`), so you can use Windows without bindings. A real window with a live clock works. |
| Proof | Twelve small apps built, run and tested: web server, web app, pastebin, live log stream, JSON tool, search tool, duplicate finder, calculator language, terminal game, window, website generator, benchmarks. |
| Speed | 6 to 39 times faster than CPython 3.11 on four small programs. 1.3 to 3.9 times slower than Go, 2 to 5 times slower than optimized Rust. |
| Licence | Apache 2.0, with the creator's credit kept through the NOTICE file. |

## 2. What MINK lacks today (honest)

| Area | Limit |
|---|---|
| Platform | Windows 10 and newer only. No Linux, macOS, browser or mobile. |
| Memory | Manual: you free every string and buffer, and most library calls consume the string you pass them. This is the biggest day-to-day cost. A program that leaks fails at exit. |
| Ergonomics | No generics-heavy library, no closures over library types, strings are moved by every call, and the ownership check is not path-sensitive. Temporaries must be named and freed. |
| UI | No callbacks, so no custom window procedure; no widget library. Clicks and painting must be polled. |
| Ecosystem | 27 standard modules and nothing else. No package registry, no third-party libraries, no `mink doc`. |
| Tooling | No debugger information, so a debugger shows no source lines. VS Code only, and not on the Marketplace. No profiler. |
| Language | One flat namespace (name clashes), `pub` is not enforced, no real modules or visibility, no macros. |
| SQL | The built-in SQL engine supports only `SELECT *` and `COUNT(*)`, no joins. |
| Speed | Slower than Go and Rust. Array reads and writes are the weak spot: each one is checked. |
| Trust | Unsigned files (SmartScreen warns), one maintainer, young, no users yet. |
| Distribution | winget not published yet. |

## 3. What we measured against the others

| Measure | MINK | Go | Rust | Python |
|---|---|---|---|---|
| Program size (a 4-line numeric program) | 73 KB | 2,406 KB | 123 KB | needs an interpreter |
| Toolchain on disk | 5 MB (the compiler, one file) | 225 MB | 973 MB | about 100 MB for the interpreter and library |
| Time to build a small program | about 105 ms | about 160 ms | about 115 ms | none |
| Speed on the four benchmark programs | baseline | 1.3 to 3.9 times faster | 2 to 5 times faster | 6 to 39 times slower |

Notes. The Python install size is from general knowledge and was not measured on this PC. The build times
include process start and are close to each other at this size, so build speed is not a selling point until we
measure larger programs. The Rust toolchain here targets Windows with Microsoft's linker, which comes from a
separate Visual Studio component that this PC already had; MINK needs no linker.

## 4. Where MINK can honestly win

MINK will not beat Go on servers, Rust on safety, Python on libraries or C++ on reach. It can win where those
languages are weak, and the winning story has to be one sentence.

### The wedge: "Tiny native Windows tools, with nothing to install first."

1. **Smallest path from an idea to a Windows `.exe`.** A 5 MB compiler, no linker, no Visual Studio, programs of
   tens of kilobytes that start instantly. A person with a Windows PC can go from nothing to a shipped tool in
   minutes. Go and Rust produce larger programs and need larger toolchains; Python needs an interpreter on the
   other PC.
2. **Direct Windows power.** Any Windows function can be called with no bindings and no generator. With
   callbacks and a `ui` module (planned) this becomes real windows, tray icons and menus in a 100 KB program.
3. **Errors that teach.** Coded, located, explained errors, a leak report, checked memory. Good for learners and
   for people who write small tools and want the compiler to say what is wrong.
4. **A compiler you can read.** Zero dependencies, documented stage by stage, small enough to learn from. This is
   a real draw for students, teachers and compiler hobbyists.

Who this is for, in order: people who write small Windows utilities and automation and are tired of shipping a
Python runtime or a 2 MB Go binary; students and teachers of compilers and systems programming; game modders
and tool authors on Windows; developers who like small, understandable tools.

What to say: **"MINK builds small, native Windows programs with a 5 MB compiler and nothing else to install."**
Every word of that is true today and measured.

### What not to claim yet

Fast (it is slower than Go and Rust), safe (memory is manual), complete (27 modules), cross-platform (it is not),
production-ready for large systems (it is young and has one maintainer).

## 5. How to beat them, in order of value

Each item says which weakness it removes and how we will know it worked.

| # | Work | Removes | Proof it worked |
|---|---|---|---|
| 1 | **Callbacks, then a `ui` module** ([plan](../roadmap/CALLBACKS_AND_UI_PLAN.md)) | "MINK cannot make a real app" | A settings window with a button, list and timer, in a program under 150 KB |
| 2 | **Package registry and `mink doc`** ([spec](../specs/PACKAGE_REGISTRY_SPEC.md)) | "No ecosystem" | Ten useful packages installable by name |
| 3 | **Debug information (PDB)** ([spec](../specs/DOC_AND_DEBUG_SPEC.md)) | "I cannot debug it" | Step through MINK source in Visual Studio or VS Code |
| 4 | **Faster arrays**: remove checks that can be proven safe, keep loop counters in registers | The speed gap to Go and Rust | Sieve within 2 times of Go |
| 5 | **Less manual memory**: scoped strings freed automatically (`defer` exists), borrowing for library calls, a path-sensitive ownership check | The biggest day-to-day cost | The twelve apps need half as many `rt_str_free` calls |
| 6 | **Real modules and visibility** | Name clashes, `pub` not enforced | Two modules with the same function name in one program |
| 7 | **Trust**: code signing, winget, Marketplace extension, a public roadmap | "Is this safe to run?" | No SmartScreen warning, `winget install MINK` works |
| 8 | **A larger standard library where tools need it**: command-line parsing, `.ini`/TOML, dates and time zones, Windows registry and services, clipboard, notifications | "I cannot write my tool with it" | Rewrite five common PowerShell or Python admin scripts as MINK programs |
| 9 | **Documentation and learning**: a guided tour, a book, examples for every module | Learning cost | A new user ships a tool in an afternoon, measured by watching one |
| 10 | **Community**: Discussions, a showcase of tools people built, a contributor guide that works | One maintainer | First ten outside contributions |

The order is chosen so that the wedge gets stronger first (1, 4, 5, 7), then the ecosystem (2, 8) and the
developer experience (3, 6, 9), with community (10) running alongside.

## 6. The measure of success

Before asking whether MINK beats other languages, ask whether a visitor has a reason to try it. For each target
user, one concrete task where MINK is clearly the easiest way:

- "Turn this 30-line PowerShell script into a 70 KB `.exe` with no dependencies."
- "Write a small Windows tray utility."
- "Show a student how a compiler turns source into a Windows program."

If we can show each of those working, end to end, in a short demo, that is a reason to choose MINK. The test
apps are the first set of evidence for the first and third; a `ui`-based tray utility is the missing one.

## 7. Next steps for this paper

1. Measure the missing numbers: Python install size, build time on a 2,000-line program, memory use at run time.
2. Pick the three tasks above and build one demo each.
3. Put the one-sentence pitch and the measured table on the website, replacing anything we cannot back up.
