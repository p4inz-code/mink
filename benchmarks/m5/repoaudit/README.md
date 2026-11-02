# repoaudit — M5 Application 1

Offline ASCII project inventory and policy audit, implemented in MINK for
Windows x86_64. This is the first of the three frozen M5 applications; the
contract is in
[`docs/benchmarks/M5/M5_APPLICATION_SELECTION.md`](../../../../docs/benchmarks/M5/M5_APPLICATION_SELECTION.md)
(section 5).

## What it does

```text
repoaudit <config.json>
repoaudit --help
```

The config is a JSON object with exactly these keys:

```json
{
  "root": "tree",
  "out": "out",
  "include_extensions": [".mink", ".md", ".txt"],
  "exclude_dirs": ["skip", "target"],
  "markers": ["TODO", "FIXME"],
  "max_file_bytes": 65536,
  "max_markers": 5
}
```

`out` must already exist. The tool writes:

| Artifact | Contents |
| --- | --- |
| `out/inventory.csv` | `path,bytes,lines,binary,todo,fixme`, sorted by path byte order |
| `out/violations.csv` | `path,rule,detail` in that same path order |
| `out/summary.json` | compact fixed-order totals |
| stdout | one CRLF line: `files=N bytes=N lines=N todo=N fixme=N violations=N` |

Exit codes: `0` clean, `1` usage, `2` config missing/not a file, `3` config
JSON/schema, `4` root missing/not a directory, `5` out missing or report write
failure, `6` violations found after reports were written, `7` scan I/O failure.

## Build

From the repository root:

```bash
target/release/mink.exe build benchmarks/m5/repoaudit/app/main.mink
mkdir -p benchmarks/m5/repoaudit/build
cp benchmarks/m5/repoaudit/app/main.exe benchmarks/m5/repoaudit/build/repoaudit.exe
```

The artifact is one native PE executable with no sidecar files and no
third-party runtime dependency beyond Windows. Build outputs are ignored.

## Test

```bash
python benchmarks/m5/repoaudit/tests/run_tests.py
```

`--no-build` reuses the existing `build/repoaudit.exe`; `--update-goldens`
regenerates `expected/` from the independent Python oracle. The oracle is a
test driver only; it is not a second shipped implementation.

The suite covers the committed basic/clean/empty fixtures, byte-exact goldens,
all exit classes, malformed and boundary configs, exclusions, mixed-case
extensions, CRLF/LF/empty/oversized/binary files, path spaces, repeated-run
determinism, a Windows sharing-lock scan failure, and an isolated standalone
executable.

## Frozen semantics and notes

- `lines` is `0` for an empty text file, otherwise LF count plus one when the
  last byte is not LF. Binary and size-skipped files use `-1`.
- `binary` is `1` when a NUL byte appears in the first 4,096 bytes; binary
  files contribute no marker counts and no marker-limit violation.
- Marker matches are non-overlapping and case-sensitive. `markers[0]` feeds
  the `todo` column and `markers[1]` feeds `fixme`; the frozen report schema has
  no column for additional configured markers. Entries 3–4 are accepted by the
  schema but do not affect the frozen reports.
- Paths are ASCII, use `/`, and are relative to `root`. Enumeration order never
  reaches the reports because paths are sorted in byte order after the walk.
- CSV fields containing a comma, quote, CR, or LF are RFC-4180 quoted. CSV line
  endings are LF; stdout is CRLF; `summary.json` has no trailing newline.
- `exclude_dirs` names are base names. Slash, backslash, and control characters
  are rejected, which is consistent with Windows base-name constraints.
- No Unicode path guarantee, glob/gitignore support, hashing, archives,
  network, process execution, watch mode, auto-fix, or parallel scanning.
- No MINK compiler, runtime, or stdlib source is modified by this application.
