# M4 — Environment Baseline

**Captured:** 2026-11-01, before any M4 workload implementation.
**Purpose:** pin the machine, toolchains, build configurations and artifact
shapes that M4 measurements will use. The smoke values below are calibration,
**not M4 benchmark results**.

## 1. Machine and OS

| Item | Value |
| --- | --- |
| Host name | `MYSTERIOUSPUNK` |
| OS | Microsoft Windows 11 Pro |
| Version / build | `10.0.26200`, build `26200` |
| Architecture | x86_64 (64-bit) |
| Processor | Intel64 Family 6 Model 183 Stepping 1, ~3200 MHz |
| Logical processors | 32 |
| Memory | 33,230,620 kB reported by `/proc/meminfo` (~31.7 GiB) |
| System disk | `C:` 931 GiB total, 110 GiB available at capture time |
| Shell | Git Bash / MINGW64_NT-10.0-26200 |
| Time zone | UTC+05:30 |
| Power/background | Standard workstation state; no benchmark-specific power plan, Defender exclusion or CPU pinning applied |

The physical core count was not captured because `wmic` is unavailable on this
host. The 32 logical processor count and processor identification come from
`systeminfo`.

## 2. Repository state

| Item | Value |
| --- | --- |
| Repository | `C:\Users\Admin\Desktop\Mink` |
| Branch | `main` |
| HEAD | `691ec38386bb37e204b834e61b0ff8de99a0ac65` |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` |
| Divergence | ahead 7, behind 1 (`git rev-list --left-right --count origin/main...HEAD` → `1  7`) |
| Version | **1.0.4** |
| Worktree before Session 150 | 40 modified tracked files, 26 untracked entries; no staged files; `git diff --check` clean |
| M3 baseline | 3,168 passed / 0 failed / 1 ignored, 77 binaries, 59 existing warnings (Session 149) |

The worktree carries substantial uncommitted Sessions 128–149 work. The MINK
binary used below was built from that tree, not from a clean HEAD. Its
SHA-256 is therefore part of the baseline.

## 3. MINK compiler

| Item | Value |
| --- | --- |
| Path | `target/release/mink.exe` |
| Version output | `mink 1.0.4` |
| Size | 4,028,416 bytes |
| SHA-256 | `d8f6134850e2af1a7cd4f38a7e8a80d7692d566eac1bae97ddc28ed827149f09` |
| File timestamp | 2026-11-01 00:31 |
| CLI | `build`, `run`, `check`, `test`, `repl`, `explain`, `version`, package/project commands |
| Build modes | One `mink build <path>` command; no documented debug/release switch |

The generated hello executable is a PE32+ x86-64 console image. It reported
`kernel32.dll` as its only import; it needs no MINK compiler on the target
machine.

## 4. Toolchains

| Language | Version / configuration | Path / note |
| --- | --- | --- |
| MINK | 1.0.4, release compiler above | `target/release/mink.exe` |
| Python | CPython **3.11.9**, MSC v.1938 64-bit | `C:\Users\Admin\AppData\Local\Programs\Python\Python311\python.exe` |
| Python (alternate) | CPython **3.14.3** via launcher | `py -3.14`; not used for M4, to preserve M1 continuity |
| Rust | **1.97.1** (`8bab26f4f`, 2026-07-14), LLVM 22.1.6 | `x86_64-pc-windows-msvc`; `rustc`/`cargo` in `C:\Users\Admin\.cargo\bin` |
| Go | **go1.26.7** `windows/amd64` | `C:\Program Files\Go\bin\go.exe` |
| .NET SDK | **10.0.302** | `C:\Program Files\dotnet\dotnet.exe`; target `net10.0` |
| .NET runtimes | 10.0.14, 10.0.10, 8.0.33, 6.0.36 | Framework-dependent apps need a compatible installed runtime |
| C# compiler | Roslyn through `dotnet build` | No standalone `csc` on `PATH`; use the SDK |
| SQLite CLI | **absent** | Libraries provide SQLite; no `sqlite3.exe` for ad-hoc inspection |
| `objdump` / `dumpbin` | **absent** | PE imports inspected with a read-only Python parser |
| Node.js | 24.14.0 | Only relevant to MINK's npm distribution, not a workload toolchain |

`python3` on this host is the Microsoft Store alias stub and fails. The M4
harness must invoke **`python`** explicitly, as the M1/M2 records already
require. This is an environment property, not a MINK failure.

## 5. Build configurations for M4

| Language | Primary command | Notes |
| --- | --- | --- |
| MINK | `target/release/mink.exe build <app>.mink` | Single native PE; no user-selectable optimization level documented |
| Python | `python <app>.py` | CPython 3.11.9; no virtualenv or third-party package for the selected stdlib workloads |
| Rust | `cargo build --release` | MSVC target; exact profile and crate versions to be pinned with `Cargo.lock` |
| Go | `go build -trimpath -buildvcs=false -o <app>.exe` | Default Go optimizations; no `-ldflags="-s -w"` in the primary build |
| C# | `dotnet publish -c Release -r win-x64 --self-contained false` | Framework-dependent primary. Self-contained/single-file is a separate distribution probe, not the primary runtime number |

Dependency restore is measured separately. Shared compiler/dependency caches
are retained after the first restore; application output directories are
removed for each "clean" build. M4 does not wipe global caches.

## 6. Smoke verification and calibration

Each toolchain was used to build and run a minimal program. These are
**smoke checks**, not the M4 measurement protocol: 10 launches, no formal
warm-up policy, parent-measured wall time.

| Language | Build result | Run result | Artifact | Startup median (min–max), 10 runs |
| --- | --- | --- | --- | --- |
| MINK | exit 0, 1 function | exit 0, `hello-m4` | 47,104 B native PE, `kernel32.dll` only | **2.84 ms** (2.69–4.70) |
| Python 3.11 | source only | exit 0, `hello-m4` | no native artifact | **17.83 ms** (17.02–20.84) |
| Rust | `rustc -O` exit 0 | exit 0, `hello-m4` | 125,952 B native PE; imports `VCRUNTIME140.dll` + UCRT API sets | **3.75 ms** (3.47–50.67; one outlier) |
| Go | exit 0 | exit 0, `hello-m4` | 2,457,088 B native PE, `kernel32.dll` only | **4.84 ms** (4.56–7.13) |
| C# / .NET 10 | `dotnet build -c Release` exit 0 | exit 0 via `dotnet <dll>` | apphost 162,304 B + assembly 4,608 B + metadata 772 B (PDB excluded) | **34.98 ms** (33.89–36.32) |

C# startup was measured through `dotnet <dll>` (framework-dependent), not the
apphost directly; the apphost number is expected to differ and will be
measured in the real protocol. The C# smoke program was the default console
template (`Hello, World!`), not a custom hello; it verifies the toolchain only.

The observed line endings also confirm the M4 convention: MINK and Python
produce CRLF on this console, while Rust and Go produce LF; the workload
contract requires all implementations to emit canonical CRLF explicitly.

## 7. Artifact dependencies (smoke executable)

| Artifact | PE imports / runtime requirement |
| --- | --- |
| MINK hello | `kernel32.dll` only; no CRT/runtime DLL |
| Rust hello | `VCRUNTIME140.dll`, `KERNEL32.dll`, `ntdll.dll`, UCRT API-set DLLs |
| Go hello | `kernel32.dll` only |
| C# apphost | `SHELL32.dll`, `ADVAPI32.dll`, `KERNEL32.dll`, `USER32.dll`, UCRT API-set DLLs; also requires the .NET 10 runtime and the app's `.dll`/`.deps.json`/`.runtimeconfig.json` |

The import lists were read from each PE import directory by a small read-only
Python parser because `objdump`/`dumpbin` are not installed. This is artifact
evidence for the smoke builds, not a claim about every MINK program: a program
using networking or cryptography will resolve `ws2_32`/`bcrypt` and therefore
import those DLLs.

## 8. Missing or unverified dependencies

These are **not** toolchain blockers for methodology, but they must be
resolved or explicitly recorded before workload timing:

- Rust crates for W1/W2/W3/W5 (`serde_json`, `ureq`, `rusqlite`, `sha2`,
  `csv`) are not installed or version-pinned in this session.
- Go's SQLite driver (`modernc.org/sqlite`) is not downloaded or pinned in
  this session.
- C#'s `Microsoft.Data.Sqlite` package is not downloaded or pinned in this
  session.
- No external package download was performed. The only restore was the .NET
  console template's local assets.
- `sqlite3.exe`, `csc.exe` and `objdump`/`dumpbin` are absent from `PATH`;
  the documented workarounds are library APIs, `dotnet build`, and the Python
  PE parser.
- The MINK compiler is from a worktree with uncommitted M3 work. The binary
  hash above is the reproducibility pin; it is not a clean-commit build.

## 9. Measurement conditions and limitations

- Local loopback only; no public internet, cloud service or third-party host.
- No controlled cache dropping, CPU pinning, power-plan change or antivirus
  exclusion. M4 reports warm-process measurements, not cold-cache claims.
- Single machine, single developer/agent, single point in time.
- Background OS activity and code-page/console state can add noise; the
  protocol uses repeats, shuffled order and variability reporting.
- The toolchain versions above are fixed for M4. An upgrade requires a new
  baseline record and re-measurement; results from different versions are not
  combined.

## 10. Reproduction commands

```bash
# Repository and compiler
git rev-parse HEAD
git rev-parse origin/main
git status --porcelain
target/release/mink.exe version
sha256sum target/release/mink.exe

# Toolchains
python --version
py -3.11 --version
py -3.14 --version
rustc -vV
cargo --version
go version
dotnet --version
dotnet --list-sdks
dotnet --list-runtimes

# Machine
uname -a
systeminfo
grep -E "MemTotal|MemAvailable" /proc/meminfo
df -h .
```

Smoke builds are reproducible from the commands in §5; the temporary
`tmp/m4_env/` tree is ignored by Git and was not part of the repository state.
