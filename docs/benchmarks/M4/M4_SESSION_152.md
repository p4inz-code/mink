# Session 152 — W1 audit and reproducibility

**Date:** 2026-11-01
**Scope:** clean-build verification, repeatability, contract audit and
dependency audit for W1 `jsontool`. **No timing was measured, no cold-cache
claim is made and no product conclusion is drawn from implementation size.**
**Result:** COMPLETE. W1 is marked **COMPLETE** under the criteria in
`M4_WORKLOAD_SELECTION.md` §9.

## 1. Clean-build verification

`benchmarks/m4/.build/` was removed and every implementation was rebuilt with
`python benchmarks/m4/harness/build.py --workload w1_json`. Only the
benchmark-owned build directory was deleted; no shared compiler, cargo or
NuGet cache was touched.

| Language | Command | Artifact | Bytes |
| --- | --- | --- | --- |
| MINK | `target/release/mink.exe build <repo>/benchmarks/m4/.build/w1_json/mink/jsontool.mink` (cwd = repository root) | `jsontool.exe` | 161,280 |
| Python | none | `jsontool.py` (source) | 3,405 |
| Rust | `cargo build --release --offline --manifest-path ... --target-dir ...` | `release/jsontool.exe` | 211,456 |
| Go | `go build -trimpath -buildvcs=false -o ... .` | `jsontool.exe` | 3,249,152 |
| C# | `dotnet publish -c Release -r win-x64 --self-contained false -o ...` | `jsontool.exe` apphost + `jsontool.dll` 8,192 B + `.deps.json` 472 B + `.runtimeconfig.json` 449 B | 162,304 (+ 9,113) |

Each artifact was launched through the harness and passed the full suite, so
the clean-build result is verified by behaviour, not by file existence alone.

Restore requirements:

| Language | Restore |
| --- | --- |
| MINK | none |
| Python | none |
| Rust | none online: all crates present in the local cargo cache; `--offline` succeeded and no registry update occurred |
| Go | none: zero module dependencies |
| C# | `dotnet publish` restored only the project's own assets (`Restored ... (in 62 ms)`); no NuGet package was downloaded |

### Artifact dependencies (PE import tables)

Read with `harness/pe_imports.py` because `objdump`/`dumpbin` are absent.

| Artifact | Imports |
| --- | --- |
| MINK | `kernel32.dll` |
| Rust | `VCRUNTIME140.dll`, UCRT API sets, `KERNEL32.dll`, `ntdll.dll`, `bcryptprimitives.dll`, `api-ms-win-core-synch-l1-2-0.dll` |
| Go | `kernel32.dll` |
| C# apphost | `SHELL32.dll`, `ADVAPI32.dll`, `KERNEL32.dll`, `USER32.dll`, UCRT API sets; also needs the .NET 10 runtime and `jsontool.dll`**

** The C# apphost also requires the adjacent `.dll`, `.deps.json` and
`.runtimeconfig.json`; the apphost is not a self-contained program.

The Rust import list confirms the intended configuration: the repository's
`+crt-static` flag was overridden with `RUSTFLAGS=""`, so the binary matches
the dynamic-CRT baseline recorded in Session 150 instead of silently
changing it.

## 2. Repeatability

The full 23-case suite was run three times after the clean build, with the
JSONL record stream captured each time.

| Run | Result | Record-stream SHA-256 |
| --- | --- | --- |
| 1 | 115/115 pass | `e7f38e3641c0d457a47c6aff65d1c8734af958dbe2e15d8508200f0b3621f044` |
| 2 | 115/115 pass | same |
| 3 | 115/115 pass | same |

`diff` between consecutive record streams was empty. The record stream covers
the exit code and the SHA-256 of stdout and stderr for every case. W1
produces no output files, so "same generated files" is satisfied vacuously;
the fixture tree is read-only and is not modified by any run.

No performance statistic was collected.

## 3. Contract audit

Every row below was checked against the 23-case suite; "same" means the
observable bytes and exit codes are identical.

| Aspect | MINK | Python | Rust | Go | C# | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| Option-looking argument | first byte `-` | `startswith("-")` | `starts_with('-')` | `arg[0] == '-'` | `StartsWith('-')` | same |
| Minimum arity | `< 2` positional | `< 2` | `< 2` | `< 2` | `< 2` | same |
| File existence / regular file | `fs_exists` + `fs_is_file` | `os.path.exists` + `isfile` | `Path::exists` + `is_file` | `os.Stat` + `IsDir` | `File.Exists` + `Directory.Exists` | same messages and codes |
| Parse before dispatch | yes | yes | yes | yes | yes | same |
| Invalid JSON | `json_parse` returns 0 | `json.loads` raises | `from_slice` errors | `Decode` errors | `JsonDocument.Parse` throws | same, exit 3 |
| Trailing garbage | fails EOF check | raises `Extra data` | errors | second decode is not `io.EOF` | throws | same, exit 3 |
| Empty input | fails | raises | errors | errors | throws | same, exit 3 |
| Root type names | tag 0–5 | `isinstance` ladder | `Value` match | type switch | `ValueKind` switch | same |
| `sum` numeric handling | `json_as_int`, integer text | `int` only, excludes `bool` | `as_i64` | `ParseInt` on `json.Number` | `TryGetInt64` | same for integral fixtures |
| `sum` missing field | contributes 0 | contributes 0 | contributes 0 | contributes 0 | contributes 0 | same, exit 0 |
| `compact` member order | document order | dict order | `preserve_order` | `json.Compact` preserves | `JsonElement` order | same |
| `compact` UTF-8 | raw bytes | `ensure_ascii=False` | raw | raw | `UnsafeRelaxedJsonEscaping` | same |
| stdout/stderr newline | `\r\n` | explicit `\r\n` | explicit `\r\n` | explicit `\r\n` | explicit `\r\n` | same |
| stdout encoding | UTF-8 bytes | UTF-8 bytes | UTF-8 bytes | UTF-8 bytes | UTF-8 bytes | same, no BOM |
| Exit codes | 0/1/2/3/4 | 0/1/2/3/4 | 0/1/2/3/4 | 0/1/2/3/4 | 0/1/2/3/4 | same |

### Unavoidable or recorded differences

These are documented, not hidden. None is exercised by a W1 case, and none
changes a scored result.

1. **MINK numeric decoding is textual.** `json_as_int` treats every byte as a
   digit, so a decimal field such as `12.5` would be decoded as a corrupt
   integer instead of being skipped. Python/Rust/Go/C# skip non-integral
   numbers. W1 fixtures use integral `qty` values only; a decimal-field case
   is outside the contract.
2. **Control-character escaping in `compact`.** MINK escapes only
   `\n \r \t \b \f` (plus quote and backslash) and passes other control bytes
   through; Python/Rust/C# emit `\u00XX`; Go's `json.Compact` also passes them
   through. No fixture contains such a byte.
3. **Go `compact` is whitespace removal, not re-encoding.** `json.Compact`
   keeps the original escape/number spelling. For the committed fixtures the
   bytes are identical to re-serialization; a non-canonical escape such as
   `\/` would differ.
4. **MINK parses bytes, the others validate UTF-8.** An invalid UTF-8 string
   inside a document would be accepted by MINK and rejected by the other
   four. No fixture contains invalid UTF-8.
5. **C# `UnsafeRelaxedJsonEscaping` does not escape `<`, `>`, `&`, `'`, `+`.**
   MINK, Python, Rust and Go do not escape them either, so this is consistent
   with the contract; it is recorded because a stricter encoder would change
   the bytes.
6. **Number spelling is preserved where the library does not normalize it.**
   No fixture requires normalization, and no implementation was tuned to a
   fixture.

No difference violates the defined functional contract, and no expected
output was relaxed to accommodate a language.

## 4. Dependency audit

Cargo pinned every crate by exact version and by `Cargo.lock` (committed in
`benchmarks/m4/apps/rust/w1_json/`).

| Crate | Version | Direct/transitive | Built | License | Source |
| --- | --- | --- | --- | --- | --- |
| `serde_json` | 1.0.151 | direct, feature `preserve_order` | yes | MIT OR Apache-2.0 | crates.io, local cache |
| `indexmap` | 2.14.2 | transitive (`preserve_order`) | yes | Apache-2.0 OR MIT | crates.io, local cache |
| `equivalent` | 1.0.2 | transitive (indexmap) | yes | Apache-2.0 OR MIT | crates.io, local cache |
| `hashbrown` | 0.17.1 | transitive (indexmap) | yes | MIT OR Apache-2.0 | crates.io, local cache |
| `serde_core` | 1.0.229 | transitive (serde_json) | yes | MIT OR Apache-2.0 | crates.io, local cache |
| `itoa` | 1.0.18 | transitive (serde_json) | yes | MIT OR Apache-2.0 | crates.io, local cache |
| `memchr` | 2.8.3 | transitive (serde_json) | yes | Unlicense OR MIT | crates.io, local cache |
| `zmij` | 1.0.23 | transitive (serde_json float formatting) | yes | MIT | crates.io, local cache |

`cargo metadata` also reports `serde`, `serde_derive`, `proc-macro2`, `quote`,
`syn` and `unicode-ident` in the lockfile. They are **not** in the build
graph (`cargo tree -e normal` does not list them), no proc-macro crate is
compiled, and they do not appear in the artifact. They are recorded here so
the lockfile is not mistaken for the build set.

Non-Rust dependencies:

| Language | Dependency | Runtime requirement |
| --- | --- | --- |
| MINK | bundled `json`, `filesystem`, `strings` (Apache-2.0, repository) | none; native PE, `kernel32.dll` only |
| Python | standard library (PSF-2.0) | CPython 3.11.9 interpreter |
| Go | standard library (BSD-3-Clause) | none; native PE, `kernel32.dll` only |
| C# | in-box `System.Text.Json` (MIT) | .NET 10 runtime + apphost sidecar files |

Restore/build impact: Rust clean build cached-compiles 8 crates in about
3 seconds and needs no network; C# restore needs no network; Go and MINK need
no restore. No dependency was added silently and no network installation was
performed.

## 5. W1 completion criteria

| Criterion (`M4_WORKLOAD_SELECTION.md` §9) | Status |
| --- | --- |
| Five implementations exist and build from a documented clean command | PASS |
| All golden cases pass byte-for-byte for all five | PASS — 23/23 × 5 |
| Error cases produce the specified exit codes and no stack traces | PASS — exit 1/2/3/4 verified |
| Fixtures and oracle are deterministic and committed | PASS — generator + `cases.json` |
| Dependency versions and licenses recorded | PASS — §4 |
| No server/process left running | PASS — W1 starts no server |
| Harness records raw JSONL and a validation report | PASS — `results/raw/w1_json/correctness-w1_json.jsonl` |

**W1 status: COMPLETE for correctness and implementation readiness.**
No performance result exists for W1, and no performance claim is made.

## 6. Files

Created before this session: the W1 tree recorded in
`M4_SESSION_151.md`. Added or changed in Session 152: this record only. No
source, fixture, oracle or harness file changed during the audit.

## 7. Git state

| Item | Value |
| --- | --- |
| HEAD | `691ec38386bb37e204b834e61b0ff8de99a0ac65` (unchanged) |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` (unchanged) |
| Staged | none |
| Modified tracked | 40 pre-existing Session 128–149 files, untouched |
| New/untracked | `benchmarks/`, `docs/benchmarks/` |
| `git diff --check` | clean |
| Commit | none; the benchmark tree is isolated and safe to commit |

## 8. Limitations

- Repeatability was verified with three suite runs on one machine, not with a
  long soak.
- The Rust `RUSTFLAGS=""` override is a harness decision required by the
  repository's static-CRT configuration; it restores the Session 150
  baseline rather than changing it. It must be recorded in every later
  build/distribution statement.
- The lockfile contains non-built packages (see §4); only the `cargo tree`
  graph is claimed as the build set.
- Artifact sizes and import tables are build/distribution facts, not
  runtime evidence.
- W1's boundary probes (4 KiB/5 KiB, decimal fields, control bytes) remain
  unmeasured by design.

## 9. Next step

Session 153 — W2 `netcheck`: inspect the M1 `validation/e_network` workload,
fix the eight-case contract, build a harness-owned deterministic local HTTP
server with deliberate fragmentation, implement the client in all five
languages, and require all eight cases to pass before any timing.
