# Session 158 — Closing the W3 dependencies

**Date:** 2026-11-02
**Status:** **COMPLETE — W3 is 65/65 GREEN in all five languages.**
**Rule:** no performance measurement was taken. This session only resolves
dependencies, builds, and runs the correctness gate.

## 1. Authorization and starting state

Network authorization was available: `index.crates.io`, `static.crates.io` and
`proxy.golang.org` all answered HTTP 200. Only the two already-locked SQLite
dependencies were fetched; no other package was installed, no engine was
substituted and no MINK file was touched.

Starting state: HEAD `e6143bf`, W3 3/5 (MINK, Python, C# green; Rust and Go
blocked), 424/450 language-case combinations green.

## 2. Rust

The locked plan was executed unchanged: `rusqlite 0.32` with
`features = ["bundled"]`.

```text
cargo fetch                     # benchmarks/m4/apps/rust/w3_sqlite
cargo build --release --offline # via benchmarks/m4/harness/build.py
```

Resolved dependency graph (`Cargo.lock`, 24 packages including `dbshell`):

| Crate | Version | Role | License |
| --- | --- | --- | --- |
| `rusqlite` | **0.32.1** | SQLite API | MIT |
| `libsqlite3-sys` | **0.30.1** | FFI + bundled SQLite | MIT |
| bundled SQLite | **3.46.0** | engine (`sqlite3.h`) | Public domain |
| `cc` | 1.11.0 | compiles the bundled C engine | MIT/Apache-2.0 |
| `find-msvc-tools` | 0.1.15 | MSVC discovery | MIT/Apache-2.0 |
| `shlex` | 2.0.1 | `cc` dependency | MIT/Apache-2.0 |
| `pkg-config` | 0.3.34 | `libsqlite3-sys` build dep | MIT/Apache-2.0 |
| `vcpkg` | 0.2.15 | `libsqlite3-sys` build dep | MIT/Apache-2.0 |
| `hashlink` | 0.9.1 | `rusqlite` cache | MIT/Apache-2.0 |
| `hashbrown` | 0.14.5 | `hashlink` | MIT/Apache-2.0 |
| `ahash` | 0.8.12 | `hashbrown` | MIT/Apache-2.0 |
| `smallvec` | 1.16.4 | `rusqlite` | MIT/Apache-2.0 |
| `bitflags` | 2.13.2 | `rusqlite` | MIT/Apache-2.0 |
| `fallible-iterator` | 0.3.0 | `rusqlite` | MIT/Apache-2.0 |
| `fallible-streaming-iterator` | 0.1.9 | `rusqlite` | MIT/Apache-2.0 |
| `cfg-if`, `once_cell`, `zerocopy`, `version_check` | 1.0.5 / 1.21.4 / 0.8.63 / 0.9.5 | `ahash` chain | MIT/Apache-2.0 |

`Cargo.toml` now pins `rusqlite = { version = "=0.32.1", ... }`; the full
graph is committed in `Cargo.lock`.

### MSVC C compiler

`cl.exe` is not on `PATH`, but the MSVC toolchain is installed and the `cc`
crate found it through Visual Studio:

| Item | Value |
| --- | --- |
| Visual Studio | 18 Community |
| MSVC toolset | 14.51.36231 |
| `cl.exe` version | 19.51.36248 for x64 |
| Result | bundled SQLite compiled successfully; no compiler workaround needed |

Build: exit 0, release artifact
`benchmarks/m4/.build/w3_sqlite/rust/release/dbshell.exe`, 1,817,600 bytes.

## 3. Go

The locked driver was used unchanged: `modernc.org/sqlite` (pure Go, no cgo).

```text
go get modernc.org/sqlite
go mod tidy
go build -trimpath -buildvcs=false -o .build/w3_sqlite/go/dbshell.exe .
```

| Item | Resolved |
| --- | --- |
| Direct module | **modernc.org/sqlite v1.61.1** |
| `go` directive | 1.26.0 (toolchain used: go1.26.7 windows/amd64) |
| `go.mod` requirements | 1 direct + 9 indirect |
| `go.sum` | 50 checksum lines (25 modules × module + go.mod) |
| License | BSD-3-Clause (`modernc.org/sqlite`, SQLite authors) |

Complete module set reported by `go list -m all` (25 modules plus the main
module):

```text
github.com/dustin/go-humanize v1.1.0
github.com/google/pprof v0.0.0-20261016150608-1facf8f91b50
github.com/google/uuid v1.6.0
github.com/hashicorp/golang-lru/v2 v2.0.7
github.com/mattn/go-isatty v0.0.24
github.com/ncruces/go-strftime v1.1.0
github.com/remyoudompheng/bigfft v0.0.0-20230129092748-24d4a6f8daec
golang.org/x/mod v0.42.0
golang.org/x/sync v0.24.0
golang.org/x/sys v0.49.0
golang.org/x/tools v0.52.0
modernc.org/cc/v4 v4.30.3
modernc.org/ccgo/v4 v4.37.0
modernc.org/fileutil v1.4.0
modernc.org/gc/v2 v2.6.5
modernc.org/gc/v3 v3.2.0
modernc.org/goabi0 v0.2.0
modernc.org/libc v1.79.0
modernc.org/mathutil v1.7.1
modernc.org/memory v1.12.1
modernc.org/opt v0.2.0
modernc.org/sortutil v1.2.1
modernc.org/sqlite v1.61.1
modernc.org/strutil v1.2.1
modernc.org/token v1.1.0
```

``go build`` was run through the harness with `GOFLAGS=""`. Build: exit 0,
artifact `benchmarks/m4/.build/w3_sqlite/go/dbshell.exe`, 10,019,840 bytes.

The module's `go.mod` comment and both applications' source headers no longer
say "BLOCKED"; they now record the verified build state. No logic changed.

## 4. Correctness

`python benchmarks/m4/harness/validate.py --workload w3_sqlite`

| Language | Passed | Scored | State |
| --- | --- | --- | --- |
| MINK | 13/13 | 10/10 | GREEN (unchanged) |
| Python | 13/13 | 10/10 | GREEN (unchanged) |
| Rust | **13/13** | 10/10 | **GREEN (was blocked)** |
| Go | **13/13** | 10/10 | **GREEN (was blocked)** |
| C# | 13/13 | 10/10 | GREEN (re-verified, offline restore) |

**65/65 language-case combinations pass**, including both new
implementations and the 3 supporting cases (`scale_200`, `probe_500`,
`option_looking_arg`). The full run was executed twice; the two
`correctness-w3_sqlite.jsonl` files were byte-identical, including stdout and
stderr SHA-256 values and exit codes.

The 500-row probe passes byte-for-byte in all five implementations. No golden
file was modified, no fixture changed, and no expected output was weakened.

## 5. Files

```text
benchmarks/m4/apps/rust/w3_sqlite/Cargo.toml        (pin + status)
benchmarks/m4/apps/rust/w3_sqlite/Cargo.lock        (new)
benchmarks/m4/apps/rust/w3_sqlite/src/main.rs       (status comment only)
benchmarks/m4/apps/go/w3_sqlite/go.mod              (resolved requirements)
benchmarks/m4/apps/go/w3_sqlite/go.sum              (new)
benchmarks/m4/apps/go/w3_sqlite/main.go             (status comment only)
```

No MINK compiler, runtime, stdlib or test file was modified.

## 6. Limitations

- The Rust artifact statically compiles SQLite 3.46.0; the Go artifact embeds
  the pure-Go modernc engine. Both remain in-memory only and cover the W3
  common-SQL contract, not SQLite file compatibility.
- `modernc.org/sqlite` is a large dependency chain (25 modules) and produces a
  10.0 MB executable versus Rust's 1.8 MB. This is dependency/distribution
  evidence, not a performance result.
- C# still pins `Microsoft.Data.Sqlite 8.0.0` from the local NuGet cache; its
  `SQLitePCLRaw.lib.e_sqlite3 2.1.6` NU1903 advisory remains visible.
- No timing, startup or memory measurement was taken in this session.

## 7. Git

HEAD at session start: `e6143bf`. Only the six W3 benchmark files above and
this record are part of the dependency-closure commit. The pre-existing
modified M3/domain files were not staged. No reset, clean, stash, rebase,
amend, force, push, tag or publish was run.
