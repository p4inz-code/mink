# Contributing to MINK

Thank you for helping. MINK is made by Atharva Patil at P4inz Interactive Labs and licensed under the
Apache License 2.0. By sending a contribution you agree that it is licensed the same way, and the
copyright line and NOTICE file stay with every copy.

## Build and test

```
cargo build
cargo test
```

Before you send a change:

- Run the whole test suite, not only the part you touched.
- Run `git status` and look for files you did not mean to change.
- Run `mink fmt --check` on any `.mink` files you edited. Name the path: `mink fmt --check <path>`.
  `mink fmt` with no path does nothing on purpose.

## Rules for the code

- The compiler has no dependencies and forbids unsafe code. Please keep it that way.
- Windows 10 and newer is the target. Linux is frozen.
- Registry changes go through `npm/mink/windows/register-mink-filetype.ps1`, and only under `HKCU`.
- Every error gets a code, a clear message and an entry for `mink explain`.

## Rules for words

Write user-facing text in plain language, short sentences and no hype. Say what happens and what to do
next. Check the result by reading it aloud.

## Reporting bugs

Open an issue with the MINK version, your Windows version, the smallest file that shows the problem and
what you expected. Security problems go through [SECURITY.md](SECURITY.md) instead.
