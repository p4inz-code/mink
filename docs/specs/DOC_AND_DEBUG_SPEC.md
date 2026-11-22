# `mink doc` and debug information: specification

**Status:** Specification only. Not implemented in 1.0.5.

## `mink doc`

Builds browsable documentation from doc comments, the lines that start with `///` above a
declaration. The language server already reads these for hover, so the rules are shared.

- `mink doc <path>` writes a folder of plain HTML files with no scripts and no network access.
- Each module gets a page listing its functions, structs, enums and constants with signatures and doc
  comments. Names link to their declarations.
- `mink doc --check` fails if a public function has no doc comment, for CI.
- Output is deterministic, so two runs on the same source give the same files.
- The footer carries the package's copyright line and license.

## Debug information

Today a built program has no source lines for a debugger. The plan:

- `mink build --debug` writes a `.pdb` file next to the program, in the format Windows debuggers read.
- It maps machine code to source file and line, and names functions and local variables with their types.
- The program itself stays the same. Debug information is a separate file, so a release build is no
  bigger.
- `mink build` without `--debug` writes nothing new.

### Order of work

1. Line numbers and function names, enough for stack traces and stepping.
2. Local variables and types.
3. A launch configuration in the VS Code extension.

## Open questions

- Whether to write PDB directly or an intermediate format first.
- How closures and monomorphized generics are named in a debugger.
