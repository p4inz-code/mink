# Session 212 — Release Toolchain Root-Cause Investigation

Windows x86_64, 2026-11-08. Starting commit
`74beba491429a71807511bc0d49f35a65dfc84fa` (Session 211, local == `origin/main`
verified before any change).

## 1. Objective

Establish why optimized MINK compilation crashed on this machine, using
reproduction and evidence rather than another reinstall. No MINK source was to
be changed to conceal a compiler crash, and no MSRV or compiler requirement was
to be lowered.

## 2. Recorded environment

| Item | Value |
| --- | --- |
| OS build | Windows 10.0.26200.9818 (x86_64) |
| Host | ASUS desktop, 32,452 MB RAM, 32 logical processors |
| CPU | Intel64 Family 6 Model 183 Stepping 1 (Raptor Lake-S class) |
| BIOS | American Megatrends 1836, 2026-04-16 |
| rustc | 1.97.1 (8bab26f4f 2026-07-14), LLVM 22.1.6 |
| rustc binary SHA-256 | `cf79cfd77b0a144c56a0a6af6bf10bcdf095a73718cd4bf2b9d4fe2d2cbded55` |
| cargo / rustup | 1.97.1 (c980f4866 2026-06-30) / 1.29.3 |
| Host triple | `x86_64-pc-windows-msvc` |
| Installed targets | `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`, `x86_64-unknown-linux-musl` |
| Components | cargo, clippy, rust-docs, rust-std (3 targets), rustc, rustfmt |
| Linker / SDK | MSVC (Visual Studio 18 tree), Windows SDK `10.0.26100.0` |
| Cargo config | `.cargo/config.toml`: `crt-static` for `x86_64-pc-windows-msvc` |
| Relevant env vars | only `PATH`; no `RUSTFLAGS`, `CARGO_*`, `CC`, `LINK`, `LIB` or `INCLUDE` set |
| Host uptime at capture | 2,466 s (~41 min) |

`rustc.exe` is 110,592 bytes and its hash is recorded above. The component list
is complete for a stable MSVC toolchain, and `rustc -vV` is self-consistent, so
there is no sign of a partial or corrupted installation.

## 3. The recorded failure being investigated

Sessions 205–210 recorded every release `rustc` invocation failing with three
different NTSTATUS codes, reproduced after `rustup toolchain install 1.97.1
--force`, with `-j 1`, `CARGO_INCREMENTAL=0`, `codegen-units=1`,
`target-cpu=x86-64` and `opt-level 2`:

```
rustc ... -C opt-level=3 ... exit 0xc000001d (STATUS_ILLEGAL_INSTRUCTION)
rustc ... -C opt-level=3 ... exit 0xc0000374 (STATUS_HEAP_CORRUPTION)
rustc ... -C opt-level=3 ... exit 0xc0000005 (STATUS_ACCESS_VIOLATION)
```

Debug builds passed, a trivial release crate compiled, and the last successful
release artifact was dated **2026-11-07 11:17** — which is consistent with the
Session 205 record and predates the first reported crash.

## 4. Reproduction attempts

All commands were run from the repository root. Every attempt below was
recorded after the current host boot.

| # | Command | Result | Exit |
| ---: | --- | --- | ---: |
| 1 | Trivial release crate (`tmp/s212/trivial`, `Vec<u64>` workload) | compiled and printed `332833500` | 0 |
| 2 | `cargo build --release` (default incremental, default jobs) | `Finished release profile in 31.42s`, binary `mink 1.0.5` | **0** |
| 3 | `CARGO_TARGET_DIR=target/s213_clean cargo build --release` (isolated, no cached artifacts) | `Finished release profile in 30.78s`, 4,048,384 bytes | **0** |
| 4 | `CARGO_INCREMENTAL=0 cargo build --release -j 1` | compiled | 0 |
| 5 | `CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 cargo build --release -j 1` | compiled | 0 |
| 6 | `CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_OPT_LEVEL=2 cargo build --release -j 1` | compiled | 0 |
| 7 | `CARGO_TARGET_DIR=target/s213_tests cargo test --release --no-run` (all 84 test targets — the heaviest `rustc` workload available) | 87 executables produced | **0** |
| 8 | `CARGO_TARGET_DIR=target/s213_tests cargo test --release --no-fail-fast` (full release regression) | 3,262 passed / 0 failed / 9 ignored | **0** |

No attempt produced `STATUS_ILLEGAL_INSTRUCTION`, `STATUS_HEAP_CORRUPTION`,
`STATUS_ACCESS_VIOLATION`, a signal, or a "could not compile" error. Attempt 3
rules out stale artifacts: it built both the library and the binary into a
directory that did not exist before the run. Attempt 2 rebuilt the binary
(4,045,824 -> 4,048,384 bytes) rather than reusing the 2026-11-07 artifact.

## 5. Diagnosis

**The reported failure cannot be reproduced on this host. The release toolchain
is functional.** Evidence excludes each candidate cause as follows:

| Candidate | Verdict | Evidence |
| --- | --- | --- |
| Toolchain corruption | **Excluded** | Component set complete; `rustc -vV` consistent; recorded binary hash stable; Session 210 already force-reinstalled without effect; the same binaries now compile the same source. |
| Linker or SDK incompatibility | **Excluded** | The recorded crashes were in `rustc.exe` itself, before `link.exe` ran; both are installed and unchanged; release links succeed now. |
| MINK source triggering a compiler bug | **Excluded as current state** | The identical source compiles in two independent target directories and under every flag combination that previously crashed; a source-triggered compiler bug would be deterministic. |
| Dependency interaction | **Not applicable** | The crate has zero dependencies (`Cargo.toml` `[dependencies]` is empty). |
| Compiler bug in 1.97.1 | **Not supported** | The same 1.97.1 binaries now compile the same code; a codegen bug would reproduce. |
| Host-level transient instability | **Consistent with all evidence** | Three different NTSTATUS memory-corruption codes from the process itself, varying between runs, on a 32-thread host, clearing after a reboot with no toolchain change. |

The only environmental change between the failing observations and this session
is a **host reboot** (uptime 2,466 s at capture; the previous successful
release artifact is from 2026-11-07 11:17). That is consistent with a transient
host-level fault, but it is not proof of a specific hardware cause. The Windows
hardware-error log could not be queried with the available tools
(`wevtutil` rejected the query syntax here), so no WHEA event evidence is
claimed either way. This is recorded as a **characterized, non-reproducing
environment fault**, not as a fixed defect and not as a pass for the sessions
that were blocked.

## 6. Incidental finding fixed

Two integration suites located the compiler by hardcoded path
(`Command::new("target/debug/mink.exe")`): `tests/encoding_lib.rs:18` and
`tests/math_lib.rs:20`. Every other suite uses `env!("CARGO_BIN_EXE_mink")`.
Under a release-only verification run those two suites would silently exercise
the debug compiler. Both now use `CARGO_BIN_EXE_mink`, matching the other 65
suites. Verified in both profiles: `encoding_lib` 75/75 and `math_lib` 106/106,
debug and release.

## 7. Recovery action

No toolchain reinstall, downgrade, MSRV change, environment-variable change or
source workaround was applied. The smallest safe correction was to stop
reinstalling and verify the environment: the host recovered on its own across a
reboot, and the evidence above establishes a working baseline for Session 213.

## 8. Gate result

| Gate item | Result |
| --- | --- |
| Defensible diagnosis | PASS — non-reproducing, host-level transient fault; failure modes excluded |
| Problem resolved | **Yes, by environment recovery**, not by a code or toolchain change |
| Evidence preserved | PASS — commands, versions, exit codes and logs recorded |
| MINK source altered to hide a crash | None |
| Release readiness declared | No — readiness is Session 213's decision |

**Next:** Session 213 full release-mode verification against this baseline.
