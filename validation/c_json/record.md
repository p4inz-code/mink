# C — jsontool (JSON processing)

PROJECT: C JSON processing
PURPOSE: Parse a JSON document and inspect / extract / aggregate /
re-serialize it — the "load the config or API payload and read fields"
task a Python developer reaches for with `json.loads` + dictionary access.
MINK SOURCE: `validation/c_json/main.mink` (108 functions emitted)
PYTHON COMPARISON: `validation/c_json/python_ref.py`
CROSS-CHECK HARNESS: `validation/c_json/verify.py`
FIXTURES: `validation/c_json/fixtures/make_fixtures.py` (14 files: config-like
object, array of objects, nested object, the four scalar types, empty array,
empty object, escaped string, multibyte UTF-8 value, three invalid documents)

BUILD: PASS — `mink build validation/c_json/main.mink` → `main.exe`
(x86_64-windows-pe, no external toolchain, no runtime DLL).

EXECUTION: PASS — 43/43 cases produce byte-identical stdout, stderr and exit
codes compared with the Python reference (`python verify.py` → `43/43 cases
match the Python reference`).

FUNCTIONAL CORRECTNESS: PASS — `validate` (parse only), `type` (root value
type), `keys` (object member names in document order), `count` (array length),
`get` (object member re-serialized), `sum` (sum an integer field over an array
of objects), `compact` (re-serialize the whole document). Uses the arena-based
stdlib `json` module: `json_parse` (input pre-read into the arena, then
freed), the typed accessors (`json_tag`, `json_as_int`, `json_obj_len`,
`json_obj_get_key`, `json_obj_get_val`, `json_arr_len`, `json_arr_get`) and
`json_to_str`.

ERROR HANDLING: PASS — distinct exit codes verified against Python: 1 usage
(no/one positional, missing/extra args, any `-`-prefixed argument, unknown
command), 2 input missing / not a regular file, 3 the document is not valid
JSON, 4 wrong shape (not an object/array for the chosen command, missing
member). Messages go to stderr (`rt_stderr_write`), stdout stays clean.

MEMORY: PASS — the runtime leak check (`E-R06`, exit 106) is enabled; all 43
cases exit 0/1/2/3/4, never 106, including every failing path. Three leaks
were found and fixed during validation (see CLASSIFICATION).

REALISTIC INPUT: PASS — a nested config object, an array of records, the four
scalar roots, an empty array/object, a string with `\n`, quotes and a
backslash, and a multibyte UTF-8 value (raw bytes preserved, no `\u` escapes
in the fixtures).

REPEATED EXECUTION: PASS — repeated runs of the same command produce identical
bytes; `verify.py` has been run repeatedly with identical results (43/43 each
time), and `compact` is stable across two `json_to_str` calls because the
arena write pointer is persistent.

STANDALONE EXE: PASS — `main.exe` runs with no interpreter and no runtime;
the only dependency is the Windows API.

WINDOWS: PASS.

MINK LIMITATION: see CLASSIFICATION; none of the findings blocked the
application.

CLASSIFICATION:
- C (architectural limitation) — the stdlib `json` module is arena-based and
  bounded: a 64KB arena (`rt_alloc(65536)`), roughly 8000 input bytes and
  ~20 nesting depth, and the serializer writes into arena slots 7168..8191
  (its own documented limit), so a document whose parsed image grows into
  that region would collide. The fixtures stay well inside the documented
  bound.
- C (architectural limitation) — JSON numbers keep their source text: the
  parser stores the digits verbatim, so `json_to_str` reproduces `42` or
  `10.50` exactly, but `json_as_int` only decodes integer text (a `.` is
  treated as a digit and corrupts the value). `sum` therefore only sums
  integer-valued fields; a separate float path would be needed for decimals.
- C (architectural limitation) — no `\uXXXX` decoding beyond ASCII: an escape
  whose code point is > 127 is replaced with `?`, so the fixtures use raw
  UTF-8 bytes (which pass through unchanged) rather than `\u` escapes.
- C (architectural limitation) — no module-level `Vec` globals (`E-B03`); the
  argument vector is a function local, and `Vec` parameters are passed by
  value and are not consumed (the caller frees them), so the `cmd_*` helpers
  receive the argv vector plus an index.
- C (ergonomic trap, documented) — a user function's `Str` **parameter is
  consuming**: the callee must free it, and a caller that still needs its
  string must pass a copy (`rt_str_concat(s, "")`). Two of the three leaks
  below came from this rule.
- C (ergonomic trap, documented) — the read-borrowing intrinsics
  (`rt_str_concat`, `rt_print_str`, `rt_str_eq`, `rt_fs_*` reads) do not free
  their arguments; inline temporaries leak and the run ends with `E-R06`.
- D (deliberate difference) — `rt_print_str` appends CRLF without translating
  embedded newlines; the Python reference writes raw bytes + `\r\n` to match.
- D (deliberate difference) — `rt_argc`/`rt_argv` exclude the program name;
  argv[0] is the first user argument in both implementations.
- D (deliberate difference) — the tool loads and parses the file **before**
  dispatching the command, so a missing file or invalid JSON is reported even
  for an unknown command (`unknown_command_missing_file` exits 2,
  `unknown_command_broken_file` exits 3). The reference mirrors that ordering
  exactly.
- D (convention) — `fs_exists`/`fs_is_file`/`fs_read` are consuming stdlib
  wrappers (`0=ok, 1=error` for the mutators, an owned `Str` for `fs_read`);
  the app passes copies, and `json_parse` consumes and frees the input.
- A (application defect, found & fixed during validation) — three leaks, all
  caught by the leak checker (`E-R06`, exit 106):
  1. `load_doc` pre-allocated `let mut content = rt_str_alloc(0)` and then
     reassigned `content = fs_read(...)`, leaking the empty string; fixed by
     testing existence first and returning the owned content from each path;
  2. `find_key(arena, obj, name)` declared `name: Str` (consuming) but never
     freed it — fixed with `rt_str_free(name)` at its single exit, and callers
     now pass a copy;
  3. `return fail_msg("usage: ...")` returned the *number of stderr bytes
     written* as the process exit code (observed as 42/44 instead of 1/4);
     fixed by making the usage/shape branches print the message and then
     return an explicit `1`/`4`.
  `verify.py` then went from 35/43 to 43/43. No MINK compiler/runtime defect
  was found in C — the JSON module, the arena and the ownership analysis
  behaved as documented.

EVIDENCE:
- TEST — `validation/c_json/verify.py`, 43 cases, 0 differences (stdout,
  stderr, exit code).
- FIXTURE — `validation/c_json/fixtures/make_fixtures.py` (14 files).
- OUTPUT — see the transcript below.
- REGRESSION — the leak fixes are application-level; the runtime leak checker
  (`E-R06`) is the regression net and every case passes it. The full test
  suite (73 integration targets + `--lib`, 3040 tests) was re-run green before
  starting C, including `cargo test --test json` (61 passed).

## Transcript

```
$ python validation/c_json/fixtures/make_fixtures.py
fixtures: 14 files under .../validation/c_json/fixtures

$ mink build validation/c_json/main.mink
mink: build: 'validation/c_json/main.mink' -> '.../main.exe'
(target: x86_64-windows-pe, 108 function(s), 0 binding(s))

$ python validation/c_json/verify.py
ok   validate_object (exit 0)
ok   validate_array (exit 0)
ok   validate_scalar (exit 0)
ok   validate_broken (exit 3)
ok   validate_trailing (exit 3)
ok   validate_empty (exit 3)
ok   type_object (exit 0)
ok   type_array (exit 0)
ok   type_number (exit 0)
ok   type_string (exit 0)
ok   type_bool (exit 0)
ok   type_null (exit 0)
ok   keys_object (exit 0)
ok   keys_empty_object (exit 0)
ok   keys_on_array (exit 4)
ok   count_array (exit 0)
ok   count_empty_array (exit 0)
ok   count_on_object (exit 4)
ok   get_qty (exit 0)
ok   get_tags (exit 0)
ok   get_nested (exit 0)
ok   get_missing (exit 4)
ok   get_on_array (exit 4)
ok   sum_qty (exit 0)
ok   sum_missing_field (exit 0)
ok   sum_on_object (exit 4)
ok   compact_nested (exit 0)
ok   compact_escaped (exit 0)
ok   compact_unicode (exit 0)
ok   compact_number (exit 0)
ok   compact_empty_array (exit 0)
ok   compact_empty_object (exit 0)
ok   missing_file (exit 2)
ok   dir_as_file (exit 2)
ok   no_args (exit 1)
ok   one_positional (exit 1)
ok   unknown_command (exit 1)
ok   option_looking_arg (exit 1)
ok   get_missing_args (exit 1)
ok   get_extra_args (exit 1)
ok   compact_extra_args (exit 1)
ok   unknown_command_missing_file (exit 2)
ok   unknown_command_broken_file (exit 3)

43/43 cases match the Python reference

$ main.exe keys fixtures/object.json
name
qty
tags
active
meta
exit=0

$ main.exe get fixtures/object.json tags
["a","b"]
exit=0

$ main.exe sum fixtures/array.json qty
15
exit=0

$ main.exe type fixtures/bool.json
bool
exit=0

$ main.exe compact fixtures/escaped.json
{"msg":"line1\nline2 \"quoted\" \\ back"}
exit=0

$ main.exe validate fixtures/broken.json
jsontool: invalid JSON
exit=3
```

## Python comparison notes (concrete differences, no ranking)

1. Python's `json.loads` maps to `json_parse(input, arena)` — the document is
   parsed once into an arena and every accessor reads word offsets from it;
   there is no per-value Python object graph.
2. Python's `dict`/`list` maps to the arena value tags (5 = object, 4 =
   array); `d[key]` becomes `find_key` + `json_obj_get_val`, with the member
   re-serialized by `json_to_str` rather than `repr`-printed.
3. Python's `sorted(d)` maps to document order: `keys` prints members in the
   order they appear (like `dict` insertion order), not sorted — the
   reference iterates the parsed dict directly.
4. Python's numbers become `int`/`float`; MINK keeps the numeric text, so a
   value prints with its original spelling and `sum` only adds integer text.
5. Python's `json.dumps(value, separators=(",", ":"), ensure_ascii=False)`
   maps to `json_to_str`; both emit compact JSON with the same escaping for
   `"`, `\`, and the control escapes `\n \r \t \b \f`.
6. Python's `try/except json.JSONDecodeError` maps to the `json_parse` sentinel
   (offset 0) and exit 3; there is no exception type.
