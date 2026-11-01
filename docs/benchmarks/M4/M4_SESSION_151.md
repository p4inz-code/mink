# Session 151 — M4 W1 `jsontool` and harness skeleton

**Date:** 2026-11-01
**Scope:** benchmark harness skeleton, deterministic W1 fixtures, the W1
`jsontool` contract, and five implementations. **No timing was measured and
no performance claim is made.**
**Result:** COMPLETE for W1 implementation and correctness. W1 correctness
gate PASS (23 cases × 5 languages = 115/115).

## 1. Contract resolution

`M4_WORKLOAD_SELECTION.md` defines four commands, four error classes and a
byte-exact CRLF output convention, but it does not fix the order in which
arguments, files and documents are checked. The M1 application in
`validation/c_json/main.mink` does, and the M1 record treats that order as a
deliberate difference from a naive CLI. Two ambiguities were resolved from
that evidence before any implementation was written.

### 1.1 Resolution A — file and JSON errors precede command dispatch

M1 reads and parses the input file **before** the command is dispatched. Its
`unknown_command_missing_file` case exits 2 and
`unknown_command_broken_file` exits 3.

**M4 decision:** preserve the M1 ordering. The check sequence is:

1. any argument beginning with `-` → `jsontool: this tool has no options`, exit 1;
2. fewer than two positional arguments → general usage, exit 1;
3. path does not exist → `jsontool: input file does not exist: <path>`, exit 2;
4. path is not a regular file → `jsontool: not a regular file: <path>`, exit 2;
5. document is not valid JSON → `jsontool: invalid JSON`, exit 3;
6. wrong argument count for the command → command usage, exit 1;
7. wrong root shape for `sum` → `jsontool: document is not an array`, exit 4.

The consequence is verified, not incidental: `frobnicate broken.json` exits 3,
not 1.

### 1.2 Resolution B — a missing `sum` field is not an error

The M4 table lists "exit 4 wrong root shape or missing field". M1's only
"missing field" case is `sum_missing_field`, which exits **0** and prints `0`:
records that lack the field contribute nothing. M1 has no W1 field-extraction
command, so the "missing field" half of the M4 row has no W1 subject.

**M4 decision:** `sum` sums the integral values of records that contain the
field and exits 0. Exit 4 is used only for a non-array root. This is the M1
contract and is covered by the supporting case `sum_missing_field`.

### 1.3 The W1 contract

| Command | Arguments | stdout on success | Exit |
| --- | --- | --- | --- |
| `validate` | `<file>` | `ok` | 0 |
| `type` | `<file>` | `null` / `bool` / `number` / `string` / `array` / `object` | 0 |
| `sum` | `<file> <field>` | decimal integer | 0 |
| `compact` | `<file>` | compact JSON, member order preserved | 0 |

- stdout and stderr are UTF-8 with CRLF, no BOM; stdout is empty on every
  error path.
- Error text is stable and prefixed `jsontool: `; no stack traces, no
  platform-specific exception text.
- A completed 404-style application error does not exist in W1; all errors
  are the four classes above.
- The contract is deliberately smaller than M1's seven commands: `keys`,
  `count` and `get` are not part of M4 W1.

## 2. Fixtures

`benchmarks/m4/fixtures/w1_json/make_fixtures.py` writes 14 files with exact
bytes: compact JSON, no trailing newline, no random data.

| Fixture | Purpose | Bytes |
| --- | --- | --- |
| `object.json` | config-like root object, five member types | 69 |
| `array.json` | three-record array for `sum` | 52 |
| `number.json` | scalar root | 2 |
| `string.json` | scalar root | 7 |
| `bool.json` | scalar root | 4 |
| `null.json` | scalar root | 4 |
| `nested.json` | nested object/array | 54 |
| `records.json` | 50-record baseline document | **2,429** |
| `spaced.json` | whitespace-laden object for `compact` | 155 |
| `escaped.json` | `\n`, quotes and backslash in one value | 41 |
| `utf8.json` | raw multibyte UTF-8 values | 37 |
| `broken.json` | invalid document | 7 |
| `trailing.json` | valid document plus trailing garbage | 14 |
| `empty.json` | empty input | 0 |

`records.json` is the baseline-size document and stays inside the ≤2,800-byte
budget in `M4_WORKLOAD_SELECTION.md` and inside MINK's documented ~3.5 KiB
JSON envelope. The 4 KiB/5 KiB MINK-limit probes are **not** created in this
session; they remain unscored boundary evidence.

`utf8.json` and `escaped.json` use raw bytes rather than `\uXXXX` code points
because MINK's parser replaces non-ASCII escapes with `?` (M1 limitation C).

## 3. Golden cases

`benchmarks/m4/expected/w1_json/cases.json` is a **declarative** oracle: the
expected stdout, stderr and exit code for every case are authored from the
contract. The harness never runs an implementation to produce expected
values.

23 cases: the 10 scored cases from the M4 case budget plus 13 supporting
cases required by the contract (scalar `type` coverage, escaping, UTF-8,
missing `sum` field, wrong shape, usage, option-looking argument, extra
argument, trailing garbage, unknown command with a bad file).

| Scored case | Command |
| --- | --- |
| `validate_object` | `validate object.json` |
| `type_object` | `type object.json` |
| `type_array` | `type array.json` |
| `type_scalar_number` | `type number.json` |
| `sum_records` | `sum records.json qty` → `306` |
| `compact_spaced` | `compact spaced.json` |
| `validate_broken` | `validate broken.json` → exit 3 |
| `validate_empty` | `validate empty.json` → exit 3 |
| `validate_missing_file` | `validate missing.json` → exit 2 |
| `validate_directory` | `validate <fixtures dir>` → exit 2 |

## 4. Implementations

| Language | Path | Physical lines |
| --- | --- | --- |
| MINK 1.0.4 | `benchmarks/m4/apps/mink/w1_json/jsontool.mink` | 207 |
| Python 3.11.9 | `benchmarks/m4/apps/python/w1_json/jsontool.py` | 122 |
| Rust 1.97.1 | `benchmarks/m4/apps/rust/w1_json/src/main.rs` | 138 |
| Go 1.26.7 | `benchmarks/m4/apps/go/w1_json/main.go` | 158 |
| C# / .NET 10 | `benchmarks/m4/apps/csharp/w1_json/Program.cs` | 174 |

Line counts are descriptive only; they are not a productivity score.

Implementation notes:

- **MINK** is a reduced copy of the M1 logic with the same ownership style:
  consuming `Str` parameters, read-borrowing intrinsics, single exit, one
  arena allocation released at the end. No MINK compiler, runtime or stdlib
  change was made. MINK's `json_as_int` decodes integer text only; W1 `sum`
  fixtures use integers.
- **Python** mirrors the ordering and writes explicit `\r\n` to the binary
  stdout; `json.dumps(..., separators=(",", ":"), ensure_ascii=False)` gives
  the compact form.
- **Rust** uses `serde_json` with the `preserve_order` feature because the
  default `Map` sorts members, which would violate the compact contract.
- **Go** decodes with `json.Decoder.UseNumber()` and rejects trailing data
  with a second decode. `compact` uses `json.Compact`, which removes
  insignificant whitespace and preserves member order and the original
  number/escape text (byte-identical for the committed fixtures).
- **C#** uses `JsonDocument` and `Utf8JsonWriter` with
  `UnsafeRelaxedJsonEscaping` so non-ASCII characters are emitted as raw
  UTF-8 like every other implementation.

## 5. Dependencies

| Language | Dependency | Version | Source | License | Direct |
| --- | --- | --- | --- | --- | --- |
| MINK | bundled `json`, `filesystem`, `strings` | 1.0.4 | repository `stdlib/` | Apache-2.0 (repository) | yes |
| Python | standard library `json`, `os`, `sys` | 3.11.9 | CPython | PSF-2.0 | yes |
| Rust | `serde_json` | 1.0.151 | crates.io, local cache | MIT OR Apache-2.0 | yes |
| Go | standard library `encoding/json`, `os` | 1.26.7 | Go distribution | BSD-3-Clause | yes |
| C# | in-box `System.Text.Json` | .NET 10 | .NET SDK | MIT | yes |

Rust transitive dependencies and the full dependency audit are recorded in
[`M4_SESSION_152.md`](M4_SESSION_152.md) §4. No crate was downloaded: every
crate resolved from the existing local cargo cache and the build runs with
`--offline`.

## 6. Harness

| File | Purpose |
| --- | --- |
| `harness/common.py` | paths, language registry, `{fixtures}` resolution, launch commands |
| `harness/build.py` | builds MINK/Rust/Go/C# into `.build/<workload>/<language>/`; Python has no build step |
| `harness/run.py` | runs one case, captures stdout/stderr/exit, compares, optional JSON record |
| `harness/validate.py` | full gate, per-language summary, scored-case matrix, JSONL records |
| `harness/pe_imports.py` | read-only PE import-table inspection (no `objdump`/`dumpbin` on the host) |

The harness does not time anything. It records `stdout_sha256`,
`stderr_sha256`, the exit code and PASS/FAIL per case in
`results/raw/w1_json/correctness-w1_json.jsonl`. Build outputs and generated
records are git-ignored.

## 7. Build commands

```text
python benchmarks/m4/harness/build.py --workload w1_json
```

which performs:

| Language | Command |
| --- | --- |
| MINK | `target/release/mink.exe build <repo>/benchmarks/m4/.build/w1_json/mink/jsontool.mink` **from the repository root** |
| Python | none (interpreter runs the source) |
| Rust | `cargo build --release --offline --manifest-path .../Cargo.toml --target-dir ...` with `RUSTFLAGS=""` |
| Go | `go build -trimpath -buildvcs=false -o ... .` |
| C# | `dotnet publish -c Release -r win-x64 --self-contained false -o ...` |

Two environment facts were established by the build:

1. **MINK module resolution depends on the invocation directory.** Building
   from the artifact directory fails with `E-S01` for `filesystem.mink`,
   `json.mink` and `strings.mink`. Building from the repository root succeeds.
   The harness builds from the repository root while keeping the artifact in
   the benchmark build tree. This is a documented MINK behaviour, not a
   defect discovered in this session.
2. **The repository's `.cargo/config.toml` sets
   `rustflags = ["-C", "target-feature=+crt-static"]`.** The M4 environment
   baseline was captured with the default dynamic CRT, so the harness builds
   Rust with `RUSTFLAGS=""` to restore it. The resulting binary imports
   `VCRUNTIME140.dll` and the UCRT API sets, matching the baseline. This is a
   harness-local override; no project or global configuration was changed.

## 8. Correctness results

```text
23 cases x 5 languages
  mink    23/23 passed, 0 failed
  python  23/23 passed, 0 failed
  rust    23/23 passed, 0 failed
  go      23/23 passed, 0 failed
  csharp  23/23 passed, 0 failed

scored cases: 10
  validate_object          all five PASS
  type_object              all five PASS
  type_array               all five PASS
  type_scalar_number       all five PASS
  sum_records              all five PASS
  compact_spaced           all five PASS
  validate_broken          all five PASS
  validate_empty           all five PASS
  validate_missing_file    all five PASS
  validate_directory       all five PASS

GATE: PASS
```

All 115 case/language combinations pass byte-for-byte on stdout, stderr and
exit code.

## 9. Failures and fixes

No implementation defect was found: after the harness produced a launchable
binary for each language, all 23 cases passed on the first run. Three
**harness** defects were found and fixed:

1. MINK was built with the artifact directory as the working directory, so
   `mod` files were not found. Fixed by building from the repository root.
2. The Rust launch path omitted cargo's `release/` directory segment. Fixed
   in `common.run_command`.
3. Expected stderr for the missing-file case used `{fixtures}/name.json`,
   which resolved to a mixed `C:\dir/name.json` path, while every
   implementation prints the path it receives. Fixed by resolving
   `{fixtures}/` to an OS-native separator.

Expected outputs were not weakened for any implementation.

## 10. Environment assumptions

- Windows 11 x86_64, toolchains as pinned in
  `M4_ENVIRONMENT_BASELINE.md`; MINK compiler SHA-256
  `d8f6134850e2af1a7cd4f38a7e8a80d7692d566eac1bae97ddc28ed827149f09`.
- No dependency download was performed. Rust resolved from the local cache
  with `--offline`; C# restore used the in-box framework only; Go has no
  module dependencies.
- `python`, not `python3`, is invoked by the harness (the Store stub is
  unusable on this host).
- All harness processes run from the repository root.

## 11. Git state

| Item | Value |
| --- | --- |
| HEAD | `691ec38386bb37e204b834e61b0ff8de99a0ac65` (unchanged) |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` (unchanged) |
| Staged | none |
| New in this session | untracked `benchmarks/` tree and `docs/benchmarks/M4/M4_SESSION_151.md` |
| Modified tracked | unchanged 40 pre-existing Session 128–149 files |
| `git diff --check` | clean |
| Commit | none in this session; see §13 |

No reset, clean, stash, rebase, amend, push, tag or publish was performed.
The benchmark files are new and are isolatable from the pre-existing
modified files; a commit decision is recorded in the final report.

## 12. Limitations

- W1 measures one fixture family; MINK's JSON arena ceiling above ~3.5 KiB is
  not exercised.
- `sum` is defined over integral values. A decimal field is outside the W1
  contract; MINK's textual `json_as_int` would treat `.` as a digit while the
  other four implementations skip non-integers.
- MINK does not escape control characters other than
  `\n \r \t \b \f`, while Python/Rust/C# escape them as `\u00XX` and Go's
  `json.Compact` leaves them raw. No fixture contains such a character.
- Go's `compact` removes whitespace instead of re-encoding: a document with a
  non-canonical escape (for example `\/`) would differ. No fixture contains
  one.
- MINK parses bytes rather than validated UTF-8; an invalid UTF-8 string
  would pass MINK and fail the other four. No fixture contains invalid UTF-8.
- No timing, memory or build-time figure was collected. Nothing in this
  session is M4 performance evidence.

## 13. Next step

Session 152 — W1 audit and reproducibility:

1. clean-build every implementation from the documented commands and record
   artifacts and restore requirements;
2. repeat the correctness suite and confirm identical bytes;
3. audit argument parsing, numeric handling, compact output, file reading,
   error codes and stdout encoding across the five implementations;
4. record the exact dependency set, versions, licenses and restore behaviour;
5. mark W1 COMPLETE only if every criterion in `M4_WORKLOAD_SELECTION.md` §9
   is satisfied.
