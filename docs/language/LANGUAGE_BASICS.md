# MINK Language Basics

**Applies to:** MINK 1.0.5 (current repository build) and MINK 1.0.4 (published
npm package), Windows x86_64. The reference was written against the 1.0.4
repository build; version-specific statements below explicitly name 1.0.4.
**Status:** Developer reference; current version line re-checked 2026-11-08
(Session 217); command-line, stderr and stdin contracts verified 2026-11-08
(Session 238).

This guide covers the runtime behavior a developer needs in order to write
MINK programs: output, command-line input, collections, modules, ownership,
filesystem returns, and compiler/standard-library lookup.

`CORE_GRAMMAR.md` remains the syntax authority. The records under
`docs/implementation/` describe how the compiler implements the language and
are not required reading.

## 1. Build and run

A MINK program is one or more `.mink` files. The entry point is `fn main`,
with or without a return type. A `main() -> Int` return value becomes the
process exit code; a `main()` without a return type exits 0.

```mink
fn main() -> Int {
    rt_print_str("hello");
    return 0;
}
```

```bash
mink build hello.mink
hello.exe
```

`mink run hello.mink` builds and runs in one step. `mink check hello.mink`
runs the front end without writing an executable.

### 1.1 Command-line arguments and stdin

`rt_argc()` returns the number of arguments after the executable name.
`rt_argv(index)` returns argument `index` (0-based, so `rt_argv(0)` is the first
argument after the program name). An out-of-range index is the runtime failure
`E-R10`. The returned `Str` is owned; free it with `rt_str_free`.

`rt_stdin_read()` reads all available standard input to EOF and returns an
owned `Str` (empty when there is no input). Free it when done. It is a read-all
operation, not a line reader; split the returned text yourself if the program
needs lines. `rt_stderr_write(s)` writes `s` to standard error and returns the
byte count, or `-1` on failure.

```mink
fn main() -> Int {
    if rt_argc() < 1 {
        rt_stderr_write("usage: tool <name>\n");
        return 2;
    }
    let name = rt_argv(0);
    rt_print_str(name);
    rt_str_free(name);

    let input = rt_stdin_read();
    rt_print_int(rt_str_len(input));
    rt_str_free(input);
    return 0;
}
```

## 2. Output is line-oriented

Every `rt_print_*` call writes one complete line and appends CRLF (`\r\n`).
Consecutive calls cannot build a single line:

```mink
fn main() {
    rt_print_str("a=");
    rt_print_int(7);
}
```

The process writes the bytes `a=\r\n7\r\n`, displayed as:

```text
a=
7
```

`rt_print_str("")` writes an empty line. A string that already contains `\n`
produces an extra line break, because the intrinsic appends its own
terminator.

To build one line, build one `Str` and print it once:

```mink
fn main() -> Int {
    let number = rt_str_from_int(55);
    let line = rt_str_concat("total=", number);
    rt_print_str(line);
    rt_str_free(line);
    rt_str_free(number);
    return 0;
}
```

Output:

```text
total=55
```

`rt_str_concat` reads its arguments and allocates a new result; it does not
take ownership of them, so both owned inputs still need their own free.

The conversion intrinsics allocate a new owned `Str`:
`rt_str_from_int`, `rt_str_from_float`, `rt_str_from_bool`, and
`rt_str_concat`.

`rt_str_format(format, a, b, c)` takes four `Str` arguments (the format plus
up to three values), substitutes `{}` placeholders, and returns an owned
`Str`. It does not accept `Int` or `Float` arguments directly:

```mink
let line = rt_str_format("{} + {} = {}", "2", "3", "5");
```

Output:

```text
2 + 3 = 5
```

`rt_print_int` writes decimal notation. `rt_print_float` writes up to 17
significant digits with trailing zeros removed. Verified examples:

| Expression | Output |
| --- | --- |
| `rt_print_float(2.5)` | `2.5` |
| `rt_print_float(1.0)` | `1` |
| `rt_print_float(1.0 / 3.0)` | `0.33333333333333331` |
| `rt_print_float(0.1 + 0.2)` | `0.30000000000000004` |
| `rt_print_float(1e20)` | `1e+20` |
| `rt_print_float(1.0 / 0.0)` | `Inf` |
| `rt_print_float(-1.0 / 0.0)` | `-Inf` |
| `rt_print_float(0.0 / 0.0)` | `-NaN` |
| `rt_print_float(-0.0)` | `-0` |

`rt_print_char('A')` writes the character followed by CRLF. Character
literals use single quotes.

## 3. Collections

`Vec<T>`, `Map<K, V>`, and `Set<T>` are built-in types. They are handle
values: operations take a handle and return a handle. Assign the result of an
operation that returns a collection back to the variable.

```mink
fn total(m: Map<Int, Int>) -> Int {
    return rt_map_get(m, 1) + rt_map_get(m, 2);
}

fn build() -> Map<Int, Int> {
    let mut m = rt_map_new(2);
    m = rt_map_insert(m, 1, 10);
    m = rt_map_insert(m, 2, 20);
    return m;
}

fn main() -> Int {
    let mut m = rt_map_new(2);
    m = rt_map_insert(m, 1, 10);
    m = rt_map_insert(m, 2, 20);

    rt_print_int(total(m));

    let built = build();
    rt_print_int(total(built));

    rt_map_free(m);
    rt_map_free(built);
    return 0;
}
```

Output:

```text
30
30
```

Rules for maps:

- `Map<K, V>` annotations are accepted on bindings, parameters, and return
  types. Passing a map to an `Int`-typed parameter is `E-T01`; annotate the
  parameter as `Map<Int, Int>`.
- An unannotated parameter used with `rt_map_get` is inferred as a map, so
  annotations are not mandatory. A diagnostic containing
  `Map<unresolved, ...>` means a map component could not be inferred; add the
  annotation.
- `rt_map_new(capacity)` creates a map. `rt_map_insert` consumes the handle
  and returns a possibly-new one, so always assign the result back.
  `rt_map_get`, `rt_map_has`, and `rt_map_len` borrow the handle.
  `rt_map_remove` consumes and returns the map, and `rt_map_free` releases
  it.
- Keys may be `Int`, `Bool`, `Char`, `Str`, or an enum whose variants all
  carry no payload. Other key types are rejected.
- A map handle can be passed to a user function and used again by the
  caller. Keep mutations in the owner, or return the new handle to the
  caller.
- Inserting an owned `Str` value moves it into the collection; the source
  binding is dead after the call.

`Vec<T>` and `Set<T>` follow the same handle pattern with `rt_vec_*` and
`rt_set_*`.

## 4. Modules

A `mod name;` declaration loads `name.mink` from the declaring file's
directory (or `name/mod.mink`) and adds its top-level items to the program.
In 1.0.4 those items are callable unqualified from the declaring file
without a `use`.

`helper.mink`:

```mink
pub fn double(x: Int) -> Int {
    return x * 2;
}
```

`main.mink`:

```mink
mod helper;

fn main() -> Int {
    rt_print_int(double(4));
    return 0;
}
```

Output:

```text
8
```

Module rules:

- `use helper::double;` binds `double` explicitly. It is optional, and it
  does not load the file: without `mod helper;` the call fails with
  `E-S01: cannot find name double`.
- `use helper;` is accepted and does nothing.
- Qualified expression paths such as `helper::double(4)` are not supported.
- `pub` is parsed but visibility is not enforced in 1.0.4. Loaded items are
  reachable even without `pub`.
- Standard-library modules are loaded the same way: `mod filesystem;`, then
  call `fs_write(...)` unqualified.

Module lookup order, first match wins:

1. `name.mink` beside the declaring file;
2. `name/mod.mink` beside it;
3. `stdlib/name.mink` next to the installed compiler (npm layout:
   `<package>/stdlib`);
4. `stdlib/name.mink` under the current working directory (repository
   checkout);
5. package roots from the active project or environment.

A local `filesystem.mink` beside the source therefore shadows the bundled
one.

## 5. Ownership and cleanup

MINK has explicit ownership for `Str` and collections. There is no garbage
collector.

- String literals are immutable. The results of `rt_str_alloc`,
  `rt_str_concat`, `rt_str_from_int`, `rt_str_from_float`,
  `rt_str_from_bool`, `rt_str_format`, `rt_fs_read`, `rt_fs_get_cwd`, and
  `rt_argv` are owned heap strings.
- Read-only intrinsics borrow and leave the binding usable: `rt_str_len`,
  `rt_str_byte`, `rt_print_str`, and `rt_str_eq`.
- `rt_str_free` consumes the string. After it the binding is dead; a second
  free or any later use is the compile error `E-S10: use of moved value`.
- A user-function call consumes an owned `Str` argument. If the callee only
  needs to read it, take a reference and dereference it:

```mink
fn length(s: &Str) -> Int {
    return rt_str_len(*s);
}

fn main() -> Int {
    let s = rt_str_concat("a", "b");
    rt_print_int(length(&s));
    rt_str_free(s);
    return 0;
}
```

Output:

```text
2
```

Without `&`, `length(s)` would move `s`, and `rt_str_free(s)` would be
rejected with `E-S10`.

- Freeing a literal is legal and does nothing at runtime:
  `rt_str_free("literal")` exits 0.
- Every owned string must be freed. At process exit the runtime reports a
  leak as `E-R06: memory leak: live allocations remain at exit` and exits
  with code 106 (`100 + 6`). Output written before the exit is kept.
- A returned string is owned by the caller. Free `fs_read` results,
  `rt_str_concat` results, and other allocating results. A temporary can be
  freed directly: `rt_str_free(rt_str_concat("a", "b"));`.

For example, this program prints `ab` and then fails the exit-time leak
check:

```mink
fn main() -> Int {
    let s = rt_str_concat("a", "b");
    rt_print_str(s);
    return 0;
}
```

stderr:

```text
mink: runtime error[E-R06]: memory leak: live allocations remain at exit
```

exit code 106.

Collection handles are released with `rt_map_free`, `rt_vec_free`, or
`rt_set_free`. Read-only collection intrinsics borrow the handle;
insert/push/remove consume it and return a handle to assign back.

> **Limitation in 1.0.4.** Use-after-free is diagnosed for `Str` but not for
> collection handles. Code such as `rt_map_free(m); rt_map_get(m, 1);`
> compiles and runs; it is not a supported contract. Treat every freed
> collection handle as dead.

## 6. Filesystem return conventions

Load the module with `mod filesystem;`. The `fs_*` wrappers consume their
`Str` arguments, so passing an owned path moves it. Literal arguments are
safe.

| Call | Success | Failure |
| --- | --- | --- |
| `fs_write(path, data)` | bytes written (`0` for empty data) | `-1` |
| `fs_read(path)` | owned `Str` with the contents; empty file gives `""` | `""` |
| `fs_file_size(path)` | bytes (`0` for an empty file) | `-1` |
| `fs_exists(path)` | `true` or `false` | - |
| `fs_create_dir`, `fs_remove_dir`, `fs_remove_file`, `fs_copy_file`, `fs_move`, `fs_set_cwd` | `0` | `1` |
| `fs_dir_open(path)` | handle | `0` |
| `fs_dir_next(handle)` | owned `Str` entry name | `""` when exhausted or the handle is `0` |
| `fs_dir_close(handle)` | `0` | `-1` for a null handle |
| `fs_get_cwd()` | owned `Str` | - |

`fs_read` cannot distinguish a missing file from an empty file by itself:
both return a zero-length string. Use `fs_exists` (false for missing) or
`fs_file_size` (`-1` for missing, `0` for empty).

`fs_write` replaces the file's contents; there is no append mode. To append,
read the existing text, concatenate the addition, and write the result back.

These return values are the only error information the filesystem wrappers
expose; there is no last-error call.

```mink
mod filesystem;

fn main() -> Int {
    let written = fs_write("note.txt", "hello");
    let size = fs_file_size("note.txt");
    let text = fs_read("note.txt");
    let missing = fs_read("absent.txt");

    rt_print_int(written);
    rt_print_int(size);
    rt_print_int(rt_str_len(text));
    rt_print_int(rt_str_len(missing));
    rt_print_int(fs_file_size("absent.txt"));

    rt_str_free(text);
    rt_str_free(missing);
    rt_print_int(fs_remove_file("note.txt"));
    return 0;
}
```

Output:

```text
5
5
5
0
-1
0
```

## 7. Compiler and standard-library lookup

The two distributions resolve the standard library differently by design:

- **npm package.** `bin/mink.exe` sits beside `stdlib/`. `mod filesystem;`
  resolves `<package>/stdlib/filesystem.mink` from any working directory.
- **Repository build.** `target/release/mink.exe` looks for
  `<exe>/../stdlib` (that is `target/stdlib`, which does not exist) and then
  `<cwd>/stdlib`. Run it from the repository root, or copy the needed
  `stdlib/*.mink` files beside the source. Running the repository compiler
  from an unrelated directory reports
  `E-S01: module file 'filesystem.mink' not found` even though the repository
  contains `stdlib/filesystem.mink`.

This is a development-checkout layout, not a language rule. The installed
npm package is the supported developer distribution.

## 8. Diagnostics you will meet

| Code | Meaning | Typical cause |
| --- | --- | --- |
| `E-S01` | unresolved name or module not found | `mod helper;` with no `helper.mink`; `use` without `mod` |
| `E-S10` | use of moved value | using or freeing an owned `Str` after a user call or a free |
| `E-T01` | type mismatch | passing `Map<Int, Int>` to an `Int` parameter |
| `E-R06` | memory leak at exit (code 106) | forgetting `rt_str_free` or `rt_map_free` |

`mink explain <code>` prints the documented explanation for a code.

## 9. Where to look next

- `CORE_GRAMMAR.md` - syntax and grammar.
- `examples/` - complete programs, one directory each.
- `stdlib/*.mink` - standard-library functions and their comments.
- `benchmarks/m6/session176/` - small language-learning exercises and their
  verifier.
