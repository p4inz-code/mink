# Why MINK exists

MINK is a programming language that builds small, native Windows programs from readable source
files. It is made by Atharva Patil at P4inz Interactive Labs, under the Apache License 2.0.

## The problem it goes after

Writing a native Windows program today usually means choosing between two kinds of pain.

- **Easy languages** need an interpreter or a runtime on the other computer. You ship a folder of
  files, or an installer that carries a whole language with it.
- **Systems languages** give you a single fast program, but you pay for it in setup: a toolchain, a
  linker, header files or crates, and error messages that assume you already know why.

MINK tries to sit between them. One small compiler produces one `.exe`. You write short source
files and get errors that say what went wrong and how to fix it.

## What is built to back that up

These are facts about the code in this repository, not goals.

- **No outside toolchain.** The compiler is written in Rust with no dependencies and no unsafe code.
  It writes the Windows executable itself, so there is no linker or assembler to install.
- **Errors are part of the design.** Every error has a code (`E-S01`, `E-S02` and so on), a plain
  message and an explanation: `mink explain E-S02`.
- **One format for everyone.** `mink fmt` has no settings. It also checks its own output, token by
  token, so a format cannot change what a program does.
- **Real editor support.** `mink lsp` gives errors as you type, completion, hover, rename and more,
  and a VS Code extension starts it for you.
- **A normal Windows citizen.** `.mink` files get an icon, right-click Run, Check and Build, a New
  menu entry and a preview. Installing needs no administrator rights and removes cleanly.

## What MINK does not claim

The project's own rule is that it makes no competitive claim without evidence. So:

- It does not claim to be faster than C, C++ or Rust. Benchmarks live in `docs/benchmarks`, and the
  numbers there are what you should read, not this page.
- It does not claim to replace Python. Python has far more libraries and decades of tooling.
- It targets Windows 10 and newer. Linux support is frozen.
- It is young. See [`KNOWN_LIMITATIONS.md`](KNOWN_LIMITATIONS.md) for what does not work yet.

## Why one person is building it

Because the combination above did not exist in a form the author wanted to use, and because building
a language end to end is the best way to learn where the real costs are. The credit for that work
stays with the creator: the license requires that copies keep the copyright line and the NOTICE file.

## Where to go next

- Try it: the [README](../README.md).
- Learn it: [`language/LANGUAGE_BASICS.md`](language/LANGUAGE_BASICS.md).
- See what is planned: [`roadmap/`](roadmap/) and [`specs/`](specs/).
