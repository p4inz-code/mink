# The MINK language server

`mink lsp` starts a language server that talks the Language Server Protocol over standard input and
output. An editor starts it for you. You do not run it by hand.

## What it does

| Feature | What you get |
|---|---|
| Errors and warnings | The compiler checks your unsaved text and reports problems as you type. A duplicate definition also points at the first one. |
| Completion | Locals, functions, runtime functions and keywords. After `module::` it lists that module's names. After `Enum::` it lists the variants. After `.` it lists field names. |
| Hover | The signature, the doc comment above a declaration, and what a keyword does. |
| Go to definition | Works across files and through `module::name` paths. |
| Find references | Every use of a name. Field names and member accesses with the same spelling are left out. |
| Rename | Changes a name everywhere it is used. It refuses invalid names, reserved words, names starting with `rt_`, names already taken, runtime functions and standard-library names. |
| Outline | Functions, structs, enums and constants, with fields and variants underneath. |
| Signature help | Shows the parameter you are typing. |
| Format | Uses `mink fmt`, so it never changes what a program says. |

## How it works

- Errors come from the same compiler that builds your program, so editor and build always agree.
- Names, hover and rename use a token-level index of each file. It keeps working while a file has
  errors, which is when you need completion most.
- Positions are UTF-16 columns, as the protocol requires. Accented letters and emoji do not shift
  anything.
- A file that does not parse stops the compiler before it loads its modules. The server still finds
  those modules, so completion after `module::` works in half-typed code.

## Limits in this release

- It checks the whole program from the open file each time. Large programs may feel slower.
- Rename covers files the compiler loaded from the open file's modules.
- No semantic highlighting, code actions or workspace-wide symbol search yet.

## Try it

```
mink lsp --help
```

A VS Code extension that starts it for you is the next step of the 1.0.5 sprint.
