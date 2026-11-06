# MINK Standard Library Reference

**Applies to:** MINK 1.0.4 (repository build) and the installed
`@p4inz-code/mink` package as found on the audit machine.
**Status:** Developer reference, audited 2026-11-05 (M6 Session 178).
**Companion documents:** [`LANGUAGE_BASICS.md`](LANGUAGE_BASICS.md) for runtime
semantics, [`DEBUGGING_AND_TESTING.md`](DEBUGGING_AND_TESTING.md) for
diagnostics and test workflow.

This reference is built from the sources in `stdlib/*.mink` and from programs
that were compiled and executed during the audit. Every entry carries an
evidence marker:

| Marker | Meaning |
| --- | --- |
| **EXEC** | The call was compiled and run, and its output/exit code was observed. |
| **SOURCE** | Signature and contract come from the module source and its comments; it was not executed in this session. |
| **PARTIAL** | Partly executed, or executed only in a composite probe. |
| **UNKNOWN** | The source states no reliable contract; do not rely on it. |

Compilation proves syntax, not behavior. Source comments are evidence of
intent, not proof of behavior; where a comment and an execution disagree, the
execution wins and the disagreement is called out.

## Contents

1. [Environment and version reality](#1-environment-and-version-reality)
2. [Module index](#2-module-index)
3. [Output and formatting](#3-output-and-formatting)
4. [Strings](#4-strings-mod-strings)
5. [Collections](#5-collections-vec-map-set)
6. [Filesystem](#6-filesystem-mod-filesystem)
7. [JSON](#7-json-mod-json)
8. [CSV](#8-csv-mod-csv)
9. [Encoding](#9-encoding-mod-encoding)
10. [Hashing and crypto](#10-hashing-and-crypto)
11. [Math](#11-math-mod-math)
12. [Time](#12-time-mod-time)
13. [Logging](#13-logging-mod-logging)
14. [Process and environment](#14-process-and-environment)
15. [Random](#15-random-mod-random)
16. [Regular expressions](#16-regular-expressions-mod-re)
17. [Networking, HTTP and TLS](#17-networking-http-and-tls)
18. [SQLite](#18-sqlite-mod-sqlite)
19. [Compression and archives](#19-compression-and-archives)
20. [Threads and tasks](#20-threads-and-tasks)
21. [Assertions and `mink test`](#21-assertions-and-mink-test)
22. [Option and Result](#22-option-and-result)
23. [Ownership cheat sheet](#23-ownership-cheat-sheet)
24. [What is not verified](#24-what-is-not-verified)
25. [Where to look next](#25-where-to-look-next)

## 1. Environment and version reality

Two binaries were used during this audit and they are **not the same version**:

| Binary | Version | Standard-library lookup |
| --- | --- | --- |
| `mink` on `PATH` (npm global package) | `mink 1.0.2` | `<package>/stdlib`, resolved from the executable |
| `target/release/mink.exe` (repository build) | `mink 1.0.4` | `<exe>/../stdlib`, then `<cwd>/stdlib` |

The installed package reported **1.0.2** on the audit machine. Earlier M6
records assume an installed 1.0.4; that assumption did not hold here. Both
binaries compiled and ran the programs in this reference, but the repository
1.0.4 binary is the one used for every recorded output unless stated
otherwise. Record the exact version in any bug report.

The repository binary must be run with the repository root as the working
directory, or the source copied beside the needed `stdlib` files. The
installed binary carries its own `stdlib`, so it works from any directory.

```bash
# repository build
./target/release/mink.exe --version        # mink 1.0.4
./target/release/mink.exe build app.mink

# installed package
mink --version                             # mink 1.0.2
mink build app.mink
```

## 2. Module index

Load a module with `mod name;`. Its top-level functions are then called
unqualified. `use` does not load a file. Standard-library module files are
plain MINK; they can be read, and a local `name.mink` beside your source
shadows the bundled module.

| Module | Purpose | Evidence |
| --- | --- | --- |
| `assert` | Test assertions used by `mink test` | EXEC |
| `collections` | `Vec<T>` helper functions (`vec_*`) | EXEC |
| `crypto` | HMAC-SHA256, HKDF-SHA256, secure random | SOURCE |
| `csv` | RFC 4180 subset row parse/write | EXEC |
| `encoding` | Hex, Base64/URL-safe Base64, UTF-8, URL encoding, byte classes | PARTIAL |
| `environment` | Wraps the predeclared `rt_env_*` intrinsics as `env_*`; load with `mod environment;` | EXEC |
| `filesystem` | Paths, file read/write, directories, CWD | EXEC |
| `hashing` | FNV-1a, djb2, SHA-256, digest equality | SOURCE |
| `http` | HTTP/1.1 client over TCP | SOURCE |
| `httpserver` | HTTP/1.1 server helpers | SOURCE |
| `json` | Arena-based JSON parser and serializer | EXEC |
| `logging` | Leveled logging to stderr | EXEC |
| `math` | Integer/float math, trigonometry, logs | PARTIAL |
| `network` | TCP/UDP sockets, polling, timeouts | SOURCE |
| `option` | `Option<T>` enum declaration only | SOURCE |
| `process` | Child process run and output capture | EXEC |
| `random` | Deterministic xorshift64* PRNG | SOURCE |
| `re` | Backtracking regular expressions | EXEC |
| `result` | `Result<T, E>` enum declaration only | SOURCE |
| `sqlite` | In-memory SQL engine | EXEC |
| `strings` | String search, transform, split/join, parse | EXEC |
| `tasks` | Deferred-task queue | SOURCE |
| `threads` | Threads and locks | SOURCE |
| `time` | Clock, calendar, `strftime`/`strptime` | EXEC |
| `tls` | TLS client with certificate validation | SOURCE |
| `zip` | ZIP archive builder and reader | PARTIAL |
| `zlib` | Deflate/inflate, zlib, gzip, CRC-32, Adler-32 | PARTIAL |

Built-in types and intrinsics need no `mod`: `Vec<T>`, `Map<K, V>`, `Set<T>`,
and every `rt_*` intrinsic listed below.

## 3. Output and formatting

Every print intrinsic writes its value and then **CRLF** (`\r\n`). There is no
same-line print. To build one line, build one `Str` and print it once.

| Intrinsic | Behavior | Evidence |
| --- | --- | --- |
| `rt_print_str(s: Str)` | Writes `s` then CRLF; borrows `s` | EXEC |
| `rt_print_int(n: Int)` | Decimal then CRLF | EXEC |
| `rt_print_float(f: Float)` | Up to 17 significant digits, trailing zeros removed, then CRLF | EXEC |
| `rt_print_char(c: Char)` | Character then CRLF | EXEC |
| `rt_stderr_write(s: Str)` | Writes exactly `s`; **no** newline is appended | EXEC |
| `rt_str_concat(a: Str, b: Str) -> Str` | New owned string; borrows both inputs | EXEC |
| `rt_str_format(fmt, a, b, c) -> Str` | New owned string; substitutes up to three `{}` | EXEC |
| `rt_str_from_int(n) -> Str` | New owned string | EXEC |
| `rt_str_from_float(f) -> Str` | New owned string | EXEC (source signature) |
| `rt_str_from_bool(b) -> Str` | New owned string | SOURCE |
| `rt_str_len(s) -> Int`, `rt_str_byte(s, i) -> Int` | Borrow; byte-oriented | EXEC |
| `rt_str_eq(a, b) -> Bool` | Borrows both | EXEC |

Verified float output:

| Input | Output |
| --- | --- |
| `2.5` | `2.5` |
| `1.0` | `1` |
| `1.0 / 3.0` | `0.33333333333333331` |
| `0.1 + 0.2` | `0.30000000000000004` |
| `-0.0` | `-0` |

A string containing `\n` produces an extra break because the intrinsic adds
its own terminator (`rt_print_str(rt_str_concat("x", "\n"))` writes
`x\n\r\n`). `rt_str_format("a={} b={}", one, two, "")` with fewer substituted
values than placeholders leaves the remaining slots empty; it does not fail.

Known discrepancy: `src/backend/ir.rs` still describes `PrintFloat` as "up to
six fractional digits". The observed behavior is 17 significant digits. The
source comment was left unchanged; runtime semantics were not modified.

## 4. Strings (`mod strings`)

The module follows the V1 ownership contract: user functions **consume** their
`Str` parameters. Search/validation functions therefore return the consumed
input in a tuple so the caller can free it exactly once; transformations
consume the input and return a new string.

### 4.1 Search and compare

| Signature | Result and ownership | Evidence |
| --- | --- | --- |
| `str_cmp(a: Str, b: Str) -> Int` | `-1`, `0`, `1`; consumes and frees both | SOURCE |
| `str_index_of(s: Str, sub: Str) -> (Int, Str)` | `-1` when absent; frees `sub`, returns `s` | EXEC |
| `str_last_index_of(s, sub) -> (Int, Str)` | Last byte index; frees `sub`, returns `s` | SOURCE |
| `str_contains(s, sub) -> (Bool, Str)` | Returns `s` | EXEC |
| `str_starts_with(s, prefix) -> (Bool, Str)` | Returns `s` | SOURCE |
| `str_ends_with(s, suffix) -> (Bool, Str)` | Returns `s` | SOURCE |
| `str_count(s, sub) -> (Int, Str)` | Non-overlapping count; returns `s` | SOURCE |
| `str_is_numeric(s) -> (Bool, Str)` | Returns `s` | SOURCE |
| `str_is_alpha(s) -> (Bool, Str)` | Returns `s` | SOURCE |
| `str_is_alphanumeric(s) -> (Bool, Str)` | Returns `s` | SOURCE |
| `str_char_at(s: Str, i: Int) -> (Int, Str)` | Byte value; returns `s` | SOURCE |

```mink
mod strings;

fn main() -> Int {
    let (at, s) = str_index_of("hello world", "world");
    rt_print_int(at);          // 6
    rt_str_free(s);            // the input string, returned by the call
    let (yes, t) = str_contains("hello", "ell");
    if yes { rt_print_str("contains"); }
    rt_str_free(t);
    return 0;
}
```

Output (each line CRLF):

```text
6
contains
```

### 4.2 Transformation (input consumed, new string returned)

| Signature | Notes | Evidence |
| --- | --- | --- |
| `str_trim(s) -> Str` | ASCII whitespace at both ends | EXEC |
| `str_trim_start(s) -> Str`, `str_trim_end(s) -> Str` | One side | SOURCE |
| `str_to_upper(s) -> Str`, `str_to_lower(s) -> Str` | ASCII | EXEC (upper) |
| `str_sub(s, start, end) -> Str` | `end` exclusive; indices are bytes | EXEC |
| `str_repeat(s, count) -> Str` | Empty when `count <= 0` | SOURCE |
| `str_pad_left(s, len, pad_byte) -> Str` | `pad_byte` is a byte value (`48` = `'0'`) | EXEC |
| `str_pad_right(s, len, pad_byte) -> Str` | Same convention | SOURCE |
| `str_reverse(s) -> Str` | Byte order, not grapheme order | SOURCE |
| `str_replace(s, old, new) -> Str` | Replaces every occurrence | EXEC |
| `str_replace_all(s, old, new) -> Str` | Variant; see source | SOURCE |
| `str_join_2(a, b) -> Str`, `str_join_3(a, b, c) -> Str` | Plain concatenation helpers | SOURCE |

All of these consume their `Str` arguments. Passing a literal is safe because
freeing a literal is a no-op; passing a heap string passes ownership.

### 4.3 Split and join

| Signature | Contract | Evidence |
| --- | --- | --- |
| `str_split(s: Str, delim: Str) -> Vec<Str>` | Consumes both; each part is a fresh owned string; empty delimiter yields one part | EXEC |
| `str_join(parts: Vec<Str>, delim: Str) -> Str` | Consumes the vector (and its element strings) and the delimiter; returns an owned string | EXEC |

```mink
mod strings;

fn main() -> Int {
    let parts = str_split("a,b,c", ",");
    rt_print_int(rt_vec_len(parts));    // 3
    let joined = str_join(parts, "-");  // parts is consumed here
    rt_print_str(joined);               // a-b-c
    rt_str_free(joined);
    return 0;
}
```

Output: `3`, `a-b-c`.

### 4.4 Parsing

| Signature | Contract | Evidence |
| --- | --- | --- |
| `str_parse_int(s: Str) -> (Int, Bool)` | `(value, ok)`; `(0, false)` on junk or overflow; consumes `s` | EXEC |
| `str_parse_float(s: Str) -> (Float, Bool)` | Same convention | SOURCE |

The flag is a `Bool`. Do not pass it to an `if` when a module returns an `Int`
flag: `base64_decode_alloc` returns `(Str, Int)`, and `if ok` on that value is
`E-T01 expected Bool, found Int`.

## 5. Collections: `Vec`, `Map`, `Set`

Collections are built-in handle values. Operations take a handle and return a
handle; **assign the result of a consuming operation back to the variable**.
Read-only operations borrow. There is no automatic cleanup, and freeing a
collection handle more than once is an invalid free at runtime.

### 5.1 Intrinsics

| Intrinsic | Ownership | Evidence |
| --- | --- | --- |
| `rt_vec_new(cap) -> Vec<T>` | New handle | EXEC |
| `rt_vec_push(v, value) -> Vec<T>` | Consumes `v`, returns the handle to keep | EXEC |
| `rt_vec_pop(v) -> T` | Borrows `v`; returns the last value (observed `11`) | EXEC |
| `rt_vec_get(v, i) -> T`, `rt_vec_len(v) -> Int` | Borrow | EXEC |
| `rt_vec_set(v, i, value) -> Vec<T>` | Consumes, returns | EXEC |
| `rt_vec_remove(v, i) -> Vec<T>` | Consumes, returns | SOURCE |
| `rt_vec_free(v)` | Releases the handle and its element strings | EXEC |
| `rt_map_new(cap) -> Map<K,V>` | New handle | EXEC |
| `rt_map_insert(m, key, value) -> Map<K,V>` | Consumes the handle, key and value; returns the handle to keep | EXEC |
| `rt_map_get(m, key) -> V`, `rt_map_has(m, key) -> Bool`, `rt_map_len(m) -> Int` | Borrow | EXEC |
| `rt_map_remove(m, key) -> Map<K,V>` | Consumes, returns | EXEC |
| `rt_map_keys(m) -> Vec<K>`, `rt_map_values(m) -> Vec<V>` | Fresh vector of copies | EXEC |
| `rt_map_free(m)` | Releases the handle and its keys/values | EXEC |
| `rt_set_new(cap)`, `rt_set_insert(s, v)`, `rt_set_has`, `rt_set_len`, `rt_set_remove`, `rt_set_elements`, `rt_set_free` | Same handle pattern; `elements` returns `Vec<T>` | EXEC |

Map keys may be `Int`, `Bool`, `Char`, `Str`, or an enum whose variants carry
no payload. `Map<K, V>` annotations are accepted on bindings, parameters and
return types.

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
    rt_print_int(total(m));       // 30
    let built = build();
    rt_print_int(total(built));   // 30
    rt_print_int(total(built));   // 30: a map may be passed repeatedly
    rt_map_free(m);
    rt_map_free(built);
    return 0;
}
```

### 5.2 Contracts that matter

- A map handle can cross a function boundary and be used again by the caller.
  An unannotated parameter used with `rt_map_get` is inferred as a map; a
  partial inference failure can print `Map<unresolved, ...>`.
- `rt_map_insert` with an existing key replaces the value. The key is moved in,
  so a heap key must be a copy if the caller still needs the original string.
- Inserting an owned `Str` into a map moves it. `rt_map_free` releases keys and
  values, so do not free a string that was inserted.
- `rt_map_keys` / `rt_map_values` / `rt_set_elements` return owned vectors;
  free them with `rt_vec_free`. Their order is **not** insertion order; treat
  it as unspecified.
- **Use-after-free and stale handles are diagnosed (Sessions 195 and 197).**
  Every `Vec`/`Map`/`Set` accessor validates the handle against the runtime
  liveness table before reading its header, so `rt_map_free(m);
  rt_map_get(m, 1);` exits 105 (`E-R05`) instead of returning stale bytes.
  Handles also carry the allocation generation, so a freed handle whose block
  is **reused by a new allocation** is still rejected (`E-R05`, 105) rather
  than aliasing the replacement; freeing such a handle is `E-R05` too, while a
  plain double free remains `E-R04` (104). The generation is 16 bits, so the
  only remaining alias window is a handle kept alive across 65,536
  reallocations of the same block. Treat freed handles as dead.
- A `Vec<Str>` built with `rt_vec_push` inside deeply nested `if`/`else` blocks
  produced a corrupted length (16 for one element) and an invalid free in the
  Session 180 application. The same pattern worked at shallower nesting. The
  application was restructured to avoid it; see the consolidated session
  record. Treat collection mutation inside deep nesting as suspect and verify
  it.

### 5.3 `collections` module (`mod collections`)

Thin wrappers over the intrinsics: `vec_new`, `vec_free`, `vec_len`,
`vec_is_empty`, `vec_get`, `vec_set`, `vec_first`, `vec_last`, `vec_push`,
`vec_pop`, `vec_remove`, `vec_insert`, `vec_contains`, `vec_index_of`,
`vec_count`, `vec_sum`, `vec_min`, `vec_max`, `vec_reverse`.

`vec_insert(v, index, value)` consumes and returns the vector. `vec_reverse(v)`
mutates in place and returns the same handle. `vec_min`/`vec_max` on an empty
vector read index `0` and are not safe; check `vec_len` first. All `vec_sum`,
`vec_min`, `vec_max` assume numeric elements.

```mink
mod collections;

fn main() -> Int {
    let mut v = vec_new(4);
    v = vec_push(v, 10);
    v = vec_push(v, 20);
    v = vec_push(v, 30);
    rt_print_int(vec_sum(v));        // 60
    rt_print_int(vec_index_of(v, 99)); // -1
    v = vec_insert(v, 1, 99);
    rt_print_int(vec_get(v, 1));     // 99
    rt_print_int(vec_pop(v));        // 30
    vec_free(v);
    return 0;
}
```

## 6. Filesystem (`mod filesystem`)

`fs_*` wrappers consume their `Str` parameters. Literal arguments are safe;
heap paths are moved. Return values are the only error information: there is no
filesystem last-error call.

| Call | Success | Failure | Evidence |
| --- | --- | --- | --- |
| `fs_write(path, data) -> Int` | bytes written (`0` for empty data) | `-1` | EXEC |
| `fs_read(path) -> Str` | owned contents; empty file gives `""` | `""` | EXEC |
| `fs_file_size(path) -> Int` | bytes (`0` for empty) | `-1` | EXEC |
| `fs_exists(path) -> Bool` | `true` / `false` | - | EXEC |
| `fs_create_dir`, `fs_remove_dir`, `fs_remove_file`, `fs_copy_file`, `fs_move`, `fs_set_cwd` | `0` | `1` | EXEC (create/remove/copy/move per Session 177) |
| `fs_dir_open(path) -> Int` | handle | `0` | SOURCE + Session 177 |
| `fs_dir_next(handle) -> Str` | owned entry name | `""` when exhausted or `0` | SOURCE + Session 177 |
| `fs_dir_close(handle) -> Int` | `0` | `-1` for `0` | SOURCE + Session 177 |
| `fs_get_cwd() -> Str` | owned path | - | SOURCE |
| `fs_file_size_or(path, default) -> Int` | size or `default` | - | SOURCE |

`fs_read` cannot distinguish a missing file from an empty file: both return a
zero-length string. Use `fs_exists` or `fs_file_size` (`-1` vs `0`).

`fs_is_file` returns true when the path exists and its size is non-negative;
`fs_is_dir` returns true when the path exists and its size is negative. These
are size-based heuristics, not a real file-type query.

Path helpers (all consume their `Str` arguments): `path_join`, `path_parent`,
`path_filename`, `path_extension`, `path_stem`, `path_has_extension`,
`path_is_absolute`, `path_is_relative`, `path_with_extension`,
`path_normalize`. `path_join` uses `/` as its separator even on Windows and
returns `b` when `b` starts with `/`.

```mink
mod filesystem;

fn main() -> Int {
    let written = fs_write("note.txt", "hello");
    let size = fs_file_size("note.txt");
    let text = fs_read("note.txt");
    let missing = fs_read("absent.txt");
    rt_print_int(written);                       // 5
    rt_print_int(size);                          // 5
    rt_print_int(rt_str_len(text));              // 5
    rt_print_int(rt_str_len(missing));           // 0
    rt_print_int(fs_file_size("absent.txt"));    // -1
    rt_str_free(text);
    rt_str_free(missing);
    rt_print_int(fs_remove_file("note.txt"));    // 0
    return 0;
}
```

Directory enumeration: the handle is an owned resource and forgetting
`fs_dir_close` leaks. Entry order is unspecified.

## 7. JSON (`mod json`)

The parser is arena based. Allocate an arena with `rt_alloc(bytes)`, parse into
it, read values by word offset, and release the arena with `rt_free`. The
parser consumes its input string, so copy a string you still need.

| Call | Contract | Evidence |
| --- | --- | --- |
| `json_parse(input: Str, arena: Ptr<Int>) -> Int` | 64 KiB geometry (8192 slots); returns value offset, `0` on error; consumes `input` | EXEC |
| `json_parse_in(input, arena, slots) -> Int` | Caller-sized arena; `slots` includes the write-pointer slot | SOURCE |
| `json_tag(arena, offset) -> Int` | 0 null, 1 bool, 2 number, 3 string, 4 array, 5 object | SOURCE |
| `json_as_bool`, `json_as_int`, `json_as_str`, `json_num_str` | Typed reads; `json_as_str` returns an owned `Str` | EXEC (as_str/as_int) |
| `json_arr_len`, `json_arr_get(arena, arr, i) -> Int` | Array access by offset | EXEC |
| `json_obj_len`, `json_obj_get_key(arena, obj, i) -> Str`, `json_obj_get_val(arena, obj, i) -> Int` | Object access; the key is an owned `Str` | EXEC |
| `json_to_str(arena, offset) -> Str` | Owned serialization | EXEC |
| `json_null`, `json_bool_val`, `json_int_val`, `json_num_text`, `json_str_val`, `json_arr_start`, `json_obj_start` | Constructors that write at a word pointer | SOURCE |

```mink
mod json;

fn main() -> Int {
    let arena = rt_alloc(65536);
    let v = json_parse("{\"name\":\"mink\",\"count\":3}", arena);
    let key = json_obj_get_key(arena, v, 0);
    rt_print_str(key);                                  // name
    rt_str_free(key);
    let name = json_as_str(arena, json_obj_get_val(arena, v, 0));
    rt_print_str(name);                                 // mink
    rt_str_free(name);
    rt_print_int(json_as_int(arena, json_obj_get_val(arena, v, 1))); // 3
    let round = json_to_str(arena, v);
    rt_print_str(round);
    rt_str_free(round);
    rt_free(arena);
    return 0;
}
```

Output: `name`, `mink`, `3`, `{"name":"mink","count":3}`.

`json_parse` returns `0` for malformed input and for a document that does not
fit. A successful parse returns a non-zero offset; offset `0` can never be a
valid value because the arena begins with the input bytes.

**Arena vs. returned strings.** The arena is one `rt_alloc` block owned by the
caller and released with `rt_free(arena)`. The accessors do **not** return views
into it: `json_as_str`, `json_obj_get_key`, `json_num_str` and `json_to_str`
each allocate a new heap `Str` with `rt_str_alloc` and copy the bytes out. Those
results are ordinary owned strings — free each with `rt_str_free` — and they
stay valid after the arena is released. The exit-time leak scan counts the
arena and every copied string as separate live blocks, so both must be
released; otherwise the program exits 106 (`E-R06`).

## 8. CSV (`mod csv`)

RFC 4180 subset: comma separated, CR/LF/CRLF records, double-quoted fields,
`""` escapes a quote inside a quoted field. The module works one record at a
time; split a document on line endings first, but pass whole records whose
quoted fields contain newlines.

| Call | Contract | Evidence |
| --- | --- | --- |
| `csv_parse_row(line: Str) -> Vec<Str>` | Consumes `line`; returns owned field strings; empty input yields an empty vector | EXEC |
| `csv_escape(field: Str) -> Str` | Consumes; quotes/escapes only when required | EXEC |
| `csv_write_row(fields: Vec<Str>) -> Str` | Does **not** consume the vector; returns an owned line | EXEC |
| `csv_field_count(row: Vec<Str>) -> Int` | Borrows | EXEC |
| `csv_round_trip(fields: Vec<Str>) -> Vec<Str>` | Write then parse | SOURCE |

```mink
mod csv;

fn main() -> Int {
    let row = csv_parse_row("a,\"b,c\",\"d\"\"e\"");
    rt_print_int(csv_field_count(row));   // 3
    let out = csv_write_row(row);
    rt_print_str(out);                    // a,"b,c","d""e"
    rt_str_free(out);
    rt_vec_free(row);                     // releases the field strings
    return 0;
}
```

Forgetting `rt_vec_free(row)` leaves the field strings live and exits 106.

## 9. Encoding (`mod encoding`)

All functions consume `Str` parameters. The decoders return a tuple whose
second element is an **`Int` status** (`1` success, `0` failure), not a `Bool`;
using it directly as a condition is `E-T01`.

| Function | Result | Evidence |
| --- | --- | --- |
| `hex_encode(s) -> Str`, `hex_encode_upper(s) -> Str` | Hex text | EXEC |
| `hex_decode_alloc(s) -> (Str, Int)` | Decoded bytes and status | SOURCE |
| `base64_encode(s) -> Str`, `base64_url_encode(s) -> Str` | Base64 text | EXEC (standard) |
| `base64_decode_alloc(s) -> (Str, Int)`, `base64_url_decode_alloc(s) -> (Str, Int)` | Decoded and status | EXEC |
| `url_encode(s) -> Str`, `url_decode_alloc(s) -> (Str, Int)` | Percent encoding | EXEC (encode) |
| `utf8_validate(s) -> Bool`, `utf8_char_count(s) -> Int` | Borrow-only validation/count | PARTIAL |
| `utf8_decode(s) -> Vec<Int>`, `utf8_encode(codes: Vec<Int>) -> Str` | Code points | SOURCE |
| `utf8_char_at(s, i) -> Int`, `utf8_byte_index(s, i) -> Int`, `utf8_slice(s, start, end) -> Str` | Code-point indexing | SOURCE |
| `byte_is_digit/upper/lower/alpha/alphanumeric/hex/printable/whitespace(b: Int) -> Bool` | Byte classes | SOURCE |
| `str_is_ascii`, `int_to_hex`, `hex_to_int` | Helpers | SOURCE |

```mink
mod encoding;

fn main() -> Int {
    let h = hex_encode("AB");
    rt_print_str(h);                       // 4142
    rt_str_free(h);
    let b = base64_encode("hello");
    rt_print_str(b);                       // aGVsbG8=
    rt_str_free(b);
    let (plain, status) = base64_decode_alloc("aGVsbG8=");
    rt_print_str(plain);                   // hello
    rt_str_free(plain);
    if status == 1 { rt_print_str("b64-ok"); }
    return 0;
}
```

## 10. Hashing and crypto

### 10.1 `mod hashing`

| Function | Contract | Evidence |
| --- | --- | --- |
| `hash_fnv1a(s: Str) -> Int` | 64-bit FNV-1a; consumes `s` | SOURCE |
| `hash_djb2(s) -> Int` | djb2; consumes `s` | SOURCE |
| `hash_combine(a: Int, b: Int) -> Int` | Mix two hash values | SOURCE |
| `hash_sha256(s: Str) -> Str` | Lowercase hex digest, owned; consumes `s` | SOURCE |
| `hash_digest_equal(a, b) -> Bool` | Constant-time-ish compare; consumes both | SOURCE |
| `hex_encode_byte(b: Int) -> Str` | Two hex characters | SOURCE |

### 10.2 `mod crypto`

| Function | Contract | Evidence |
| --- | --- | --- |
| `crypto_init() -> Int` | Initialize the CSPRNG; `0` on success | SOURCE |
| `crypto_random_bytes(count) -> Str`, `crypto_random_hex(count) -> Str` | Owned random data | SOURCE |
| `crypto_random_int() -> Int` | Random 64-bit value | SOURCE |
| `hmac_sha256(key, message) -> Str` | Owned hex HMAC; consumes both | SOURCE |
| `hkdf_extract(salt, ikm) -> Str`, `hkdf_expand(prk, info, length) -> Str`, `hkdf_sha256(ikm, salt, info, length) -> Str` | HKDF-SHA256 | SOURCE |
| `crypto_verify(a, b) -> Bool` | Compare digests | SOURCE |
| `crypto_secure_zero_str(s) -> Str` | Overwrite then return the string | SOURCE |

`crypto.mink` declares its `hashing` dependency, so `mod crypto;` alone pulls
in `hash_sha256`. `crypto_init` should be called before random-byte use.

## 11. Math (`mod math`)

Functions are pure numeric and return values directly. Representative API:

| Group | Functions | Evidence |
| --- | --- | --- |
| Constants | `math_pi`, `math_e`, `math_tau`, `math_sqrt2`, `math_ln2`, `math_ln10`, `math_inf` | EXEC (pi) |
| Integer | `math_abs`, `math_min`, `math_max`, `math_clamp`, `math_sign`, `math_pow`, `math_factorial`, `math_isqrt`, `math_gcd`, `math_lcm`, `math_popcount`, `math_is_power_of_two`, `math_next_power_of_two` | EXEC (pow, gcd, isqrt, popcount, clamp) |
| Conversion | `math_int_to_float`, `math_float_to_int` | SOURCE |
| Float basics | `math_float_abs/min/max/sign/floor/ceil/round/trunc/pow/sqrt/ln/log2/log10/log/exp` | EXEC (sqrt, floor) |
| Trigonometry | `math_float_sin/cos/tan/asin/acos/atan/atan2`, `math_float_sinh/cosh/tanh` | SOURCE |
| Interpolation | `math_lerp`, `math_inverse_lerp`, `math_remap`, `math_degrees`, `math_radians`, `math_approximately_equal` | SOURCE |

Verified values: `math_pow(2, 10)` = `1024`, `math_gcd(48, 18)` = `6`,
`math_isqrt(144)` = `12`, `math_popcount(255)` = `8`,
`math_float_sqrt(2.0)` prints `1.4142135623730949`, `math_pi()` prints
`3.1415926535897931`, `math_float_floor(2.9)` prints `2`,
`math_clamp(15, 0, 10)` = `10`.

The float implementations are Taylor series / iterative approximations, not
hardware library calls. Accuracy was not measured beyond the values above.

## 12. Time (`mod time`)

| Function | Contract | Evidence |
| --- | --- | --- |
| `time_now() -> Int` | Unix seconds | SOURCE |
| `time_millis() -> Int` | Milliseconds since boot (runtime tick base) | SOURCE |
| `time_ticks() -> Int`, `time_freq() -> Int` | High-resolution counter and frequency | SOURCE |
| `duration_ms(start, end) -> Int`, `duration_us(start, end) -> Int` | Differences of tick values | SOURCE |
| `time_year/month/day/hour/minute/second(ts) -> Int` | UTC calendar fields | EXEC |
| `time_weekday(ts) -> Int` | Day of week | SOURCE |
| `time_is_leap_year(y) -> Int`, `time_days_in_month(y, m) -> Int` | Calendar helpers | SOURCE |
| `time_format(ts) -> Str` | `YYYY-MM-DD HH:MM:SS`, owned | EXEC |
| `time_strftime(ts, fmt) -> Str` | `strftime` directives, owned | EXEC |
| `time_strptime(s, fmt) -> (Int, Bool)` | Parsed timestamp and success flag; consumes both strings | EXEC |
| `time_epoch(y, mo, d, h, mi, s) -> Int` | Build a UTC timestamp | EXEC |
| `time_add(ts, seconds) -> Int`, `time_diff(a, b) -> Int` | Arithmetic | SOURCE |
| Name helpers | `time_month_name/abbr`, `time_weekday_name/abbr` | SOURCE |

Verified: `time_epoch(2026, 11, 5, 12, 30, 45)` = `1793881845`, and
`time_format` / `time_strftime("%Y-%m-%d %H:%M:%S")` both print
`2026-11-05 12:30:45`. `time_strptime("2026-11-05", "%Y-%m-%d")` returns
`1793836800`, true.

`time_now` is wall-clock Unix seconds; `time_millis` is not a Unix timestamp,
so do not mix the two.

## 13. Logging (`mod logging`)

Levels use Python's numeric values: DEBUG 10, INFO 20, WARNING 30, ERROR 40,
CRITICAL 50. The threshold is stored in the process environment as
`MINK_LOG_LEVEL`; there are no mutable globals. Records go to **stderr** as
`LEVEL: message\n`.

| Call | Contract | Evidence |
| --- | --- | --- |
| `log_set_level(level) -> Int` | `0` success, `-1` failure | EXEC |
| `log_set_level_name(name: Str) -> Int` | Consumes the name; `0` success | EXEC |
| `log_get_level() -> Int` | Current threshold (default 20) | EXEC |
| `log_enabled(level) -> Bool` | Level passes threshold | EXEC |
| `log_log(level, msg: Str) -> Int` | Consumes the message | SOURCE |
| `log_debug/info/warning/error/critical(msg: Str) -> Int` | Convenience wrappers | EXEC |
| `log_level_name(level) -> Str` | Owned or literal name | SOURCE |
| `log_level_from_name(name) -> Int` | `0` for unknown | SOURCE |

```mink
mod logging;

fn main() -> Int {
    rt_print_int(log_set_level_name("DEBUG"));  // 0
    log_debug("dbg-line");                      // stderr
    rt_print_int(log_set_level(40));            // 0
    log_info("suppressed");
    log_error("err-line");                      // stderr
    if log_enabled(30) { rt_print_str("warn-enabled"); }
    return 0;
}
```

stdout: `0`, `0`, `warn-disabled` (at level 40). stderr:
`DEBUG: dbg-line`, `ERROR: err-line`.

## 14. Process and environment

### 14.1 `mod process`

| Call | Contract | Evidence |
| --- | --- | --- |
| `process_run(cmd: Str) -> Int` | Child exit code; consumes `cmd` | EXEC |
| `process_run_input(cmd, input) -> Int` | Writes up to one pipe buffer of stdin | SOURCE |
| `process_stdout() -> Str`, `process_stderr() -> Str` | **BSS-backed capture buffers: print, do not free** | EXEC |
| `process_stdout_len() -> Int`, `process_stderr_len() -> Int` | Captured byte counts | EXEC |
| `process_id() -> Int` | Current PID | EXEC |
| `process_is_valid_cmd(cmd) -> Bool`, `process_run_ok(cmd) -> Bool`, `process_has_output(cmd) -> Bool` | Helpers; consume their strings | SOURCE |

Freeing a captured buffer is `E-R04 invalid free` (exit 104). This was
reproduced:

```mink
mod process;

fn main() -> Int {
    let code = process_run("cmd /c echo hello-proc");  // 0
    let out = process_stdout();
    rt_print_int(rt_str_len(out));                     // 12
    rt_print_str(out);                                 // hello-proc
    // Do NOT call rt_str_free(out).
    return 0;
}
```

`rt_stderr_write` does not append a newline, so a diagnostic written with it
runs into the next stderr output.

### 14.2 Environment (`mod environment`)

Sessions 178-181 recorded this module as unloadable: it declared raw
`extern fn rt_env_*;` items, and the parser accepts only `fn`, `let` and
`const` at top level (`E-P01`). Session 182 repaired it by wrapping the
predeclared intrinsics in ordinary functions. `mod environment;` now loads
and the tests in `tests/environment_lib.rs` pin the module contract.

| Call | Contract | Evidence |
| --- | --- | --- |
| `env_get(name: Str) -> Str` | Owned value; empty string when unset; consumes `name` | EXEC |
| `env_set(name: Str, value: Str) -> Int` | `0` success, `-1` failure; consumes both strings | EXEC |
| `env_has(name: Str) -> Bool` | Existence; consumes `name` | EXEC |
| `env_remove(name: Str) -> Int` | `0` success, `-1` failure; consumes `name` | EXEC |
| `env_cwd() -> Str` | Owned current directory | EXEC |

The underlying `rt_env_*` and `rt_fs_get_cwd` intrinsics remain predeclared,
so they also work **without** `mod environment;`. Prefer the module wrappers
for the unprefixed API used by the rest of the standard library.

```mink
mod environment;

fn main() -> Int {
    env_set("MINK_EXAMPLE", "on");
    let value = env_get("MINK_EXAMPLE");
    rt_print_str(value);
    rt_str_free(value);
    env_remove("MINK_EXAMPLE");
    return 0;
}
```

Windows variable names are case-insensitive; empty values are supported.

## 15. Random (`mod random`)

Deterministic xorshift64* PRNG. It is **not** cryptographically secure.

| Call | Contract | Evidence |
| --- | --- | --- |
| `random_seed(seed: Int)` | Seed 0 is treated as 1 | SOURCE |
| `random_next() -> Int` | Full 64-bit range, may be negative | SOURCE |
| `random_int(min, max) -> Int` | Inclusive range | SOURCE |
| `random_bool() -> Int`, `random_byte() -> Int`, `random_float_bits() -> Int` | Scalar helpers | SOURCE |
| `random_choice(count) -> Int`, `random_shuffle_3(a, b, c) -> Int` | Selection helpers | SOURCE |
| `random_uniform_check(n) -> Int`, `random_bit_count(n) -> Int` | Diagnostics | SOURCE |

Use `crypto_random_*` when unpredictability matters.

## 16. Regular expressions (`mod re`)

Backtracking engine, arena based. Search functions return the consumed subject
back in a tuple, like `strings`.

| Call | Contract | Evidence |
| --- | --- | --- |
| `re_is_match(pat, text) -> (Bool, Str)` | Returns the subject | EXEC |
| `re_search(pat, text) -> (Int, Str)` | First match start, `-1` when absent | EXEC |
| `re_match_end(pat, text) -> (Int, Str)` | End offset | SOURCE |
| `re_full_match(pat, text) -> (Bool, Str)` | Whole-subject match | SOURCE |
| `re_find(pat, text) -> (Int, Int, Str)` | Start, end, subject | SOURCE |
| `re_count(pat, text) -> (Int, Str)` | Match count | SOURCE |
| `re_find_all(pat, text) -> (Vec<Int>, Str)` | Vec of match starts | EXEC |
| `re_split(pat, text) -> (Vec<Str>, Str)` | Owned parts vector | EXEC |
| `re_replace(pat, text, repl) -> Str` | Owned result, replaces all | EXEC |
| `re_replace_first(pat, text, repl) -> Str` | Owned result | SOURCE |

Verified: `re_is_match("^a+$", "aaa")` true; `re_search("b+", "abbbc")` = `1`;
`re_find_all("[0-9]+", "a1b22c333")` has 3 entries; `re_replace("[0-9]+",
"a1b22", "N")` = `aNbN`; `re_split(",", "a,b,c")` has 3 parts.

The engine takes a match-step budget; catastrophic patterns return no match
instead of running forever. The exact budget is internal.

## 17. Networking, HTTP and TLS

These modules were inspected but not executed in this session; network
behavior is covered by the M3/M4 records and `tests/network_lib.rs`,
`tests/http_lib.rs`, `tests/httpserver_lib.rs`.

### 17.1 `mod network`

`net_init`, `net_cleanup`, `net_last_error`, `net_socket(af, ty, proto)`,
`net_tcp_socket`, `net_udp_socket`, `net_connect`, `net_connect_timeout`,
`net_bind`, `net_listen`, `net_accept`, `net_send`, `net_recv`,
`net_recvfrom`, `net_sendto`, `net_sender_ip`, `net_sender_port`, `net_close`,
`net_shutdown`, `net_resolve`, `net_free_resolve`, `net_hostname`,
`net_htons`, `net_ntohs`, `net_is_valid_socket`, `net_is_ok`, `net_dial`,
`net_listen_on`, `net_poll`, `net_poll_alloc`, `net_poll_set`,
`net_poll_events`, `net_poll_revents`, `net_wait_readable`,
`net_wait_writable`, `net_can_read`, `net_can_write`, `net_would_block`,
`net_would_block_code`, `net_set_nonblocking`, `net_socket_error`,
`net_timeout_code`, `net_timeout_ms`, `net_recv_timeout`.

Constants are functions: `AF_INET()`, `SOCK_STREAM()`, `SOCK_DGRAM()`,
`IPPROTO_TCP()`, `IPPROTO_UDP()`, `SD_RECEIVE/SEND/BOTH`,
`net_pollin()`, `net_pollout()`.

Sockets are `Int` handles; `net_close` returns `0`/`-1`. `net_recv` returns an
owned `Str`. `net_init` should run once before socket use and `net_cleanup`
at shutdown.

### 17.2 `mod http`

Client only. `http_last_error`, `http_request(method, path)`,
`http_get(path)`, `http_add_header`, `http_finalize_request`,
`http_header(response, name)`, `http_content_length`, `http_status_code`,
`http_status_text`, `http_body`, `http_recv_all`, `http_send(request, host,
port)`, `http_client_get_with(host, port, path)`,
`http_client_post_with(host, port, path, body)`, `http_client_get(url)`,
`http_client_post(url, body)`. Error sentinels: `HTTP_ERR_SOCKET()` `-1`,
`HTTP_ERR_CONNECT()` `-2`, `HTTP_ERR_SEND()` `-3`, `HTTP_ERR_TOO_LARGE()`
`-4`, `HTTP_ERR_RECV()` `-5`; `MAX_BODY()` is 4 MiB.

`http_client_get` returns an owned `Str` containing the whole response
(headers and body), or an empty string on failure. Use `http_last_error` to
distinguish. The module declares `mod network;` itself, so `mod http;` is
enough.

### 17.3 `mod httpserver`

Server helpers: `http_server_listen`, `http_server_listen_backlog`,
`http_server_accept`, `http_server_accept_default`, `http_server_close`,
`http_server_read_request`, `http_server_serve`,
`http_server_request_line`, `http_server_method`, `http_server_path`,
`http_server_version`, `http_server_header`, `http_server_content_length`,
`http_server_body`, `http_server_parse`, `http_server_response`,
`http_server_response_text`, `http_server_ok`, `http_server_not_found`,
`http_server_send`, `http_server_respond`, `http_server_respond_ok`,
`http_server_respond_not_found`, `http_server_reason`.
Limits: `http_server_backlog()` 16, `http_server_timeout_ms()` 5000,
`http_server_recv_chunk()` 4088, `http_server_max_request()` 65536,
`http_server_max_headers()` 64.

### 17.4 `mod tls` and HTTPS

`tls_connect(host, port, ca_path, out_err: Ptr<Int>) -> Ptr<Int>`,
`tls_status(h)`, `tls_err(h)`, `tls_cipher(h)`, `tls_send(h, data)`,
`tls_recv(h, maxlen)`, `tls_close(h)`, `https_client_get_with(host, port,
path, ca_path)`, `https_client_get(url, ca_path)`. A CA bundle path is
required; certificate validation includes hostname and chain checks. Handles
are `Ptr<Int>` values released by `tls_close`. Not executed here.

## 18. SQLite (`mod sqlite`)

An in-memory SQL engine. The "database" is an opaque `Str` blob threaded
through every call; each mutating call returns the new blob. Results and
errors are tuples.

| Call | Contract | Evidence |
| --- | --- | --- |
| `sql_new() -> Str` | Empty database blob | EXEC |
| `sql_exec(db: Str, sql: Str) -> (Str, Str)` | `(result text, new db)`; consumes both inputs | EXEC |
| `sql_rows(db, table) -> (Int, Str)` | `(row count, new db)` | EXEC |
| `sql_has_table(db, name) -> (Bool, Str)` | `(exists, new db)` | EXEC |
| `sql_tables(db) -> (Str, Str)` | `(table list, new db)` | SOURCE |
| `sql_in_transaction(db) -> (Bool, Str)` | Transaction state and db | SOURCE |
| `sql_free(db)` | Releases the blob | EXEC |
| `sql_quote(s) -> Str`, `sql_field(line, index) -> Str` | Helpers | SOURCE |

```mink
mod sqlite;

fn main() -> Int {
    let mut db = sql_new();
    let created = sql_exec(db, "CREATE TABLE t (id INT, name STR)");
    rt_print_str(created.0);       // OK
    rt_str_free(created.0);
    db = created.1;
    let inserted = sql_exec(db, "INSERT INTO t VALUES (1, 'a')");
    rt_str_free(inserted.0);
    db = inserted.1;
    let q = sql_exec(db, "SELECT * FROM t");
    rt_print_str(q.0);             // 1\ta
    rt_str_free(q.0);
    db = q.1;
    let has = sql_has_table(db, "t");
    db = has.1;
    if has.0 { rt_print_str("has-table"); }
    sql_free(db);
    return 0;
}
```

Every tuple's second string is the *new* database handle and must be assigned
back. Discarding it and freeing the old `db` is an invalid free (reproduced:
`E-R04` at `stdlib/sqlite.mink:1476`). The engine is a teaching/embedded
implementation, not a SQLite binding; its SQL surface is limited to the
statements exercised in `examples/inventory_db`.

## 19. Compression and archives

### 19.1 `mod zlib`

| Call | Contract | Evidence |
| --- | --- | --- |
| `zlib_compress(data: Str, level: Int) -> Str` | Raw zlib stream, owned; consumes data | EXEC |
| `zlib_decompress(data: Str) -> (Str, Int)` | `(output, status)`; status `0` success | EXEC |
| `deflate_raw(data, level) -> Str`, `inflate_raw(data) -> (Str, Int)` | Raw deflate | SOURCE |
| `gzip_compress(data, level) -> Str`, `gzip_decompress(data) -> (Str, Int)` | gzip container | SOURCE |
| `zlib_crc32(data) -> (Int, Str)`, `zlib_adler32(data) -> (Int, Str)` | Checksum and the returned input string; free the second element | EXEC (crc32) |

Level is a compression effort from `0` to `9`-ish; `6` was used successfully.
`zlib_decompress` returns a tuple; passing a heap argument consumes it, so pass
a copy when the original is still needed.

### 19.2 `mod zip`

Builder: `zip_new()`, `zip_new_level(level)`, `zip_add(arc, name, data)`,
`zip_add_stored`, `zip_add_dir`, `zip_finish(arc) -> Str`, `zip_free(arc)`,
`zip_count(arc)`.

**`zip_finish` frees the archive arena itself.** Calling `zip_free(arc)` after
`zip_finish` is an invalid free (`E-R04`, exit 104), reproduced. Free only the
returned archive string.

Reader: `zip_read_count(data) -> (Int, Str)`, `zip_list(data) -> Vec<Str>`,
`zip_entry_name(data, i) -> (Str, Int)`, `zip_entry_size(data, i) -> (Int,
Str)`, `zip_entry_data(data, i) -> (Str, Int)`,
`zip_entry_data_by_name(data, name) -> (Str, Int)`, `zip_entry_is_dir`,
`zip_is_safe_path(name) -> Bool`, `zip_extract_all(data, dest) -> Int`,
`zip_extract_entry(data, i, dest) -> Int`.

Every reader call consumes the archive `Str`, so pass a copy when reusing it.
A composite reader probe (count + list + entry data) exited 106 with a leak;
individual list-only and entry-only probes exited 0. The leak source was not
isolated. Treat the reader as needing explicit cleanup review before
production use.

## 20. Threads and tasks

### 20.1 `mod threads`

`thread_spawn(f, arg: Ptr<Int>) -> Int`, `thread_join(handle) -> Int`,
`thread_self() -> Int`, `lock_new() -> Int`, `lock_acquire(lock)`,
`lock_release(lock)`, `lock_free(lock)`. Thread arguments are raw `Ptr<Int>`
blocks allocated with `rt_alloc`; ownership of that memory is the caller's.
Not executed here.

### 20.2 `mod tasks`

`task_pending() -> Int`, `task_run() -> Int`, `task_stop()`,
`task_await(handle) -> Int`. Deferred work queue; not executed here.

## 21. Assertions and `mink test`

`mod assert` provides `assert_true`, `assert_false`, `assert_eq_int`,
`assert_ne_int`, `assert_eq_str`, `assert_ne_str`, `assert_eq_float`.

- On success they are silent.
- On failure they print a diagnostic to stderr and call `rt_exit(1)`.
- `assert_eq_str` / `assert_ne_str` consume both strings and free them on the
  success path.

`mink test file.mink` discovers top-level functions named `test_*`, runs each,
and prints `PASS:` / `FAIL:` lines plus a summary. Exit code 1 if any test
fails.

```bash
$ ./target/release/mink.exe test tmp/demo.mink
mink: test: found 2 test(s) in 'tmp/demo.mink'
  PASS: test_add
  PASS: test_str

mink: test result: 2 passed, 0 failed, 2 total
```

Observed limitation: `assert_eq_int`'s failure message printed only
`assertion failed: expected ` (the two values were missing from the captured
stderr). Do not rely on assertion messages for machine-readable failure
details; assert on the process exit code and your own diagnostics.

## 22. Option and Result

`stdlib/option.mink` and `stdlib/result.mink` contain only generic enum
declarations:

```mink
enum Option<T> { Some(T), None }
enum Result<T, E> { Ok(T), Err(E) }
```

There are no helper functions (`unwrap`, `map`, `?`). Matching is written with
`match`; a payload can be extracted through a variant pattern. Generic enum
instantiation was not exercised in this session; treat these declarations as
type vocabulary rather than a ready-made error-handling API.

## 23. Ownership cheat sheet

| Situation | Rule |
| --- | --- |
| String literal | Immortal; `rt_str_free` on it is a no-op |
| `rt_str_concat`, `rt_str_format`, `rt_str_from_*`, `fs_read`, `rt_argv` | Return owned strings; free exactly once |
| `json_as_str`, `json_obj_get_key`, `json_num_str`, `json_to_str` | Return heap copies, not arena views; free each, and they outlive `rt_free(arena)` |
| JSON arena from `rt_alloc` | One owned block; release with `rt_free(arena)` |
| `rt_print_str`, `rt_str_len`, `rt_str_byte`, `rt_str_eq` | Borrow; no free |
| User function with a `Str` parameter | Consumes (moves) the argument |
| Read-only user call | Pass `&s` and use `*s` in the callee |
| Returned string | Caller owns it |
| `rt_map_insert` / `rt_vec_push` on a `Str` | Moves the value into the collection |
| `rt_map_free` / `rt_vec_free` / `rt_set_free` | Releases the handle and its element strings |
| Leak at exit | `E-R06`, exit code 106; earlier output is kept |
| Invalid free | `E-R04`, exit code 104 |
| Collection use-after-free or stale handle | `E-R05`, exit 105 (Sessions 195, 197) |

There is no path-sensitive ownership: a value freed inside a conditional is
treated as moved for the rest of the function. Structure helpers with one
return and unconditional releases rather than freeing in branches.

## 24. What is not verified

- `crypto`, `hashing`, `random`, `tls`, `network`, `http`, `httpserver`,
  `threads`, `tasks`, `option`, `result`: source inspection only in this
  session. Their signatures are listed; behavior is covered (or not) by
  existing tests, not by this audit.
- `zip` reader leak source (composite probe exited 106).
- `fs_dir_*` execution: source and Session 177 evidence, not re-executed here.
- `math` accuracy beyond the listed values; trigonometry and logarithms were
  not executed.
- `encoding` decode status semantics were executed for Base64 only; hex/URL
  decode paths are source-only.
- `json` constructors and `json_parse_in` sizing were not executed.
- Generic enums (`Option<T>`, `Result<T, E>`) were not instantiated.
- Linux behavior: this reference is Windows x86_64 only.
- No module was audited for exhaustive edge-case coverage. Absence of a
  function here does not mean it is unsupported; check the module source.

## 25. Where to look next

- [`LANGUAGE_BASICS.md`](LANGUAGE_BASICS.md): runtime semantics, modules,
  ownership, filesystem conventions.
- [`DEBUGGING_AND_TESTING.md`](DEBUGGING_AND_TESTING.md): compiler diagnostics,
  test harnesses, bug-report template.
- [`CORE_GRAMMAR.md`](CORE_GRAMMAR.md): syntax authority.
- `stdlib/*.mink`: implementation and comments.
- `examples/`: complete programs.
- `benchmarks/m6/session180/`: the audit's cold-start application and verifier.
