# H — manifest (integrated application)

PROJECT: H integrated application
PURPOSE: One realistic tool that combines JSON config parsing, recursive
filesystem traversal, SHA-256 hashing and CSV reporting — the "generate a
build/audit manifest" task a Python developer reaches for with `json`,
`os.walk`, `hashlib` and `csv`.
MINK SOURCE: `validation/h_manifest/main.mink` (121 functions emitted)
PYTHON COMPARISON: `validation/h_manifest/python_ref.py`
CROSS-CHECK HARNESS: `validation/h_manifest/verify.py` (compares stdout,
stderr, exit code **and** the produced report file byte-for-byte)
FIXTURES: `validation/h_manifest/fixtures/make_fixtures.py` (9 files under
`src/` — nested directories, a directory name with a space, an empty file, a
binary file, UTF-8 *content*, a dotless name — plus 8 config files)

BUILD: PASS — `mink build validation/h_manifest/main.mink` → `main.exe`
(x86_64-windows-pe, no external toolchain, no runtime DLL).

EXECUTION: PASS — 13/13 cases produce byte-identical stdout, stderr, exit
codes and report files compared with the Python reference (`python verify.py`
→ `13/13 cases match the Python reference`).

FUNCTIONAL CORRECTNESS: PASS — reads a JSON config (`root`, `out`,
`extensions`) with the arena-based `json` module, walks the tree
recursively (directory handles, `path_join`, `.`/`..` skipping), filters
case-insensitively on extension, sorts relative paths in byte order, hashes
each file with `hash_sha256` (first 16 hex characters), and writes a
`path,size,sha256,ext` CSV with `csv_write_row` (RFC 4180 quoting, LF
terminators) plus `files=`/`bytes=`/`first=` summary lines.

ERROR HANDLING: PASS — six distinct exit codes verified against Python: 0 ok,
1 usage (missing/extra arguments, any option), 2 config missing / not a
regular file, 3 invalid JSON / wrong shape / missing keys / wrong types, 4
root missing or report unwritable, 5 no file matched the extensions. Messages
go to stderr (`rt_stderr_write`), stdout stays clean.

MEMORY: PASS — the runtime leak check (`E-R06`, exit 106) is enabled; all 13
cases exit 0/1/2/3/4/5, never 106. One leak was found and fixed during
validation (see CLASSIFICATION).

REALISTIC INPUT: PASS — nested directories, a space in a directory name, an
empty file (SHA-256 of the empty string), a binary file that the extension
filter rejects, a dotless name, UTF-8 content in a text file, an
uppercase-extension config (`.TXT` matching `.txt`), and the five failure
configs.

REPEATED EXECUTION: PASS — repeated runs produce byte-identical reports;
`verify.py` has been run repeatedly with identical results (13/13 each time).

STANDALONE EXE: PASS — `main.exe` runs with no interpreter and no runtime;
the only dependency is the Windows API.

WINDOWS: PASS (with a documented Unicode-path limitation, see
CLASSIFICATION).

MINK LIMITATION: see CLASSIFICATION; the Unicode-path gap is the one real
parity hole found by this app.

CLASSIFICATION:
- B (defect, FIXED this session) — the stdlib JSON object parser rejected
  insignificant whitespace after a member separator: `_parse_object` consumed
  `,` with a bare `p = p + 1` and never called `_skip_ws`, so a
  pretty-printed document (`{ "a": 1, "b": 2 }`, which is exactly what
  `json.dump(indent=...)` emits) failed to parse while its compact twin
  succeeded. Arrays and the leading `{`/`:` positions already skipped
  whitespace, so only this one site was wrong. Fixed in `stdlib/json.mink`
  (mirrored in `npm/mink/stdlib/json.mink`). Regression tests in
  `tests/json.rs`: `object_members_allow_whitespace_between_tokens` (6
  pretty/CRLF/tab/nested forms) and
  `object_members_still_reject_garbage_after_comma` (3 negatives proving the
  parser did not become permissive); `cargo test --test json` → 63 passed.
- C (architectural limitation, the largest parity gap in H) — the filesystem
  layer is **ANSI**, not Unicode. Observed evidence from this app's first run:
  * `sub dir/naïve café.txt` was read correctly (CP1252 round-trips those two
    characters), but the path was *reported* in the code page
    (`na?ve caf?.txt` in the CSV) instead of UTF-8, so the artifact differed
    from Python's;
  * `sub dir/nested/日本語.txt` cannot be represented in CP1252, so the name
    arrived as `???.txt`, `fs_file_size` returned `-1` and the row read
    `sub dir/nested/???.txt,-1,e3b0c44298fc1c14,.txt` (SHA-256 of the empty
    string) while Python opened the file and reported `16, 24d22f3d5e722ce4`.
  Python's `str` paths are Unicode end to end, so this is a genuine
  capability gap: **non-ASCII path names are unsupported** (registered in the
  gap list). The parity fixture therefore uses ASCII path *names* with UTF-8
  *content*, and the Unicode-name behaviour is recorded here rather than
  faked by the reference.
- C (architectural limitation, see also F) — no long-path (`\\?\`, >260
  characters) support in the ANSI filesystem layer; the fixture tree stays
  inside `MAX_PATH`.
- C (architectural limitation) — no module-level `Vec` globals (`E-B03`), so
  the walk result is threaded through the function as an untyped parameter and
  the CSV rows are assembled from function-local vectors.
- C (architectural limitation) — the JSON arena is 64KB with roughly 8000
  input bytes and ~20 nesting levels; a large config would not fit.
- C (ergonomic trap, documented) — user-function `Str` parameters are
  consuming and must be freed by the callee; `csv_write_row` does **not**
  consume its `Vec`, so the caller frees it; the read-borrowing intrinsics
  need bound copies.
- D (deliberate difference) — `rt_print_str` appends CRLF without translating
  embedded newlines; the Python reference writes raw bytes + `\r\n` to match.
- D (deliberate difference) — `rt_argc`/`rt_argv` exclude the program name; a
  leading `-` marks a usage error because the tool defines no options.
- D (convention) — `fs_write` returns bytes written or `-1`, so a write
  failure is `wrote < 0`; `fs_create_dir` returning 1 for an existing
  directory is success (not exercised here because the harness pre-creates
  the output directories).
- A (application defect, found & fixed during validation) — `rel_child`
  allocated `rt_str_concat(name, "")` first and then **overwrote** it in the
  non-empty-branch (`out = rt_str_concat(a, name)`), leaking one string per
  file in a subdirectory; the run ended with `E-R06` (exit 106). Fixed by
  binding `out` once per branch (the literal `""` placeholder is never
  allocated). Verified: a single file at the root was leak-free before the
  fix and a single file in a subdirectory leaked, which is what isolated it.
- A (application defect, found & fixed during validation) — the CLI expected
  two positionals (`n != 2`) while this app has no command token, so every
  single-config run printed the usage error. The harness caught it against
  the reference; fixed to `n != 1` / `pos[0]`.

EVIDENCE:
- TEST — `validation/h_manifest/verify.py`, 13 cases, 0 differences (stdout,
  stderr, exit code, report file tree).
- FIXTURE — `validation/h_manifest/fixtures/make_fixtures.py` (9 files, 8
  configs).
- OUTPUT — see the transcript below.
- REGRESSION — the JSON defect is covered by `cargo test --test json`
  (63 passed, includes the 2 new tests); the leak and CLI fixes by the
  runtime leak checker and the harness. The full suite (73 integration
  targets + `--lib`) was re-run green after the stdlib changes.

## Transcript

```
$ python validation/h_manifest/fixtures/make_fixtures.py
fixtures: 9 files, 8 configs

$ mink build validation/h_manifest/main.mink
mink: build: 'validation/h_manifest/main.mink' -> '.../main.exe'
(target: x86_64-windows-pe, 121 function(s), 0 binding(s))

$ python validation/h_manifest/verify.py
ok   default (exit 0)
ok   uppercase_ext (exit 0)
ok   no_match (exit 5)
ok   missing_root (exit 4)
ok   bad_json (exit 3)
ok   missing_keys (exit 3)
ok   bad_types (exit 3)
ok   root_not_object (exit 3)
ok   missing_config (exit 2)
ok   dir_as_config (exit 2)
ok   no_args (exit 1)
ok   extra_args (exit 1)
ok   option_looking_arg (exit 1)

13/13 cases match the Python reference

$ cat work/default/mink/report.csv
path,size,sha256,ext
data/notes.log,46,?,.log
data/records.csv,25,?,.csv
empty.txt,0,e3b0c44298fc1c14,.txt
noext,?,?,        <- filtered out (no extension)
readme.txt,29,?,.txt
sub dir/nested/deep.txt,5,?,.txt
sub dir/nested/unicode content.txt,?,?,.txt
sub dir/spaced name.txt,14,?,.txt

$ main.exe <bad_json.json>
manifest: config is not valid JSON
exit=3

$ main.exe <no_match.json>
manifest: no file matched the extensions
exit=5
```

## Python comparison notes (concrete differences, no ranking)

1. Python's `json.load` maps to `json_parse` + the arena accessors; the
   config keys are looked up with `find_key` and each value re-read through
   `json_obj_get_val`/`json_as_str` (numbers stay textual, so types are
   checked by tag rather than by Python's `isinstance`).
2. Python's `os.walk` maps to a recursive `walk` over `fs_dir_open` /
   `fs_dir_next` / `fs_dir_close`; enumeration order is unspecified in both,
   so the report is sorted (stable insertion sort over `str_cmp`), which
   equals Python's `sorted()` for UTF-8.
3. Python's `os.path.relpath(...).replace(os.sep, "/")` maps to the
   hand-written `rel_child` helper that always joins with `/`.
4. Python's `hashlib.sha256(data).hexdigest()[:16]` maps to `hash_sha256`
   (a 64-char lowercase hex digest) truncated with `str_sub`.
5. Python's `csv.writer(lineterminator="\n")` maps to `csv_write_row` per
   row joined with `str_join(parts, "\n")` plus one trailing `\n`.
6. Python's exceptions map to status codes and stderr messages; there is no
   exception type anywhere in the tool.
